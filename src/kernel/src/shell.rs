use core::ptr::addr_of_mut;

use hollowstar_common::{color, COMMAND_STACK_TOP, VERSION_STRING};

use crate::{archive, console, elf, keyboard, println, syscall};

const LINE_MAX: usize = 256;
const HISTORY_MAX: usize = 8;
const ARGS_MAX: usize = 512;

static mut LINE: [u8; LINE_MAX] = [0; LINE_MAX];
static mut HISTORY: [[u8; LINE_MAX]; HISTORY_MAX] = [[0; LINE_MAX]; HISTORY_MAX];
static mut HISTORY_LEN: [usize; HISTORY_MAX] = [0; HISTORY_MAX];
static mut HISTORY_COUNT: usize = 0;
static mut ARGS: [u8; ARGS_MAX] = [0; ARGS_MAX];

extern "C" {
    fn run_command_asm(entry: u64, args_ptr: u64, args_len: u64, stack_top: u64);
}

fn line_ptr() -> *mut u8 {
    addr_of_mut!(LINE) as *mut u8
}

fn args_ptr() -> *mut u8 {
    addr_of_mut!(ARGS) as *mut u8
}

const PROMPT: &str = "Hollowstar> ";

fn print_prompt() {
    console::set_color(color::CYAN);
    console::write_str(PROMPT);
    console::set_color(color::WHITE);
}

fn add_history(buffer: *const u8, len: usize) {
    if len == 0 {
        return;
    }
    unsafe {
        let count = *addr_of_mut!(HISTORY_COUNT);
        if count > 0 {
            let last = count - 1;
            let last_len = (*addr_of_mut!(HISTORY_LEN))[last];
            if last_len == len {
                let hist = &*addr_of_mut!(HISTORY);
                if &hist[last][..len] == core::slice::from_raw_parts(buffer, len) {
                    return;
                }
            }
        }

        let mut count = count;
        if count == HISTORY_MAX {
            let hist = &mut *addr_of_mut!(HISTORY);
            let lens = &mut *addr_of_mut!(HISTORY_LEN);
            for i in 1..HISTORY_MAX {
                let (dst, src) = hist.split_at_mut(i);
                dst[i - 1].copy_from_slice(&src[0]);
                lens[i - 1] = lens[i];
            }
            count = HISTORY_MAX - 1;
        }

        let hist = &mut *addr_of_mut!(HISTORY);
        let lens = &mut *addr_of_mut!(HISTORY_LEN);
        hist[count][..len].copy_from_slice(core::slice::from_raw_parts(buffer, len));
        lens[count] = len;
        *addr_of_mut!(HISTORY_COUNT) = count + 1;
    }
}

fn history_copy(index: usize, dest: *mut u8) -> usize {
    unsafe {
        let hist = &*addr_of_mut!(HISTORY);
        let lens = &*addr_of_mut!(HISTORY_LEN);
        let len = lens[index];
        core::ptr::copy_nonoverlapping(hist[index].as_ptr(), dest, len);
        len
    }
}

fn caret_cell(start_x: usize, start_y: usize, caret: usize) -> (usize, usize) {
    let cols = console::columns().max(1);
    let rows = console::rows().max(1);
    let mut x = start_x;
    let mut y = start_y;
    let mut left = caret;
    while left > 0 {
        x += 1;
        if x >= cols {
            x = 0;
            y = y.saturating_add(1);
        }
        left -= 1;
    }
    if y >= rows {
        y = rows - 1;
    }
    (x, y)
}

/// Reprint the input and park the `|` caret to the right of the selected
/// character. `caret == 0` puts the bar at the start of the line.
fn show_input(
    line_row: usize,
    prompt: bool,
    buffer: *const u8,
    len: usize,
    caret: usize,
    clear_to: usize,
) -> (usize, usize) {
    // `\r` keeps a serial terminal on the same line. The graphics cursor is
    // then parked at the start of the prompt row, even if the bar had wrapped.
    console::write_byte(b'\r');
    console::set_cursor(0, line_row);
    if prompt {
        print_prompt();
    }
    let (sx, sy) = console::cursor_pos();
    for i in 0..len {
        console::write_byte(unsafe { *buffer.add(i) });
    }
    if clear_to > len {
        for _ in 0..(clear_to - len) {
            console::write_byte(b' ');
        }
    }
    let (cx, cy) = caret_cell(sx, sy, caret);
    console::set_cursor(cx, cy);
    console::flush();
    (sx, sy)
}

const ALIASES: [&[u8]; 4] = [b"exit", b"fastfetch", b"neofetch", b"shutdown"];

fn name_count() -> usize {
    archive::count() + ALIASES.len()
}

fn name_at(index: usize) -> &'static [u8] {
    let commands = archive::count();
    if index < commands {
        archive::name_bytes(archive::entry(index).unwrap())
    } else {
        ALIASES[index - commands]
    }
}

/// Complete the command being typed. Returns the new length and whether the
/// matches were printed (so the prompt has to be drawn again).
fn complete(buffer: *mut u8, len: usize) -> (usize, bool) {
    let line = unsafe { core::slice::from_raw_parts(buffer, len) };
    if line.iter().any(|&b| b == b' ' || b == b'\t') {
        return (len, false);
    }

    let mut matches = [0usize; 32];
    let mut nmatch = 0usize;
    let mut i = 0usize;
    while i < name_count() {
        let name = name_at(i);
        if name.starts_with(line) && nmatch < matches.len() {
            matches[nmatch] = i;
            nmatch += 1;
        }
        i += 1;
    }
    if nmatch == 0 {
        return (len, false);
    }
    let mut a = 0usize;
    while a < nmatch {
        let mut b = a + 1;
        while b < nmatch {
            if name_at(matches[b]) < name_at(matches[a]) {
                matches.swap(a, b);
            }
            b += 1;
        }
        a += 1;
    }
    if nmatch == 1 {
        let name = name_at(matches[0]);
        let mut new_len = name.len().min(LINE_MAX - 2);
        unsafe {
            core::ptr::copy_nonoverlapping(name.as_ptr(), buffer, new_len);
            if new_len + 1 < LINE_MAX {
                *buffer.add(new_len) = b' ';
                new_len += 1;
            }
        }
        return (new_len, false);
    }

    let first = name_at(matches[0]);
    let mut common = line.len();
    while common < first.len() {
        let byte = first[common];
        let mut ok = true;
        let mut k = 1usize;
        while k < nmatch {
            let name = name_at(matches[k]);
            if common >= name.len() || name[common] != byte {
                ok = false;
                break;
            }
            k += 1;
        }
        if !ok {
            break;
        }
        common += 1;
    }
    if common > line.len() {
        unsafe {
            core::ptr::copy_nonoverlapping(first.as_ptr(), buffer, common);
        }
        return (common, false);
    }

    console::write_byte(b'\n');
    let width = console::columns().max(16);
    let mut col = 0usize;
    let mut k = 0usize;
    while k < nmatch {
        let name = name_at(matches[k]);
        if col > 0 && col + name.len() + 2 > width {
            console::write_byte(b'\n');
            col = 0;
        } else if col > 0 {
            console::write_str("  ");
            col += 2;
        }
        console::write_bytes(name);
        col += name.len();
        k += 1;
    }
    console::write_byte(b'\n');
    (len, true)
}

pub fn read_line(prompt: bool) -> usize {
    let buffer = line_ptr();
    let mut len = 0usize;
    let mut caret = 0usize;
    let mut shown = 0usize;
    let mut hist_pos = unsafe { *addr_of_mut!(HISTORY_COUNT) };
    let mut line_row = console::cursor_pos().1;

    if prompt {
        print_prompt();
    }
    let (mut start_x, mut start_y) = console::cursor_pos();
    console::flush();

    loop {
        let byte = keyboard::read_char();
        match byte {
            b'\n' | b'\r' => {
                let (end_x, end_y) = caret_cell(start_x, start_y, len);
                console::set_cursor(end_x, end_y);
                console::write_byte(b'\n');
                console::flush();
                add_history(buffer, len);
                return len;
            }
            b'\x08' | 0x7F => {
                if caret > 0 {
                    let mut i = caret - 1;
                    while i + 1 < len {
                        unsafe {
                            *buffer.add(i) = *buffer.add(i + 1);
                        }
                        i += 1;
                    }
                    len -= 1;
                    caret -= 1;
                    let start = show_input(line_row, prompt, buffer, len, caret, shown);
                    start_x = start.0;
                    start_y = start.1;
                    shown = len;
                }
            }
            b'\x03' => {
                let (end_x, end_y) = caret_cell(start_x, start_y, len);
                console::set_cursor(end_x, end_y);
                console::write_str("^C\n");
                console::flush();
                return 0;
            }
            b'\x0C' => {
                console::clear();
                line_row = 0;
                let start = show_input(line_row, prompt, buffer, len, caret, 0);
                start_x = start.0;
                start_y = start.1;
                shown = len;
            }
            b'\x15' => {
                len = 0;
                caret = 0;
                let start = show_input(line_row, prompt, buffer, len, caret, shown);
                start_x = start.0;
                start_y = start.1;
                shown = 0;
            }
            b'\t' => {
                let old_len = len;
                let (end_x, end_y) = caret_cell(start_x, start_y, len);
                console::set_cursor(end_x, end_y);
                let (new_len, listed) = complete(buffer, len);
                len = new_len;
                caret = len;
                if listed {
                    line_row = console::cursor_pos().1;
                }
                let start = show_input(line_row, prompt, buffer, len, caret, if listed { 0 } else { shown.max(old_len) });
                start_x = start.0;
                start_y = start.1;
                shown = len;
            }
            0x80 => {
                if hist_pos > 0 {
                    hist_pos -= 1;
                    let old_len = len.max(shown);
                    len = history_copy(hist_pos, buffer);
                    caret = len;
                    let start = show_input(line_row, prompt, buffer, len, caret, old_len);
                    start_x = start.0;
                    start_y = start.1;
                    shown = len;
                }
            }
            0x81 => {
                if hist_pos < unsafe { *addr_of_mut!(HISTORY_COUNT) } {
                    hist_pos += 1;
                    let old_len = len.max(shown);
                    if hist_pos == unsafe { *addr_of_mut!(HISTORY_COUNT) } {
                        len = 0;
                    } else {
                        len = history_copy(hist_pos, buffer);
                    }
                    caret = len;
                    let start = show_input(line_row, prompt, buffer, len, caret, old_len);
                    start_x = start.0;
                    start_y = start.1;
                    shown = len;
                }
            }
            0x82 => {
                if caret > 0 {
                    caret -= 1;
                    let (cx, cy) = caret_cell(start_x, start_y, caret);
                    console::set_cursor(cx, cy);
                    console::flush();
                }
            }
            0x83 => {
                if caret < len {
                    caret += 1;
                    let (cx, cy) = caret_cell(start_x, start_y, caret);
                    console::set_cursor(cx, cy);
                    console::flush();
                }
            }
            0x84 | 0x85 | 0x86 | 0x87 => {}
            0x20..=0x7E => {
                if len + 1 < LINE_MAX {
                    let mut i = len;
                    while i > caret {
                        unsafe {
                            *buffer.add(i) = *buffer.add(i - 1);
                        }
                        i -= 1;
                    }
                    unsafe {
                        *buffer.add(caret) = byte;
                    }
                    len += 1;
                    caret += 1;
                    let start = show_input(line_row, prompt, buffer, len, caret, shown);
                    start_x = start.0;
                    start_y = start.1;
                    shown = len;
                }
            }
            _ => {}
        }
    }
}

pub fn read_line_into(out: &mut [u8]) -> usize {
    let max = out.len();
    if max == 0 {
        return 0;
    }
    let mut len = 0usize;
    loop {
        let byte = keyboard::read_char();
        match byte {
            b'\n' | b'\r' => {
                console::write_byte(b'\n');
                console::flush();
                return len;
            }
            b'\x08' | 0x7F => {
                if len > 0 {
                    len -= 1;
                    console::write_byte(0x08);
                    console::flush();
                }
            }
            b'\x03' => {
                console::write_str("^C\n");
                console::flush();
                return 0;
            }
            b'\x0C' => {
                console::clear();
                for i in 0..len {
                    console::write_byte(out[i]);
                }
                console::flush();
            }
            0x20..=0x7E => {
                if len + 1 < max {
                    out[len] = byte;
                    len += 1;
                    console::write_byte(byte);
                    console::flush();
                }
            }
            _ => {}
        }
    }
}

fn trim(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && (bytes[start] == b' ' || bytes[start] == b'\t') {
        start += 1;
    }
    while end > start && (bytes[end - 1] == b' ' || bytes[end - 1] == b'\t') {
        end -= 1;
    }
    &bytes[start..end]
}

fn unquote(bytes: &[u8]) -> &[u8] {
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &bytes[1..bytes.len() - 1];
        }
    }
    bytes
}

/// Split `echo hi > note.txt` into the command and an optional redirect.
/// `>` inside single or double quotes is just text.
fn split_redirect(line: &[u8]) -> (&[u8], Option<(&[u8], bool)>) {
    let mut quote = 0u8;
    let mut i = 0;
    while i < line.len() {
        let byte = line[i];
        if quote == 0 && (byte == b'"' || byte == b'\'') {
            quote = byte;
        } else if quote != 0 && byte == quote {
            quote = 0;
        } else if quote == 0 && byte == b'>' {
            let append = i + 1 < line.len() && line[i + 1] == b'>';
            let skip = if append { 2 } else { 1 };
            let filename = unquote(trim(&line[i + skip..]));
            return (trim(&line[..i]), Some((filename, append)));
        }
        i += 1;
    }
    (trim(line), None)
}

fn print_error(prefix: &str, detail: &[u8], message: &str) {
    console::set_color(color::RED);
    console::write_str("hsh: ");
    if !prefix.is_empty() {
        console::write_str(prefix);
        console::write_str(": ");
    }
    console::write_bytes(detail);
    console::write_str(message);
    console::set_color(color::WHITE);
}

fn redirect_flags(append: bool) -> u64 {
    if append {
        hollowstar_common::FS_WRITE | hollowstar_common::FS_APPEND
    } else {
        hollowstar_common::FS_WRITE
    }
}

fn execute(line: &[u8]) {
    let (line, redirect) = split_redirect(line);

    let mut start = 0usize;
    while start < line.len() && (line[start] == b' ' || line[start] == b'\t') {
        start += 1;
    }
    let mut name_end = start;
    while name_end < line.len() && line[name_end] != b' ' && line[name_end] != b'\t' {
        name_end += 1;
    }

    let mut args_start = name_end;
    while args_start < line.len() && (line[args_start] == b' ' || line[args_start] == b'\t') {
        args_start += 1;
    }
    let mut args_end = line.len();
    while args_end > args_start
        && (line[args_end - 1] == b' ' || line[args_end - 1] == b'\t')
    {
        args_end -= 1;
    }

    let mut name = &line[start..name_end];
    let args = &line[args_start..args_end];

    // `> file` with no command is a valid way to create or empty a file.
    if name.is_empty() {
        if let Some((filename, append)) = redirect {
            if filename.is_empty() {
                print_error("", b"", "missing file name after >\n");
                return;
            }
            let fd = crate::ramdisk::open(filename, redirect_flags(append));
            if fd == 0 {
                print_error("", filename, ": cannot open for writing\n");
                return;
            }
            crate::ramdisk::close(fd);
        }
        crate::ramdisk::command_done();
        return;
    }

    if name == &b"fastfetch"[..] || name == &b"neofetch"[..] {
        name = &b"fetch"[..];
    } else if name == &b"shutdown"[..] {
        name = &b"halt"[..];
    } else if name == &b"exit"[..] {
        console::write_str("System halted.\n");
        syscall::halt_now();
    }

    // Resolve the command before touching the redirect target. A typo like
    // `nosuchcmd > notes.txt` must not create or truncate notes.txt.
    let Some(entry) = archive::find(name) else {
        print_error("", name, ": command not found\n");
        crate::ramdisk::command_done();
        return;
    };

    if let Some((filename, append)) = redirect {
        if filename.is_empty() {
            print_error("", b"", "missing file name after >\n");
            crate::ramdisk::command_done();
            return;
        }
        let fd = crate::ramdisk::open(filename, redirect_flags(append));
        if fd == 0 {
            print_error("", filename, ": cannot open for writing\n");
            crate::ramdisk::command_done();
            return;
        }
        crate::ramdisk::set_redirect(fd);
    }

    let args_buffer = args_ptr();
    let args_len = args.len().min(ARGS_MAX - 1);
    unsafe {
        core::ptr::copy_nonoverlapping(args.as_ptr(), args_buffer, args_len);
    }

    let Some(entry_point) = elf::load(entry) else {
        print_error("", name, ": failed to load from /commands\n");
        crate::ramdisk::command_done();
        return;
    };

    unsafe {
        run_command_asm(
            entry_point,
            args_buffer as u64,
            args_len as u64,
            COMMAND_STACK_TOP,
        );
    }
    syscall::command_finished();
    console::finish_command();
    console::flush();
}

pub fn run() -> ! {
    console::clear();
    console::set_color(color::WHITE);
    println!("Hollowstar {} (x86_64)", VERSION_STRING);
    console::set_color(color::DARK_GRAY);
    println!("type help");
    console::set_color(color::WHITE);
    println!();

    loop {
        console::ensure_newline();
        let len = read_line(true);
        if len > 0 {
            let buffer = line_ptr();
            let line = unsafe { core::slice::from_raw_parts(buffer, len) };
            execute(line);
        }
    }
}
