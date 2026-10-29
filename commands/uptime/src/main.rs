#![no_std]
#![no_main]

use hollowstar as hs;

#[no_mangle]
pub extern "C" fn _start(_args_ptr: *const u8, _args_len: usize) -> ! {
    let ms = hs::uptime_ms();
    let seconds = ms / 1000;
    hs::print!("up {}s ({} ms)\n", seconds, ms);
    hs::exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write("uptime: panic\n");
    hs::exit(1)
}
