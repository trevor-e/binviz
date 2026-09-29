// A 32-bit Windows program for the MSVC ABI, built with clang and linked by
// lld-link without a C runtime (see scripts/build-fixtures.sh), to check what
// binviz finds in a stripped PE32 against what its PDB names.
//
// Its functions are reached every way code is: called directly, through
// import thunks and the import table, through vtables (with an adjustor
// thunk), from a table of callbacks, by an address passed as an argument, by
// the export table and the TLS callback list, as the cases of a switch
// compiled to a jump table, and not at all. It has every calling convention
// (cdecl, stdcall, fastcall, thiscall) and floating point constants.
// x86demo-msvc.s adds what MSVC's code does and clang's doesn't.

extern "C" {
__declspec(dllimport) __declspec(noreturn) void __stdcall ExitProcess(unsigned code);
__declspec(dllimport) unsigned __stdcall GetTickCount(void);
// Not dllimport: called through the import thunk the linker writes.
void __stdcall Sleep(unsigned ms);

// An MSVC-ABI object that uses floating point refers to this.
int _fltused = 1;

// In x86demo-msvc.s.
int msvc_switch(int op);
int checked_index(int i);
int tail_caller(int x);
int with_handler(int x);
int with_finally(int x);
int cond_tail(int x);

__declspec(noreturn) __declspec(noinline) void fatal(int code) {
    ExitProcess(code);
}
}

// cdecl, stdcall, fastcall.
__declspec(noinline) int sum3(int a, int b, int c) {
    return a + b + c * 2;
}

__declspec(noinline) int __stdcall mix(int a, int b) {
    return (a << 3) ^ b;
}

__declspec(noinline) int __fastcall scale(int a, int b) {
    return a * b + (a >> 1);
}

__declspec(noinline) int fib(int n) {
    return n < 2 ? n : fib(n - 1) + fib(n - 2);
}

__declspec(noinline) int checked_div(int a, int b) {
    if (b == 0) {
        fatal(3);
    }
    return a / b;
}

__declspec(noinline) double ratio(int a, int b) {
    return (double)a / (double)b * 1.25 + 0.5;
}

// thiscall: virtual methods, reached only through their vtables.
struct Shape {
    int id;
    virtual int area() const = 0;
    virtual int sides() const { return 0; }
    int scaled(int k) const;
};

struct Square : Shape {
    int s;
    int area() const override { return s * s; }
    int sides() const override { return 4; }
};

struct Rect : Shape {
    int w, h;
    int area() const override { return w * h; }
    int sides() const override { return 4; }
};

struct Tri : Shape {
    int b, h;
    int area() const override { return b * h / 2; }
    int sides() const override { return 3; }
};

__declspec(noinline) int Shape::scaled(int k) const {
    return area() * k + id;
}

// A second base declaring area() too: Labelled::area overrides both, and
// Named's vtable in Labelled points at a thunk that adjusts `this` first.
struct Named {
    int letters;
    virtual int area() const = 0;
};

struct Labelled : Square, Named {
    int area() const override { return s * s + letters; }
};

// Callbacks: reached only through these tables, one read-only, one writable.
static int negate(int x) { return -x; }
static int square(int x) { return x * x; }
static int increment(int x) { return x + 1; }
int (*const unary_ops[])(int) = {negate, square, increment};
int (*hooks[])(int) = {increment, square};

// Callbacks passed by address (`push offset twice`).
__declspec(noinline) int apply(int (*f)(int), int x) {
    return f(x) + f(x + 1);
}

__declspec(noinline) static int twice(int x) {
    return x * 2;
}

__declspec(noinline) static int thrice(int x) {
    return x * 3 - 1;
}

// A switch compiled to a jump table: each case its own code.
__declspec(noinline) int dispatch(int op, int x) {
    switch (op) {
    case 0:
        return sum3(x, 1, 2);
    case 1:
        return mix(x, 7);
    case 2:
        return scale(x, 3);
    case 3:
        return x * 7 + 3;
    case 4:
        return fib(x & 7);
    case 5:
        return x ^ 0x55;
    case 6:
        return apply(twice, x);
    case 7:
        return (x >> 2) - 9;
    default:
        return -1;
    }
}

// Nothing refers to this: only looking at the code nothing reaches finds it.
__declspec(noinline) int unused_checksum(const unsigned char *p, int n) {
    int s = 0;
    for (int i = 0; i < n; i++) {
        s = s * 31 + p[i];
    }
    return s;
}

// Only the export table refers to this.
extern "C" __declspec(dllexport) int exported_add(int a, int b) {
    return a + b + 1;
}

// A TLS callback, found through the TLS directory lld-link points at _tls_used.
typedef void(__stdcall *TlsCallback)(void *, unsigned long, void *);
int thread_starts;

static void __stdcall on_thread(void *, unsigned long reason, void *) {
    if (reason == 2) {
        thread_starts++;
    }
}

#pragma section(".CRT$XLB", read)
extern "C" __declspec(allocate(".CRT$XLB")) const TlsCallback tls_callbacks[] = {on_thread, nullptr};
unsigned tls_index;

struct TlsDirectory {
    void *raw_start;
    void *raw_end;
    unsigned *index;
    const TlsCallback *callbacks;
    unsigned zero_fill;
    unsigned characteristics;
};
extern "C" const TlsDirectory _tls_used = {nullptr, nullptr, &tls_index, tls_callbacks, 0, 0};

// The load configuration, as the C runtime writes it: here for its table of
// safe exception handlers, which lld-link fills in.
extern "C" {
extern void *__safe_se_handler_table[];
extern char __safe_se_handler_count;

struct LoadConfig {
    unsigned size;
    unsigned timestamp;
    unsigned short major, minor;
    unsigned global_flags_clear, global_flags_set;
    unsigned critical_section_timeout;
    unsigned decommit_free_block, decommit_total_free;
    void *lock_prefix_table;
    unsigned max_allocation, virtual_memory_threshold;
    unsigned process_heap_flags, process_affinity_mask;
    unsigned short csd_version, dependent_load_flags;
    void *edit_list;
    unsigned *security_cookie;
    void **se_handler_table;
    char *se_handler_count;
};
unsigned security_cookie = 0xBB40E64E;
}
extern "C" const LoadConfig _load_config_used = {
    sizeof(LoadConfig), 0, 0, 0, 0, 0, 0, 0, 0, nullptr, 0, 0, 0, 0, 0, 0, nullptr,
    &security_cookie, __safe_se_handler_table, &__safe_se_handler_count,
};

extern "C" void start() {
    unsigned t = GetTickCount();
    int x = (int)(t & 15);

    Square sq;
    sq.id = 1;
    sq.s = x;
    Rect rc;
    rc.id = 2;
    rc.w = 3;
    rc.h = x + 1;
    Tri tr;
    tr.id = 3;
    tr.b = 4;
    tr.h = x + 2;
    Labelled lb;
    lb.id = 4;
    lb.s = 2;
    lb.letters = 6;
    Shape *shapes[] = {&sq, &rc, &tr, &lb};
    Shape *s = shapes[t % 4];
    const Named *named = &lb;

    int r = dispatch(x & 7, x);
    r += s->scaled(2) + s->sides();
    r += named->area();
    r += unary_ops[t % 3](x) + hooks[t % 2](x);
    r += apply(thrice, x);
    r += checked_div(r, x + 1);
    r += (int)ratio(r, x + 3);
    r += msvc_switch(x) + checked_index(x) + tail_caller(x) + with_handler(x);
    r += with_finally(x) + cond_tail(x);
    Sleep((unsigned)r & 3);
    ExitProcess((unsigned)r);
}
