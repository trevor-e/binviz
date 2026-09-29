// Part of x86lib.lib (see x86lib-a.c): compiled with function sections,
// each function a COMDAT of its own, as MSVC's /Gy does.

const char *lib_find(const char *s, int c) {
    while (*s && *s != (char)c) {
        s++;
    }
    return *s == (char)c ? s : 0;
}

int lib_mix(int a, int b, int c) {
    int x = a * 31 + b;
    x ^= x >> 7;
    return x * 17 + c * 13 - (a >> 3);
}

// Never called, so not linked in: nothing may take its name.
int lib_unused(int a, int b) {
    return (a * 7 + b * 11) ^ (a >> 2) ^ 0x5a5a;
}
