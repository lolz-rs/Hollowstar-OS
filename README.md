# Hollowstar

Hollowstar is a small bare-metal x86_64 operating system that boots from a
custom bootloader straight into a command-line shell. There is no GRUB, no
Limine, and no host OS underneath: the bootloader, kernel, drivers, and
commands are all built from this repository.

- **Bootloader:** 512-byte MBR + 16 KiB stage2 (NASM), real mode → protected
  mode → long mode. A second stage2 build boots from a self-contained El Torito
  ISO, so the same OS runs on QEMU, VMware, VirtualBox, and real BIOS firmware.
- **Kernel:** Rust `no_std` with C (VGA text console, string primitives) and
  NASM (entry, IDT stubs, command trampoline, embedded archive).
- **Commands:** separate Rust ELF binaries stored in an embedded `/commands`
  archive and loaded on demand.
- **Console:** 1024×768×32 VBE linear framebuffer with a double-buffered,
  vsync-presented software text console and an 80×25 VGA text fallback. Output
  is mirrored to COM1 serial at 115200 8N1.
- **Graphics:** a small software drawing library (pixels, rects, lines, circles,
  polygons, text, and hue/brightness helpers) shared by commands. Frames render
  into a 3 MiB back buffer at `0x01000000` and are copied to the framebuffer
  during vertical retrace, so updates do not tear.
- **Audio:** PC speaker via PIT channel 2 and port `0x61`, with tone, off, and
  blocking beep syscalls.
- **Mouse:** PS/2 auxiliary device on IRQ12 with standard 3-byte packets and
  optional IntelliMouse 4-byte wheel/extra-button packets. A 12×19 arrow cursor
  is composited into the back buffer during present and then removed, so
  commands keep an uncorrupted drawing surface.
- **Memory:** page tables are built from the firmware E820 map, identity-mapping
  every usable byte of RAM up to 512 GiB with 2 MiB pages. The framebuffer
  aperture is mapped too and marked write-combining through the PAT. Any VM
  memory size works; 8 GiB and 16 GiB are covered by the automated tests.
- **Prompt:** `Hollowstar>` · **Hostname:** `Hollowstar`

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

## Requirements

The build script expects the toolchains in this repository's `tools/`
directory (they are not checked in because of their size):

| Tool | Expected path |
| --- | --- |
| NASM 3.x | `tools/nasm-3.02/nasm.exe` |
| Clang/LLVM | `tools/llvm/bin/clang.exe` |
| QEMU | `tools/qemu/qemu-system-x86_64.exe` |
| Rust + Cargo | `%USERPROFILE%\.cargo\bin\cargo.exe` |
| Python 3 | `python` on `PATH` |

The Rust target must be installed:

```powershell
rustup target add x86_64-unknown-none
```

## Build

From the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\build.ps1
```

Outputs in `build/`:

| File | Purpose |
| --- | --- |
| `hollowstar.iso` | Bootable El Torito ISO (any VM software) |
| `hollowstar.img` | 16 MiB raw BIOS/MBR disk image |
| `Hollowstar.vmdk` | VMware monolithic-sparse disk |
| `Hollowstar.vmx` | VMware VM definition (8 GiB RAM, CD-ROM first, then IDE disk) |
| `kernel.bin` | Flat kernel (load address `0x100000`) |
| `commands.hsb` | Packed `/commands` archive |

The build compiles each command crate, packs the archive, assembles the kernel
objects, links and flattens the kernel, then writes the disk image, the ISO,
and the VMware artifacts. The kernel is relinked on every build because the
command archive and the NASM/C objects are external linker inputs that Cargo
cannot track.

## Repository layout

| Path | Contents |
| --- | --- |
| `src/bootloader` | MBR, ISO boot sector, and stage2 (NASM) |
| `src/kernel` | Kernel (Rust, C, NASM) |
| `src/common` | Boot info and syscall ABI shared with commands |
| `commands` | Command crates and the shared `hollowstar` library |
| `tools/*.py` | Image, ISO, archive, and test scripts |
| `build.ps1` | Build entry point |

## Run

### QEMU

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

The audio options are what make `beep` audible; drop them for a silent VM.

### VMware / VirtualBox / other VMs

Open `build/Hollowstar.vmx` in VMware Workstation/Player (or Fusion on macOS).
The VM boots the ISO from the CD-ROM drive first and falls back to the IDE disk.
To use the ISO on its own, create a new "Other 64-bit" VM with BIOS firmware,
attach `build/hollowstar.iso` as the CD/DVD image, and set the CD-ROM first in
the boot order. VirtualBox and other hypervisors accept the same ISO.

The ISO is a standard ISO9660 volume with an El Torito no-emulation boot
catalog. The boot image is self-contained: firmware loads it at `0x7C00`, the
boot sector jumps to stage2 at `0x8000`, and the kernel is already at `0xC000`.
No CD reads happen after the initial load, which is why the image is portable.

A serial console is available on COM1 (115200 8N1) if you add one to the VM;
VGA, PS/2 keyboard, and PS/2 mouse work without any extra configuration.

## Commands

All commands live in `/commands` (an embedded read-only archive; there is no
filesystem yet).

| Command | Description |
| --- | --- |
| `fetch` | Show system information |
| `echo` | Print arguments (`echo -n` omits the newline) |
| `help` | List commands and descriptions |
| `commands` | List command names in `/commands` |
| `clear` | Clear the terminal |
| `uname` | Print system name and version |
| `uptime` | Show time since boot |
| `meminfo` | Show memory summary |
| `memtest` | Write/read a pattern at the top of usable RAM |
| `cpuinfo` | Show CPU information |
| `version` | Show Hollowstar version |
| `gfxinfo` | Show framebuffer, back buffer, and graphics-console details |
| `gfxdemo` | Graphics demo: slow rainbow strobe, spinning red square, orbits, stars, waves, colour bars, and a PC-speaker arpeggio (`gfxdemo [seconds]`, ESC/Q to exit) |
| `beep` | PC speaker test (`beep [frequency] [milliseconds]`, or `beep melody`) |
| `mouse` | Show PS/2 mouse position, deltas, buttons, wheel, and packet counters |
| `mousedemo` | Live mouse demo: button indicators, readouts, drawing canvas, colour palette (left draws, right clears, middle cycles the brush, ESC/Q exits) |
| `sleep` | Sleep for N milliseconds |
| `reboot` | Reboot the machine |
| `halt` | Halt the machine |

Aliases: `fastfetch`/`neofetch` → `fetch`, `shutdown` → `halt`,
`exit` → halt.

## Architecture

### Boot flow

1. **MBR** (`src/bootloader/boot.asm`) — sets up segments, checks INT 13h LBA
   extensions, reads 32 sectors of stage2 from LBA 2 to `0x8000`, and jumps.
2. **ISO boot sector** (`src/bootloader/iso_boot.asm`) — 512-byte El Torito
   entry point. Firmware has already loaded the whole boot image at `0x7C00`;
   this sector jumps straight to the stage2 copy at `0x8000`.
3. **Stage2** (`src/bootloader/stage2.asm`) — reads boot info (disk path),
   enables A20, collects the E820 map, asks the VBE BIOS for a 32-bit linear
   mode (1024×768, then 800×600, then 640×480), copies the BIOS 8×16 font to
   `0x9E000`, loads the flat kernel (disk path) or uses the kernel already in
   the boot image (ISO path), enters protected mode, copies the kernel to
   `0x100000`, builds identity page tables covering all usable RAM plus the
   framebuffer aperture, marks the framebuffer pages write-combining through the
   PAT, enables PAE/long mode/paging, and calls the kernel entry with the
   boot-info pointer in `RDI`. If VBE fails, the graphics fields stay zeroed and
   the kernel uses the VGA text fallback.
4. **Kernel** (`src/kernel`) — zeroes BSS, initializes GDT/TSS, IDT, PIC, PIT,
   the PC speaker, the PS/2 mouse, the command archive, the graphics console (or
   VGA text fallback), and enters the shell.

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
| 20–21 | Path tables (little/big endian) |
| 22 | Root directory (`BOOT.IMG;1`) |
| 23+ | Boot image |

Boot image offsets: boot sector at `0x0000` (loads at `0x7C00`), stage2 at
`0x0400` (loads at `0x8000`), kernel at `0x4400` (loads at `0xC000`). The
catalog load segment is `0x07C0` and its sector count is rounded up to whole
2048-byte sectors.

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
| `0x10000` | Kernel staging area (real mode / disk path) |
| `0x50000` | 80×25 text shadow for the monitor dump |
| `0x90000` | Kernel stack (grows down) |
| `0x9E000` | BIOS 8×16 font copy |
| `0x100000` | Kernel load address |
| `0x200000`–`0x400000` | Page-directory area (512 × 4 KiB) |
| `0x700000` | Double-fault IST stack |
| `0x800000` | Command load address |
| `0xC00000` | Command stack top (64 KiB) |
| `0x01000000` | Graphics back buffer (pitch × height, up to 3 MiB) |

Stage2 rounds the highest usable E820 end up to a whole GiB, allocates one page
directory per GiB (capped at 512 GiB), and fills it with 2 MiB identity pages.
If VBE selected a linear framebuffer, the page tables are extended far enough to
cover it and its PDEs get the PAT bit, with IA32_PAT entry 4 programmed as
write-combining. `meminfo` reports the E820 total/usable and the amount actually
mapped, and `memtest` writes and verifies a pattern near the top of the usable
range. On a 16 GiB guest `mapped` reads 17.0 GiB because the identity map also
covers the 1 GiB PCI hole that sits between low and high RAM; the E820 usable
total is still 16.0 GiB.

### Graphics and audio

Stage2 picks the first available 32-bit direct-colour linear mode in this order:
1024×768, 800×600, 640×480. It copies the BIOS 8×16 font to `0x9E000` before
leaving real mode. The kernel validates the mode (linear flag, 32 bpp, sane
pitch/size, a usable back-buffer range, and a readable 8×16 font) and only then
switches the console to graphics; otherwise it keeps the 80×25 VGA text console.

The graphics console keeps a cell buffer, draws glyphs from the BIOS font, and
writes an 80×25 text shadow at `0x50000` so the monitor-based dump test works
even when the legacy `0xB8000` window is inactive in VBE mode. Frames are
composed in the back buffer at `0x01000000`, then `SYS_GFX_PRESENT` waits for
vertical retrace and copies the finished buffer to the framebuffer with
`rep movsq`. Commands call `SYS_GFX_ENTER` to take over the screen, draw with
`commands/lib/src/gfx.rs`, and call `SYS_GFX_LEAVE` to restore the console.

Audio uses PIT channel 2 to generate a square wave and port `0x61` to gate it to
the PC speaker. This needs no sound-card driver, so it is the most portable
option. QEMU only emulates the speaker when an audio backend is attached, so
the run commands above pass `-audiodev dsound,id=snd0` and
`-machine pc,pcspk-audiodev=snd0`; without them `beep` succeeds but stays
silent. Swap `dsound` for `sdl` or `wav` to pick a different QEMU backend.
VirtualBox and VMware expose the PC speaker through their own audio settings.
The `gfxdemo` command plays a slow arpeggio while it runs; `beep` is a
standalone test.

### PS/2 mouse and cursor

The kernel probes the 8042 auxiliary port, enables IRQ12, and configures the
device for the standard 3-byte packet protocol. It also tries the IntelliMouse
200/100/80 sample-rate sequence; when the device reports ID 3 or 4, the driver
switches to 4-byte packets so the wheel and extra buttons work. Packet parsing
checks the first-byte sync bit, records overflow bits, handles signed 8-bit
deltas, and clamps the position to the active framebuffer. The wheel value is
accumulated rather than per-packet.

`SYS_MOUSE_STATE` returns a `MouseState` containing the absolute position, the
last packet deltas, accumulated wheel ticks, button bits, packet count, and
feature flags. `commands/lib` wraps it as `mouse_state()` and
`mouse_available()`. Button bits are `MOUSE_BUTTON_LEFT`, `MIDDLE`, `RIGHT`,
`BACK`, and `FORWARD`.

The cursor is a 12×19 white-on-black arrow. `graphics::present()` saves the
pixels underneath it, draws the arrow into the back buffer, copies the frame to
the framebuffer during vertical retrace, and then restores the saved pixels.
The overlay exists only during the copy, so command canvases stay clean.
`graphics::clear()` resets the overlay, and the shell's idle loop repaints the
console when the mouse moves, so the arrow also tracks over the text console.
`mousedemo` shows live button indicators, deltas, wheel, packet counts, a
drawing canvas, and a colour palette.

### Commands and syscalls

Commands are ELF64 `ET_EXEC` binaries linked at `0x800000`. The shell looks
up the name in the archive, loads the ELF's `PT_LOAD` segments (zeroing BSS),
switches to the command stack, and calls the entry point with `(args_ptr,
args_len)`. Commands return through `int 0x80` with `SYS_EXIT`; the kernel
abandons the command stack and resumes the shell.

Syscalls use `int 0x80` (vector 128) with the number in `RAX` and arguments in
`RDI`, `RSI`, `RDX`, `R10`, `R8`. The shared ABI lives in
`src/common/src/lib.rs` and is used by both the kernel and commands. The
graphics/audio extensions are `SYS_GFX_INFO`, `SYS_GFX_ENTER`, `SYS_GFX_LEAVE`,
`SYS_GFX_PRESENT`, `SYS_GFX_VSYNC`, `SYS_AUDIO_TONE`, `SYS_AUDIO_OFF`,
`SYS_AUDIO_BEEP`, `SYS_MOUSE_STATE`, and the non-blocking `SYS_KEY_POLL`.

## Tests

The harnesses drive real QEMU boots:

```powershell
# Raw disk image
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

`test_qemu.py` boots the image, waits for `Hollowstar>`, runs every command
over COM1 (including `gfxinfo`, `beep`, a one-second `gfxdemo`, and the `mouse`
diagnostic), checks the expected output, confirms there is no panic/exception,
dumps VGA text memory through the QEMU monitor, and verifies the post-`clear`
screen. In graphics mode the dump comes from the RAM shadow at `0x50000`; in VGA
fallback it comes from `0xB8000`. It also parks the mouse cursor in the top-left
corner with monitor `mouse_move` events, takes a screendump right after `clear`,
and requires the lower half of the framebuffer to be entirely black, which
catches row-stride bugs without the cursor being mistaken for stale pixels.
`--cdrom` switches to the El Torito path and `--memory` sets the guest RAM size.

`test_iso.py` parses the ISO without booting it: primary volume descriptor,
path tables, root directory records, El Torito boot record, catalog checksum,
load segment, and the embedded boot image (boot sector, stage2, kernel bytes).

`test_extra.py` verifies PS/2 keyboard input (via the QEMU monitor), history
recall with the up arrow, the `reboot` path (the kernel is entered twice), and
the `fastfetch`, `echo -n`, and `exit` aliases.

`test_vmdk.py` boots the generated `Hollowstar.vmdk` directly (rather than the
raw image), confirming the artifact shipped for VMware is bootable.

`test_gfx_modes.py` shrinks QEMU's VGA aperture to exercise the resolution
fallback: 2 MiB selects 800×600×32, while 1 MiB (too small for any supported
mode) leaves the kernel in the VGA text console with `gfxinfo` reporting that
no framebuffer is active.

`test_audio.py` boots with QEMU's `wav` audio backend and
`-machine pc,pcspk-audiodev=snd0`, plays 880 Hz and 440 Hz through `beep`, then
checks the captured WAV for a non-silent waveform at both frequencies. This
proves the PIT/port-`0x61` path reaches QEMU's speaker emulation rather than
only printing a confirmation line.

`test_mouse.py` boots the image, reads the initial `mouse` position, enters
`mousedemo`, and injects events through the QEMU monitor. It verifies the
software cursor at the screen centre, moves it with `mouse_move` and checks that
no trail is left, presses left/middle/right with `mouse_button` and checks the
matching indicators, confirms left-click paints the canvas and right-click
clears it, then reads the final `mouse` state over serial to confirm the packet
counter, coordinates, and wheel delta. Screendumps verify the actual cursor and
button pixels.

`test_ps2_shared.py` types a command through the PS/2 keyboard while injecting
mouse movement between keystrokes, then checks the typed text and the decoded
packet count. It covers the shared 8042 output buffer: IRQ1 must leave AUX bytes
for IRQ12, and IRQ12 must leave scancodes for IRQ1.

You can inspect the text dumps as text:

```powershell
python tools/show_vga.py build/vga_ram.bin   # graphics-mode shadow at 0x50000
python tools/show_vga.py build/vga.bin       # VGA text fallback at 0xB8000
```

On this machine the raw image, the ISO, and the VMDK all reach the prompt in
roughly 0.2 seconds under QEMU. The ISO has been booted with 512 MiB, 8 GiB, and
16 GiB of guest RAM; `meminfo` reports the expected total/usable amounts and
`memtest` passes at the top of each range. `gfxinfo` reports a 1024×768×32
linear framebuffer with a 3 MiB back buffer, and `gfxdemo 1` renders roughly 75
frames per second under QEMU with no panics.

> The serial harness writes one byte every 2 ms. QEMU's stdio backend delivers
> an entire `write()` to the guest at once, which would overrun the 16-byte
> UART FIFO; pacing the input emulates a real 115200-baud line and the PS/2
> test covers the keyboard path used by VMware.

## Limitations

- No filesystem, networking, user mode, or multitasking yet. Commands run in
  ring 0 on a dedicated stack.
- `/commands` is a read-only archive embedded in the kernel; adding a command
  means adding a crate under `commands/` and rebuilding.
- Audio is limited to the PC speaker. There is no sound-card driver, mixer, or
  streaming audio.
- Mouse support is PS/2 only. There is no USB HID driver, hot-plug detection, or
  scroll-wheel acceleration; the wheel value is a raw accumulated tick count.
- Graphics uses VBE BIOS modes only: 1024×768×32, 800×600×32, or 640×480×32. If
  the firmware exposes no suitable linear mode, the kernel falls back to the
  80×25 VGA text console and `gfxinfo`/`gfxdemo` report that no framebuffer is
  active.
- VMware was not installed in the development environment, so VMware boot was
  not executed directly. The generated VMDK is a valid monolithic-sparse
  VMware disk, and the ISO is a standard El Torito image; both were verified
  under QEMU's SeaBIOS, which follows the same BIOS boot path.
