#![no_std]
#![no_main]

use hollowstar as hs;

fn cstr(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).unwrap_or("?")
}

#[no_mangle]
pub extern "C" fn _start(_args_ptr: *const u8, _args_len: usize) -> ! {
    let count = hs::command_count();
    hs::write_color(hs::color::CYAN, "Commands in /commands:\n");
    for i in 0..count {
        let mut name = [0u8; 32];
        let mut description = [0u8; 64];
        if !hs::command_get(i, &mut name, &mut description) {
            continue;
        }
        let name = cstr(&name);
        let description = cstr(&description);
        hs::write("  ");
        hs::write_color(hs::color::GREEN, name);
        for _ in name.len()..14 {
            hs::write(" ");
        }
        hs::write(description);
        hs::write("\n");
    }
    hs::exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write("help: panic\n");
    hs::exit(1)
}
