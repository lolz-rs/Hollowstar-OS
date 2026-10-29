"""Extra hardware checks: PS/2 keyboard input and reboot."""
import pathlib
import socket
import subprocess
import threading
import time

QEMU = "tools/qemu/qemu-system-x86_64.exe"
IMG = str(pathlib.Path("build/hollowstar.img").resolve())
MON = 45455


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


def start(extra=()):
    proc = subprocess.Popen(
        [QEMU, "-m", "512", "-drive", f"format=raw,file={IMG},if=ide,index=0",
         "-boot", "c", "-display", "none", "-serial", "stdio",
         "-monitor", f"tcp:127.0.0.1:{MON},server,nowait", *extra],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    r = Reader(proc.stdout)
    r.start()
    return proc, r


def monitor(cmds):
    sock = socket.create_connection(("127.0.0.1", MON), timeout=5)
    sock.recv(4096)
    for cmd in cmds:
        sock.sendall((cmd + "\n").encode())
        time.sleep(0.05)
    time.sleep(0.2)
    sock.close()


def main():
    failures = []

    # --- PS/2 keyboard through the QEMU monitor ---
    proc, r = start()
    try:
        if not wait_for(r, b"Hollowstar> "):
            failures.append("no prompt before PS/2 test")
        keys = []
        for ch in "echo ps2 keyboard":
            keys.append("spc" if ch == " " else ch)
        keys.append("ret")
        monitor([f"sendkey {k}" for k in keys])
        time.sleep(0.8)
        text = r.snap().decode("latin-1", "replace")
        if "ps2 keyboard" not in text:
            failures.append("PS/2 keyboard input did not reach the shell")
        # Up arrow recalls the last history entry; Enter runs it again.
        monitor(["sendkey up", "sendkey ret"])
        time.sleep(0.8)
        text = r.snap().decode("latin-1", "replace")
        if text.count("ps2 keyboard") < 2:
            failures.append("PS/2 up-arrow history recall failed")
        print("PS/2 output tail:", repr(text[-80:]))
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except Exception:
            proc.kill()
        time.sleep(0.5)

    # --- reboot command ---
    proc, r = start()
    try:
        if not wait_for(r, b"Hollowstar> "):
            failures.append("no prompt before reboot test")
        for ch in b"reboot\r\n":
            proc.stdin.write(bytes([ch]))
            proc.stdin.flush()
            time.sleep(0.003)
        time.sleep(2.5)
        text = r.snap()
        if text.count(b"Hollowstar 0.1.0 (x86_64)") < 2:
            failures.append("reboot did not re-enter the kernel")
        if b"Rebooting..." not in text:
            failures.append("reboot message missing")
        print("reboot banners:", text.count(b"Hollowstar 0.1.0 (x86_64)"))
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except Exception:
            proc.kill()

    # --- aliases and exit ---
    proc, r = start()
    try:
        if not wait_for(r, b"Hollowstar> "):
            failures.append("no prompt before alias test")
        for line in (b"fastfetch", b"echo -n no-newline", b"exit"):
            for ch in line + b"\r\n":
                proc.stdin.write(bytes([ch]))
                proc.stdin.flush()
                time.sleep(0.003)
            time.sleep(0.5)
        time.sleep(0.5)
        text = r.snap().decode("latin-1", "replace")
        if "Hollowstar 0.1.0" not in text or "kernel" not in text:
            failures.append("fastfetch alias did not run fetch")
        if "no-newlineHollowstar> " not in text:
            failures.append("echo -n emitted a newline (prompt not on the same line)")
        if "System halted." not in text:
            failures.append("exit alias did not halt")
        print("alias output tail:", repr(text[-70:]))
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except Exception:
            proc.kill()

    if failures:
        for f in failures:
            print("FAIL:", f)
        return 1
    print("PASS: PS/2 keyboard, reboot, and aliases verified.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
