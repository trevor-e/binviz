// A game DLL shaped like Quake 2's gamex86.dll, for the MSVC ABI: built with
// clang for x87 floating point (as MSVC 6 compiled it) and linked by
// lld-link without a C runtime (see scripts/build-fixtures.sh), to check what
// binviz makes of such a binary stripped, against its PDB.
//
// The engine hands the DLL a table of its functions, which GetGameAPI copies
// into gi (gamedemo-msvc.s does it the way MSVC 6 did, with rep movsd); from
// then on the code calls through gi's slots. Cvars are pointers the engine
// fills in, read through (value at 0x14). level is a structure whose fields
// the code reads and writes at fixed addresses. Monsters point at tables of
// frames holding callbacks, stored into their edicts. Light styles are
// strings a letter long. gamedemo-msvc.s adds what MSVC's code does and
// clang's doesn't.

typedef struct cvar_s {
    char *name;
    char *string;
    char *latched_string;
    int flags;
    int modified;
    float value;
    struct cvar_s *next;
} cvar_t;

typedef struct edict_s edict_t;

typedef struct {
    void (*aifunc)(edict_t *self, float dist);
    float dist;
    void (*thinkfunc)(edict_t *self);
} mframe_t;

typedef struct {
    int firstframe;
    int lastframe;
    mframe_t *frame;
    void (*endfunc)(edict_t *self);
} mmove_t;

struct edict_s {
    float origin[3];
    int health;
    int max_health;
    edict_t *enemy;
    mmove_t *currentmove;
    int frame;
    float nextthink;
    void (*think)(edict_t *self);
    void (*pain)(edict_t *self, edict_t *other, float kick, int damage);
    void (*die)(edict_t *self, edict_t *attacker, int damage);
};

typedef struct {
    void (*bprintf)(int printlevel, char *fmt, ...);
    void (*dprintf)(char *fmt, ...);
    void (*cprintf)(edict_t *ent, int printlevel, char *fmt, ...);
    void (*centerprintf)(edict_t *ent, char *fmt, ...);
    void (*sound)(edict_t *ent, int channel, int soundindex, float volume, float attenuation, float timeofs);
    void (*configstring)(int num, char *string);
    void (*error)(char *fmt, ...);
    int (*modelindex)(char *name);
    int (*soundindex)(char *name);
    void (*setmodel)(edict_t *ent, char *name);
    int (*pointcontents)(float *point);
    void (*linkentity)(edict_t *ent);
    cvar_t *(*cvar)(char *var_name, char *value, int flags);
    int (*argc)(void);
} game_import_t;

typedef struct {
    int apiversion;
    void (*Init)(void);
    void (*RunFrame)(void);
} game_export_t;

typedef struct {
    int framenum;
    float time;
    char level_name[32];
    int total_monsters;
    int killed_monsters;
    edict_t *sight_client;
} level_locals_t;

#define CS_LIGHTS 32

game_import_t gi;
game_export_t globals;
level_locals_t level;
cvar_t *deathmatch;
cvar_t *skill;
int sound_death;
int _fltused = 1;

// In gamedemo-msvc.s.
void spawn_messages(edict_t *ent, int count);
char *find_char(char *s, int c);
double sqrt_either(double x);
double vec_length(float *v);
void gib_die(edict_t *self, edict_t *attacker, int damage);
void debris_die(edict_t *self, edict_t *attacker, int damage);

__declspec(noinline) static void ai_stand(edict_t *self, float dist) {
    if (self->enemy) {
        self->frame += 1;
    }
}

__declspec(noinline) static void ai_run(edict_t *self, float dist) {
    self->origin[0] += dist;
    gi.linkentity(self);
}

__declspec(noinline) static void soldier_attack(edict_t *self) {
    gi.sound(self, 1, sound_death, 1.0f, 1.0f, 0.0f);
}

static mframe_t soldier_frames_stand[] = {
    {ai_stand, 0, 0},
    {ai_stand, 0, 0},
    {ai_stand, 0, soldier_attack},
};
static mmove_t soldier_move_stand = {0, 2, soldier_frames_stand, 0};

static mframe_t soldier_frames_run[] = {
    {ai_run, 10, 0},
    {ai_run, 11, 0},
};
static mmove_t soldier_move_run = {3, 4, soldier_frames_run, 0};

__declspec(noinline) void soldier_stand(edict_t *self) {
    self->currentmove = &soldier_move_stand;
}

__declspec(noinline) void soldier_run(edict_t *self) {
    if (self->enemy) {
        self->currentmove = &soldier_move_run;
    } else {
        soldier_stand(self);
    }
}

// Reached only through the pointer stored in an edict: a bare return.
__declspec(noinline) void player_pain(edict_t *self, edict_t *other, float kick, int damage) {}

__declspec(noinline) static void soldier_die(edict_t *self, edict_t *attacker, int damage) {
    self->health = -1;
    level.killed_monsters++;
    gi.bprintf(1, "%s killed a soldier\n", attacker ? "someone" : "the world");
}

__declspec(noinline) static void soldier_think(edict_t *self) {
    self->nextthink = level.time + 0.1f;
    if (self->currentmove) {
        self->frame = self->currentmove->firstframe;
    }
}

// Callbacks stored into the edict: not calls.
__declspec(noinline) void SP_monster_soldier(edict_t *self) {
    if (deathmatch->value) {
        return;
    }
    self->health = 20 + (int)skill->value * 10;
    self->max_health = self->health;
    self->think = soldier_think;
    self->pain = player_pain;
    self->die = soldier_die;
    self->nextthink = level.time + 0.1f;
    sound_death = gi.soundindex("soldier/death1.wav");
    gi.setmodel(self, "models/monsters/soldier/tris.md2");
    level.total_monsters++;
    soldier_stand(self);
}

// Two callbacks with the same code, which the linker folded into one.
__declspec(noinline) void ThrowGib(edict_t *self) {
    self->die = gib_die;
    self->nextthink = level.time + 10.0f;
}

__declspec(noinline) void ThrowDebris(edict_t *self) {
    self->die = debris_die;
    gi.linkentity(self);
}

__declspec(noinline) void InitGame(void) {
    gi.dprintf("==== InitGame ====\n");
    deathmatch = gi.cvar("deathmatch", "0", 4);
    skill = gi.cvar("skill", "1", 0);
    // The light styles: 'a' is dark, 'm' is normal, 'z' is the brightest.
    gi.configstring(CS_LIGHTS + 0, "m");
    gi.configstring(CS_LIGHTS + 1, "mmnmmommommnonmmonqnmmo");
    gi.configstring(CS_LIGHTS + 63, "a");
}

__declspec(noinline) int weapon_damage(int weapon, int skill_level) {
    switch (weapon) {
    case 0:
        return 15;
    case 1:
        return 4 + skill_level;
    case 2:
        return 8 * skill_level;
    case 3:
        return 100 - skill_level;
    case 4:
        return 50 + skill_level * 3;
    case 5:
        return 120;
    default:
        return 0;
    }
}

__declspec(noinline) void G_RunFrame(void) {
    level.framenum++;
    level.time = level.framenum * 0.1f;
    if (deathmatch->value) {
        spawn_messages(level.sight_client, level.framenum);
    }
    if (skill->value > 2.0f && level.sight_client) {
        level.sight_client->health -= weapon_damage(level.framenum & 7, (int)skill->value);
        if (find_char(level.level_name, '*')) {
            level.total_monsters = (int)sqrt_either(level.time);
        }
        if (vec_length(level.sight_client->origin) > 64.0) {
            ThrowGib(level.sight_client);
            ThrowDebris(level.sight_client);
        }
    }
}
