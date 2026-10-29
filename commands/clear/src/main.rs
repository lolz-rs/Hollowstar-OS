#![no_std]
#![no_main]

use hollowstar as hs;

#[no_mangle]
pub extern "C" fn _start(_args_ptr: *const u8, _args_len: usize) -> ! {
    hs::clear();
    hs::exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write("clear: panic\n");
    hs::exit(1)
}
