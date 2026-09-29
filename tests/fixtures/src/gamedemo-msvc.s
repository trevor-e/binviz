# What MSVC 6's code in a game DLL does and clang's doesn't, for gamedemo.dll
# (see gamedemo.c): GetGameAPI copying the engine's table of functions into gi
# with rep movsd; the arguments of two calls through gi popped by one
# add esp; a loop's shared tail placed before the function's entry; a
# function with a second entry that takes its argument on the FPU stack; and
# two functions with the same code folded into one.

    .intel_syntax noprefix

    .def @feat.00; .scl 3; .type 0; .endef
    .globl @feat.00
@feat.00 = 1

    .section .rdata, "dr"
_msg_spawned:
    .asciz "spawned\n"
_msg_count:
    .asciz "%i entities\n"

    .text

# game_export_t *GetGameAPI(game_import_t *import): gi = *import (14 slots).
    .def _GetGameAPI; .scl 2; .type 32; .endef
    .globl _GetGameAPI
    .p2align 4, 0xcc
_GetGameAPI:
    push esi
    push edi
    mov ecx, 14
    mov esi, dword ptr [esp + 12]
    mov edi, offset _gi
    rep movsd
    mov dword ptr [_globals], 3
    mov dword ptr [_globals + 4], offset _InitGame
    mov dword ptr [_globals + 8], offset _G_RunFrame
    mov eax, offset _globals
    pop edi
    pop esi
    ret

# void spawn_messages(edict_t *ent, int count): two calls through gi, their
# arguments popped together; [esp+16] and [esp+12] are both count.
    .def _spawn_messages; .scl 2; .type 32; .endef
    .globl _spawn_messages
    .p2align 4, 0xcc
_spawn_messages:
    push esi
    mov esi, dword ptr [esp + 8]
    .byte 0x68
    .long _msg_spawned    # push offset msg_spawned
    call dword ptr [_gi + 4]
    push dword ptr [esp + 16]
    .byte 0x68
    .long _msg_count    # push offset msg_count
    push 2
    push esi
    call dword ptr [_gi + 8]
    add esp, 20
    mov eax, dword ptr [esp + 12]
    add dword ptr [esi + 16], eax
    pop esi
    ret

# char *find_char(char *s, int c): the found case's tail sits before the entry.
    .p2align 4, 0xcc
Lfound:
    lea eax, [edx - 1]
    pop ebx
    ret
    .def _find_char; .scl 2; .type 32; .endef
    .globl _find_char
_find_char:
    push ebx
    mov edx, dword ptr [esp + 8]
    mov bl, byte ptr [esp + 12]
Lnext_char:
    mov al, byte ptr [edx]
    inc edx
    cmp al, bl
    je Lfound
    test al, al
    jne Lnext_char
    xor eax, eax
    pop ebx
    ret

# double sqrt_either(double x), and a second entry taking x on the FPU stack
# (as MSVC's _CIsqrt does), which vec_length calls.
    .def _sqrt_either; .scl 2; .type 32; .endef
    .globl _sqrt_either
    .p2align 4, 0xcc
_sqrt_either:
    fld qword ptr [esp + 4]
    .def __CIsqrt_either; .scl 2; .type 32; .endef
    .globl __CIsqrt_either
__CIsqrt_either:
    fsqrt
    ret

# double vec_length(float *v)
    .def _vec_length; .scl 2; .type 32; .endef
    .globl _vec_length
    .p2align 4, 0xcc
_vec_length:
    mov eax, dword ptr [esp + 4]
    fld dword ptr [eax]
    fmul st(0), st(0)
    fld dword ptr [eax + 4]
    fmul st(0), st(0)
    faddp st(1), st(0)
    fld dword ptr [eax + 8]
    fmul st(0), st(0)
    faddp st(1), st(0)
    call __CIsqrt_either
    ret

# void gib_die(edict_t *self, edict_t *attacker, int damage) and debris_die:
# the same code, folded into one by the linker.
    .def _gib_die; .scl 2; .type 32; .endef
    .globl _gib_die
    .def _debris_die; .scl 2; .type 32; .endef
    .globl _debris_die
    .p2align 4, 0xcc
_gib_die:
_debris_die:
    mov eax, dword ptr [esp + 4]
    mov dword ptr [eax + 12], -1
    ret
