#![no_std]
#![no_main]
#![allow(static_mut_refs)]

//! A small nano-style editor for the RAM disk.
//!
//! `nano notes.txt` opens that file (or starts a new one). The arrow keys move
//! the caret, Home/End jump to the ends of a line, Page Up/Page Down move a
//! screen at a time, Ctrl+K cuts a line, Ctrl+U pastes it, Ctrl+S saves, and
//! Ctrl+X exits. Ctrl+C aborts without saving.

use hollowstar as hs;

const MAX: usize = 32 * 1024;
const STATUS_MAX: usize = 56;

static mut TEXT: [u8; MAX] = [0; MAX];
static mut LEN: usize = 0;
static mut CUR: usize = 0;
static mut DIRTY: bool = false;
static mut PENDING_QUIT: bool = false;
static mut SAVED_ONCE: bool = false;
static mut NAME: [u8; hs::FS_MAX_NAME] = [0; hs::FS_MAX_NAME];
static mut NAME_LEN: usize = 0;
static mut STATUS: [u8; STATUS_MAX] = [0; STATUS_MAX];
static mut STATUS_LEN: usize = 0;
static mut CUT: [u8; MAX] = [0; MAX];
static mut CUT_LEN: usize = 0;

fn name() -> &'static str {
    unsafe { core::str::from_utf8(&NAME[..NAME_LEN]).unwrap_or("untitled.txt") }
}

fn text() -> &'static [u8] {
    unsafe { &TEXT[..LEN] }
}

fn set_status(message: &str) {
    unsafe {
        let n = message.len().min(STATUS_MAX);
        STATUS[..n].copy_from_slice(&message.as_bytes()[..n]);
        STATUS_LEN = n;
    }
}

fn usage() -> ! {
    hs::write("usage: nano [file]\n");
    hs::write("  Arrows move, Home/End jump, Page Up/Down scroll.\n");
    hs::write("  Ctrl+S save, Ctrl+X exit, Ctrl+K cut line, Ctrl+U paste.\n");
    hs::write("  Ctrl+A/Ctrl+E go to the start/end of the line.\n");
    hs::write("  Ctrl+C aborts without saving.\n");
    hs::exit(0)
}

fn load(filename: &str) {
    unsafe {
        LEN = 0;
        CUR = 0;
        DIRTY = false;
        PENDING_QUIT = false;
        SAVED_ONCE = false;
        STATUS_LEN = 0;
        CUT_LEN = 0;
        let n = filename.len().min(NAME.len());
        NAME[..n].copy_from_slice(&filename.as_bytes()[..n]);
        NAME_LEN = n;
    }

    let Some(fd) = hs::fs_open(filename, hs::FS_READ) else {
        set_status("new file");
        return;
    };
    let n = unsafe { hs::fs_read_all(fd, &mut TEXT[..]).unwrap_or(0) };
    hs::fs_close(fd);
    unsafe {
        LEN = n;
    }
    if n >= MAX {
        set_status("truncated at 32 KiB");
    } else {
        set_status("opened");
    }
}

fn save() -> bool {
    let Some(fd) = hs::fs_open(name(), hs::FS_WRITE) else {
        set_status("save failed");
        return false;
    };
    let bytes = text();
    let written = hs::fs_write_all(fd, bytes).unwrap_or(0);
    hs::fs_close(fd);
    if written == bytes.len() {
        unsafe {
            DIRTY = false;
            PENDING_QUIT = false;
            SAVED_ONCE = true;
        }
        set_status("saved");
        true
    } else {
        set_status("save failed (disk full?)");
        false
    }
}

fn cursor_rc() -> (usize, usize) {
    let bytes = text();
    let cur = unsafe { CUR.min(LEN) };
    let mut row = 0usize;
    let mut col = 0usize;
    let mut i = 0usize;
    while i < cur {
        if bytes[i] == b'\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
        i += 1;
    }
    (row, col)
}

fn line_start(index: usize) -> usize {
    let bytes = text();
    let mut i = index.min(bytes.len());
    while i > 0 && bytes[i - 1] != b'\n' {
        i -= 1;
    }
    i
}

fn line_len(start: usize) -> usize {
    let bytes = text();
    let mut i = start;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i - start
}

fn move_left() {
    unsafe {
        if CUR > 0 {
            CUR -= 1;
        }
    }
}

fn move_right() {
    unsafe {
        if CUR < LEN {
            CUR += 1;
        }
    }
}

fn move_home() {
    unsafe {
        CUR = line_start(CUR);
    }
}

fn move_end() {
    unsafe {
        let start = line_start(CUR);
        CUR = start + line_len(start);
    }
}

fn move_up() {
    let cur = unsafe { CUR };
    let start = line_start(cur);
    if start == 0 {
        unsafe {
            CUR = 0;
        }
        return;
    }
    let col = cur - start;
    let prev = line_start(start - 1);
    let prev_len = line_len(prev);
    unsafe {
        CUR = prev + col.min(prev_len);
    }
}

fn move_down() {
    let bytes = text();
    let cur = unsafe { CUR };
    let start = line_start(cur);
    let col = cur - start;
    let mut next = start + line_len(start);
    if next >= bytes.len() {
        unsafe {
            CUR = LEN;
        }
        return;
    }
    next += 1;
    let next_len = line_len(next);
    unsafe {
        CUR = next + col.min(next_len);
    }
}

fn move_page(direction: i32) {
    let (_, rows) = screen();
    let count = rows.saturating_sub(2).max(1);
    let mut i = 0usize;
    while i < count {
        if direction < 0 {
            move_up();
        } else {
            move_down();
        }
        i += 1;
    }
}

fn insert(byte: u8) {
    unsafe {
        if LEN >= MAX {
            set_status("file is full");
            return;
        }
        let mut i = LEN;
        while i > CUR {
            TEXT[i] = TEXT[i - 1];
            i -= 1;
        }
        TEXT[CUR] = byte;
        LEN += 1;
        CUR += 1;
        DIRTY = true;
        PENDING_QUIT = false;
    }
}

fn backspace() {
    unsafe {
        if CUR == 0 {
            return;
        }
        let mut i = CUR - 1;
        while i + 1 < LEN {
            TEXT[i] = TEXT[i + 1];
            i += 1;
        }
        LEN -= 1;
        CUR -= 1;
        DIRTY = true;
        PENDING_QUIT = false;
    }
}

fn delete_forward() {
    unsafe {
        if CUR >= LEN {
            return;
        }
        let mut i = CUR;
        while i + 1 < LEN {
            TEXT[i] = TEXT[i + 1];
            i += 1;
        }
        LEN -= 1;
        DIRTY = true;
        PENDING_QUIT = false;
    }
}

fn cut_line() {
    unsafe {
        let start = line_start(CUR);
        let mut end = start + line_len(start);
        if end < LEN && TEXT[end] == b'\n' {
            end += 1;
        }
        let n = end - start;
        if n == 0 {
            set_status("nothing to cut");
            return;
        }
        core::ptr::copy_nonoverlapping(TEXT.as_ptr().add(start), CUT.as_mut_ptr(), n);
        CUT_LEN = n;
        let mut i = end;
        while i < LEN {
            TEXT[i - n] = TEXT[i];
            i += 1;
        }
        LEN -= n;
        CUR = start.min(LEN);
        DIRTY = true;
        PENDING_QUIT = false;
        set_status("line cut");
    }
}

fn paste() {
    unsafe {
        if CUT_LEN == 0 {
            set_status("nothing to paste");
            return;
        }
        if LEN + CUT_LEN > MAX {
            set_status("not enough room");
            return;
        }
        let n = CUT_LEN;
        let mut i = LEN;
        while i > CUR {
            TEXT[i + n - 1] = TEXT[i - 1];
            i -= 1;
        }
        core::ptr::copy_nonoverlapping(CUT.as_ptr(), TEXT.as_mut_ptr().add(CUR), n);
        LEN += n;
        CUR += n;
        DIRTY = true;
        PENDING_QUIT = false;
        set_status("pasted");
    }
}

fn screen() -> (usize, usize) {
    let (cols, rows) = hs::console_size();
    (cols.max(20), rows.max(5))
}

fn redraw() {
    let (cols, rows) = screen();
    let view_rows = rows.saturating_sub(2).max(1);
    let width = cols.saturating_sub(1).max(1);
    let (cur_row, cur_col) = cursor_rc();
    let top = if cur_row >= view_rows {
        cur_row + 1 - view_rows
    } else {
        0
    };
    let left = if cur_col >= width {
        cur_col + 1 - width
    } else {
        0
    };

    hs::clear();
    hs::write_color(hs::color::CYAN, " nano ");
    hs::write(name());
    if unsafe { DIRTY } {
        hs::write_color(hs::color::YELLOW, "  [modified]");
    }
    hs::write("\n");

    let bytes = text();
    let mut line = 0usize;
    let mut i = 0usize;
    while line < top && i < bytes.len() {
        if bytes[i] == b'\n' {
            line += 1;
        }
        i += 1;
    }

    let mut shown = 0usize;
    while shown < view_rows {
        if i >= bytes.len() {
            hs::write("\n");
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            let end = i;
            let from = start.saturating_add(left).min(end);
            let to = start.saturating_add(left).saturating_add(width).min(end);
            if from < to {
                hs::write_bytes(&bytes[from..to]);
            }
            hs::write("\n");
            if i < bytes.len() && bytes[i] == b'\n' {
                i += 1;
            }
        }
        shown += 1;
    }

    let compact = cols < 100;
    let mut used: usize = if compact { 16 } else { 52 };
    if compact {
        hs::write_color(hs::color::DARK_GRAY, "^S save  ^X exit  ");
    } else {
        hs::write_color(
            hs::color::DARK_GRAY,
            "^S save  ^X exit  ^K cut  ^U paste  ^A/^E line  ",
        );
    }
    hs::print!("Ln {}, Col {}", cur_row + 1, cur_col + 1);
    used += 20;
    if unsafe { DIRTY } {
        hs::write_color(hs::color::YELLOW, "  modified");
        used += 10;
    }
    unsafe {
        if STATUS_LEN > 0 {
            let room = cols.saturating_sub(used + 2);
            let n = STATUS_LEN.min(room);
            if n > 0 {
                hs::write("  ");
                hs::write(core::str::from_utf8(&STATUS[..n]).unwrap_or(""));
            }
        }
    }

    let x = cur_col.saturating_sub(left).min(width.saturating_sub(1));
    let y = (1 + cur_row.saturating_sub(top)).min(rows.saturating_sub(2));
    hs::set_cursor(x, y);
}

fn finish() -> ! {
    hs::clear();
    if unsafe { DIRTY } {
        hs::write("nano: discarded ");
    } else if unsafe { SAVED_ONCE } {
        hs::write("nano: saved ");
    } else {
        hs::write("nano: closed ");
    }
    hs::write(name());
    hs::write("\n");
    hs::exit(0)
}

#[no_mangle]
pub extern "C" fn _start(args_ptr: *const u8, args_len: usize) -> ! {
    let args = hs::args_str(args_ptr, args_len).trim();
    if args == "-h" || args == "--help" || args == "help" {
        usage();
    }
    let filename = if args.is_empty() { "untitled.txt" } else { args };
    if filename.len() > hs::FS_MAX_NAME
        || filename
            .bytes()
            .any(|b| !b.is_ascii_graphic() || b == b'/' || b == b'>')
    {
        hs::write_err("nano: bad file name\n");
        hs::exit(1);
    }

    load(filename);
    redraw();

    loop {
        let key = hs::getchar();
        if key != 0x18 {
            unsafe {
                PENDING_QUIT = false;
            }
        }
        match key {
            0x13 | 0x0F => {
                save();
            }
            0x18 => {
                if unsafe { DIRTY && !PENDING_QUIT } {
                    unsafe {
                        PENDING_QUIT = true;
                    }
                    set_status("unsaved changes: ^X again to discard");
                } else {
                    finish();
                }
            }
            0x01 => move_home(),
            0x05 => move_end(),
            0x0B => cut_line(),
            0x15 => paste(),
            0x80 => move_up(),
            0x81 => move_down(),
            0x82 => move_left(),
            0x83 => move_right(),
            0x84 => move_home(),
            0x85 => move_end(),
            0x86 => move_page(-1),
            0x87 => move_page(1),
            0x7F => delete_forward(),
            0x08 => backspace(),
            b'\t' => insert(b'\t'),
            b'\n' | b'\r' => insert(b'\n'),
            0x20..=0x7E => insert(key),
            _ => {}
        }
        redraw();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hs::write_err("nano: panic\n");
    hs::exit(1)
}
