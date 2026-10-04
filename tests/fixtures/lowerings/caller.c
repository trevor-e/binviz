typedef struct RECT { int x; } RECT;
extern void legacy_draw(RECT *);
extern int legacy_vblank(int);
void frame(RECT *r) { legacy_draw(r); }
int poll(int n) { return legacy_vblank(n); }
