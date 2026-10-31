//! In-memory flat file store.
//!
//! Files live in fixed slots after the console scrollback and before the
//! command load address. Nothing is written to disk, so a reboot clears them.
//! One command runs at a time, so the open file table is global and dropped
//! when the command finishes.

use hollowstar_common::{
    FS_APPEND, FS_MAX_FILES, FS_MAX_FILE_BYTES, FS_MAX_NAME, FS_MAX_OPEN, FS_READ, FS_WRITE,
};

const BASE: usize = 0x0050_0000;
const SLOT: usize = FS_MAX_FILE_BYTES;

struct File {
    used: bool,
    name: [u8; FS_MAX_NAME],
    name_len: u8,
    size: u32,
}

struct Open {
    used: bool,
    index: usize,
    pos: u32,
    readable: bool,
    writable: bool,
}

static mut FILES: [File; FS_MAX_FILES] = [const { File::empty() }; FS_MAX_FILES];
static mut OPENS: [Open; FS_MAX_OPEN] = [const { Open::empty() }; FS_MAX_OPEN];
static mut REDIRECT: u64 = 0;

impl File {
    const fn empty() -> Self {
        Self {
            used: false,
            name: [0; FS_MAX_NAME],
            name_len: 0,
            size: 0,
        }
    }
}

impl Open {
    const fn empty() -> Self {
        Self {
            used: false,
            index: 0,
            pos: 0,
            readable: false,
            writable: false,
        }
    }
}

fn slot(index: usize) -> *mut u8 {
    (BASE + index * SLOT) as *mut u8
}

fn valid_name(name: &[u8]) -> bool {
    if name.is_empty() || name.len() > FS_MAX_NAME {
        return false;
    }
    name.iter()
        .all(|b| b.is_ascii_graphic() && *b != b'/' && *b != b'>')
}

fn find(name: &[u8]) -> Option<usize> {
    unsafe {
        let mut i = 0;
        while i < FS_MAX_FILES {
            if FILES[i].used
                && FILES[i].name_len as usize == name.len()
                && &FILES[i].name[..name.len()] == name
            {
                return Some(i);
            }
            i += 1;
        }
    }
    None
}

fn alloc_slot() -> Option<usize> {
    unsafe {
        let mut i = 0;
        while i < FS_MAX_FILES {
            if !FILES[i].used {
                return Some(i);
            }
            i += 1;
        }
    }
    None
}

fn free_fd() -> Option<usize> {
    unsafe {
        let mut i = 0;
        while i < FS_MAX_OPEN {
            if !OPENS[i].used {
                return Some(i);
            }
            i += 1;
        }
    }
    None
}

pub fn init() {
    unsafe {
        let mut i = 0;
        while i < FS_MAX_FILES {
            FILES[i] = File::empty();
            i += 1;
        }
    }
    command_done();
    let hello = b"Welcome to Hollowstar.\n\
\n\
This is a RAM disk, so files disappear when you reboot.\n\
\n\
Try:\n\
  ls\n\
  echo hello > note.txt\n\
  cat note.txt\n\
  nano note.txt\n";
    let _ = create_bytes(b"welcome.txt", hello);
}

fn create_bytes(name: &[u8], data: &[u8]) -> bool {
    let fd = open(name, FS_WRITE);
    if fd == 0 {
        return false;
    }
    let mut off = 0usize;
    while off < data.len() {
        let n = write(fd, &data[off..]);
        if n == 0 || n == u64::MAX {
            close(fd);
            return false;
        }
        off += n as usize;
    }
    close(fd);
    true
}

/// Open a file. Returns a 1-based descriptor, or 0 on failure.
///
/// `FS_WRITE` creates the file when needed and truncates an existing one
/// unless `FS_APPEND` is also set. `FS_APPEND` implies write access.
pub fn open(name: &[u8], flags: u64) -> u64 {
    if !valid_name(name) {
        return 0;
    }
    let read = (flags & FS_READ) != 0;
    let append = (flags & FS_APPEND) != 0;
    let write = (flags & FS_WRITE) != 0 || append;
    if !read && !write {
        return 0;
    }

    // Reserve a descriptor first so a full handle table cannot create or
    // truncate a file as a side effect of a failed open.
    let Some(fd_index) = free_fd() else {
        return 0;
    };

    let (index, created) = match find(name) {
        Some(index) => (index, false),
        None if write => {
            let Some(index) = alloc_slot() else {
                return 0;
            };
            unsafe {
                FILES[index] = File::empty();
                FILES[index].used = true;
                FILES[index].name[..name.len()].copy_from_slice(name);
                FILES[index].name_len = name.len() as u8;
            }
            (index, true)
        }
        None => return 0,
    };

    unsafe {
        if !created && write && !append {
            FILES[index].size = 0;
        }
        let size = FILES[index].size;
        OPENS[fd_index] = Open {
            used: true,
            index,
            pos: if append { size } else { 0 },
            readable: read,
            writable: write,
        };
    }
    (fd_index + 1) as u64
}

pub fn close(fd: u64) {
    let Some(slot) = fd_slot(fd) else {
        return;
    };
    unsafe {
        OPENS[slot] = Open::empty();
        if REDIRECT == fd {
            REDIRECT = 0;
        }
    }
}

fn fd_slot(fd: u64) -> Option<usize> {
    if fd == 0 || fd > FS_MAX_OPEN as u64 {
        return None;
    }
    let slot = fd as usize - 1;
    unsafe {
        if OPENS[slot].used {
            Some(slot)
        } else {
            None
        }
    }
}

/// Read up to `dst.len()` bytes. Returns 0 at end of file and `u64::MAX` for
/// a bad descriptor or a descriptor that was not opened for reading.
pub fn read(fd: u64, dst: &mut [u8]) -> u64 {
    if dst.is_empty() {
        return 0;
    }
    let Some(slot_i) = fd_slot(fd) else {
        return u64::MAX;
    };
    unsafe {
        let open = &mut OPENS[slot_i];
        if !open.readable {
            return u64::MAX;
        }
        let file = &FILES[open.index];
        if open.pos >= file.size {
            return 0;
        }
        let left = (file.size - open.pos) as usize;
        let n = left.min(dst.len());
        let src = slot(open.index).add(open.pos as usize);
        core::ptr::copy_nonoverlapping(src, dst.as_mut_ptr(), n);
        open.pos += n as u32;
        n as u64
    }
}

/// Write up to `src.len()` bytes. Returns the number written, 0 when the file
/// is full, and `u64::MAX` for a bad descriptor or a read-only descriptor.
pub fn write(fd: u64, src: &[u8]) -> u64 {
    if src.is_empty() {
        return 0;
    }
    let Some(slot_i) = fd_slot(fd) else {
        return u64::MAX;
    };
    unsafe {
        let open = &mut OPENS[slot_i];
        if !open.writable {
            return u64::MAX;
        }
        let room = SLOT.saturating_sub(open.pos as usize);
        let n = room.min(src.len());
        if n == 0 {
            return 0;
        }
        let dst = slot(open.index).add(open.pos as usize);
        core::ptr::copy_nonoverlapping(src.as_ptr(), dst, n);
        open.pos += n as u32;
        if open.pos > FILES[open.index].size {
            FILES[open.index].size = open.pos;
        }
        n as u64
    }
}

pub fn count() -> u64 {
    let mut n = 0u64;
    unsafe {
        let mut i = 0;
        while i < FS_MAX_FILES {
            if FILES[i].used {
                n += 1;
            }
            i += 1;
        }
    }
    n
}

/// Copy the name of used file `nth` (0-based among used files). Returns 1 and
/// writes the size when `size_ptr` is non-zero.
pub fn list(nth: u64, name_ptr: u64, name_cap: u64, size_ptr: u64) -> u64 {
    if name_ptr == 0 || name_cap == 0 {
        return 0;
    }
    let mut seen = 0u64;
    unsafe {
        let mut i = 0;
        while i < FS_MAX_FILES {
            if FILES[i].used {
                if seen == nth {
                    let cap = name_cap as usize;
                    let n = (FILES[i].name_len as usize).min(cap.saturating_sub(1));
                    let dst = name_ptr as *mut u8;
                    core::ptr::copy_nonoverlapping(FILES[i].name.as_ptr(), dst, n);
                    *dst.add(n) = 0;
                    if size_ptr != 0 {
                        *(size_ptr as *mut u32) = FILES[i].size;
                    }
                    return 1;
                }
                seen += 1;
            }
            i += 1;
        }
    }
    0
}

pub fn remove(name: &[u8]) -> u64 {
    let Some(index) = find(name) else {
        return 0;
    };
    unsafe {
        FILES[index] = File::empty();
        let mut i = 0;
        while i < FS_MAX_OPEN {
            if OPENS[i].used && OPENS[i].index == index {
                OPENS[i] = Open::empty();
            }
            i += 1;
        }
    }
    1
}

pub fn set_redirect(fd: u64) {
    unsafe {
        REDIRECT = fd;
    }
}

pub fn redirect_write(bytes: &[u8]) -> Option<u64> {
    unsafe {
        if REDIRECT == 0 {
            None
        } else {
            Some(write(REDIRECT, bytes))
        }
    }
}

pub fn redirect_byte(byte: u8) -> bool {
    unsafe {
        if REDIRECT == 0 {
            false
        } else {
            write(REDIRECT, &[byte]);
            true
        }
    }
}

/// Drop all descriptors and the current redirection. Called after every
/// command, including aborted ones, so a command cannot leak handles.
pub fn command_done() {
    unsafe {
        REDIRECT = 0;
        let mut i = 0;
        while i < FS_MAX_OPEN {
            OPENS[i] = Open::empty();
            i += 1;
        }
    }
}
