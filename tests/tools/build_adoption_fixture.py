"""Real compiler objects for void execution, DATA closure and COP2 callbacks."""
import hashlib,json,shutil,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'tools'));import compiler_facts
def encode(x):return json.dumps(x,sort_keys=True,separators=(',',':')).encode()
def main():
    f=ROOT/'tests/fixtures/adoption';f.mkdir(exist_ok=True)
    (f/'void.c').write_text('unsigned state; void void_fill(unsigned n) { state = n + 1; } unsigned get_word(void) { return state; }\n')
    c=dict(schemaVersion=1,compiler=shutil.which('clang'),target='wasm32-unknown-unknown',flags=['-O0'],units=[dict(id='void',source='void.c')])
    (f/'config.json').write_bytes(encode(c));assert compiler_facts.produce(f/'config.json',f/'facts.json')==0
    facts=json.loads((f/'facts.json').read_bytes());e=facts['evidence'];clang=c['compiler']
    flags=['--target=wasm32-unknown-unknown','-O0','-nostdlib','-x','cpp-output']
    subprocess.run([clang,*flags,str(f/'facts.artifacts/void.prepared.i'),'-c','-o',str(f/'void.o')],check=True)
    subprocess.run([clang,'--target=wasm32-unknown-unknown','-nostdlib',str(f/'void.o'),'-Wl,--no-entry','-Wl,--export=void_fill','-Wl,--export=get_word','-Wl,--export=state','-Wl,--export=__data_end','-o',str(f/'void.wasm')],check=True)
    (f/'unresolved.c').write_text('extern unsigned missing_a, missing_b; unsigned unresolved(void) { return missing_a + missing_b; }\n')
    subprocess.run([clang,'--target=wasm32-unknown-unknown','-O0','-c',str(f/'unresolved.c'),'-o',str(f/'unresolved.o')],check=True)
    subprocess.run([clang,'--target=wasm32-unknown-unknown','-nostdlib',str(f/'unresolved.o'),'-Wl,--no-entry','-Wl,--allow-undefined','-Wl,--export=unresolved','-Wl,--export=missing_a','-Wl,--export=missing_b','-Wl,--export=__data_end','-o',str(f/'unresolved.wasm')],check=True)
    def a(id,name,role):e['artifacts'].append(dict(id=id,role=role,location=name,sha256=hashlib.sha256((f/name).read_bytes()).hexdigest()))
    (f/'recipe.json').write_bytes(encode(dict(compiler=facts['compiler'],compileFlags=flags,link='void.o actual exported void body')))
    for id,name,role in [('adopt-recipe','recipe.json','recipe'),('void-object','void.o','object'),('void-module','void.wasm','linked-module'),('unresolved-object','unresolved.o','object'),('unresolved-module','unresolved.wasm','linked-module')]:a(id,name,role)
    e['stages'].extend([dict(id='void-compile',parents=['void:prepared','compiler'],outputs=['void-object'],recipe='adopt-recipe',result='success'),dict(id='void-link',parents=['void-object','compiler'],outputs=['void-module'],recipe='adopt-recipe',result='success')])
    (f/'build-evidence.json').write_bytes(encode(e))
    # A callback ABI fixture, deliberately not a GTE implementation or oracle.
    source='__declspec(dllexport) unsigned gte_d[32],gte_c[32],gte_unimplemented;\n'
    source+='__declspec(dllexport) void gte_mtc2(int n,unsigned v){gte_d[n]=v;} __declspec(dllexport) void gte_ctc2(int n,unsigned v){gte_c[n]=v;}\n'
    source+='__declspec(dllexport) unsigned gte_mfc2(int n){return gte_d[n];} __declspec(dllexport) unsigned gte_cfc2(int n){return gte_c[n];}\n'
    source+='__declspec(dllexport) void gte_cop2(unsigned w){if(w!=1){gte_unimplemented=1;return;}gte_d[0]+=gte_c[0];}\n'
    (f/'callback.c').write_text(source)
    subprocess.run([clang,'--target=x86_64-pc-windows-msvc','-O0','-ffreestanding','-c',str(f/'callback.c'),'-o',str(f/'callback.obj')],check=True)
    subprocess.run(['lld-link-14','/dll','/noentry','/out:'+str(f/'callback.dll'),str(f/'callback.obj')],check=True)
if __name__=='__main__':main()
