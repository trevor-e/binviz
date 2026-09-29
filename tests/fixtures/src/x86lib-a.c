// Part of x86lib.lib, the stand-in for a statically linked C runtime in
// x86match.exe (see x86match.cpp): compiled without function sections, so
// its functions share one .text and are padded out to each other.

int lib_checksum(const unsigned char *p, int n) {
    unsigned sum = 0x1234;
    for (int i = 0; i < n; i++) {
        sum = (sum << 5) + sum + p[i];
    }
    return (int)(sum ^ (sum >> 16));
}

void lib_fill(unsigned char *p, int n, int value) {
    for (int i = 0; i < n; i++) {
        p[i] = (unsigned char)(value + i);
    }
}

// Too short to be told apart from other code by its bytes.
int lib_tiny(int x) {
    return x + 1;
}
