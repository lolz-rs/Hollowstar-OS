#![no_std]

use core::arch::asm;
use core::ffi::c_void;
use core::fmt;

pub use hollowstar_common::*;

pub mod gfx;

#[inline(always)]
pub unsafe fn syscall5(num: u64, a: u64, b: u64, c: u64, d: u64, e: u64) -> u64 {
    let ret: u64;
    asm!(
        "int 0x80",
        inlateout("rax") num => ret,
        in("rdi") a,
        in("rsi") b,
        in("rdx") c,
        in("r10") d,
        in("r8") e,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack)
    );
    ret
}

pub fn write(text: &str) {
    write_bytes(text.as_bytes());
}

pub fn write_bytes(bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    unsafe {
        syscall5(SYS_WRITE, 1, bytes.as_ptr() as u64, bytes.len() as u64, 0, 0);
    }
}

/// Write to stderr. The kernel renders it in red and does not redirect it to
/// a file, which is what error messages want.
pub fn write_err(text: &str) {
    write_err_bytes(text.as_bytes());
}

pub fn write_err_bytes(bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    unsafe {
        syscall5(SYS_WRITE, 2, bytes.as_ptr() as u64, bytes.len() as u64, 0, 0);
    }
}

/// Write text in a packed `0x00RRGGBB` true colour, then restore the previous
/// colour.
pub fn write_color(value: u32, text: &str) {
    if text.is_empty() {
        return;
    }
    unsafe {
        syscall5(
            SYS_WRITE_COLOR,
            value as u64,
            text.as_ptr() as u64,
            text.len() as u64,
            0,
            0,
        );
    }
}

/// Set the foreground colour (packed `0x00RRGGBB`). Returns the previous one.
pub fn set_color(value: u32) -> u32 {
    unsafe { syscall5(SYS_SET_COLOR, value as u64, 0, 0, 0, 0) as u32 }
}

pub fn get_color() -> u32 {
    unsafe { syscall5(SYS_GET_COLOR, 0, 0, 0, 0, 0) as u32 }
}

/// Start a continuous PC-speaker tone. Frequency 0 is equivalent to `audio_off`.
pub fn tone(freq: u32) {
    unsafe {
        syscall5(SYS_AUDIO_TONE, freq as u64, 0, 0, 0, 0);
    }
}

pub fn audio_off() {
    unsafe {
        syscall5(SYS_AUDIO_OFF, 0, 0, 0, 0, 0);
    }
}

pub fn beep(freq: u32, ms: u64) {
    unsafe {
        syscall5(SYS_AUDIO_BEEP, freq as u64, ms, 0, 0, 0);
    }
}

/// Open a RAM-disk file. Use `FS_READ`, `FS_WRITE`, and/or `FS_APPEND`.
pub fn fs_open(name: &str, flags: u64) -> Option<u64> {
    if name.is_empty() {
        return None;
    }
    let fd = unsafe {
        syscall5(
            SYS_FS_OPEN,
            name.as_ptr() as u64,
            name.len() as u64,
            flags,
            0,
            0,
        )
    };
    if fd == 0 {
        None
    } else {
        Some(fd)
    }
}

pub fn fs_close(fd: u64) {
    unsafe {
        syscall5(SYS_FS_CLOSE, fd, 0, 0, 0, 0);
    }
}

pub fn fs_read(fd: u64, buf: &mut [u8]) -> Option<usize> {
    if buf.is_empty() {
        return Some(0);
    }
    let n = unsafe {
        syscall5(
            SYS_FS_READ,
            fd,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
            0,
            0,
        )
    };
    if n == u64::MAX {
        None
    } else {
        Some(n as usize)
    }
}

pub fn fs_write(fd: u64, bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() {
        return Some(0);
    }
    let n = unsafe {
        syscall5(
            SYS_FS_WRITE,
            fd,
            bytes.as_ptr() as u64,
            bytes.len() as u64,
            0,
            0,
        )
    };
    if n == u64::MAX {
        None
    } else {
        Some(n as usize)
    }
}

pub fn fs_count() -> usize {
    unsafe { syscall5(SYS_FS_COUNT, 0, 0, 0, 0, 0) as usize }
}

pub fn fs_list(index: usize, name: &mut [u8]) -> Option<u32> {
    if name.is_empty() {
        return None;
    }
    let mut size = 0u32;
    let ok = unsafe {
        syscall5(
            SYS_FS_LIST,
            index as u64,
            name.as_mut_ptr() as u64,
            name.len() as u64,
            &mut size as *mut u32 as u64,
            0,
        )
    };
    if ok == 0 {
        None
    } else {
        Some(size)
    }
}

pub fn fs_remove(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    unsafe { syscall5(SYS_FS_REMOVE, name.as_ptr() as u64, name.len() as u64, 0, 0, 0) != 0 }
}

/// Read until EOF or until `buf` is full. Returns the number of bytes stored.
pub fn fs_read_all(fd: u64, buf: &mut [u8]) -> Option<usize> {
    let mut off = 0usize;
    while off < buf.len() {
        let n = fs_read(fd, &mut buf[off..])?;
        if n == 0 {
            break;
        }
        off += n;
    }
    Some(off)
}

/// Write the whole buffer, looping over partial writes. Returns the number of
/// bytes written, which is less than `bytes.len()` only when the file is full.
pub fn fs_write_all(fd: u64, bytes: &[u8]) -> Option<usize> {
    let mut off = 0usize;
    while off < bytes.len() {
        let n = fs_write(fd, &bytes[off..])?;
        if n == 0 {
            break;
        }
        off += n;
    }
    Some(off)
}

pub fn fs_write_str(fd: u64, text: &str) -> Option<usize> {
    fs_write_all(fd, text.as_bytes())
}

/// Total bytes currently stored on the RAM disk.
pub fn fs_used_bytes() -> u64 {
    let count = fs_count();
    let mut total = 0u64;
    let mut i = 0usize;
    while i < count {
        let mut name = [0u8; FS_MAX_NAME + 1];
        if let Some(size) = fs_list(i, &mut name) {
            total += size as u64;
        }
        i += 1;
    }
    total
}

/// Columns and rows of the active console.
pub fn console_size() -> (usize, usize) {
    let value = unsafe { syscall5(SYS_CONSOLE_INFO, 0, 0, 0, 0, 0) };
    let cols = (value >> 32) as usize;
    let rows = (value & 0xFFFF_FFFF) as usize;
    if cols == 0 || rows == 0 {
        (80, 25)
    } else {
        (cols, rows)
    }
}

pub fn set_cursor(x: usize, y: usize) {
    unsafe {
        syscall5(SYS_CURSOR, x as u64, y as u64, 0, 0, 0);
    }
}

/// Non-blocking keyboard/serial poll. Returns None when no key is waiting.
pub fn key_poll() -> Option<u8> {
    let value = unsafe { syscall5(SYS_KEY_POLL, 0, 0, 0, 0, 0) as u8 };
    if value == 0 {
        None
    } else {
        Some(value)
    }
}

/// Read the PS/2 mouse state; `flags` reports availability and wheel support.
pub fn mouse_state() -> MouseState {
    let mut state = MouseState::zeroed();
    unsafe {
        syscall5(
            SYS_MOUSE_STATE,
            &mut state as *mut MouseState as u64,
            0,
            0,
            0,
            0,
        );
    }
    state
}

pub fn mouse_available() -> bool {
    (mouse_state().flags & MOUSE_FLAG_AVAILABLE) != 0
}

pub fn clear() {
    unsafe {
        syscall5(SYS_CLEAR, 0, 0, 0, 0, 0);
    }
}

pub fn read_line(buffer: &mut [u8]) -> usize {
    unsafe {
        syscall5(
            SYS_READ_LINE,
            buffer.as_mut_ptr() as u64,
            buffer.len() as u64,
            0,
            0,
            0,
        ) as usize
    }
}

pub fn exit(code: u64) -> ! {
    unsafe {
        syscall5(SYS_EXIT, code, 0, 0, 0, 0);
    }
    loop {
        core::hint::spin_loop();
    }
}

pub fn info(info: &mut SystemInfo) {
    unsafe {
        syscall5(SYS_INFO, info as *mut SystemInfo as u64, 0, 0, 0, 0);
    }
}

pub fn uptime_ms() -> u64 {
    unsafe { syscall5(SYS_UPTIME_MS, 0, 0, 0, 0, 0) }
}

pub fn sleep(ms: u64) {
    unsafe {
        syscall5(SYS_SLEEP, ms, 0, 0, 0, 0);
    }
}

pub fn reboot() -> ! {
    unsafe {
        syscall5(SYS_REBOOT, 0, 0, 0, 0, 0);
    }
    loop {
        core::hint::spin_loop();
    }
}

pub fn halt() -> ! {
    unsafe {
        syscall5(SYS_HALT, 0, 0, 0, 0, 0);
    }
    loop {
        core::hint::spin_loop();
    }
}

pub fn getchar() -> u8 {
    unsafe { syscall5(SYS_GETCHAR, 0, 0, 0, 0, 0) as u8 }
}

pub fn putchar(byte: u8) {
    unsafe {
        syscall5(SYS_PUTCHAR, byte as u64, 0, 0, 0, 0);
    }
}

pub fn command_count() -> usize {
    unsafe { syscall5(SYS_CMD_COUNT, 0, 0, 0, 0, 0) as usize }
}

pub fn command_get(index: usize, name: &mut [u8], description: &mut [u8]) -> bool {
    unsafe {
        syscall5(
            SYS_CMD_GET,
            index as u64,
            name.as_mut_ptr() as u64,
            name.len() as u64,
            description.as_mut_ptr() as u64,
            description.len() as u64,
        ) != 0
    }
}

pub fn args_str(ptr: *const u8, len: usize) -> &'static str {
    if ptr.is_null() || len == 0 {
        return "";
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    core::str::from_utf8(bytes).unwrap_or("")
}

struct Writer;

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write(s);
        Ok(())
    }
}

pub fn print_fmt(args: fmt::Arguments<'_>) {
    let _ = fmt::write(&mut Writer, args);
}

/// Print a KiB count using the largest sensible unit. Values below 1 GiB stay
/// in MiB so small VMs read naturally. Rounding keeps firmware-reserved holes
/// from turning 8 GiB into "7.9 GiB".
pub fn print_size(kb: u64) {
    if kb >= 1024 * 1024 {
        let tenths = (kb.saturating_mul(10) + 512 * 1024) / (1024 * 1024);
        print_fmt(core::format_args!("{}.{} GiB", tenths / 10, tenths % 10));
    } else if kb >= 1024 {
        print_fmt(core::format_args!("{} MiB", (kb + 512) / 1024));
    } else {
        print_fmt(core::format_args!("{} KiB", kb));
    }
}

/// Print a byte count using B, KiB, or MiB. Keeps one decimal place for the
/// larger units so small files read naturally.
pub fn print_size_bytes(bytes: u64) {
    if bytes >= 1024 * 1024 {
        let tenths = (bytes.saturating_mul(10) + 512 * 1024) / (1024 * 1024);
        print_fmt(core::format_args!("{}.{} MiB", tenths / 10, tenths % 10));
    } else if bytes >= 1024 {
        let tenths = (bytes.saturating_mul(10) + 512) / 1024;
        print_fmt(core::format_args!("{}.{} KiB", tenths / 10, tenths % 10));
    } else {
        print_fmt(core::format_args!("{} B", bytes));
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::print_fmt(core::format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::print!("{}\n", core::format_args!($($arg)*))
    };
}

// The no_std command binaries need these symbols for core/compiler-builtins.
#[no_mangle]
pub unsafe extern "C" fn memcpy(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    let dest = dest as *mut u8;
    let src = src as *const u8;
    let mut i = 0;
    while i < n {
        *dest.add(i) = *src.add(i);
        i += 1;
    }
    dest as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    let dest = dest as *mut u8;
    let src = src as *const u8;
    if (dest as usize) < (src as usize) {
        let mut i = 0;
        while i < n {
            *dest.add(i) = *src.add(i);
            i += 1;
        }
    } else if (dest as usize) > (src as usize) {
        let mut i = n;
        while i > 0 {
            i -= 1;
            *dest.add(i) = *src.add(i);
        }
    }
    dest as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn memset(dest: *mut c_void, value: i32, n: usize) -> *mut c_void {
    let dest = dest as *mut u8;
    let mut i = 0;
    while i < n {
        *dest.add(i) = value as u8;
        i += 1;
    }
    dest as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn memcmp(a: *const c_void, b: *const c_void, n: usize) -> i32 {
    let a = a as *const u8;
    let b = b as *const u8;
    let mut i = 0;
    while i < n {
        let x = *a.add(i);
        let y = *b.add(i);
        if x != y {
            return x as i32 - y as i32;
        }
        i += 1;
    }
    0
}
