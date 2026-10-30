# Hollowstar

Hollowstar is a small operating system I wrote for x86_64. It has its own bootloader, so there's no GRUB and no Linux under it. You boot it and get a shell.

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

- Boots from a disk image or an ISO
- 1080p by default, and it drops to a smaller mode if the PC can't do that
- True color. Text and graphics both use normal RGB, so millions of colors, not the old 16
- Keyboard, mouse (wheel too), and PC speaker
- Commands are separate Rust programs packed into the kernel

Code is in `src/`, commands are in `commands/`, and the build/test scripts are in `tools/`.

## Build

The compilers are too big to put in git, so they live in `tools/` (nasm, clang, qemu). You also need Rust and Python.

```powershell
rustup target add x86_64-unknown-none
powershell -ExecutionPolicy Bypass -File .\build.ps1
```

That writes `build/hollowstar.img` and `build/hollowstar.iso`. If you want a different default resolution:

```powershell
powershell -ExecutionPolicy Bypass -File .\build.ps1 -GfxWidth 2560 -GfxHeight 1440
```

## Run

```powershell
tools\qemu\qemu-system-x86_64.exe -m 512 `
    -drive format=raw,file=build\hollowstar.img,if=ide,index=0 -boot c `
    -audiodev dsound,id=snd0 -machine pc,pcspk-audiodev=snd0
```

The audio flags are just so `beep` makes noise. Drop them if you don't care.

ISO is `build/hollowstar.iso`. VMware can open `build/Hollowstar.vmx`. Serial is COM1 if you add `-display none -serial stdio`.

## Commands

Type `help` and it lists them. The ones I actually use are `fetch`, `gfxdemo`, `gfxinfo`, `mousedemo`, `mouse`, and `beep`. `halt` turns the machine off. `fastfetch` is just `fetch`, and `exit` is `halt`.

## How it works

The boot sector loads stage2. Stage2 asks the BIOS for a graphics mode (1080p first), turns on long mode, and jumps to the kernel. The kernel sets up interrupts, the timer, the mouse, and then the shell.

Commands are little ELF programs. The shell loads one, runs it, and comes back when it exits. They talk to the kernel with `int 0x80`. Colors are just `0xRRGGBB`. Drawing goes into a back buffer and gets copied to the screen, so it doesn't tear as bad. `gfxdemo` is the stress test for that.

If graphics mode fails it falls back to the old VGA text screen.

## Tests

```powershell
python tools/test_qemu.py --qemu tools/qemu/qemu-system-x86_64.exe --image build/hollowstar.img --vga-out build/vga.bin
```

That one boots it and runs the commands. The other scripts in `tools/` check graphics modes, the mouse, audio, and the ISO. I run them in QEMU.
