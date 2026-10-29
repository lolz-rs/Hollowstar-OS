; Hollowstar interrupt stubs.
;
; Stack frame layout after isr_common pushes registers:
;   r15 r14 r13 r12 r11 r10 r9 r8 rbp rdi rsi rdx rcx rbx rax
;   vector error rip cs rflags rsp ss
[BITS 64]

extern interrupt_handler

%macro ISR_NOERR 1
global isr%1
isr%1:
    push qword 0
    push qword %1
    jmp isr_common
%endmacro

%macro ISR_ERR 1
global isr%1
isr%1:
    push qword %1
    jmp isr_common
%endmacro

section .text

ISR_NOERR 0
ISR_NOERR 1
ISR_NOERR 2
ISR_NOERR 3
ISR_NOERR 4
ISR_NOERR 5
ISR_NOERR 6
ISR_NOERR 7
ISR_ERR   8
ISR_NOERR 9
ISR_ERR   10
ISR_ERR   11
ISR_ERR   12
ISR_ERR   13
ISR_ERR   14
ISR_NOERR 15
ISR_NOERR 16
ISR_ERR   17
ISR_NOERR 18
ISR_NOERR 19
ISR_NOERR 20
ISR_ERR   21
ISR_NOERR 22
ISR_NOERR 23
ISR_NOERR 24
ISR_NOERR 25
ISR_NOERR 26
ISR_NOERR 27
ISR_NOERR 28
ISR_ERR   29
ISR_ERR   30
ISR_NOERR 31

irq0_stub:
    push qword 0
    push qword 32
    jmp isr_common

irq1_stub:
    push qword 0
    push qword 33
    jmp isr_common

irq4_stub:
    push qword 0
    push qword 36
    jmp isr_common

irq12_stub:
    push qword 0
    push qword 44
    jmp isr_common

default_stub:
    push qword 0
    push qword 0xFF
    jmp isr_common

syscall_stub:
    push qword 0
    push qword 0x80
    jmp isr_common

isr_common:
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    mov rdi, rsp
    mov rbp, rsp
    and rsp, -16
    call interrupt_handler
    mov rsp, rbp

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax

    add rsp, 16
    iretq

section .rodata
align 8
global isr_stub_table
isr_stub_table:
%assign i 0
%rep 32
    dq isr%+i
%assign i i+1
%endrep
    dq irq0_stub
    dq irq1_stub
    dq default_stub
    dq default_stub
    dq irq4_stub
%assign i 37
%rep (44 - 37)
    dq default_stub
%assign i i+1
%endrep
    dq irq12_stub
%assign i 45
%rep (0x80 - 45)
    dq default_stub
%assign i i+1
%endrep
    dq syscall_stub
%assign i 0x81
%rep (256 - 0x81)
    dq default_stub
%assign i i+1
%endrep
