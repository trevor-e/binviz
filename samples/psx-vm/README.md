# The VM sample: the whole loop, driven through the MCP server

A bytecode interpreter with a script compiler, the size and shape of a
game's field system: 48 functions in four objects (a runtime, the VM, the
expression compiler, the game), two overlays that share one load address,
a memory image with one of them loaded, a trace of the syscall handlers
reached only through a table, and name candidates read straight off the
sources. Then [loop.py](../loop.py) plays the agent: it runs the loop the
landing page describes, Map, Pick, Context, Compile, Compare, over the MCP
server, with the compiler's own objects standing in for the C it would
write, and a wrong object every fourth pick to exercise `attempted`.

What the code has that the earlier samples lack: a 24-way opcode switch
(one big jump table), a table of syscall handlers, a hash-based symbol
table, mutually recursive parser functions, functions returning structures
by value (the hidden pointer in `$a0`), a 64-bit accumulator in register
pairs, signed and unsigned narrow loads, a large stack frame with local
tables, a family of four near-identical functions for `similar_functions`,
and a runtime object marked as library code.

## Build and run

```bash
python samples/psx-vm/build.py
```

```bash
cargo build --release -p binviz-cli -p binviz-mcp
```

```bash
python samples/loop.py samples/psx-vm/build --rounds 60
```

The loop opens the executable blind (no notes), names what the candidates
place, marks the runtime as library, then picks, gathers context, lists
look-alikes, compares and marks, round after round, until `next_functions`
has nothing left. Expect: every name proposed is right; the first picks are
the leaves many callers wait on (`vm_fail`); a matched `adjust_gold` makes
its three twins "like a done one" with it as the template; a wrong object
scores low and the function comes back as `tried 1×`; the run ends with
every function matched or library.

## The same by hand

```bash
binviz info samples/psx-vm/build/main.exe
```

```bash
binviz --trace samples/psx-vm/build/trace.txt info samples/psx-vm/build/main.exe
```

```bash
binviz names samples/psx-vm/build/main.exe samples/psx-vm/build/candidates.json
```

```bash
binviz --notes samples/psx-vm/build/main.notes.json match samples/psx-vm/build/main.exe samples/psx-vm/build/vm.o
```

```bash
binviz --notes samples/psx-vm/build/main.notes.json signature samples/psx-vm/build/main.exe make_token
```

```bash
binviz locate samples/psx-vm/build/ram.bin samples/psx-vm/build/dungeon.bin
```

```bash
binviz locate samples/psx-vm/build/ram.bin samples/psx-vm/build/town.bin
```

The last two say which of the two overlays sharing `0x80100000` the image
holds. `signature` on `make_token` or `range_of` says the first argument is
the hidden pointer of a structure returned by value.
