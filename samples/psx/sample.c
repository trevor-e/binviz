/* A small PlayStation-shaped program to try the decompilation loop on:
 * a structure walked through a pointer, a switch, string literals, a global,
 * a call chain, a leaf, and a function with more arguments than the four
 * registers carry. Build it with build.py, then see README.md. */

typedef struct {
    int x;
    int y;
    short hp;
    unsigned char kind;
    unsigned char flags;
    const char *name;
} Entity;

static unsigned int frame;
static const char *const names[3] = {"slime", "bat", "goblin"};

int entity_speed(unsigned char kind)
{
    switch (kind) {
    case 0: return 1;
    case 1: return 6;
    case 2: return 3;
    case 3: return 9;
    case 4: return 2;
    default: return 0;
    }
}

int entity_step(Entity *e, int dx, int dy, int ticks, int wrap)
{
    int speed = entity_speed(e->kind);
    e->x += dx * speed * ticks;
    e->y += dy * speed * ticks;
    if (e->x > wrap) {
        e->x -= wrap;
    }
    e->hp -= 1;
    frame += 1;
    return e->hp;
}

const char *entity_name(unsigned char kind)
{
    if (kind < 3) {
        return names[kind];
    }
    return "unknown";
}

volatile int sink;

int main(void)
{
    Entity e;
    int total = 0;
    int i;
    e.x = 0;
    e.y = 0;
    e.hp = 10;
    e.kind = 2;
    e.flags = 0;
    e.name = entity_name(2);
    for (i = 0; i < sink; i++) {
        total += entity_step(&e, 1, -1, i, 100);
    }
    return total;
}
