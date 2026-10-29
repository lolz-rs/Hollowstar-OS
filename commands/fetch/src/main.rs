#![no_std]
#![no_main]

use hollowstar as hs;

// A hollow five-point star. The logo column is 25 characters wide.
const STAR: [&str; 14] = [
    "            #",
    "           # #",
    "          #   #",
    "         #     #",
    "#########       #########",
    " ##                   ##",
    "   ##               ##",
    "     ##           ##",
    "       #    #    #",
    "      #   ## ##   #",
    "      #  #     ## #",
    "     # ##        # #",
    "     ##           ##",
    "    #               #",
];

fn cstr(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).unwrap_or("?")
}

fn star(line: &str) {
    hs::write_color(0x0B, line);
    let mut pad = 26usize.saturating_sub(line.len());
    while pad > 0 {
        hs::write(" ");
        pad -= 1;
    }
}

fn label(text: &str) {
    hs::write_color(0x08, text);
}

fn value(text: &str) {
    hs::write_color(0x07, text);
}

#[no_mangle]
pub extern "C" fn _start(_args_ptr: *const u8, _args_len: usize) -> ! {
    let mut info = hs::SystemInfo::zeroed();
    hs::info(&mut info);

    star(STAR[0]);
    hs::write_color(0x0F, "Hollowstar ");
    value(hs::VERSION_STRING);
    hs::write("\n");

    star(STAR[1]);
    label("kernel   ");
    value("0.1.0 x86_64");
    hs::write("\n");

    star(STAR[2]);
    label("uptime   ");
    let seconds = info.uptime_ms / 1000;
    if seconds >= 60 {
        hs::print!("{}m {}s\n", seconds / 60, seconds % 60);
    } else {
        hs::print!("{}s\n", seconds);
    }

    star(STAR[3]);
    label("cpu      ");
    let brand = cstr(&info.cpu_brand);
    let vendor = cstr(&info.cpu_vendor);
    if brand.is_empty() || brand == vendor {
        value(vendor);
    } else {
        value(brand);
    }
    hs::write("\n");

    star(STAR[4]);
    label("memory   ");
    hs::print_size(info.mem_usable_kb);
    hs::write("\n");

    star(STAR[5]);
    label("commands ");
    hs::print!("{}\n", info.command_count);

    for line in STAR.iter().skip(6) {
        star(line);
        hs::write("\n");
    }

    hs::exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write("fetch: panic\n");
    hs::exit(1)
}
