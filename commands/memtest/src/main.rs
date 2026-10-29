#![no_std]
#![no_main]

use core::ptr::{read_volatile, write_volatile};

use hollowstar as hs;

const PATTERN: u64 = 0xA5A5_5A5A_C3C3_3C3C;

#[no_mangle]
pub extern "C" fn _start(_args_ptr: *const u8, _args_len: usize) -> ! {
    let mut info = hs::SystemInfo::zeroed();
    hs::info(&mut info);

    hs::write_color(0x0B, "High memory\n");
    hs::write("  top     ");
    hs::print_size(info.mem_top_kb);
    hs::write("\n");

    let top = info.mem_top_kb.saturating_mul(1024);
    if top < 4 * 1024 * 1024 {
        hs::write_color(0x0C, "  not enough RAM to probe\n");
        hs::exit(1);
    }

    let mut failures = 0u32;
    for offset in [8u64, 1024 * 1024, 2 * 1024 * 1024] {
        let addr = (top.saturating_sub(offset)) & !7u64;
        let ptr = addr as *mut u64;
        let old = unsafe { read_volatile(ptr) };
        unsafe { write_volatile(ptr, PATTERN) };
        let value = unsafe { read_volatile(ptr) };
        unsafe { write_volatile(ptr, old) };

        hs::print!("  {:#012x}  ", addr);
        if value == PATTERN {
            hs::write_color(0x0A, "ok\n");
        } else {
            hs::write_color(0x0C, "bad\n");
            failures += 1;
        }
    }

    if failures == 0 {
        hs::write("  all probes passed\n");
        hs::exit(0)
    } else {
        hs::print!("  {} probe(s) failed\n", failures);
        hs::exit(1)
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write("memtest: panic\n");
    hs::exit(1)
}
