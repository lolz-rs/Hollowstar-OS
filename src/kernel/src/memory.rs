use core::slice;

use hollowstar_common::{BootInfo, MemoryMapEntry, BOOT_INFO_ADDR};

pub fn boot_info() -> &'static BootInfo {
    unsafe { &*(BOOT_INFO_ADDR as *const BootInfo) }
}

pub fn entries() -> &'static [MemoryMapEntry] {
    let info = boot_info();
    let count = info.memmap_count as usize;
    if count == 0 {
        return &[];
    }
    unsafe {
        slice::from_raw_parts(info.memmap_addr as *const MemoryMapEntry, count)
    }
}

pub struct Summary {
    pub total_kb: u64,
    pub usable_kb: u64,
    pub reserved_kb: u64,
    pub mapped_kb: u64,
    pub top_kb: u64,
}

pub fn summary() -> Summary {
    let mut usable: u64 = 0;
    let mut reserved: u64 = 0;
    let mut usable_end: u64 = 0;

    for entry in entries() {
        if entry.kind == 1 && entry.length != 0 {
            let end = entry.base.saturating_add(entry.length);
            if end > usable_end {
                usable_end = end;
            }
        }
    }

    for entry in entries() {
        if entry.length == 0 {
            continue;
        }
        if entry.kind == 1 {
            usable = usable.saturating_add(entry.length);
        } else if entry.base < usable_end {
            // Ignore MMIO windows above RAM (e.g. the PCIe hole at 1 TiB).
            reserved = reserved.saturating_add(entry.length);
        }
    }

    let total = usable.saturating_add(reserved);
    let mapped_kb = boot_info().mapped_kb as u64;
    let mapped_end = mapped_kb.saturating_mul(1024);

    Summary {
        total_kb: total / 1024,
        usable_kb: usable / 1024,
        reserved_kb: reserved / 1024,
        mapped_kb,
        // The page tables cover mapped_end; never report a top above what is mapped.
        top_kb: usable_end.min(mapped_end) / 1024,
    }
}
