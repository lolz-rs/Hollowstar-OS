; Hollowstar command runner and non-local exit.
[BITS 64]
default rel

section .text
global run_command_asm
global command_abort
global command_saved_rsp

; run_command_asm(entry, args_ptr, args_len, stack_top)
;   RDI = entry point
;   RSI = argument string pointer
;   RDX = argument string length
;   RCX = command stack top
run_command_asm:
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    mov [command_saved_rsp], rsp

    mov r8, rdi
    mov rdi, rsi
    mov rsi, rdx
    mov rsp, rcx
    and rsp, -16
    cld
    call r8

    sti
    mov rsp, [command_saved_rsp]
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    ret

; Called by the exit syscall. Abandons the command stack and returns to the
; Rust caller of run_command_asm().
command_abort:
    mov rsp, [command_saved_rsp]
    sti
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    ret

section .bss
align 8
command_saved_rsp: resq 1
