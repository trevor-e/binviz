"""Persistent configured runner pairs; actual execution records, no emulator.

Usage: python tools/proof_campaign.py CONFIG.json REPORT.json
Each runner accepts JSON lines: init (frozen artifact paths), case, stop.
It replies with ready/version/profile, then {id, execution} per requested case.
"""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
import time

VERSION="binviz-proof-campaign-1"
def encode(v):return json.dumps(v,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()
def digest(b):return hashlib.sha256(b).hexdigest()
def atomic(path,raw):
    path.parent.mkdir(parents=True,exist_ok=True);fd,tmp=tempfile.mkstemp(prefix=".binviz-",dir=path.parent)
    try:
        with os.fdopen(fd,"wb") as f:f.write(raw);f.flush();os.fsync(f.fileno())
        os.replace(tmp,path)
    finally:
        if os.path.exists(tmp):os.unlink(tmp)

class Runner:
    def __init__(self,config,root,artifacts,timeout,cancel=None):
        command=[]
        for arg in config["command"]:
            for id,path in artifacts.items():arg=arg.replace("{artifact:"+id+"}",path)
            command.append(arg)
        executable=shutil.which(command[0]) or str((root/command[0]).resolve());command[0]=executable
        self.stderr=tempfile.TemporaryFile();self.timeout=timeout;self.cancel=cancel;self.lines=queue.Queue(maxsize=2)
        self.proc=subprocess.Popen(command,cwd=root,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.stderr,text=True,encoding="utf-8",bufsize=1,env={**os.environ,**config.get("environment",{})})
        def read():
            try:
                while True:
                    line=self.proc.stdout.readline(16*1024*1024+1)
                    if not line:break
                    self.lines.put(line)
                    if len(line)>16*1024*1024:break
            finally:
                try:self.lines.put(None,timeout=1)
                except queue.Full:pass
        self.thread=threading.Thread(target=read,daemon=True);self.thread.start()
        try:
            providers=config.get('nativeProviders',{})
            for name,p in providers.items():
                if p.get('kind')!='cop2' or p.get('authority') not in ('software-model','hardware-oracle') or any(p.get(k) not in artifacts for k in ('artifact','profileArtifact','sourceArtifact','compilerArtifact')) or p.get('operations')!=['read-data','write-data','read-control','write-control','command','load','store']:
                    raise ValueError('native provider needs pinned backend/source/compiler/profile and a complete COP2 callback interface')
            self.hello=self.request(dict(op="init",schemaVersion=1,artifacts=artifacts,profile=config.get("profile",{}),nativeProviders=providers))
            if self.hello.get("ready") is not True or not self.hello.get("version") or not self.hello.get("architecture"):raise ValueError("runner must initialize with version and architecture")
            if providers and self.hello.get('nativeProviders')!=providers:raise ValueError('runner did not install the exact selected native providers')
        except (OSError,ValueError):self.close();raise
    def request(self,message):
        self.proc.stdin.write(json.dumps(message)+"\n");self.proc.stdin.flush()
        deadline=time.monotonic()+self.timeout
        while True:
            if self.cancel is not None and self.cancel.is_set():self.proc.kill();raise ValueError("campaign cancelled")
            try:line=self.lines.get(timeout=min(.1,max(.001,deadline-time.monotonic())));break
            except queue.Empty:
                if time.monotonic()>=deadline:self.proc.kill();raise ValueError("bounded runner timeout")
        if line is None:raise ValueError("runner stopped without a result")
        if len(line)>16*1024*1024:raise ValueError("runner observation exceeds 16 MiB")
        return json.loads(line)
    def close(self):
        if self.proc.poll() is None:
            try:self.proc.stdin.write('{"op":"stop"}\n');self.proc.stdin.flush();self.proc.wait(timeout=2)
            except (OSError,subprocess.TimeoutExpired):self.proc.kill();self.proc.wait()
        self.proc.stdin.close();self.proc.stdout.close();self.thread.join(timeout=2);self.stderr.close()

def run(config_path,report_path):
    path=Path(config_path).resolve();root=path.parent;report_path=Path(report_path).resolve();c=json.loads(path.read_bytes())
    if c.get("schemaVersion")!=1 or not c.get("cases") or not c.get("comparison"):raise ValueError("campaign requires schemaVersion 1, cases and comparison")
    if len({case["id"] for case in c["cases"]})!=len(c["cases"]):raise ValueError("duplicate case id")
    roles=("baseline","candidate") if "baseline" in c else ("native","wasm")
    if any(k not in c for k in roles):raise ValueError("campaign requires both configured participants")
    workers=int(c.get("workers",1));budget=int(c.get("memoryMb",512));cost=sum(int(c[k].get("memoryMb",128)) for k in roles)
    if workers<1 or cost<1 or cost>budget:raise ValueError("runner pair exceeds the configured memory budget")
    workers=min(workers,budget//cost,len(c["cases"]));timeout=float(c.get("timeoutSeconds",10))
    if timeout<=0:raise ValueError("positive runner timeout required")
    cancel=threading.Event()
    artifacts=[];frozen={};originals={};results={};hellos={};lock=threading.RLock();started=time.monotonic()
    report_path.parent.mkdir(parents=True,exist_ok=True)
    scratch=Path(tempfile.mkdtemp(prefix=".binviz-campaign-",dir=report_path.parent))
    def add(id,role,p):
        p=p.resolve();raw=p.read_bytes();sha=digest(raw);originals[p]=sha;artifacts.append(dict(id=id,role=role,location=str(p),sha256=sha));return raw,sha
    raw,_=add("campaign-config","recipe",path)
    for id,name in c.get("artifacts",{}).items():
        p=(root/name).resolve();raw,sha=add(id,"campaign-input",p);dest=scratch/(sha[:16]+"-"+p.name);atomic(dest,raw);frozen[id]=str(dest)
    for kind in roles:
        spec=c[kind]
        executable=shutil.which(spec["command"][0]) or str((root/spec["command"][0]).resolve());add(kind+":runner","runner",Path(executable))
        for i,tool in enumerate(spec.get("tools",[])):add(kind+":tool:"+str(i),"runner-tool",Path(shutil.which(tool) or str((root/tool).resolve())))
    def save():
        observed=dict(comparison=c["comparison"],cases=[results[x["id"]] for x in c["cases"] if x["id"] in results],runners=hellos,requested=len(c["cases"]),unexamined=len(c["cases"])-len(results),configuration=c)
        observations_path=report_path.with_name(report_path.stem+".observations.json");bytes=encode(observed);atomic(observations_path,bytes)
        evidence=dict(artifacts=[*artifacts,dict(id="campaign-observations",role="execution-observations",location=str(observations_path),sha256=digest(bytes))],stages=[dict(id="campaign-execution",parents=[a["id"] for a in artifacts if a["id"]!="campaign-config"],outputs=["campaign-observations"],recipe="campaign-config",result="success" if len(results)==len(c["cases"]) else "partial")])
        atomic(report_path,encode(dict(format="binviz-campaign",schemaVersion=1,adapter=VERSION,evidence=evidence,**observed,statistics=dict(wallSeconds=time.monotonic()-started,workers=workers,scheduledMemoryMb=workers*cost,runnerProcesses=len(hellos)*2))))
    def worker(index,cases):
        n=w=None
        try:
            n=Runner(c[roles[0]],root,frozen,timeout,cancel);w=Runner(c[roles[1]],root,frozen,timeout,cancel)
            with lock:hellos[str(index)]={roles[0]:n.hello,roles[1]:w.hello}
            for case in cases:
                if cancel.is_set():break
                result=dict(id=case["id"],fixture=case)
                try:
                    for kind,runner in zip(roles,(n,w)):
                        reply=runner.request(dict(op="case",**case))
                        if reply.get("id")!=case["id"] or "execution" not in reply:raise ValueError("runner response id/record differs")
                        execution=reply["execution"]
                        identity=dict(role=kind,architecture=runner.hello["architecture"],runnerKind=runner.hello.get("runnerKind",runner.hello["version"]))
                        if execution.get("participant",identity)!=identity:raise ValueError("execution identity differs from initialized runner")
                        execution["participant"]=identity;result[kind]=execution
                        for name,provider in c[kind].get('nativeProviders',{}).items():
                            state=execution.get('providerState',{}).get(name)
                            if not isinstance(state,dict) or state.get('authority')!=provider['authority'] or 'data' not in state or 'control' not in state:raise ValueError('native provider final state/authority missing')
                except (OSError,ValueError,BrokenPipeError) as e:result["runnerFailure"]=str(e)
                with lock:
                    results[case["id"]]=result;save();print(json.dumps(dict(stage="campaign-case",case=case["id"],processed=len(results),total=len(c["cases"]),elapsedSeconds=time.monotonic()-started)),file=sys.stderr,flush=True)
        except (OSError,ValueError) as e:
            with lock:
                if not cancel.is_set():
                    for case in cases:
                        results.setdefault(case["id"],dict(id=case["id"],fixture=case,runnerFailure=str(e)))
                save()
        finally:
            if n:n.close()
            if w:w.close()
    try:
        with ThreadPoolExecutor(workers) as pool:
            futures=[pool.submit(worker,i,c["cases"][i::workers]) for i in range(workers)]
            try:
                for future in futures:future.result()
            except KeyboardInterrupt:
                cancel.set()
                for future in futures:future.result()
        changed=[str(p) for p,sha in originals.items() if not p.exists() or digest(p.read_bytes())!=sha]
        if changed:
            for r in results.values():r["runnerFailure"]="campaign inputs changed during execution: "+", ".join(changed)
        save()
        return 1 if cancel.is_set() or any("runnerFailure" in r for r in results.values()) else 0
    finally:
        resolved=scratch.resolve();parent=report_path.parent.resolve()
        if parent in resolved.parents and not scratch.is_symlink():shutil.rmtree(scratch)

if __name__=="__main__":
    try:
        if len(sys.argv)!=3:raise ValueError(__doc__)
        sys.exit(run(sys.argv[1],sys.argv[2]))
    except (OSError,ValueError,KeyError) as e:print(str(e),file=sys.stderr);sys.exit(1)
