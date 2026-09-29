// A small program for WebAssembly, the way a game's browser port looks from
// the inside: an import from the host, entities in memory, behaviours called
// through a function table, a switch, and a check that traps.
// Built by scripts/build-fixtures.sh (clang --target=wasm32, then rust-lld).

extern void host_log(const char *message, int value) __attribute__((import_module("env"), import_name("log")));

struct Entity {
    int x, y;
    int hp;
    int kind;
    const char *name;
};

static struct Entity entities[3] = {
    {1, 2, 10, 0, "rat"},
    {5, 5, 30, 1, "guard"},
    {9, 1, 90, 2, "dragon"},
};

// Zero-filled: in memory, not in the file.
static int frames;
static int deaths;

const char *const messages[] = {"entity died", "entity moved", "frame done"};

static inline int clamp(int v, int lo, int hi) { return v < lo ? lo : v > hi ? hi : v; }

__attribute__((noinline)) void damage(struct Entity *e, int amount) {
    e->hp = clamp(e->hp - amount, 0, 100);
    if (e->hp == 0) {
        deaths++;
        host_log(messages[0], e->kind);
    }
}

typedef void (*Behavior)(struct Entity *);

__attribute__((noinline)) void idle(struct Entity *e) { e->hp = clamp(e->hp + 1, 0, 100); }

__attribute__((noinline)) void wander(struct Entity *e) {
    e->x = clamp(e->x + (frames & 1 ? 1 : -1), 0, 15);
    host_log(messages[1], e->x);
}

__attribute__((noinline)) void chase(struct Entity *e) {
    struct Entity *target = &entities[0];
    e->x += target->x > e->x ? 1 : -1;
    e->y += target->y > e->y ? 1 : -1;
    if (e->x == target->x && e->y == target->y)
        damage(target, 4);
}

static Behavior behaviors[] = {idle, wander, chase};

__attribute__((noinline)) int score_for(int kind) {
    switch (kind) {
    case 0: return 1;
    case 1: return entities[1].hp / 3;
    case 2: return entities[2].hp * 2 + deaths;
    case 3: return frames;
    case 4: return -deaths;
    case 6: return entities[0].x + entities[0].y;
    default: return 0;
    }
}

__attribute__((export_name("step"))) int step(int n) {
    int score = 0;
    for (int i = 0; i < n; i++) {
        for (int k = 0; k < 3; k++) {
            behaviors[entities[k].kind](&entities[k]);
            score += score_for(entities[k].kind);
        }
        frames++;
    }
    host_log(messages[2], frames);
    return score;
}

// The trap a stack trace is made of: `crash(200)` fails the check.
__attribute__((noinline)) static int check(int v) {
    if (v > 100)
        __builtin_trap();
    return v + 1;
}

static inline int twice_checked(int v) { return check(v * 2) + 1; }

__attribute__((noinline)) int middle(int v) {
    host_log(messages[2], v);
    return twice_checked(v) * 3;
}

__attribute__((export_name("crash"))) int crash(int v) { return middle(v) + 7; }
