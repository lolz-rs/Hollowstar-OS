//! Lightweight software drawing for Hollowstar commands.
//!
//! Everything renders into the kernel-owned back buffer. Call `present()` to
//! copy the finished frame to the screen at the next vertical retrace.
//!
//! The back buffer is 32-bit true colour (16.7 million colours). Besides solid
//! fills this module provides gradients, alpha blending, rounded rectangles,
//! triangles, and HSV helpers.

use core::arch::asm;
use core::ptr;

use crate::{syscall5, GraphicsInfo, SYS_GFX_ENTER, SYS_GFX_INFO, SYS_GFX_LEAVE, SYS_GFX_PRESENT,
            SYS_GFX_PRESENT_RECT, SYS_GFX_VSYNC};

static mut INFO: GraphicsInfo = GraphicsInfo::zeroed();
static mut READY: bool = false;
static mut R_SHIFT: u32 = 16;
static mut G_SHIFT: u32 = 8;
static mut B_SHIFT: u32 = 0;

static mut DIRTY: bool = false;
static mut DIRTY_X0: i32 = 0;
static mut DIRTY_Y0: i32 = 0;
static mut DIRTY_X1: i32 = 0;
static mut DIRTY_Y1: i32 = 0;

fn mask_shift(mask: u32) -> u32 {
    if mask == 0 {
        0
    } else {
        mask.trailing_zeros()
    }
}

pub fn init() -> bool {
    unsafe {
        let mut info = GraphicsInfo::zeroed();
        let ok = syscall5(
            SYS_GFX_INFO,
            &mut info as *mut GraphicsInfo as u64,
            0,
            0,
            0,
            0,
        ) != 0;
        if ok
            && info.back_buffer != 0
            && info.width > 0
            && info.height > 0
            && info.pitch >= info.width.saturating_mul(4)
            && info.font_addr != 0
        {
            R_SHIFT = mask_shift(info.red_mask);
            G_SHIFT = mask_shift(info.green_mask);
            B_SHIFT = mask_shift(info.blue_mask);
            INFO = info;
            READY = true;
            DIRTY = false;
            true
        } else {
            READY = false;
            false
        }
    }
}

#[inline]
pub fn ready() -> bool {
    unsafe { READY }
}

pub fn info() -> &'static GraphicsInfo {
    unsafe { &*ptr::addr_of!(INFO) }
}

#[inline]
pub fn width() -> i32 {
    info().width as i32
}

#[inline]
pub fn height() -> i32 {
    info().height as i32
}

pub fn enter() -> bool {
    unsafe {
        DIRTY = false;
        syscall5(SYS_GFX_ENTER, 0, 0, 0, 0, 0) != 0
    }
}

pub fn leave() {
    unsafe {
        DIRTY = false;
        syscall5(SYS_GFX_LEAVE, 0, 0, 0, 0, 0);
    }
}

fn mark_dirty(x: i32, y: i32, w: i32, h: i32) {
    if !ready() || w <= 0 || h <= 0 {
        return;
    }
    let info = info();
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x.saturating_add(w)).min(info.width as i32);
    let y1 = (y.saturating_add(h)).min(info.height as i32);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    unsafe {
        if !DIRTY {
            DIRTY = true;
            DIRTY_X0 = x0;
            DIRTY_Y0 = y0;
            DIRTY_X1 = x1;
            DIRTY_Y1 = y1;
        } else {
            DIRTY_X0 = DIRTY_X0.min(x0);
            DIRTY_Y0 = DIRTY_Y0.min(y0);
            DIRTY_X1 = DIRTY_X1.max(x1);
            DIRTY_Y1 = DIRTY_Y1.max(y1);
        }
    }
}

/// Copy the changed rectangle to the screen. Does nothing when no drawing
/// happened since the previous present.
pub fn present() {
    if !ready() {
        return;
    }
    unsafe {
        if !DIRTY {
            return;
        }
        syscall5(
            SYS_GFX_PRESENT_RECT,
            DIRTY_X0 as u64,
            DIRTY_Y0 as u64,
            (DIRTY_X1 - DIRTY_X0) as u64,
            (DIRTY_Y1 - DIRTY_Y0) as u64,
            0,
        );
        DIRTY = false;
    }
}

/// Copy the whole frame even if nothing is marked dirty.
pub fn present_full() {
    unsafe {
        DIRTY = false;
        syscall5(SYS_GFX_PRESENT, 0, 0, 0, 0, 0);
    }
}

pub fn vsync() {
    unsafe {
        syscall5(SYS_GFX_VSYNC, 0, 0, 0, 0, 0);
    }
}

pub fn clear(color: u32) {
    if !ready() {
        return;
    }
    let info = info();
    fill_rect(0, 0, info.width as i32, info.height as i32, color);
}

#[inline]
pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    unsafe {
        ((r as u32) << R_SHIFT) | ((g as u32) << G_SHIFT) | ((b as u32) << B_SHIFT)
    }
}

#[inline]
fn channels(color: u32) -> (u32, u32, u32) {
    unsafe {
        (
            (color >> R_SHIFT) & 0xFF,
            (color >> G_SHIFT) & 0xFF,
            (color >> B_SHIFT) & 0xFF,
        )
    }
}

#[inline]
fn lerp_channel(a: u32, b: u32, t: u32) -> u32 {
    if t == 0 {
        a
    } else if t >= 255 {
        b
    } else {
        (a * (255 - t) + b * t + 127) / 255
    }
}

/// Interpolate between two true-colour values. `t` runs from 0 to 255.
#[inline]
pub fn lerp(a: u32, b: u32, t: u32) -> u32 {
    let (ar, ag, ab) = channels(a);
    let (br, bg, bb) = channels(b);
    rgb(
        lerp_channel(ar, br, t) as u8,
        lerp_channel(ag, bg, t) as u8,
        lerp_channel(ab, bb, t) as u8,
    )
}

#[inline]
pub fn pixel(x: i32, y: i32, color: u32) {
    if !ready() {
        return;
    }
    let info = info();
    if x < 0 || y < 0 || x >= info.width as i32 || y >= info.height as i32 {
        return;
    }
    let stride = (info.pitch / 4) as usize;
    unsafe {
        ptr::write(
            (info.back_buffer as *mut u32).add(y as usize * stride + x as usize),
            color,
        );
    }
    mark_dirty(x, y, 1, 1);
}

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

pub fn fill_rect(x: i32, y: i32, w: i32, h: i32, color: u32) {
    if !ready() || w <= 0 || h <= 0 {
        return;
    }
    let info = info();
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x.saturating_add(w)).min(info.width as i32);
    let y1 = (y.saturating_add(h)).min(info.height as i32);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let count = (x1 - x0) as usize;
    let stride = (info.pitch / 4) as usize;
    let base = info.back_buffer as *mut u32;
    mark_dirty(x0, y0, x1 - x0, y1 - y0);
    if x0 == 0 && x1 == info.width as i32 {
        let dst = unsafe { base.add(y0 as usize * stride) };
        fill_span(dst, (y1 - y0) as usize * stride, color);
    } else {
        let mut row = y0;
        while row < y1 {
            let dst = unsafe { base.add(row as usize * stride + x0 as usize) };
            fill_span(dst, count, color);
            row += 1;
        }
    }
}

/// Blend one pixel with the existing framebuffer contents.
/// `alpha` is 0 (transparent) to 255 (opaque).
#[inline]
pub fn blend_pixel(x: i32, y: i32, color: u32, alpha: u32) {
    if alpha == 0 {
        return;
    }
    if alpha >= 255 {
        pixel(x, y, color);
        return;
    }
    if !ready() {
        return;
    }
    let info = info();
    if x < 0 || y < 0 || x >= info.width as i32 || y >= info.height as i32 {
        return;
    }
    let stride = (info.pitch / 4) as usize;
    unsafe {
        let dst = (info.back_buffer as *mut u32).add(y as usize * stride + x as usize);
        let old = *dst;
        let (sr, sg, sb) = channels(color);
        let (dr, dg, db) = channels(old);
        let inv = 255 - alpha;
        *dst = rgb(
            ((sr * alpha + dr * inv + 127) / 255) as u8,
            ((sg * alpha + dg * inv + 127) / 255) as u8,
            ((sb * alpha + db * inv + 127) / 255) as u8,
        );
    }
    mark_dirty(x, y, 1, 1);
}

/// Alpha-blend a rectangle over whatever is already in the back buffer.
pub fn fill_rect_alpha(x: i32, y: i32, w: i32, h: i32, color: u32, alpha: u32) {
    if alpha == 0 || w <= 0 || h <= 0 {
        return;
    }
    if alpha >= 255 {
        fill_rect(x, y, w, h, color);
        return;
    }
    if !ready() {
        return;
    }
    let info = info();
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x.saturating_add(w)).min(info.width as i32);
    let y1 = (y.saturating_add(h)).min(info.height as i32);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let stride = (info.pitch / 4) as usize;
    let base = info.back_buffer as *mut u32;
    let (sr, sg, sb) = channels(color);
    let inv = 255 - alpha;
    let mut row = y0;
    while row < y1 {
        let mut col = x0;
        while col < x1 {
            unsafe {
                let dst = base.add(row as usize * stride + col as usize);
                let old = *dst;
                let (dr, dg, db) = channels(old);
                *dst = rgb(
                    ((sr * alpha + dr * inv + 127) / 255) as u8,
                    ((sg * alpha + dg * inv + 127) / 255) as u8,
                    ((sb * alpha + db * inv + 127) / 255) as u8,
                );
            }
            col += 1;
        }
        row += 1;
    }
    mark_dirty(x0, y0, x1 - x0, y1 - y0);
}

/// Fill a rectangle with a vertical gradient from `top` to `bottom`.
pub fn vertical_gradient(x: i32, y: i32, w: i32, h: i32, top: u32, bottom: u32) {
    if w <= 0 || h <= 0 {
        return;
    }
    if h == 1 {
        fill_rect(x, y, w, 1, top);
        return;
    }
    let mut row = 0;
    while row < h {
        let t = (row as u32 * 255) / (h as u32 - 1);
        fill_rect(x, y + row, w, 1, lerp(top, bottom, t));
        row += 1;
    }
}

/// Fill a rectangle with a horizontal gradient from `left` to `right`.
pub fn horizontal_gradient(x: i32, y: i32, w: i32, h: i32, left: u32, right: u32) {
    if !ready() || w <= 0 || h <= 0 {
        return;
    }
    let info = info();
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x.saturating_add(w)).min(info.width as i32);
    let y1 = (y.saturating_add(h)).min(info.height as i32);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let stride = (info.pitch / 4) as usize;
    let base = info.back_buffer as *mut u32;
    let mut row = y0;
    while row < y1 {
        let mut col = x0;
        while col < x1 {
            let t = if w == 1 {
                0
            } else {
                ((col - x) as u32 * 255) / (w as u32 - 1)
            };
            unsafe {
                ptr::write(
                    base.add(row as usize * stride + col as usize),
                    lerp(left, right, t.min(255)),
                );
            }
            col += 1;
        }
        row += 1;
    }
    mark_dirty(x0, y0, x1 - x0, y1 - y0);
}

/// Rectangle outline drawn with `thickness` pixels on each edge.
pub fn rect_outline(x: i32, y: i32, w: i32, h: i32, thickness: i32, color: u32) {
    if w <= 0 || h <= 0 || thickness <= 0 {
        return;
    }
    let t = thickness.min(w).min(h);
    fill_rect(x, y, w, t, color);
    fill_rect(x, y + h - t, w, t, color);
    fill_rect(x, y + t, t, h - 2 * t, color);
    fill_rect(x + w - t, y + t, t, h - 2 * t, color);
}

/// Filled rectangle with rounded corners.
pub fn fill_round_rect(x: i32, y: i32, w: i32, h: i32, radius: i32, color: u32) {
    if w <= 0 || h <= 0 {
        return;
    }
    let r = radius.max(0).min(w / 2).min(h / 2);
    if r == 0 {
        fill_rect(x, y, w, h, color);
        return;
    }
    fill_rect(x + r, y, w - 2 * r, h, color);
    fill_rect(x, y + r, r, h - 2 * r, color);
    fill_rect(x + w - r, y + r, r, h - 2 * r, color);
    fill_circle(x + r, y + r, r, color);
    fill_circle(x + w - r - 1, y + r, r, color);
    fill_circle(x + r, y + h - r - 1, r, color);
    fill_circle(x + w - r - 1, y + h - r - 1, r, color);
}

pub fn line(mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: u32) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        pixel(x0, y0, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = err * 2;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

pub fn circle(cx: i32, cy: i32, r: i32, color: u32) {
    if r <= 0 {
        pixel(cx, cy, color);
        return;
    }
    let mut x = r;
    let mut y = 0;
    let mut err = 1 - r;
    while x >= y {
        pixel(cx + x, cy + y, color);
        pixel(cx + y, cy + x, color);
        pixel(cx - y, cy + x, color);
        pixel(cx - x, cy + y, color);
        pixel(cx - x, cy - y, color);
        pixel(cx - y, cy - x, color);
        pixel(cx + y, cy - x, color);
        pixel(cx + x, cy - y, color);
        y += 1;
        if err < 0 {
            err += 2 * y + 1;
        } else {
            x -= 1;
            err += 2 * (y - x) + 1;
        }
    }
}

pub fn fill_circle(cx: i32, cy: i32, r: i32, color: u32) {
    if r <= 0 {
        pixel(cx, cy, color);
        return;
    }
    let r2 = r * r;
    let mut dy = -r;
    while dy <= r {
        let mut dx = 0;
        while dx * dx + dy * dy <= r2 {
            dx += 1;
        }
        if dx > 0 {
            fill_rect(cx - dx + 1, cy + dy, 2 * dx - 1, 1, color);
        }
        dy += 1;
    }
}

/// Fill a convex or concave polygon using a scanline crossing table.
pub fn fill_polygon(points: &[(i32, i32)], color: u32) {
    if !ready() || points.len() < 3 {
        return;
    }
    let info = info();
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    for p in points {
        min_y = min_y.min(p.1);
        max_y = max_y.max(p.1);
    }
    min_y = min_y.max(0);
    max_y = max_y.min(info.height as i32 - 1);
    if max_y < min_y {
        return;
    }

    let mut xs = [0i32; 16];
    let mut y = min_y;
    while y <= max_y {
        let mut count = 0usize;
        let mut i = 0usize;
        while i < points.len() {
            let (x0, y0) = points[i];
            let (x1, y1) = points[(i + 1) % points.len()];
            if (y0 <= y && y < y1) || (y1 <= y && y < y0) {
                let dy = y1 - y0;
                if dy != 0 {
                    let x = x0 + (y - y0) * (x1 - x0) / dy;
                    if count < xs.len() {
                        xs[count] = x;
                        count += 1;
                    }
                }
            }
            i += 1;
        }
        let mut a = 0usize;
        while a < count {
            let mut b = a + 1;
            while b < count {
                if xs[b] < xs[a] {
                    xs.swap(a, b);
                }
                b += 1;
            }
            a += 1;
        }
        let mut k = 0usize;
        while k + 1 < count {
            let x0 = xs[k].max(0);
            let x1 = xs[k + 1].min(info.width as i32 - 1);
            if x1 >= x0 {
                fill_rect(x0, y, x1 - x0 + 1, 1, color);
            }
            k += 2;
        }
        y += 1;
    }
}

pub fn fill_triangle(p0: (i32, i32), p1: (i32, i32), p2: (i32, i32), color: u32) {
    fill_polygon(&[p0, p1, p2], color);
}

pub fn polygon_outline(points: &[(i32, i32)], color: u32) {
    if points.len() < 2 {
        return;
    }
    let mut i = 0usize;
    while i < points.len() {
        let (x0, y0) = points[i];
        let (x1, y1) = points[(i + 1) % points.len()];
        line(x0, y0, x1, y1, color);
        i += 1;
    }
}

pub fn text(x: i32, y: i32, text: &str, color: u32, scale: u32) {
    if !ready() {
        return;
    }
    let info = info();
    let fw = info.font_width.max(1);
    let fh = info.font_height.max(1);
    let scale = scale.max(1);
    let font = info.font_addr as *const u8;
    if font.is_null() {
        return;
    }
    let mut cx = x;
    let mut cy = y;
    for &b in text.as_bytes() {
        if b == b'\n' {
            cx = x;
            cy += (fh * scale) as i32;
            continue;
        }
        if b != b' ' {
            unsafe {
                let glyph = font.add(b as usize * fh as usize);
                let mut row = 0;
                while row < fh {
                    let bits = *glyph.add(row as usize);
                    let cols = fw.min(8);
                    let mut col = 0;
                    while col < cols {
                        if (bits & (0x80 >> col)) == 0 {
                            col += 1;
                            continue;
                        }
                        let start = col;
                        while col < cols && (bits & (0x80 >> col)) != 0 {
                            col += 1;
                        }
                        fill_rect(
                            cx + (start * scale) as i32,
                            cy + (row * scale) as i32,
                            ((col - start) * scale) as i32,
                            scale as i32,
                            color,
                        );
                    }
                    row += 1;
                }
            }
        }
        cx += (fw * scale) as i32;
    }
}

pub fn text_width(text: &str, scale: u32) -> i32 {
    let info = info();
    text.len() as i32 * info.font_width.max(1) as i32 * scale.max(1) as i32
}

fn hue_rgb(h: u8) -> (u8, u8, u8) {
    let h = h as u32;
    let region = h / 43;
    let f = (h % 43) * 6;
    match region {
        0 => (255, f as u8, 0),
        1 => ((255 - f) as u8, 255, 0),
        2 => (0, 255, f as u8),
        3 => (0, (255 - f) as u8, 255),
        4 => (f as u8, 0, 255),
        _ => (255, 0, (255 - f) as u8),
    }
}

pub fn hue(h: u8) -> u32 {
    let (r, g, b) = hue_rgb(h);
    rgb(r, g, b)
}

pub fn hue_bright(h: u8, brightness: u8) -> u32 {
    let (r, g, b) = hue_rgb(h);
    let f = brightness as u32;
    rgb(
        (r as u32 * f / 255) as u8,
        (g as u32 * f / 255) as u8,
        (b as u32 * f / 255) as u8,
    )
}

/// HSV to true colour. Hue is 0..255 around the wheel, saturation and value
/// are 0..255.
pub fn hsv(h: u8, s: u8, v: u8) -> u32 {
    if s == 0 {
        return rgb(v, v, v);
    }
    let region = (h as u32 * 6) / 256;
    let f = (h as u32 * 6) % 256;
    let p = (v as u32 * (255 - s as u32)) / 255;
    let q = (v as u32 * (255 - (s as u32 * f) / 255)) / 255;
    let t = (v as u32 * (255 - (s as u32 * (255 - f)) / 255)) / 255;
    let v = v as u32;
    match region {
        0 => rgb(v as u8, t as u8, p as u8),
        1 => rgb(q as u8, v as u8, p as u8),
        2 => rgb(p as u8, v as u8, t as u8),
        3 => rgb(p as u8, q as u8, v as u8),
        4 => rgb(t as u8, p as u8, v as u8),
        _ => rgb(v as u8, p as u8, q as u8),
    }
}

pub fn scale_color(color: u32, factor: u8) -> u32 {
    let (r, g, b) = channels(color);
    let f = factor as u32;
    rgb(
        (r * f / 255) as u8,
        (g * f / 255) as u8,
        (b * f / 255) as u8,
    )
}

// Quarter-wave sine table, 0..90 degrees in 5.625-degree steps, scaled by 1024.
const SIN_Q: [i32; 17] = [
    0, 100, 200, 297, 392, 483, 569, 650, 724, 792, 851, 903, 946, 980, 1004, 1019, 1024,
];

/// Sine of a 0..255 angle (256 = full circle), scaled by 1024.
pub fn sin(angle: i32) -> i32 {
    let a = angle & 255;
    let (idx, sign) = if a < 64 {
        (a, 1)
    } else if a < 128 {
        (128 - a, 1)
    } else if a < 192 {
        (a - 128, -1)
    } else {
        (256 - a, -1)
    };
    let q = (idx / 4) as usize;
    let r = (idx % 4) as i32;
    let v = if q >= SIN_Q.len() - 1 {
        SIN_Q[SIN_Q.len() - 1]
    } else {
        SIN_Q[q] + (SIN_Q[q + 1] - SIN_Q[q]) * r / 4
    };
    v * sign
}

pub fn cos(angle: i32) -> i32 {
    sin(angle + 64)
}
