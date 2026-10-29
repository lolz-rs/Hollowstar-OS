#![no_std]
#![no_main]

//! Mouse demo: button indicators, position readouts, and a drawing canvas.
//! Left draws, right clears, middle cycles the brush, ESC/Q exits.

use hollowstar as hs;
use hs::gfx;
use hs::{
    MouseState, MOUSE_BUTTON_BACK, MOUSE_BUTTON_FORWARD, MOUSE_BUTTON_LEFT, MOUSE_BUTTON_MIDDLE,
    MOUSE_BUTTON_RIGHT, MOUSE_FLAG_AVAILABLE, MOUSE_FLAG_OVERFLOW, MOUSE_FLAG_WHEEL,
};

const PALETTE: [(u8, u8, u8); 8] = [
    (245, 248, 255),
    (235, 70, 75),
    (250, 150, 55),
    (245, 220, 80),
    (95, 215, 120),
    (75, 210, 220),
    (95, 145, 250),
    (205, 115, 235),
];

struct Layout {
    top_h: i32,
    panel_x: i32,
    panel_y: i32,
    panel_w: i32,
    panel_h: i32,
    buttons_y: i32,
    button_row_h: i32,
    button_box_h: i32,
    info_y: i32,
    palette_x: i32,
    palette_y: i32,
    palette_label_y: i32,
    swatch: i32,
    swatch_gap: i32,
    canvas_x: i32,
    canvas_y: i32,
    canvas_w: i32,
    canvas_h: i32,
}

impl Layout {
    fn new() -> Self {
        let w = gfx::width();
        let h = gfx::height();
        let top_h = 44;
        let margin = 10;
        let panel_w = (w / 4).clamp(170, 240);
        let panel_x = margin;
        let panel_y = top_h + margin;
        let panel_h = (h - panel_y - margin).max(220);

        let button_row_h = 30;
        let button_box_h = 24;
        let buttons_y = panel_y + 30;
        let info_y = buttons_y + 5 * button_row_h + 6;

        let compact = panel_h < 500;
        let swatch = if compact { 22 } else { 30 };
        let swatch_gap = if compact { 6 } else { 8 };
        let grid_h = 2 * swatch + swatch_gap;
        let grid_w = 4 * swatch + 3 * swatch_gap;
        let palette_y = panel_y + panel_h - 8 - grid_h;
        let palette_label_y = palette_y - 22;
        let palette_x = panel_x + (panel_w - grid_w) / 2;

        let canvas_x = panel_x + panel_w + margin;
        let canvas_y = panel_y;
        let canvas_w = (w - canvas_x - margin).max(120);
        let canvas_h = (h - canvas_y - margin).max(120);

        Self {
            top_h,
            panel_x,
            panel_y,
            panel_w,
            panel_h,
            buttons_y,
            button_row_h,
            button_box_h,
            info_y,
            palette_x,
            palette_y,
            palette_label_y,
            swatch,
            swatch_gap,
            canvas_x,
            canvas_y,
            canvas_w,
            canvas_h,
        }
    }
}

fn bg() -> u32 {
    gfx::rgb(10, 12, 18)
}

fn top_bg() -> u32 {
    gfx::rgb(16, 19, 28)
}

fn panel_bg() -> u32 {
    gfx::rgb(20, 23, 32)
}

fn panel_border() -> u32 {
    gfx::rgb(48, 56, 76)
}

fn canvas_bg() -> u32 {
    gfx::rgb(16, 18, 26)
}

fn canvas_grid() -> u32 {
    gfx::rgb(28, 32, 43)
}

fn accent() -> u32 {
    gfx::rgb(90, 175, 255)
}

fn white() -> u32 {
    gfx::rgb(245, 248, 255)
}

fn dim() -> u32 {
    gfx::rgb(135, 148, 170)
}

fn palette_color(index: usize) -> u32 {
    let (r, g, b) = PALETTE[index & 7];
    gfx::rgb(r, g, b)
}

fn fill_border(x: i32, y: i32, w: i32, h: i32, t: i32, color: u32) {
    if w <= 0 || h <= 0 || t <= 0 {
        return;
    }
    gfx::fill_rect(x, y, w, t, color);
    gfx::fill_rect(x, y + h - t, w, t, color);
    gfx::fill_rect(x, y, t, h, color);
    gfx::fill_rect(x + w - t, y, t, h, color);
}

fn panel_rect(x: i32, y: i32, w: i32, h: i32) {
    gfx::fill_rect(x, y, w, h, panel_bg());
    fill_border(x, y, w, h, 2, panel_border());
}

fn button_mask(index: usize) -> u32 {
    match index {
        0 => MOUSE_BUTTON_LEFT,
        1 => MOUSE_BUTTON_MIDDLE,
        2 => MOUSE_BUTTON_RIGHT,
        3 => MOUSE_BUTTON_BACK,
        _ => MOUSE_BUTTON_FORWARD,
    }
}

fn button_name(index: usize) -> &'static str {
    match index {
        0 => "LEFT",
        1 => "MIDDLE",
        2 => "RIGHT",
        3 => "BACK",
        _ => "FORWARD",
    }
}

fn button_color(index: usize) -> u32 {
    match index {
        0 => gfx::rgb(95, 220, 135),
        1 => gfx::rgb(255, 195, 85),
        2 => gfx::rgb(255, 105, 115),
        3 => gfx::rgb(120, 165, 255),
        _ => gfx::rgb(195, 135, 255),
    }
}

fn u32_str(mut value: u32, buf: &mut [u8; 16]) -> &str {
    let mut i = buf.len();
    if value == 0 {
        i -= 1;
        buf[i] = b'0';
    }
    while value > 0 {
        i -= 1;
        buf[i] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("?")
}

fn i32_str(value: i32, buf: &mut [u8; 16]) -> &str {
    let negative = value < 0;
    let mut magnitude = value.unsigned_abs();
    let mut i = buf.len();
    if magnitude == 0 {
        i -= 1;
        buf[i] = b'0';
    }
    while magnitude > 0 {
        i -= 1;
        buf[i] = b'0' + (magnitude % 10) as u8;
        magnitude /= 10;
    }
    if negative {
        i -= 1;
        buf[i] = b'-';
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("?")
}

fn draw_canvas_base(l: &Layout) {
    gfx::fill_rect(l.canvas_x, l.canvas_y, l.canvas_w, l.canvas_h, canvas_bg());
    let grid = canvas_grid();
    let mut x = l.canvas_x + 32;
    while x < l.canvas_x + l.canvas_w {
        gfx::fill_rect(x, l.canvas_y, 1, l.canvas_h, grid);
        x += 32;
    }
    let mut y = l.canvas_y + 32;
    while y < l.canvas_y + l.canvas_h {
        gfx::fill_rect(l.canvas_x, y, l.canvas_w, 1, grid);
        y += 32;
    }
    fill_border(
        l.canvas_x - 2,
        l.canvas_y - 2,
        l.canvas_w + 4,
        l.canvas_h + 4,
        2,
        accent(),
    );
}

fn draw_static(l: &Layout) {
    let w = gfx::width();
    gfx::clear(bg());

    gfx::fill_rect(0, 0, w, l.top_h, top_bg());
    gfx::fill_rect(0, l.top_h - 2, w, 2, accent());
    gfx::text(14, 6, "MOUSE DEMO", white(), 2);

    let hint = "L DRAW   R CLEAR   M BRUSH   ESC/Q QUIT";
    let hx = (w - gfx::text_width(hint, 1) - 14).max(14);
    gfx::text(hx, 14, hint, dim(), 1);

    panel_rect(l.panel_x, l.panel_y, l.panel_w, l.panel_h);
    gfx::text(l.panel_x + 10, l.panel_y + 8, "BUTTONS", accent(), 1);
    gfx::text(l.panel_x + 10, l.info_y, "MOUSE STATE", accent(), 1);
    gfx::text(
        l.panel_x + 10,
        l.palette_label_y,
        "BRUSH COLOR",
        accent(),
        1,
    );

    draw_canvas_base(l);
}

fn draw_buttons(l: &Layout, state: &MouseState) {
    let x = l.panel_x + 10;
    let w = l.panel_w - 20;
    let mut i = 0;
    while i < 5 {
        let y = l.buttons_y + i as i32 * l.button_row_h;
        let color = button_color(i);
        let pressed = (state.buttons & button_mask(i)) != 0;
        let fill = if pressed { color } else { gfx::rgb(28, 32, 43) };
        let border = if pressed {
            white()
        } else {
            gfx::scale_color(color, 80)
        };
        gfx::fill_rect(x, y, w, l.button_box_h, fill);
        fill_border(x, y, w, l.button_box_h, 2, border);

        let text_color = if pressed {
            gfx::rgb(12, 14, 20)
        } else {
            gfx::scale_color(color, 165)
        };
        gfx::text(x + 8, y + 4, button_name(i), text_color, 1);

        let dot = if pressed {
            white()
        } else {
            gfx::rgb(62, 68, 84)
        };
        gfx::fill_circle(x + w - 14, y + l.button_box_h / 2, 4, dot);
        i += 1;
    }
}

fn info_line(l: &Layout, index: i32, label: &str, value: &str) {
    let y = l.info_y + 18 + index * 16;
    gfx::fill_rect(l.panel_x + 8, y, l.panel_w - 16, 16, panel_bg());
    gfx::text(l.panel_x + 12, y, label, dim(), 1);
    gfx::text(l.panel_x + 92, y, value, white(), 1);
}

fn draw_info(l: &Layout, state: &MouseState, brush: i32) {
    let mut buf = [0u8; 16];

    info_line(l, 0, "X", i32_str(state.x, &mut buf));
    info_line(l, 1, "Y", i32_str(state.y, &mut buf));
    info_line(l, 2, "DX", i32_str(state.dx, &mut buf));
    info_line(l, 3, "DY", i32_str(state.dy, &mut buf));
    info_line(l, 4, "WHEEL", i32_str(state.wheel, &mut buf));
    info_line(l, 5, "PACKETS", u32_str(state.packets, &mut buf));
    info_line(l, 6, "BRUSH", i32_str(brush, &mut buf));

    let status = if (state.flags & MOUSE_FLAG_AVAILABLE) == 0 {
        "NO MOUSE"
    } else if (state.flags & MOUSE_FLAG_OVERFLOW) != 0 {
        "OVERFLOW"
    } else if (state.flags & MOUSE_FLAG_WHEEL) != 0 {
        "WHEEL"
    } else {
        "READY"
    };
    info_line(l, 7, "STATUS", status);
}

fn swatch_rect(l: &Layout, index: usize) -> (i32, i32, i32, i32) {
    let col = (index % 4) as i32;
    let row = (index / 4) as i32;
    let step = l.swatch + l.swatch_gap;
    (
        l.palette_x + col * step,
        l.palette_y + row * step,
        l.swatch,
        l.swatch,
    )
}

fn draw_palette(l: &Layout, selected: usize) {
    let mut i = 0;
    while i < 8 {
        let (x, y, w, h) = swatch_rect(l, i);
        gfx::fill_rect(x, y, w, h, palette_color(i));
        if i == selected {
            fill_border(x, y, w, h, 3, white());
        } else {
            fill_border(x, y, w, h, 2, gfx::rgb(56, 62, 78));
        }
        i += 1;
    }
}

fn swatch_at(l: &Layout, x: i32, y: i32) -> Option<usize> {
    let mut i = 0;
    while i < 8 {
        let (sx, sy, sw, sh) = swatch_rect(l, i);
        if x >= sx && x < sx + sw && y >= sy && y < sy + sh {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn in_canvas(l: &Layout, x: i32, y: i32) -> bool {
    x >= l.canvas_x
        && x < l.canvas_x + l.canvas_w
        && y >= l.canvas_y
        && y < l.canvas_y + l.canvas_h
}

fn clamp_to_canvas(l: &Layout, x: i32, y: i32, radius: i32) -> (i32, i32) {
    let min_x = l.canvas_x + radius;
    let max_x = l.canvas_x + l.canvas_w - 1 - radius;
    let min_y = l.canvas_y + radius;
    let max_y = l.canvas_y + l.canvas_h - 1 - radius;
    (x.clamp(min_x, max_x), y.clamp(min_y, max_y))
}

fn stroke(x0: i32, y0: i32, x1: i32, y1: i32, color: u32, radius: i32) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0;
    let mut y = y0;
    loop {
        gfx::fill_circle(x, y, radius, color);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = err * 2;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

#[no_mangle]
pub extern "C" fn _start(args_ptr: *const u8, args_len: usize) -> ! {
    let args = hs::args_str(args_ptr, args_len).trim();
    if args == "help" || args == "-h" || args == "--help" {
        hs::write("usage: mousedemo\n");
        hs::write("  Left draws, right clears the canvas, middle cycles the brush size.\n");
        hs::write("  ESC or Q exits. The cursor is drawn by the kernel mouse driver.\n");
        hs::exit(0);
    }

    if !gfx::init() {
        hs::write_color(0x0C, "mousedemo: no linear framebuffer available\n");
        hs::exit(1);
    }
    if !gfx::enter() {
        hs::write_color(0x0C, "mousedemo: failed to enter graphics mode\n");
        hs::exit(1);
    }

    let l = Layout::new();
    draw_static(&l);

    let mut state = hs::mouse_state();
    let mut prev = state;
    let mut color_index = 0usize;
    let mut brush = 1i32;
    let mut last: Option<(i32, i32)> = None;
    let mut frames = 0u64;

    draw_buttons(&l, &state);
    draw_info(&l, &state, brush);
    draw_palette(&l, color_index);
    gfx::present();

    loop {
        if let Some(key) = hs::key_poll() {
            if key == 0x1B || key == b'q' || key == b'Q' {
                break;
            }
        }

        state = hs::mouse_state();

        let left = (state.buttons & MOUSE_BUTTON_LEFT) != 0;
        let right = (state.buttons & MOUSE_BUTTON_RIGHT) != 0;
        let middle = (state.buttons & MOUSE_BUTTON_MIDDLE) != 0;
        let left_press = left && (prev.buttons & MOUSE_BUTTON_LEFT) == 0;
        let right_press = right && (prev.buttons & MOUSE_BUTTON_RIGHT) == 0;
        let middle_press = middle && (prev.buttons & MOUSE_BUTTON_MIDDLE) == 0;

        if left_press {
            if let Some(index) = swatch_at(&l, state.x, state.y) {
                color_index = index;
            }
        }
        if right_press && in_canvas(&l, state.x, state.y) {
            draw_canvas_base(&l);
            last = None;
        }
        if middle_press {
            brush = if brush >= 3 { 1 } else { brush + 1 };
        }

        if left && in_canvas(&l, state.x, state.y) {
            let (cx, cy) = clamp_to_canvas(&l, state.x, state.y, brush);
            let color = palette_color(color_index);
            match last {
                Some((px, py)) => stroke(px, py, cx, cy, color, brush),
                None => gfx::fill_circle(cx, cy, brush, color),
            }
            last = Some((cx, cy));
        } else {
            last = None;
        }

        draw_buttons(&l, &state);
        draw_info(&l, &state, brush);
        draw_palette(&l, color_index);
        gfx::present();

        prev = state;
        frames = frames.saturating_add(1);
    }

    gfx::leave();

    hs::write_color(0x0B, "mousedemo: ");
    hs::print!(
        "{} frames, {} packets, cursor {},{}",
        frames, state.packets, state.x, state.y
    );
    if (state.flags & MOUSE_FLAG_AVAILABLE) == 0 {
        hs::write(" (mouse not detected)");
    }
    hs::write("\n");
    hs::exit(0)
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    hs::audio_off();
    hs::write("mousedemo: panic: ");
    hs::print!("{}\n", info);
    hs::exit(1)
}
