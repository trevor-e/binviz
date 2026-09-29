# The decompilation loop on a sample PlayStation program

A small program shaped like game code (a structure walked through a pointer,
a `switch`, string literals, a global, a call chain, a leaf, a function with
five arguments), built as a PS-X EXE so that every step of a matching
decompilation can be tried on something with a known answer before a real
game is involved. Nothing here needs the game, the Psy-Q SDK, or any
download: the MIPS code generator and linker ship with Rust's `llvm-tools`.

## Build

```bash
rustup component add llvm-tools
```

```bash
python samples/psx/build.py
```

With a `clang` on `PATH` (or `--cc <path>`) the C version, [sample.c](sample.c),
is compiled for the MIPS I. Without one, the identical Rust version,
[sample.rs](sample.rs), goes through `rustc` and `llc` instead. Either way the
output is `build/sample.o` (the compiler's object, with relocations),
`build/sample.elf`, `build/sample.exe` (a PS-X EXE loaded at `0x80010000`) and
`build/sample.notes.json`, which names the functions, since a real game's
executable would have no names either and you would be adding them.

Build binviz's CLI once:

```bash
cargo build --release -p binviz-cli
```

The commands below use `binviz` for `target/release/binviz`.

## 1. Look at it as if you didn't know it

```bash
binviz --notes samples/psx/build/sample.notes.json info samples/psx/build/sample.exe
```

```bash
binviz --notes samples/psx/build/sample.notes.json signature samples/psx/build/sample.exe entity_step
```

The signature should say five `int` arguments (the fifth from the stack), a
result, no frame, no calls (the compiler inlined `entity_speed`), and the
`Entity` fields walked off the first argument: `0x0`, `0x4` (32-bit),
`0x8` (16-bit), `0xa` (8-bit).

```bash
binviz --notes samples/psx/build/sample.notes.json context samples/psx/build/sample.exe entity_step
```

That is what an agent gets to write the C from: the code with names
resolved, the signature, callers and callees with theirs, strings, globals
and notes.

## 2. Score the compiler's own output: everything should match

```bash
binviz --notes samples/psx/build/sample.notes.json match samples/psx/build/sample.exe samples/psx/build/sample.o
```

Every function is compared with the object's function of the same name
(the names come from the notes) with the linker's fields masked. All should
be 100%. If one isn't, that is a bug in binviz's scoring, not in the code.

## 3. Rebuild a function yourself and match it

Write your own version of a function, as you would in a decompilation, in a
file of its own, say `rebuild/step.c` (or `.rs` when there is no C
compiler; copy the function and its types from the sample and change it):

```bash
python samples/psx/build.py --rebuild rebuild/step.c
```

```bash
binviz --notes samples/psx/build/sample.notes.json match samples/psx/build/sample.exe samples/psx/build/step.o entity_step
```

The output lines your instructions up with the original's and explains
each difference: registers allocated differently, a stack frame or slot of
another size, a branch of another length, reordered instructions, a `nop`
missing from a delay slot. Change the source until it reads 100%.

[rebuild/step.rs](rebuild/step.rs) is one to start from: `entity_step` with
the multiplications written the other way round and `wrap` made a global.
Build it with `--rebuild rebuild/step.rs`, match it, and read what the diff
says before fixing it.

## 4. The other tools

- `binviz splat samples/psx/build/sample.exe sample out` writes a splat
  config and `symbol_addrs.txt` for a project built on this executable.
- `binviz --notes … report samples/psx/build/sample.exe report.json` places an
  objdiff report on it, once a project produces one.
- `binviz sdk <exe> <folder of Psy-Q .LIB files>` names the SDK's functions in
  a real game; the sample uses no SDK, so there is nothing for it here.
- Dump 2 MiB of RAM from an emulator running any game and open it with
  `--psx-exe <its boot exe>` to see the same views on a live image.

## Caveats

- The compiler here is LLVM, not the Psy-Q GCC a real game was built with.
  The loop is the same; the idioms differ (LLVM turned the `switch` into a
  lookup table where GCC 2.x would emit a jump table).
- LLVM emits a `mul` instruction the R3000 doesn't have, even for MIPS I.
  binviz decodes it; a real console would not run this executable as is.
