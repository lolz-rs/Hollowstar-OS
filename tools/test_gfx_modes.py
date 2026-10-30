#!/usr/bin/env python3
"""Check VBE resolution priority, performance, and the VGA text fallback.

QEMU's std VGA only exposes a mode when its aperture is large enough:

  16 MiB -> 1920x1080x32 (the default, 8.3 MiB back buffer)
   4 MiB -> 1280x720x32  (1080p and 1600x900 do not fit)
   2 MiB -> 800x600x32   (1024x768 needs 3 MiB)
   1 MiB -> VGA text     (640x480 needs 1.2 MiB)

The 1080p run also checks that gfxdemo clears the old ~20 fps ceiling.
"""
import pathlib
import re
import subprocess
import threading
import time

QEMU = "tools/qemu/qemu-system-x86_64.exe"
IMG = str(pathlib.Path("build/hollowstar.img").resolve())


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

    def snap(self):
        with self.lock:
            return bytes(self.data)


def wait_for(reader, needle, timeout=30):
    end = time.time() + timeout
    while time.time() < end:
        if needle in reader.snap():
            return True
        time.sleep(0.05)
    return False


def run(vgamem_mb, commands, needles, demo_seconds=1):
    proc = subprocess.Popen(
        [QEMU, "-m", "512", "-drive", f"format=raw,file={IMG},if=ide,index=0",
         "-boot", "c", "-display", "none", "-serial", "stdio", "-no-reboot",
         "-device", f"VGA,vgamem_mb={vgamem_mb}"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    reader = Reader(proc.stdout)
    reader.start()

    ok = wait_for(reader, b"Hollowstar> ")
    if ok:
        for command in commands:
            for ch in (command + "\r\n").encode():
                proc.stdin.write(bytes([ch]))
                proc.stdin.flush()
                time.sleep(0.002)
            time.sleep(demo_seconds + 2.0 if command.startswith("gfxdemo") else 1.5)
    proc.terminate()
    try:
        proc.wait(timeout=3)
    except Exception:
        proc.kill()

    text = reader.snap().decode("latin-1", "replace")
    missing = [n for n in needles if n not in text]
    return ok, missing, text


def main():
    failures = []

    ok, missing, text = run(16, ["gfxinfo", "gfxdemo 1"],
                            ["1920x1080x32", "8100 KiB", "240x67",
                             "double buffered", "frames", "fps"])
    if not ok:
        failures.append("vgamem_mb=16 did not reach the prompt")
    if missing:
        failures.append("vgamem_mb=16 missing: " + ", ".join(missing))
    else:
        print("1080p default: ok")
    match = re.search(r"gfxdemo: (\d+) frames in \d+ ms, 1920x1080x32, (\d+) fps", text)
    if not match:
        failures.append("could not parse the 1080p gfxdemo result")
    elif int(match.group(2)) < 20:
        failures.append(f"1080p gfxdemo ran at only {match.group(2)} fps")
    else:
        print(f"1080p gfxdemo: {match.group(2)} fps")

    ok, missing, text = run(4, ["gfxinfo"],
                            ["1280x720x32", "160x45", "double buffered"])
    if not ok:
        failures.append("vgamem_mb=4 did not reach the prompt")
    if missing:
        failures.append("vgamem_mb=4 missing: " + ", ".join(missing))
    else:
        print("1280x720 fallback: ok")

    ok, missing, text = run(2, ["gfxinfo"],
                            ["800x600x32", "100x37", "double buffered"])
    if not ok:
        failures.append("vgamem_mb=2 did not reach the prompt")
    if missing:
        failures.append("vgamem_mb=2 missing: " + ", ".join(missing))
    else:
        print("800x600 fallback: ok")

    ok, missing, text = run(1, ["gfxinfo"],
                            ["no linear framebuffer active", "VGA text fallback"])
    if not ok:
        failures.append("vgamem_mb=1 did not reach the prompt")
    if missing:
        failures.append("vgamem_mb=1 missing: " + ", ".join(missing))
    else:
        print("VGA text fallback: ok")

    if failures:
        for failure in failures:
            print("FAIL:", failure)
        raise SystemExit(1)
    print("PASS: resolution priority, performance, and VGA text fallback verified.")


if __name__ == "__main__":
    main()
