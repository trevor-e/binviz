/* A typed build candidate. The original caller facts remain independent. */
extern int provider(int);
int caller_a(void) { return provider(1); }
int caller_b(void) { return provider(2); }
unsigned char tentative[14];
