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

fn print_prompt() {
    console::set_color(color::CYAN);
    console::write_str("Hollowstar> ");
    console::set_color(color::WHITE);
    console::flush();
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

fn redraw(prompt: bool, old_len: usize, new_len: usize, buffer: *const u8) {
    console::write_byte(b'\r');
    if prompt {
        print_prompt();
    }
    for i in 0..new_len {
        console::write_byte(unsafe { *buffer.add(i) });
    }
    if old_len > new_len {
        for _ in 0..(old_len - new_len) {
            console::write_byte(b' ');
        }
        for _ in 0..(old_len - new_len) {
            console::write_byte(0x08);
        }
    }
}

pub fn read_line(prompt: bool) -> usize {
    let buffer = line_ptr();
    let mut len = 0usize;
    let mut hist_pos = unsafe { *addr_of_mut!(HISTORY_COUNT) };

    if prompt {
        print_prompt();
    }

    loop {
        let byte = keyboard::read_char();
        match byte {
            b'\n' | b'\r' => {
                console::write_byte(b'\n');
                console::flush();
                add_history(buffer, len);
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
                if prompt {
                    print_prompt();
                }
                for i in 0..len {
                    console::write_byte(unsafe { *buffer.add(i) });
                }
                console::flush();
            }
            b'\x15' => {
                while len > 0 {
                    len -= 1;
                    console::write_byte(0x08);
                }
                console::flush();
            }
            0x80 => {
                if hist_pos > 0 {
                    hist_pos -= 1;
                    let old_len = len;
                    len = history_copy(hist_pos, buffer);
                    redraw(prompt, old_len, len, buffer);
                    console::flush();
                }
            }
            0x81 => {
                if hist_pos < unsafe { *addr_of_mut!(HISTORY_COUNT) } {
                    hist_pos += 1;
                    let old_len = len;
                    if hist_pos == unsafe { *addr_of_mut!(HISTORY_COUNT) } {
                        len = 0;
                    } else {
                        len = history_copy(hist_pos, buffer);
                    }
                    redraw(prompt, old_len, len, buffer);
                    console::flush();
                }
            }
            0x82 | 0x83 | 0x84 | 0x85 => {}
            0x20..=0x7E => {
                if len + 1 < LINE_MAX {
                    unsafe {
                        *buffer.add(len) = byte;
                    }
                    len += 1;
                    console::write_byte(byte);
                    console::flush();
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

fn execute(line: &[u8]) {
    let mut start = 0usize;
    while start < line.len() && (line[start] == b' ' || line[start] == b'\t') {
        start += 1;
    }
    if start >= line.len() {
        return;
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

    if name == &b"fastfetch"[..] || name == &b"neofetch"[..] {
        name = &b"fetch"[..];
    } else if name == &b"shutdown"[..] {
        name = &b"halt"[..];
    } else if name == &b"exit"[..] {
        console::write_str("System halted.\n");
        syscall::halt_now();
    }

    let Some(entry) = archive::find(name) else {
        console::write_str("hsh: ");
        console::write_bytes(name);
        console::write_str(": command not found in /commands\n");
        return;
    };

    let args_buffer = args_ptr();
    let args_len = args.len().min(ARGS_MAX - 1);
    unsafe {
        core::ptr::copy_nonoverlapping(args.as_ptr(), args_buffer, args_len);
    }

    let Some(entry_point) = elf::load(entry) else {
        console::write_str("hsh: ");
        console::write_bytes(name);
        console::write_str(": failed to load from /commands\n");
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
        let len = read_line(true);
        if len > 0 {
            let buffer = line_ptr();
            let line = unsafe { core::slice::from_raw_parts(buffer, len) };
            execute(line);
        }
    }
}
