"""Real compiler-selected GPU-style discard and equal-arity service fixtures."""
import json,shutil,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'tools'));import compiler_facts
def main():
    f=ROOT/'tests/fixtures/lowerings';f.mkdir(exist_ok=True)
    (f/'caller.c').write_text('typedef struct RECT { int x; } RECT;\nextern void legacy_draw(RECT *);\nextern int legacy_vblank(int);\nvoid frame(RECT *r) { legacy_draw(r); }\nint poll(int n) { return legacy_vblank(n); }\n')
    (f/'providers.c').write_text('typedef struct RECT { int x; } RECT;\nint selected_gpu(RECT *r) { return ++r->x; }\nextern int runtime_vblank(int) __attribute__((import_module("env"),import_name("runtime_vblank")));\nint selected_vblank(int n) { return runtime_vblank(n); }\n')
    c=dict(schemaVersion=1,compiler=shutil.which('clang'),target='wasm32-unknown-unknown',flags=['-O0'],units=[dict(id=id,source=id+'.c') for id in ['caller','providers']]);(f/'config.json').write_text(json.dumps(c));assert compiler_facts.produce(f/'config.json',f/'facts.json')==0
    (f/'candidate.c').write_text('typedef struct RECT { int x; } RECT; extern int selected_gpu(RECT *), selected_vblank(int); void frame(RECT *r) { (void)selected_gpu(r); } int poll(int n) { return selected_vblank(n); }\n')
    clang=c['compiler'];flags=['--target=wasm32-unknown-unknown','-O0','-nostdlib','-fno-inline']
    subprocess.run([clang,*flags,'-c',str(f/'providers.c'),'-o',str(f/'providers.o')],check=True)
    subprocess.run([clang,*flags,str(f/'candidate.c'),str(f/'providers.o'),'-Wl,--no-entry','-Wl,--export=frame','-Wl,--export=poll','-Wl,--export=selected_gpu','-Wl,--export=selected_vblank','-o',str(f/'linked.wasm')],check=True)
if __name__=='__main__':main()
