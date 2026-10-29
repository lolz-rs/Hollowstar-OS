//! PC speaker audio.
//!
//! PIT channel 2 generates the tone and port 0x61 gates it to the speaker.
//! This works on QEMU, VirtualBox, VMware and real hardware without needing a
//! sound-card driver.

use crate::io::{inb, outb};
use crate::pit;

const PIT_FREQUENCY: u32 = 1_193_182;

pub fn init() {
    off();
}

pub fn tone(freq: u32) {
    if freq == 0 {
        off();
        return;
    }
    let divisor = (PIT_FREQUENCY / freq).clamp(1, 0xFFFF) as u16;
    unsafe {
        // Channel 2, lobyte/hibyte, mode 3 (square wave), binary.
        outb(0x43, 0xB6);
        outb(0x42, (divisor & 0xFF) as u8);
        outb(0x42, (divisor >> 8) as u8);
        let value = inb(0x61);
        outb(0x61, value | 0x03);
    }
}

pub fn off() {
    unsafe {
        let value = inb(0x61);
        outb(0x61, value & !0x03);
    }
}

pub fn beep(freq: u32, ms: u64) {
    tone(freq);
    pit::sleep_ms(ms.max(1));
    off();
}
