# Hollowstar

Hollowstar is an OS I wrote from scratch for x86_64. No GRUB, no Linux, it just boots into a shell.

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

## Stuff it can do

- Boots from a disk image or an ISO
- 1080p by default, smaller modes if the PC can't do that
- True color. Text and graphics both use normal RGB, not the old 16 colors
- Its own font, so it doesn't look like the BIOS text anymore
- Keyboard and mouse, plus the PC speaker
- Commands are separate little Rust programs packed into the kernel

The shell is pretty normal. Tab finishes command names. Left and right move the text caret. Up and down go through old commands. The mouse wheel scrolls back through stuff that already left the screen. Ctrl+C stops whatever is running.

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

Type `help`. The fun ones are `fetch`, `gfxdemo`, `mousedemo`, and `beep`. `halt` turns it off. `exit` does the same thing.

## How it works (kinda)

The boot sector loads stage2. Stage2 asks the BIOS for a graphics mode, then jumps into the kernel. The kernel sets up interrupts, the mouse, and the shell.

Commands are ELF files. The shell loads one, runs it, and comes back when it exits or you hit Ctrl+C. They talk to the kernel with `int 0x80`. Drawing goes into a back buffer first so the screen doesn't tear as much.

If graphics mode fails it just uses the old VGA text screen.

## Tests

```powershell
python tools/test_qemu.py --qemu tools/qemu/qemu-system-x86_64.exe --image build/hollowstar.img --vga-out build/vga.bin
```

That boots it and runs the commands. The other scripts in `tools/` check graphics, the mouse, sound, and the ISO. I run all of this in QEMU.
