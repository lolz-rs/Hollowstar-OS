use crate::io::{inb, io_wait, outb};

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

pub fn init() {
    unsafe {
        // Start initialization sequence.
        outb(PIC1_COMMAND, 0x11);
        io_wait();
        outb(PIC2_COMMAND, 0x11);
        io_wait();

        // Vector offsets: IRQ0..7 -> 32..39, IRQ8..15 -> 40..47.
        outb(PIC1_DATA, 0x20);
        io_wait();
        outb(PIC2_DATA, 0x28);
        io_wait();

        // Cascade wiring.
        outb(PIC1_DATA, 0x04);
        io_wait();
        outb(PIC2_DATA, 0x02);
        io_wait();

        outb(PIC1_DATA, 0x01);
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();

        // Unmask IRQ0, IRQ1, IRQ2 (cascade), IRQ4 (COM1); mask everything else.
        outb(PIC1_DATA, 0xE8);
        outb(PIC2_DATA, 0xFF);
    }
}

pub fn eoi(irq: u8) {
    unsafe {
        if irq >= 8 {
            outb(PIC2_COMMAND, 0x20);
        }
        outb(PIC1_COMMAND, 0x20);
    }
}

pub fn unmask(irq: u8) {
    unsafe {
        let port = if irq < 8 { PIC1_DATA } else { PIC2_DATA };
        let bit = if irq < 8 { irq } else { irq - 8 };
        let value = inb(port) & !(1u8 << bit);
        outb(port, value);
    }
}

#[allow(dead_code)]
pub fn mask(irq: u8) {
    unsafe {
        let port = if irq < 8 { PIC1_DATA } else { PIC2_DATA };
        let bit = if irq < 8 { irq } else { irq - 8 };
        let value = inb(port) | (1u8 << bit);
        outb(port, value);
    }
}
