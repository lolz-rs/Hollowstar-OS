import pathlib
import sys

path = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "build/vga.bin")
raw = path.read_bytes()
for row in range(25):
    line = bytes(raw[(row * 80 + col) * 2] for col in range(80))
    print(f"{row:02d}|{line.decode('latin-1').rstrip()}")
