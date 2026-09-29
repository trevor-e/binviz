# The decompilation loop on a real game

Yamagi Quake II's game code (GPL; downloaded, never committed) as the target of a matching
decompilation: its numeric constants changed at random first (blind, so recalling id's code
doesn't match), then built by clang as an x86-64 `game.so`. Decompile a function from the
binary, compile your C with the same flags, and let binviz say whether it is the same code.

Needs clang, python3, and binviz built (`cargo build --release -p binviz-cli -p binviz-mcp`).

```bash
python3 samples/quake2-loop/build.py            # build/game.so, build/objs/ (the answer key)
```

Pick functions, and don't read `build/quake2-5.34/src/game*/*.c`: the headers (structures,
constants) are fair game, as they are for a decompilation with its headers.

```bash
binviz context samples/quake2-loop/build/game.so door_go_up    # what to write the C from
binviz header build/types.o                                    # the structures' layout (see below)
```

Write the C in `build/quake2-5.34/src/gamemut/decomp/<function>.c` (it includes
`"../header/local.h"`, and a monster's own header from `"../monster/<name>/<name>.h"`), then:

```bash
samples/quake2-loop/try.sh door_go_up         # compile with the game's flags; binviz match
python3 samples/quake2-loop/verify.py door_go_up:AngleMove_Calc:door_use_areaportals
                                              # apart from binviz: bytes and every reference
```

`binviz match build/game.so build/objs` is the control: the compiler's own objects, all 100%.

Two things a single function's file needs, which the real file has: a `static` the function
reads must be written somewhere in the file (else clang folds it to 0), and a function the
original inlines (`Move_Final` into `Move_Begin`) must be in the file too. For the structures'
layout, compile the headers with debug info and let binviz write them out:

```bash
cd samples/quake2-loop/build && printf '#include "../header/local.h"\n' > quake2-5.34/src/gamemut/decomp/types.c
clang -g -fno-eliminate-unused-debug-types -w -DYQ2OSTYPE='"Linux"' -DYQ2ARCH='"x86_64"' \
  -DOSTYPE='"Linux"' -DARCH='"x86_64"' -c quake2-5.34/src/gamemut/decomp/types.c -o types.o
binviz header types.o > layout.h
```

The first run (docs/decomp-plan.md, progress log): ten functions picked at random, 44 to 526
bytes, all matched, and checked by `verify.py`.
