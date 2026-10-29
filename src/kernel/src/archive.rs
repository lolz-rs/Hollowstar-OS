use core::mem::size_of;
use core::slice;

use hollowstar_common::{
    ArchiveEntry, ArchiveHeader, ARCHIVE_MAGIC, ARCHIVE_VERSION, MAX_COMMANDS,
};

extern "C" {
    static commands_archive_start: u8;
    static commands_archive_end: u8;
}

fn bytes() -> &'static [u8] {
    unsafe {
        let start = &commands_archive_start as *const u8;
        let end = &commands_archive_end as *const u8;
        if end <= start {
            return &[];
        }
        slice::from_raw_parts(start, end as usize - start as usize)
    }
}

fn header() -> Option<&'static ArchiveHeader> {
    let data = bytes();
    if data.len() < size_of::<ArchiveHeader>() {
        return None;
    }
    let header = unsafe { &*(data.as_ptr() as *const ArchiveHeader) };
    if header.magic != ARCHIVE_MAGIC || header.version != ARCHIVE_VERSION {
        return None;
    }
    if header.count as usize > MAX_COMMANDS {
        return None;
    }
    Some(header)
}

pub fn init() -> bool {
    let Some(header) = header() else {
        return false;
    };
    let data = bytes();
    let table_end = size_of::<ArchiveHeader>() + header.count as usize * size_of::<ArchiveEntry>();
    if table_end > data.len() {
        return false;
    }
    for i in 0..header.count as usize {
        let Some(entry) = entry(i) else {
            return false;
        };
        let offset = entry.offset as usize;
        let size = entry.size as usize;
        if offset < table_end || offset.saturating_add(size) > data.len() {
            return false;
        }
    }
    true
}

pub fn count() -> usize {
    header().map(|h| h.count as usize).unwrap_or(0)
}

pub fn entry(index: usize) -> Option<&'static ArchiveEntry> {
    let header = header()?;
    if index >= header.count as usize {
        return None;
    }
    let data = bytes();
    let offset = size_of::<ArchiveHeader>() + index * size_of::<ArchiveEntry>();
    if offset + size_of::<ArchiveEntry>() > data.len() {
        return None;
    }
    Some(unsafe { &*(data.as_ptr().add(offset) as *const ArchiveEntry) })
}

pub fn name_bytes(entry: &'static ArchiveEntry) -> &'static [u8] {
    let end = entry
        .name
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(entry.name.len());
    &entry.name[..end]
}

pub fn description_str(entry: &'static ArchiveEntry) -> &'static str {
    let end = entry
        .description
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(entry.description.len());
    core::str::from_utf8(&entry.description[..end]).unwrap_or("")
}

pub fn data(entry: &ArchiveEntry) -> Option<&'static [u8]> {
    let archive = bytes();
    let offset = entry.offset as usize;
    let size = entry.size as usize;
    let end = offset.checked_add(size)?;
    if end > archive.len() {
        return None;
    }
    Some(&archive[offset..end])
}

pub fn find(name: &[u8]) -> Option<&'static ArchiveEntry> {
    for i in 0..count() {
        let entry = entry(i)?;
        if name_bytes(entry) == name {
            return Some(entry);
        }
    }
    None
}

#[allow(dead_code)]
pub fn archive_size() -> usize {
    bytes().len()
}
