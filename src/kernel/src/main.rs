#![no_std]
#![no_main]

mod archive;
mod audio;
mod console;
mod cpu;
mod elf;
mod font;
mod gdt;
mod graphics;
mod idt;
mod io;
mod keyboard;
mod memory;
mod mouse;
mod pic;
mod pit;
mod ramdisk;
mod serial;
mod shell;
mod syscall;

use core::panic::PanicInfo;

use hollowstar_common::{color, BootInfo, BOOT_MAGIC, BOOT_VERSION};

fn halt() -> ! {
    loop {
        unsafe {
            io::disable_interrupts();
            io::hlt();
        }
    }
}

#[no_mangle]
pub extern "C" fn kernel_main(boot_info: *const BootInfo) -> ! {
    console::init();
    console::set_color(color::WHITE);

    if boot_info.is_null() {
        println!("Hollowstar: missing boot info");
        halt();
    }

    let info = unsafe { &*boot_info };
    if (info.magic != BOOT_MAGIC) || (info.version != BOOT_VERSION) {
        println!("Hollowstar: invalid boot info");
        halt();
    }

    gdt::init();
    idt::init();
    pic::init();
    pit::init();
    audio::init();
    mouse::init();
    ramdisk::init();

    if !archive::init() {
        console::set_color(color::RED);
        println!("Hollowstar: /commands archive is invalid");
        console::set_color(color::WHITE);
    }

    unsafe {
        io::enable_interrupts();
    }

    // The PIT is live now, so presenting can use the retrace timeout safely.
    console::set_present_ready(true);
    shell::run();
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    console::set_color(color::RED);
    println!();
    println!("KERNEL PANIC: {}", info);
    console::set_color(color::WHITE);
    console::flush();
    halt();
}
