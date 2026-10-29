#!/usr/bin/env python3
"""Validate the Hollowstar ISO9660 + El Torito structure.

This checks the on-disk metadata a BIOS or VM reads before it ever runs our
code: the primary volume descriptor, path tables, root directory, boot record,
boot catalog checksum, and the embedded boot image. It complements
test_qemu.py, which proves the image actually boots.
"""
import argparse
import pathlib
import struct
import sys

SECTOR = 2048
BOOT_INFO_MAGIC = 0x52545348


def u16le(data, offset):
    return struct.unpack_from("<H", data, offset)[0]


def u32le(data, offset):
    return struct.unpack_from("<I", data, offset)[0]


def u32be(data, offset):
    return struct.unpack_from(">I", data, offset)[0]


def check(condition, failures, message):
    if not condition:
        failures.append(message)


def parse_dir_record(record):
    if len(record) < 34:
        raise ValueError("directory record too short")
    name_len = record[32]
    return {
        "length": record[0],
        "extent": u32le(record, 2),
        "size": u32le(record, 10),
        "flags": record[25],
        "name": record[33:33 + name_len],
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--iso", default="build/hollowstar.iso")
    parser.add_argument("--boot", default="build/iso_boot.bin")
    parser.add_argument("--stage2", default="build/stage2_iso.bin")
    parser.add_argument("--kernel", default="build/kernel.bin")
    args = parser.parse_args()

    iso_path = pathlib.Path(args.iso)
    data = iso_path.read_bytes()
    failures = []

    check(len(data) % SECTOR == 0, failures, "ISO size is not a whole number of sectors")
    sectors = len(data) // SECTOR
    check(sectors >= 24, failures, f"ISO is too small ({sectors} sectors)")

    # Primary Volume Descriptor.
    pvd = data[16 * SECTOR:17 * SECTOR]
    check(pvd[0] == 1, failures, "PVD type is not 1")
    check(pvd[1:6] == b"CD001", failures, "PVD identifier is not CD001")
    check(pvd[6] == 1, failures, "PVD version is not 1")
    volume_sectors = u32le(pvd, 80)
    check(volume_sectors == sectors, failures,
          f"PVD volume size {volume_sectors} != file size {sectors} sectors")
    check(u16le(pvd, 128) == SECTOR, failures, "PVD logical block size is not 2048")
    path_size = u32le(pvd, 132)
    l_path_lba = u32le(pvd, 140)
    m_path_lba = u32be(pvd, 148)
    check(path_size >= 10, failures, f"PVD path table size is {path_size}")
    check(l_path_lba != 0, failures, "PVD has no little-endian path table")
    check(m_path_lba != 0, failures, "PVD has no big-endian path table")
    check(pvd[881] == 1, failures, "PVD file structure version is not 1")

    root = parse_dir_record(pvd[156:190])
    check(root["name"] == b"\x00", failures, "PVD root record name is not 0x00")
    check(root["flags"] & 0x02, failures, "PVD root record is not a directory")
    root_lba = root["extent"]
    root_size = root["size"]
    check(root_lba + (root_size + SECTOR - 1) // SECTOR <= sectors,
          failures, "root directory extends past the volume")

    # Path tables (little- and big-endian).
    for lba, endian, label in ((l_path_lba, "<", "L"), (m_path_lba, ">", "M")):
        table = data[lba * SECTOR:lba * SECTOR + path_size]
        check(len(table) == path_size, failures, f"{label} path table is short")
        name_len = table[0]
        check(name_len == 1, failures, f"{label} path table root name length is {name_len}")
        extent = struct.unpack_from(endian + "I", table, 2)[0]
        parent = struct.unpack_from(endian + "H", table, 6)[0]
        check(extent == root_lba, failures,
              f"{label} path table root extent {extent} != {root_lba}")
        check(parent == 1, failures, f"{label} path table root parent is {parent}")

    # Root directory records.
    directory = data[root_lba * SECTOR:root_lba * SECTOR + root_size]
    names = {}
    offset = 0
    while offset < len(directory):
        length = directory[offset]
        if length == 0:
            break
        record = parse_dir_record(directory[offset:offset + length])
        names[record["name"]] = record
        offset += length
    check(b"\x00" in names, failures, "root directory has no '.' record")
    check(b"\x01" in names, failures, "root directory has no '..' record")
    check(b"BOOT.IMG;1" in names, failures, "root directory has no BOOT.IMG;1 record")
    if b"\x00" in names:
        check(names[b"\x00"]["extent"] == root_lba, failures, "'.' extent is wrong")
        check(names[b"\x00"]["flags"] & 0x02, failures, "'.' is not a directory")
    if b"\x01" in names:
        check(names[b"\x01"]["extent"] == root_lba, failures, "'..' extent is wrong")

    boot_record = names.get(b"BOOT.IMG;1")
    check(boot_record is not None, failures, "boot image record missing")
    if boot_record is not None:
        check(boot_record["flags"] & 0x02 == 0, failures, "boot image is a directory")
        check(boot_record["size"] > 0, failures, "boot image is empty")
        check(boot_record["extent"] + (boot_record["size"] + SECTOR - 1) // SECTOR <= sectors,
              failures, "boot image extends past the volume")

    # Boot Record Volume Descriptor (El Torito).
    brvd = data[17 * SECTOR:18 * SECTOR]
    check(brvd[0] == 0, failures, "boot record type is not 0")
    check(brvd[1:6] == b"CD001", failures, "boot record identifier is not CD001")
    check(brvd[6] == 1, failures, "boot record version is not 1")
    check(brvd[7:39].rstrip(b"\x00") == b"EL TORITO SPECIFICATION",
          failures, "boot system identifier is not El Torito")
    catalog_lba = u32le(brvd, 71)
    check(0 < catalog_lba < sectors, failures,
          f"boot catalog LBA {catalog_lba} is outside the volume")

    # Boot catalog.
    catalog = data[catalog_lba * SECTOR:(catalog_lba + 1) * SECTOR]
    check(catalog[0] == 1, failures, "validation entry header is not 1")
    check(catalog[1] == 0, failures, "validation entry platform is not x86")
    check(catalog[30:32] == b"\x55\xaa", failures, "validation entry signature is wrong")
    checksum = sum(u16le(catalog, i) for i in range(0, 32, 2)) & 0xFFFF
    check(checksum == 0, failures, f"validation entry checksum is {checksum:#06x}, not 0")

    entry = catalog[32:64]
    check(entry[0] == 0x88, failures, "boot entry is not bootable")
    check(entry[1] == 0x00, failures, "boot entry is not no-emulation")
    check(u16le(entry, 2) == 0x07C0, failures, "boot load segment is not 0x07C0")
    boot_sectors = u16le(entry, 6)
    boot_lba = u32le(entry, 8)
    check(boot_sectors > 0, failures, "boot sector count is zero")
    check(boot_lba > 0 and boot_lba + (boot_sectors + 3) // 4 <= sectors,
          failures, f"boot image LBA {boot_lba} / {boot_sectors} sectors is outside the volume")

    # The boot image itself.
    boot_image = data[boot_lba * SECTOR:boot_lba * SECTOR + boot_sectors * 512]
    check(len(boot_image) == boot_sectors * 512, failures, "boot image is truncated")
    check(boot_image[510:512] == b"\x55\xaa", failures, "boot sector signature is missing")

    boot_path = pathlib.Path(args.boot)
    stage2_path = pathlib.Path(args.stage2)
    kernel_path = pathlib.Path(args.kernel)
    if boot_path.exists():
        check(boot_image[0:512] == boot_path.read_bytes(),
              failures, "boot image sector 0 does not match iso_boot.bin")
    if stage2_path.exists():
        stage2 = stage2_path.read_bytes()
        check(boot_image[0x400:0x400 + len(stage2)] == stage2,
              failures, "boot image stage2 does not match stage2_iso.bin")
    if kernel_path.exists():
        kernel = kernel_path.read_bytes()
        check(boot_image[0x4400:0x4400 + len(kernel)] == kernel,
              failures, "boot image kernel does not match kernel.bin")
        check(0x7C00 + 0x4400 + len(kernel) < 0x9FC00,
              failures, "boot image would overwrite the EBDA")

    if failures:
        print("ISO FAILURES:")
        for failure in failures:
            print(" -", failure)
        return 1

    print(
        f"PASS: ISO structure verified ({sectors} sectors, root at {root_lba}, "
        f"boot image at {boot_lba} / {boot_sectors} sectors)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
