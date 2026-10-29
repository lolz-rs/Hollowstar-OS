; The /commands archive, embedded in kernel .rodata.
[BITS 64]

section .rodata
global commands_archive_start
global commands_archive_end

align 8
commands_archive_start:
    incbin "commands.hsb"
commands_archive_end:
