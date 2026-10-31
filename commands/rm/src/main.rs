#![no_std]
#![no_main]

//! Remove files from the flat RAM disk.

use hollowstar as hs;

fn usage() -> ! {
    hs::write("usage: rm [-f] [-v] <file>...\n");
    hs::write("  -f  ignore missing files\n");
    hs::write("  -v  print each removed file\n");
    hs::exit(0)
}

#[no_mangle]
pub extern "C" fn _start(args_ptr: *const u8, args_len: usize) -> ! {
    let raw = hs::args_str(args_ptr, args_len).trim();
    let mut rest = raw;
    let mut force = false;
    let mut verbose = false;

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
                    b'f' => force = true,
                    b'v' => verbose = true,
                    _ => ok = false,
                }
            }
            if !ok {
                hs::write_err("rm: unknown option: ");
                hs::write_err(&rest[..end]);
                hs::write_err("\n");
                hs::exit(1);
            }
            rest = rest[end..].trim_start();
            continue;
        }
        break;
    }

    if rest.is_empty() {
        if force {
            hs::exit(0);
        }
        hs::write_err("usage: rm [-f] [-v] <file>...\n");
        hs::exit(1);
    }

    let mut status = 0u64;
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

        if hs::fs_remove(name) {
            if verbose {
                hs::write("removed ");
                hs::write(name);
                hs::write("\n");
            }
        } else if !force {
            hs::write_err("rm: ");
            hs::write_err(name);
            hs::write_err(": no such file\n");
            status = 1;
        }
    }

    hs::exit(status)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write_err("rm: panic\n");
    hs::exit(1)
}
