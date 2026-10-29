; Hollowstar MBR boot sector
; Loads stage2 from LBA 2 into 0x8000 and jumps to it.
; BIOS, real mode, 512 bytes.

[BITS 16]
[ORG 0x7C00]

%define STAGE2_LBA      2
%define STAGE2_SECTORS  32
%define STAGE2_SEG      0x0000
%define STAGE2_OFF      0x8000

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    sti

    mov [boot_drive], dl

    ; Require INT 13h LBA extensions.
    mov ah, 0x41
    mov bx, 0x55AA
    mov dl, [boot_drive]
    int 0x13
    jc disk_error
    cmp bx, 0xAA55
    jne disk_error

    mov byte [retries], 3
.retry:
    mov si, dap
    mov ah, 0x42
    mov dl, [boot_drive]
    int 0x13
    jnc .loaded

    ; Reset the disk and try again.
    xor ah, ah
    mov dl, [boot_drive]
    int 0x13
    dec byte [retries]
    jnz .retry
    jmp disk_error

.loaded:
    mov dl, [boot_drive]
    jmp STAGE2_SEG:STAGE2_OFF

disk_error:
    mov si, msg_disk
    call print
.hang:
    cli
    hlt
    jmp .hang

print:
    lodsb
    test al, al
    jz .done
    mov ah, 0x0E
    mov bx, 0x0007
    int 0x10
    jmp print
.done:
    ret

boot_drive: db 0
retries:    db 0

align 4
dap:
    db 0x10, 0
    dw STAGE2_SECTORS
    dw STAGE2_OFF
    dw STAGE2_SEG
    dq STAGE2_LBA

msg_disk: db "Hollowstar: disk read error", 13, 10, 0

times 510 - ($ - $$) db 0
dw 0xAA55
