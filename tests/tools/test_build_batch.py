"""Cold/warm, targeted invalidation, corruption and concurrent publication tests."""
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT / "tools"))
from build_batch import Batch

class BuildBatchTests(unittest.TestCase):
    def test_repository_layout_complete_scan_and_refused_measurements(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'src').mkdir();(root/'src/a.c').write_text('a');(root/'src/scan.c').write_text('scan')
            stage=dict(id='scan',phase='compile',inputs={'source':'src/a.c'},inputLayout={'source':'repo/src/a.c'},includeDirectories=['src'],includeLayout={'0':'repo/src'},executionRoot='repo',outputs={'out':'result','measure':'timing.json'},measurementsOutput='measure',command=[sys.executable,'-c',"from pathlib import Path;import sys,json;assert Path('src/a.c').read_text()=='a';assert len(list(Path('src').glob('*.c')))==2;Path(sys.argv[1]).write_text('built');Path(sys.argv[2]).write_text(json.dumps({'scanCount':2,'compileSeconds':0.1}));sys.exit(int(sys.argv[3]))",'{output:out}','{output:measure}','0'],memoryMb=32,mountMappings=[dict(source='execution-root',target='/work',readOnly=True),dict(source='outputs',target='/work/build',readOnly=False)])
            config=dict(schemaVersion=1,workers=1,memoryMb=64,stages=[stage],cache=dict(workspace=str(root),directory='.binviz-short-cache'),output=dict(workspace=str(root),directory='.binviz-short-output'))
            path=root/'config.json';path.write_text(json.dumps(config));cold=self.run_batch(path);self.assertEqual(cold['stages'][0]['diagnostic']['producerMeasurements']['scanCount'],2)
            self.assertEqual(self.run_batch(path)['statistics']['subprocesses'],0)
            (root/'src/scan.c').write_text('changed');self.assertEqual(self.run_batch(path)['statistics']['subprocesses'],1)
            stage['command'][-1]='7';path.write_text(json.dumps(config));batch=Batch(path,root/'failed.json');self.assertEqual(batch.run(),1)
            r=json.loads((root/'failed.json').read_bytes())['stages'][0];self.assertEqual(r['state'],'rejected');self.assertEqual(r['diagnostic']['producerMeasurements']['scanCount'],2);self.assertGreater(r['diagnostic']['producerSeconds'],0);self.assertGreater(r['diagnostic']['stagingSeconds'],0)

    def fixture(self,root):
        for name in ("a","b"):(root/(name+".c")).write_text(name)
        command=[sys.executable,"-c","import pathlib,sys; pathlib.Path(sys.argv[2]).write_bytes(pathlib.Path(sys.argv[1]).read_bytes()+b' compiled')","{input:source}","{output:object}"]
        stages=[dict(id=name,phase="compile",inputs={name+":source":name+".c"},outputs={name+":object":name+".o"},command=[x.replace("{input:source}","{input:"+name+":source}").replace("{output:object}","{output:"+name+":object}") for x in command],memoryMb=32) for name in ("a","b")]
        stages.append(dict(id="link",phase="link",dependsOn=["a","b"],inputs={"a:object":"stage:a:a:object","b:object":"stage:b:b:object"},outputs=dict(module="linked.wasm"),command=[sys.executable,"-c","import pathlib,sys; pathlib.Path(sys.argv[3]).write_bytes(pathlib.Path(sys.argv[1]).read_bytes()+pathlib.Path(sys.argv[2]).read_bytes())","{input:a:object}","{input:b:object}","{output:module}"],memoryMb=32))
        path=root/"config.json";path.write_text(json.dumps(dict(schemaVersion=1,workers=2,memoryMb=64,stages=stages)))
        return path
    def run_batch(self,path,name="report.json"):
        b=Batch(path,path.parent/name);self.assertEqual(b.run(),0);return json.loads((path.parent/name).read_bytes())
    def test_cold_warm_leaf_and_corrupt_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);path=self.fixture(root)
            cold=self.run_batch(path);self.assertEqual(cold["statistics"]["subprocesses"],3)
            warm=self.run_batch(path);self.assertEqual(warm["statistics"]["cacheHits"],3);self.assertEqual(warm["statistics"]["subprocesses"],0)
            self.assertEqual([s["outputs"] for s in cold["stages"]],[s["outputs"] for s in warm["stages"]])
            (root/"a.c").write_text("changed a")
            leaf=self.run_batch(path);self.assertEqual(leaf["statistics"]["subprocesses"],2);self.assertEqual(leaf["stages"][1]["state"],"cached")
            Path(leaf["stages"][0]["outputs"]["a:object"]["path"]).write_bytes(b"corrupt")
            repair=self.run_batch(path);self.assertEqual(repair["stages"][0]["state"],"success");self.assertEqual(repair["statistics"]["subprocesses"],1)
            self.assertLessEqual(repair["statistics"]["peakScheduledMemoryMb"],64)
    def test_header_shadowing_flags_and_concurrent_deduplication(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);path=self.fixture(root);include=root/"include";include.mkdir();(include/"x.h").write_text("one")
            c=json.loads(path.read_bytes());c["stages"][0]["includeDirectories"]=["include"];path.write_text(json.dumps(c))
            self.run_batch(path)
            (include/"shadow").mkdir();(include/"shadow/x.h").write_text("two")
            r=self.run_batch(path);self.assertEqual(r["stages"][0]["state"],"success")
            c["stages"][0]["environment"]={"SEMANTIC_FLAG":"changed"};path.write_text(json.dumps(c))
            r=self.run_batch(path);self.assertEqual(r["stages"][0]["state"],"success")
            (root/"a.c").write_text("concurrent edit")
            with ThreadPoolExecutor(2) as pool:
                results=list(pool.map(lambda n:self.run_batch(path,n),["one.json","two.json"]))
            self.assertEqual(sum(r["statistics"]["subprocesses"] for r in results),2)
            self.assertEqual(results[0]["stages"][-1]["outputs"],results[1]["stages"][-1]["outputs"])
    def test_before_candidate_have_identical_compiler_visible_paths(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);path=self.fixture(root);c=json.loads(path.read_bytes());c["stages"]=c["stages"][:2]
            command=[sys.executable,"-c","import pathlib,sys; pathlib.Path(sys.argv[2]).write_text(sys.argv[1]+'|'+sys.argv[2]+'|'+str(pathlib.Path.cwd()))"]
            for stage in c["stages"]:
                key=next(iter(stage["inputs"]));output=next(iter(stage["outputs"]))
                stage.update(compilerVisibleNamespace="readability",inputSlots={key:"source"},inputNames={key:"routine.i"},outputs={output:"routine.o"},command=[*command,"{input:"+key+"}","{output:"+output+"}"])
            path.write_text(json.dumps(c));report=self.run_batch(path)
            objects=[Path(next(iter(stage["outputs"].values()))["path"]).read_bytes() for stage in report["stages"]]
            self.assertEqual(objects[0],objects[1]);self.assertNotIn(b".work-",objects[0])
            self.assertEqual(self.run_batch(path)["statistics"]["subprocesses"],0)

    def test_bad_dependency_and_over_budget_stage_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);path=self.fixture(root);c=json.loads(path.read_bytes());c["stages"][0]["dependsOn"]=["link"];path.write_text(json.dumps(c))
            with self.assertRaises(ValueError):Batch(path,root/"out.json")
            c["stages"][0]["dependsOn"]=[];c["stages"][0]["memoryMb"]=100;path.write_text(json.dumps(c))
            with self.assertRaises(ValueError):Batch(path,root/"out.json")

    def test_cleanup_failure_preserves_real_process_diagnostic_and_releases_locks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);path=self.fixture(root);c=json.loads(path.read_bytes());c["stages"]=c["stages"][:1]
            c["stages"][0]["command"]=[sys.executable,"-c","import sys; print('compiler context'); print('real compiler failure',file=sys.stderr); sys.exit(7)"]
            path.write_text(json.dumps(c));batch=Batch(path,root/"report.json");remove=batch.remove
            def fail_work_cleanup(path,parent):
                if path.name.startswith(".work-"):raise OSError(145,"directory not empty")
                return remove(path,parent)
            batch.remove=fail_work_cleanup
            self.assertEqual(batch.run(),1);report=json.loads((root/"report.json").read_bytes());stage=report["stages"][0]
            self.assertEqual(stage["state"],"rejected");self.assertEqual(stage["diagnostic"]["exitCode"],7)
            self.assertIn("compiler context",stage["diagnostic"]["stdout"]);self.assertIn("real compiler failure",stage["diagnostic"]["stderr"])
            self.assertEqual(len(stage["cleanupWarnings"]),1);self.assertFalse(list(batch.cache.glob("*.lock")))
            self.assertFalse(list(batch.cache.glob("*/record.json")))
            diagnostic=next(a for a in report["evidence"]["artifacts"]if a["role"]=="diagnostic")
            self.assertEqual(json.loads(Path(diagnostic["location"]).read_bytes())["exitCode"],7)

    def test_published_outputs_remain_verified_after_cleanup_warning(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);path=self.fixture(root);c=json.loads(path.read_bytes());c["stages"]=c["stages"][:1];path.write_text(json.dumps(c))
            batch=Batch(path,root/"cold.json");remove=batch.remove
            def fail_work_cleanup(path,parent):
                if path.name.startswith(".work-"):raise OSError(145,"directory not empty")
                return remove(path,parent)
            batch.remove=fail_work_cleanup
            self.assertEqual(batch.run(),0);cold=json.loads((root/"cold.json").read_bytes())
            self.assertEqual(cold["stages"][0]["state"],"success");self.assertTrue(cold["stages"][0]["cleanupWarnings"])
            warm=self.run_batch(path,"warm.json");self.assertEqual(warm["statistics"]["subprocesses"],0)
            self.assertEqual(cold["stages"][0]["outputs"],warm["stages"][0]["outputs"])

    def test_cleanup_failure_does_not_hide_missing_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);path=self.fixture(root);c=json.loads(path.read_bytes());c["stages"]=c["stages"][:1]
            c["stages"][0]["outputs"]={"a:object":"missing-compiler-output.o"};c["stages"][0]["command"]=[sys.executable,"-c","print('compile finished without expected output')"]
            path.write_text(json.dumps(c));batch=Batch(path,root/"report.json");remove=batch.remove
            def fail_work_cleanup(path,parent):
                if path.name.startswith(".work-"):raise OSError(145,"directory not empty")
                return remove(path,parent)
            batch.remove=fail_work_cleanup
            self.assertEqual(batch.run(),1);stage=json.loads((root/"report.json").read_bytes())["stages"][0]
            self.assertEqual(stage["state"],"refused");self.assertIn("missing-compiler-output.o",stage["reason"])
            self.assertEqual(stage["diagnostic"]["exitCode"],0);self.assertTrue(stage["recipe"]);self.assertTrue(stage["cleanupWarnings"])

if __name__=="__main__":unittest.main()
