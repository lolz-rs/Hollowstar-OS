# Hollowstar

Hollowstar is an OS I wrote from scratch for x86_64. No GRUB, no Linux, it just boots into a shell.

```
Hollowstar> fetch
            #             Hollowstar 0.1.0
           # #            kernel   0.1.0 x86_64
          #   #           uptime   0s
         #     #          cpu      QEMU Virtual CPU version 2.5+
#########       ######### memory   511 MiB
 ##                   ##  commands 23
   ##               ##
     ##           ##
       #    #    #
      #   ## ##   #
      #  #     ## #
     # ##        # #
     ##           ##
    #               #
```

## Stuff it can do

- Boots from a disk image or an ISO
- 1080p by default, smaller modes if the PC can't do that
- True color. Text and graphics both use normal RGB, not the old 16 colors
- Its own font, so it doesn't look like the BIOS text anymore
- Keyboard and mouse, plus the PC speaker
- A flat in-memory RAM disk with `ls`, `cat`, `echo`, `rm`, and `nano`
- Commands are separate little Rust programs packed into the kernel

The shell is pretty normal. Tab finishes command names. Left and right move the text caret. Up and down go through old commands. The mouse wheel scrolls back through stuff that already left the screen. Ctrl+C stops whatever is running. Shell errors start with `hsh:` and show up in red.

Code is in `src/`, commands are in `commands/`, build and test scripts are in `tools/`.

## Build

The compilers are huge so they aren't in git. Put nasm, clang, and qemu under `tools/`. You also need Rust and Python.

```powershell
rustup target add x86_64-unknown-none
powershell -ExecutionPolicy Bypass -File .\build.ps1
```

That makes `build/hollowstar.img` and `build/hollowstar.iso`. If you want a different resolution:

```powershell
powershell -ExecutionPolicy Bypass -File .\build.ps1 -GfxWidth 2560 -GfxHeight 1440
```

## Run

```powershell
tools\qemu\qemu-system-x86_64.exe -m 512 `
    -drive format=raw,file=build\hollowstar.img,if=ide,index=0 -boot c `
    -audiodev dsound,id=snd0 -machine pc,pcspk-audiodev=snd0
```

The audio part is only so `beep` makes noise. You can leave it off.

The ISO is `build/hollowstar.iso`. VMware can open `build/Hollowstar.vmx`.

## Commands

Type `help` for the same list from inside the OS. The fun ones are `fetch`, `gfxdemo`, `mousedemo`, and `beep`. `halt` turns it off. `exit` does the same thing.

| Command | What it does |
| --- | --- |
| `beep [frequency] [milliseconds]` | PC speaker test. `beep melody` plays a tune |
| `cat [-n] file...` | Print files from the RAM disk. `-n` numbers the lines |
| `clear` | Clear the terminal |
| `commands` | List the command archive |
| `cpuinfo` | Show CPU information |
| `echo [-n] [-e] [text]` | Print text. `-n` drops the newline, `-e` expands escapes |
| `fetch` | Show system information |
| `gfxdemo` | True-color rainbow, panels, orbits, stars, and waves |
| `gfxinfo` | Show framebuffer and graphics console details |
| `halt` | Halt the machine |
| `help` | List commands and descriptions |
| `ls [-1] [-l]` | List the RAM disk. `-1` prints names only, `-l` is the default table |
| `meminfo` | Show a memory summary |
| `memtest` | Test RAM at the top of the usable range |
| `mouse` | Show PS/2 mouse position, buttons, wheel, and packet counters |
| `mousedemo` | Live mouse readouts and a drawing canvas. `S` saves it |
| `nano [file]` | Small text editor for the RAM disk |
| `reboot` | Reboot the machine |
| `rm [-f] [-v] file...` | Delete files. Quiet unless you pass `-v` |
| `sleep <ms>` | Sleep for a number of milliseconds |
| `uname` | Print the system name and version |
| `uptime` | Show time since boot |
| `version` | Show the Hollowstar version |

`fastfetch` and `neofetch` are aliases for `fetch`. `shutdown` is an alias for `halt`.

## The RAM disk

Files live in memory, so a reboot clears them. It is flat: no folders, just names. It holds 32 files of up to 64 KiB each, for 2 MiB total.

```text
Hollowstar> echo hello > note.txt
Hollowstar> echo more >> note.txt
Hollowstar> cat -n note.txt
     1	hello
     2	more
Hollowstar> ls
NAME                         SIZE
note.txt                     11 B
welcome.txt                  138 B
--------------------------------------------
2 files, 149 B used, 2.0 MiB free
```

- `>` creates or replaces a file, `>>` appends, and `> file` on its own creates or empties one.
- A redirect is opened only after the command name resolves, so `typo > notes.txt` cannot wipe a file.
- `>` inside quotes is plain text: `echo "a > b"` prints `a > b`.
- `echo -e` understands `\n`, `\t`, `\r`, `\\`, `\0`, and `\xHH`.
- Command errors go to stderr, so they stay on screen and render red even when stdout is redirected.
- `nano` opens a file (or starts a new one). Arrows move, Home/End jump, Page Up/Down scroll, Ctrl+K cuts a line, Ctrl+U pastes, Ctrl+S saves, and Ctrl+X exits. If the buffer is modified, Ctrl+X asks once before discarding it. Ctrl+C aborts without saving.
- In `mousedemo`, `S` saves an aspect-correct 128x80 PPM preview as `canvas.ppm`.

Read and write permissions are tracked per open file, and all handles are dropped when a command finishes or is interrupted.

## How it works (kinda)

The boot sector loads stage2. Stage2 asks the BIOS for a graphics mode, then jumps into the kernel. The kernel sets up interrupts, the mouse, the RAM disk, and the shell.

Commands are ELF files packed into a `/commands` archive. The shell loads one, runs it, and comes back when it exits or you hit Ctrl+C. They talk to the kernel with `int 0x80`. Drawing goes into a back buffer first so the screen doesn't tear as much.

The RAM disk is a fixed array of 64 KiB slots in RAM. Files are opened into a small descriptor table with read/write flags, and writes are checked against the slot size. Nothing is written back to the boot medium.

If graphics mode fails it just uses the old VGA text screen.

## Tests

```powershell
python tools/test_qemu.py --qemu tools/qemu/qemu-system-x86_64.exe --image build/hollowstar.img --vga-out build/vga.bin
```

That boots it and runs the shell commands. The other scripts in `tools/` check graphics modes, the mouse, shared PS/2 input, audio, the ISO, the VMDK, and extra commands. I run all of this in QEMU.
