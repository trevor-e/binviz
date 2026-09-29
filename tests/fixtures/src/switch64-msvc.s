# What MSVC's x64 code does and clang's doesn't, for switch64.dll (see
# switch64.c): a switch whose jump table holds offsets from the image base
# (__ImageBase), kept in .text right after the function with the table of
# bytes that picks an entry for each case.

    .intel_syntax noprefix
    .text

# int msvc_switch64(int op): cases 1 to 9, the bytes pick one of four entries.
    .def msvc_switch64; .scl 2; .type 32; .endef
    .globl msvc_switch64
    .p2align 4, 0xcc
msvc_switch64:
    lea eax, [rcx - 1]
    cmp eax, 8
    ja .Ldefault
    lea rdx, [rip + __ImageBase]
    cdqe
    movzx eax, byte ptr [rdx + rax + .Lindex@IMGREL]
    mov r8d, dword ptr [rdx + 4*rax + .Ltable@IMGREL]
    add r8, rdx
    jmp r8
.Lhundred:
    mov eax, 100
    ret
.Ldouble:
    lea eax, [rcx + rcx]
    ret
.Ladd:
    lea eax, [rcx + 300]
    ret
.Ldefault:
    xor eax, eax
    ret
    .p2align 2, 0xcc
.Ltable:
    .long .Lhundred@IMGREL
    .long .Ldouble@IMGREL
    .long .Ladd@IMGREL
    .long .Ldefault@IMGREL
.Lindex:
    .byte 0, 1, 1, 2, 3, 3, 0, 2, 1

    .section .drectve,"yn"
    .ascii " /EXPORT:msvc_switch64"
