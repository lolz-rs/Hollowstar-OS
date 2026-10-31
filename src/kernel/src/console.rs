use core::fmt;

use hollowstar_common::{color, BootInfo, BOOT_MAGIC, BOOT_VERSION};

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
/// Cell capacity. 1080p with the 14px font is 137x45.
const MAX_COLS: usize = 256;
const MAX_ROWS: usize = 128;
const MAX_CELLS: usize = MAX_COLS * MAX_ROWS;
/// Lines kept after they scroll off the top. Wheel-up shows them again.
const SCROLL_LINES: usize = 128;
const SCROLL_ADDR: usize = 0x0048_0000;

/// Standard VGA 16-colour palette. Only used by the VGA text fallback and the
/// 80x25 monitor dump, which are limited to 16 hardware colours. The graphics
/// console stores full true-colour values instead.
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

/// One console cell. Colours are packed `0x00RRGGBB` true colour.
/// 12 bytes: 512×256 cells is 1.5 MiB, placed at `CELLS_ADDR`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Cell {
    ch: u8,
    fg: u32,
    bg: u32,
}

/// The cell buffer lives in free low RAM between the page-directory area
/// (which starts at 2 MiB) and the command load address (8 MiB). Keeping it
/// out of .bss lets a full 4K grid coexist with the bootloader's 512 KiB flat
/// kernel limit, and it is still identity-mapped.
const CELLS_ADDR: usize = 0x0040_0000;

static mut GRAPHICS: bool = false;
static mut PRESENT_READY: bool = false;
static mut COLS: usize = SHADOW_WIDTH;
static mut ROWS: usize = SHADOW_HEIGHT;
static mut CURSOR_X: usize = 0;
static mut CURSOR_Y: usize = 0;
static mut COLOR: u32 = color::WHITE;
static mut CELLS: *mut Cell = CELLS_ADDR as *mut Cell;
static mut CURSOR_DRAWN: bool = false;
static mut DRAWN_X: usize = 0;
static mut DRAWN_Y: usize = 0;
/// How many lines above the live screen the view is. 0 means the prompt.
static mut VIEW: usize = 0;
static mut SCROLL_COUNT: usize = 0;
static mut SCROLL_HEAD: usize = 0;

#[inline]
unsafe fn cell_get(index: usize) -> Cell {
    *CELLS.add(index)
}

#[inline]
unsafe fn cell_set(index: usize, cell: Cell) {
    *CELLS.add(index) = cell;
}

#[inline]
fn shadow_ptr() -> *mut u16 {
    SHADOW_ADDR as *mut u16
}

#[inline]
fn graphics_active() -> bool {
    unsafe { GRAPHICS }
}

/// Nearest VGA palette index for a true-colour value.
fn vga_index(rgb: u32) -> u8 {
    let r = ((rgb >> 16) & 0xFF) as i32;
    let g = ((rgb >> 8) & 0xFF) as i32;
    let b = (rgb & 0xFF) as i32;
    let mut best = 0usize;
    let mut best_distance = i32::MAX;
    let mut i = 0usize;
    while i < 16 {
        let (pr, pg, pb) = PALETTE_RGB[i];
        let dr = r - pr as i32;
        let dg = g - pg as i32;
        let db = b - pb as i32;
        let distance = dr * dr + dg * dg + db * db;
        if distance < best_distance {
            best_distance = distance;
            best = i;
        }
        i += 1;
    }
    best as u8
}

fn vga_attr(fg: u32, bg: u32) -> u8 {
    vga_index(fg) | (vga_index(bg) << 4)
}

fn vga_index_rgb(index: u8) -> u32 {
    let (r, g, b) = PALETTE_RGB[(index & 0x0F) as usize];
    color::rgb(r, g, b)
}

fn blank_cell() -> Cell {
    unsafe {
        Cell {
            ch: b' ',
            fg: COLOR,
            bg: color::BLACK,
        }
    }
}

fn clear_cells() {
    unsafe {
        let blank = blank_cell();
        // Cells are stored with a fixed MAX_COLS row stride, so clear the whole
        // backing store rather than COLS * ROWS contiguous entries.
        let mut i = 0;
        while i < MAX_CELLS {
            cell_set(i, blank);
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
        let attr = vga_attr(blank.fg, blank.bg) as u16;
        let word = (blank.ch as u16) | (attr << 8);
        let mut i = 0;
        while i < SHADOW_WIDTH * SHADOW_HEIGHT {
            *shadow_ptr().add(i) = word;
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
                shadow_put(x, y, cell_get(y * MAX_COLS + x));
                x += 1;
            }
            y += 1;
        }
    }
}

fn shadow_put(x: usize, y: usize, cell: Cell) {
    if x >= SHADOW_WIDTH || y >= SHADOW_HEIGHT {
        return;
    }
    let attr = vga_attr(cell.fg, cell.bg) as u16;
    unsafe {
        *shadow_ptr().add(y * SHADOW_WIDTH + x) = (cell.ch as u16) | (attr << 8);
    }
}

fn draw_cell(x: usize, y: usize, cell: Cell) {
    draw_cell_impl(x, y, cell, false);
}

/// Draw one cell. When `bg_prefilled` is set the back buffer is already the
/// cell background colour, so a black background does not need another fill.
/// That turns a full-screen redraw into one buffer clear plus glyph pixels.
fn draw_cell_impl(x: usize, y: usize, cell: Cell, bg_prefilled: bool) {
    if !graphics_active() {
        return;
    }
    let fg = graphics::from_rgb(cell.fg);
    let bg = graphics::from_rgb(cell.bg);
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

        if cell.ch == b' ' || cell.ch == 0 {
            return;
        }
        let glyph_stride = ((fw + 7) / 8) as usize;
        let font = info.font_addr as *const u8;
        if font.is_null() {
            return;
        }
        let glyph = font.add(cell.ch as usize * fh as usize * glyph_stride);
        let mut row = 0;
        while row < fh {
            let dst = base.add((py + row) as usize * stride + px as usize);
            let mut col = 0;
            while col < fw {
                let bits = *glyph.add(row as usize * glyph_stride + (col / 8) as usize);
                if (bits & (0x80 >> (col % 8))) != 0 {
                    *dst.add(col as usize) = fg;
                }
                col += 1;
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
                draw_cell_impl(x, y, cell_get(y * MAX_COLS + x), true);
                x += 1;
            }
            y += 1;
        }
        CURSOR_DRAWN = false;
    }
}

/// A `|` in the gap on the right of the previous character. Glyphs leave a
/// blank column on the left, so a 2px bar there reads as a text caret.
fn draw_caret_bar(cell_x: usize, cell_y: usize) {
    let fw = graphics::font_width().max(1);
    let fh = graphics::font_height().max(1);
    // One pixel on the right edge of the previous cell, so the bar does not
    // cover the letter it is next to.
    let px = (cell_x as u32 * fw).saturating_sub(1);
    let py = cell_y as u32 * fh + 2;
    let h = fh.saturating_sub(4).max(1);
    graphics::fill_rect(px, py, 1, h, graphics::from_rgb(unsafe { COLOR }));
}

fn draw_cursor() {
    if !graphics_active() {
        return;
    }
    unsafe {
        draw_caret_bar(CURSOR_X, CURSOR_Y);
        CURSOR_DRAWN = true;
        DRAWN_X = CURSOR_X;
        DRAWN_Y = CURSOR_Y;
    }
}

fn put_cell(x: usize, y: usize, ch: u8) {
    if x >= unsafe { COLS } || y >= unsafe { ROWS } {
        return;
    }
    let cell = unsafe {
        Cell {
            ch,
            fg: COLOR,
            bg: color::BLACK,
        }
    };
    unsafe {
        cell_set(y * MAX_COLS + x, cell);
    }
    if !graphics::cmd_active() {
        draw_cell(x, y, cell);
    }
    shadow_put(x, y, cell);
}

fn scroll_line(slot: usize) -> *mut Cell {
    unsafe { (SCROLL_ADDR as *mut Cell).add(slot * MAX_COLS) }
}

fn scroll_slot(oldest_index: usize) -> usize {
    unsafe {
        let start = if SCROLL_COUNT == SCROLL_LINES {
            SCROLL_HEAD
        } else {
            0
        };
        (start + oldest_index) % SCROLL_LINES
    }
}

fn push_scroll_line() {
    unsafe {
        let dst = scroll_line(SCROLL_HEAD);
        let blank = blank_cell();
        let mut x = 0;
        while x < MAX_COLS {
            let cell = if x < COLS { cell_get(x) } else { blank };
            *dst.add(x) = cell;
            x += 1;
        }
        SCROLL_HEAD = (SCROLL_HEAD + 1) % SCROLL_LINES;
        if SCROLL_COUNT < SCROLL_LINES {
            SCROLL_COUNT += 1;
        }
    }
}

fn viewed_cell(x: usize, screen_y: usize) -> Cell {
    unsafe {
        let index = SCROLL_COUNT - VIEW + screen_y;
        if index < SCROLL_COUNT {
            *scroll_line(scroll_slot(index)).add(x)
        } else {
            let live = index - SCROLL_COUNT;
            if live < ROWS && x < COLS {
                cell_get(live * MAX_COLS + x)
            } else {
                blank_cell()
            }
        }
    }
}

/// Paint the screen for the current scroll position. `VIEW == 0` is the live
/// prompt; a larger value shows lines that have already scrolled off.
fn paint_view() {
    if !graphics_active() {
        return;
    }
    graphics::clear(0);
    unsafe {
        let mut y = 0;
        while y < ROWS {
            let mut x = 0;
            while x < COLS {
                draw_cell_impl(x, y, viewed_cell(x, y), true);
                x += 1;
            }
            y += 1;
        }
        CURSOR_DRAWN = false;
        let screen_y = CURSOR_Y + VIEW;
        if screen_y < ROWS {
            draw_caret_bar(CURSOR_X, screen_y);
            CURSOR_DRAWN = VIEW == 0;
            DRAWN_X = CURSOR_X;
            DRAWN_Y = CURSOR_Y;
        }
    }
}

fn ensure_live_view() {
    unsafe {
        if VIEW != 0 {
            VIEW = 0;
            paint_view();
        }
    }
}

/// Mouse wheel. Scrolling up moves into older lines.
pub fn scroll_by_wheel(delta: i32) {
    if !graphics_active() || graphics::cmd_active() || delta == 0 {
        return;
    }
    unsafe {
        let max = SCROLL_COUNT as i32;
        // The wheel delta comes in backwards relative to the console: negate
        // it so wheel-up shows older lines and wheel-down returns to the prompt.
        let next = (VIEW as i32 - delta.saturating_mul(3)).clamp(0, max) as usize;
        if next == VIEW {
            return;
        }
        VIEW = next;
    }
    paint_view();
    graphics::present_dirty();
}

pub fn columns() -> usize {
    unsafe { COLS }
}

pub fn rows() -> usize {
    unsafe { ROWS }
}

pub fn cursor_pos() -> (usize, usize) {
    unsafe { (CURSOR_X, CURSOR_Y) }
}

/// Move the text caret. The next `flush` draws the bar there.
pub fn set_cursor(x: usize, y: usize) {
    unsafe {
        if COLS == 0 || ROWS == 0 {
            return;
        }
        CURSOR_X = x.min(COLS - 1);
        CURSOR_Y = y.min(ROWS - 1);
    }
}

fn scroll() {
    unsafe {
        push_scroll_line();
        let mut y = 1;
        while y < ROWS {
            let mut x = 0;
            while x < COLS {
                cell_set((y - 1) * MAX_COLS + x, cell_get(y * MAX_COLS + x));
                x += 1;
            }
            y += 1;
        }
        let blank = blank_cell();
        let mut x = 0;
        while x < COLS {
            cell_set((ROWS - 1) * MAX_COLS + x, blank);
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
    ensure_live_view();
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
    unsafe {
        VIEW = 0;
    }
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

/// Set the foreground colour. `value` is packed `0x00RRGGBB`.
pub fn set_color(value: u32) {
    if graphics_active() {
        unsafe {
            COLOR = value;
        }
    } else {
        unsafe {
            vga_set_color(vga_attr(value, color::BLACK));
        }
    }
}

pub fn get_color() -> u32 {
    if graphics_active() {
        unsafe { COLOR }
    } else {
        unsafe { vga_index_rgb(vga_get_color() & 0x0F) }
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
    if unsafe { VIEW } != 0 {
        paint_view();
        graphics::present_dirty();
        return;
    }
    unsafe {
        if CURSOR_DRAWN {
            // The bar sits on the right edge of the previous cell. Repaint both
            // or a ghost caret is left behind, especially when moving right.
            if DRAWN_X > 0 {
                draw_cell(
                    DRAWN_X - 1,
                    DRAWN_Y,
                    cell_get(DRAWN_Y * MAX_COLS + DRAWN_X - 1),
                );
            }
            draw_cell(DRAWN_X, DRAWN_Y, cell_get(DRAWN_Y * MAX_COLS + DRAWN_X));
        }
    }
    draw_cursor();
    graphics::present_dirty();
}

pub fn enter_graphics_command() -> bool {
    if !graphics_active() {
        return false;
    }
    unsafe {
        VIEW = 0;
    }
    crate::mouse::drop_pending_wheel();
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
