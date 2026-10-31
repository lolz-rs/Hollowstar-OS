#![no_std]
#![no_main]

//! Print text, optionally interpreting backslash escapes.

use hollowstar as hs;

fn usage() -> ! {
    hs::write("usage: echo [-n] [-e] [text]\n");
    hs::write("  -n  do not print the trailing newline\n");
    hs::write("  -e  interpret \\n, \\t, \\r, \\\\, \\0, and \\xHH\n");
    hs::exit(0)
}

fn strip_outer_quotes(text: &[u8]) -> &[u8] {
    if text.len() >= 2 {
        let first = text[0];
        let last = text[text.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &text[1..text.len() - 1];
        }
    }
    text
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn write_escaped(bytes: &[u8]) {
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'\\' || i + 1 >= bytes.len() {
            hs::write_bytes(&[bytes[i]]);
            i += 1;
            continue;
        }
        let next = bytes[i + 1];
        match next {
            b'n' => {
                hs::write("\n");
                i += 2;
            }
            b't' => {
                hs::write("\t");
                i += 2;
            }
            b'r' => {
                hs::write("\r");
                i += 2;
            }
            b'\\' => {
                hs::write("\\");
                i += 2;
            }
            b'0' => {
                hs::write_bytes(&[0]);
                i += 2;
            }
            b'x' if i + 3 < bytes.len() => {
                if let (Some(hi), Some(lo)) = (hex_value(bytes[i + 2]), hex_value(bytes[i + 3])) {
                    hs::write_bytes(&[(hi << 4) | lo]);
                    i += 4;
                } else {
                    hs::write_bytes(&[b'\\', b'x']);
                    i += 2;
                }
            }
            _ => {
                hs::write_bytes(&[b'\\', next]);
                i += 2;
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn _start(args_ptr: *const u8, args_len: usize) -> ! {
    let raw = hs::args_str(args_ptr, args_len);
    let mut rest = raw.trim_start();
    let mut newline = true;
    let mut escapes = false;

    loop {
        if rest.is_empty() {
            break;
        }
        if rest == "-h" || rest == "--help" {
            usage();
        }
        if rest.starts_with('-') && !rest.starts_with("--") {
            let end = rest
                .find(|c: char| c == ' ' || c == '\t')
                .unwrap_or(rest.len());
            let opts = &rest[1..end];
            if opts.is_empty() {
                break;
            }
            let mut ok = true;
            for c in opts.bytes() {
                match c {
                    b'n' => newline = false,
                    b'e' => escapes = true,
                    b'E' => escapes = false,
                    _ => ok = false,
                }
            }
            if !ok {
                hs::write_err("echo: unknown option: ");
                hs::write_err(&rest[..end]);
                hs::write_err("\n");
                hs::exit(1);
            }
            rest = rest[end..].trim_start();
            continue;
        }
        break;
    }

    let text = strip_outer_quotes(rest.as_bytes());
    if escapes {
        write_escaped(text);
    } else {
        hs::write_bytes(text);
    }
    if newline {
        hs::write("\n");
    }
    hs::exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write_err("echo: panic\n");
    hs::exit(1)
}
