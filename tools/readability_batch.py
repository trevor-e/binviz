"""One configured readability build/acceptance run; game-owned candidates only.

python tools/readability_batch.py CONFIG.json OUTPUT_DIR
Config: schemaVersion=1, buildConfig, workspace, binviz. Stages are ordinary
build-batch stages with compilerVisibleNamespace, inputSlots and inputNames.
The workspace contains exact readabilityBatches requests and pinned inputs.
"""
import json,subprocess,sys
from pathlib import Path
from build_batch import Batch
def run(config,out):
    path=Path(config).resolve();c=json.loads(path.read_bytes());root=path.parent;out=Path(out).resolve();out.mkdir(parents=True,exist_ok=True)
    if c.get("schemaVersion")!=1:raise ValueError("readability batch requires schemaVersion 1")
    report=out/"build-report.json"
    status=Batch(root/c["buildConfig"],report).run()
    if status:return status
    p=subprocess.run([str((root/c["binviz"]).resolve()),"workspace",str((root/c["workspace"]).resolve()),"--root",str(root),"--build-report",str(report),"--readability","--json"],capture_output=True,text=True,encoding="utf-8",timeout=120)
    if p.returncode:raise ValueError(p.stderr or p.stdout)
    decisions=json.loads(p.stdout);(out/"acceptance.json").write_text(json.dumps(decisions,indent=2),encoding="utf-8")
    return 0 if decisions and all(d["state"]=="ready-for-review" for d in decisions) else 1
if __name__=="__main__":
    try:sys.exit(run(*sys.argv[1:]))
    except (OSError,ValueError,KeyError,subprocess.TimeoutExpired) as e:print(str(e),file=sys.stderr);sys.exit(1)
