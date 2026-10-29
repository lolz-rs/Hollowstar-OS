import pathlib
import subprocess
import threading
import time

qemu = "tools/qemu/qemu-system-x86_64.exe"
vmdk = str(pathlib.Path("build/Hollowstar.vmdk").resolve())
proc = subprocess.Popen(
    [qemu, "-m", "512", "-drive", f"format=vmdk,file={vmdk},if=ide,index=0",
     "-boot", "c", "-display", "none", "-serial", "stdio", "-no-reboot"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
out = bytearray()


def rd():
    while True:
        c = proc.stdout.read(1)
        if not c:
            return
        out.extend(c)


threading.Thread(target=rd, daemon=True).start()
t = time.time()
while time.time() - t < 30 and b"Hollowstar> " not in out:
    time.sleep(0.01)
boot = time.time() - t
ok = b"Hollowstar> " in out
if ok:
    for ch in b"version\r\n":
        proc.stdin.write(bytes([ch]))
        proc.stdin.flush()
        time.sleep(0.003)
    time.sleep(0.5)
proc.terminate()
try:
    proc.wait(timeout=3)
except Exception:
    proc.kill()
text = bytes(out).decode("latin-1", "replace")
print("vmdk boot:", ok, "time=%.3fs" % boot)
print(text[-200:])
raise SystemExit(0 if ok and "Hollowstar 0.1.0" in text else 1)
