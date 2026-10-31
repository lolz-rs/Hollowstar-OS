#![no_std]
#![no_main]

//! Print one or more RAM-disk files.

use hollowstar as hs;

fn usage() -> ! {
    hs::write("usage: cat [-n] <file>...\n");
    hs::write("  -n  number output lines\n");
    hs::exit(0)
}

#[no_mangle]
pub extern "C" fn _start(args_ptr: *const u8, args_len: usize) -> ! {
    let raw = hs::args_str(args_ptr, args_len).trim();
    let mut rest = raw;
    let mut number = false;

    loop {
        if rest.is_empty() {
            break;
        }
        if rest == "-h" || rest == "--help" {
            usage();
        }
        if rest.starts_with("-n")
            && (rest.len() == 2 || rest.as_bytes()[2].is_ascii_whitespace())
        {
            number = true;
            rest = rest[2..].trim_start();
            continue;
        }
        if rest.starts_with('-') {
            hs::write_err("cat: unknown option: ");
            hs::write_err(rest);
            hs::write_err("\n");
            hs::exit(1);
        }
        break;
    }

    if rest.is_empty() {
        usage();
    }

    let mut status = 0u64;
    let mut line = 1u64;
    let bytes = rest.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() {
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }
        let start = pos;
        while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let name = core::str::from_utf8(&bytes[start..pos]).unwrap_or("");

        let Some(fd) = hs::fs_open(name, hs::FS_READ) else {
            hs::write_err("cat: ");
            hs::write_err(name);
            hs::write_err(": no such file\n");
            status = 1;
            continue;
        };

        let mut buf = [0u8; 512];
        let mut at_start = true;
        loop {
            let Some(n) = hs::fs_read(fd, &mut buf) else {
                hs::write_err("cat: ");
                hs::write_err(name);
                hs::write_err(": read failed\n");
                status = 1;
                break;
            };
            if n == 0 {
                break;
            }
            if number {
                let mut i = 0usize;
                while i < n {
                    if at_start {
                        hs::print!("{:>6}\t", line);
                        line += 1;
                        at_start = false;
                    }
                    let byte = buf[i];
                    hs::write_bytes(&[byte]);
                    if byte == b'\n' {
                        at_start = true;
                    }
                    i += 1;
                }
            } else {
                hs::write_bytes(&buf[..n]);
            }
        }
        hs::fs_close(fd);
    }

    hs::exit(status)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write_err("cat: panic\n");
    hs::exit(1)
}
