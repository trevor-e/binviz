typedef unsigned short word;
typedef int *pointer;
extern int count();
extern int no_result();
extern int narrow();
extern int missing(int);
extern void a(void), b(void), c(void);
static int local(int x) { return x; }
static int (*callback)(int) = local;
int caller(void) {
    word w = 7;
    int value = 0;
    pointer ptr = &value;
    int result = count(1) + no_result();
    result += ((int (*)(word))narrow)(w);
    (void)no_result();
    if (no_result()) result++;
    (no_result(), result++);
    result += missing(1);
    a(); b(); c();
    return result + local(*ptr);
}
