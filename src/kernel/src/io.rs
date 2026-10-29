use core::arch::asm;

#[inline(always)]
pub unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

#[inline(always)]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    value
}

#[inline(always)]
pub unsafe fn io_wait() {
    outb(0x80, 0);
}

#[inline(always)]
pub unsafe fn read_rflags() -> u64 {
    let flags: u64;
    asm!("pushfq; pop {}", out(reg) flags, options(nomem, nostack, preserves_flags));
    flags
}

#[inline(always)]
pub unsafe fn enable_interrupts() {
    asm!("sti", options(nomem, nostack));
}

#[inline(always)]
pub unsafe fn disable_interrupts() {
    asm!("cli", options(nomem, nostack));
}

#[inline(always)]
pub unsafe fn hlt() {
    asm!("hlt", options(nomem, nostack));
}

#[inline(always)]
pub unsafe fn wait_for_interrupt() {
    asm!("sti; hlt; cli", options(nomem, nostack));
}
