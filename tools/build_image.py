#!/usr/bin/env python3
"""Assemble the Hollowstar raw disk image."""
import argparse
import pathlib
import struct
import sys

BOOT_INFO_LBA = 1
STAGE2_LBA = 2
STAGE2_SECTORS = 32
KERNEL_LBA = 64
KERNEL_MAX = 512 * 1024
BOOT_MAGIC = 0x52545348  # "HSTR"
BOOT_VERSION = 2
KERNEL_LOAD = 0x100000
KERNEL_ENTRY = 0x100000
MEMMAP_ADDR = 0x5200


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--boot", required=True)
    parser.add_argument("--stage2", required=True)
    parser.add_argument("--kernel", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--size", default="16M")
    args = parser.parse_args()

    size = int(args.size, 0) if not args.size.endswith("M") else int(args.size[:-1]) * 1024 * 1024
    boot = pathlib.Path(args.boot).read_bytes()
    stage2 = pathlib.Path(args.stage2).read_bytes()
    kernel = pathlib.Path(args.kernel).read_bytes()

    if len(boot) != 512:
        raise SystemExit(f"boot sector must be 512 bytes, got {len(boot)}")
    if len(stage2) > STAGE2_SECTORS * 512:
        raise SystemExit(f"stage2 too large: {len(stage2)} bytes")
    if len(kernel) > KERNEL_MAX:
        raise SystemExit(f"kernel too large: {len(kernel)} bytes (max {KERNEL_MAX})")
    if size < KERNEL_LBA * 512 + len(kernel):
        raise SystemExit("disk image too small")

    image = bytearray(size)
    image[0:512] = boot

    kernel_sectors = (len(kernel) + 511) // 512
    boot_info = struct.pack(
        "<IIQIIQQQIIII",
        BOOT_MAGIC,
        BOOT_VERSION,
        KERNEL_LBA,
        kernel_sectors,
        0,
        KERNEL_LOAD,
        KERNEL_ENTRY,
        MEMMAP_ADDR,
        0,
        24,
        len(kernel),
        0,
    )
    # Graphics fields are filled in by stage2 after the VBE BIOS call.
    boot_info += struct.pack(
        "<QIIIIIIIIQIIQII",
        0,  # framebuffer_addr
        0,  # framebuffer_pitch
        0,  # framebuffer_width
        0,  # framebuffer_height
        0,  # framebuffer_bpp
        0,  # red mask
        0,  # green mask
        0,  # blue mask
        0,  # graphics flags
        0,  # font_addr
        0,  # font_width
        0,  # font_height
        0,  # back_buffer_addr
        0,  # back_buffer_size
        0,  # reserved
    )
    assert len(boot_info) == 136
    image[BOOT_INFO_LBA * 512:BOOT_INFO_LBA * 512 + 64] = boot_info

    stage2_offset = STAGE2_LBA * 512
    image[stage2_offset:stage2_offset + len(stage2)] = stage2

    kernel_offset = KERNEL_LBA * 512
    image[kernel_offset:kernel_offset + len(kernel)] = kernel

    out_path = pathlib.Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(image)
    print(
        f"disk image: {out_path} ({len(image)} bytes, kernel {len(kernel)} bytes / {kernel_sectors} sectors)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
