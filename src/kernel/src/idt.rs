use core::arch::asm;
use core::mem::size_of;
use core::ptr::addr_of_mut;

use crate::{gdt, io, keyboard, mouse, pic, pit, println, serial, syscall};

#[repr(C)]
pub struct InterruptFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub vector: u64,
    pub error: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const fn zero() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

static mut IDT: [IdtEntry; 256] = [IdtEntry::zero(); 256];
static mut IDTR: IdtPointer = IdtPointer { limit: 0, base: 0 };

extern "C" {
    static isr_stub_table: [u64; 256];
}

pub fn init() {
    unsafe {
        let table = &isr_stub_table;
        let idt = &mut *addr_of_mut!(IDT);
        for i in 0..256 {
            let handler = table[i];
            let ist = if i == 8 { 1 } else { 0 };
            let dpl: u8 = if i == 0x80 { 3 } else { 0 };
            idt[i] = IdtEntry {
                offset_low: handler as u16,
                selector: gdt::KERNEL_CODE,
                ist,
                type_attr: 0x8E | (dpl << 5),
                offset_mid: (handler >> 16) as u16,
                offset_high: (handler >> 32) as u32,
                reserved: 0,
            };
        }

        let idtr = &mut *addr_of_mut!(IDTR);
        idtr.limit = (size_of::<[IdtEntry; 256]>() - 1) as u16;
        idtr.base = addr_of_mut!(IDT) as u64;
        asm!("lidt [{}]", in(reg) idtr, options(readonly, nostack, preserves_flags));
    }
}

fn halt_forever() -> ! {
    loop {
        unsafe {
            io::disable_interrupts();
            io::hlt();
        }
    }
}

fn exception(frame: &InterruptFrame) -> ! {
    println!();
    println!("!!! CPU EXCEPTION {} (error {:#x})", frame.vector, frame.error);
    println!("RIP={:#018x} CS={:#x} RFLAGS={:#018x}", frame.rip, frame.cs, frame.rflags);
    if frame.vector == 14 {
        let cr2: u64;
        unsafe {
            asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));
        }
        println!("Page fault address: {:#018x}", cr2);
    }
    println!("System halted.");
    halt_forever();
}

#[no_mangle]
pub extern "C" fn interrupt_handler(frame: *mut InterruptFrame) {
    let frame = unsafe { &mut *frame };
    match frame.vector {
        0..=31 => exception(frame),
        32 => {
            pit::tick();
            pic::eoi(0);
        }
        33 => {
            keyboard::handle_irq();
            pic::eoi(1);
        }
        36 => {
            while serial::irq_pending() {
                keyboard::handle_serial_byte(serial::read_byte());
            }
            pic::eoi(4);
        }
        44 => {
            mouse::handle_irq();
            pic::eoi(12);
        }
        0x80 => {
            frame.rax = syscall::dispatch(frame.rax, frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8);
        }
        other => {
            println!();
            println!("!!! Unhandled interrupt vector {:#x}", other);
            pic::eoi(0);
        }
    }
}
