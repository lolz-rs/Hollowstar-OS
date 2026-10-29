#![no_std]
#![no_main]

use hollowstar as hs;

#[no_mangle]
pub extern "C" fn _start(args_ptr: *const u8, args_len: usize) -> ! {
    let args = hs::args_str(args_ptr, args_len).trim();
    let Ok(ms) = args.parse::<u64>() else {
        hs::write("usage: sleep <milliseconds>\n");
        hs::exit(1);
    };
    hs::sleep(ms);
    hs::exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write("sleep: panic\n");
    hs::exit(1)
}
