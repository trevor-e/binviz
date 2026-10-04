typedef struct RECT { int x; } RECT;
int selected_gpu(RECT *r) { return ++r->x; }
extern int runtime_vblank(int) __attribute__((import_module("env"),import_name("runtime_vblank")));
int selected_vblank(int n) { return runtime_vblank(n); }
