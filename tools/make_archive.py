#!/usr/bin/env python3
"""Build the /commands archive embedded in the Hollowstar kernel."""
import argparse
import pathlib
import struct
import sys

ARCHIVE_MAGIC = 0x4D435348  # "HSCM"
ARCHIVE_VERSION = 1
HEADER_SIZE = 16
ENTRY_SIZE = 112


def elf_check(path: pathlib.Path, load_addr: int) -> None:
    data = path.read_bytes()
    if len(data) < 64 or data[:4] != b"\x7fELF":
        raise SystemExit(f"{path}: not an ELF file")
    if data[4] != 2 or data[5] != 1:
        raise SystemExit(f"{path}: expected ELF64 little-endian")
    e_type, e_machine = struct.unpack_from("<HH", data, 16)
    if e_type != 2 or e_machine != 62:
        raise SystemExit(f"{path}: expected ET_EXEC x86_64")
    entry = struct.unpack_from("<Q", data, 24)[0]
    if entry < load_addr:
        raise SystemExit(f"{path}: entry {entry:#x} is below {load_addr:#x}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--commands-dir", required=True)
    parser.add_argument("--target-dir", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--load", default="0x800000")
    args = parser.parse_args()

    commands_dir = pathlib.Path(args.commands_dir)
    target_dir = pathlib.Path(args.target_dir)
    load_addr = int(args.load, 0)

    commands = []
    for directory in sorted(commands_dir.iterdir()):
        if not directory.is_dir() or directory.name in ("lib", "target", ".cargo"):
            continue
        cargo = directory / "Cargo.toml"
        desc = directory / "desc.txt"
        if not cargo.exists() or not desc.exists():
            continue
        name = directory.name
        elf = target_dir / name
        if not elf.exists():
            elf = target_dir / (name + ".exe")
        if not elf.exists():
            raise SystemExit(f"missing built command: {elf}")
        elf_check(elf, load_addr)
        description = desc.read_text(encoding="utf-8").strip().splitlines()[0]
        commands.append((name, description, elf))

    if not commands:
        raise SystemExit("no commands found")

    count = len(commands)
    table_end = HEADER_SIZE + count * ENTRY_SIZE
    offset = (table_end + 7) & ~7
    blobs = []
    for name, description, elf in commands:
        data = elf.read_bytes()
        name_bytes = name.encode("ascii")[:31]
        desc_bytes = description.encode("ascii", "replace")[:63]
        blobs.append((name_bytes, desc_bytes, offset, data))
        offset = (offset + len(data) + 7) & ~7

    out = bytearray(offset)
    struct.pack_into("<IIII", out, 0, ARCHIVE_MAGIC, ARCHIVE_VERSION, count, 0)
    for i, (name_bytes, desc_bytes, data_offset, data) in enumerate(blobs):
        entry_offset = HEADER_SIZE + i * ENTRY_SIZE
        struct.pack_into(
            "<32s64sIIQ",
            out,
            entry_offset,
            name_bytes.ljust(32, b"\0"),
            desc_bytes.ljust(64, b"\0"),
            data_offset,
            len(data),
            load_addr,
        )
        out[data_offset:data_offset + len(data)] = data

    out_path = pathlib.Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(out)

    print(f"commands archive: {out_path} ({len(out)} bytes, {count} commands)")
    for name, _description, _elf in commands:
        print(f"  /commands/{name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
