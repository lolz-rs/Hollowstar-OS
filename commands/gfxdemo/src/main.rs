#![no_std]
#![no_main]

use hollowstar as hs;
use hs::gfx;

const NOTES: [u32; 6] = [262, 330, 392, 523, 392, 330];

fn parse_u64(text: &str) -> Option<u64> {
    let mut value = 0u64;
    let mut any = false;
    for b in text.bytes() {
        if b.is_ascii_digit() {
            value = value.saturating_mul(10).saturating_add((b - b'0') as u64);
            any = true;
        } else if b == b' ' || b == b'\t' {
            if any {
                break;
            }
        } else {
            return None;
        }
    }
    if any {
        Some(value)
    } else {
        None
    }
}

fn u32_str(mut value: u32, buf: &mut [u8; 12]) -> &str {
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

fn draw_u32(x: i32, y: i32, value: u32, color: u32) {
    let mut buf = [0u8; 12];
    let text = u32_str(value, &mut buf);
    gfx::text(x, y, text, color, 1);
}

fn draw_background(t: u64) {
    let w = gfx::width();
    let h = gfx::height();
    let bands = 96i32;
    let band_h = (h + bands - 1) / bands;
    let phase = (t / 60) as u32;
    let pulse = ((gfx::sin((t / 20) as i32) + 1024) / 2) as u32;
    let brightness = 80 + (pulse * 100 / 1024) as u8;

    let mut b = 0i32;
    while b < bands {
        let y = b * band_h;
        let wobble = (gfx::sin((b * 7 + phase as i32) & 255) / 32) as i32;
        let hue = ((phase as i32 + b * 3 + wobble) & 255) as u8;
        let color = gfx::hue_bright(hue, brightness);
        gfx::fill_rect(0, y, w, band_h, color);
        b += 1;
    }
}

fn draw_color_bars() {
    let colors = [
        gfx::rgb(255, 255, 255),
        gfx::rgb(255, 255, 0),
        gfx::rgb(0, 255, 255),
        gfx::rgb(0, 255, 0),
        gfx::rgb(255, 0, 255),
        gfx::rgb(255, 0, 0),
        gfx::rgb(0, 0, 255),
        gfx::rgb(0, 0, 0),
    ];
    let w = gfx::width();
    let bar = w / 8;
    let mut i = 0i32;
    while i < 8 {
        gfx::fill_rect(i * bar, 0, bar, 8, colors[i as usize]);
        i += 1;
    }
}

/// Smooth true-colour rainbow: 16.7 million colours instead of the old
/// 16-colour text palette.
fn draw_rainbow(t: u64) {
    let w = gfx::width();
    let phase = (t / 30) as u8;
    let span = 6i32;
    let mut x = 0i32;
    while x < w {
        let hue = phase.wrapping_add((x * 256 / w.max(1)) as u8);
        gfx::fill_rect(x, 8, span, 6, gfx::hsv(hue, 255, 255));
        x += span;
    }
}

/// Translucent panel showing hue swatches and an alpha ramp, exercising the
/// blend and HSV helpers in `gfx`.
fn draw_truecolor_panel(t: u64) {
    let w = gfx::width();
    if w < 900 {
        return;
    }
    let pw = 320;
    let ph = 122;
    let x = w - pw - 40;
    let y = 40;
    let tint = gfx::hsv((t / 40) as u8, 170, 255);
    gfx::fill_rect_alpha(x, y, pw, ph, tint, 120);
    gfx::rect_outline(x, y, pw, ph, 2, gfx::rgb(255, 255, 255));
    gfx::text(x + 14, y + 12, "TRUE COLOUR", gfx::rgb(255, 255, 255), 1);

    let swatches = 16i32;
    let gap = 2i32;
    let inner = pw - 28;
    let sw = (inner - (swatches - 1) * gap) / swatches;
    let mut i = 0i32;
    while i < swatches {
        let hue = ((i * 256) / swatches) as u8;
        gfx::fill_rect(
            x + 14 + i * (sw + gap),
            y + 34,
            sw,
            30,
            gfx::hsv(hue.wrapping_add((t / 25) as u8), 255, 255),
        );
        i += 1;
    }

    let steps = 8i32;
    let aw = (inner - (steps - 1) * gap) / steps;
    let mut a = 0i32;
    while a < steps {
        gfx::fill_rect_alpha(
            x + 14 + a * (aw + gap),
            y + 78,
            aw,
            26,
            gfx::rgb(255, 255, 255),
            (a as u32) * 255 / (steps as u32 - 1),
        );
        a += 1;
    }
}

fn draw_stars(t: u64) {
    let w = gfx::width();
    let h = gfx::height();
    let mut i = 0u32;
    while i < 140 {
        let seed = i
            .wrapping_mul(2_654_435_761)
            .wrapping_add(0x9E37_79B9);
        let x = (seed % w as u32) as i32;
        let speed = 1 + ((seed >> 16) % 3) as u64;
        let y = ((seed / 7 + (t / 35) as u32 * speed as u32) % h as u32) as i32;
        let twinkle = ((gfx::sin((t / 9 + i as u64 * 17) as i32) + 1024) / 2) as u32;
        let v = (130 + twinkle * 125 / 1024) as u8;
        let color = gfx::rgb(v, v, v);
        gfx::pixel(x, y, color);
        if i % 9 == 0 {
            gfx::pixel(x + 1, y, color);
        }
        i += 1;
    }
}

fn draw_orbits(t: u64, cx: i32, cy: i32) {
    let mut i = 0i32;
    while i < 6 {
        let angle = ((t / 8) as i32 + i * 43) & 255;
        let radius = 150 + i * 34;
        let x = cx + gfx::cos(angle) * radius / 1024;
        let y = cy + gfx::sin(angle) * radius / 1024;
        let r = 7 + (i % 3) * 3;
        let color = gfx::hue((i as u8) * 40 + (t / 45) as u8);
        gfx::fill_circle(x, y, r, color);
        gfx::circle(x, y, r + 3, gfx::rgb(255, 255, 255));
        i += 1;
    }
}

fn draw_square(t: u64, cx: i32, cy: i32) {
    let size = (gfx::height() / 5).max(48) as i32;
    let angle = ((t / 20) as i32) & 255;
    let c = gfx::cos(angle);
    let s = gfx::sin(angle);
    let pts = [
        (
            cx + (size * c - size * s) / 1024,
            cy + (size * s + size * c) / 1024,
        ),
        (
            cx + (-size * c - size * s) / 1024,
            cy + (-size * s + size * c) / 1024,
        ),
        (
            cx + (-size * c + size * s) / 1024,
            cy + (-size * s - size * c) / 1024,
        ),
        (
            cx + (size * c + size * s) / 1024,
            cy + (size * s - size * c) / 1024,
        ),
    ];
    gfx::fill_polygon(&pts, gfx::rgb(225, 25, 35));
    gfx::polygon_outline(&pts, gfx::rgb(255, 190, 190));

    let inner = size / 2;
    let ipts = [
        (
            cx + (inner * c - inner * s) / 1024,
            cy + (inner * s + inner * c) / 1024,
        ),
        (
            cx + (-inner * c - inner * s) / 1024,
            cy + (-inner * s + inner * c) / 1024,
        ),
        (
            cx + (-inner * c + inner * s) / 1024,
            cy + (-inner * s - inner * c) / 1024,
        ),
        (
            cx + (inner * c + inner * s) / 1024,
            cy + (inner * s - inner * c) / 1024,
        ),
    ];
    gfx::fill_polygon(&ipts, gfx::rgb(125, 10, 20));
}

fn draw_waves(t: u64) {
    let w = gfx::width();
    let h = gfx::height();
    let phase = (t / 10) as i32;

    let base1 = h - 38;
    let amp1 = 18;
    let mut prev = (0i32, base1 + gfx::sin(phase) * amp1 / 1024);
    let mut x = 4i32;
    while x < w {
        let y = base1 + gfx::sin(phase + x / 4) * amp1 / 1024;
        gfx::line(prev.0, prev.1, x, y, gfx::rgb(130, 255, 220));
        prev = (x, y);
        x += 4;
    }

    let base2 = h - 20;
    let amp2 = 11;
    let phase2 = phase + 128;
    prev = (0, base2 + gfx::sin(phase2) * amp2 / 1024);
    x = 4;
    while x < w {
        let y = base2 + gfx::sin(phase2 + x / 3) * amp2 / 1024;
        gfx::line(prev.0, prev.1, x, y, gfx::rgb(255, 190, 90));
        prev = (x, y);
        x += 4;
    }
}

fn panel(x: i32, y: i32, w: i32, h: i32) {
    let border = gfx::rgb(90, 210, 255);
    gfx::fill_rect(x, y, w, h, gfx::rgb(6, 10, 18));
    gfx::fill_rect(x, y, w, 2, border);
    gfx::fill_rect(x, y + h - 2, w, 2, border);
    gfx::fill_rect(x, y, 2, h, border);
    gfx::fill_rect(x + w - 2, y, 2, h, border);
}

fn draw_ui(t: u64, fps: u64, frame: u64) {
    let w = gfx::width();
    let h = gfx::height();
    let white = gfx::rgb(255, 255, 255);
    let cyan = gfx::rgb(120, 230, 255);
    let dim = gfx::rgb(150, 165, 185);

    panel(40, 40, 430, 112);
    gfx::text(62, 58, "HOLLOWSTAR", white, 3);
    gfx::text(62, 108, "GRAPHICS DEMO", cyan, 2);

    panel(40, h - 132, 360, 92);
    gfx::text(58, h - 116, "MODE", dim, 1);
    draw_u32(120, h - 116, gfx::info().width, white);
    gfx::text(170, h - 116, "x", dim, 1);
    draw_u32(184, h - 116, gfx::info().height, white);
    gfx::text(230, h - 116, "x32", dim, 1);

    gfx::text(58, h - 94, "FPS", dim, 1);
    draw_u32(120, h - 94, fps as u32, white);
    gfx::text(58, h - 72, "FRAME", dim, 1);
    draw_u32(120, h - 72, frame as u32, white);

    let hint = "ESC / Q TO EXIT";
    let hx = w - gfx::text_width(hint, 1) - 24;
    gfx::text(hx + 1, 25, hint, gfx::rgb(0, 0, 0), 1);
    gfx::text(hx, 24, hint, white, 1);

    let _ = t;
}

#[no_mangle]
pub extern "C" fn _start(args_ptr: *const u8, args_len: usize) -> ! {
    let args = hs::args_str(args_ptr, args_len).trim();
    if args == "help" || args == "-h" || args == "--help" {
        hs::write("usage: gfxdemo [seconds]\n");
        hs::write("  Runs the framebuffer demo until ESC/Q, or for the given number of seconds.\n");
        hs::exit(0);
    }
    let duration_ms = match parse_u64(args) {
        Some(seconds) if seconds > 0 => seconds.saturating_mul(1000),
        _ => 0,
    };

    if !gfx::init() {
        hs::write_color(0x0C, "gfxdemo: no linear framebuffer available\n");
        hs::write("gfxdemo: this build needs a VBE linear mode; VGA text fallback is active.\n");
        hs::exit(1);
    }
    if !gfx::enter() {
        hs::write_color(0x0C, "gfxdemo: failed to enter graphics mode\n");
        hs::exit(1);
    }

    let info = *gfx::info();
    let start = hs::uptime_ms();
    let mut frame = 0u64;
    let mut fps = 0u64;
    let mut fps_frames = 0u64;
    let mut fps_last = start;
    let mut note_on = false;
    let mut last_note = usize::MAX;

    loop {
        let now = hs::uptime_ms();
        let elapsed = now.saturating_sub(start);
        if duration_ms > 0 && elapsed >= duration_ms {
            break;
        }
        if let Some(key) = hs::key_poll() {
            if key == 0x1B || key == b'q' || key == b'Q' {
                break;
            }
        }

        let note_index = ((elapsed / 700) as usize) % NOTES.len();
        let note_phase = elapsed % 700;
        if note_phase < 240 {
            if !note_on || note_index != last_note {
                hs::tone(NOTES[note_index]);
                note_on = true;
                last_note = note_index;
            }
        } else if note_on {
            hs::audio_off();
            note_on = false;
        }

        let cx = info.width as i32 / 2;
        let cy = info.height as i32 / 2 + 10;
        draw_background(elapsed);
        draw_color_bars();
        draw_rainbow(elapsed);
        draw_stars(elapsed);
        draw_orbits(elapsed, cx, cy);
        draw_square(elapsed, cx, cy);
        draw_waves(elapsed);
        draw_truecolor_panel(elapsed);
        draw_ui(elapsed, fps, frame);
        gfx::present();

        frame += 1;
        fps_frames += 1;
        let span = now.saturating_sub(fps_last);
        if span >= 1000 {
            fps = fps_frames.saturating_mul(1000) / span.max(1);
            fps_frames = 0;
            fps_last = now;
        }
    }

    hs::audio_off();
    gfx::leave();

    let elapsed = hs::uptime_ms().saturating_sub(start);
    let avg_fps = if elapsed > 0 {
        frame.saturating_mul(1000) / elapsed
    } else {
        0
    };
    hs::write_color(0x0B, "gfxdemo: ");
    hs::print!(
        "{} frames in {} ms, {}x{}x{}, {} fps\n",
        frame, elapsed, info.width, info.height, info.bpp, avg_fps
    );
    hs::exit(0)
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    hs::audio_off();
    hs::write("gfxdemo: panic: ");
    hs::print!("{}\n", info);
    hs::exit(1)
}
