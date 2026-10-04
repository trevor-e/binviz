"""Actual compiler baseline/candidate objects for scoped publication acceptance."""
import hashlib,json,shutil,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/"tools"))
import compiler_facts
def main():
    f=ROOT/"tests/fixtures/acceptance";f.mkdir(exist_ok=True)
    (f/"p.c").write_text("int provider(int word) { return word + 1; }\n")
    (f/"q.c").write_text("int provider(int value) { return value + 1; }\n")
    clang=shutil.which("clang")
    config=dict(schemaVersion=1,compiler=clang,target="wasm32-unknown-unknown",flags=["-O1"],storageFacts=True,units=[dict(id=id,source=id+".c") for id in ("p","q")])
    (f/"config.json").write_text(json.dumps(config));assert compiler_facts.produce(f/"config.json",f/"facts.json")==0
    facts=json.loads((f/"facts.json").read_bytes());e=facts["evidence"]
    def artifact(id,role,path):e["artifacts"].append(dict(id=id,role=role,location=path.name,sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
    visible=f/".visible";visible.mkdir(exist_ok=True);source=visible/"provider.i";object=visible/"provider.o"
    flags=["--target=mipsel-unknown-none","-O2","-march=mips32","-msoft-float","-fomit-frame-pointer","-x","cpp-output","-c"]
    for id,kind in [("p","baseline"),("q","candidate")]:
        source.write_bytes((f/"facts.artifacts"/(id+".prepared.i")).read_bytes())
        subprocess.run([clang,*flags,str(source),"-o",str(object)],check=True,cwd=visible)
        output=f/(kind+".o");shutil.copyfile(object,output);artifact(kind+"-object","native-object",output)
        recipe=dict(compilerVisibleNamespace="acceptance",inputSlots={id+":prepared":"source"},inputNames={id+":prepared":"provider.i"},command=[clang,*flags,"{input:"+id+":prepared}","-o","{output:"+kind+"-object}"],tools={clang:next(a["sha256"] for a in e["artifacts"] if a["id"]=="compiler")},environmentDigests={},includes=[],outputs={kind+"-object":"provider.o"})
        path=f/(kind+".recipe.json");path.write_text(json.dumps(recipe,sort_keys=True,separators=(",",":")));artifact(kind+"-recipe","recipe",path)
        e["stages"].append(dict(id=kind+"-compile",parents=[id+":prepared","compiler"],outputs=[kind+"-object"],recipe=kind+"-recipe",result="success"))
    subprocess.run([shutil.which("llvm-objcopy"),"-O","binary","--only-section=.text",str(f/"baseline.o"),str(f/"native.bin")],check=True)
    artifact("native","native-member",f/"native.bin")
    (f/"evidence.json").write_text(json.dumps(e,sort_keys=True,separators=(",",":")))
    source.unlink();object.unlink();visible.rmdir()
if __name__=="__main__":main()
