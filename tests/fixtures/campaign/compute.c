/* Freestanding host-native / WASM semantic runner fixture, not a PS1 emulator. */
#ifdef _WIN32
#define EXPORTED __declspec(dllexport)
#else
#define EXPORTED
#endif
static unsigned char ram[2];
static unsigned trace[2], trace_count;
static void device(unsigned word) { trace[trace_count++] = word; }
EXPORTED unsigned compute(unsigned word) {
    unsigned result = (unsigned)(int)(short)(word & 0xffff);
    trace_count = 0;
    ram[0] = (unsigned char)word;
    ram[1] = (unsigned char)(word >> 8);
    device(word);
#ifdef WRONG_WORD
    result ^= 0x80000000u;
#endif
#ifdef WRONG_RAM
    ram[1] ^= 1;
#endif
#ifdef EXTRA_DEVICE
    device(word + 1);
#endif
    return result;
}
EXPORTED unsigned get_ram(unsigned index) { return ram[index]; }
EXPORTED unsigned get_trace(unsigned index) { return trace[index]; }
EXPORTED unsigned get_trace_count(void) { return trace_count; }
