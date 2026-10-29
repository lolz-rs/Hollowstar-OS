use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::io::{disable_interrupts, enable_interrupts, inb, read_rflags, wait_for_interrupt};

const BUFFER_SIZE: usize = 256;

const PS2_DATA: u16 = 0x60;
const PS2_STATUS: u16 = 0x64;
const STATUS_OUTPUT_FULL: u8 = 1 << 0;
const STATUS_AUX_DATA: u8 = 1 << 5;

static mut KEY_BUFFER: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
static KEY_HEAD: AtomicUsize = AtomicUsize::new(0);
static KEY_TAIL: AtomicUsize = AtomicUsize::new(0);

static mut SHIFT: bool = false;
static mut CTRL: bool = false;
static mut CAPS: bool = false;
static mut EXTENDED: bool = false;

const NORMAL: [u8; 58] = [
    0, 0x1B, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'-', b'=', 0x08, b'\t',
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', b'\n', 0, b'a', b's',
    b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';', b'\'', b'`', 0, b'\\', b'z', b'x', b'c', b'v',
    b'b', b'n', b'm', b',', b'.', b'/', 0, 0, 0, b' ',
];

const SHIFTED: [u8; 58] = [
    0, 0x1B, b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'_', b'+', 0x08, b'\t',
    b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', b'O', b'P', b'{', b'}', b'\n', 0, b'A', b'S',
    b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':', b'"', b'~', 0, b'|', b'Z', b'X', b'C', b'V',
    b'B', b'N', b'M', b'<', b'>', b'?', 0, 0, 0, b' ',
];

unsafe fn push(byte: u8) {
    let head = KEY_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) % BUFFER_SIZE;
    if next == KEY_TAIL.load(Ordering::Acquire) {
        return; // buffer full
    }
    let buffer = &mut *addr_of_mut!(KEY_BUFFER);
    buffer[head] = byte;
    KEY_HEAD.store(next, Ordering::Release);
}

unsafe fn pop() -> Option<u8> {
    let tail = KEY_TAIL.load(Ordering::Relaxed);
    if tail == KEY_HEAD.load(Ordering::Acquire) {
        return None;
    }
    let buffer = &*addr_of_mut!(KEY_BUFFER);
    let byte = buffer[tail];
    KEY_TAIL.store((tail + 1) % BUFFER_SIZE, Ordering::Release);
    Some(byte)
}

/// IRQ1: read only keyboard bytes, leaving AUX bytes for IRQ12.
pub fn handle_irq() {
    let status = unsafe { inb(PS2_STATUS) };
    if status & STATUS_OUTPUT_FULL == 0 || status & STATUS_AUX_DATA != 0 {
        return;
    }
    let scancode = unsafe { inb(PS2_DATA) };
    handle_scancode(scancode);
}

fn handle_scancode(scancode: u8) {
    unsafe {
        if scancode == 0xE0 {
            *addr_of_mut!(EXTENDED) = true;
            return;
        }
        if scancode == 0xE1 {
            return;
        }

        let released = (scancode & 0x80) != 0;
        let code = scancode & 0x7F;

        if *addr_of_mut!(EXTENDED) {
            *addr_of_mut!(EXTENDED) = false;
            if !released {
                match code {
                    0x48 => push(0x80), // up
                    0x50 => push(0x81), // down
                    0x4B => push(0x82), // left
                    0x4D => push(0x83), // right
                    0x47 => push(0x84), // home
                    0x4F => push(0x85), // end
                    0x53 => push(0x7F), // delete
                    _ => {}
                }
            }
            return;
        }

        if released {
            match code {
                0x2A | 0x36 => *addr_of_mut!(SHIFT) = false,
                0x1D => *addr_of_mut!(CTRL) = false,
                _ => {}
            }
            return;
        }

        match code {
            0x2A | 0x36 => {
                *addr_of_mut!(SHIFT) = true;
                return;
            }
            0x1D => {
                *addr_of_mut!(CTRL) = true;
                return;
            }
            0x3A => {
                *addr_of_mut!(CAPS) = !*addr_of_mut!(CAPS);
                return;
            }
            _ => {}
        }

        if code as usize >= NORMAL.len() {
            return;
        }

        let normal = NORMAL[code as usize];
        let mut byte = if *addr_of_mut!(SHIFT) {
            SHIFTED[code as usize]
        } else {
            normal
        };

        // Caps lock applies to letters only.
        if normal.is_ascii_lowercase() && *addr_of_mut!(CAPS) {
            byte = if *addr_of_mut!(SHIFT) { normal } else { SHIFTED[code as usize] };
        }

        if *addr_of_mut!(CTRL) {
            byte = match byte {
                b'a'..=b'z' => byte - b'a' + 1,
                b'A'..=b'Z' => byte - b'A' + 1,
                b'[' => 0x1B,
                b'\\' => 0x1C,
                b']' => 0x1D,
                _ => byte,
            };
        }

        if byte != 0 {
            push(byte);
        }
    }
}

pub fn handle_serial_byte(byte: u8) {
    let byte = match byte {
        b'\r' => b'\n',
        0x7F => 0x08,
        _ => byte,
    };
    unsafe {
        push(byte);
    }
}

pub fn try_read() -> Option<u8> {
    unsafe {
        let flags = read_rflags();
        disable_interrupts();
        let byte = pop();
        if (flags & 0x200) != 0 {
            enable_interrupts();
        }
        byte
    }
}

pub fn read_char() -> u8 {
    let had_interrupts = unsafe { (read_rflags() & 0x200) != 0 };
    let mut idle = 0u32;
    loop {
        if let Some(byte) = try_read() {
            if had_interrupts {
                unsafe { enable_interrupts() };
            }
            return byte;
        }
        if let Some(byte) = crate::serial::poll() {
            if had_interrupts {
                unsafe { enable_interrupts() };
            }
            return byte;
        }
        idle += 1;
        if idle < 50_000 {
            unsafe {
                core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
            }
        } else {
            idle = 0;
            // Repaint the console (and cursor) when the mouse moved.
            crate::mouse::service();
            unsafe {
                wait_for_interrupt();
            }
        }
    }
}
