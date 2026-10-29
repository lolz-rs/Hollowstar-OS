#!/usr/bin/env python3
"""Boot Hollowstar in QEMU, drive the serial shell, and verify output."""
import argparse
import pathlib
import socket
import subprocess
import sys
import threading
import time

PROMPT = b"Hollowstar> "


class Reader(threading.Thread):
    def __init__(self, pipe):
        super().__init__(daemon=True)
        self.pipe = pipe
        self.data = bytearray()
        self.lock = threading.Lock()

    def run(self):
        while True:
            chunk = self.pipe.read(1)
            if not chunk:
                return
            with self.lock:
                self.data.extend(chunk)

    def snapshot(self):
        with self.lock:
            return bytes(self.data)


def wait_for(reader, needle, timeout=30.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if needle in reader.snapshot():
            return True
        time.sleep(0.05)
    return False


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--qemu", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--vga-out", required=True)
    parser.add_argument("--monitor-port", type=int, default=45454)
    parser.add_argument("--memory", default="512")
    parser.add_argument("--cdrom", action="store_true", help="boot the image as a CD-ROM")
    args = parser.parse_args()

    image = str(pathlib.Path(args.image).resolve())
    vga_out = pathlib.Path(args.vga_out).resolve()
    vga_out.parent.mkdir(parents=True, exist_ok=True)

    if args.cdrom:
        boot_args = ["-cdrom", image, "-boot", "d"]
    else:
        boot_args = ["-drive", f"format=raw,file={image},if=ide,index=0", "-boot", "c"]

    cmd = [
        args.qemu,
        "-m", args.memory,
        *boot_args,
        "-display", "none",
        "-serial", "stdio",
        "-monitor", f"tcp:127.0.0.1:{args.monitor_port},server,nowait",
        "-no-shutdown",
    ]

    proc = subprocess.Popen(
        cmd,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    reader = Reader(proc.stdout)
    reader.start()

    failures = []

    def send(text):
        # QEMU's stdio backend delivers a whole write at once, which overruns
        # the 16-byte UART FIFO. Pace bytes like a real 115200-baud line.
        for byte in text.encode() + b"\r\n":
            proc.stdin.write(bytes([byte]))
            proc.stdin.flush()
            time.sleep(0.002)

    def monitor(command, wait=0.3):
        sock = socket.create_connection(("127.0.0.1", args.monitor_port), timeout=5)
        try:
            sock.recv(4096)
            sock.sendall(command.encode())
            time.sleep(wait)
        finally:
            sock.close()

    clear_shot = vga_out.with_name(vga_out.stem + "_clear.ppm")

    try:
        if not wait_for(reader, PROMPT, 30):
            print(reader.snapshot().decode("latin-1", "replace"))
            raise SystemExit("FAIL: Hollowstar prompt did not appear")

        commands = [
            ("fetch", 0.35),
            ("echo hello from hollowstar", 0.35),
            ("help", 0.35),
            ("uname", 0.35),
            ("uptime", 0.35),
            ("meminfo", 0.35),
            ("memtest", 0.35),
            ("cpuinfo", 0.35),
            ("commands", 0.35),
            ("version", 0.35),
            ("gfxinfo", 0.6),
            ("beep 880 100", 0.6),
            ("gfxdemo 1", 2.5),
            ("mouse", 0.35),
            ("sleep 100", 0.35),
            ("bogus", 0.35),
            ("clear", 0.35),
            ("echo after clear", 0.35),
            ("halt", 0.5),
        ]
        for command, delay in commands:
            send(command)
            time.sleep(delay)
            if command == "clear":
                # Park the software cursor in the top-left corner so it cannot
                # be mistaken for stale framebuffer pixels in the lower half.
                # The PS/2 driver clamps accumulated deltas to the screen.
                for _ in range(3):
                    monitor("mouse_move -2000 -2000\n", 0.25)
                time.sleep(0.2)
                monitor(f'screendump "{str(clear_shot).replace(chr(92), "/")}"\n', 0.4)

        time.sleep(1.0)
        output = reader.snapshot()
        text = output.decode("latin-1", "replace")

        expected = {
            "fetch": ["Hollowstar", "kernel", "cpu", "memory", "commands"],
            "echo": ["hello from hollowstar"],
            "help": ["Commands in /commands:", "fetch", "Show system information"],
            "uname": ["Hollowstar 0.1.0 x86_64"],
            "uptime": ["up "],
            "meminfo": ["usable", "mapped"],
            "memtest": ["High memory", "all probes passed"],
            "cpuinfo": ["vendor", "brand", "x86_64"],
            "commands": ["/commands", "fetch", "echo"],
            "version": ["Hollowstar 0.1.0"],
            "gfxinfo": ["framebuffer", "back buffer", "double buffered", "vsync"],
            "beep": ["beep:", "Hz", "PC speaker"],
            "gfxdemo": ["gfxdemo:", "frames", "x32"],
            "mouse": ["mouse:", "position", "packets"],
            "bogus": ["bogus: command not found in /commands"],
            "clear": ["after clear"],
            "halt": ["System halted."],
        }
        for command, needles in expected.items():
            for needle in needles:
                if needle not in text:
                    failures.append(f"missing output for {command}: {needle!r}")

        if ("PANIC" in text) or ("CPU EXCEPTION" in text):
            failures.append("kernel panic or CPU exception in serial output")

        # Regression check: `clear` must blank the whole framebuffer, not just
        # the rows covered by a COLS*ROWS contiguous clear. Look at the lower
        # half of the screen right after `clear` and require it to be black.
        if not clear_shot.exists():
            failures.append("post-clear screendump was not created")
        else:
            ppm = clear_shot.read_bytes()
            try:
                header, dims, depth, pixels = ppm.split(b"\n", 3)
                width, height = (int(v) for v in dims.split())
                if header.strip() != b"P6" or depth.strip() != b"255":
                    raise ValueError("unexpected PPM header")
                bottom = height // 2
                stride = width * 3
                stale = 0
                for y in range(bottom, height):
                    row = pixels[y * stride:(y + 1) * stride]
                    stale += sum(1 for x in range(0, len(row), 3)
                                 if row[x] or row[x + 1] or row[x + 2])
                if stale:
                    failures.append(
                        f"post-clear framebuffer has {stale} stale pixels in the lower half")
                else:
                    print(f"post-clear framebuffer: lower {height - bottom} rows are black")
            except (ValueError, IndexError) as exc:
                failures.append(f"could not parse post-clear screendump: {exc}")

        # Dump VGA text memory through the QEMU monitor. In VBE graphics mode
        # the kernel keeps its 80x25 text shadow at 0x50000 instead, so dump
        # both and accept whichever is active.
        ram_out = vga_out.with_name(vga_out.stem + "_ram" + vga_out.suffix)
        try:
            sock = socket.create_connection(("127.0.0.1", args.monitor_port), timeout=5)
            sock.recv(4096)
            path = str(vga_out).replace("\\", "/")
            sock.sendall(f'pmemsave 0xb8000 4000 "{path}"\n'.encode())
            time.sleep(0.2)
            ram_path = str(ram_out).replace("\\", "/")
            sock.sendall(f'pmemsave 0x50000 4000 "{ram_path}"\n'.encode())
            time.sleep(0.5)
            sock.sendall(b"quit\n")
            time.sleep(0.3)
            sock.close()
        except OSError as exc:
            failures.append(f"monitor/VGA dump failed: {exc}")

        screen = ""
        for candidate in (vga_out, ram_out):
            if candidate.exists():
                raw = candidate.read_bytes()
                chars = bytes(raw[i] for i in range(0, len(raw), 2))
                screen += chars.decode("latin-1", "replace") + "\n"
        if not (vga_out.exists() or ram_out.exists()):
            failures.append("VGA dump was not created")
        else:
            if "Hollowstar>" not in screen:
                failures.append("VGA dump does not contain the Hollowstar prompt")
            if "after clear" not in screen:
                failures.append("VGA dump does not contain post-clear echo output")

        print("----- serial output -----")
        print(text)
        print("-------------------------")

        if failures:
            print("\nFAILURES:")
            for failure in failures:
                print(" -", failure)
            return 1

        print("\nPASS: Hollowstar booted, commands ran, VGA and serial output verified.")
        return 0

    finally:
        try:
            if proc.poll() is None:
                proc.terminate()
                proc.wait(timeout=3)
        except Exception:
            try:
                proc.kill()
            except Exception:
                pass


if __name__ == "__main__":
    sys.exit(main())
