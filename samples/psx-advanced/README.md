# The advanced sample: two objects, an overlay, a memory image, a trace

Everything [the basic sample](../psx/README.md) does, plus the parts of a real
game that one flat program cannot show:

- **Two object files** ([engine.rs](engine.rs), [game.rs](game.rs)) linked into
  one boot executable, so `match` runs against each object.
- **A dispatcher with a real jump table** (`world_command`), which the
  follower has to read to reach the case bodies.
- **Handlers reached only through a table of function pointers**
  (`on_idle`, `on_walk`, `on_hurt`, `on_dead`): invisible to following the
  code, found by a trace of what an emulator ran.
- **An overlay** ([field.rs](field.rs)) linked at `0x80100000` against the
  executable's symbols, as a game's field code is loaded from the disc.
- **A memory image**, 2 MiB of RAM with the kernel's entry points, the
  executable and the overlay in place, as an emulator would dump it.
- **Name candidates** from "another build", to try `names`.

Also fixed-point maths, a byte copy, a pool allocator, recursion, a global
structure array and strings used from many places.

## Build

```bash
python samples/psx-advanced/build.py
```

Needs only Rust's `llvm-tools`, like the basic sample. Outputs in `build/`:
`main.exe` and `main.notes.json`, `engine.o` and `game.o`, `field.bin` and
`field.notes.json`, `ram.bin`, `trace.txt`, `candidates.json`.

The commands below run from the repository root with `binviz` standing for
`target/release/binviz` (`cargo build --release -p binviz-cli`).

## What to try, and what to expect

**Blind.** Without notes, following the code from the entry point finds 12
functions; `world_command` is whole (416 bytes) because its jump table was
read; the four handlers are missing because nothing calls them.

```bash
binviz info samples/psx-advanced/build/main.exe
```

**The trace.** With the emulator's trace, the four handlers become functions
and their strings are attributed.

```bash
binviz --trace samples/psx-advanced/build/trace.txt info samples/psx-advanced/build/main.exe
```

**Names from another build.** Five of the ten candidates place from strings
alone, one more through the call graph (`LogEvent`, at 40%: the only
unmatched function `PoolAlloc` calls). With the trace, the two handlers
place too. `FieldEnter` and `FieldTick` are in the overlay, not here, and
`Nowhere` is a control.

```bash
binviz --trace samples/psx-advanced/build/trace.txt names samples/psx-advanced/build/main.exe samples/psx-advanced/build/candidates.json
```

**Matching, two objects.** Every function of both objects should match at 100%.

```bash
binviz --notes samples/psx-advanced/build/main.notes.json match samples/psx-advanced/build/main.exe samples/psx-advanced/build/engine.o
```

```bash
binviz --notes samples/psx-advanced/build/main.notes.json match samples/psx-advanced/build/main.exe samples/psx-advanced/build/game.o
```

**The overlay.** Opened at its load address with the executable's names,
its calls into the executable read as `spawn` and `log_event`; its own
global sits in RAM above the file.

```bash
binviz --psx-exe samples/psx-advanced/build/main.exe --notes samples/psx-advanced/build/main.notes.json --overlay-at 0x80100000 context samples/psx-advanced/build/field.bin 0x80100000
```

**The memory image.** Opened plain, the kernel's entry points and the
prologue scan find the code; with the executable, its functions are named
and the overlay's code shows at `0x80100000`.

```bash
binviz --psx-exe samples/psx-advanced/build/main.exe --notes samples/psx-advanced/build/main.notes.json func samples/psx-advanced/build/ram.bin 0x80100000
```

**splat.** A config with two units, split where `game.rs`'s code starts.

```bash
binviz --notes samples/psx-advanced/build/main.notes.json splat samples/psx-advanced/build/main.exe advanced out 0x80010650
```

## Rebuilding a function

```bash
python samples/psx-advanced/build.py --rebuild my.rs
```

```bash
binviz --notes samples/psx-advanced/build/main.notes.json match samples/psx-advanced/build/main.exe samples/psx-advanced/build/my.o <function>
```

## What this sample found in binviz

Building it exposed four bugs, all fixed: `movn` and `movz` (MIPS32
instructions LLVM emits even for MIPS I) stopped the follower in the middle
of the dispatcher; a string loaded in a call's delay slot was lost because
the decoder cleared the caller-saved registers at the call instead of after
its slot; notes for data symbols were followed as code into two megabytes
of zeroed RAM; and a trace covering four adjacent functions seeded only the
first. Expect a real game to find more.
