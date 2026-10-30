; Hollowstar stage2 bootloader
; Real mode -> protected mode -> long mode, loads the kernel and hands off.
;
; Disk layout:
;   LBA 0        boot sector
;   LBA 1        boot info (136 bytes)
;   LBA 2..33    this stage2 (16 KiB max)
;   LBA 64..     flat kernel image
;
; ISO layout (El Torito no-emulation boot image, loaded at 0x7C00):
;   0x7C00       ISO boot sector
;   0x7E00       512 bytes of padding
;   0x8000       this stage2
;   0xC000       flat kernel image
;
; Page tables are built from the E820 map so every usable byte of RAM is
; identity-mapped with 2 MiB pages. One PML4 entry covers 512 GiB.

[BITS 16]
[ORG 0x8000]

%define BOOT_INFO       0x5000
%define MEMMAP          0x5200
%define VBE_CTRL        0x5900          ; 512-byte VBE controller info
%define VBE_MODE        0x5C00          ; 256-byte VBE mode info
%define FONT_SEG        0x9E00          ; BIOS 8x16 font at linear 0x9E000
%define FONT_ADDR       0x9E000
%define BOOT_MAGIC      0x52545348      ; "HSTR" in little endian
%define KERNEL_TMP_SEG  0x1000          ; 0x10000
%define KERNEL_MAX      (512 * 1024)
%define PML4_ADDR       0x1000
%define PDPT_ADDR       0x2000
%define PD_ADDR         0x200000
%define MAX_PD          512             ; 512 x 1 GiB = 512 GiB

; Preferred VBE mode. The default is 1080p; build.ps1 can override it with
; -GfxWidth/-GfxHeight for a higher-resolution default.
%ifndef GFX_WIDTH
%define GFX_WIDTH       1920
%endif
%ifndef GFX_HEIGHT
%define GFX_HEIGHT      1080
%endif

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    sti

%ifdef ISO_BOOT
    ; The firmware already loaded the whole boot image. Fill in boot info
    ; without touching a disk.
    mov dword [BOOT_INFO + 0], BOOT_MAGIC
    mov dword [BOOT_INFO + 4], 2
    mov dword [BOOT_INFO + 8], 0
    mov dword [BOOT_INFO + 12], 0
    mov dword [BOOT_INFO + 16], KERNEL_SECTORS
    mov dword [BOOT_INFO + 20], 0
    mov dword [BOOT_INFO + 24], 0x100000
    mov dword [BOOT_INFO + 28], 0
    mov dword [BOOT_INFO + 32], 0x100000
    mov dword [BOOT_INFO + 36], 0
    mov dword [BOOT_INFO + 40], MEMMAP
    mov dword [BOOT_INFO + 44], 0
    mov dword [BOOT_INFO + 48], 0
    mov dword [BOOT_INFO + 52], 24
    mov dword [BOOT_INFO + 56], KERNEL_SIZE
    mov dword [BOOT_INFO + 60], 0
%else
    mov [boot_drive], dl

    ; Read boot info sector (LBA 1) to 0x5000.
    mov word [dap_count], 1
    mov word [dap_offset], 0
    mov word [dap_segment], BOOT_INFO >> 4
    mov dword [dap_lba], 1
    mov dword [dap_lba + 4], 0
    call disk_read
    jc boot_error

    cmp dword [BOOT_INFO], BOOT_MAGIC
    jne boot_error

    movzx eax, byte [boot_drive]
    mov dword [BOOT_INFO + 20], eax
%endif

    call enable_a20
    call do_e820
    call vbe_setup
%ifndef ISO_BOOT
    call load_kernel
%endif

    jmp enter_pm

; ---------------------------------------------------------------------------
; Disk helpers (disk image path only)
; ---------------------------------------------------------------------------
disk_read:
    mov byte [retries], 3
.retry:
    mov si, dap
    mov ah, 0x42
    mov dl, [boot_drive]
    int 0x13
    jnc .ok
    xor ah, ah
    mov dl, [boot_drive]
    int 0x13
    dec byte [retries]
    jnz .retry
    stc
    ret
.ok:
    clc
    ret

; ---------------------------------------------------------------------------
; A20 gate
; ---------------------------------------------------------------------------
enable_a20:
    mov ax, 0x2401
    int 0x15

    in al, 0x92
    or al, 0x02
    and al, 0xFE
    out 0x92, al

    ; Standard wrap test: with A20 off, FFFF:0510 aliases 0000:0500.
    xor ax, ax
    mov ds, ax
    mov byte [0x0500], 0x00
    mov ax, 0xFFFF
    mov ds, ax
    mov byte [0x0510], 0xFF
    xor ax, ax
    mov ds, ax
    cmp byte [0x0500], 0x00
    jne .fail
    ret
.fail:
    mov si, msg_a20
    jmp boot_error

; ---------------------------------------------------------------------------
; E820 memory map
; ---------------------------------------------------------------------------
do_e820:
    mov di, MEMMAP
    xor ebx, ebx
    mov dword [memmap_n], 0
.loop:
    mov eax, 0xE820
    mov edx, 0x534D4150
    mov ecx, 24
    mov dword [di + 20], 1       ; ACPI 3.0: entry valid
    int 0x15
    jc .done
    cmp eax, 0x534D4150
    jne .done
    cmp ecx, 20
    jb .skip
    mov eax, [di + 16]
    test eax, eax
    jz .skip
    inc dword [memmap_n]
    add di, 24
    cmp dword [memmap_n], 64
    jae .done
.skip:
    test ebx, ebx
    jz .done
    jmp .loop
.done:
    mov eax, [memmap_n]
    mov [BOOT_INFO + 48], eax
    mov dword [BOOT_INFO + 52], 24
    ret

; ---------------------------------------------------------------------------
; VBE linear framebuffer setup
;
; Selects a 32-bit direct-color linear mode (1920x1080 by default, with
; 16:9 and 4:3 fallbacks), copies the BIOS 8x16 font to 0x9E000 and records
; everything in BootInfo.
; Any failure simply leaves the graphics fields zeroed so the kernel falls
; back to the VGA text console.
; ---------------------------------------------------------------------------
vbe_setup:
    xor eax, eax
    mov [BOOT_INFO + 64], eax
    mov [BOOT_INFO + 68], eax
    mov [BOOT_INFO + 72], eax
    mov [BOOT_INFO + 76], eax
    mov [BOOT_INFO + 80], eax
    mov [BOOT_INFO + 84], eax
    mov [BOOT_INFO + 88], eax
    mov [BOOT_INFO + 92], eax
    mov [BOOT_INFO + 96], eax
    mov [BOOT_INFO + 100], eax
    mov [BOOT_INFO + 104], eax
    mov [BOOT_INFO + 108], eax
    mov [BOOT_INFO + 112], eax
    mov [BOOT_INFO + 116], eax
    mov [BOOT_INFO + 120], eax
    mov [BOOT_INFO + 124], eax
    mov [BOOT_INFO + 128], eax
    mov [BOOT_INFO + 132], eax

    xor ax, ax
    mov ds, ax
    mov es, ax

    ; VBE 2.0+ controller information.
    mov dword [VBE_CTRL], 'VBE2'
    mov ax, 0x4F00
    mov di, VBE_CTRL
    int 0x10
    cmp ax, 0x004F
    jne .fail
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov eax, [VBE_CTRL]
    cmp eax, 'VBE2'
    je .sig_ok
    cmp eax, 'VESA'
    jne .fail
.sig_ok:

    ; Copy the 8x16 font while the BIOS is still in text mode.
    call copy_font

    ; Find the best available mode. 1080p is the default; the rest are
    ; progressively smaller 16:9 and 4:3 fallbacks for limited VRAM.
    mov ax, GFX_WIDTH
    mov bx, GFX_HEIGHT
    call find_mode
    cmp cx, 0xFFFF
    jne .found
    mov ax, 1600
    mov bx, 900
    call find_mode
    cmp cx, 0xFFFF
    jne .found
    mov ax, 1280
    mov bx, 720
    call find_mode
    cmp cx, 0xFFFF
    jne .found
    mov ax, 1024
    mov bx, 768
    call find_mode
    cmp cx, 0xFFFF
    jne .found
    mov ax, 800
    mov bx, 600
    call find_mode
    cmp cx, 0xFFFF
    jne .found
    mov ax, 640
    mov bx, 480
    call find_mode
    cmp cx, 0xFFFF
    je .fail

.found:
    mov [vbe_mode_num], cx
    ; Switch to the chosen mode with the linear-framebuffer bit set.
    xor ax, ax
    mov es, ax
    mov ax, 0x4F02
    mov bx, [vbe_mode_num]
    or bx, 0x4000
    int 0x10
    cmp ax, 0x004F
    jne .fail
    xor ax, ax
    mov ds, ax
    mov es, ax

    ; Re-read the mode info: some BIOSes update pitch/base on mode set.
    xor ax, ax
    mov es, ax
    mov ax, 0x4F01
    mov cx, [vbe_mode_num]
    mov di, VBE_MODE
    int 0x10
    cmp ax, 0x004F
    jne .fail
    xor ax, ax
    mov ds, ax
    mov es, ax

    mov eax, [VBE_MODE + 40]     ; physical framebuffer base
    mov [BOOT_INFO + 64], eax
    mov dword [BOOT_INFO + 68], 0

    movzx eax, word [VBE_MODE + 16]
    mov [BOOT_INFO + 72], eax    ; pitch in bytes
    movzx eax, word [VBE_MODE + 18]
    mov [BOOT_INFO + 76], eax    ; width
    movzx eax, word [VBE_MODE + 20]
    mov [BOOT_INFO + 80], eax    ; height
    movzx eax, byte [VBE_MODE + 25]
    mov [BOOT_INFO + 84], eax    ; bpp

    ; Colour masks from the direct-colour fields.
    movzx ecx, byte [VBE_MODE + 31]
    mov eax, 1
    shl eax, cl
    dec eax
    movzx ecx, byte [VBE_MODE + 32]
    shl eax, cl
    mov [BOOT_INFO + 88], eax

    movzx ecx, byte [VBE_MODE + 33]
    mov eax, 1
    shl eax, cl
    dec eax
    movzx ecx, byte [VBE_MODE + 34]
    shl eax, cl
    mov [BOOT_INFO + 92], eax

    movzx ecx, byte [VBE_MODE + 35]
    mov eax, 1
    shl eax, cl
    dec eax
    movzx ecx, byte [VBE_MODE + 36]
    shl eax, cl
    mov [BOOT_INFO + 96], eax

    mov dword [BOOT_INFO + 100], 7  ; available | linear | font

    mov dword [BOOT_INFO + 104], FONT_ADDR
    mov dword [BOOT_INFO + 108], 0
    mov dword [BOOT_INFO + 112], 8
    mov dword [BOOT_INFO + 116], 16

    mov dword [BOOT_INFO + 120], 0x01000000
    mov dword [BOOT_INFO + 124], 0
    mov eax, [BOOT_INFO + 72]
    movzx ecx, word [BOOT_INFO + 80]
    imul eax, ecx
    mov [BOOT_INFO + 128], eax
    mov dword [BOOT_INFO + 132], 0
    ret

.fail:
    ret

; Find a 32-bpp direct-colour linear mode with the requested size.
; Input: AX = width, BX = height. Output: CX = mode number or 0xFFFF.
find_mode:
    push ax
    push bx
    xor dx, dx
    mov ds, dx
    mov es, dx
    mov si, [VBE_CTRL + 14]
    mov dx, [VBE_CTRL + 16]
    mov fs, dx
.scan:
    mov cx, [fs:si]
    cmp cx, 0xFFFF
    je .none
    add si, 2
    mov [vbe_mode_num], cx

    push ax
    push bx
    push si
    push fs
    xor ax, ax
    mov es, ax
    mov ax, 0x4F01
    mov di, VBE_MODE
    int 0x10
    mov dx, ax
    pop fs
    pop si
    pop bx
    pop ax
    cmp dx, 0x004F
    jne .scan
    xor dx, dx
    mov ds, dx
    mov es, dx

    mov dx, [VBE_MODE + 0]
    test dx, 0x0001              ; supported
    jz .scan
    test dx, 0x0010              ; graphics
    jz .scan
    test dx, 0x0080              ; linear framebuffer
    jz .scan
    cmp byte [VBE_MODE + 27], 6  ; direct colour
    jne .scan
    cmp byte [VBE_MODE + 25], 32 ; 32 bpp
    jne .scan
    cmp word [VBE_MODE + 18], ax
    jne .scan
    cmp word [VBE_MODE + 20], bx
    jne .scan
    mov cx, [vbe_mode_num]
    pop bx
    pop ax
    ret
.none:
    mov cx, 0xFFFF
    pop bx
    pop ax
    ret

; Copy 256 8x16 glyphs from the BIOS font to FONT_SEG:0000.
copy_font:
    push ds
    push es
    mov ax, 0x1130
    mov bh, 0x06
    int 0x10
    mov ax, es
    mov ds, ax
    mov si, bp
    mov ax, FONT_SEG
    mov es, ax
    xor di, di
    mov cx, 4096
    cld
    rep movsb
    pop es
    pop ds
    ret

; ---------------------------------------------------------------------------
; Load flat kernel to 0x10000 (disk image path only)
; ---------------------------------------------------------------------------
load_kernel:
    mov eax, [BOOT_INFO + 16]    ; sectors
    test eax, eax
    jz .bad
    cmp eax, KERNEL_MAX / 512
    ja .bad
    mov [remaining], eax

    mov eax, [BOOT_INFO + 8]
    mov [cur_lba], eax
    mov eax, [BOOT_INFO + 12]
    mov [cur_lba + 4], eax

    mov word [dest_seg], KERNEL_TMP_SEG
.loop:
    mov eax, [remaining]
    test eax, eax
    jz .done
    cmp eax, 64
    jbe .chunk_ok
    mov eax, 64
.chunk_ok:
    mov [chunk], ax
    mov [dap_count], ax
    mov word [dap_offset], 0
    mov ax, [dest_seg]
    mov [dap_segment], ax
    mov eax, [cur_lba]
    mov [dap_lba], eax
    mov eax, [cur_lba + 4]
    mov [dap_lba + 4], eax

    call disk_read
    jc .bad

    movzx eax, word [chunk]
    add dword [cur_lba], eax
    adc dword [cur_lba + 4], 0

    movzx eax, word [chunk]
    sub [remaining], eax

    movzx eax, word [chunk]
    shl eax, 5                   ; sectors * 512 / 16
    add [dest_seg], ax
    jmp .loop
.done:
    ret
.bad:
    mov si, msg_kernel
    jmp boot_error

; ---------------------------------------------------------------------------
; Protected mode / long mode
; ---------------------------------------------------------------------------
enter_pm:
    cli
    lgdt [gdt_desc]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:pm32

[BITS 32]
pm32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Copy the kernel to its final load address.
    cld
%ifdef ISO_BOOT
    mov esi, 0xC000
%else
    mov esi, KERNEL_TMP_SEG << 4
%endif
    mov edi, 0x100000
    mov ecx, [BOOT_INFO + 56]    ; kernel_size
    rep movsb

    ; Clear PML4 and PDPT (0x1000..0x2FFF).
    mov edi, PML4_ADDR
    xor eax, eax
    mov ecx, 0x2000 / 4
    rep stosd

    ; Clear the page-directory area (0x200000..0x400000).
    mov edi, PD_ADDR
    mov ecx, (MAX_PD * 0x1000) / 4
    rep stosd

    ; Find the highest usable end address in the E820 map.
    xor ebx, ebx                 ; max end, low 32 bits
    xor ebp, ebp                 ; max end, high 32 bits
    mov esi, MEMMAP
    mov ecx, [BOOT_INFO + 48]
    test ecx, ecx
    jz .no_entries
.e820_loop:
    cmp dword [esi + 16], 1      ; type == usable
    jne .e820_next
    mov eax, [esi]
    mov edx, [esi + 4]
    add eax, [esi + 8]
    adc edx, [esi + 12]
    cmp edx, ebp
    ja .e820_update
    jb .e820_next
    cmp eax, ebx
    jbe .e820_next
.e820_update:
    mov ebx, eax
    mov ebp, edx
.e820_next:
    add esi, 24
    dec ecx
    jnz .e820_loop
.no_entries:

    ; pd_count = ceil(max_end / 1 GiB), clamped to [1, MAX_PD].
    mov eax, ebx
    mov edx, ebp
    add eax, 0x3FFFFFFF
    adc edx, 0
    shrd eax, edx, 30
    shr edx, 30
    test eax, eax
    jnz .pd_count_ok
    mov eax, 1
.pd_count_ok:
    cmp eax, MAX_PD
    jbe .pd_count_cap
    mov eax, MAX_PD
.pd_count_cap:
    mov [ram_pd_count], eax
    mov [pd_count], eax

    ; Extend the page tables far enough to cover the linear framebuffer.
    test dword [BOOT_INFO + 100], 1
    jz .fb_done
    mov eax, [BOOT_INFO + 64]
    mov edx, [BOOT_INFO + 68]
    add eax, [BOOT_INFO + 128]
    adc edx, 0
    add eax, 0x3FFFFFFF
    adc edx, 0
    shrd eax, edx, 30
    shr edx, 30
    test eax, eax
    jnz .fb_count_ok
    mov eax, 1
.fb_count_ok:
    cmp eax, MAX_PD
    jbe .fb_count_cap
    mov eax, MAX_PD
.fb_count_cap:
    cmp eax, [pd_count]
    jbe .fb_done
    mov [pd_count], eax
.fb_done:

    ; PML4[0] -> PDPT
    mov dword [PML4_ADDR], PDPT_ADDR | 0x03
    mov dword [PML4_ADDR + 4], 0

    ; PDPT[i] -> PD[i]
    mov edi, PDPT_ADDR
    mov ebx, PD_ADDR
    mov ecx, [pd_count]
.pdpt_loop:
    mov eax, ebx
    or eax, 0x03
    mov [edi], eax
    mov dword [edi + 4], 0
    add ebx, 0x1000
    add edi, 8
    dec ecx
    jnz .pdpt_loop

    ; 2 MiB identity pages for every 1 GiB region.
    mov edi, PD_ADDR
    mov eax, 0x83                ; present | writable | 2 MiB page
    xor edx, edx
    mov ecx, [pd_count]
    shl ecx, 9                   ; pd_count * 512 entries
.pd_loop:
    mov [edi], eax
    mov [edi + 4], edx
    add eax, 0x200000
    adc edx, 0
    add edi, 8
    dec ecx
    jnz .pd_loop

    ; Mark framebuffer 2 MiB pages with the PAT bit (entry 4 = WC).
    test dword [BOOT_INFO + 100], 1
    jz .pat_done
    mov eax, [BOOT_INFO + 64]
    mov edx, [BOOT_INFO + 68]
    shrd eax, edx, 21
    shr edx, 21
    mov ebx, eax                 ; first PDE index
    mov eax, [BOOT_INFO + 64]
    mov edx, [BOOT_INFO + 68]
    add eax, [BOOT_INFO + 128]
    adc edx, 0
    add eax, 0x1FFFFF
    adc edx, 0
    shrd eax, edx, 21
    shr edx, 21
    mov ecx, [pd_count]
    shl ecx, 9
    cmp eax, ecx
    jbe .pat_end_ok
    mov eax, ecx
.pat_end_ok:
.pat_loop:
    cmp ebx, eax
    jae .pat_done
    mov edi, PD_ADDR
    lea edi, [edi + ebx * 8]
    or dword [edi], 0x1000
    inc ebx
    jmp .pat_loop
.pat_done:

    ; Record RAM mapping (not the framebuffer aperture) in KiB.
    mov eax, [ram_pd_count]
    shl eax, 20                  ; * 1 GiB in KiB
    mov [BOOT_INFO + 60], eax

    ; IA32_PAT entry 4 = write-combining.
    mov ecx, 0x277
    rdmsr
    and edx, 0xFFFFFF00
    or edx, 0x00000001
    wrmsr

    ; Enable PAE.
    mov eax, cr4
    or eax, 0x20
    mov cr4, eax

    ; Load PML4.
    mov eax, PML4_ADDR
    mov cr3, eax

    ; Enable long mode.
    mov ecx, 0xC0000080
    rdmsr
    or eax, 0x100
    wrmsr

    ; Enable paging + protection.
    mov eax, cr0
    or eax, 0x80000001
    mov cr0, eax

    jmp 0x18:long_mode

[BITS 64]
long_mode:
    mov ax, 0x20
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    mov rsp, 0x90000
    xor rbp, rbp
    mov rdi, BOOT_INFO
    mov rax, 0x100000
    call rax

.hang:
    cli
    hlt
    jmp .hang

; ---------------------------------------------------------------------------
; Data
; ---------------------------------------------------------------------------
[BITS 16]
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

boot_error:
    ; A VBE mode may already be active; return to plain text so the message
    ; is visible on real hardware as well as in emulators.
    push si
    mov ax, 0x0003
    int 0x10
    pop si
    call print
.hang:
    cli
    hlt
    jmp .hang

boot_drive:   db 0
retries:      db 0
chunk:        dw 0
remaining:    dd 0
dest_seg:     dw 0
cur_lba:      dq 0
memmap_n:     dd 0
pd_count:     dd 1
ram_pd_count: dd 1
vbe_mode_num: dw 0xFFFF

align 4
dap:
    db 0x10, 0
dap_count:   dw 0
dap_offset:  dw 0
dap_segment: dw 0
dap_lba:     dq 0

align 8
gdt_start:
    dq 0
    dq 0x00CF9A000000FFFF        ; 0x08 32-bit code
    dq 0x00CF92000000FFFF        ; 0x10 32-bit data
    dq 0x00AF9A000000FFFF        ; 0x18 64-bit code
    dq 0x00AF92000000FFFF        ; 0x20 64-bit data
gdt_end:
gdt_desc:
    dw gdt_end - gdt_start - 1
    dd gdt_start

msg_a20:    db "Hollowstar: A20 failed", 13, 10, 0
msg_kernel: db "Hollowstar: kernel load failed", 13, 10, 0
msg_boot:   db "Hollowstar: boot failed", 13, 10, 0

times (32 * 512) - ($ - $$) db 0
