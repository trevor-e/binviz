// C types whose layout a header must get right, built with clang for 32-bit
// x86 twice (see scripts/build-fixtures.sh): an ELF with DWARF, laid out by
// the System V rules, and a PE with a PDB, by Microsoft's. The two differ
// (bit fields of mixed types, a double's alignment), which is the point: a
// header made from either must compile back to the same layout.
//
// No C library: everything here is the program's own.

// An MSVC-ABI object that uses floating point refers to this.
int _fltused = 1;

typedef unsigned char byte;
typedef float vec3_t[3];
typedef vec3_t vec3_alias;
typedef int qboolean;

enum color { RED, GREEN = 5, BLUE = -1 };

// An anonymous enum named by its typedef.
typedef enum { MODE_OFF, MODE_ON, MODE_AUTO = 0x7fffffff } mode_t;

// One byte, not an int.
enum __attribute__((packed)) small { SMALL_A = 1, SMALL_B = 200 };

// Bit fields of several types: Microsoft's rules start a new storage unit
// when the type's size changes, System V's pack them where they fit.
struct flags {
    unsigned int a : 1;
    unsigned int b : 3;
    int c : 5;
    unsigned short d : 4;
    unsigned char e : 2;
    enum color col : 4;
    qboolean on : 1;
    long long big : 40;
    unsigned int after;
};

union value {
    int i;
    float f;
    byte bytes[4];
    struct {
        short lo, hi;
    } halves;
};

// Larger than its largest member, for the int's alignment.
union odd {
    char c[5];
    int i;
};

struct opaque;
struct entity;

typedef void (*think_t)(struct entity *self, float time);
typedef int (*compare_t)(const void *, const void *);

struct entity {
    vec3_t origin;
    vec3_alias angles[2];
    float matrix[3][4];
    struct entity *enemy;
    struct entity *chain[4];
    think_t think;
    void (*touch)(struct entity *self, struct entity *other, int (*filter)(int));
    int (*(*pick)(int))(int);
    int (*rows)[4];
    int (*printf_like)(const char *, ...);
    union value v;
    union {
        int count;
        float delay;
    };
    struct {
        char tag;
        double weight;
    };
    struct {
        int x, y;
    } pos;
    struct flags fl;
    mode_t mode;
    enum small size;
    char name[16];
    const char *label;
    volatile int *reg;
    int *const fixed;
    compare_t cmp;
    struct opaque *handle;
    union odd odd;
    long long ticks;
    char tail;
};

// Packed: no alignment at all, and a structure inside that is packed too.
#pragma pack(push, 1)
struct packed {
    char c;
    int i;
    short s;
    struct {
        char x;
        int y;
    } inner;
    unsigned int bits : 12;
    unsigned int more : 12;
};
#pragma pack(pop)

// Packed to 2: an int and a double at offsets natural alignment wouldn't give.
#pragma pack(push, 2)
struct packed2 {
    char c;
    int i;
    double d;
};
#pragma pack(pop)

struct node {
    struct node *next;
    struct node **prev_next;
    union value data;
};

// An anonymous structure named by its typedef, and a tag with a typedef.
typedef struct {
    int w, h;
} size2_t;

typedef struct list {
    struct node *head;
    int length;
} list_t;

struct entity world[2];
struct packed packed_one;
struct packed2 packed_two;
list_t lists[3];
size2_t screen = {640, 480};
struct flags settings;

static int negate(int x) {
    return -x;
}

static int (*picker(int which))(int) {
    return which ? negate : 0;
}

__attribute__((noinline)) int entity_count(struct entity *e, int n) {
    int total = 0;
    for (int i = 0; i < n; i++) {
        total += e[i].count + e[i].fl.c + e[i].pos.x + (e[i].size == SMALL_B);
    }
    return total;
}

__attribute__((noinline)) void entity_think(struct entity *e, float time) {
    if (e->think) {
        e->think(e, time);
    }
    e->pick = picker;
}

__attribute__((noinline)) float vec_length2(const vec3_t v) {
    return v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
}

__attribute__((noinline)) int sum(int n, ...) {
    return n;
}

__attribute__((noinline)) int area(size2_t s) {
    return s.w * s.h;
}

__attribute__((noinline)) struct packed make_packed(char c, int i) {
    struct packed p = {0};
    p.c = c;
    p.i = i;
    p.bits = 7;
    return p;
}

__attribute__((noinline, stdcall)) int list_length(list_t *l) {
    int n = 0;
    for (struct node *p = l->head; p; p = p->next) {
        n++;
    }
    return n + l->length;
}

__attribute__((noinline, fastcall)) unsigned flag_bits(struct flags *f, union odd *o) {
    return f->a + f->b + f->d + f->e + (unsigned)f->big + (unsigned)o->i + f->col + f->on;
}

__attribute__((noinline)) mode_t next_mode(mode_t m, enum color c) {
    return m == MODE_AUTO ? MODE_OFF : (mode_t)(m + (c == RED));
}

__attribute__((noinline)) double packed_sum(struct packed2 *p, struct opaque *h) {
    return p->c + p->i + p->d + (h != 0);
}

int start(void) {
    int n = entity_count(world, 2) + sum(1, 2) + area(screen);
    entity_think(&world[0], 1.0f);
    packed_one = make_packed('x', n);
    n += list_length(&lists[0]) + (int)flag_bits(&settings, &world[1].odd);
    n += (int)vec_length2(world[0].origin) + (int)packed_sum(&packed_two, world[0].handle);
    n += next_mode(world[1].mode, BLUE);
    return n;
}

// The ELF's entry point.
void _start(void) {
    start();
    for (;;) {
    }
}
