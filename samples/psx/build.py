"""Build the sample PlayStation program, and rebuilds of its functions.

    python build.py                    build/sample.exe, sample.o, sample.elf, sample.notes.json
    python build.py --rebuild my.c     build/my.o from your C (or .rs), the same way, to `match`

A MIPS C compiler is used when there is one (clang on PATH, or --cc <path>);
otherwise the Rust version of the sample is compiled through the llc that
ships with Rust's llvm-tools (rustup component add llvm-tools). Linking and
wrapping as a PS-X EXE need only Rust's tools.
"""
import argparse
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
BUILD = os.path.join(HERE, "build")


def run(cmd, **kw):
    print("  " + " ".join(os.path.basename(c) if i == 0 else c for i, c in enumerate(cmd)))
    subprocess.run(cmd, check=True, **kw)


def llvm_tools():
    sysroot = subprocess.run(["rustc", "--print", "sysroot"], capture_output=True, text=True, check=True).stdout.strip()
    host = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    host = next(l.split()[1] for l in host.splitlines() if l.startswith("host:"))
    path = os.path.join(sysroot, "lib", "rustlib", host, "bin")
    exe = ".exe" if os.name == "nt" else ""
    if not os.path.exists(os.path.join(path, "llc" + exe)):
        sys.exit(f"no llc in {path}: run `rustup component add llvm-tools`")
    return path, exe


def compile_c(cc, src, obj):
    run([cc, "--target=mipsel-unknown-elf", "-march=mips1", "-msoft-float", "-mno-abicalls", "-fno-pic", "-G0",
         "-O2", "-fno-builtin", "-nostdlib", "-c", src, "-o", obj])


def compile_rs(tools, exe, src, obj):
    ll = obj[:-2] + ".ll"
    run(["rustc", "--target", "wasm32-unknown-unknown", "--crate-type=lib", "-C", "panic=abort", "-C", "opt-level=2",
         "-C", "debuginfo=0", "--emit=llvm-ir", "-o", ll, src])
    text = open(ll).read()
    text = "\n".join(l for l in text.splitlines() if not l.startswith(("target datalayout", "target triple")))
    open(ll, "w").write(text)
    run([os.path.join(tools, "llc" + exe), "-mtriple=mipsel-unknown-elf", "-mcpu=mips1", "-float-abi=soft",
         "-relocation-model=static", "-filetype=obj", "-O2", ll, "-o", obj])


def compile_any(cc, tools, exe, src, obj):
    if src.endswith(".rs"):
        compile_rs(tools, exe, src, obj)
    elif cc:
        compile_c(cc, src, obj)
    else:
        sys.exit(f"{src}: no MIPS C compiler (put clang on PATH or pass --cc); the .rs sample builds without one")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cc", help="a clang that can target mipsel (default: clang on PATH, if any)")
    ap.add_argument("--rebuild", metavar="SRC", help="compile this .c or .rs to build/<name>.o and stop")
    args = ap.parse_args()
    os.makedirs(BUILD, exist_ok=True)
    tools, exe = llvm_tools()
    cc = args.cc or shutil.which("clang")

    if args.rebuild:
        src = os.path.abspath(args.rebuild)
        obj = os.path.join(BUILD, os.path.splitext(os.path.basename(src))[0] + ".o")
        compile_any(cc, tools, exe, src, obj)
        print(f"\n{obj}: now\n  binviz --notes build/sample.notes.json match build/sample.exe {os.path.relpath(obj, HERE)} <function>")
        return

    src = os.path.join(HERE, "sample.c" if cc else "sample.rs")
    print(f"compiling {os.path.basename(src)} with {'clang' if cc else 'rustc + llc'}")
    obj = os.path.join(BUILD, "sample.o")
    compile_any(cc, tools, exe, src, obj)
    elf = os.path.join(BUILD, "sample.elf")
    run([os.path.join(tools, "rust-lld" + exe), "-flavor", "gnu", "-T", os.path.join(HERE, "psx.ld"), "-o", elf, obj])
    run([sys.executable, os.path.join(HERE, "wrap.py"), elf, os.path.join(BUILD, "sample.exe"),
         os.path.join(BUILD, "sample.notes.json"), tools])
    print("\nnext (from this folder, with binviz on PATH or `cargo run --release -p binviz-cli --`):")
    print("  binviz --notes build/sample.notes.json context build/sample.exe entity_step")
    print("  binviz --notes build/sample.notes.json match build/sample.exe build/sample.o")


if __name__ == "__main__":
    main()
