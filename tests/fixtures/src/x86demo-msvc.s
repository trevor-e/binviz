# What MSVC's 32-bit code does and clang's doesn't, for x86demo.exe (see
# x86demo.cpp): a switch whose jump table and byte index table sit in .text
# right after the function, the hot-patchable prologue, calls that never
# return followed directly by the next function, tail calls (one on a
# condition) to functions nothing else refers to, a structured exception
# handler frame, and a __finally block called in place.

    .intel_syntax noprefix

    .def @feat.00; .scl 3; .type 0; .endef
    .globl @feat.00
@feat.00 = 1

    .text

# switch (op) { ... } for op in 1..9: the index table picks one of four cases.
    .def _msvc_switch; .scl 2; .type 32; .endef
    .globl _msvc_switch
    .p2align 4, 0xcc
_msvc_switch:
    mov eax, dword ptr [esp + 4]
    dec eax
    cmp eax, 8
    ja Ldefault
    movzx eax, byte ptr [eax + Lindex]
    jmp dword ptr [4*eax + Ltable]
Lhundred:
    mov eax, 100
    ret
Ldouble:
    mov eax, dword ptr [esp + 4]
    add eax, eax
    ret
Ladd:
    mov eax, dword ptr [esp + 4]
    add eax, 300
    ret
Ldefault:
    xor eax, eax
    ret
    .p2align 2, 0x90
Ltable:
    .long Lhundred
    .long Ldouble
    .long Ladd
    .long Ldefault
Lindex:
    .byte 0, 1, 1, 2, 3, 3, 0, 2, 1

# Straight after the tables; only a pointer in .data refers to it.
    .def _hotpatched; .scl 2; .type 32; .endef
    .globl _hotpatched
_hotpatched:
    mov edi, edi
    push ebp
    mov ebp, esp
    mov eax, dword ptr [ebp + 8]
    imul eax, eax, 13
    pop ebp
    ret

# Ends in a call to fatal(), which never returns: the next function follows
# directly, and only a pointer in .data refers to it.
    .def _checked_index; .scl 2; .type 32; .endef
    .globl _checked_index
    .p2align 4, 0xcc
_checked_index:
    mov eax, dword ptr [esp + 4]
    cmp eax, 16
    jae Lout_of_range
    lea eax, [eax + 2*eax]
    ret
Lout_of_range:
    push 7
    call _fatal

    .def _after_fatal; .scl 2; .type 32; .endef
    .globl _after_fatal
_after_fatal:
    mov eax, dword ptr [esp + 4]
    not eax
    ret

# The same after ExitProcess, called through the import address table.
    .def _quit; .scl 2; .type 32; .endef
    .globl _quit
    .p2align 4, 0xcc
_quit:
    push dword ptr [esp + 4]
    call dword ptr [__imp__ExitProcess@4]

    .def _after_exit; .scl 2; .type 32; .endef
    .globl _after_exit
_after_exit:
    mov eax, dword ptr [esp + 4]
    neg eax
    ret

# A tail call to a function further on, which nothing else refers to.
    .def _tail_caller; .scl 2; .type 32; .endef
    .globl _tail_caller
    .p2align 4, 0xcc
_tail_caller:
    mov eax, dword ptr [esp + 4]
    add eax, 5
    mov dword ptr [esp + 4], eax
    jmp _tail_target

# An exception handler frame: the handler is pushed as an immediate and
# listed in the image's table of safe exception handlers.
    .def _with_handler; .scl 2; .type 32; .endef
    .globl _with_handler
    .p2align 4, 0xcc
_with_handler:
    push ebp
    mov ebp, esp
    # push offset _seh_handler (the Intel syntax parser can't write this one)
    .att_syntax
    pushl $_seh_handler
    .intel_syntax noprefix
    push dword ptr fs:[0]
    mov dword ptr fs:[0], esp
    mov eax, dword ptr [ebp + 8]
    imul eax, eax, 3
    mov ecx, dword ptr [esp]
    mov dword ptr fs:[0], ecx
    mov esp, ebp
    pop ebp
    ret

# __try { ... } __finally { ... }: the __finally block sits in the middle of
# the function, which calls it in place and branches over it to go on; the
# unwinder enters it (restoring a register first) through the scope table.
    .def _with_finally; .scl 2; .type 32; .endef
    .globl _with_finally
    .p2align 4, 0xcc
_with_finally:
    push ebp
    mov ebp, esp
    push esi
    mov esi, dword ptr [ebp + 8]
Lfinally_loop:
    call Lfinally
    dec esi
    test esi, esi
    jle Lfinally_done
    jmp Lfinally_loop
Lfinally_unwind:
    mov esi, dword ptr [ebp - 4]
Lfinally:
    inc dword ptr [_finally_count]
    ret
Lfinally_done:
    mov eax, esi
    pop esi
    pop ebp
    ret

# A tail call on a condition, as MSVC 2019 writes them: to a function
# further on, which nothing else refers to.
    .def _cond_tail; .scl 2; .type 32; .endef
    .globl _cond_tail
    .p2align 4, 0xcc
_cond_tail:
    mov eax, dword ptr [esp + 4]
    test eax, eax
    je _cond_target
    lea eax, [eax + 4*eax]
    ret

    .def _seh_handler; .scl 2; .type 32; .endef
    .globl _seh_handler
    .safeseh _seh_handler
    .p2align 4, 0xcc
_seh_handler:
    mov eax, 1
    ret

    .def _tail_target; .scl 2; .type 32; .endef
    .globl _tail_target
    .p2align 4, 0xcc
_tail_target:
    mov eax, dword ptr [esp + 4]
    shl eax, 2
    ret

    .def _cond_target; .scl 2; .type 32; .endef
    .globl _cond_target
    .p2align 4, 0xcc
_cond_target:
    mov eax, -1
    ret

    .section .rdata,"dr"
    .p2align 2
Lscope_table:
    .long -1
    .long 0
    .long Lfinally_unwind

    .data
    .p2align 2
    .globl _finally_count
_finally_count:
    .long 0
    .globl _msvc_hooks
_msvc_hooks:
    .long _hotpatched
    .long _after_fatal
    .long _quit
    .long _after_exit
