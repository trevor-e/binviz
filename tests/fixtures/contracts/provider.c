int count(int a, int b) { return a + b; }
void no_result(void) {}
short narrow(short x) { return x; }
void a(void) {}
void b(void) {}
void c(void) {}
int old_style(x) short x; { return x; }
typedef int *pointer;
pointer *pointers(pointer *x) { return x; }
struct aggregate { int x; };
struct aggregate aggregate(struct aggregate x) { return x; }
int variadic(int x, ...) { return x; }
