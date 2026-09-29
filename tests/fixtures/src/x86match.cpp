// A 32-bit Windows program for the MSVC ABI, built with clang and linked by
// lld-link without a C runtime (see scripts/build-fixtures.sh), whose own
// object file is matched against it: the loop of a matching decompilation.
// Built once as the program (with a PDB), once more as the object a
// decompilation would compile, and once with EDITED defined: the object of a
// decompilation that is not there yet, each edit a mistake the matcher must
// explain (a constant, operands swapped, a local of another size, a variable
// that became a constant, the wrong function called, a signed type for an
// unsigned one, < for <=).
//
// It calls functions of x86lib.lib (x86lib-*.c), the stand-in for a C
// runtime linked in statically, which binviz names by their signatures.

extern "C" {
__declspec(dllimport) __declspec(noreturn) void __stdcall ExitProcess(unsigned code);
__declspec(dllimport) unsigned __stdcall GetTickCount(void);

// An MSVC-ABI object that uses floating point refers to this.
int _fltused = 1;

// In x86lib.lib.
int lib_checksum(const unsigned char *p, int n);
void lib_fill(unsigned char *p, int n, int value);
const char *lib_find(const char *s, int c);
int lib_tiny(int x);
int lib_mix(int a, int b, int c);

struct Player {
    int health;
    int ammo;
    short level;
#ifdef EDITED
    signed char flags;
#else
    unsigned char flags;
#endif
    float speed;
};

Player g_player;
int g_frames;
int g_scores[8];
const char *const g_names[] = {"zero", "one", "two", "three"};

// cdecl: _clamp.
__declspec(noinline) int clamp(int v, int lo, int hi) {
    if (v < lo) {
        return lo;
    }
    return v > hi ? hi : v;
}

// cdecl: _diff; the edit swaps its operands.
__declspec(noinline) int diff(int a, int b) {
#ifdef EDITED
    return b - a;
#else
    return a - b;
#endif
}

// stdcall: _damage@8; the edit takes another constant.
__declspec(noinline) int __stdcall damage(Player *p, int amount) {
#ifdef EDITED
    p->health -= amount * 2 + 7;
#else
    p->health -= amount * 2 + 5;
#endif
    return clamp(p->health, 0, 100);
}

// fastcall: @add_ammo@8; the edit calls another function.
__declspec(noinline) int __fastcall add_ammo(Player *p, int n) {
#ifdef EDITED
    p->ammo = diff(p->ammo + n, 3);
#else
    p->ammo = clamp(p->ammo + n, 0, 250);
#endif
    return p->ammo;
}

// A switch: a jump table.
__declspec(noinline) int switch_op(int op, int x) {
    switch (op) {
    case 0:
        return x + 1;
    case 1:
        return x * 3;
    case 2:
        return x - 9;
    case 3:
        return x << 4;
    case 4:
        return g_frames + x;
    case 5:
        return x ^ 0x55;
    default:
        return 0;
    }
}

const char *name_of(int i) {
    return g_names[i & 3];
}

// The edit's local buffer is larger.
__declspec(noinline) int sum_local(int seed) {
#ifdef EDITED
    unsigned char buf[48];
#else
    unsigned char buf[32];
#endif
    lib_fill(buf, 16, seed);
    return lib_checksum(buf, 16);
}

// The edit uses a constant where the original has a variable.
__declspec(noinline) int scale_by(int x, int n) {
#ifdef EDITED
    return x * 3 + 4;
#else
    return x * 3 + n;
#endif
}

// The edit's flags are signed.
__declspec(noinline) int flag_bits(const Player *p) {
    return p->flags * 3;
}

// The edit compares with <= where the original has <.
__declspec(noinline) int below(int a, int b) {
#ifdef EDITED
    return a <= b ? 10 : 20;
#else
    return a < b ? 10 : 20;
#endif
}

__declspec(noinline) float scaled_speed(const Player *p) {
    return p->speed * 1.5f;
}

__declspec(noinline) unsigned ticks_since(unsigned t) {
    return GetTickCount() - t;
}
}

// C++: mangled names.
namespace game {
__declspec(noinline) int level_up(Player &p) {
    p.level++;
    return p.level * 10 + g_scores[p.level & 7];
}
} // namespace game

struct Counter {
    int n;
    int bump(int k);
};

__declspec(noinline) int Counter::bump(int k) {
    n += k;
    return n;
}

extern "C" void start(void) {
    unsigned char buf[32];
    Player *p = &g_player;
    lib_fill(buf, sizeof buf, 7);
    int total = lib_checksum(buf, sizeof buf) + lib_tiny(3) + lib_mix(1, 2, 3);
    total += damage(p, 4) + add_ammo(p, 9) + switch_op(total & 7, 5) + diff(total, 2);
    total += sum_local(total) + scale_by(total, 3) + flag_bits(p) + below(total, 9);
    total += (int)scaled_speed(p) + (int)ticks_since(12) + game::level_up(*p);
    total += *lib_find(name_of(total), 'e');
    Counter c = {total};
    ExitProcess(c.bump(g_frames));
}
