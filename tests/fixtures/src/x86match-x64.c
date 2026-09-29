// The x86-64 ELF counterpart of x86match.cpp: a program (linked by ld.lld,
// no C library) whose own object is matched against it, and the object of an
// edited copy (EDITED defined), each edit a mistake the matcher must explain.
// Arguments come in registers here, globals and constants through [rip+N].

struct Player {
    int health;
    int ammo;
    short level;
    unsigned char flags;
    float speed;
};

struct Player g_player;
int g_frames;
const char *const g_names[] = {"zero", "one", "two", "three"};

__attribute__((noinline)) int clamp(int v, int lo, int hi) {
    if (v < lo) {
        return lo;
    }
    return v > hi ? hi : v;
}

// The edit swaps the operands: other registers.
__attribute__((noinline)) int diff(int a, int b) {
#ifdef EDITED
    return b - a;
#else
    return a - b;
#endif
}

// The edit takes another constant.
__attribute__((noinline)) int damage(struct Player *p, int amount) {
#ifdef EDITED
    p->health -= amount * 2 + 7;
#else
    p->health -= amount * 2 + 5;
#endif
    return clamp(p->health, 0, 100);
}

// A switch: a jump table in .rodata.
__attribute__((noinline)) int switch_op(int op, int x) {
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

__attribute__((noinline)) const char *name_of(int i) {
    return g_names[i & 3];
}

__attribute__((noinline)) float scaled_speed(const struct Player *p) {
    return p->speed * 1.5f;
}

// The edit reads another global.
__attribute__((noinline)) int ammo_left(void) {
#ifdef EDITED
    return g_frames * 2;
#else
    return g_player.ammo * 2;
#endif
}

void start(void) {
    int total = damage(&g_player, 4) + diff(g_frames, 2) + switch_op(g_frames & 7, 5);
    total += (int)scaled_speed(&g_player) + ammo_left() + *name_of(total);
    g_frames = total;
    for (;;) {
    }
}
