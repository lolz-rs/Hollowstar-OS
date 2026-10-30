//! VBE linear-framebuffer support with a RAM back buffer and vsync present.
//!
//! The kernel owns the back buffer and the console. Commands draw into the
//! same buffer through the `SYS_GFX_*` calls and ask the kernel to present a
//! finished frame, which keeps every update tear-free.
//!
//! Drawing tracks a dirty rectangle. The console presents only the cells that
//! changed, and commands can pass their own rectangle to
//! `SYS_GFX_PRESENT_RECT`, so a full-screen copy is only paid for when the
//! whole screen actually changed.

use core::arch::asm;
use core::ptr;

use hollowstar_common::{
    BootInfo, GraphicsInfo, BACK_BUFFER_ADDR, BACK_BUFFER_MAX_BYTES, BOOT_MAGIC, BOOT_VERSION,
    GFX_FLAG_AVAILABLE, GFX_FLAG_FONT, GFX_FLAG_LINEAR,
};

use crate::{io, memory};

/// Sanity limit. 1920x1080x32 is 8.3 MiB; 3840x2160x32 is 33.2 MiB.
const MAX_FRAMEBUFFER_BYTES: u64 = BACK_BUFFER_MAX_BYTES;

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
static mut BACK_ADDR: u64 = BACK_BUFFER_ADDR;
static mut BACK_SIZE: u32 = 0;
static mut RED_SHIFT: u32 = 16;
static mut GREEN_SHIFT: u32 = 8;
static mut BLUE_SHIFT: u32 = 0;

static mut DIRTY: bool = false;
static mut DIRTY_X0: u32 = 0;
static mut DIRTY_Y0: u32 = 0;
static mut DIRTY_X1: u32 = 0;
static mut DIRTY_Y1: u32 = 0;

/// Some firmware (and QEMU's headless VGA) never toggles the retrace bit.
/// Probe once, then stop paying for a doomed wait on every frame.
static mut VSYNC_PROBED: bool = false;
static mut VSYNC_OK: bool = true;

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
    // Stage2 records the buffer it sized for this mode. Fall back to the
    // fixed low-RAM address for older boot info.
    let back_addr = if info.back_buffer_addr != 0 {
        info.back_buffer_addr
    } else {
        BACK_BUFFER_ADDR
    };
    let back_end = back_addr.saturating_add(size);
    if !range_is_usable(back_addr, back_end) {
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
        BACK_ADDR = back_addr;
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

pub fn info() -> GraphicsInfo {
    let mut gi = GraphicsInfo::zeroed();
    if !active() {
        return gi;
    }
    unsafe {
        gi.back_buffer = BACK_ADDR;
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

fn clear_dirty() {
    unsafe {
        DIRTY = false;
    }
}

/// Mark a rectangle (x, y, w, h) as changed. Coordinates are clipped.
pub fn mark_dirty(x: u32, y: u32, w: u32, h: u32) {
    if !active() || w == 0 || h == 0 {
        return;
    }
    let (width, height) = unsafe { (WIDTH, HEIGHT) };
    if x >= width || y >= height {
        return;
    }
    let x1 = x.saturating_add(w).min(width);
    let y1 = y.saturating_add(h).min(height);
    if x1 <= x || y1 <= y {
        return;
    }
    unsafe {
        if !DIRTY {
            DIRTY = true;
            DIRTY_X0 = x;
            DIRTY_Y0 = y;
            DIRTY_X1 = x1;
            DIRTY_Y1 = y1;
        } else {
            DIRTY_X0 = DIRTY_X0.min(x);
            DIRTY_Y0 = DIRTY_Y0.min(y);
            DIRTY_X1 = DIRTY_X1.max(x1);
            DIRTY_Y1 = DIRTY_Y1.max(y1);
        }
    }
}

fn dirty_rect() -> Option<(u32, u32, u32, u32)> {
    unsafe {
        if !DIRTY {
            None
        } else {
            Some((
                DIRTY_X0,
                DIRTY_Y0,
                DIRTY_X1 - DIRTY_X0,
                DIRTY_Y1 - DIRTY_Y0,
            ))
        }
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
        let dst = BACK_ADDR as *mut u32;
        asm!(
            "cld; rep stosd",
            inout("rdi") dst => _,
            in("eax") color,
            inout("rcx") count => _,
            options(nostack)
        );
    }
    mark_dirty(0, 0, width(), height());
}

#[inline]
pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    unsafe {
        ((r as u32) << RED_SHIFT) | ((g as u32) << GREEN_SHIFT) | ((b as u32) << BLUE_SHIFT)
    }
}

/// Fill one span of 32-bit pixels.
#[inline]
fn fill_span(dst: *mut u32, count: usize, color: u32) {
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
    let stride = (pitch / 4) as usize;
    let count = (x1 - x) as usize;
    mark_dirty(x, y, x1 - x, y1 - y);
    let base = unsafe { BACK_ADDR as *mut u32 };
    if x == 0 && x1 == width {
        // Whole rows are contiguous, so one store covers the rectangle.
        let dst = unsafe { base.add(y as usize * stride) };
        fill_span(dst, (y1 - y) as usize * stride, color);
    } else {
        let mut row = y;
        while row < y1 {
            let dst = unsafe { base.add(row as usize * stride + x as usize) };
            fill_span(dst, count, color);
            row += 1;
        }
    }
}

/// Copy `len` bytes. The caller has already clipped everything.
#[inline]
fn copy_bytes(src: *const u8, dst: *mut u8, len: usize) {
    if len == 0 {
        return;
    }
    unsafe {
        // Large, aligned copies are the common case (a full 1080p frame).
        // Sixteen-byte streaming stores beat `rep movsq` on emulated VGA
        // memory, where each qword string iteration is expensive.
        if len >= 4096 && ((src as usize) | (dst as usize)) & 15 == 0 {
            let mut s = src;
            let mut d = dst;
            let mut n = len;
            while n >= 64 {
                asm!(
                    "movdqu xmm0, [{s}]",
                    "movdqu xmm1, [{s} + 16]",
                    "movdqu xmm2, [{s} + 32]",
                    "movdqu xmm3, [{s} + 48]",
                    "movntdq [{d}], xmm0",
                    "movntdq [{d} + 16], xmm1",
                    "movntdq [{d} + 32], xmm2",
                    "movntdq [{d} + 48], xmm3",
                    s = in(reg) s,
                    d = in(reg) d,
                    out("xmm0") _,
                    out("xmm1") _,
                    out("xmm2") _,
                    out("xmm3") _,
                    options(nostack)
                );
                s = s.add(64);
                d = d.add(64);
                n -= 64;
            }
            while n >= 16 {
                asm!(
                    "movdqu xmm0, [{s}]",
                    "movntdq [{d}], xmm0",
                    s = in(reg) s,
                    d = in(reg) d,
                    out("xmm0") _,
                    options(nostack)
                );
                s = s.add(16);
                d = d.add(16);
                n -= 16;
            }
            asm!("sfence", options(nostack, preserves_flags));
            let mut tail_src = s;
            let mut tail_dst = d;
            let mut tail = n;
            while tail > 0 {
                *tail_dst = *tail_src;
                tail_src = tail_src.add(1);
                tail_dst = tail_dst.add(1);
                tail -= 1;
            }
            return;
        }

        let qwords = len / 8;
        if qwords != 0 {
            asm!(
                "cld; rep movsq",
                inout("rsi") src => _,
                inout("rdi") dst => _,
                inout("rcx") qwords => _,
                options(nostack)
            );
        }
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
}

/// Wait for the start of vertical blanking. VGA-compatible adapters expose
/// the retrace bit on port 0x3DA; if it never changes, fall back to a bounded
/// spin so a non-VGA framebuffer cannot hang the machine.
pub fn wait_vsync() {
    if !active() {
        return;
    }
    unsafe {
        if VSYNC_PROBED && !VSYNC_OK {
            return;
        }
    }
    // QEMU's headless VGA and some firmware do not toggle the retrace bit.
    // Bound the wait so a missing retrace costs a fraction of a frame instead
    // of several milliseconds of trapped port reads.
    const VSYNC_TIMEOUT: u32 = 100_000;
    let mut timeout = 0u32;
    unsafe {
        while (io::inb(0x3DA) & 0x08) != 0 {
            timeout += 1;
            if timeout > VSYNC_TIMEOUT {
                VSYNC_PROBED = true;
                VSYNC_OK = false;
                return;
            }
        }
        timeout = 0;
        while (io::inb(0x3DA) & 0x08) == 0 {
            timeout += 1;
            if timeout > VSYNC_TIMEOUT {
                VSYNC_PROBED = true;
                VSYNC_OK = false;
                return;
            }
        }
        VSYNC_PROBED = true;
        VSYNC_OK = true;
    }
}

/// Copy a rectangle from the back buffer to the framebuffer. The software
/// cursor is composited for the duration of the copy and then removed from the
/// back buffer, so commands never see it in their drawing surface.
pub fn present_rect(x: u32, y: u32, w: u32, h: u32) {
    if !active() {
        return;
    }
    let (width, height, pitch, fb_pitch) = unsafe { (WIDTH, HEIGHT, PITCH, FB_PITCH) };
    if x >= width || y >= height || w == 0 || h == 0 {
        return;
    }
    let mut x0 = x;
    let mut y0 = y;
    let mut x1 = x.saturating_add(w).min(width);
    let mut y1 = y.saturating_add(h).min(height);

    crate::mouse::cursor_draw();
    if let Some((cx, cy, cw, ch)) = crate::mouse::cursor_bounds() {
        if cx < x1 && cy < y1 && cx + cw > x0 && cy + ch > y0 {
            x0 = x0.min(cx);
            y0 = y0.min(cy);
            x1 = x1.max((cx + cw).min(width));
            y1 = y1.max((cy + ch).min(height));
        }
    }
    // cursor_draw() also marks the previous cursor rectangle dirty, so the
    // old arrow is repainted instead of being left on the screen.
    if let Some((dx, dy, dw, dh)) = dirty_rect() {
        x0 = x0.min(dx);
        y0 = y0.min(dy);
        x1 = x1.max((dx + dw).min(width));
        y1 = y1.max((dy + dh).min(height));
    }
    if x1 <= x0 || y1 <= y0 {
        crate::mouse::cursor_restore();
        clear_dirty();
        return;
    }

    // A syscall gate enters with interrupts disabled. Presenting can take
    // several milliseconds, so let the PIT and serial IRQs run while the copy
    // is in flight; otherwise uptime and sleep drift under graphics load.
    let flags = unsafe { io::read_rflags() };
    let reenable = (flags & 0x200) == 0;
    if reenable {
        unsafe {
            io::enable_interrupts();
        }
    }
    wait_vsync();
    let back = unsafe { BACK_ADDR as *const u8 };
    let fb = unsafe { FB_ADDR as *mut u8 };
    let rw = (x1 - x0) as usize;
    if x0 == 0 && rw == width as usize && pitch == fb_pitch {
        // Full rows: one contiguous copy, padding included.
        let len = (y1 - y0) as usize * pitch as usize;
        unsafe {
            copy_bytes(
                back.add(y0 as usize * pitch as usize),
                fb.add(y0 as usize * fb_pitch as usize),
                len,
            );
        }
    } else {
        let mut row = y0;
        let row_bytes = rw * 4;
        while row < y1 {
            unsafe {
                copy_bytes(
                    back.add(row as usize * pitch as usize + x0 as usize * 4),
                    fb.add(row as usize * fb_pitch as usize + x0 as usize * 4),
                    row_bytes,
                );
            }
            row += 1;
        }
    }
    if reenable {
        unsafe {
            io::disable_interrupts();
        }
    }
    crate::mouse::cursor_restore();
    clear_dirty();
}

/// Present everything that changed since the last present.
pub fn present_dirty() {
    if !active() {
        return;
    }
    if let Some((x, y, w, h)) = dirty_rect() {
        present_rect(x, y, w, h);
    }
}

/// Present the whole frame.
pub fn present() {
    present_rect(0, 0, width(), height());
}
