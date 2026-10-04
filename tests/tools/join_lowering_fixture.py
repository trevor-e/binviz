"""Join actual compiler/linker/service records with finite reviewed native fixtures."""
import hashlib,json,shutil,struct,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];F=ROOT/'tests/fixtures/lowerings';sys.path.insert(0,str(ROOT/'tools'));from proof_campaign import run
def encode(x):return json.dumps(x,sort_keys=True,separators=(',',':')).encode()
def sha(b):return hashlib.sha256(b).hexdigest()
def main():
    cli=ROOT/'target/debug/binviz.exe';facts=json.loads((F/'facts.json').read_bytes());e=json.loads(json.dumps(facts['evidence']));m=json.loads(subprocess.check_output([cli,'linked',F/'linked.wasm']))
    def artifact(id,role,name,raw=None):
        p=F/name
        if raw is not None:p.write_bytes(raw)
        e['artifacts'].append(dict(id=id,role=role,location=name,sha256=sha(p.read_bytes())))
    artifact('lowered-module','linked-module','linked.wasm');artifact('selected-object','wasm-object','providers.o');artifact('candidate','source','candidate.c');artifact('build-recipe','recipe','recipe.json',encode(dict(compiler=facts['compiler'],flags=['-O0','-nostdlib','-fno-inline'],selected=['selected_gpu','selected_vblank'])))
    e['stages'].extend([dict(id='selected-compile',parents=['providers:prepared','compiler'],outputs=['selected-object'],recipe='build-recipe',result='success'),dict(id='selected-link',parents=['selected-object','candidate','compiler'],outputs=['lowered-module'],recipe='build-recipe',result='success')])
    units=[];bindings=[];policies=[]
    specs=[('frame',0x80010000,[0x0c008000,0,0x24020000,0x03e00008,0]),('poll',0x80010000,[0x0c008004,0,0x03e00008,0]),('gpu',0x80020000,[0x03e00008,0x24020001,0,0]),('vblank',0x80020010,[0x03e00008,0x24020001,0,0])]
    names=dict(frame='frame',poll='poll',gpu='selected_gpu',vblank='selected_vblank')
    for id,start,words in specs:
        raw=struct.pack('<'+'I'*len(words),*words)
        for suffix,role in [('asset','original'),('member','member'),('native','native')]:artifact(id+':'+suffix,role,id+'.bin',raw if suffix=='asset' else None)
        extent=dict(start=hex(start),bytes=hex(len(raw)),exact=True)
        units.append(dict(identity=dict(id=id,asset=id+':asset',memberOffset='0x0',memberSize=hex(len(raw)),loadAddress=hex(start),context=id),memberArtifact=id+':member',architecture='ps1-mipsel',functions=[dict(id=id+':'+names[id],name=names[id],identity=dict(unit=id,entry=hex(start),role='primary',analysisExtent=extent,matchingExtent=extent),analysisArtifact=id+':native',matchingArtifact=None,owner=None,aliases=[],exclusions=[])],gaps=[]))
    for caller,provider,kind in [('frame','gpu','discarded-result'),('poll','vblank','direct-binding')]:
        call=next(c for c in facts['calls'] if c['callee']==('legacy_draw' if caller=='frame' else 'legacy_vblank'))
        definition=next(d for d in facts['definitions'] if d['hasBody'] and d['name']==names[provider]);linked=next(f for f in m['functions'] if caller in f['exports'])
        binding=dict(call=call['id'],caller=caller+':'+caller,provider=provider+':'+names[provider],definition=definition['id'],providerKind='reconstructed-c',originalPc='0x80010000',originalWord=hex(specs[0 if caller=='frame' else 1][2][0]),delayWord='0x0',module='lowered-module',object='selected-object',callerExport=caller,providerExport=names[provider],linkedCallOffset=linked['calls'][0]['offset'],correspondenceArtifact=caller+':correspondence')
        artifact(binding['correspondenceArtifact'],'reviewed-correspondence',caller+'.correspondence.json',encode(binding));bindings.append(binding)
        p=dict(id=kind,kind=kind,allowedCallers=[binding['caller']],allowedSites=[call['id']],provider=binding['provider'],definition=definition['id'],expectedCounts={call['id']:1},registers=[2] if caller=='frame' else [],dependencies=[a['id'] for a in e['artifacts']],reviewArtifact=caller+':policy',guard=None,calleeCertificates=[],closureCertificates=['vblank-service'] if caller=='poll' else [])
        policies.append(p)
    # Actual controlled service execution; kept separate from native device authority.
    shutil.copyfile(ROOT/'tests/fixtures/adoption/service_runner.mjs',F/'service_runner.mjs')
    spec=dict(command=[shutil.which('node'),'{artifact:service-runner}'],memoryMb=32,profile=dict(moduleArtifact='lowered-module'))
    config=dict(schemaVersion=1,workers=1,memoryMb=64,artifacts={'lowered-module':'linked.wasm','service-runner':'service_runner.mjs'},baseline=spec,candidate=spec,comparison=dict(returnMask='0xffffffff',ram=[],registers=[],eventKinds=['call','device','return'],requiredCheckpoints=['after-service'],expectedFrontiers=[]),cases=[dict(id='service-zero',arguments=['0x0']),dict(id='service-wrap',arguments=['0xffffffff'])]);(F/'service-config.json').write_bytes(encode(config));assert run(F/'service-config.json',F/'service-campaign.json')==0
    campaign=json.loads((F/'service-campaign.json').read_bytes())
    # Retain the exact shared campaign graph, avoiding duplicate module records.
    ids={a['id'] for a in e['artifacts']}
    for a in campaign['evidence']['artifacts']:
        if a['id'] not in ids:e['artifacts'].append(a);ids.add(a['id'])
    e['stages'].extend(campaign['evidence']['stages']);artifact('service-campaign','campaign','service-campaign.json')
    selected=next(f for f in m['functions'] if 'selected_vblank' in f['exports']);service=m['functions'][selected['calls'][0]['target']]['importIdentity']
    closure=dict(id='vblank-service',dependencies={a['id']:a['sha256'] for a in e['artifacts']},reviewArtifact='service-review',claim=dict(kind='service-chain',module='lowered-module',root=selected['index'],edges=[dict(caller=selected['index'],offset=selected['calls'][0]['offset'],target=selected['calls'][0]['target'])],service=service,obligations=[dict(campaign='service-campaign',cases=['service-zero','service-wrap'],checkpoint='after-service')]))
    artifact('service-review','reviewed-proof','service-review.json',encode(closure))
    for p in policies:
        p['dependencies']=[a['id'] for a in e['artifacts']];p.pop('calleeCertificates',None);p.pop('closureCertificates',None) if not p.get('closureCertificates') else None;artifact(p['reviewArtifact'],'reviewed-policy',p['kind']+'.json',encode(p))
    w=dict(format='binviz-workspace',schemaVersion=1,evidence=e,inventory=dict(schemaVersion=1,units=units,decisions=[]),compilerFacts=facts,bindings=bindings,policies=policies,applications=[],builds=[],layouts=[],proofClosures=[closure]);(F/'workspace.json').write_bytes(encode(w))
if __name__=='__main__':main()
