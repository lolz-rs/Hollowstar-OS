#![no_std]
#![no_main]

use hollowstar as hs;

#[no_mangle]
pub extern "C" fn _start(args_ptr: *const u8, args_len: usize) -> ! {
    let args = hs::args_str(args_ptr, args_len);
    let bytes = args.as_bytes();

    let mut text = bytes;
    let mut newline = true;
    if bytes.len() >= 2 && &bytes[..2] == b"-n" && (bytes.len() == 2 || bytes[2] == b' ') {
        newline = false;
        text = &bytes[2..];
        while !text.is_empty() && text[0] == b' ' {
            text = &text[1..];
        }
    }

    hs::write_bytes(text);
    if newline {
        hs::write("\n");
    }
    hs::exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write("echo: panic\n");
    hs::exit(1)
}
