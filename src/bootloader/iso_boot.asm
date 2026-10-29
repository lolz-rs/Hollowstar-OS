; Hollowstar El Torito no-emulation boot sector.
;
; The ISO builder places this sector at the start of the boot image, pads one
; more sector, then puts stage2 at offset 0x2000 and the kernel at 0x4400.
; Firmware loads the whole image at 0x7C00, so stage2 is already at 0x8000.

[BITS 16]
[ORG 0x7C00]

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    sti
    jmp 0x0000:0x8000

times 510 - ($ - $$) db 0
dw 0xAA55
