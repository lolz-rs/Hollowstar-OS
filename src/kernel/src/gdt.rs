use core::arch::asm;
use core::mem::size_of;
use core::ptr::addr_of_mut;

#[repr(C, packed)]
struct GdtPointer {
    limit: u16,
    base: u64,
}

#[repr(C, packed)]
struct Tss {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    iomap_base: u16,
}

// Selector layout intentionally matches the bootloader GDT:
//   0x00 null, 0x08 32-bit code, 0x10 32-bit data,
//   0x18 64-bit code, 0x20 64-bit data, 0x28 TSS.
static mut GDT: [u64; 7] = [0; 7];
static mut TSS: Tss = Tss {
    reserved0: 0,
    rsp0: 0x90000,
    rsp1: 0,
    rsp2: 0,
    reserved1: 0,
    ist: [0; 7],
    reserved2: 0,
    reserved3: 0,
    iomap_base: size_of::<Tss>() as u16,
};

static mut GDTR: GdtPointer = GdtPointer { limit: 0, base: 0 };

pub const KERNEL_CODE: u16 = 0x18;
pub const KERNEL_DATA: u16 = 0x20;
pub const TSS_SELECTOR: u16 = 0x28;

pub fn init() {
    unsafe {
        let tss = &mut *addr_of_mut!(TSS);
        tss.ist[0] = 0x700000; // double-fault IST stack
        tss.rsp0 = 0x90000;

        let gdt = &mut *addr_of_mut!(GDT);
        gdt[0] = 0;
        gdt[1] = 0x00CF_9A00_0000_FFFF; // 32-bit code
        gdt[2] = 0x00CF_9200_0000_FFFF; // 32-bit data
        gdt[3] = 0x00AF_9A00_0000_FFFF; // 64-bit code
        gdt[4] = 0x00AF_9200_0000_FFFF; // 64-bit data

        let base = addr_of_mut!(TSS) as u64;
        let limit = (size_of::<Tss>() - 1) as u64;
        gdt[5] = (limit & 0xFFFF)
            | ((base & 0x00FF_FFFF) << 16)
            | (0x89u64 << 40) // available 64-bit TSS, present
            | (((limit >> 16) & 0x0F) << 48)
            | (((base >> 24) & 0xFF) << 56);
        gdt[6] = base >> 32;

        let gdtr = &mut *addr_of_mut!(GDTR);
        gdtr.limit = (size_of::<[u64; 7]>() - 1) as u16;
        gdtr.base = addr_of_mut!(GDT) as u64;

        asm!("lgdt [{}]", in(reg) gdtr, options(readonly, nostack, preserves_flags));
        asm!(
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ss, ax",
            in("ax") KERNEL_DATA,
            options(nostack, preserves_flags)
        );
        asm!("ltr ax", in("ax") TSS_SELECTOR, options(nostack, preserves_flags));
    }
}
