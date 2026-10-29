#![no_std]
#![no_main]

use hollowstar as hs;

fn parse_u32(text: &str) -> Option<u32> {
    let mut value = 0u32;
    let mut any = false;
    for b in text.bytes() {
        if b.is_ascii_digit() {
            value = value.saturating_mul(10).saturating_add((b - b'0') as u32);
            any = true;
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

#[no_mangle]
pub extern "C" fn _start(args_ptr: *const u8, args_len: usize) -> ! {
    let args = hs::args_str(args_ptr, args_len).trim();
    if args == "help" || args == "-h" || args == "--help" {
        hs::write("usage: beep [frequency] [milliseconds]\n");
        hs::write("  Default is 880 Hz for 200 ms. 'beep melody' plays a short arpeggio.\n");
        hs::exit(0);
    }

    if args == "melody" {
        let notes = [262u32, 330, 392, 523, 392, 330, 262];
        for note in notes {
            hs::beep(note, 160);
            hs::sleep(40);
        }
        hs::write_color(0x0B, "beep: ");
        hs::write("played a 7-note arpeggio on the PC speaker\n");
        hs::exit(0);
    }

    let mut parts = args.split_whitespace();
    let freq = parts.next().and_then(parse_u32).unwrap_or(880).clamp(20, 20_000);
    let ms = parts
        .next()
        .and_then(parse_u32)
        .unwrap_or(200)
        .clamp(1, 10_000);

    hs::write_color(0x0B, "beep: ");
    hs::print!("{} Hz for {} ms (PIT channel 2 / PC speaker)\n", freq, ms);
    hs::beep(freq, ms as u64);
    hs::exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::audio_off();
    hs::write("beep: panic\n");
    hs::exit(1)
}
