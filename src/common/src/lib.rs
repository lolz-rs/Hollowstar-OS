#![no_std]

pub const VERSION_MAJOR: u32 = 0;
pub const VERSION_MINOR: u32 = 1;
pub const VERSION_PATCH: u32 = 0;
pub const VERSION_STRING: &str = "0.1.0";

pub const HOSTNAME: &str = "Hollowstar";

pub const BOOT_INFO_ADDR: u64 = 0x5000;
pub const MEMMAP_ADDR: u64 = 0x5200;
pub const BOOT_MAGIC: u32 = 0x5254_5348; // "HSTR"
pub const BOOT_VERSION: u32 = 2;

/// Back buffer lives in the low 16 MiB of RAM, after the kernel and command
/// image. Graphics is disabled when the firmware mode or RAM check fails.
pub const BACK_BUFFER_ADDR: u64 = 0x0100_0000;
pub const GFX_FLAG_AVAILABLE: u32 = 1 << 0;
pub const GFX_FLAG_LINEAR: u32 = 1 << 1;
pub const GFX_FLAG_FONT: u32 = 1 << 2;

pub const ARCHIVE_MAGIC: u32 = 0x4D43_5348; // "HSCM"
pub const ARCHIVE_VERSION: u32 = 1;
pub const MAX_COMMANDS: usize = 64;

pub const COMMAND_LOAD_ADDR: u64 = 0x0080_0000;
pub const COMMAND_STACK_TOP: u64 = 0x00C0_0000;
pub const COMMAND_STACK_SIZE: u64 = 0x0001_0000;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootInfo {
    pub magic: u32,
    pub version: u32,
    pub kernel_lba: u64,
    pub kernel_sectors: u32,
    pub boot_drive: u32,
    pub kernel_load: u64,
    pub kernel_entry: u64,
    pub memmap_addr: u64,
    pub memmap_count: u32,
    pub memmap_entry_size: u32,
    pub kernel_size: u32,
    /// Identity-mapped RAM in KiB, written by stage2 after building the page tables.
    pub mapped_kb: u32,
    /// Linear framebuffer chosen by the VBE BIOS (0 when unavailable).
    pub framebuffer_addr: u64,
    pub framebuffer_pitch: u32,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    pub framebuffer_bpp: u32,
    pub framebuffer_red_mask: u32,
    pub framebuffer_green_mask: u32,
    pub framebuffer_blue_mask: u32,
    pub graphics_flags: u32,
    /// BIOS 8x16 font copied to low memory by stage2.
    pub font_addr: u64,
    pub font_width: u32,
    pub font_height: u32,
    pub back_buffer_addr: u64,
    pub back_buffer_size: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryMapEntry {
    pub base: u64,
    pub length: u64,
    pub kind: u32,
    pub attributes: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ArchiveHeader {
    pub magic: u32,
    pub version: u32,
    pub count: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ArchiveEntry {
    pub name: [u8; 32],
    pub description: [u8; 64],
    pub offset: u32,
    pub size: u32,
    pub load_addr: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SystemInfo {
    pub uptime_ms: u64,
    pub mem_total_kb: u64,
    pub mem_usable_kb: u64,
    pub mem_reserved_kb: u64,
    pub mem_mapped_kb: u64,
    pub mem_top_kb: u64,
    pub command_count: u32,
    pub boot_drive: u32,
    pub cpu_vendor: [u8; 12],
    pub cpu_brand: [u8; 48],
    pub cpu_family: u32,
    pub cpu_model: u32,
    pub cpu_stepping: u32,
    pub kernel_size: u64,
    pub version_major: u32,
    pub version_minor: u32,
    pub version_patch: u32,
    pub reserved: u32,
}

/// Graphics state shared with commands. Commands draw into `back_buffer`
/// using `pitch` bytes per scanline and 32-bit pixels, then ask the kernel to
/// present the completed frame.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GraphicsInfo {
    pub back_buffer: u64,
    pub framebuffer: u64,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u32,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub framebuffer_pitch: u32,
    pub font_addr: u64,
    pub font_width: u32,
    pub font_height: u32,
    pub text_cols: u32,
    pub text_rows: u32,
    pub flags: u32,
    pub reserved: u32,
}

impl GraphicsInfo {
    pub const fn zeroed() -> Self {
        Self {
            back_buffer: 0,
            framebuffer: 0,
            width: 0,
            height: 0,
            pitch: 0,
            bpp: 0,
            red_mask: 0,
            green_mask: 0,
            blue_mask: 0,
            framebuffer_pitch: 0,
            font_addr: 0,
            font_width: 0,
            font_height: 0,
            text_cols: 0,
            text_rows: 0,
            flags: 0,
            reserved: 0,
        }
    }
}

/// PS/2 mouse state shared with commands. `x`/`y` are absolute screen
/// coordinates, `dx`/`dy` the last packet's movement, and `wheel` an
/// accumulated tick count.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub dx: i32,
    pub dy: i32,
    pub wheel: i32,
    pub buttons: u32,
    pub packets: u32,
    pub flags: u32,
}

impl MouseState {
    pub const fn zeroed() -> Self {
        Self {
            x: 0,
            y: 0,
            dx: 0,
            dy: 0,
            wheel: 0,
            buttons: 0,
            packets: 0,
            flags: 0,
        }
    }
}

pub const MOUSE_FLAG_AVAILABLE: u32 = 1 << 0;
pub const MOUSE_FLAG_WHEEL: u32 = 1 << 1;
pub const MOUSE_FLAG_OVERFLOW: u32 = 1 << 2;

pub const MOUSE_BUTTON_LEFT: u32 = 1 << 0;
pub const MOUSE_BUTTON_MIDDLE: u32 = 1 << 1;
pub const MOUSE_BUTTON_RIGHT: u32 = 1 << 2;
pub const MOUSE_BUTTON_BACK: u32 = 1 << 3;
pub const MOUSE_BUTTON_FORWARD: u32 = 1 << 4;

impl SystemInfo {
    pub const fn zeroed() -> Self {
        Self {
            uptime_ms: 0,
            mem_total_kb: 0,
            mem_usable_kb: 0,
            mem_reserved_kb: 0,
            mem_mapped_kb: 0,
            mem_top_kb: 0,
            command_count: 0,
            boot_drive: 0,
            cpu_vendor: [0; 12],
            cpu_brand: [0; 48],
            cpu_family: 0,
            cpu_model: 0,
            cpu_stepping: 0,
            kernel_size: 0,
            version_major: 0,
            version_minor: 0,
            version_patch: 0,
            reserved: 0,
        }
    }
}

// Syscall numbers. Keep in sync with the kernel dispatcher.
pub const SYS_WRITE: u64 = 0;
pub const SYS_READ_LINE: u64 = 1;
pub const SYS_EXIT: u64 = 2;
pub const SYS_CLEAR: u64 = 3;
pub const SYS_INFO: u64 = 4;
pub const SYS_UPTIME_MS: u64 = 5;
pub const SYS_SLEEP: u64 = 6;
pub const SYS_REBOOT: u64 = 7;
pub const SYS_HALT: u64 = 8;
pub const SYS_GETCHAR: u64 = 9;
pub const SYS_PUTCHAR: u64 = 10;
pub const SYS_WRITE_COLOR: u64 = 11;
pub const SYS_CMD_COUNT: u64 = 12;
pub const SYS_CMD_GET: u64 = 13;
pub const SYS_SET_COLOR: u64 = 14;
pub const SYS_GET_COLOR: u64 = 15;
pub const SYS_GFX_INFO: u64 = 16;
pub const SYS_GFX_ENTER: u64 = 17;
pub const SYS_GFX_LEAVE: u64 = 18;
pub const SYS_GFX_PRESENT: u64 = 19;
pub const SYS_GFX_VSYNC: u64 = 20;
pub const SYS_AUDIO_TONE: u64 = 21;
pub const SYS_AUDIO_OFF: u64 = 22;
pub const SYS_AUDIO_BEEP: u64 = 23;
pub const SYS_KEY_POLL: u64 = 24;
pub const SYS_MOUSE_STATE: u64 = 25;
