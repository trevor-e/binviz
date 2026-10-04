"""Actual compiled callback backend; no second emulator or decoder."""
import hashlib,json,os,sys,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'tools'));from native_provider import Cop2Provider,OPERATIONS
@unittest.skipUnless(os.name=='nt','compiled Windows callback DLL fixture')
class ProviderTests(unittest.TestCase):
    def test_complete_callbacks_trace_budget_unknown_and_pins(self):
        f=ROOT/'tests/fixtures/adoption';backend=f/'callback.dll'
        self.assertTrue(backend.exists(),'regenerate the actual offline compiler fixture')
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);compiler=root/'compiler';compiler.write_bytes(b'test identity, not compiler executable')
            selection=dict(kind='cop2',authority='software-model',artifact='backend',sourceArtifact='source',compilerArtifact='compiler',profileArtifact='profile',operations=OPERATIONS)
            artifacts=dict(backend=str(backend),source=str(f/'callback.c'),compiler=str(compiler),profile=str(root/'profile.json'))
            profile=dict(selection=selection,sha256={k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in artifacts.items() if k!='profile'},commands=[1],operationBudget=10)
            Path(artifacts['profile']).write_text(json.dumps(profile));p=Cop2Provider(selection,artifacts);p.reset([0]*32,[0]*32)
            class ExistingExecutor:
                def set_cop2_provider(self,callbacks):self.callbacks=callbacks;return list(callbacks)
            cpu=ExistingExecutor();self.assertEqual(list(p.install(cpu)),OPERATIONS)
            p.write_data(0,4,0x100);p.write_control(0,3,0x104);self.assertEqual(p.read_data(0,0x108),4);self.assertEqual(p.read_control(0,0x10c),3);p.command(1,0x110,True)
            ram={0x200:12};p.load(1,0x200,lambda a:ram[a],0x114);p.store(0,0x204,lambda a,v:ram.__setitem__(a,v),0x118)
            self.assertEqual(ram[0x204],7);self.assertEqual(p.final_state()['data'][:2],[7,12]);self.assertTrue(p.events[4]['delaySlot']);self.assertEqual(len(p.events),7)
            with self.assertRaisesRegex(ValueError,'unknown'):p.command(2,0x120)
            with self.assertRaisesRegex(ValueError,'injection'):p.install(object())
            p.budget=7
            with self.assertRaisesRegex(ValueError,'budget'):p.read_data(0,0x124)
            profile['sha256']['source']='0'*64;Path(artifacts['profile']).write_text(json.dumps(profile))
            with self.assertRaisesRegex(ValueError,'drift'):Cop2Provider(selection,artifacts)
if __name__=='__main__':unittest.main()
