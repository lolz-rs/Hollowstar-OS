use core::fmt;

use hollowstar_common::{BootInfo, BOOT_MAGIC, BOOT_VERSION};

use crate::{graphics, memory, serial};

extern "C" {
    fn vga_clear();
    fn vga_putc(c: i8);
    fn vga_set_color(color: u8);
    fn vga_get_color() -> u8;
}

const SHADOW_WIDTH: usize = 80;
const SHADOW_HEIGHT: usize = 25;
/// In VBE modes the legacy 0xB8000 window is not reliably readable or
/// writable, so keep the 80x25 text shadow in free low RAM instead. The VGA
/// text fallback still uses 0xB8000 through vga.c.
const SHADOW_ADDR: usize = 0x50000;
/// Cell capacity for the largest mode we support (3840x2160 gives a 480x135
/// grid). The live grid is clamped to this; 1080p uses 240x67.
const MAX_COLS: usize = 512;
const MAX_ROWS: usize = 256;
const MAX_CELLS: usize = MAX_COLS * MAX_ROWS;

/// Standard VGA 16-colour palette, used to colour text in graphics mode.
const PALETTE_RGB: [(u8, u8, u8); 16] = [
    (0x00, 0x00, 0x00),
    (0x00, 0x00, 0xAA),
    (0x00, 0xAA, 0x00),
    (0x00, 0xAA, 0xAA),
    (0xAA, 0x00, 0x00),
    (0xAA, 0x00, 0xAA),
    (0xAA, 0x55, 0x00),
    (0xAA, 0xAA, 0xAA),
    (0x55, 0x55, 0x55),
    (0x55, 0x55, 0xFF),
    (0x55, 0xFF, 0x55),
    (0x55, 0xFF, 0xFF),
    (0xFF, 0x55, 0x55),
    (0xFF, 0x55, 0xFF),
    (0xFF, 0xFF, 0x55),
    (0xFF, 0xFF, 0xFF),
];

static mut GRAPHICS: bool = false;
static mut PRESENT_READY: bool = false;
static mut COLS: usize = SHADOW_WIDTH;
static mut ROWS: usize = SHADOW_HEIGHT;
static mut CURSOR_X: usize = 0;
static mut CURSOR_Y: usize = 0;
static mut COLOR: u8 = 0x0F;
static mut CELLS: [u16; MAX_CELLS] = [0; MAX_CELLS];
static mut PALETTE: [u32; 16] = [0; 16];
static mut CURSOR_DRAWN: bool = false;
static mut DRAWN_X: usize = 0;
static mut DRAWN_Y: usize = 0;

#[inline]
fn shadow_ptr() -> *mut u16 {
    SHADOW_ADDR as *mut u16
}

#[inline]
fn graphics_active() -> bool {
    unsafe { GRAPHICS }
}

fn init_palette() {
    unsafe {
        let mut i = 0;
        while i < 16 {
            let (r, g, b) = PALETTE_RGB[i];
            PALETTE[i] = graphics::rgb(r, g, b);
            i += 1;
        }
    }
}

#[inline]
fn palette_color(index: u8) -> u32 {
    unsafe { PALETTE[(index & 0x0F) as usize] }
}

fn blank_cell() -> u16 {
    unsafe { (b' ' as u16) | ((COLOR as u16) << 8) }
}

fn clear_cells() {
    unsafe {
        let blank = blank_cell();
        // Cells are stored with a fixed MAX_COLS row stride, so clear the whole
        // backing store rather than COLS * ROWS contiguous entries.
        let mut i = 0;
        while i < MAX_CELLS {
            CELLS[i] = blank;
            i += 1;
        }
        CURSOR_X = 0;
        CURSOR_Y = 0;
        CURSOR_DRAWN = false;
    }
}

fn shadow_clear() {
    unsafe {
        let blank = blank_cell();
        let mut i = 0;
        while i < SHADOW_WIDTH * SHADOW_HEIGHT {
            *shadow_ptr().add(i) = blank;
            i += 1;
        }
    }
}

/// Rewrite the 80x25 dump shadow from the live cell buffer. Used after a
/// scroll, when rows that were previously below row 24 move into view.
fn shadow_sync() {
    unsafe {
        let mut y = 0;
        while y < SHADOW_HEIGHT {
            let mut x = 0;
            while x < SHADOW_WIDTH {
                *shadow_ptr().add(y * SHADOW_WIDTH + x) = CELLS[y * MAX_COLS + x];
                x += 1;
            }
            y += 1;
        }
    }
}

fn shadow_put(x: usize, y: usize, ch: u8, attr: u8) {
    if x >= SHADOW_WIDTH || y >= SHADOW_HEIGHT {
        return;
    }
    unsafe {
        *shadow_ptr().add(y * SHADOW_WIDTH + x) = (ch as u16) | ((attr as u16) << 8);
    }
}

fn draw_cell(x: usize, y: usize, ch: u8, attr: u8) {
    draw_cell_impl(x, y, ch, attr, false);
}

/// Draw one cell. When `bg_prefilled` is set the back buffer is already the
/// palette's black, so a black background does not need another fill. That
/// turns a full-screen redraw into one buffer clear plus glyph pixels.
fn draw_cell_impl(x: usize, y: usize, ch: u8, attr: u8, bg_prefilled: bool) {
    if !graphics_active() {
        return;
    }
    let fg = palette_color(attr & 0x0F);
    let bg = palette_color((attr >> 4) & 0x0F);
    let info = graphics::info();
    let fw = info.font_width.max(1);
    let fh = info.font_height.max(1);
    let px = x as u32 * fw;
    let py = y as u32 * fh;
    let stride = (info.pitch / 4) as usize;
    let base = info.back_buffer as *mut u32;

    // Direct pixel stores keep a full redraw cheap: the console has thousands
    // of cells, and a `rep stosd` per glyph run would dominate the frame.
    unsafe {
        if !bg_prefilled || bg != 0 {
            let mut row = 0;
            while row < fh {
                let dst = base.add((py + row) as usize * stride + px as usize);
                let mut col = 0;
                while col < fw {
                    *dst.add(col as usize) = bg;
                    col += 1;
                }
                row += 1;
            }
            graphics::mark_dirty(px, py, fw, fh);
        }

        if ch == b' ' || ch == 0 {
            return;
        }
        let glyph_cols = fw.min(8);
        let font = info.font_addr as *const u8;
        if font.is_null() {
            return;
        }
        let glyph = font.add(ch as usize * fh as usize);
        let mut row = 0;
        while row < fh {
            let bits = *glyph.add(row as usize);
            if bits != 0 {
                let dst = base.add((py + row) as usize * stride + px as usize);
                let mut col = 0;
                while col < glyph_cols {
                    if (bits & (0x80 >> col)) != 0 {
                        *dst.add(col as usize) = fg;
                    }
                    col += 1;
                }
            }
            row += 1;
        }
        graphics::mark_dirty(px, py, fw, fh);
    }
}

fn redraw_all() {
    if !graphics_active() {
        return;
    }
    // One clear covers the whole back buffer, including the partial last row
    // that the text grid does not reach.
    graphics::clear(0);
    unsafe {
        let mut y = 0;
        while y < ROWS {
            let mut x = 0;
            while x < COLS {
                let cell = CELLS[y * MAX_COLS + x];
                draw_cell_impl(x, y, (cell & 0xFF) as u8, (cell >> 8) as u8, true);
                x += 1;
            }
            y += 1;
        }
        CURSOR_DRAWN = false;
    }
}

fn draw_cursor() {
    if !graphics_active() {
        return;
    }
    unsafe {
        let fw = graphics::font_width().max(1);
        let fh = graphics::font_height().max(1);
        let px = CURSOR_X as u32 * fw;
        let py = CURSOR_Y as u32 * fh + fh.saturating_sub(2);
        graphics::fill_rect(px, py, fw, 2, palette_color(COLOR & 0x0F));
        CURSOR_DRAWN = true;
        DRAWN_X = CURSOR_X;
        DRAWN_Y = CURSOR_Y;
    }
}

fn put_cell(x: usize, y: usize, ch: u8) {
    if x >= unsafe { COLS } || y >= unsafe { ROWS } {
        return;
    }
    let attr = unsafe { COLOR };
    unsafe {
        CELLS[y * MAX_COLS + x] = (ch as u16) | ((attr as u16) << 8);
    }
    if !graphics::cmd_active() {
        draw_cell(x, y, ch, attr);
    }
    shadow_put(x, y, ch, attr);
}

fn scroll() {
    unsafe {
        let mut y = 1;
        while y < ROWS {
            let mut x = 0;
            while x < COLS {
                CELLS[(y - 1) * MAX_COLS + x] = CELLS[y * MAX_COLS + x];
                x += 1;
            }
            y += 1;
        }
        let blank = blank_cell();
        let mut x = 0;
        while x < COLS {
            CELLS[(ROWS - 1) * MAX_COLS + x] = blank;
            x += 1;
        }
        CURSOR_Y = ROWS - 1;
    }
    shadow_sync();
    redraw_all();
}

fn newline() {
    unsafe {
        CURSOR_X = 0;
        if CURSOR_Y + 1 >= ROWS {
            scroll();
        } else {
            CURSOR_Y += 1;
        }
    }
}

fn write_graphics(byte: u8) {
    unsafe {
        match byte {
            b'\n' => newline(),
            b'\r' => CURSOR_X = 0,
            b'\x08' => {
                if CURSOR_X > 0 {
                    CURSOR_X -= 1;
                } else if CURSOR_Y > 0 {
                    CURSOR_Y -= 1;
                    CURSOR_X = COLS - 1;
                } else {
                    return;
                }
                put_cell(CURSOR_X, CURSOR_Y, b' ');
            }
            b'\t' => {
                let next = (CURSOR_X + 8) & !7;
                while CURSOR_X < next {
                    if CURSOR_X >= COLS {
                        newline();
                    } else {
                        put_cell(CURSOR_X, CURSOR_Y, b' ');
                        CURSOR_X += 1;
                    }
                }
            }
            _ => {
                if CURSOR_X >= COLS {
                    newline();
                }
                put_cell(CURSOR_X, CURSOR_Y, byte);
                CURSOR_X += 1;
                if CURSOR_X >= COLS {
                    newline();
                }
            }
        }
    }
}

pub fn init() {
    serial::init();

    let info: &BootInfo = memory::boot_info();
    let mut graphics_ok = false;
    if info.magic == BOOT_MAGIC && info.version == BOOT_VERSION {
        graphics_ok = graphics::init(info);
    }

    if graphics_ok {
        unsafe {
            GRAPHICS = true;
            let fw = graphics::font_width().max(1);
            let fh = graphics::font_height().max(1);
            COLS = ((graphics::width() / fw) as usize).clamp(1, MAX_COLS);
            ROWS = ((graphics::height() / fh) as usize).clamp(1, MAX_ROWS);
        }
        init_palette();
        clear_cells();
        shadow_clear();
        redraw_all();
    } else {
        unsafe {
            vga_clear();
        }
    }
}

/// Enable presenting after interrupts and the PIT are live. Before this point
/// the back buffer is filled but nothing is copied to the screen.
pub fn set_present_ready(ready: bool) {
    unsafe {
        PRESENT_READY = ready;
    }
    if ready {
        flush();
    }
}

pub fn clear() {
    if graphics_active() {
        clear_cells();
        shadow_clear();
        // redraw_all() blanks the whole back buffer first: the text grid may
        // not cover the last partial row (for example 1080 with a 16-pixel
        // font), and a previous graphics command could have left pixels there.
        redraw_all();
        flush();
    } else {
        unsafe {
            vga_clear();
        }
    }
    // ANSI clear for serial terminals.
    serial::write_str("\x1b[2J\x1b[H");
}

pub fn set_color(color: u8) {
    if graphics_active() {
        unsafe {
            COLOR = color;
        }
    } else {
        unsafe {
            vga_set_color(color);
        }
    }
}

pub fn get_color() -> u8 {
    if graphics_active() {
        unsafe { COLOR }
    } else {
        unsafe { vga_get_color() }
    }
}

pub fn write_byte(byte: u8) {
    if graphics_active() {
        write_graphics(byte);
    } else {
        unsafe {
            vga_putc(byte as i8);
        }
    }
    match byte {
        b'\n' => {
            serial::write_byte(b'\r');
            serial::write_byte(b'\n');
        }
        b'\x08' => {
            serial::write_bytes(b"\x08 \x08");
        }
        _ => serial::write_byte(byte),
    }
}

pub fn write_bytes(bytes: &[u8]) {
    for &byte in bytes {
        write_byte(byte);
    }
}

pub fn write_str(text: &str) {
    write_bytes(text.as_bytes());
}

/// Copy the finished back buffer to the framebuffer at the next vertical
/// retrace. The console cursor is drawn here so it never tears.
pub fn flush() {
    if !graphics_active() || graphics::cmd_active() {
        return;
    }
    if !(unsafe { PRESENT_READY }) {
        return;
    }
    unsafe {
        if CURSOR_DRAWN {
            let cell = CELLS[DRAWN_Y * MAX_COLS + DRAWN_X];
            draw_cell(DRAWN_X, DRAWN_Y, (cell & 0xFF) as u8, (cell >> 8) as u8);
        }
    }
    draw_cursor();
    graphics::present_dirty();
}

pub fn enter_graphics_command() -> bool {
    if !graphics_active() {
        return false;
    }
    graphics::enter()
}

pub fn leave_graphics_command() {
    if !graphics_active() {
        return;
    }
    graphics::leave();
    // The command may have drawn into margins the text grid does not cover.
    redraw_all();
    flush();
}

/// Called when a command finishes, even if it exited without calling
/// `gfx::leave`. Restores the console only if a command still owns the screen.
pub fn finish_command() {
    if graphics::cmd_active() {
        leave_graphics_command();
    }
}

struct Writer;

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_str(s);
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments<'_>) {
    let _ = fmt::write(&mut Writer, args);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::console::_print(core::format_args!($($arg)*))
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
