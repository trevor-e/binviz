typedef struct RECT { int x; } RECT; extern int selected_gpu(RECT *), selected_vblank(int); void frame(RECT *r) { (void)selected_gpu(r); } int poll(int n) { return selected_vblank(n); }
