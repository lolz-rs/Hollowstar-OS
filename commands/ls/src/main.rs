#![no_std]
#![no_main]

//! List the flat RAM disk.

use core::cmp::Ordering;

use hollowstar as hs;

const NAME_BUF: usize = hs::FS_MAX_NAME + 1;

fn cstr(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).unwrap_or("?")
}

fn name_cmp(a: &[u8; NAME_BUF], b: &[u8; NAME_BUF]) -> Ordering {
    let mut i = 0usize;
    while i < NAME_BUF {
        let ca = a[i];
        let cb = b[i];
        if ca != cb {
            return ca.cmp(&cb);
        }
        if ca == 0 {
            return Ordering::Equal;
        }
        i += 1;
    }
    Ordering::Equal
}

fn usage() -> ! {
    hs::write("usage: ls [-1 | -l]\n");
    hs::write("  -1  one file name per line\n");
    hs::write("  -l  long form with sizes (default)\n");
    hs::exit(0)
}

#[no_mangle]
pub extern "C" fn _start(args_ptr: *const u8, args_len: usize) -> ! {
    let args = hs::args_str(args_ptr, args_len).trim();
    let mut oneline = false;
    if !args.is_empty() {
        if args == "-1" || args == "--oneline" {
            oneline = true;
        } else if args == "-l" || args == "--long" {
            oneline = false;
        } else if args == "-h" || args == "--help" {
            usage();
        } else {
            hs::write_err("ls: unknown option: ");
            hs::write_err(args);
            hs::write_err("\n");
            hs::write_err("usage: ls [-1 | -l]\n");
            hs::exit(1);
        }
    }

    let count = hs::fs_count();
    if count == 0 {
        hs::write("ls: no files\n");
        hs::exit(0);
    }

    let mut names = [[0u8; NAME_BUF]; hs::FS_MAX_FILES];
    let mut sizes = [0u32; hs::FS_MAX_FILES];
    let mut n = 0usize;
    let mut i = 0usize;
    while i < count && n < hs::FS_MAX_FILES {
        if let Some(size) = hs::fs_list(i, &mut names[n]) {
            sizes[n] = size;
            n += 1;
        }
        i += 1;
    }

    // Insertion sort keeps the list deterministic without needing an allocator.
    let mut a = 0usize;
    while a < n {
        let mut b = a + 1;
        while b < n {
            if name_cmp(&names[b], &names[a]) == Ordering::Less {
                names.swap(a, b);
                sizes.swap(a, b);
            }
            b += 1;
        }
        a += 1;
    }

    if oneline {
        let mut i = 0usize;
        while i < n {
            hs::write(cstr(&names[i]));
            hs::write("\n");
            i += 1;
        }
        hs::exit(0);
    }

    hs::print!("{:<28} {}\n", "NAME", "SIZE");
    let mut i = 0usize;
    while i < n {
        hs::print!("{:<28} ", cstr(&names[i]));
        hs::print_size_bytes(sizes[i] as u64);
        hs::write("\n");
        i += 1;
    }

    hs::write("--------------------------------------------\n");
    hs::print!(
        "{} file{}, ",
        n,
        if n == 1 { "" } else { "s" }
    );
    hs::print_size_bytes(hs::fs_used_bytes());
    hs::write(" used, ");
    hs::print_size_bytes((hs::FS_CAPACITY_BYTES as u64).saturating_sub(hs::fs_used_bytes()));
    hs::write(" free\n");
    hs::exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write_err("ls: panic\n");
    hs::exit(1)
}
