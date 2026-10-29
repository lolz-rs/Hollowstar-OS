//! VBE linear-framebuffer support with a RAM back buffer and vsync present.
//!
//! The kernel owns the back buffer and the console. Commands draw into the
//! same buffer through the `SYS_GFX_*` calls and ask the kernel to present a
//! finished frame, which keeps every update tear-free.

use core::arch::asm;
use core::ptr;

use hollowstar_common::{
    BootInfo, GraphicsInfo, BACK_BUFFER_ADDR, BOOT_MAGIC, BOOT_VERSION, GFX_FLAG_AVAILABLE,
    GFX_FLAG_FONT, GFX_FLAG_LINEAR,
};

use crate::{io, memory};

/// Sanity limit: 1024x768x32 is 3 MiB, 1920x1080x32 is 8.3 MiB.
const MAX_FRAMEBUFFER_BYTES: u64 = 16 * 1024 * 1024;

static mut ACTIVE: bool = false;
static mut CMD_ACTIVE: bool = false;
static mut FB_ADDR: u64 = 0;
static mut FB_PITCH: u32 = 0;
static mut WIDTH: u32 = 0;
static mut HEIGHT: u32 = 0;
static mut PITCH: u32 = 0;
static mut BPP: u32 = 0;
static mut FONT_ADDR: u64 = 0;
static mut FONT_WIDTH: u32 = 8;
static mut FONT_HEIGHT: u32 = 16;
static mut BACK_SIZE: u32 = 0;
static mut RED_SHIFT: u32 = 16;
static mut GREEN_SHIFT: u32 = 8;
static mut BLUE_SHIFT: u32 = 0;

fn mask_shift(mask: u32) -> u32 {
    if mask == 0 {
        0
    } else {
        mask.trailing_zeros()
    }
}

fn range_is_usable(start: u64, end: u64) -> bool {
    for entry in memory::entries() {
        if entry.kind != 1 || entry.length == 0 {
            continue;
        }
        let entry_end = entry.base.saturating_add(entry.length);
        if entry.base <= start && end <= entry_end {
            return true;
        }
    }
    false
}

pub fn init(info: &BootInfo) -> bool {
    if info.magic != BOOT_MAGIC || info.version != BOOT_VERSION {
        return false;
    }
    let flags = info.graphics_flags;
    if (flags & (GFX_FLAG_AVAILABLE | GFX_FLAG_LINEAR | GFX_FLAG_FONT))
        != (GFX_FLAG_AVAILABLE | GFX_FLAG_LINEAR | GFX_FLAG_FONT)
    {
        return false;
    }
    if info.framebuffer_addr == 0 || info.framebuffer_bpp != 32 {
        return false;
    }
    if info.framebuffer_width < 320 || info.framebuffer_height < 200 {
        return false;
    }
    if info.framebuffer_pitch < info.framebuffer_width.saturating_mul(4) {
        return false;
    }
    let size = (info.framebuffer_pitch as u64) * (info.framebuffer_height as u64);
    if size == 0 || size > MAX_FRAMEBUFFER_BYTES {
        return false;
    }
    let font_width = if info.font_width == 0 { 8 } else { info.font_width };
    let font_height = if info.font_height == 0 {
        16
    } else {
        info.font_height
    };
    // The font must be inside usable RAM so reading it cannot fault.
    if !range_is_usable(info.font_addr, info.font_addr.saturating_add(4096))
        || !font_is_sane(info.font_addr, font_width, font_height)
    {
        return false;
    }
    let back_end = BACK_BUFFER_ADDR.saturating_add(size);
    if !range_is_usable(BACK_BUFFER_ADDR, back_end) {
        return false;
    }

    unsafe {
        FB_ADDR = info.framebuffer_addr;
        FB_PITCH = info.framebuffer_pitch;
        WIDTH = info.framebuffer_width;
        HEIGHT = info.framebuffer_height;
        PITCH = info.framebuffer_pitch;
        BPP = info.framebuffer_bpp;
        FONT_ADDR = info.font_addr;
        FONT_WIDTH = font_width;
        FONT_HEIGHT = font_height;
        BACK_SIZE = size as u32;
        RED_SHIFT = mask_shift(info.framebuffer_red_mask);
        GREEN_SHIFT = mask_shift(info.framebuffer_green_mask);
        BLUE_SHIFT = mask_shift(info.framebuffer_blue_mask);
        CMD_ACTIVE = false;
        ACTIVE = true;
        // Clear only after ACTIVE is set; clear() is a no-op otherwise.
        clear(0);
    }
    true
}

/// Check that the copied BIOS font looks like an 8x16 VGA font. A few firmware
/// implementations return a stale or missing pointer from the INT 10h font
/// call; in that case we keep the VGA text console instead of drawing garbage.
fn font_is_sane(addr: u64, width: u32, height: u32) -> bool {
    if addr == 0 || width == 0 || width > 8 || height < 8 || height > 32 {
        return false;
    }
    let font = addr as *const u8;
    // The space glyph must be blank and 'A' must contain ink.
    for row in 0..height {
        if unsafe { ptr::read_volatile(font.add(0x20 * height as usize + row as usize)) } != 0 {
            return false;
        }
    }
    let mut ink = 0u32;
    for row in 0..height {
        let bits = unsafe { ptr::read_volatile(font.add(0x41 * height as usize + row as usize)) };
        ink += (bits & 0xFF).count_ones();
    }
    ink > 0
}

#[inline]
pub fn active() -> bool {
    unsafe { ACTIVE }
}

#[inline]
pub fn cmd_active() -> bool {
    unsafe { CMD_ACTIVE }
}

#[inline]
pub fn width() -> u32 {
    unsafe { WIDTH }
}

#[inline]
pub fn height() -> u32 {
    unsafe { HEIGHT }
}

#[inline]
pub fn font_width() -> u32 {
    unsafe { FONT_WIDTH }
}

#[inline]
pub fn font_height() -> u32 {
    unsafe { FONT_HEIGHT }
}

#[inline]
pub fn font_addr() -> u64 {
    unsafe { FONT_ADDR }
}

pub fn info() -> GraphicsInfo {
    let mut gi = GraphicsInfo::zeroed();
    if !active() {
        return gi;
    }
    unsafe {
        gi.back_buffer = BACK_BUFFER_ADDR;
        gi.framebuffer = FB_ADDR;
        gi.width = WIDTH;
        gi.height = HEIGHT;
        gi.pitch = PITCH;
        gi.bpp = BPP;
        gi.red_mask = 0xFF << RED_SHIFT;
        gi.green_mask = 0xFF << GREEN_SHIFT;
        gi.blue_mask = 0xFF << BLUE_SHIFT;
        gi.framebuffer_pitch = FB_PITCH;
        gi.font_addr = FONT_ADDR;
        gi.font_width = FONT_WIDTH;
        gi.font_height = FONT_HEIGHT;
        gi.text_cols = WIDTH / FONT_WIDTH;
        gi.text_rows = HEIGHT / FONT_HEIGHT;
        gi.flags = GFX_FLAG_AVAILABLE | GFX_FLAG_LINEAR | GFX_FLAG_FONT;
    }
    gi
}

/// Take over the screen for a command. The console cell buffer is preserved;
/// `leave` redraws it when the command is done.
pub fn enter() -> bool {
    if !active() {
        return false;
    }
    clear(0);
    unsafe {
        CMD_ACTIVE = true;
    }
    true
}

pub fn leave() {
    unsafe {
        CMD_ACTIVE = false;
    }
}

pub fn clear(color: u32) {
    if !active() {
        return;
    }
    // Saved cursor pixels are about to be overwritten.
    crate::mouse::cursor_reset();
    unsafe {
        let count = (BACK_SIZE as usize) / 4;
        let dst = BACK_BUFFER_ADDR as *mut u32;
        asm!(
            "cld; rep stosd",
            inout("rdi") dst => _,
            in("eax") color,
            inout("rcx") count => _,
            options(nostack)
        );
    }
}

#[inline]
pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    unsafe {
        ((r as u32) << RED_SHIFT) | ((g as u32) << GREEN_SHIFT) | ((b as u32) << BLUE_SHIFT)
    }
}

pub fn fill_rect(x: u32, y: u32, w: u32, h: u32, color: u32) {
    if !active() {
        return;
    }
    let (width, height, pitch) = unsafe { (WIDTH, HEIGHT, PITCH) };
    if x >= width || y >= height {
        return;
    }
    let x1 = x.saturating_add(w).min(width);
    let y1 = y.saturating_add(h).min(height);
    if x1 <= x || y1 <= y {
        return;
    }
    let count = (x1 - x) as usize;
    let stride = (pitch / 4) as usize;
    for row in y..y1 {
        let dst = unsafe {
            (BACK_BUFFER_ADDR as *mut u32).add(row as usize * stride + x as usize)
        };
        unsafe {
            asm!(
                "cld; rep stosd",
                inout("rdi") dst => _,
                in("eax") color,
                inout("rcx") count => _,
                options(nostack)
            );
        }
    }
}

#[inline]
pub fn pixel(x: u32, y: u32, color: u32) {
    if !active() {
        return;
    }
    let (width, height, pitch) = unsafe { (WIDTH, HEIGHT, PITCH) };
    if x >= width || y >= height {
        return;
    }
    let stride = (pitch / 4) as usize;
    unsafe {
        ptr::write(
            (BACK_BUFFER_ADDR as *mut u32).add(y as usize * stride + x as usize),
            color,
        );
    }
}

/// Wait for the start of vertical blanking. VGA-compatible adapters expose
/// the retrace bit on port 0x3DA; if it never changes, fall back to a bounded
/// spin so a non-VGA framebuffer cannot hang the machine.
pub fn wait_vsync() {
    if !active() {
        return;
    }
    let mut timeout = 0u32;
    unsafe {
        while (io::inb(0x3DA) & 0x08) != 0 {
            timeout += 1;
            if timeout > 4_000_000 {
                return;
            }
        }
        timeout = 0;
        while (io::inb(0x3DA) & 0x08) == 0 {
            timeout += 1;
            if timeout > 4_000_000 {
                return;
            }
        }
    }
}

pub fn present() {
    if !active() {
        return;
    }
    // Draw the cursor for this frame, then put the canvas back.
    crate::mouse::cursor_draw();
    wait_vsync();
    unsafe {
        let src = BACK_BUFFER_ADDR as *const u8;
        let dst = FB_ADDR as *mut u8;
        let len = BACK_SIZE as usize;
        let qwords = len / 8;
        asm!(
            "cld; rep movsq",
            inout("rsi") src => _,
            inout("rdi") dst => _,
            inout("rcx") qwords => _,
            options(nostack)
        );
        let tail = len % 8;
        if tail != 0 {
            asm!(
                "cld; rep movsb",
                inout("rsi") src => _,
                inout("rdi") dst => _,
                inout("rcx") tail => _,
                options(nostack)
            );
        }
    }
    crate::mouse::cursor_restore();
}
