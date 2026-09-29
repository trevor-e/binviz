// Switches compiled to jump tables in 64-bit code (see switch64-msvc.s for
// MSVC's form). clang's tables hold offsets from the table itself in
// position-independent code (the PE DLL, the PIE) and addresses in code that
// isn't (the static ELF).

#ifdef _WIN32
#define EXPORT __declspec(dllexport)
#else
#define EXPORT __attribute__((visibility("default")))
#endif
#define NOINLINE __attribute__((noinline))

// Cases 0 to 7, each its own code.
EXPORT NOINLINE int dispatch(int op, int x) {
    switch (op) {
    case 0:
        return x + 1;
    case 1:
        return x * 7;
    case 2:
        return x ^ 0x55;
    case 3:
        return x - 9;
    case 4:
        return x << 3;
    case 5:
        return x / 3;
    case 6:
        return ~x;
    case 7:
        return x * x;
    default:
        return -1;
    }
}

// Cases from 10, with holes that go where the rest do.
EXPORT NOINLINE int opcode(int op, int x) {
    switch (op) {
    case 10:
        return x + 3;
    case 11:
        return x * 5;
    case 12:
        return x - 7;
    case 14:
        return x ^ 9;
    case 15:
        return x >> 1;
    case 17:
        return x | 0x100;
    default:
        return 0;
    }
}

#ifndef _WIN32
int start(void) {
    return dispatch(3, 4) + opcode(12, 5);
}
#endif
