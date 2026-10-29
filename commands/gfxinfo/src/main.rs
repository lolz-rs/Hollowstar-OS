#![no_std]
#![no_main]

use hollowstar as hs;
use hs::gfx;

#[no_mangle]
pub extern "C" fn _start(_args_ptr: *const u8, _args_len: usize) -> ! {
    if !gfx::init() {
        hs::write_color(0x0C, "gfxinfo: no linear framebuffer active\n");
        hs::write("  The kernel is using the VGA text fallback.\n");
        hs::exit(1);
    }

    let info = gfx::info();
    let back_size = info.pitch as u64 * info.height as u64;

    hs::write_color(0x0B, "Graphics\n");
    hs::write("  framebuffer ");
    hs::print!("{:#010x}\n", info.framebuffer);
    hs::write("  back buffer ");
    hs::print!("{:#010x} ({} KiB)\n", info.back_buffer, back_size / 1024);
    hs::write("  mode        ");
    hs::print!("{}x{}x{}\n", info.width, info.height, info.bpp);
    hs::write("  pitch       ");
    hs::print!("{} bytes (fb {} bytes)\n", info.pitch, info.framebuffer_pitch);
    hs::write("  text grid   ");
    hs::print!("{}x{} cells\n", info.text_cols, info.text_rows);
    hs::write("  font        ");
    hs::print!("{:#010x} ({}x{})\n", info.font_addr, info.font_width, info.font_height);
    hs::write("  masks       ");
    hs::print!(
        "R {:#08x} G {:#08x} B {:#08x}\n",
        info.red_mask, info.green_mask, info.blue_mask
    );
    hs::write("  double buffered + vsync present: yes\n");

    hs::exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write("gfxinfo: panic\n");
    hs::exit(1)
}
