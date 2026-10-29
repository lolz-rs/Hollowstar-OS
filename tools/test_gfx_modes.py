#!/usr/bin/env python3
"""Check VBE resolution priority and the VGA text fallback.

QEMU's std VGA exposes 1024x768x32 only when its aperture is large enough.
Shrinking it to 2 MiB should select 800x600x32; 1 MiB is too small for any of
the three supported modes, so the kernel must keep the VGA text console.
"""
import pathlib
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


def run(vgamem_mb, commands, needles):
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
            time.sleep(1.5)
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

    ok, missing, text = run(2, ["gfxinfo", "gfxdemo 1"],
                            ["800x600x32", "double buffered", "frames", "fps"])
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
    print("PASS: resolution priority and VGA text fallback verified.")


if __name__ == "__main__":
    main()
