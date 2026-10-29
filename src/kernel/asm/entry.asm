; Hollowstar kernel entry (64-bit).
[BITS 64]

section .text.entry
global _start
extern kernel_main
extern __bss_start
extern __bss_end

_start:
    cli
    cld
    mov rsp, 0x90000
    xor rbp, rbp

    ; Enable the FPU and SSE before any Rust code can use them.
    mov rax, cr0
    and rax, 0xFFFFFFFFFFFFFFFB   ; clear EM
    or rax, 0x2                   ; set MP
    mov cr0, rax
    mov rax, cr4
    or rax, 0x600                 ; OSFXSR | OSXMMEXCPT
    mov cr4, rax
    fninit

    ; Boot info pointer arrives in RDI. Save it before rep stosb.
    mov r12, rdi

    lea rdi, [rel __bss_start]
    lea rcx, [rel __bss_end]
    sub rcx, rdi
    xor eax, eax
    rep stosb

    mov rdi, r12
    call kernel_main

.hang:
    cli
    hlt
    jmp .hang
