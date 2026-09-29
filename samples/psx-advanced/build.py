"""Build the advanced sample: a boot executable from two objects, an overlay
linked against it, a memory image with both loaded, an emulator-style trace,
and a list of name candidates.

    python build.py                    everything, into build/
    python build.py --rebuild my.rs    build/my.o from your source, to `match`

Needs only Rust's llvm-tools (rustup component add llvm-tools); see
../psx/README.md for the loop, and README.md here for what this one adds.
"""
import argparse
import json
import os
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
BASIC = os.path.join(os.path.dirname(HERE), "psx")
BUILD = os.path.join(HERE, "build")
sys.path.insert(0, BASIC)


def run(cmd, **kw):
    print("  " + " ".join(os.path.basename(c) if i == 0 else os.path.relpath(c, HERE) if os.path.isabs(c) else c for i, c in enumerate(cmd)))
    return subprocess.run(cmd, check=True, **kw)


def llvm_tools():
    sysroot = subprocess.run(["rustc", "--print", "sysroot"], capture_output=True, text=True, check=True).stdout.strip()
    host = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    host = next(l.split()[1] for l in host.splitlines() if l.startswith("host:"))
    path = os.path.join(sysroot, "lib", "rustlib", host, "bin")
    exe = ".exe" if os.name == "nt" else ""
    if not os.path.exists(os.path.join(path, "llc" + exe)):
        sys.exit(f"no llc in {path}: run `rustup component add llvm-tools`")
    return path, exe


def compile_rs(tools, exe, src, obj):
    ll = obj[:-2] + ".ll"
    run(["rustc", "--target", "wasm32-unknown-unknown", "--crate-type=lib", "-C", "panic=abort", "-C", "opt-level=2",
         "-C", "debuginfo=0", "--emit=llvm-ir", "-o", ll, src])
    text = open(ll).read()
    text = "\n".join(l for l in text.splitlines() if not l.startswith(("target datalayout", "target triple")))
    open(ll, "w").write(text)
    run([os.path.join(tools, "llc" + exe), "-mtriple=mipsel-unknown-elf", "-mcpu=mips1", "-float-abi=soft",
         "-relocation-model=static", "-filetype=obj", "-O2", ll, "-o", obj])


def symbols(tools, exe, elf):
    """(address, size, kind, name) of every defined symbol."""
    out = run([os.path.join(tools, "llvm-nm" + exe), "-S", "--defined-only", elf], capture_output=True, text=True).stdout
    found = []
    for line in out.splitlines():
        parts = line.split()
        if len(parts) == 4 and not parts[3].startswith(("$", "__", ".L")):
            found.append((int(parts[0], 16), int(parts[1], 16), parts[2].lower(), parts[3]))
    return found


def notes_of(syms):
    return [{"address": a, "size": s, "name": n, "comment": "", "reviewed": False} for a, s, k, n in syms if k in "tdrb"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--rebuild", metavar="SRC")
    args = ap.parse_args()
    os.makedirs(BUILD, exist_ok=True)
    tools, exe = llvm_tools()
    if args.rebuild:
        src = os.path.abspath(args.rebuild)
        obj = os.path.join(BUILD, os.path.splitext(os.path.basename(src))[0] + ".o")
        compile_rs(tools, exe, src, obj)
        print(f"\n{os.path.relpath(obj, HERE)}: now  binviz --notes build/main.notes.json match build/main.exe {os.path.relpath(obj, HERE)} <function>")
        return

    # The boot executable, from two objects.
    objs = []
    for name in ["engine", "game"]:
        obj = os.path.join(BUILD, name + ".o")
        compile_rs(tools, exe, os.path.join(HERE, name + ".rs"), obj)
        objs.append(obj)
    main_elf = os.path.join(BUILD, "main.elf")
    run([os.path.join(tools, "rust-lld" + exe), "-flavor", "gnu", "-T", os.path.join(BASIC, "psx.ld"), "-o", main_elf] + objs)
    main_exe = os.path.join(BUILD, "main.exe")
    run([sys.executable, os.path.join(BASIC, "wrap.py"), main_elf, main_exe, os.path.join(BUILD, "main.notes.json"), tools])
    main_syms = symbols(tools, exe, main_elf)

    # The overlay, linked against the executable's symbols, as a flat image.
    field_obj = os.path.join(BUILD, "field.o")
    compile_rs(tools, exe, os.path.join(HERE, "field.rs"), field_obj)
    field_elf = os.path.join(BUILD, "field.elf")
    run([os.path.join(tools, "rust-lld" + exe), "-flavor", "gnu", "-T", os.path.join(HERE, "overlay.ld"),
         "--just-symbols=" + main_elf, "-o", field_elf, field_obj])
    field_bin = os.path.join(BUILD, "field.bin")
    run([os.path.join(tools, "llvm-objcopy" + exe), "-O", "binary", field_elf, field_bin])
    field_syms = [s for s in symbols(tools, exe, field_elf) if s[0] >= 0x80100000]
    json.dump(notes_of(field_syms), open(os.path.join(BUILD, "field.notes.json"), "w"), indent=1)

    # A memory image: the kernel's entry points, the executable where it loads, the overlay where it loads.
    ram = bytearray(0x200000)
    for at, word in [(0x80, 0x3C1A0000), (0xA0, 0x3C080000), (0xB0, 0x3C080000), (0xC0, 0x3C080000)]:
        ram[at:at + 4] = struct.pack("<I", word)
        ram[at + 8:at + 12] = struct.pack("<I", 0x03400008)  # jr $k0
    exe_bytes = open(main_exe, "rb").read()
    load = struct.unpack_from("<I", exe_bytes, 0x18)[0] - 0x80000000
    body = exe_bytes[0x800:]
    ram[load:load + len(body)] = body
    field = open(field_bin, "rb").read()
    ram[0x100000:0x100000 + len(field)] = field
    open(os.path.join(BUILD, "ram.bin"), "wb").write(ram)

    # A trace: the handlers reached only through the table, as an emulator would have seen them run.
    handlers = [s for s in main_syms if s[3] in ("on_idle", "on_walk", "on_hurt", "on_dead")]
    with open(os.path.join(BUILD, "trace.txt"), "w") as f:
        for a, size, _, name in handlers:
            for pc in range(a, a + size, 4):
                f.write(f"{pc:08x}: (traced) {name}+{pc - a:#x}\n")

    # Name candidates: another build's functions with the strings they use and the functions they call.
    candidates = [
        {"name": "PoolAlloc", "strings": ["pool exhausted"], "calls": ["LogEvent"]},
        {"name": "LogEvent", "strings": [], "calls": []},
        {"name": "OnWalk", "strings": ["wrapped around"], "calls": ["LogEvent"]},
        {"name": "OnHurt", "strings": ["entity died"], "calls": ["LogEvent"]},
        {"name": "WorldCommand", "strings": ["command four", "bad command"], "calls": ["LogEvent", "PoolAlloc", "FxMul", "FxDiv", "StrLen", "EntityUpdate"]},
        {"name": "Spawn", "strings": ["no free entity slot", "spawned"], "calls": ["LogEvent", "FxMul"]},
        {"name": "GameMain", "strings": ["game start"], "calls": ["Spawn", "BuildTree", "TreeSum", "Frame", "LogEvent"]},
        {"name": "FieldEnter", "strings": ["entering the forest"], "calls": ["LogEvent", "Spawn"]},
        {"name": "FieldTick", "strings": ["forest ambush"], "calls": ["LogEvent", "WorldCommand", "FxDiv"]},
        {"name": "Nowhere", "strings": ["a string this build never had"], "calls": []},
    ]
    json.dump(candidates, open(os.path.join(BUILD, "candidates.json"), "w"), indent=1)
    print(f"\nbuilt: main.exe ({len(body)} bytes), field.bin ({len(field)} bytes at 0x80100000), ram.bin, trace.txt, candidates.json")
    print("see README.md for what to run")


if __name__ == "__main__":
    main()
