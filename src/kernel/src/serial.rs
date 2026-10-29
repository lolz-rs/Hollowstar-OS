use crate::io::{inb, outb};

const COM1: u16 = 0x3F8;

pub fn init() {
    unsafe {
        outb(COM1 + 1, 0x00); // disable UART interrupts
        outb(COM1 + 3, 0x80); // enable DLAB
        outb(COM1 + 0, 0x01); // divisor low: 115200 baud
        outb(COM1 + 1, 0x00); // divisor high
        outb(COM1 + 3, 0x03); // 8N1
        outb(COM1 + 2, 0xC7); // FIFO enable/clear, 14-byte threshold
        outb(COM1 + 4, 0x0B); // IRQs enabled, RTS/DSR set
        outb(COM1 + 1, 0x01); // enable received-data-available interrupt
    }
}

/// Poll the UART directly with interrupts masked, so an IRQ cannot steal the
/// byte between the line-status check and the data read.
pub fn poll() -> Option<u8> {
    use crate::io::{disable_interrupts, enable_interrupts, read_rflags};

    unsafe {
        let flags = read_rflags();
        disable_interrupts();
        let byte = if (inb(COM1 + 5) & 0x01) != 0 {
            Some(match inb(COM1) {
                b'\r' => b'\n',
                0x7F => 0x08,
                b => b,
            })
        } else {
            None
        };
        if (flags & 0x200) != 0 {
            enable_interrupts();
        }
        byte
    }
}

#[inline]
pub fn irq_pending() -> bool {
    unsafe { (inb(COM1 + 5) & 0x01) != 0 }
}

#[inline]
pub fn read_byte() -> u8 {
    unsafe { inb(COM1) }
}

#[inline]
pub fn write_byte(byte: u8) {
    unsafe {
        while (inb(COM1 + 5) & 0x20) == 0 {}
        outb(COM1, byte);
    }
}

pub fn write_bytes(bytes: &[u8]) {
    for &byte in bytes {
        if byte == b'\n' {
            write_byte(b'\r');
        }
        write_byte(byte);
    }
}

pub fn write_str(text: &str) {
    write_bytes(text.as_bytes());
}
