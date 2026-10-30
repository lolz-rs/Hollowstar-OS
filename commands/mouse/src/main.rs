#![no_std]
#![no_main]

//! Print the live PS/2 mouse state.

use hollowstar as hs;
use hs::{
    MOUSE_BUTTON_BACK, MOUSE_BUTTON_FORWARD, MOUSE_BUTTON_LEFT, MOUSE_BUTTON_MIDDLE,
    MOUSE_BUTTON_RIGHT, MOUSE_FLAG_AVAILABLE, MOUSE_FLAG_OVERFLOW, MOUSE_FLAG_WHEEL,
};

fn up_down(pressed: bool) -> &'static str {
    if pressed {
        "down"
    } else {
        "up"
    }
}

#[no_mangle]
pub extern "C" fn _start(args_ptr: *const u8, args_len: usize) -> ! {
    let args = hs::args_str(args_ptr, args_len).trim();
    if args == "help" || args == "-h" || args == "--help" {
        hs::write("usage: mouse\n");
        hs::write("  Prints position, deltas, wheel, buttons, packet count and features.\n");
        hs::exit(0);
    }

    let state = hs::mouse_state();
    if (state.flags & MOUSE_FLAG_AVAILABLE) == 0 {
        hs::write_color(hs::color::RED, "mouse: not detected\n");
        hs::write("mouse: the PS/2 auxiliary device did not initialize.\n");
        hs::exit(1);
    }

    hs::write_color(hs::color::CYAN, "mouse: ");
    hs::write("PS/2 auxiliary device ready\n");
    hs::print!("  position: {}, {}\n", state.x, state.y);
    hs::print!("  delta:    {}, {}\n", state.dx, state.dy);
    hs::print!("  wheel:    {}\n", state.wheel);
    hs::print!(
        "  buttons:  {:#04x} (left={} middle={} right={} back={} forward={})\n",
        state.buttons,
        up_down((state.buttons & MOUSE_BUTTON_LEFT) != 0),
        up_down((state.buttons & MOUSE_BUTTON_MIDDLE) != 0),
        up_down((state.buttons & MOUSE_BUTTON_RIGHT) != 0),
        up_down((state.buttons & MOUSE_BUTTON_BACK) != 0),
        up_down((state.buttons & MOUSE_BUTTON_FORWARD) != 0),
    );
    hs::print!("  packets:  {}\n", state.packets);
    hs::print!(
        "  features: {}{}\n",
        if (state.flags & MOUSE_FLAG_WHEEL) != 0 {
            "wheel"
        } else {
            "3-byte"
        },
        if (state.flags & MOUSE_FLAG_OVERFLOW) != 0 {
            ", overflow-seen"
        } else {
            ""
        },
    );
    hs::exit(0)
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    hs::write("mouse: panic: ");
    hs::print!("{}\n", info);
    hs::exit(1)
}
