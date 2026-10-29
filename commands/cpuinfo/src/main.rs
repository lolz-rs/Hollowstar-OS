#![no_std]
#![no_main]

use hollowstar as hs;

fn cstr(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).unwrap_or("?")
}

#[no_mangle]
pub extern "C" fn _start(_args_ptr: *const u8, _args_len: usize) -> ! {
    let mut info = hs::SystemInfo::zeroed();
    hs::info(&mut info);

    hs::write_color(0x0B, "CPU\n");
    hs::print!("  vendor     {}\n", cstr(&info.cpu_vendor));
    hs::print!("  brand      {}\n", cstr(&info.cpu_brand));
    hs::print!("  family     {}\n", info.cpu_family);
    hs::print!("  model      {}\n", info.cpu_model);
    hs::print!("  stepping   {}\n", info.cpu_stepping);
    hs::print!("  arch       x86_64\n");

    hs::exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write("cpuinfo: panic\n");
    hs::exit(1)
}
