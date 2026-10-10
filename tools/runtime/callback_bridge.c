/* Universal callback transport: one meaningful i32 word, void typed target.
 * Compile once; validate its actual imports/export with Binviz linked before use.
 * The target owns native identity, execution phase and semantic validation. */
extern void ff9_callback_void1(int value);
int ff9_callback_u20(int a0, int a1, int a2, int a3, int a4,
                     int a5, int a6, int a7, int a8, int a9,
                     int a10, int a11, int a12, int a13, int a14,
                     int a15, int a16, int a17, int a18, int a19)
{
    ff9_callback_void1(a0);
    return 0;
}
