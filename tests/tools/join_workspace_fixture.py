"""Join the frozen real-Clang fixture using Binviz's actual WASM inspection.

Run on the host after build_workspace_fixture.py and cargo build -p binviz-cli.
This is test configuration, not a second C parser, native CFG or WASM reader.
"""
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[2]
F=ROOT / "tests/fixtures/workspace"
def data(v):return json.dumps(v,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()
def digest(b):return hashlib.sha256(b).hexdigest()
def main():
    cli=ROOT / "target/debug" / ("binviz.exe" if sys.platform=="win32" else "binviz")
    module=json.loads(subprocess.check_output([cli,"linked",F / "linked.wasm"]))
    facts=json.loads((F / "facts.json").read_text())
    evidence=json.loads(json.dumps(facts["evidence"]))
    def artifact(id,role,name,bytes=None):
        path=F / name
        if bytes is not None:path.write_bytes(bytes)
        evidence["artifacts"].append(dict(id=id,role=role,location=name,sha256=digest(path.read_bytes())))
        return id
    artifact("provider-object","object","provider.o")
    artifact("linked","linked-module","linked.wasm")
    artifact("build-recipe","recipe","build-recipe.json",data(dict(compiler=facts["compiler"],target="wasm32-unknown-unknown",flags=["-O0","-fno-inline","-mmutable-globals"],exports=["caller_a","caller_b","provider","tentative","__stack_pointer","__data_end","__heap_base"])))
    artifact("candidate","prepared","linked.c")
    evidence["stages"].extend([
        dict(id="provider-compile",parents=["p:prepared","compiler"],outputs=["provider-object"],recipe="build-recipe",result="success"),
        dict(id="module-link",parents=["provider-object","candidate","compiler"],outputs=["linked"],recipe="build-recipe",result="success")])
    units=[]; bindings=[]
    for u,name,entry,words in [("overlay-a","caller_a",0x80010000,[0x0c008000,0x24050007,0x24050000,0x03e00008,0]),("overlay-b","caller_b",0x80010000,[0x0c008000,0x24050009,0x24050000,0x03e00008,0]),("main","provider",0x80020000,[0x24050000,0x24820001,0x03e00008,0])]:
        raw=struct.pack("<"+"I"*len(words),*words)
        asset=artifact(u+":asset","original",u+".bin",raw)
        member=artifact(u+":member","member",u+".bin")
        native=artifact(u+":native","native",u+".bin")
        units.append(dict(identity=dict(id=u,asset=asset,memberOffset="0x0",memberSize=hex(len(raw)),loadAddress=hex(entry),context=u),memberArtifact=member,architecture="ps1-mipsel",functions=[dict(id=u+":"+name,name=name,identity=dict(unit=u,entry=hex(entry),role="primary",analysisExtent=dict(start=hex(entry),bytes=hex(len(raw)),exact=True),matchingExtent=dict(start=hex(entry),bytes="0x8",exact=True)),analysisArtifact=native,matchingArtifact=None,owner=None,aliases=[],exclusions=[])],gaps=[]))
    definition=next(d for d in facts["definitions"] if d["name"]=="provider" and d["hasBody"])
    for call in facts["calls"]:
        name="caller_"+call["unit"]
        caller=next(f for f in module["functions"] if name in f["exports"])
        binding=dict(call=call["id"],caller="overlay-"+call["unit"]+":"+name,provider="main:provider",definition=definition["id"],providerKind="reconstructed-c",originalPc="0x80010000",originalWord="0x0c008000",delayWord="0x2405000"+("7" if call["unit"]=="a" else "9"),module="linked",object="provider-object",callerExport=name,providerExport="provider",linkedCallOffset=caller["calls"][0]["offset"],correspondenceArtifact=call["unit"]+":correspondence")
        artifact(binding["correspondenceArtifact"],"reviewed-correspondence",call["unit"]+".correspondence.json",data(binding));bindings.append(binding)
    a=bindings[0]
    policy=dict(id="a-extra-a1",kind="extra-word-nonuse",allowedCallers=[a["caller"]],allowedSites=[a["call"]],provider=a["provider"],definition=a["definition"],expectedCounts={a["call"]:2},registers=[5],dependencies=["overlay-a:native","main:native","a:prepared","linked","provider-object","a:correspondence"],reviewArtifact="a:policy",guard=None)
    artifact("a:policy","reviewed-policy","a.policy.json",data(policy))
    proof=dict(id="provider-a1",provider="main:provider",register=5,dependencies=["main:native"],reviewArtifact="provider-a1-review",nativeSha256=digest((F / "main.bin").read_bytes()),extent=units[2]["functions"][0]["identity"]["analysisExtent"],callees=[],mode="conservative")
    artifact("provider-a1-review","reviewed-callee-proof","provider-a1.review.json",data(proof))
    g=module["globals"]
    layout=dict(module="linked",guestRam=dict(name="guest-data",start="0x400",bytes="0xe"),reserved=[dict(name="stack",start="0x410",bytes=hex(int(g["__stack_pointer"],16)-0x410))],stackPointerExport="__stack_pointer",stackPointer=g["__stack_pointer"],dataEndExport="__data_end",dataEnd=g["__data_end"],heapBaseExport="__heap_base",heapBase=g["__heap_base"],memoryImport=None,tableImport=None,sharedMemory=False,requiredImports=[],data=[dict(name="tentative",export="tentative",address=g["tentative"],bytes="0xe",aliasOf=None)])
    workspace=dict(format="binviz-workspace",schemaVersion=1,evidence=evidence,inventory=dict(schemaVersion=1,units=units,decisions=[]),compilerFacts=facts,bindings=bindings,policies=[policy],applications=[],builds=[dict(unit="p",stage="provider-compile",phase="compile"),dict(unit="a",stage="module-link",phase="link"),dict(unit="b",stage="module-link",phase="link")],layouts=[layout])
    workspace["calleeCertificates"]=[proof]
    (F / "workspace.json").write_bytes(data(workspace))

if __name__=="__main__":main()
