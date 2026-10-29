#![no_std]
#![no_main]

use hollowstar as hs;

#[no_mangle]
pub extern "C" fn _start(_args_ptr: *const u8, _args_len: usize) -> ! {
    let mut info = hs::SystemInfo::zeroed();
    hs::info(&mut info);

    hs::write_color(0x0B, "Memory\n");
    hs::write("  total      ");
    hs::print_size(info.mem_total_kb);
    hs::write("\n  usable     ");
    hs::print_size(info.mem_usable_kb);
    hs::write("\n  mapped     ");
    hs::print_size(info.mem_mapped_kb);
    hs::write("\n  reserved   ");
    hs::print_size(info.mem_reserved_kb);
    hs::write("\n  kernel     ");
    hs::print!("{} KiB\n", info.kernel_size / 1024);
    hs::print!("  commands   {}\n", info.command_count);

    hs::exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write("meminfo: panic\n");
    hs::exit(1)
}
