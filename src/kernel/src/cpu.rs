use core::arch::x86_64::{__cpuid, __cpuid_count};

use hollowstar_common::SystemInfo;

pub fn vendor() -> [u8; 12] {
    let leaf = __cpuid(0);
    let mut out = [0u8; 12];
    out[0..4].copy_from_slice(&leaf.ebx.to_le_bytes());
    out[4..8].copy_from_slice(&leaf.edx.to_le_bytes());
    out[8..12].copy_from_slice(&leaf.ecx.to_le_bytes());
    out
}

pub fn brand() -> [u8; 48] {
    let mut out = [0u8; 48];
    let max = __cpuid(0x8000_0000).eax;
    if max >= 0x8000_0004 {
        for i in 0..3 {
            let leaf = __cpuid(0x8000_0002 + i);
            let base = (i as usize) * 16;
            out[base..base + 4].copy_from_slice(&leaf.eax.to_le_bytes());
            out[base + 4..base + 8].copy_from_slice(&leaf.ebx.to_le_bytes());
            out[base + 8..base + 12].copy_from_slice(&leaf.ecx.to_le_bytes());
            out[base + 12..base + 16].copy_from_slice(&leaf.edx.to_le_bytes());
        }
    } else {
        let v = vendor();
        out[..v.len()].copy_from_slice(&v);
    }
    trim_ascii(&mut out);
    out
}

fn trim_ascii(bytes: &mut [u8]) {
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == 0 || bytes[end - 1] == b' ') {
        end -= 1;
    }
    for byte in &mut bytes[end..] {
        *byte = 0;
    }
}

pub fn family_model_stepping() -> (u32, u32, u32) {
    let leaf = __cpuid(1);
    let stepping = leaf.eax & 0x0F;
    let mut family = (leaf.eax >> 8) & 0x0F;
    let mut model = (leaf.eax >> 4) & 0x0F;
    if family == 0x0F {
        family += (leaf.eax >> 20) & 0xFF;
    }
    if family == 0x06 || family == 0x0F {
        model += ((leaf.eax >> 16) & 0x0F) << 4;
    }
    (family, model, stepping)
}

pub fn fill_info(info: &mut SystemInfo) {
    info.cpu_vendor = vendor();
    info.cpu_brand = brand();
    let (family, model, stepping) = family_model_stepping();
    info.cpu_family = family;
    info.cpu_model = model;
    info.cpu_stepping = stepping;
}

#[allow(dead_code)]
pub fn has_sse2() -> bool {
    let leaf = __cpuid(1);
    (leaf.edx & (1 << 26)) != 0
}

#[allow(dead_code)]
pub fn has_avx() -> bool {
    let leaf = __cpuid(1);
    (leaf.ecx & (1 << 28)) != 0
}

#[allow(dead_code)]
pub fn has_x2apic() -> bool {
    let leaf = __cpuid_count(1, 0);
    (leaf.ecx & (1 << 21)) != 0
}
