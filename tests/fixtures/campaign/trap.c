volatile unsigned cursor = 5;
void reset_cursor(void) { cursor = 5; }
unsigned checked_divide(unsigned divisor) {
#ifdef WRITE_FIRST
    cursor++;
#endif
    if (!divisor) __builtin_trap();
#ifndef WRITE_FIRST
    cursor++;
#endif
    return cursor / divisor;
}
