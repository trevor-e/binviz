/* A shared library's code as a decompilation compiles it (position-independent, as a Linux game's
 * game.so is): calls to its own exported functions go through the PLT, its globals are read
 * through the GOT, and statics, float constants and strings are reached through the object's own
 * sections. With -DEDITED, the C is wrong where only the data it refers to shows it: the wrong
 * static, another constant, another string. */

extern void emit(const char *name, float volume);

int frames;
static int sound_open;
static int sound_close;

#ifdef EDITED
#define SOUND sound_close
#define VOLUME 0.75f
#define NAME "door/open2.wav"
#else
#define SOUND sound_open
#define VOLUME 0.5f
#define NAME "door/open1.wav"
#endif

void set_sounds(int open, int close)
{
    sound_open = open;
    sound_close = close;
}

int both_sounds(void)
{
    return sound_open + sound_close;
}

int count(int n)
{
    frames += n;
    return frames;
}

int step(int n)
{
    return count(n) + 1;
}

int which_sound(void)
{
    return SOUND;
}

float scaled(float x)
{
    return x * VOLUME;
}

void open_door(void)
{
    emit(NAME, 1.0f);
}
