use core::ptr;

use hollowstar_common::{ArchiveEntry, COMMAND_LOAD_ADDR, COMMAND_STACK_SIZE, COMMAND_STACK_TOP};

use crate::archive;

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
    ]))
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
        *data.get(offset + 2)?,
        *data.get(offset + 3)?,
    ]))
}

fn u64_at(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
        *data.get(offset + 2)?,
        *data.get(offset + 3)?,
        *data.get(offset + 4)?,
        *data.get(offset + 5)?,
        *data.get(offset + 6)?,
        *data.get(offset + 7)?,
    ]))
}

pub fn load(entry: &ArchiveEntry) -> Option<u64> {
    let data = archive::data(entry)?;

    if data.len() < 64 || &data[0..4] != b"\x7FELF" {
        return None;
    }
    if data[4] != 2 || data[5] != 1 || data[6] != 1 {
        return None;
    }
    if u16_at(data, 16)? != 2 || u16_at(data, 18)? != 62 {
        return None;
    }
    if u32_at(data, 20)? != 1 {
        return None;
    }

    let entry_point = u64_at(data, 24)?;
    let phoff = u64_at(data, 32)? as usize;
    let phentsize = u16_at(data, 54)? as usize;
    let phnum = u16_at(data, 56)? as usize;

    if phentsize < 56 || phnum == 0 || phnum > 32 {
        return None;
    }
    let table_end = phoff.checked_add(phentsize.checked_mul(phnum)?)?;
    if table_end > data.len() {
        return None;
    }

    let stack_floor = COMMAND_STACK_TOP - COMMAND_STACK_SIZE;
    let mut loaded = false;

    for i in 0..phnum {
        let ph = phoff + i * phentsize;
        let p_type = u32_at(data, ph)?;
        if p_type != 1 {
            continue;
        }
        let p_offset = u64_at(data, ph + 8)? as usize;
        let p_vaddr = u64_at(data, ph + 16)?;
        let p_filesz = u64_at(data, ph + 32)? as usize;
        let p_memsz = u64_at(data, ph + 40)? as usize;

        if p_filesz > p_memsz {
            return None;
        }
        if p_vaddr < COMMAND_LOAD_ADDR {
            return None;
        }
        let mem_end = p_vaddr.checked_add(p_memsz as u64)?;
        if mem_end > stack_floor {
            return None;
        }
        let file_end = p_offset.checked_add(p_filesz)?;
        if file_end > data.len() {
            return None;
        }

        unsafe {
            let dest = p_vaddr as *mut u8;
            if p_filesz != 0 {
                ptr::copy_nonoverlapping(data.as_ptr().add(p_offset), dest, p_filesz);
            }
            if p_memsz > p_filesz {
                ptr::write_bytes(dest.add(p_filesz), 0, p_memsz - p_filesz);
            }
        }
        loaded = true;
    }

    if !loaded || entry_point < COMMAND_LOAD_ADDR || entry_point >= stack_floor {
        return None;
    }

    Some(entry_point)
}
