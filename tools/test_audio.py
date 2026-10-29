#!/usr/bin/env python3
"""Verify that the PC-speaker path really produces sound in QEMU.

QEMU only emulates the PC speaker when an audio backend is attached to the
machine. This test boots Hollowstar with the `wav` backend, plays two tones,
then checks the captured waveform for a non-silent signal at the requested
frequencies.
"""
import argparse
import math
import pathlib
import socket
import struct
import subprocess
import sys
import threading
import time
import wave

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


def tone_stats(samples, rate):
    if not samples:
        return 0.0, 0.0
    rms = math.sqrt(sum(x * x for x in samples) / len(samples))
    crossings = sum(1 for a, b in zip(samples, samples[1:]) if (a < 0) != (b < 0))
    freq = crossings / 2.0 / (len(samples) / rate)
    return rms, freq


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--qemu", default="tools/qemu/qemu-system-x86_64.exe")
    parser.add_argument("--image", default="build/hollowstar.img")
    parser.add_argument("--wav-out", default="build/pcspk_test.wav")
    parser.add_argument("--monitor-port", type=int, default=45456)
    args = parser.parse_args()

    image = str(pathlib.Path(args.image).resolve())
    wav_out = pathlib.Path(args.wav_out).resolve()
    wav_out.parent.mkdir(parents=True, exist_ok=True)
    if wav_out.exists():
        wav_out.unlink()

    proc = subprocess.Popen(
        [
            args.qemu, "-m", "512",
            "-drive", f"format=raw,file={image},if=ide,index=0",
            "-boot", "c", "-display", "none", "-serial", "stdio",
            "-audiodev", f"wav,id=snd0,path={wav_out}",
            "-machine", "pc,pcspk-audiodev=snd0",
            "-monitor", f"tcp:127.0.0.1:{args.monitor_port},server,nowait",
            "-no-shutdown",
        ],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    reader = Reader(proc.stdout)
    reader.start()

    def send(text):
        for byte in text.encode() + b"\r\n":
            proc.stdin.write(bytes([byte]))
            proc.stdin.flush()
            time.sleep(0.002)

    def quit_qemu():
        try:
            sock = socket.create_connection(("127.0.0.1", args.monitor_port), timeout=5)
            sock.recv(4096)
            sock.sendall(b"quit\n")
            time.sleep(0.5)
            sock.close()
        except OSError:
            pass

    failures = []
    try:
        if not wait_for(reader, PROMPT, 30):
            print(reader.snapshot().decode("latin-1", "replace"))
            raise SystemExit("FAIL: Hollowstar prompt did not appear")

        send("beep 880 400")
        time.sleep(0.9)
        send("beep 440 400")
        time.sleep(0.9)
        quit_qemu()
        try:
            proc.wait(timeout=3)
        except Exception:
            proc.kill()

        if not wav_out.exists():
            raise SystemExit("FAIL: QEMU did not create the WAV capture")

        with wave.open(str(wav_out), "rb") as capture:
            rate = capture.getframerate()
            channels = capture.getnchannels()
            data = capture.readframes(capture.getnframes())
        raw = struct.unpack("<%dh" % (len(data) // 2), data)
        mono = [(raw[i] + raw[i + 1]) // 2 for i in range(0, len(raw), 2)] \
            if channels == 2 else list(raw)

        peak = max((abs(x) for x in mono), default=0)
        if peak < 1000:
            failures.append(f"captured audio is silent (peak {peak})")

        window = int(rate * 0.3)
        first_rms, first_freq = tone_stats(mono[:window], rate)
        last_rms, last_freq = tone_stats(mono[-window:], rate)
        if first_rms < 1000:
            failures.append(f"first tone is silent (rms {first_rms:.0f})")
        if last_rms < 1000:
            failures.append(f"second tone is silent (rms {last_rms:.0f})")
        if abs(first_freq - 880) > 40:
            failures.append(f"first tone is {first_freq:.0f} Hz, expected 880")
        if abs(last_freq - 440) > 30:
            failures.append(f"second tone is {last_freq:.0f} Hz, expected 440")

        print(f"captured {len(mono) / rate:.2f}s at {rate} Hz, peak {peak}")
        print(f"first tone:  rms {first_rms:.0f}, {first_freq:.0f} Hz")
        print(f"second tone: rms {last_rms:.0f}, {last_freq:.0f} Hz")

        if failures:
            for failure in failures:
                print("FAIL:", failure)
            return 1
        print("PASS: PC speaker produced 880 Hz and 440 Hz tones.")
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
