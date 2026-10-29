#!/usr/bin/env python3
"""Extract loadable ELF64 segments into a flat binary for the bootloader."""
import argparse
import pathlib
import struct
import sys


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("input")
    parser.add_argument("output")
    parser.add_argument("base", nargs="?", default="0x100000")
    args = parser.parse_args()

    data = pathlib.Path(args.input).read_bytes()
    base = int(args.base, 0)

    if len(data) < 64 or data[:4] != b"\x7fELF":
        raise SystemExit("not an ELF file")
    if data[4] != 2 or data[5] != 1:
        raise SystemExit("expected ELF64 little-endian")
    e_type, e_machine = struct.unpack_from("<HH", data, 16)
    if e_type != 2 or e_machine != 62:
        raise SystemExit("expected ET_EXEC x86_64")
    e_entry = struct.unpack_from("<Q", data, 24)[0]
    e_phoff = struct.unpack_from("<Q", data, 32)[0]
    e_phentsize, e_phnum = struct.unpack_from("<HH", data, 54)

    segments = []
    for i in range(e_phnum):
        ph = e_phoff + i * e_phentsize
        p_type, _p_flags = struct.unpack_from("<II", data, ph)
        if p_type != 1:
            continue
        p_offset, p_vaddr, p_paddr, p_filesz, p_memsz = struct.unpack_from(
            "<QQQQQ", data, ph + 8
        )
        if p_filesz == 0:
            continue
        address = p_paddr if p_paddr != 0 else p_vaddr
        if address < base:
            raise SystemExit(f"segment at {address:#x} is below base {base:#x}")
        segments.append((address, data[p_offset:p_offset + p_filesz], p_memsz))

    if not segments:
        raise SystemExit("no loadable segments")

    end = max(address + len(blob) for address, blob, _memsz in segments)
    image = bytearray(end - base)
    for address, blob, _memsz in segments:
        start = address - base
        image[start:start + len(blob)] = blob

    out_path = pathlib.Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(image)
    print(f"flat kernel: {out_path} ({len(image)} bytes, entry {e_entry:#x})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
