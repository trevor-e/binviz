"""Materialize a verified immutable overlay into a fresh owned output directory.
Usage: python tools/snapshot_overlay.py WORKSPACE.json REQUEST_ID OUT_DIR
The CLI's adoption report is recomputed before any output is created.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import hashlib

def export(workspace,request_id,out,executable):
    workspace=Path(workspace).resolve();out=Path(out).resolve()
    report=json.loads(subprocess.check_output([executable,'workspace',str(workspace),'--adoption','--json']))
    selected=[r for r in report if r['id']==request_id]
    if len(selected)!=1 or selected[0]['state']!='verified' or selected[0]['request']['claim']['kind']!='snapshot-overlay':raise ValueError('overlay is not independently verified')
    request=selected[0]['request'];w=json.loads(workspace.read_bytes());artifacts={a['id']:a for a in w['evidence']['artifacts']}
    members=[]
    for m in selected[0]['details']['members']:
        if m['path']=='.binviz-overlay.json':raise ValueError('snapshot member conflicts with output ownership record')
        key=m['selectedArtifact'];a=artifacts[key];raw=(workspace.parent/a['location']).read_bytes()
        if hashlib.sha256(raw).hexdigest()!=request['dependencies'][key]:raise ValueError('overlay bytes drifted after verification')
        members.append((m['path'],raw))
    for a in artifacts.values():
        source=(workspace.parent/a['location']).resolve()
        if out==source or source in out.parents or out in source.parents:raise ValueError('overlay output overlaps a declared input location')
    out.mkdir(parents=False,exist_ok=False)
    for name,raw in members:
        dest=out/name;dest.parent.mkdir(parents=True,exist_ok=True)
        with dest.open('xb') as f:f.write(raw)
    with (out/'.binviz-overlay.json').open('x',encoding='utf-8') as f:json.dump(selected[0],f,indent=2)
    return out
if __name__=='__main__':
    if len(sys.argv)!=4:raise SystemExit(__doc__)
    print(export(*sys.argv[1:],os.environ.get('BINVIZ','binviz')))
