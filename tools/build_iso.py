#!/usr/bin/env python3
"""Build a bootable El Torito ISO9660 image for Hollowstar.

The boot image is self-contained: firmware loads it at 0x7C00, stage2 sits at
0x8000 and the flat kernel at 0xC000. No CD reads are needed after the initial
load, so the same image boots under QEMU, VMware, VirtualBox and friends.
"""
import argparse
import pathlib
import struct
import sys

SECTOR = 2048
BOOT_INFO_MAGIC = 0x52545348

# ISO layout
SYSTEM_SECTORS = 16
PVD_LBA = 16
BRVD_LBA = 17
TERMINATOR_LBA = 18
CATALOG_LBA = 19
L_PATH_LBA = 20
M_PATH_LBA = 21
ROOT_LBA = 22
BOOT_LBA = 23

# Boot image layout (offsets inside the image, loaded at 0x7C00)
BOOT_SECTOR = 0x0000
STAGE2_OFFSET = 0x0400      # -> 0x8000
KERNEL_OFFSET = 0x4400      # -> 0xC000


def both16(value: int) -> bytes:
    return struct.pack("<H", value) + struct.pack(">H", value)


def both32(value: int) -> bytes:
    return struct.pack("<I", value) + struct.pack(">I", value)


def date7() -> bytes:
    # 2026-10-28 00:00:00 GMT
    return bytes([126, 10, 28, 0, 0, 0, 0])


def date17() -> bytes:
    return b"2026102800000000" + b"\x00"


def dir_record(name: bytes, extent: int, size: int, flags: int) -> bytes:
    record = bytearray()
    record += b"\x00"                       # length, patched below
    record += b"\x00"                       # extended attribute length
    record += both32(extent)
    record += both32(size)
    record += date7()
    record += bytes([flags])
    record += b"\x00\x00"                   # file unit size, interleave gap
    record += both16(1)                     # volume sequence number
    record += bytes([len(name)])
    record += name
    if len(record) % 2:
        record += b"\x00"
    record[0] = len(record)
    return bytes(record)


def make_pvd(volume_sectors: int, root_record: bytes, path_size: int) -> bytes:
    pvd = bytearray(SECTOR)
    pvd[0] = 1
    pvd[1:6] = b"CD001"
    pvd[6] = 1
    pvd[8:40] = b"HOLLOWSTAR".ljust(32, b" ")
    pvd[40:72] = b"HOLLOWSTAR".ljust(32, b" ")
    pvd[80:88] = both32(volume_sectors)
    pvd[120:124] = both16(1)                 # volume set size
    pvd[124:128] = both16(1)                 # volume sequence number
    pvd[128:132] = both16(SECTOR)            # logical block size
    pvd[132:140] = both32(path_size)         # path table size
    pvd[140:144] = struct.pack("<I", L_PATH_LBA)
    pvd[144:148] = struct.pack("<I", 0)
    pvd[148:152] = struct.pack(">I", M_PATH_LBA)
    pvd[152:156] = struct.pack(">I", 0)
    pvd[156:190] = root_record
    pvd[190:318] = b"HOLLOWSTAR".ljust(128, b" ")
    pvd[318:446] = b"HOLLOWSTAR".ljust(128, b" ")
    pvd[446:574] = b"HOLLOWSTAR".ljust(128, b" ")
    pvd[574:702] = b"HOLLOWSTAR".ljust(128, b" ")
    pvd[813:830] = date17()                  # creation
    pvd[830:847] = date17()                  # modification
    pvd[847:864] = b"0000000000000000\x00"   # expiration: none
    pvd[864:881] = b"0000000000000000\x00"   # effective: immediately
    pvd[881] = 1
    return bytes(pvd)


def make_brvd(catalog_lba: int) -> bytes:
    brvd = bytearray(SECTOR)
    brvd[0] = 0
    brvd[1:6] = b"CD001"
    brvd[6] = 1
    brvd[7:39] = b"EL TORITO SPECIFICATION".ljust(32, b"\x00")
    brvd[39:71] = b"HOLLOWSTAR".ljust(32, b"\x00")
    brvd[71:75] = struct.pack("<I", catalog_lba)
    return bytes(brvd)


def make_catalog(load_rba: int, sector_count: int) -> bytes:
    catalog = bytearray(SECTOR)

    # Validation entry.
    catalog[0] = 0x01
    catalog[1] = 0x00
    catalog[4:28] = b"HOLLOWSTAR".ljust(24, b"\x00")
    catalog[30:32] = b"\x55\xAA"

    # Initial/default entry: no emulation, load segment 0x07C0.
    entry = 32
    catalog[entry + 0] = 0x88
    catalog[entry + 1] = 0x00
    catalog[entry + 2:entry + 4] = struct.pack("<H", 0x07C0)
    catalog[entry + 4] = 0x00
    catalog[entry + 6:entry + 8] = struct.pack("<H", sector_count)
    catalog[entry + 8:entry + 12] = struct.pack("<I", load_rba)

    # Checksum: all 16-bit words of the validation entry must sum to zero.
    checksum = 0
    for i in range(0, 32, 2):
        checksum += struct.unpack_from("<H", catalog, i)[0]
    checksum = (-checksum) & 0xFFFF
    struct.pack_into("<H", catalog, 28, checksum)
    return bytes(catalog)


def make_path_table(big_endian: bool) -> bytes:
    table = bytearray(SECTOR)
    table[0] = 1                 # length of directory identifier
    table[1] = 0                 # extended attribute length
    if big_endian:
        table[2:6] = struct.pack(">I", ROOT_LBA)
        table[6:8] = struct.pack(">H", 1)
    else:
        table[2:6] = struct.pack("<I", ROOT_LBA)
        table[6:8] = struct.pack("<H", 1)
    table[8] = 0                 # root identifier
    table[9] = 0                 # padding
    return bytes(table)


def make_root_directory(boot_size: int) -> bytes:
    directory = bytearray(SECTOR)
    offset = 0
    for name, extent, size, flags in (
        (b"\x00", ROOT_LBA, SECTOR, 0x02),
        (b"\x01", ROOT_LBA, SECTOR, 0x02),
        (b"BOOT.IMG;1", BOOT_LBA, boot_size, 0x00),
    ):
        record = dir_record(name, extent, size, flags)
        directory[offset:offset + len(record)] = record
        offset += len(record)
    return bytes(directory)


def build_boot_image(boot: bytes, stage2: bytes, kernel: bytes) -> bytes:
    if len(boot) != 512:
        raise SystemExit(f"boot sector must be 512 bytes, got {len(boot)}")
    if len(stage2) > 32 * 512:
        raise SystemExit(f"stage2 too large: {len(stage2)} bytes")
    if len(kernel) > 512 * 1024:
        raise SystemExit(f"kernel too large: {len(kernel)} bytes")

    image = bytearray(KERNEL_OFFSET + len(kernel))
    image[BOOT_SECTOR:BOOT_SECTOR + 512] = boot
    image[STAGE2_OFFSET:STAGE2_OFFSET + len(stage2)] = stage2
    image[KERNEL_OFFSET:KERNEL_OFFSET + len(kernel)] = kernel
    return bytes(image)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--boot", required=True)
    parser.add_argument("--stage2", required=True)
    parser.add_argument("--kernel", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    boot = pathlib.Path(args.boot).read_bytes()
    stage2 = pathlib.Path(args.stage2).read_bytes()
    kernel = pathlib.Path(args.kernel).read_bytes()

    boot_image = build_boot_image(boot, stage2, kernel)
    boot_sectors = (len(boot_image) + SECTOR - 1) // SECTOR
    padded_boot = boot_image.ljust(boot_sectors * SECTOR, b"\x00")
    # The catalog counts 512-byte units; use whole 2048-byte sectors so the
    # firmware never reads past the volume.
    catalog_sectors = boot_sectors * 4

    volume_sectors = BOOT_LBA + boot_sectors
    root_record = dir_record(b"\x00", ROOT_LBA, SECTOR, 0x02)

    image = bytearray(volume_sectors * SECTOR)
    image[PVD_LBA * SECTOR:(PVD_LBA + 1) * SECTOR] = make_pvd(
        volume_sectors, root_record, 10
    )
    image[BRVD_LBA * SECTOR:(BRVD_LBA + 1) * SECTOR] = make_brvd(CATALOG_LBA)
    image[TERMINATOR_LBA * SECTOR:(TERMINATOR_LBA + 1) * SECTOR] = (
        bytes([255]) + b"CD001" + bytes([1]) + bytes(SECTOR - 7)
    )
    image[CATALOG_LBA * SECTOR:(CATALOG_LBA + 1) * SECTOR] = make_catalog(
        BOOT_LBA, catalog_sectors
    )
    image[L_PATH_LBA * SECTOR:(L_PATH_LBA + 1) * SECTOR] = make_path_table(False)
    image[M_PATH_LBA * SECTOR:(M_PATH_LBA + 1) * SECTOR] = make_path_table(True)
    image[ROOT_LBA * SECTOR:(ROOT_LBA + 1) * SECTOR] = make_root_directory(
        len(boot_image)
    )
    image[BOOT_LBA * SECTOR:BOOT_LBA * SECTOR + len(padded_boot)] = padded_boot

    out_path = pathlib.Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(image)

    print(
        f"iso image: {out_path} ({len(image)} bytes, boot image {len(boot_image)} bytes, "
        f"{catalog_sectors} catalog sectors)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
