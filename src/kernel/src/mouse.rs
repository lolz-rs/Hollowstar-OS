//! PS/2 mouse driver and software cursor.
//!
//! IRQ12 receives 3-byte packets, or 4-byte packets when the device answers the
//! IntelliMouse wheel probe. The cursor is composited into the back buffer for
//! the duration of `graphics::present`, then removed.

use core::ptr::{addr_of, addr_of_mut};

use hollowstar_common::{
    MouseState, MOUSE_BUTTON_BACK, MOUSE_BUTTON_FORWARD, MOUSE_BUTTON_LEFT,
    MOUSE_BUTTON_MIDDLE, MOUSE_BUTTON_RIGHT, MOUSE_FLAG_AVAILABLE, MOUSE_FLAG_OVERFLOW,
    MOUSE_FLAG_WHEEL,
};

use crate::io::{inb, io_wait, outb};

const PS2_DATA: u16 = 0x60;
const PS2_STATUS: u16 = 0x64;
const PS2_COMMAND: u16 = 0x64;

const STATUS_OUTPUT_FULL: u8 = 1 << 0;
const STATUS_INPUT_FULL: u8 = 1 << 1;
const STATUS_AUX_DATA: u8 = 1 << 5;

const CMD_READ_CONFIG: u8 = 0x20;
const CMD_WRITE_CONFIG: u8 = 0x60;
const CMD_DISABLE_MOUSE: u8 = 0xA7;
const CMD_ENABLE_MOUSE: u8 = 0xA8;
const CMD_DISABLE_KEYBOARD: u8 = 0xAD;
const CMD_ENABLE_KEYBOARD: u8 = 0xAE;
const CMD_SEND_MOUSE: u8 = 0xD4;

const TIMEOUT: u32 = 50_000;

const CURSOR_W: i32 = 12;
const CURSOR_H: i32 = 19;
const CURSOR_CELLS: usize = (CURSOR_W * CURSOR_H) as usize;

/// 0 = transparent, 1 = white fill, 2 = black outline.
const CURSOR: [[u8; CURSOR_W as usize]; CURSOR_H as usize] = [
    [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [2, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [2, 1, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0],
    [2, 1, 1, 1, 2, 0, 0, 0, 0, 0, 0, 0],
    [2, 1, 1, 1, 1, 2, 0, 0, 0, 0, 0, 0],
    [2, 1, 1, 1, 1, 1, 2, 0, 0, 0, 0, 0],
    [2, 1, 1, 1, 1, 1, 1, 2, 0, 0, 0, 0],
    [2, 1, 1, 1, 1, 1, 1, 1, 2, 0, 0, 0],
    [2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 0, 0],
    [2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 0],
    [2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2],
    [2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 0],
    [2, 1, 1, 1, 2, 1, 1, 1, 2, 0, 0, 0],
    [2, 1, 1, 2, 0, 2, 1, 1, 2, 0, 0, 0],
    [2, 1, 2, 0, 0, 2, 1, 1, 2, 0, 0, 0],
    [2, 2, 0, 0, 0, 0, 2, 1, 2, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 2, 1, 2, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0],
];

static mut AVAILABLE: bool = false;
static mut WHEEL_SUPPORTED: bool = false;
static mut PACKET: [u8; 4] = [0; 4];
static mut PACKET_INDEX: usize = 0;
static mut PACKET_SIZE: usize = 3;

static mut X: i32 = 0;
static mut Y: i32 = 0;
static mut DX: i32 = 0;
static mut DY: i32 = 0;
static mut WHEEL: i32 = 0;
static mut BUTTONS: u32 = 0;
static mut PACKETS: u32 = 0;
static mut OVERFLOW: bool = false;
static mut DIRTY: bool = false;

static mut CURSOR_VISIBLE: bool = false;
static mut CURSOR_SAVE: [u32; CURSOR_CELLS] = [0; CURSOR_CELLS];
static mut CURSOR_SAVE_X: i32 = 0;
static mut CURSOR_SAVE_Y: i32 = 0;
static mut CURSOR_SAVE_W: i32 = 0;
static mut CURSOR_SAVE_H: i32 = 0;

#[inline]
pub fn available() -> bool {
    unsafe { AVAILABLE }
}

#[inline]
pub fn wheel_supported() -> bool {
    unsafe { WHEEL_SUPPORTED }
}

fn wait_input_clear() -> bool {
    let mut i = 0;
    while i < TIMEOUT {
        if unsafe { inb(PS2_STATUS) } & STATUS_INPUT_FULL == 0 {
            return true;
        }
        unsafe { io_wait() };
        i += 1;
    }
    false
}

fn wait_output_full() -> bool {
    let mut i = 0;
    while i < TIMEOUT {
        if unsafe { inb(PS2_STATUS) } & STATUS_OUTPUT_FULL != 0 {
            return true;
        }
        unsafe { io_wait() };
        i += 1;
    }
    false
}

fn read_data_timeout() -> Option<u8> {
    if !wait_output_full() {
        return None;
    }
    Some(unsafe { inb(PS2_DATA) })
}

fn flush_output() {
    let mut i = 0;
    while i < 64 {
        if unsafe { inb(PS2_STATUS) } & STATUS_OUTPUT_FULL == 0 {
            break;
        }
        let _ = unsafe { inb(PS2_DATA) };
        i += 1;
    }
}

fn controller_command(command: u8) -> bool {
    if !wait_input_clear() {
        return false;
    }
    unsafe {
        outb(PS2_COMMAND, command);
    }
    true
}

fn controller_write_data(value: u8) -> bool {
    if !wait_input_clear() {
        return false;
    }
    unsafe {
        outb(PS2_DATA, value);
    }
    true
}

fn send_mouse_byte(byte: u8) -> bool {
    if !controller_command(CMD_SEND_MOUSE) {
        return false;
    }
    controller_write_data(byte)
}

/// Send a byte and wait for ACK, retrying up to three times on a `0xFE` resend.
fn mouse_command(byte: u8) -> bool {
    let mut attempt = 0;
    while attempt < 3 {
        if send_mouse_byte(byte) {
            match read_data_timeout() {
                Some(0xFA) => return true,
                Some(0xFE) => {}
                _ => {}
            }
        }
        attempt += 1;
    }
    false
}

fn mouse_command_arg(command: u8, value: u8) -> bool {
    mouse_command(command) && mouse_command(value)
}

fn mouse_get_id() -> Option<u8> {
    if !mouse_command(0xF2) {
        return None;
    }
    read_data_timeout()
}

/// IntelliMouse probe: 200/100/80 sample rates, then `Get Device ID`.
/// ID 3 adds a wheel, ID 4 adds a wheel and back/forward buttons.
fn detect_wheel() -> bool {
    if !mouse_command_arg(0xF3, 200) {
        return false;
    }
    if !mouse_command_arg(0xF3, 100) {
        return false;
    }
    if !mouse_command_arg(0xF3, 80) {
        return false;
    }
    matches!(mouse_get_id(), Some(3) | Some(4))
}

fn controller_init() -> bool {
    if !controller_command(CMD_DISABLE_KEYBOARD) {
        return false;
    }
    if !controller_command(CMD_DISABLE_MOUSE) {
        return false;
    }
    flush_output();

    if !controller_command(CMD_READ_CONFIG) {
        return false;
    }
    let Some(config) = read_data_timeout() else {
        return false;
    };
    // Enable the AUX interrupt and clear clock inhibit; other bits stay as-is.
    let config = (config | 0x02) & !0x20;
    if !controller_command(CMD_WRITE_CONFIG) || !controller_write_data(config) {
        return false;
    }
    if !controller_command(CMD_ENABLE_MOUSE) {
        return false;
    }
    true
}

fn enable_keyboard() {
    let _ = controller_command(CMD_ENABLE_KEYBOARD);
}

pub fn init() {
    unsafe {
        AVAILABLE = false;
        WHEEL_SUPPORTED = false;
        PACKET_INDEX = 0;
        PACKET_SIZE = 3;
        DX = 0;
        DY = 0;
        WHEEL = 0;
        BUTTONS = 0;
        PACKETS = 0;
        OVERFLOW = false;
        DIRTY = false;
        CURSOR_VISIBLE = false;
    }

    if !controller_init() {
        enable_keyboard();
        return;
    }

    // Drop stale bytes (such as a power-on ID) before the command handshake.
    flush_output();

    if !mouse_command(0xF6) {
        enable_keyboard();
        return;
    }

    let wheel = detect_wheel();
    let configured = mouse_command_arg(0xF3, 100)
        && mouse_command_arg(0xE8, 3)
        && mouse_command(0xE6)
        && mouse_command(0xF4);
    if !configured {
        enable_keyboard();
        return;
    }

    let (width, height) = screen_size();
    unsafe {
        PACKET_SIZE = if wheel { 4 } else { 3 };
        WHEEL_SUPPORTED = wheel;
        X = width / 2;
        Y = height / 2;
        AVAILABLE = true;
    }
    crate::pic::unmask(12);
    enable_keyboard();
}

fn screen_size() -> (i32, i32) {
    if crate::graphics::active() {
        (
            crate::graphics::width().max(1) as i32,
            crate::graphics::height().max(1) as i32,
        )
    } else {
        (640, 480)
    }
}

fn translate_buttons(byte0: u8, byte3: u8) -> u32 {
    let mut buttons = 0u32;
    if byte0 & 0x01 != 0 {
        buttons |= MOUSE_BUTTON_LEFT;
    }
    if byte0 & 0x04 != 0 {
        buttons |= MOUSE_BUTTON_MIDDLE;
    }
    if byte0 & 0x02 != 0 {
        buttons |= MOUSE_BUTTON_RIGHT;
    }
    if byte3 & 0x10 != 0 {
        buttons |= MOUSE_BUTTON_BACK;
    }
    if byte3 & 0x20 != 0 {
        buttons |= MOUSE_BUTTON_FORWARD;
    }
    buttons
}

fn process_packet() {
    let packet = unsafe { *addr_of_mut!(PACKET) };
    let flags = packet[0];
    let mut dx = packet[1] as i32;
    let mut dy = packet[2] as i32;
    if flags & 0x10 != 0 {
        dx -= 256;
    }
    if flags & 0x20 != 0 {
        dy -= 256;
    }

    let overflow = (flags & 0xC0) != 0;
    if overflow {
        dx = 0;
        dy = 0;
    }

    let mut wheel_delta = 0i32;
    let buttons = unsafe {
        if PACKET_SIZE == 4 {
            let z = (packet[3] & 0x0F) as i32;
            wheel_delta = if z & 0x08 != 0 { z - 16 } else { z };
            translate_buttons(flags, packet[3])
        } else {
            translate_buttons(flags, 0)
        }
    };

    unsafe {
        DX = dx;
        DY = dy;
        if wheel_delta != 0 {
            WHEEL = WHEEL.saturating_add(wheel_delta);
        }
        if overflow {
            OVERFLOW = true;
        }
        if dx != 0 || dy != 0 {
            let (width, height) = screen_size();
            X = X.saturating_add(dx).clamp(0, width - 1);
            // PS/2 reports positive Y as up; screen Y grows downward.
            Y = Y.saturating_sub(dy).clamp(0, height - 1);
            DIRTY = true;
        }
        if buttons != BUTTONS {
            BUTTONS = buttons;
            DIRTY = true;
        }
        PACKETS = PACKETS.saturating_add(1);
    }
}

fn handle_byte(byte: u8) {
    unsafe {
        if !AVAILABLE {
            return;
        }
        let index = PACKET_INDEX;
        if index == 0 && (byte & 0x08) == 0 {
            // Lost packet synchronization: wait for the next valid first byte.
            PACKET_INDEX = 0;
            return;
        }
        PACKET[index] = byte;
        PACKET_INDEX = index + 1;
        if PACKET_INDEX >= PACKET_SIZE {
            PACKET_INDEX = 0;
            process_packet();
        }
    }
}

/// IRQ12: consume AUX bytes only, so keyboard scancodes stay for IRQ1.
pub fn handle_irq() {
    let status = unsafe { inb(PS2_STATUS) };
    if status & STATUS_OUTPUT_FULL == 0 || status & STATUS_AUX_DATA == 0 {
        // Keyboard byte, or nothing pending.
        return;
    }
    let byte = unsafe { inb(PS2_DATA) };
    handle_byte(byte);
}

pub fn state() -> MouseState {
    let mut state = MouseState::zeroed();
    // Snapshot under cli so a packet cannot land halfway through the copy.
    let flags = unsafe { crate::io::read_rflags() };
    unsafe {
        crate::io::disable_interrupts();
        if AVAILABLE {
            state.x = X;
            state.y = Y;
            state.dx = DX;
            state.dy = DY;
            state.wheel = WHEEL;
            state.buttons = BUTTONS;
            state.packets = PACKETS;
            state.flags = MOUSE_FLAG_AVAILABLE;
            if wheel_supported() {
                state.flags |= MOUSE_FLAG_WHEEL;
            }
            if OVERFLOW {
                state.flags |= MOUSE_FLAG_OVERFLOW;
            }
        }
        if (flags & 0x200) != 0 {
            crate::io::enable_interrupts();
        }
    }
    state
}

/// Repaint the console when the cursor moved and no command owns the screen.
pub fn service() {
    if !available() || !crate::graphics::active() || crate::graphics::cmd_active() {
        return;
    }
    let dirty = unsafe { DIRTY };
    if dirty {
        unsafe {
            DIRTY = false;
        }
        crate::console::flush();
    }
}

/// Forget the saved pixels before the back buffer is cleared.
pub fn cursor_reset() {
    unsafe {
        CURSOR_VISIBLE = false;
        CURSOR_SAVE_W = 0;
        CURSOR_SAVE_H = 0;
    }
}

/// Composite the cursor into the back buffer, saving the covered pixels.
pub fn cursor_draw() {
    unsafe {
        if !AVAILABLE || CURSOR_VISIBLE || !crate::graphics::active() {
            return;
        }
        let info = crate::graphics::info();
        if info.width == 0 || info.height == 0 || info.pitch < info.width * 4 {
            return;
        }
        let width = info.width as i32;
        let height = info.height as i32;
        let x = X.clamp(0, width - 1);
        let y = Y.clamp(0, height - 1);
        let w = CURSOR_W.min(width - x);
        let h = CURSOR_H.min(height - y);
        if w <= 0 || h <= 0 {
            return;
        }

        // If the arrow moved, repaint the pixels it covered on the screen.
        // cursor_restore() already put the original pixels back in the back
        // buffer; marking the old rectangle makes the next present copy them.
        if CURSOR_SAVE_W > 0
            && CURSOR_SAVE_H > 0
            && (CURSOR_SAVE_X != x || CURSOR_SAVE_Y != y)
        {
            crate::graphics::mark_dirty(
                CURSOR_SAVE_X as u32,
                CURSOR_SAVE_Y as u32,
                CURSOR_SAVE_W as u32,
                CURSOR_SAVE_H as u32,
            );
        }

        let stride = (info.pitch / 4) as usize;
        let base = info.back_buffer as *mut u32;
        let save = addr_of_mut!(CURSOR_SAVE) as *mut u32;
        let mut row = 0;
        while row < h {
            let mut col = 0;
            while col < w {
                *save.add(row as usize * CURSOR_W as usize + col as usize) = *base.add(
                    (y + row) as usize * stride + (x + col) as usize,
                );
                col += 1;
            }
            row += 1;
        }
        CURSOR_SAVE_X = x;
        CURSOR_SAVE_Y = y;
        CURSOR_SAVE_W = w;
        CURSOR_SAVE_H = h;

        let white = crate::graphics::rgb(255, 255, 255);
        let black = crate::graphics::rgb(0, 0, 0);
        row = 0;
        while row < h {
            let mut col = 0;
            while col < w {
                let pixel = CURSOR[row as usize][col as usize];
                if pixel != 0 {
                    *base.add((y + row) as usize * stride + (x + col) as usize) =
                        if pixel == 1 { white } else { black };
                }
                col += 1;
            }
            row += 1;
        }
        CURSOR_VISIBLE = true;
        crate::graphics::mark_dirty(x as u32, y as u32, w as u32, h as u32);
    }
}

/// Current cursor rectangle, or `None` when the mouse is unavailable.
pub fn cursor_bounds() -> Option<(u32, u32, u32, u32)> {
    unsafe {
        if !AVAILABLE || !crate::graphics::active() {
            return None;
        }
        let info = crate::graphics::info();
        if info.width == 0 || info.height == 0 {
            return None;
        }
        let width = info.width as i32;
        let height = info.height as i32;
        let x = X.clamp(0, width - 1);
        let y = Y.clamp(0, height - 1);
        let w = CURSOR_W.min(width - x);
        let h = CURSOR_H.min(height - y);
        if w <= 0 || h <= 0 {
            None
        } else {
            Some((x as u32, y as u32, w as u32, h as u32))
        }
    }
}

/// Restore the pixels saved by `cursor_draw`.
pub fn cursor_restore() {
    unsafe {
        if !CURSOR_VISIBLE {
            return;
        }
        let info = crate::graphics::info();
        let stride = (info.pitch / 4) as usize;
        let base = info.back_buffer as *mut u32;
        let save = addr_of!(CURSOR_SAVE) as *const u32;
        let mut row = 0;
        while row < CURSOR_SAVE_H {
            let mut col = 0;
            while col < CURSOR_SAVE_W {
                *base.add(
                    (CURSOR_SAVE_Y + row) as usize * stride
                        + (CURSOR_SAVE_X + col) as usize,
                ) = *save.add(row as usize * CURSOR_W as usize + col as usize);
                col += 1;
            }
            row += 1;
        }
        CURSOR_VISIBLE = false;
    }
}
