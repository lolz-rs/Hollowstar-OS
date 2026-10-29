#!/usr/bin/env python3
"""Boot Hollowstar, inject mouse events through QEMU, and verify mousedemo.

Checks the cursor overlay, movement without trails, the button indicators,
canvas drawing and clearing, and the counters read back over serial.
"""
import argparse
import pathlib
import re
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


class Monitor:
    def __init__(self, port):
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=5)
        self.sock.settimeout(1.0)
        time.sleep(0.2)
        try:
            self.sock.recv(65536)
        except socket.timeout:
            pass

    def cmd(self, command, wait=0.2):
        self.sock.sendall(command.encode() + b"\n")
        time.sleep(wait)
        try:
            return self.sock.recv(65536).decode("latin-1", "replace")
        except socket.timeout:
            return ""

    def screendump(self, path, wait=0.5):
        posix = str(path).replace("\\", "/")
        self.cmd(f'screendump "{posix}"', wait)

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass


def load_ppm(path):
    data = path.read_bytes()
    header, dims, depth, pixels = data.split(b"\n", 3)
    if header.strip() != b"P6" or depth.strip() != b"255":
        raise ValueError("unexpected PPM header")
    width, height = (int(value) for value in dims.split())
    if len(pixels) < width * height * 3:
        raise ValueError("short PPM pixel data")
    return width, height, pixels


def pixel(pixels, width, x, y):
    index = (y * width + x) * 3
    return pixels[index], pixels[index + 1], pixels[index + 2]


def cursor_counts(pixels, width, height, x, y):
    """Count the pure-white fill and pure-black outline of the software cursor."""
    white = 0
    black = 0
    for dy in range(19):
        py = y + dy
        if py < 0 or py >= height:
            continue
        for dx in range(12):
            px = x + dx
            if px < 0 or px >= width:
                continue
            r, g, b = pixel(pixels, width, px, py)
            if r == 255 and g == 255 and b == 255:
                white += 1
            elif r == 0 and g == 0 and b == 0:
                black += 1
    return white, black


def bright_pixels(pixels, width, height, x, y, w=12, h=19, threshold=200):
    """Count bright pixels in a box, used to detect the brush dot."""
    count = 0
    for py in range(max(0, y), min(height, y + h)):
        for px in range(max(0, x), min(width, x + w)):
            r, g, b = pixel(pixels, width, px, py)
            if r >= threshold and g >= threshold and b >= threshold:
                count += 1
    return count


def count_exact(pixels, width, height, x, y, w, h, color):
    count = 0
    for py in range(max(0, y), min(height, y + h)):
        for px in range(max(0, x), min(width, x + w)):
            if pixel(pixels, width, px, py) == color:
                count += 1
    return count


def left_indicator_pressed(pixels, width, height):
    # Layout: panel at x=10, button box at x=20, rows start at y=84 with a
    # 30-pixel pitch and a 24-pixel box. (25, 96) is inside the LEFT fill,
    # left of the "LEFT" label.
    r, g, b = pixel(pixels, width, 25, 96)
    return g > 150 and r < 160 and b < 160


def middle_indicator_pressed(pixels, width, height):
    r, g, b = pixel(pixels, width, 25, 126)
    return r > 200 and g > 150 and b < 130


def right_indicator_pressed(pixels, width, height):
    r, g, b = pixel(pixels, width, 25, 156)
    return r > 200 and g < 150 and b < 150


def last_pair(text, pattern):
    matches = re.findall(pattern, text)
    return matches[-1] if matches else None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--qemu", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--out-prefix", required=True)
    parser.add_argument("--monitor-port", type=int, default=45460)
    parser.add_argument("--memory", default="512")
    args = parser.parse_args()

    image = str(pathlib.Path(args.image).resolve())
    prefix = pathlib.Path(args.out_prefix).resolve()
    prefix.parent.mkdir(parents=True, exist_ok=True)

    cmd = [
        args.qemu,
        "-m", args.memory,
        "-drive", f"format=raw,file={image},if=ide,index=0",
        "-boot", "c",
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
    monitor = None

    def send(text):
        for byte in text.encode() + b"\r\n":
            proc.stdin.write(bytes([byte]))
            proc.stdin.flush()
            time.sleep(0.002)

    def shot(name):
        path = prefix.with_name(prefix.name + "_" + name + ".ppm")
        monitor.screendump(path)
        return load_ppm(path)

    try:
        if not wait_for(reader, PROMPT, 30):
            print(reader.snapshot().decode("latin-1", "replace"))
            raise SystemExit("FAIL: Hollowstar prompt did not appear")

        monitor = Monitor(args.monitor_port)

        send("mouse")
        time.sleep(0.6)
        initial_text = reader.snapshot().decode("latin-1", "replace")
        if "mouse: not detected" in initial_text:
            failures.append("mouse driver did not detect the QEMU PS/2 mouse")
        start_pos = last_pair(initial_text, r"position:\s*(-?\d+),\s*(-?\d+)")
        if start_pos is None:
            failures.append("initial `mouse` output did not report a position")

        send("mousedemo")
        time.sleep(0.8)

        width, height, pixels = shot("initial")
        cx, cy = width // 2, height // 2
        if start_pos is not None:
            sx, sy = int(start_pos[0]), int(start_pos[1])
            if abs(sx - cx) > 2 or abs(sy - cy) > 2:
                failures.append(
                    f"mouse started at {sx},{sy}, expected screen centre {cx},{cy}")

        white, black = cursor_counts(pixels, width, height, cx, cy)
        if white < 15 or black < 8:
            failures.append(
                f"cursor not visible at centre: {white} white / {black} black pixels")
        else:
            print(f"cursor at centre {cx},{cy}: {white} white, {black} black pixels")

        # Move right by one 8-bit-safe delta.
        monitor.cmd("mouse_move 60 0", 0.4)
        width, height, pixels = shot("moved")
        nx, ny = cx + 60, cy
        white, black = cursor_counts(pixels, width, height, nx, ny)
        if white < 15 or black < 8:
            failures.append(
                f"cursor did not move to {nx},{ny}: {white} white / {black} black pixels")
        else:
            print(f"cursor moved to {nx},{ny}: {white} white, {black} black pixels")

        trail = bright_pixels(pixels, width, height, cx, cy)
        if trail:
            failures.append(f"cursor left {trail} bright pixels at its old position")

        # Press the left button: the indicator must light up and the canvas dot
        # must appear under the cursor.
        monitor.cmd("mouse_button 1", 0.5)
        width, height, pixels = shot("left")
        if not left_indicator_pressed(pixels, width, height):
            failures.append("LEFT button indicator did not light up while pressed")
        else:
            print("LEFT button indicator lit while pressed")
        dot = count_exact(pixels, width, height, nx, ny, 12, 19, (245, 248, 255))
        if dot == 0:
            failures.append("left button did not draw a dot on the canvas")
        else:
            print(f"canvas received {dot} palette-white pixels under the cursor")

        monitor.cmd("mouse_button 0", 0.3)

        # QEMU's legacy mouse_button mask is left=1, right=2, middle=4.
        # Middle cycles the brush size and must light the MIDDLE indicator.
        monitor.cmd("mouse_button 4", 0.5)
        width, height, pixels = shot("middle")
        if not middle_indicator_pressed(pixels, width, height):
            failures.append("MIDDLE button indicator did not light up while pressed")
        else:
            print("MIDDLE button indicator lit while pressed")
        monitor.cmd("mouse_button 0", 0.3)

        # Move away, then right-click to clear the canvas.
        monitor.cmd("mouse_move -60 0", 0.4)
        monitor.cmd("mouse_button 2", 0.5)
        width, height, pixels = shot("right")
        if not right_indicator_pressed(pixels, width, height):
            failures.append("RIGHT button indicator did not light up while pressed")
        else:
            print("RIGHT button indicator lit while pressed")
        monitor.cmd("mouse_button 0", 0.3)
        width, height, pixels = shot("cleared")
        leftover = bright_pixels(pixels, width, height, nx, ny, threshold=200)
        if leftover:
            failures.append(f"right-click clear left {leftover} bright pixels behind")
        else:
            print("right-click cleared the canvas")
        if left_indicator_pressed(pixels, width, height):
            failures.append("LEFT button indicator stayed lit after release")

        # Wheel event (only meaningful when the driver negotiated IntelliMouse).
        monitor.cmd("mouse_move 0 0 1", 0.4)

        send("q")
        if not wait_for(reader, b"mousedemo:", 5):
            failures.append("mousedemo did not exit on Q")
        time.sleep(0.4)

        send("mouse")
        time.sleep(0.6)
        text = reader.snapshot().decode("latin-1", "replace")
        final_pos = last_pair(text, r"position:\s*(-?\d+),\s*(-?\d+)")
        packets = last_pair(text, r"packets:\s*(\d+)")
        wheel = last_pair(text, r"wheel:\s*(-?\d+)")
        if final_pos is None:
            failures.append("final `mouse` output did not report a position")
        else:
            fx, fy = int(final_pos[0]), int(final_pos[1])
            if abs(fx - cx) > 2 or abs(fy - cy) > 2:
                failures.append(
                    f"mouse ended at {fx},{fy}, expected {cx},{cy} after moving back")
            else:
                print(f"final mouse position {fx},{fy}")
        if packets is None or int(packets) == 0:
            failures.append("mouse driver reported no decoded packets")
        else:
            print(f"decoded {packets} mouse packets")
        if "features: wheel" in text and wheel is not None and int(wheel) == 0:
            failures.append("driver negotiated the wheel protocol but saw no wheel delta")
        elif wheel is not None and int(wheel) != 0:
            print(f"wheel delta accumulated: {wheel}")

        if "PANIC" in text or "CPU EXCEPTION" in text:
            failures.append("kernel panic or CPU exception in serial output")

        print("----- serial output -----")
        print(text)
        print("-------------------------")

        if failures:
            print("\nFAILURES:")
            for failure in failures:
                print(" -", failure)
            return 1

        print("\nPASS: mouse driver, cursor overlay, buttons and canvas verified.")
        return 0

    finally:
        if monitor is not None:
            monitor.close()
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
