#!/usr/bin/env python3
"""Interleave PS/2 keystrokes and mouse packets to check 8042 byte routing.

IRQ1 and IRQ12 share the controller's output buffer. If either handler reads a
byte that belongs to the other device, typed text is corrupted or the mouse
packet stream desynchronises. This test types a command while injecting mouse
movement between keystrokes, then checks the command output and the decoded
packet count.
"""
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
    parser.add_argument("--qemu", default="tools/qemu/qemu-system-x86_64.exe")
    parser.add_argument("--image", default="build/hollowstar.img")
    parser.add_argument("--monitor-port", type=int, default=45457)
    parser.add_argument("--memory", default="512")
    args = parser.parse_args()

    image = str(pathlib.Path(args.image).resolve())
    proc = subprocess.Popen(
        [
            args.qemu,
            "-m", args.memory,
            "-drive", f"format=raw,file={image},if=ide,index=0",
            "-boot", "c",
            "-display", "none",
            "-serial", "stdio",
            "-monitor", f"tcp:127.0.0.1:{args.monitor_port},server,nowait",
            "-no-shutdown",
        ],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    reader = Reader(proc.stdout)
    reader.start()

    failures = []
    try:
        if not wait_for(reader, PROMPT):
            raise SystemExit("FAIL: Hollowstar prompt did not appear")

        mon = socket.create_connection(("127.0.0.1", args.monitor_port), timeout=5)
        mon.settimeout(1.0)
        time.sleep(0.2)
        try:
            mon.recv(65536)
        except socket.timeout:
            pass

        def monitor(command):
            mon.sendall(command.encode() + b"\n")
            time.sleep(0.01)

        expected = "echo interleaved input ok"
        keys = ["spc" if c == " " else c for c in expected] + ["ret"]

        # One keystroke, then three mouse packets. Deltas alternate so the
        # cursor returns to the centre, but every packet still hits IRQ12.
        for i, key in enumerate(keys):
            monitor(f"sendkey {key}")
            time.sleep(0.02)
            dx = 3 if i % 2 == 0 else -3
            dy = 2 if i % 2 == 0 else -2
            for _ in range(3):
                monitor(f"mouse_move {dx} {dy}")

        time.sleep(1.0)
        text = reader.snapshot().decode("latin-1", "replace")
        if "interleaved input ok" not in text:
            failures.append("interleaved PS/2 keystrokes were lost or corrupted")

        for ch in "mouse\r\n":
            proc.stdin.write(ch.encode())
            proc.stdin.flush()
            time.sleep(0.01)
        time.sleep(0.8)
        text = reader.snapshot().decode("latin-1", "replace")
        if "position: 512, 384" not in text:
            failures.append("mouse deltas did not cancel back to the centre")
        if "packets:  0" in text[text.rfind("Hollowstar> mouse"):]:
            failures.append("no mouse packets decoded during the stress run")
        if "PANIC" in text or "CPU EXCEPTION" in text:
            failures.append("panic or CPU exception during stress run")

        print(text[text.rfind("Hollowstar> mouse"):])
        mon.close()
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except Exception:
            proc.kill()

    if failures:
        print("FAILURES:")
        for failure in failures:
            print(" -", failure)
        return 1
    print("PASS: keyboard and mouse shared the 8042 buffer without corruption.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
