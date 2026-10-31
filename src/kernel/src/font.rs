//! Built-in 14×24 monospace font.
//!
//! `tools/make_font.py` writes `font.bin`. Two bytes per row, leftmost pixel
//! in the high bit, 256 glyphs. The BIOS 8×16 font is only used by VGA text
//! mode; graphics mode draws this instead.

pub const WIDTH: u32 = 14;
pub const HEIGHT: u32 = 24;

static DATA: [u8; 256 * 24 * 2] = *include_bytes!("font.bin");

pub fn addr() -> u64 {
    DATA.as_ptr() as u64
}
