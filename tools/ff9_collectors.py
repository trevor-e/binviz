"""Compare frozen FF9 collectors with shared Binviz backends, without game writes.

python tools/ff9_collectors.py GAME_ROOT BINVIZ_EXE OUTPUT_DIR
Reads the resident geometry register fixture and CD selected-provider modules.
Retains legacy reports. Agreement permits reviewing retirement, never silently retires.
"""
import hashlib,json,subprocess,sys
from pathlib import Path

def sha(raw):return hashlib.sha256(raw).hexdigest()
def invoke(exe,*args):
    p=subprocess.run([str(exe),*map(str,args)],capture_output=True,text=True,encoding="utf-8",timeout=120)
    if p.returncode:raise ValueError(p.stderr or p.stdout)
    return json.loads(p.stdout)
def normalize(report):
    result={k:report[k] for k in ("register","entry","extentStart","extentWords","outcome","states")}
    for category in ("reads","endpoints","frontiers"):
        result[category]=[{k:v for k,v in row.items() if k not in ("endpoint",)} for row in report[category]]
    return result
def compare(game,exe,out):
    game=Path(game).resolve();exe=Path(exe).resolve();out=Path(out).resolve()
    if game==out or game in out.parents:raise ValueError("migration output must be outside the game checkout")
    out.mkdir(parents=True,exist_ok=True);frozen={};checks=[]
    def read(path):
        raw=path.read_bytes();frozen[path]=sha(raw);return raw
    read(exe)
    geometry=game/"build/agent-resident-geometry"
    legacy=json.loads(read(geometry/"register-audits.json"))
    requests=geometry/"register-requests.json";read(requests);ram=geometry/"initial.ram";read(ram)
    current=invoke(exe,"register-use",ram,"--overlay-at","0x80000000","--batch",requests,"--json")
    before={r["id"]:r["report"] for r in legacy["requests"]};after={r["id"]:r["report"] for r in current["requests"]}
    if set(before)!=set(after):raise ValueError("frozen/shared register request scope differs")
    for id in before:checks.append(dict(kind="register-audit",id=id,agrees=normalize(before[id])==normalize(after[id]),legacy=before[id],shared=after[id]))
    (out/"shared-register-audits.json").write_text(json.dumps(current,indent=2),encoding="utf-8")
    cd=game/"build/agent-resident-providers-private/cd-boundary"
    modules=json.loads(read(cd/"compiled-modules.json"));linked={}
    for name,metadata in modules.items():
        path=cd/(name+".wasm");raw=read(path);report=invoke(exe,"linked",path);linked[name]=report
        checks.append(dict(kind="linked-module",id=name,agrees=sha(raw)==metadata["moduleSha256"],actualSha256=report["sha256"],legacySha256=metadata["moduleSha256"]))
    direct=[f for f in linked["provider"]["functions"] if f.get("importIdentity",{} ) and f["importIdentity"]["field"]=="ff9_cd_busy"]
    coexist=[f for f in linked["coexistence"]["functions"] if f.get("importIdentity",{}) and f["importIdentity"]["field"]=="ff9_cd_busy"]
    checks.append(dict(kind="selected-service-signatures",id="ff9_cd_busy",agrees=any(f["parameters"]==[] for f in direct) and {len(f["parameters"]) for f in coexist}>={0,6},direct=direct,coexisting=coexist))
    read(cd/"proof.json");read(cd/"policy.json");read(cd/"production-inputs.json")
    for path,pin in frozen.items():
        if sha(path.read_bytes())!=pin:raise ValueError("frozen collector input drifted: "+str(path))
    report=dict(format="binviz-ff9-migration-report",schemaVersion=1,agrees=all(c["agrees"] for c in checks),checks=checks,inputs=[dict(location=str(p),sha256=h) for p,h in frozen.items()],legacyCollectorsRetained=True,
        scope="Six frozen native register requests, exact selected linked modules and zero/six-word service import signatures. Historical execution counters remain historical; scheduler observations are not replayed or promoted by this comparison.")
    (out/"migration-report.json").write_text(json.dumps(report,indent=2),encoding="utf-8")
    return 0 if report["agrees"] else 1
if __name__=="__main__":
    try:sys.exit(compare(*sys.argv[1:]))
    except (OSError,ValueError,KeyError,subprocess.TimeoutExpired) as e:print(str(e),file=sys.stderr);sys.exit(1)
