"""Build real freestanding native DLL and WASM campaign variants with Clang."""
from pathlib import Path
import subprocess
ROOT=Path(__file__).resolve().parents[2]
source=ROOT / "tests/fixtures/campaign/compute.c"
out=ROOT / "target/campaign-demo";out.mkdir(parents=True,exist_ok=True)
subprocess.run(["clang","--target=x86_64-pc-windows-msvc","-ffreestanding","-O0","-c",str(source),"-o",str(out / "native.obj")],check=True)
subprocess.run(["lld-link-14","/dll","/noentry","/out:"+str(out / "native.dll"),str(out / "native.obj")],check=True)
for name,flag in [("good",None),("wrong-word","WRONG_WORD"),("wrong-ram","WRONG_RAM"),("extra-device","EXTRA_DEVICE")]:
    command=["clang","--target=wasm32-unknown-unknown","-O0","-nostdlib",str(source),"-Wl,--no-entry","-Wl,--export-all","-o",str(out / (name+".wasm"))]
    if flag:command.append("-D"+flag)
    subprocess.run(command,check=True)
for name,extra in [("trap-good",[]),("trap-write-first",["-DWRITE_FIRST"])]:
    subprocess.run(["clang","--target=wasm32-unknown-unknown","-O2","-nostdlib",*extra,str(ROOT/"tests/fixtures/campaign/trap.c"),"-Wl,--no-entry","-Wl,--export-all","-Wl,--global-base=4096","-o",str(out/(name+".wasm"))],check=True)
