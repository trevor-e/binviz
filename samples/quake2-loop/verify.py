"""Independent of binviz: a function's bytes and relocations in my object vs the compiler's own
object, with trailing alignment filler dropped and each relocation resolved to the symbol it reaches
(section-relative ones included), so only the code and what it refers to are compared."""
import os, re, subprocess, struct, sys
os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), "build"))
def sections(obj):
    """name -> bytes of each section of an ELF64 object."""
    d = open(obj, "rb").read()
    shoff, = struct.unpack_from("<Q", d, 0x28)
    shentsize, shnum, shstrndx = struct.unpack_from("<HHH", d, 0x3A)
    hdrs = [struct.unpack_from("<IIQQQQIIQQ", d, shoff + i * shentsize) for i in range(shnum)]
    names = hdrs[shstrndx]
    strtab = d[names[4]:names[4] + names[5]]
    out = {}
    for h in hdrs:
        name = strtab[h[0]:strtab.index(b"\0", h[0])].decode()
        out[name] = d[h[4]:h[4] + h[5]] if h[1] != 8 else b""
    return out
def labels(obj):
    """Local labels (.LCPI…, .L.str…): (section, offset) from the symbol table."""
    out = subprocess.run(["llvm-objdump", "-t", obj], capture_output=True, text=True).stdout
    lab = {}
    for l in out.splitlines():
        m = re.match(r"([0-9a-f]{16}) .{7} (\S+)\s+[0-9a-f]{16} (\.L\S+)$", l)
        if m:
            lab[m.group(3)] = (m.group(2), int(m.group(1), 16))
    return lab
def symbols(obj):
    """(section, start, end, name) of each named data/function symbol."""
    out = subprocess.run(["llvm-objdump", "-t", obj], capture_output=True, text=True).stdout
    syms = []
    for l in out.splitlines():
        m = re.match(r"([0-9a-f]{16}) (.{7}) (\S+)\s+([0-9a-f]{16}) (\S+)$", l)
        if m and m.group(3) not in ("*UND*", "*ABS*") and not m.group(5).startswith(".") and m.group(5) != m.group(3):
            start, size = int(m.group(1), 16), int(m.group(4), 16)
            syms.append((m.group(3), start, start + max(size, 1), m.group(5)))
    return syms
def body(obj, fn):
    syms = symbols(obj)
    secs, labs = sections(obj), labels(obj)
    def content(sec, off):
        data = secs.get(sec, b"")[off:]
        if ".str" in sec:
            if b"\0" in data:
                return "string " + repr(data[:data.index(b"\0")].decode("latin-1"))
            return f"{sec}+{off} (past its end?)"
        m = re.search(r"cst(\d+)$", sec)
        return "constant " + data[:int(m.group(1)) if m else 16].hex()
    out = subprocess.run(["llvm-objdump", "-d", "-r", "--no-leading-addr", f"--disassemble-symbols={fn}", obj], capture_output=True, text=True).stdout
    out = subprocess.run(["llvm-objdump", "-d", "-r", f"--disassemble-symbols={fn}", obj], capture_output=True, text=True).stdout
    items = []  # (addr, len, bytes, text) and relocations (field addr, type, sym, addend)
    relocs = []
    for l in out.splitlines():
        m = re.match(r"\s*([0-9a-f]+):\s+((?:[0-9a-f]{2} ?)+?) *\t\s*(.*)", l)
        if m:
            b = m.group(2).split()
            items.append([int(m.group(1), 16), len(b), " ".join(b), m.group(3).strip()])
            continue
        m = re.match(r"\s*([0-9a-f]+):\s+(R_X86_64_\S+)\s+(\S+)", l)
        if m:
            sm = re.match(r"(.+?)([+-]0x[0-9a-f]+)?$", m.group(3))
            relocs.append((int(m.group(1), 16), m.group(2), sm.group(1), int(sm.group(2) or "0", 16)))
    # Drop the filler after the function's code.
    while items and re.match(r"(nop|xchg\s+%ax, %ax|data16|int3)", items[-1][3]):
        items.pop()
    end = items[-1][0] + items[-1][1] if items else 0
    lines = [("bytes", i[2]) for i in items]
    for at, kind, sym, addend in relocs:
        if at >= end:
            continue
        if sym.startswith(".") and sym not in labs:  # a section: which symbol in it does the code reach?
            ins = next(i for i in items if i[0] <= at < i[0] + i[1])
            target = addend + (ins[0] + ins[1] - at) if "PC" in kind or "PLT" in kind or "GOT" in kind else addend
            hit = [s for s in syms if s[0] == sym and s[1] <= target < s[2]]
            sym = f"{hit[0][3]}+{target - hit[0][1]}" if hit else content(sym, target)
            addend = 0
        elif sym in labs:  # a label: what it holds
            sec, off = labs[sym]
            ins = next(i for i in items if i[0] <= at < i[0] + i[1])
            sym = content(sec, off + addend + (ins[0] + ins[1] - at))
            addend = 0
        lines.append(("reloc", kind, sym, addend))
    return lines
# verify.py <function>[:<also> …] …  — each function's decomp/<function>.c object against the
# compiler's own object for it; `also` are functions the same file holds (ones it inlines).
wanted = [a.split(":") for a in sys.argv[1:]]
owner = {}
for o in sorted(__import__("glob").glob("objs/*.o")):
    for l in subprocess.run(["llvm-nm", "--defined-only", o], capture_output=True, text=True).stdout.splitlines():
        p = l.split()
        if len(p) == 3 and p[1] in "Tt":
            owner.setdefault(p[2], o)
checked = [(owner[w[0]], w[0], 0) for w in wanted]
extra = {w[0]: w[1:] for w in wanted}
ok = total = 0
for orig_obj, fn, _ in checked:
    for f in [fn] + extra.get(fn, []):
        total += 1
        a, b = body(orig_obj, f), body(f"decomp_{fn}.o", f)
        same = a == b and bool(a)
        ok += same
        n = sum(len(x[1].split()) for x in a if x[0] == "bytes")
        print(f"{'IDENTICAL' if same else 'DIFFERENT'}  {f:24s} {n:4d} bytes of code, {sum(x[0] == 'reloc' for x in a):2d} references")
        if not same:
            import difflib
            for l in list(difflib.unified_diff([str(x) for x in a], [str(x) for x in b], lineterm="", n=0))[2:8]: print("     ", l)
print(f"{ok} of {total} identical")
