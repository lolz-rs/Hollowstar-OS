# Hollowstar

Hollowstar is a small x86_64 operating system that boots from its own
bootloader into a command-line shell. There's no GRUB and no Linux
underneath: the bootloader, kernel, drivers, and commands all live in this
repository.

It's a work in progress. There's no filesystem, networking, or user mode yet,
but the parts that are here work and are tested under QEMU.

```
Hollowstar> fetch
            #             Hollowstar 0.1.0
           # #            kernel   0.1.0 x86_64
          #   #           uptime   0s
         #     #          cpu      QEMU Virtual CPU version 2.5+
#########       ######### memory   511 MiB
 ##                   ##  commands 19
   ##               ##
     ##           ##
       #    #    #
      #   ## ##   #
      #  #     ## #
     # ##        # #
     ##           ##
    #               #
```

## What it does

- Boots from a 512-byte MBR or an El Torito ISO. Testing happens under QEMU;
  the ISO uses the same BIOS path VMware and VirtualBox take.
- Runs a 1024×768×32 VBE console, with 800×600 and 640×480 fallbacks and an
  80×25 VGA text mode when VBE is not available.
- Loads commands as separate Rust ELF binaries from an embedded `/commands`
  archive.
- Reads a PS/2 keyboard and mouse, including the wheel and the back/forward
  buttons.
- Plays tones through the PC speaker.
- Identity-maps all usable RAM plus the framebuffer, up to 512 GiB.

Source is in `src/` (bootloader, kernel, and the shared ABI), commands are in
`commands/`, and the Python build and test scripts are in `tools/`.

## Build

The toolchains are too large to commit, so put them under `tools/` first:

| Tool | Path |
| --- | --- |
| NASM 3.x | `tools/nasm-3.02/nasm.exe` |
| Clang/LLVM | `tools/llvm/bin/clang.exe` |
| QEMU | `tools/qemu/qemu-system-x86_64.exe` |
| Rust + Cargo | `%USERPROFILE%\.cargo\bin\cargo.exe` |
| Python 3 | `python` on `PATH` |

You also need the bare-metal Rust target:

```powershell
rustup target add x86_64-unknown-none
```

Then build from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\build.ps1
```

Everything lands in `build/`:

| File | What it is |
| --- | --- |
| `hollowstar.iso` | Bootable El Torito ISO |
| `hollowstar.img` | 16 MiB raw BIOS/MBR disk image |
| `Hollowstar.vmdk` | VMware monolithic-sparse disk |
| `Hollowstar.vmx` | VMware VM definition (8 GiB RAM, CD first, then IDE disk) |
| `kernel.bin` | Flat kernel, loaded at `0x100000` |
| `commands.hsb` | Packed `/commands` archive |

The kernel gets relinked on every build. The command archive and the C/NASM
objects are linker inputs that Cargo cannot see.

## Run

```powershell
# raw disk image, VGA window, PC speaker audio
tools\qemu\qemu-system-x86_64.exe -m 512 `
    -drive format=raw,file=build\hollowstar.img,if=ide,index=0 -boot c `
    -audiodev dsound,id=snd0 -machine pc,pcspk-audiodev=snd0

# ISO with 8 GiB of guest RAM
tools\qemu\qemu-system-x86_64.exe -m 8192 -cdrom build\hollowstar.iso -boot d `
    -audiodev dsound,id=snd0 -machine pc,pcspk-audiodev=snd0

# serial console instead of a window
tools\qemu\qemu-system-x86_64.exe -m 512 `
    -drive format=raw,file=build\hollowstar.img,if=ide,index=0 -boot c `
    -display none -serial stdio
```

The audio flags are only there to make `beep` audible. Drop them for a silent
VM.

For VMware, open `build/Hollowstar.vmx` or attach `build/hollowstar.iso` to a
new "Other 64-bit" VM with BIOS firmware and the CD-ROM first in the boot
order. VirtualBox takes the same ISO. The boot image is self-contained:
firmware loads it at `0x7C00`, the boot sector jumps to stage2 at `0x8000`,
and the kernel is already at `0xC000`, so nothing is read from the CD after
that.

A serial console is available on COM1 at 115200 8N1. VGA, PS/2 keyboard, and
PS/2 mouse need no extra configuration.

## Commands

Everything lives in `/commands`, a read-only archive baked into the kernel.

| Command | Description |
| --- | --- |
| `fetch` | system summary |
| `echo` | print arguments; `-n` skips the newline |
| `help` | list commands and descriptions |
| `commands` | list names in `/commands` |
| `clear` | clear the screen |
| `uname` | name and version |
| `uptime` | time since boot |
| `meminfo` | memory summary |
| `memtest` | write and check a pattern near the top of usable RAM |
| `cpuinfo` | CPU details |
| `version` | version string |
| `gfxinfo` | framebuffer, back buffer, and console details |
| `gfxdemo [seconds]` | animated graphics demo; ESC or Q quits |
| `beep [hz] [ms]` | PC speaker tone, or `beep melody` |
| `mouse` | mouse position, deltas, buttons, wheel, packet count |
| `mousedemo` | button indicators, drawing canvas, palette; left draws, right clears, middle changes the brush |
| `sleep ms` | wait |
| `reboot` | restart |
| `halt` | stop the machine |

Aliases: `fastfetch`/`neofetch` for `fetch`, `shutdown` for `halt`, and `exit`
for `halt`.

## How it boots

1. **MBR** (`src/bootloader/boot.asm`). Sets up segments, checks for INT 13h
   LBA support, reads 32 sectors from LBA 2 to `0x8000`, and jumps there.
2. **ISO boot sector** (`src/bootloader/iso_boot.asm`). Firmware loads the
   whole El Torito image at `0x7C00`; this sector jumps to the stage2 copy at
   `0x8000`.
3. **Stage2** (`src/bootloader/stage2.asm`). Turns on A20, collects the E820
   map, and asks VBE for a 32-bit linear mode: 1024×768 first, then 800×600,
   then 640×480. It copies the BIOS 8×16 font to `0x9E000` and loads the
   kernel, either from the disk image or from the boot image on the ISO path.
   Then it enters protected mode, moves the kernel to `0x100000`, builds
   identity page tables for all usable RAM and the framebuffer, marks the
   framebuffer write-combining through the PAT, enables long mode, and calls
   the kernel with the boot-info pointer in `RDI`.
4. **Kernel** (`src/kernel`). Zeroes BSS, sets up GDT/TSS, IDT, PIC, and PIT,
   starts the speaker and mouse drivers, loads the command archive, brings up
   the console, and runs the shell.

If VBE fails, the graphics fields stay zeroed and the kernel keeps the VGA text
console.

### Disk layout

| LBA | Contents |
| --- | --- |
| 0 | MBR boot sector |
| 1 | Boot info (136 bytes, magic `HSTR`, version 2) |
| 2–33 | Stage2 (16 KiB max) |
| 64+ | Flat kernel |

### ISO layout

| ISO sector | Contents |
| --- | --- |
| 16 | Primary volume descriptor |
| 17 | El Torito boot record |
| 18 | Volume descriptor terminator |
| 19 | Boot catalog |
| 20–21 | Path tables (little and big endian) |
| 22 | Root directory (`BOOT.IMG;1`) |
| 23+ | Boot image |

Inside the boot image, the boot sector sits at `0x0000` (loaded at `0x7C00`),
stage2 at `0x0400` (loaded at `0x8000`), and the kernel at `0x4400` (loaded at
`0xC000`). The catalog load segment is `0x07C0`, and the sector count is
rounded up to whole 2048-byte sectors.

### Memory layout

| Address | Use |
| --- | --- |
| `0x1000` | PML4 |
| `0x2000` | PDPT |
| `0x5000` | Boot info (136 bytes) |
| `0x5200` | E820 map (up to 64 × 24-byte entries) |
| `0x5900` | VBE controller info (512 bytes) |
| `0x5C00` | VBE mode info (256 bytes) |
| `0x8000` | Stage2 |
| `0x10000` | Kernel staging area |
| `0x50000` | 80×25 text shadow for the monitor dump |
| `0x90000` | Kernel stack (grows down) |
| `0x9E000` | BIOS 8×16 font copy |
| `0x100000` | Kernel load address |
| `0x200000`–`0x400000` | Page-directory area (512 × 4 KiB) |
| `0x700000` | Double-fault IST stack |
| `0x800000` | Command load address |
| `0xC00000` | Command stack top (64 KiB) |
| `0x01000000` | Graphics back buffer (pitch × height, up to 3 MiB) |

Stage2 rounds the top of usable RAM up to a whole GiB, allocates one page
directory per GiB (capped at 512 GiB), and fills it with 2 MiB identity pages.
If VBE gave us a framebuffer, the map is extended to cover it and those PDEs
get the PAT bit, with `IA32_PAT` entry 4 set to write-combining.

`meminfo` reports the E820 total and usable amounts plus how much is actually
mapped. `memtest` writes and verifies a pattern near the top of usable RAM. On
a 16 GiB guest, `mapped` reads 17.0 GiB because the identity map also covers
the 1 GiB PCI hole between low and high RAM; usable RAM is still 16.0 GiB.

## Graphics and sound

Stage2 takes the first 32-bit direct-colour linear mode it can get. The kernel
checks the mode before switching the console: linear flag, 32 bpp, sane pitch
and size, a usable back buffer, and a readable 8×16 font. If any of that is
wrong, it stays in 80×25 VGA text.

The graphics console keeps a cell buffer and draws glyphs from the BIOS font.
It also mirrors the 80×25 text grid to `0x50000`, so the test harness can dump
text even when the legacy `0xB8000` window is inactive in VBE mode. Frames are
composed in the back buffer at `0x01000000`; `SYS_GFX_PRESENT` waits for
vertical retrace and copies the result with `rep movsq`. Commands call
`SYS_GFX_ENTER`, draw with `commands/lib/src/gfx.rs`, and hand the screen back
with `SYS_GFX_LEAVE`.

The PC speaker runs from PIT channel 2 and is gated through port `0x61`, so it
needs no sound-card driver. QEMU only emulates it when an audio backend is
attached: the run commands above pass `-audiodev dsound,id=snd0` and
`-machine pc,pcspk-audiodev=snd0`. Swap `dsound` for `sdl` or `wav` if you
prefer. VMware and VirtualBox route the speaker through their own audio
settings. `gfxdemo` plays a slow arpeggio; `beep` is a standalone test.

## Mouse

The driver probes the 8042 auxiliary port, enables IRQ12, and starts with the
standard 3-byte packet protocol. It also runs the IntelliMouse 200/100/80
sample-rate sequence; if the device answers with ID 3 or 4, it switches to
4-byte packets and the wheel and side buttons start working. Packets are
checked for the sync bit, overflow bits are recorded, deltas are signed 8-bit
values, and the position is clamped to the framebuffer. The wheel count
accumulates.

The cursor is a 12×19 white-on-black arrow. During `graphics::present()` the
kernel saves the pixels underneath it, draws the arrow, copies the frame, and
then restores the saved pixels, so commands never see the cursor in their own
drawing surface. `graphics::clear()` resets the saved area, and the shell's
idle loop repaints the console when the mouse moves, which makes the arrow
work over the text console too.

`SYS_MOUSE_STATE` returns a `MouseState` with the absolute position, the last
delta, accumulated wheel ticks, button bits, packet count, and feature flags.
`commands/lib` wraps it as `mouse_state()` and `mouse_available()`.

## How commands run

Commands are ELF64 `ET_EXEC` binaries linked at `0x800000`. The shell looks up
the name in the archive, loads the `PT_LOAD` segments (zeroing BSS), switches
to the command stack, and calls the entry point with `(args_ptr, args_len)`.
A command exits through `int 0x80` with `SYS_EXIT`; the kernel abandons the
command stack and resumes the shell.

Syscalls use `int 0x80` with the number in `RAX` and arguments in `RDI`,
`RSI`, `RDX`, `R10`, `R8`. The ABI lives in `src/common/src/lib.rs` and is
shared by the kernel and commands. Graphics, audio, and mouse calls are
`SYS_GFX_*`, `SYS_AUDIO_*`, and `SYS_MOUSE_STATE`, plus `SYS_KEY_POLL` for
non-blocking input.

## Tests

The harnesses boot real QEMU instances:

```powershell
# raw disk image
python tools/test_qemu.py --qemu tools/qemu/qemu-system-x86_64.exe `
    --image build/hollowstar.img --vga-out build/vga.bin

# ISO, 8 GiB RAM
python tools/test_qemu.py --qemu tools/qemu/qemu-system-x86_64.exe `
    --image build/hollowstar.iso --vga-out build/vga-iso.bin `
    --cdrom --memory 8192

python tools/test_iso.py
python tools/test_extra.py
python tools/test_vmdk.py
python tools/test_gfx_modes.py
python tools/test_audio.py
python tools/test_mouse.py --qemu tools/qemu/qemu-system-x86_64.exe `
    --image build/hollowstar.img --out-prefix build/test_mouse
python tools/test_ps2_shared.py
```

| Script | What it checks |
| --- | --- |
| `test_qemu.py` | boots the raw image or ISO, runs every command over COM1, checks the output, and verifies the screen after `clear` |
| `test_iso.py` | parses the ISO structures without booting |
| `test_extra.py` | PS/2 keyboard input, history recall, reboot, and aliases |
| `test_vmdk.py` | boots the generated VMware disk |
| `test_gfx_modes.py` | forces the 800×600 and VGA text fallbacks |
| `test_audio.py` | captures a WAV and checks the 880 Hz and 440 Hz tones |
| `test_mouse.py` | cursor, movement, button indicators, canvas, and wheel |
| `test_ps2_shared.py` | keyboard and mouse traffic interleaved |

`test_qemu.py` is the broad one. It dumps the text console through the QEMU
monitor (the shadow at `0x50000` in graphics mode, `0xB8000` in VGA fallback),
parks the cursor in the top-left with `mouse_move`, takes a screendump right
after `clear`, and requires the lower half of the framebuffer to be black. That
catches row-stride bugs without mistaking the cursor for stale pixels.
`--cdrom` switches to the ISO path and `--memory` sets guest RAM.

You can read the text dumps directly:

```powershell
python tools/show_vga.py build/vga_ram.bin   # graphics-mode shadow
python tools/show_vga.py build/vga.bin       # VGA text fallback
```

Boots take about 0.2 seconds under QEMU here. The ISO has run with 512 MiB,
8 GiB, and 16 GiB of RAM; `meminfo` and `memtest` agree at each size.
`gfxdemo 1` renders around 75 frames per second with no panics.

One quirk worth knowing: the serial harness writes a byte every 2 ms. QEMU's
stdio backend delivers a whole `write()` to the guest at once, which would
overrun the 16-byte UART FIFO. Pacing the input mimics a real 115200-baud line.

## Limitations

- No filesystem, networking, user mode, or multitasking. Commands run in ring 0
  on a dedicated stack.
- `/commands` is a read-only archive inside the kernel. Adding a command means
  adding a crate under `commands/` and rebuilding.
- Audio is PC speaker only. No sound card, mixer, or streaming.
- Mouse support is PS/2 only. No USB HID, no hot-plug, no wheel acceleration;
  the wheel value is a raw accumulated count.
- Graphics uses VBE BIOS modes only: 1024×768×32, 800×600×32, or 640×480×32.
  Without a suitable linear mode, the kernel falls back to 80×25 VGA text.
- VMware was not installed during development, so boot was not run in VMware
  itself. The VMDK is a valid monolithic-sparse disk and the ISO is a standard
  El Torito image; both boot under QEMU's SeaBIOS, which follows the same BIOS
  path.
