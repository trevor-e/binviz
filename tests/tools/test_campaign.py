"""Actual native DLL versus actual WASM execution and negative controls.
Build modules with build_campaign_fixture.py first. No game assets are needed.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT / "tools"))
from proof_campaign import run

@unittest.skipUnless(sys.platform=="win32" and (ROOT/"target/campaign-demo/native.dll").exists() and shutil.which("node"),"build real native/WASM fixture first")
class CampaignTests(unittest.TestCase):
    def config(self,root,variant):
        fixture=ROOT/"tests/fixtures/campaign";modules=ROOT/"target/campaign-demo"
        artifacts={"native-module":str(modules/"native.dll"),"wasm-module":str(modules/(variant+".wasm")),"native-adapter":str(fixture/"native_runner.py"),"wasm-adapter":str(fixture/"wasm_runner.mjs")}
        config=dict(schemaVersion=1,workers=2,memoryMb=128,timeoutSeconds=10,artifacts=artifacts,native=dict(command=[sys.executable,"{artifact:native-adapter}"],memoryMb=32),wasm=dict(command=[shutil.which("node"),"{artifact:wasm-adapter}"],memoryMb=32),comparison=dict(returnMask="0xffffffff",ram=["state"],registers=[],eventKinds=["call","device","return"],requiredCheckpoints=["after-device"],expectedFrontiers=[],masks=[],localObjects=[]),cases=[dict(id="word-"+hex(v),arguments=[hex(v)],seed="0x448") for v in [0,1,0x7fff,0x8000,0xffff,0x8000ffff,0x80123456]])
        path=root/"campaign.json";path.write_text(json.dumps(config));return path
    def compare(self,root,variant):
        path=self.config(root,variant);raw=root/"raw.json";self.assertEqual(run(path,raw),0)
        cli=ROOT/"target/debug/binviz.exe";report=root/"report.json"
        p=subprocess.run([cli,"campaign-compare",raw,"--out",report],capture_output=True,text=True)
        return p,json.loads(report.read_bytes())
    def test_two_wasm_roles_and_native_break_exception_point(self):
        for native in [False,True]:
            game=Path(r"C:/Users/elkin/dev/ff9-decomp")
            if native and not (game/"wasm/test-asmgen.py").exists():continue
            for variant in ["trap-good","trap-write-first"]:
                with self.subTest(native=native,variant=variant),tempfile.TemporaryDirectory() as tmp:
                    root=Path(tmp);path=self.config(root,variant);c=json.loads(path.read_bytes())
                    c["wasm"]["command"]=[shutil.which("node"),"{artifact:trap-adapter}"];c["artifacts"]["trap-adapter"]=str(ROOT/"tests/fixtures/campaign/trap_runner.mjs")
                    if native:
                        c["native"]["command"]=[sys.executable,"{artifact:break-adapter}"]
                        c["artifacts"].update({"break-adapter":str(ROOT/"tests/fixtures/campaign/native_break_runner.py"),"cpu-model":str(game/"wasm/test-asmgen.py"),"cpu-helper":str(game/"wasm/asmgen.py")})
                    else:
                        c["baseline"]=dict(c["wasm"]);c["candidate"]=c.pop("wasm");c.pop("native")
                    c["comparison"].update(ram=["cursor"],eventKinds=[],requiredCheckpoints=["exception-point"]);c["cases"]=[dict(id="zero-divisor",arguments=["0x0"])]
                    # A two-WASM baseline uses the good module; candidate selects the variant.
                    if not native:
                        c["artifacts"]["baseline-wasm"]=str(ROOT/"target/campaign-demo/trap-good.wasm")
                        c["baseline"]["profile"]={"moduleArtifact":"baseline-wasm"}
                    path.write_text(json.dumps(c));raw=root/"raw.json";self.assertEqual(run(path,raw),0)
                    report=root/"report.json";p=subprocess.run([ROOT/"target/debug/binviz.exe","campaign-compare",raw,"--out",report],capture_output=True,text=True)
                    r=json.loads(report.read_bytes());self.assertEqual(r["executedPairs"],1)
                    self.assertEqual(p.returncode==0,variant=="trap-good",p.stderr)
                    if variant!="trap-good":self.assertTrue(any(d["kind"]=="ram-byte" for d in r["firstFailure"]["result"]["differences"]))

    def test_real_executions_and_each_concrete_failure(self):
        for variant,kind in [("good",None),("wrong-word","return"),("wrong-ram","ram-byte"),("extra-device","ordered-events")]:
            with self.subTest(variant=variant),tempfile.TemporaryDirectory() as tmp:
                p,r=self.compare(Path(tmp),variant)
                self.assertEqual(r["executedPairs"],7);self.assertTrue(r["observationsBound"])
                self.assertTrue(all(c["state"]=="verified" for c in r["identityChecks"]))
                if kind is None:self.assertEqual(p.returncode,0,p.stderr);self.assertEqual(r["failed"],0)
                else:
                    self.assertNotEqual(p.returncode,0);self.assertEqual(r["failed"],7)
                    self.assertTrue(any(d["kind"]==kind for d in r["firstFailure"]["result"]["differences"]))

if __name__=="__main__":unittest.main()
