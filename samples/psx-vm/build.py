"""Build the VM sample: a boot executable from four objects (a runtime, the
VM, the expression compiler, the game), two overlays sharing one address, a
memory image with one of them loaded, a trace of the syscall handlers, name
candidates read off the sources, and the ground-truth notes.

    python build.py
    python build.py --rebuild my.rs
"""
import argparse
import json
import os
import re
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
BASIC = os.path.join(os.path.dirname(HERE), "psx")
ADVANCED = os.path.join(os.path.dirname(HERE), "psx-advanced")
BUILD = os.path.join(HERE, "build")
OBJECTS = ["rt", "vm", "parser", "game"]
OVERLAYS = ["town", "dungeon"]
OVERLAY_AT = 0x80100000
LOADED_OVERLAY = "dungeon"


def run(cmd, **kw):
    print("  " + " ".join(os.path.basename(c) if i == 0 else (os.path.relpath(c, HERE) if os.path.isabs(c) else c) for i, c in enumerate(cmd)))
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
    out = run([os.path.join(tools, "llvm-nm" + exe), "-S", "--defined-only", elf], capture_output=True, text=True).stdout
    found = []
    for line in out.splitlines():
        parts = line.split()
        if len(parts) == 4 and not parts[3].startswith(("$", "__", ".L")):
            found.append((int(parts[0], 16), int(parts[1], 16), parts[2].lower(), parts[3]))
    return found


def notes_of(syms):
    return [{"address": a, "size": s, "name": n, "comment": "", "reviewed": False, "kind": "function" if k == "t" else "data"}
            for a, s, k, n in syms if k in "tdrb"]


def camel(name):
    return "".join(p.capitalize() for p in name.split("_"))


def candidates_from_sources(files):
    """Each `extern "C" fn` with the string literals in its body and the
    functions it calls, named as another build would name them (CamelCase)."""
    fn_re = re.compile(r'pub (?:unsafe )?extern "C" fn (\w+)\s*\(')
    known = set()
    bodies = []
    for f in files:
        text = open(f).read()
        for m in fn_re.finditer(text):
            name = m.group(1)
            # The body: from the first `{` after the signature to its matching `}`.
            i = text.index("{", m.end())
            depth, j = 0, i
            while j < len(text):
                depth += text[j] == "{"
                depth -= text[j] == "}"
                if depth == 0:
                    break
                j += 1
            known.add(name)
            bodies.append((name, text[i:j + 1]))
    out = []
    for name, body in bodies:
        strings = [s.replace("\\0", "") for s in re.findall(r'b"((?:[^"\\]|\\.)*)"', body)]
        strings = [s for s in strings if s]
        calls = sorted({c for c in re.findall(r"\b([a-z_][a-z0-9_]*)\s*\(", body) if c in known and c != name})
        out.append({"name": camel(name), "strings": strings, "calls": [camel(c) for c in calls]})
    out.append({"name": "NeverThere", "strings": ["a string no build had"], "calls": []})
    return out


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

    objs = []
    for name in OBJECTS:
        obj = os.path.join(BUILD, name + ".o")
        compile_rs(tools, exe, os.path.join(HERE, name + ".rs"), obj)
        objs.append(obj)
    main_elf = os.path.join(BUILD, "main.elf")
    run([os.path.join(tools, "rust-lld" + exe), "-flavor", "gnu", "-T", os.path.join(BASIC, "psx.ld"), "-o", main_elf] + objs)
    main_exe = os.path.join(BUILD, "main.exe")
    run([sys.executable, os.path.join(BASIC, "wrap.py"), main_elf, main_exe, os.path.join(BUILD, "main.notes.json"), tools])
    main_syms = symbols(tools, exe, main_elf)

    overlays = {}
    for name in OVERLAYS:
        obj = os.path.join(BUILD, name + ".o")
        compile_rs(tools, exe, os.path.join(HERE, name + ".rs"), obj)
        elf = os.path.join(BUILD, name + ".elf")
        run([os.path.join(tools, "rust-lld" + exe), "-flavor", "gnu", "-T", os.path.join(ADVANCED, "overlay.ld"),
             "--just-symbols=" + main_elf, "-o", elf, obj])
        binary = os.path.join(BUILD, name + ".bin")
        run([os.path.join(tools, "llvm-objcopy" + exe), "-O", "binary", elf, binary])
        syms = [s for s in symbols(tools, exe, elf) if s[0] >= OVERLAY_AT]
        json.dump(notes_of(syms), open(os.path.join(BUILD, name + ".notes.json"), "w"), indent=1)
        overlays[name] = open(binary, "rb").read()

    ram = bytearray(0x200000)
    for at, word in [(0x80, 0x3C1A0000), (0xA0, 0x3C080000), (0xB0, 0x3C080000), (0xC0, 0x3C080000)]:
        ram[at:at + 4] = struct.pack("<I", word)
        ram[at + 8:at + 12] = struct.pack("<I", 0x03400008)
    exe_bytes = open(main_exe, "rb").read()
    load = struct.unpack_from("<I", exe_bytes, 0x18)[0] - 0x80000000
    body = exe_bytes[0x800:]
    ram[load:load + len(body)] = body
    loaded = overlays[LOADED_OVERLAY]
    ram[OVERLAY_AT - 0x80000000:OVERLAY_AT - 0x80000000 + len(loaded)] = loaded
    open(os.path.join(BUILD, "ram.bin"), "wb").write(ram)

    handlers = [s for s in main_syms if s[3].startswith("sys_")]
    with open(os.path.join(BUILD, "trace.txt"), "w") as f:
        for a, size, _, name in handlers:
            for pc in range(a, a + size, 4):
                f.write(f"{pc:08x}\n")

    files = [os.path.join(HERE, n + ".rs") for n in OBJECTS + OVERLAYS]
    candidates = candidates_from_sources(files)
    json.dump(candidates, open(os.path.join(BUILD, "candidates.json"), "w"), indent=1)
    library = sorted(s[3] for s in main_syms if s[2] == "t" and s[3] in open(os.path.join(HERE, "rt.rs")).read())
    json.dump(library, open(os.path.join(BUILD, "library.json"), "w"))
    print(f"\nbuilt: main.exe ({len(body)} bytes, {sum(1 for s in main_syms if s[2] == 't')} functions), "
          f"{', '.join(f'{n}.bin ({len(b)} bytes)' for n, b in overlays.items())} at {OVERLAY_AT:#x} "
          f"({LOADED_OVERLAY} loaded in ram.bin), trace.txt, {len(candidates)} candidates")


if __name__ == "__main__":
    main()
