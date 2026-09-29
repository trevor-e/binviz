"""Mutate Yamagi Quake II's game code blind (numeric constants in the .c files), then build it
with clang as an x86-64 game.so: the target to decompile. Prints counts only, never source.

    python3 build.py [--seed N]      in build/: the source, game.so, objs/ (the compiler's own
                                     objects, the answer key), decomp/ for your C
"""
import os, random, re, subprocess, shutil, glob, sys, urllib.request, tarfile
HERE = os.path.dirname(os.path.abspath(__file__))
os.makedirs(os.path.join(HERE, "build"), exist_ok=True)
os.chdir(os.path.join(HERE, "build"))
VERSION = "5.34"
if not os.path.isdir(f"quake2-{VERSION}"):
    url = f"https://deponie.yamagi.org/quake2/quake2-{VERSION}.tar.xz"
    print("downloading", url)
    urllib.request.urlretrieve(url, f"quake2-{VERSION}.tar.xz")
    with tarfile.open(f"quake2-{VERSION}.tar.xz") as t:
        t.extractall()
SRC = f"quake2-{VERSION}/src/game"
OUT = f"quake2-{VERSION}/src/gamemut"
SEED = int(sys.argv[sys.argv.index("--seed") + 1]) if "--seed" in sys.argv else 20260929
FLAGS = ["clang", "-O2", "-fno-strict-aliasing", "-fPIC", "-fno-stack-protector", "-fcf-protection=none", "-w", "-DYQ2OSTYPE=\"Linux\"", "-DYQ2ARCH=\"x86_64\"", "-DOSTYPE=\"Linux\"", "-DARCH=\"x86_64\"", "-c"]
shutil.rmtree(OUT, ignore_errors=True)
shutil.copytree(SRC, OUT)
shutil.rmtree("objs", ignore_errors=True)
os.makedirs("objs", exist_ok=True)
os.makedirs(OUT + "/decomp", exist_ok=True)
rng = random.Random(SEED)
num = re.compile(r"(?<![\w.#])(\d+\.\d+|\d+)(?![\w.xX])")
changed = kept = reverted = 0
files = sorted(glob.glob(OUT + "/**/*.c", recursive=True))
for path in files:
    orig = open(path, encoding="latin-1").read()
    out_lines = []
    n = [0]
    for line in orig.split("\n"):
        s = line.lstrip()
        if s.startswith("#") or s.startswith("case ") or "[" in line and "]" in line and "=" not in line:
            out_lines.append(line); continue
        def mut(m):
            v = m.group(1)
            if rng.random() > 0.3: return v
            if "." in v:
                n[0] += 1; return repr(float(v) + rng.choice([0.5, 1.5, 2.0]))
            if int(v) < 2: return v
            n[0] += 1; return str(int(v) + rng.randint(1, 3))
        out_lines.append(num.sub(mut, line))
    text = "\n".join(out_lines)
    open(path, "w", encoding="latin-1").write(text)
    obj = "objs/" + path[len(OUT) + 1:].replace("/", "_")[:-2] + ".o"
    r = subprocess.run(FLAGS + [path, "-o", obj], capture_output=True)
    if r.returncode != 0:
        open(path, "w", encoding="latin-1").write(orig)
        r = subprocess.run(FLAGS + [path, "-o", obj], capture_output=True)
        reverted += 1
        if r.returncode != 0:
            raise SystemExit(f"{path} doesn't compile even unchanged: {r.stderr[-300:]}")
    else:
        changed += n[0]
    kept += 1
subprocess.run(["clang", "-shared", "-o", "game.so"] + sorted(glob.glob("objs/*.o")) + ["-lm"], check=True)
print(f"{kept} files built; {changed} constants changed; {reverted} files kept unmutated (the mutation broke them)")
