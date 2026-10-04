"""Regenerate tracked compiler/object fixtures with real Clang; no game assets.

Run inside the offline toolchain container from the repository root.
The compiler itself is not checked in; ordinary Rust tests substitute its bytes
to exercise identity transitions. Production manifests retain the actual digest.
"""
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
import compiler_facts

def main():
    fixture=ROOT / "tests/fixtures/workspace"
    assert compiler_facts.produce(fixture / "config.json", fixture / "facts.json")==0
    clang=shutil.which("clang")
    common=[clang,"--target=wasm32-unknown-unknown","-O0","-mmutable-globals","-fno-inline","-nostdlib"]
    subprocess.run([*common,"-c",str(fixture / "provider.c"),"-o",str(fixture / "provider.o")],check=True)
    subprocess.run([*common,"-c",str(fixture / "linked.c"),"-o",str(fixture / "caller.o")],check=True)
    subprocess.run([*common,str(fixture / "linked.c"),str(fixture / "provider.o"),"-Wl,--no-entry","-Wl,--export=caller_a","-Wl,--export=caller_b","-Wl,--export=provider","-Wl,--export=tentative","-Wl,--export=__stack_pointer","-Wl,--export=__data_end","-Wl,--export=__heap_base","-o",str(fixture / "linked.wasm")],check=True)

if __name__=="__main__":main()
