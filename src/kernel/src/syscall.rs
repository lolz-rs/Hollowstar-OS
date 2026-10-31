use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use hollowstar_common::color;
use hollowstar_common::*;

use crate::{archive, audio, console, cpu, graphics, io, keyboard, memory, mouse, pit, shell};

static COMMAND_EXIT_CODE: AtomicU64 = AtomicU64::new(0);
/// Set when Ctrl+C should stop the running command.
static CANCEL: AtomicBool = AtomicBool::new(false);
/// Set when it is safe to abandon the command immediately (not inside a syscall).
static IMMEDIATE: AtomicBool = AtomicBool::new(false);
static ABORTING: AtomicBool = AtomicBool::new(false);
/// Non-zero while `int 0x80` is inside the kernel. Ctrl+C must not cut that short.
static SYSCALL_DEPTH: AtomicU32 = AtomicU32::new(0);

extern "C" {
    fn command_abort() -> !;
    static mut command_armed: u8;
}

fn armed() -> bool {
    unsafe { core::ptr::read_volatile(core::ptr::addr_of!(command_armed)) != 0 }
}

fn disarm() {
    unsafe {
        core::ptr::write_volatile(core::ptr::addr_of_mut!(command_armed), 0);
    }
}

/// A running command pressed Ctrl+C. Returns true when the key should not be
/// delivered as ordinary input.
pub fn note_ctrl_c() -> bool {
    if !armed() || ABORTING.load(Ordering::SeqCst) {
        return false;
    }
    CANCEL.store(true, Ordering::SeqCst);
    if SYSCALL_DEPTH.load(Ordering::SeqCst) == 0 {
        IMMEDIATE.store(true, Ordering::SeqCst);
    }
    true
}

/// After the IRQ has sent EOI, abandon the command if it is not inside a syscall.
pub fn take_immediate_abort() -> bool {
    IMMEDIATE.swap(false, Ordering::SeqCst) && armed() && !ABORTING.load(Ordering::SeqCst)
}

pub fn enter_syscall() {
    SYSCALL_DEPTH.fetch_add(1, Ordering::SeqCst);
}

pub fn leave_syscall() {
    SYSCALL_DEPTH.fetch_sub(1, Ordering::SeqCst);
}

pub fn cancel_requested() -> bool {
    CANCEL.load(Ordering::SeqCst) && armed() && !ABORTING.load(Ordering::SeqCst)
}

/// Stop the running command and return to the shell. Safe only when no syscall
/// is half-finished, or when the caller is that syscall and has finished its work.
pub fn abort_command() -> ! {
    if ABORTING.swap(true, Ordering::SeqCst) {
        loop {
            unsafe {
                io::hlt();
            }
        }
    }
    CANCEL.store(false, Ordering::SeqCst);
    IMMEDIATE.store(false, Ordering::SeqCst);
    SYSCALL_DEPTH.store(0, Ordering::SeqCst);
    disarm();
    COMMAND_EXIT_CODE.store(130, Ordering::Relaxed);
    audio::off();
    console::finish_command();
    console::set_color(color::WHITE);
    console::write_str("^C\n");
    console::flush();
    ABORTING.store(false, Ordering::SeqCst);
    unsafe { command_abort() }
}

pub fn command_finished() {
    CANCEL.store(false, Ordering::SeqCst);
    IMMEDIATE.store(false, Ordering::SeqCst);
    ABORTING.store(false, Ordering::SeqCst);
    SYSCALL_DEPTH.store(0, Ordering::SeqCst);
    disarm();
}

pub fn dispatch(num: u64, a: u64, b: u64, c: u64, d: u64, e: u64) -> u64 {
    if cancel_requested() {
        abort_command();
    }
    let result = match num {
        SYS_WRITE => sys_write(a, b, c),
        SYS_READ_LINE => sys_read_line(a, b),
        SYS_EXIT => {
            COMMAND_EXIT_CODE.store(a, Ordering::Relaxed);
            audio::off();
            console::finish_command();
            console::flush();
            unsafe { command_abort() }
        }
        SYS_CLEAR => {
            console::clear();
            0
        }
        SYS_INFO => sys_info(a),
        SYS_UPTIME_MS => pit::uptime_ms(),
        SYS_SLEEP => {
            pit::sleep_ms(a);
            0
        }
        SYS_REBOOT => {
            console::write_str("Rebooting...\n");
            audio::off();
            console::flush();
            reboot_now()
        }
        SYS_HALT => {
            console::write_str("System halted.\n");
            audio::off();
            console::flush();
            halt_now()
        }
        SYS_GETCHAR => keyboard::read_char() as u64,
        SYS_PUTCHAR => {
            console::write_byte(a as u8);
            0
        }
        SYS_WRITE_COLOR => sys_write_color(a as u32, b, c),
        SYS_CMD_COUNT => archive::count() as u64,
        SYS_CMD_GET => sys_cmd_get(a, b, c, d, e),
        SYS_SET_COLOR => {
            let old = console::get_color();
            console::set_color(a as u32);
            old as u64
        }
        SYS_GET_COLOR => console::get_color() as u64,
        SYS_GFX_INFO => sys_gfx_info(a),
        SYS_GFX_ENTER => console::enter_graphics_command() as u64,
        SYS_GFX_LEAVE => {
            console::leave_graphics_command();
            0
        }
        SYS_GFX_PRESENT => {
            graphics::present();
            0
        }
        SYS_GFX_PRESENT_RECT => {
            graphics::present_rect(a as u32, b as u32, c as u32, d as u32);
            0
        }
        SYS_GFX_VSYNC => {
            graphics::wait_vsync();
            0
        }
        SYS_AUDIO_TONE => {
            audio::tone(a as u32);
            0
        }
        SYS_AUDIO_OFF => {
            audio::off();
            0
        }
        SYS_AUDIO_BEEP => {
            audio::beep(a as u32, b);
            0
        }
        SYS_KEY_POLL => keyboard::try_read()
            .or_else(|| crate::serial::poll())
            .unwrap_or(0) as u64,
        SYS_MOUSE_STATE => sys_mouse_state(a),
        _ => u64::MAX,
    };
    if cancel_requested() {
        abort_command();
    }
    result
}

fn sys_mouse_state(ptr: u64) -> u64 {
    if ptr == 0 {
        return 0;
    }
    let state = mouse::state();
    unsafe {
        ptr::write(ptr as *mut MouseState, state);
    }
    if (state.flags & MOUSE_FLAG_AVAILABLE) != 0 {
        1
    } else {
        0
    }
}

fn sys_gfx_info(ptr: u64) -> u64 {
    if ptr == 0 {
        return 0;
    }
    let info = graphics::info();
    unsafe {
        ptr::write(ptr as *mut GraphicsInfo, info);
    }
    if graphics::active() {
        1
    } else {
        0
    }
}

fn sys_write(fd: u64, ptr: u64, len: u64) -> u64 {
    if ptr == 0 || len == 0 {
        return 0;
    }
    let len = len.min(1024 * 1024) as usize;
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    if fd == 2 {
        let old = console::get_color();
        console::set_color(color::RED);
        console::write_bytes(bytes);
        console::set_color(old);
    } else {
        console::write_bytes(bytes);
    }
    len as u64
}

fn sys_write_color(value: u32, ptr: u64, len: u64) -> u64 {
    if ptr == 0 || len == 0 {
        return 0;
    }
    let len = len.min(1024 * 1024) as usize;
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    let old = console::get_color();
    console::set_color(value);
    console::write_bytes(bytes);
    console::set_color(old);
    len as u64
}

fn sys_read_line(ptr: u64, max: u64) -> u64 {
    if ptr == 0 || max == 0 {
        return 0;
    }
    let max = max.min(4096) as usize;
    let buffer = unsafe { core::slice::from_raw_parts_mut(ptr as *mut u8, max) };
    console::flush();
    shell::read_line_into(buffer) as u64
}

fn sys_info(ptr: u64) -> u64 {
    if ptr == 0 {
        return 0;
    }
    let mut info = SystemInfo::zeroed();
    info.uptime_ms = pit::uptime_ms();

    let mem = memory::summary();
    info.mem_total_kb = mem.total_kb;
    info.mem_usable_kb = mem.usable_kb;
    info.mem_reserved_kb = mem.reserved_kb;
    info.mem_mapped_kb = mem.mapped_kb;
    info.mem_top_kb = mem.top_kb;

    info.command_count = archive::count() as u32;
    info.boot_drive = memory::boot_info().boot_drive;
    info.kernel_size = memory::boot_info().kernel_size as u64;
    info.version_major = VERSION_MAJOR;
    info.version_minor = VERSION_MINOR;
    info.version_patch = VERSION_PATCH;

    cpu::fill_info(&mut info);
    unsafe {
        ptr::write(ptr as *mut SystemInfo, info);
    }
    1
}

fn sys_cmd_get(index: u64, name_ptr: u64, name_max: u64, desc_ptr: u64, desc_max: u64) -> u64 {
    let Some(entry) = archive::entry(index as usize) else {
        return 0;
    };
    let name = archive::name_bytes(entry);
    let desc = archive::description_str(entry).as_bytes();

    if !copy_cstr(name, name_ptr as *mut u8, name_max as usize) {
        return 0;
    }
    if !copy_cstr(desc, desc_ptr as *mut u8, desc_max as usize) {
        return 0;
    }
    1
}

fn copy_cstr(src: &[u8], dst: *mut u8, max: usize) -> bool {
    if dst.is_null() || max == 0 {
        return false;
    }
    let count = src.len().min(max - 1);
    unsafe {
        ptr::copy_nonoverlapping(src.as_ptr(), dst, count);
        *dst.add(count) = 0;
    }
    true
}

pub fn halt_now() -> ! {
    loop {
        unsafe {
            io::disable_interrupts();
            io::hlt();
        }
    }
}

pub fn reboot_now() -> ! {
    unsafe {
        io::outb(0xCF9, 0x02);
        io::io_wait();
        io::outb(0xCF9, 0x06);
        io::io_wait();
        io::outb(0x64, 0xFE);
    }
    halt_now()
}
