"""Real void WASM calls and compiler/linker profiles; effect negative controls."""
import hashlib,json,shutil,subprocess,sys,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'tools'));from proof_campaign import run
@unittest.skipUnless(shutil.which('node'),'Node required for actual WASM execution')
class VoidCampaignTests(unittest.TestCase):
    def test_executed_void_effects_and_word_getter_refusal(self):
        f=ROOT/'tests/fixtures/adoption';compiler=ROOT/'target/adoption-clang.bin'
        self.assertTrue(compiler.exists(),'copy the compiler identity during fixture build')
        for variant in ['good','wrong-ram','extra-event','fake-zero','word-profile']:
            with self.subTest(variant=variant),tempfile.TemporaryDirectory() as tmp:
                root=Path(tmp);facts=json.loads((f/'facts.json').read_bytes());build=json.loads((f/'build-evidence.json').read_bytes())
                artifacts={'void-module':str(f/'void.wasm'),'void-facts':str(f/'facts.json'),'build-evidence':str(f/'build-evidence.json'),'void-runner':str(f/'void_runner.mjs')}
                for a in build['artifacts']:
                    artifacts[a['id']]=str(compiler) if a['id']=='compiler' else str(ROOT/Path(a['location']).relative_to('/workspace')) if a['location'].startswith('/workspace/') else str(f/a['location'])
                for a in facts['evidence']['artifacts']:
                    artifacts[a['id']]=str(compiler) if a['id']=='compiler' else str(ROOT/Path(a['location']).relative_to('/workspace')) if a['location'].startswith('/workspace/') else str(f/a['location'])
                definition=next(d for d in facts['definitions'] if d['name']==('get_word' if variant=='word-profile' else 'void_fill'))
                actual=json.loads(subprocess.check_output([ROOT/'target/debug/binviz.exe','linked',f/'void.wasm']))
                index=next(x['index'] for x in actual['functions'] if definition['name'] in x['names'])
                profile=dict(compilerFacts='void-facts',definition=definition['id'],module='void-module',function=index,buildEvidence='build-evidence',reviewArtifact='void-review')
                review=root/'review.json';review.write_text(json.dumps(profile));artifacts['void-review']=str(review)
                spec=dict(command=[shutil.which('node'),'{artifact:void-runner}'],memoryMb=32,profile=dict(voidProfile=profile,variant='good'))
                config=dict(schemaVersion=1,workers=1,memoryMb=64,artifacts=artifacts,baseline=spec,candidate={**spec,'profile':dict(voidProfile=profile,variant=variant)},comparison=dict(returnMask='0xffffffff',ram=['state'],registers=[],eventKinds=['call','return','device'],requiredCheckpoints=['after-void'],expectedFrontiers=[],voidProfiles=[profile,profile]),cases=[dict(id='zero',arguments=['0x0']),dict(id='wrap',arguments=['0xffffffff'])])
                path=root/'campaign.json';path.write_text(json.dumps(config));raw=root/'raw.json';self.assertEqual(run(path,raw),0)
                output=root/'compared.json';p=subprocess.run([ROOT/'target/debug/binviz.exe','campaign-compare',raw,'--out',output],capture_output=True,text=True)
                self.assertEqual(p.returncode==0,variant=='good',p.stderr)
                if variant!='word-profile':
                    result=json.loads(output.read_bytes());self.assertEqual(result['observationsBound'],True)
                    if variant=='good':self.assertEqual(result['executedPairs'],2)
                    elif variant=='fake-zero':self.assertEqual(result['refused'],2)
                    else:self.assertEqual(result['failed'],2)
if __name__=='__main__':unittest.main()
