"""Synthetic compiler adapter regressions; requires configured Clang, no game assets."""
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
import compiler_facts as adapter


@unittest.skipUnless(shutil.which(os.environ.get("CLANG", "clang")), "Clang not installed")
class CompilerAdapterTests(unittest.TestCase):
    def test_tag_typedef_labels_and_individual_comma_declarators(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            (root/'p.c').write_text('typedef struct RECT { int x; } RECT; extern void draw(RECT *); extern int a(void), *object, b(int), c(void); void run(int n, RECT *r) { switch(n) { case 1: draw(r); break; default: draw(r); } }')
            config=dict(schemaVersion=1,compiler=os.environ.get('CLANG','clang'),target='wasm32-unknown-unknown',flags=['-std=gnu89'],units=[dict(id='p',source='p.c')])
            path=root/'config.json';path.write_text(json.dumps(config));self.assertEqual(adapter.produce(path,root/'out/facts.json'),0)
            facts=json.loads((root/'out/facts.json').read_bytes());prepared=(root/'out/facts.artifacts/p.prepared.i').read_bytes()
            self.assertEqual([c['resultConsumed'] for c in facts['calls']],[False,False])
            self.assertEqual(next(d for d in facts['definitions'] if d['name']=='draw')['parameters'][0]['canonical'],'struct RECT *')
            for d in facts['definitions']:
                span=d['nameSpan'];self.assertEqual(prepared[span['start']:span['end']],d['name'].encode())
            declarations=[d for d in facts['definitions'] if d['name'] in ('a','b','c')]
            self.assertEqual(len(declarations),3);self.assertEqual(len({d['declarationGroup'] for d in declarations}),1)
            self.assertIn(b'*object',prepared)
        aliases={'RECT':'struct RECT','P':'RECT *'};macros={'__SIZEOF_POINTER__':'4'}
        for text in ['RECT *','struct RECT *','P']:
            canonical=adapter.compiler_type(dict(qualType=text),macros,aliases)['canonical'];self.assertEqual(canonical,'struct RECT *')
            self.assertEqual(adapter.compiler_type(dict(qualType=canonical),macros,aliases)['canonical'],canonical)

    def produce(self, root, units=None, compiler=None):
        fixture = ROOT / "tests/fixtures/contracts"
        for name in ("caller.c", "provider.c"):
            shutil.copyfile(fixture / name, root / name)
        config = json.loads((fixture / "config.json").read_text())
        config["compiler"] = compiler or os.environ.get("CLANG", "clang")
        if units is not None:
            config["units"] = units
        path = root / "config.json"
        path.write_text(json.dumps(config))
        status = adapter.produce(path, root / "out/facts.json")
        return status, json.loads((root / "out/facts.json").read_text())

    def test_intrinsic_traps_and_compiler_storage_are_named_and_source_bound(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/"trap.c").write_text("extern void sink(void *); int guarded(int n) { char local[12]; sink(local); if (!n) __builtin_trap(); return n; }")
            c=dict(schemaVersion=1,compiler=os.environ.get("CLANG","clang"),target="wasm32-unknown-unknown",flags=["-O1"],units=[dict(id="trap",source="trap.c")],storageFacts=True)
            config=root/"config.json";config.write_text(json.dumps(c));self.assertEqual(adapter.produce(config,root/"out/facts.json"),0)
            f=json.loads((root/"out/facts.json").read_bytes());self.assertFalse(f["gaps"])
            self.assertEqual(f["intrinsics"][0]["name"],"__builtin_trap");self.assertEqual(f["intrinsics"][0]["effect"],"trap")
            storage=next(i for i in f["compilerStorage"] if i["name"]=="guarded")
            self.assertEqual(storage["allocations"][0]["bytes"],"0xc");self.assertTrue(storage["lifetimes"])
            self.assertTrue(any(a["id"]=="trap:llvm-ir" for a in f["evidence"]["artifacts"]))

    def test_intrinsics_and_storage_survive_warm_semantic_flag_cache(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/"trap.c").write_text("extern void sink(void *); int guarded(int n) { char local[12]; sink(local); if (!n) __builtin_trap(); return n; }")
            flags=["-O1","-w","-fcommon","-fwrapv","-fno-strict-aliasing","-fno-delete-null-pointer-checks"]
            c=dict(schemaVersion=1,compiler=os.environ.get("CLANG","clang"),target="wasm32-unknown-unknown",flags=flags,cache="cache",units=[dict(id="trap",source="trap.c")],storageFacts=True)
            config=root/"config.json";config.write_text(json.dumps(c))
            def build():
                self.assertEqual(adapter.produce(config,root/"out/facts.json"),0)
                return json.loads((root/"out/facts.artifacts/cache-report.json").read_bytes())
            cold=build();facts=(root/"out/facts.json").read_bytes();ir=(root/"out/facts.artifacts/trap.llvm.ll").read_bytes();warm=build()
            self.assertEqual(cold["statistics"]["extractionSubprocesses"],2)
            self.assertEqual(warm["statistics"]["extractionSubprocesses"],0);self.assertEqual(warm["statistics"]["cacheHits"],1)
            self.assertEqual(facts,(root/"out/facts.json").read_bytes());self.assertEqual(ir,(root/"out/facts.artifacts/trap.llvm.ll").read_bytes())
            decoded=json.loads(facts);self.assertEqual(decoded["intrinsics"][0]["effect"],"trap");self.assertTrue(decoded["compilerStorage"])


    def test_definitions_spans_promotions_casts_and_discarded_results(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            status, f = self.produce(root)
            self.assertEqual(status, 0)
            self.assertEqual(len(f["calls"]), 11)
            self.assertEqual([c["resultConsumed"] for c in f["calls"] if c["callee"] == "no_result"], [True, False, True, False])
            cast = next(c for c in f["calls"] if c["callee"] == "narrow")
            self.assertEqual(cast["arguments"][0]["promoted"]["canonical"], "unsigned short")
            self.assertEqual(cast["arguments"][0]["promoted"]["spelling"], "word")
            groups = [d for d in f["definitions"] if d["unit"] == "caller" and d["name"] in ("a", "b", "c")]
            self.assertEqual(len({d["declarationGroup"] for d in groups}), 1)
            self.assertEqual(len({d["id"] for d in groups}), 3)
            old = next(d for d in f["definitions"] if d["name"] == "old_style")
            self.assertTrue(old["hasBody"])
            self.assertEqual(old["parameters"][0]["bits"], 16)
            self.assertEqual(old["abiParameters"][0]["bits"], 32)
            pointers = next(d for d in f["definitions"] if d["name"] == "pointers")
            self.assertEqual(pointers["returnType"]["canonical"], "int **")
            self.assertEqual(len(f["addressReferences"]), 1)
            for c in f["calls"]:
                prepared = root / "out/facts.artifacts" / (c["unit"] + ".prepared.i")
                span = c["span"]
                self.assertIn(c["callee"].encode(), prepared.read_bytes()[span["start"]:span["end"]])

    def test_missing_source_is_an_explicit_gap_with_partial_coverage(self):
        with tempfile.TemporaryDirectory() as tmp:
            status, f = self.produce(Path(tmp), [dict(id="caller", source="caller.c"), dict(id="missing", source="absent.c")])
            self.assertEqual(status, 1)
            self.assertEqual(len(f["calls"]), 11)
            self.assertEqual(f["units"][1]["status"], "failed")
            self.assertTrue(any(g["unit"] == "missing" for g in f["gaps"]))

    def test_target_plain_char_long_widths_and_unknown_types(self):
        macros = {"__CHAR_BIT__": "8", "__SIZEOF_INT__": "4", "__SIZEOF_LONG__": "8", "__SIZEOF_SHORT__": "2", "__SIZEOF_POINTER__": "8", "__CHAR_UNSIGNED__": "1"}
        self.assertFalse(adapter.compiler_type(dict(qualType="char"), macros)["signed"])
        self.assertEqual(adapter.compiler_type(dict(qualType="long"), macros)["bits"], 64)
        self.assertEqual(adapter.compiler_type(dict(qualType="struct S"), macros)["category"], "unsupported")

    def test_unavailable_compiler_records_every_unexamined_unit(self):
        with tempfile.TemporaryDirectory() as tmp:
            status, f = self.produce(Path(tmp), compiler="compiler-does-not-exist")
            self.assertEqual(status, 1)
            self.assertEqual(len(f["gaps"]), 2)
            self.assertEqual(f["calls"], [])
            self.assertTrue(all(u["status"] == "failed" for u in f["units"]))
            self.assertEqual(f["compiler"]["profile"], "unsupported")

    def test_extraction_cache_warm_leaf_corruption_and_header_shadow(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "first").mkdir()
            (root / "second").mkdir()
            (root / "second/types.h").write_text("typedef int word;\n")
            (root / "a.c").write_text('#include "types.h"\nword f(word a) { return a; }\n')
            (root / "b.c").write_text("int b(int x) { return x + 1; }\n")
            config = dict(schemaVersion=1, compiler=os.environ.get("CLANG", "clang"), target="mipsel-none-elf",
                          flags=["-std=gnu89", "-Ifirst", "-Isecond"], cache="cache",
                          units=[dict(id="a", source="a.c"), dict(id="b", source="b.c")])
            cfg = root / "config.json"
            cfg.write_text(json.dumps(config))

            def build():
                self.assertEqual(adapter.produce(cfg, root / "out/facts.json"), 0)
                return json.loads((root / "out/facts.artifacts/cache-report.json").read_bytes())

            cold = build()
            before = (root / "out/facts.json").read_bytes()
            warm = build()
            self.assertEqual((cold["statistics"]["extractionSubprocesses"], warm["statistics"]["extractionSubprocesses"]), (2, 0))
            self.assertEqual(warm["statistics"]["cacheHits"], 2)
            self.assertEqual(before, (root / "out/facts.json").read_bytes())
            (root / "a.c").write_text('#include "types.h"\nword f(word a) { return a + 2; }\n')
            leaf = build()
            self.assertEqual([u["state"] for u in leaf["units"]], ["extracted", "cached"])
            cache_path = root / "cache" / (leaf["units"][0]["key"] + ".json")
            damaged = json.loads(cache_path.read_bytes())
            damaged["facts"]["definitions"] = []
            cache_path.write_text(json.dumps(damaged))
            repaired = build()
            self.assertEqual([u["state"] for u in repaired["units"]], ["extracted", "cached"])
            self.assertIn("corrupt", repaired["units"][0]["reason"])
            (root / "first/types.h").write_text("typedef unsigned int word;\n")
            shadow = build()
            self.assertEqual([u["state"] for u in shadow["units"]], ["extracted", "cached"])
            facts = json.loads((root / "out/facts.json").read_bytes())
            self.assertFalse(next(d for d in facts["definitions"] if d["name"] == "f")["returnType"]["signed"])
            with patch.dict(os.environ, {"BINVIZ_CACHE_TEST": "changed"}):
                changed_env = build()
            self.assertEqual(changed_env["statistics"]["extractionSubprocesses"], 2)
            self.assertFalse(adapter.cacheable_flags(["@arguments.rsp"]))
            self.assertFalse(adapter.cacheable_flags(["-Xclang", "-load", "plugin.so"]))


class CompilerCacheValidationTests(unittest.TestCase):
    def test_optional_cache_payloads_are_validated_without_dropping_effects(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/"entry.json";inputs=dict(unit="trap")
            base=dict(definitions=[],calls=[],addressReferences=[],gaps=[])
            def read(payload):
                path.write_bytes(adapter.json_bytes(dict(inputs=inputs,sha256=adapter.digest(adapter.json_bytes(payload)),facts=payload)))
                return adapter.read_cached_facts(path,inputs)
            effect=dict(name="__builtin_trap",effect="trap")
            for payload in [base,{**base,"intrinsics":[effect]},{**base,"compilerStorage":[],"storageIr":"define void @f() {}"}]:
                self.assertEqual(read(payload),payload)
            for payload in [{**base,"intrinsics":{}},{**base,"compilerStorage":[]},{**base,"storageIr":"bad"},{**base,"compilerStorage":[],"storageIr":[]},{**base,"unknownEffects":[]}]:
                self.assertIsNone(read(payload))

    def test_semantic_flags_remain_explicit_and_hidden_inputs_refuse(self):
        self.assertTrue(adapter.cacheable_flags(["-w","-fcommon","-fwrapv","-fno-strict-aliasing","-fno-delete-null-pointer-checks"]))
        self.assertFalse(adapter.cacheable_flags(["-fplugin=hidden.so"]))
        self.assertFalse(adapter.cacheable_flags(["-Xclang","-load","plugin.so"]))
        self.assertFalse(adapter.cacheable_flags(["@hidden.rsp"]))


if __name__ == "__main__":
    unittest.main()
