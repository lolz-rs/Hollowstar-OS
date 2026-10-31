use core::sync::atomic::{AtomicU64, Ordering};

use crate::io::outb;

const PIT_FREQUENCY: u64 = 1_193_182;
const TICK_HZ: u64 = 100;

static TICKS: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    let divisor = (PIT_FREQUENCY / TICK_HZ) as u16;
    unsafe {
        outb(0x43, 0x36); // channel 0, lobyte/hibyte, square wave
        outb(0x40, (divisor & 0xFF) as u8);
        outb(0x40, (divisor >> 8) as u8);
    }
}

pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

pub fn uptime_ms() -> u64 {
    TICKS.load(Ordering::Relaxed) * (1000 / TICK_HZ)
}

pub fn sleep_ms(ms: u64) {
    let target = uptime_ms().saturating_add(ms);
    while uptime_ms() < target {
        if crate::syscall::cancel_requested() {
            crate::syscall::abort_command();
        }
        unsafe {
            crate::io::wait_for_interrupt();
        }
    }
}
