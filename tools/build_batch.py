"""Configured, content-addressed build stages. No shell, game builder or compiler parser.

Usage: python tools/build_batch.py CONFIG.json REPORT.json
Commands use {input:ID}, {output:ID}, and {include:N} frozen-path placeholders.
Outputs are published in a separate configured directory, never over input files.
"""
from concurrent.futures import ThreadPoolExecutor, wait, FIRST_COMPLETED
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

VERSION="binviz-build-batch-1"
def encode(v):return json.dumps(v,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()
def digest(b):return hashlib.sha256(b).hexdigest()
def atomic(path,bytes):
    path.parent.mkdir(parents=True,exist_ok=True)
    fd,tmp=tempfile.mkstemp(prefix=".binviz-",dir=path.parent)
    try:
        with os.fdopen(fd,"wb") as f:f.write(bytes);f.flush();os.fsync(f.fileno())
        os.replace(tmp,path)
    finally:
        if os.path.exists(tmp):os.unlink(tmp)

def resident_bytes(pid):
    """Observed direct-process RSS. Descendant processes are explicitly excluded."""
    try:
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            class Counters(ctypes.Structure):
                _fields_=[("cb",wintypes.DWORD),("PageFaultCount",wintypes.DWORD),*[(n,ctypes.c_size_t) for n in ("PeakWorkingSetSize","WorkingSetSize","QuotaPeakPagedPoolUsage","QuotaPagedPoolUsage","QuotaPeakNonPagedPoolUsage","QuotaNonPagedPoolUsage","PagefileUsage","PeakPagefileUsage")]]
            kernel=ctypes.WinDLL("kernel32",use_last_error=True);psapi=ctypes.WinDLL("psapi",use_last_error=True)
            kernel.OpenProcess.argtypes=[wintypes.DWORD,wintypes.BOOL,wintypes.DWORD];kernel.OpenProcess.restype=wintypes.HANDLE
            kernel.CloseHandle.argtypes=[wintypes.HANDLE];psapi.GetProcessMemoryInfo.argtypes=[wintypes.HANDLE,ctypes.POINTER(Counters),wintypes.DWORD]
            handle=kernel.OpenProcess(0x410,False,pid)
            if not handle:return None
            try:
                c=Counters();c.cb=ctypes.sizeof(c)
                return c.WorkingSetSize if psapi.GetProcessMemoryInfo(handle,ctypes.byref(c),c.cb) else None
            finally:kernel.CloseHandle(handle)
        for line in Path(f"/proc/{pid}/status").read_text().splitlines():
            if line.startswith("VmRSS:"):return int(line.split()[1])*1024
    except (OSError,ValueError):pass
    return None

def directory_files(root):
    paths=list(root.rglob("*"))
    if any(p.is_symlink() and p.is_dir() for p in paths):raise ValueError("directory symlinks require an explicit include snapshot")
    return sorted((p for p in paths if p.is_file()),key=lambda p:p.relative_to(root).as_posix())

def relative_path(value):
    p=Path(value)
    if not value or p.is_absolute() or '..' in p.parts or '\\' in value or ':' in value:raise ValueError('unsafe declared execution layout path: '+str(value))
    return p

class Batch:
    def __init__(self,config,report):
        self.config_path=Path(config).resolve();self.root=self.config_path.parent
        self.config=json.loads(self.config_path.read_bytes());self.report_path=Path(report).resolve()
        c=self.config
        if c.get("schemaVersion")!=1:raise ValueError("build batch requires schemaVersion 1")
        self.cpu=int(c.get("workers",1));self.memory=int(c.get("memoryMb",512))
        if self.cpu<1 or self.memory<1:raise ValueError("positive worker and memory limits required")
        self.cache=self.owned_root(c.get("cache",".binviz-stage-cache"));self.output=self.owned_root(c.get("output","binviz-candidates"))
        self.cache.mkdir(parents=True,exist_ok=True);self.output.mkdir(parents=True,exist_ok=True)
        self.stages=c.get("stages",[]);ids={s["id"] for s in self.stages}
        if len(ids)!=len(self.stages) or not ids:raise ValueError("nonempty unique stages required")
        for s in self.stages:
            for value in s.get('inputLayout',{}).values():relative_path(value)
            if set(s.get('inputLayout',{}))-set(s.get('inputs',{})):raise ValueError('execution layout names an undeclared input')
            relative_path(s.get('executionRoot','.'))
            destinations=[relative_path(v).as_posix() for v in s.get('includeLayout',{}).values()]
            for i,a in enumerate(destinations):
                if any(a==b or a.startswith(b+'/') or b.startswith(a+'/') for b in destinations[i+1:]):raise ValueError('overlapping declared directory layouts')
            for key in s.get('includeLayout',{}):
                if not key.isdigit() or int(key)>=len(s.get('includeDirectories',[])):raise ValueError('directory layout needs a declared input snapshot')
            mounts=s.get('mountMappings',[])
            for i,m in enumerate(mounts):
                target=m.get('target','')
                if m.get('source') not in ('execution-root','outputs') or m.get('readOnly') is not (m.get('source')=='execution-root') or not target.startswith('/') or '..' in target.split('/'):raise ValueError('mount must map the frozen execution root read-only or the owned outputs writable')
                for n in mounts[:i]:
                    nested_output=(m['source']=='outputs' and n['source']=='execution-root' and target.startswith(n['target'].rstrip('/')+'/')) or (n['source']=='outputs' and m['source']=='execution-root' and n['target'].startswith(target.rstrip('/')+'/'))
                    if target==n['target'] or ((target.startswith(n['target'].rstrip('/')+'/') or n['target'].startswith(target.rstrip('/')+'/')) and not nested_output):raise ValueError('overlapping execution mounts')
            if not re.fullmatch(r"[A-Za-z0-9_.-]+",s["id"]):raise ValueError("unsafe stage id")
            if s.get("phase") not in ("prepare","extract","analyze","transform","compile","link","validate"):raise ValueError("unknown stage phase")
            if not s.get("outputs") or not s.get("command") or not isinstance(s["command"],list):raise ValueError("outputs and command array required")
            if float(s.get("timeoutSeconds",120))<=0:raise ValueError("positive stage timeout required")
            if int(s.get("memoryMb",128))>self.memory or int(s.get("memoryMb",128))<1:raise ValueError("stage cannot fit memory bound")
            if set(s.get("dependsOn",[]))-ids:raise ValueError("unknown stage dependency")
            if s.get("compilerVisibleNamespace") and not re.fullmatch(r"[A-Za-z0-9_.-]+",s["compilerVisibleNamespace"]):raise ValueError("unsafe compiler-visible namespace")
            for slot in s.get("inputSlots",{}).values():
                if not re.fullmatch(r"[A-Za-z0-9_.-]+",slot):raise ValueError("unsafe compiler-visible input slot")
            for key in [*s.get("inputs",{}),*s["outputs"]]:
                if not re.fullmatch(r"[A-Za-z0-9_.:-]+",key):raise ValueError("unsafe artifact id")
            for name in s["outputs"].values():
                p=Path(name)
                if p.is_absolute() or ".." in p.parts:raise ValueError("output must stay inside its stage directory")
        output_ids=[id for s in self.stages for id in s["outputs"]]
        if len(output_ids)!=len(set(output_ids)):raise ValueError("each output artifact needs exactly one producer")
        remaining={s["id"]:set(s.get("dependsOn",[])) for s in self.stages};done=set()
        while remaining:
            ready=[id for id,deps in remaining.items() if deps<=done]
            if not ready:raise ValueError("cyclic build stages")
            for id in ready:done.add(id);del remaining[id]
        self.cancel=threading.Event();self.lock=threading.RLock();self.started=time.monotonic()
        self.stats=dict(bytesRead=0,subprocesses=0,cacheHits=0,cacheMisses=0,stagingSeconds=0,producerSeconds=0,peakScheduledMemoryMb=0,peakObservedDirectProcessRssBytes=None,rssSamples=0,rssScope="sampled direct children; grandchildren and Binviz/Python memory excluded")
        self.results={};self.blobs={};self.tools={};self.include_roots={};self.running_rss={}
        self.scratch=Path(tempfile.mkdtemp(prefix=".binviz-job-",dir=self.output))
        self.identity=digest(Path(__file__).read_bytes()) if "__file__" in globals() else os.environ["BINVIZ_ADAPTER_SHA256"]
        self.preflight=None
        if c.get('preflight'):
            p=c['preflight'];exe=shutil.which(p['executable']) or str((self.root/p['executable']).resolve());workspace=(self.root/p['workspaceFile']).resolve()
            checked=subprocess.run([exe,'workspace',str(workspace),'--preflight','--json'],capture_output=True,text=True,timeout=float(p.get('timeoutSeconds',30)))
            try:self.preflight=json.loads(checked.stdout)
            except ValueError:self.preflight=dict(state='refused',reason=checked.stderr or 'preflight produced no shared report')
            if checked.returncode or self.preflight.get('state')!='verified':
                self.results={s['id']:dict(id=s['id'],phase=s['phase'],state='refused',failurePhase='preflight',reason='shared dependency preflight refused',diagnostic=self.preflight,outputs={}) for s in self.stages};self.save();self.remove(self.scratch,self.output);raise ValueError('shared dependency preflight refused before preparation/compiler work; inspect '+str(self.report_path))
            for a in self.preflight['artifacts']:
                location=workspace.parent/a['location']
                actual,_=self.include(location) if a['role']=='include-directory' else self.snapshot(location)
                if actual!=a['sha256']:raise ValueError('preflight input changed before freezing: '+a['id'])
    def within(self,value):
        p=(self.root/value).resolve()
        if p!=self.root and self.root not in p.parents:raise ValueError("cache/output directory escapes configuration root")
        return p
    def owned_root(self,value):
        if isinstance(value,str):return self.within(value)
        workspace=Path(value['workspace']).resolve();name=value['directory'];relative_path(name)
        if self.root!=workspace and workspace not in self.root.parents:raise ValueError('short staging workspace must own the configuration root')
        if '/' in name or not name.startswith('.binviz-'):raise ValueError('short staging directory must be an owned .binviz-* basename')
        dest=workspace/name
        if dest.is_symlink():raise ValueError('owned staging root cannot be a symlink')
        dest.mkdir(parents=True,exist_ok=True);owner=dest/'.owner.json';identity=encode(dict(configuration=str(self.config_path)))
        try:
            with owner.open('xb') as f:f.write(identity)
        except FileExistsError:
            if owner.read_bytes()!=identity:raise ValueError('staging root belongs to a different configuration')
        return dest
    def event(self,stage,**extra):
        with self.lock:
            print(json.dumps(dict(stage=stage,elapsedSeconds=time.monotonic()-self.started,total=len(self.stages),processed=len(self.results),**self.stats,**extra)),file=sys.stderr,flush=True)
    def snapshot(self,path):
        path=Path(path).resolve()
        with self.lock:
            if path in self.blobs:
                stat=path.stat();old=self.blobs[path]
                if (stat.st_size,stat.st_mtime_ns)!=old[2]:raise ValueError("input changed during job: "+str(path))
                return old[:2]
            before=path.stat();raw=path.read_bytes();after=path.stat();self.stats["bytesRead"]+=len(raw)
            if (before.st_size,before.st_mtime_ns)!=(after.st_size,after.st_mtime_ns):raise ValueError("input changed while reading: "+str(path))
            sha=digest(raw);frozen=self.scratch / "f" / sha[:20] / ("input"+path.suffix);atomic(frozen,raw)
            self.blobs[path]=(sha,frozen,(after.st_size,after.st_mtime_ns));return sha,frozen
    def include(self,path):
        root=Path(path).resolve();records=[]
        if not root.is_dir():raise ValueError("include directory missing")
        for p in directory_files(root):
            if p.is_file():sha,frozen=self.snapshot(p);records.append((p.relative_to(root).as_posix(),sha,frozen))
        key=digest(encode([(name,sha) for name,sha,_ in records]));dest=self.scratch / "includes" / key
        with self.lock:
            if root in self.include_roots and self.include_roots[root]!=key:raise ValueError("include search tree changed during job")
            self.include_roots[root]=key
            for name,_,frozen in records:
                out=dest/name;out.parent.mkdir(parents=True,exist_ok=True)
                if not out.exists():atomic(out,frozen.read_bytes())
            dest.mkdir(parents=True,exist_ok=True)
        return key,dest
    def cache_read(self,path,outputs):
        try:
            record=json.loads((path/"record.json").read_bytes())
            if record.get("key")!=path.name or digest(encode(dict(recipe=record["recipe"],inputs=record["inputs"])))!=path.name:return None
            if set(record["outputs"])!=set(outputs):return None
            for id,value in record["outputs"].items():
                if value["file"]!=digest(id.encode())+".bin":return None
                raw=(path/value["file"]).read_bytes()
                with self.lock:self.stats["bytesRead"]+=len(raw)
                if digest(raw)!=value["sha256"]:return None
            return record
        except (OSError,ValueError,KeyError,TypeError):return None
    def run_stage(self,s):
        id=s["id"];start=time.monotonic();inputs={};paths={};includes=[]
        result=None;recipe=None;key=None;diagnostic=None;cleanup_warnings=[];staging_started=None;producer_started=None;staging_elapsed=0;producer_elapsed=0
        try:
            if any(self.results[d]["state"] not in ("success","cached") for d in s.get("dependsOn",[])):raise ValueError("dependency stage failed or refused")
            for key,name in s.get("inputs",{}).items():
                if name.startswith("stage:"):
                    parts=name.split(":",2)
                    if len(parts)!=3 or parts[1] not in s.get("dependsOn",[]):raise ValueError("stage input needs a declared dependency")
                    parent=self.results[parts[1]];path=Path(parent["outputs"][parts[2]]["path"])
                else:path=self.root/name
                sha,frozen=self.snapshot(path);inputs[key]=sha;paths[key]=str(frozen)
            for name in s.get("includeDirectories",[]):includes.append(self.include(self.root/name))
            executable=shutil.which(s["command"][0]) or str((self.root/s["command"][0]).resolve())
            tools=[executable,*s.get("tools",[])];tool_digests={}
            for name in tools:
                path=shutil.which(name) or str((self.root/name).resolve());sha,_=self.snapshot(path);tool_digests[str(Path(path).resolve())]=sha
            env={**os.environ,**s.get("environment",{})}
            keys=set(s.get("environmentKeys",["PATH","INCLUDE","LIB","CPATH","C_INCLUDE_PATH","CPLUS_INCLUDE_PATH","LIBRARY_PATH","SDKROOT","SOURCE_DATE_EPOCH"]))|set(s.get("environment",{}))
            recipe=dict(adapter=VERSION,adapterSha256=self.identity,platform=sys.platform,command=s["command"],environmentDigests={k:digest(env.get(k,"").encode()) for k in sorted(keys)},tools=tool_digests,includes=[key for key,_ in includes],outputs=s["outputs"],phase=s["phase"],timeoutSeconds=s.get("timeoutSeconds",120))
            if s.get("compilerVisibleNamespace"):
                recipe.update(compilerVisibleNamespace=s["compilerVisibleNamespace"],inputSlots=s.get("inputSlots",{}),inputNames=s.get("inputNames",{}))
            recipe.update(inputLayout=s.get('inputLayout',{}),includeLayout=s.get('includeLayout',{}),executionRoot=s.get('executionRoot','.'),mountMappings=s.get('mountMappings',[]),measurementsOutput=s.get('measurementsOutput'))
            key=digest(encode(dict(recipe=recipe,inputs=inputs)));entry=self.cache/key;lock=self.cache/(key+".lock")
            deadline=time.monotonic()+float(s.get("timeoutSeconds",120))+30
            while True:
                if self.cancel.is_set():raise ValueError("cancelled before stage execution")
                hit=self.cache_read(entry,s["outputs"])
                if hit is not None:
                    with self.lock:self.stats["cacheHits"]+=1
                    result=dict(id=id,phase=s["phase"],state="cached",cacheKey=key,cacheReason="all semantic inputs and output digests verified",recipe=recipe,inputs=inputs,outputs={k:dict(sha256=v["sha256"],path=str(entry/v["file"])) for k,v in hit["outputs"].items()},elapsedSeconds=time.monotonic()-start)
                    self.event("cache-hit",unit=id,cacheKey=key);return result
                try:lock.mkdir();break
                except FileExistsError:
                    if time.monotonic()>deadline:raise ValueError("cache writer lock timed out; no stale hit accepted")
                    self.cancel.wait(.05)
            visible_lock=None;visible_owned=False;work=None
            try:
                # A concurrent publisher may have completed before this lock.
                hit=self.cache_read(entry,s["outputs"])
                if hit is not None:
                    with self.lock:self.stats["cacheHits"]+=1
                    result=dict(id=id,phase=s["phase"],state="cached",cacheKey=key,cacheReason="concurrent identical work reused",recipe=recipe,inputs=inputs,outputs={k:dict(sha256=v["sha256"],path=str(entry/v["file"])) for k,v in hit["outputs"].items()},elapsedSeconds=time.monotonic()-start)
                    return result
                with self.lock:self.stats["cacheMisses"]+=1
                namespace=s.get("compilerVisibleNamespace")
                if namespace:
                    visible_lock=self.cache/(".visible-"+namespace+".lock")
                    while True:
                        try:visible_lock.mkdir();visible_owned=True;break
                        except FileExistsError:
                            if self.cancel.is_set() or time.monotonic()>deadline:raise ValueError("compiler-visible namespace lock timed out")
                            self.cancel.wait(.05)
                work=self.cache/(".visible-"+namespace if namespace else ".work-"+key)
                if work.exists():self.remove(work,self.cache)
                work.mkdir();stable_inputs={}
                staging_started=time.monotonic()
                for k,path in paths.items():
                    configured=s["inputs"][k]
                    if configured.startswith("stage:"):
                        _,parent,artifact_id=configured.split(":",2);filename=next(p for p in self.stages if p["id"]==parent)["outputs"][artifact_id];filename=Path(filename).name
                    else:filename=Path(configured).name
                    filename=s.get("inputNames",{}).get(k,filename)
                    if Path(filename).name!=filename:raise ValueError("compiler-visible filename must be a basename")
                    slot=s.get("inputSlots",{}).get(k,k)
                    stable=work/relative_path(s['inputLayout'][k]) if k in s.get('inputLayout',{}) else work/"i"/digest(slot.encode())[:12]/filename;stable.parent.mkdir(parents=True,exist_ok=True)
                    if stable.exists():raise ValueError("compiler-visible input slots collide")
                    shutil.copyfile(path,stable);stable_inputs[k]=str(stable)
                stable_includes=[]
                for i,(_,path) in enumerate(includes):
                    stable=work/relative_path(s['includeLayout'][str(i)]) if str(i) in s.get('includeLayout',{}) else work/"includes"/str(i)
                    for source in directory_files(path):
                        dest=stable/source.relative_to(path)
                        if dest.exists() and dest.read_bytes()!=source.read_bytes():raise ValueError('directory snapshot conflicts with declared file input')
                        if not dest.exists():dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source,dest)
                    stable.mkdir(parents=True,exist_ok=True);stable_includes.append(stable)
                execution_root=work/relative_path(s.get('executionRoot','.'));execution_root.mkdir(parents=True,exist_ok=True)
                outpaths={k:str(work/"outputs"/name) for k,name in s["outputs"].items()}
                for p in outpaths.values():Path(p).parent.mkdir(parents=True,exist_ok=True)
                def expand(arg):
                    arg=arg.replace('{execution-root}',str(execution_root))
                    for index,m in enumerate(s.get('mountMappings',[])):
                        mount_root=execution_root if m['source']=='execution-root' else work/'outputs'
                        mount_root.mkdir(parents=True,exist_ok=True)
                        arg=arg.replace('{mount:'+str(index)+'}','type=bind,source='+str(mount_root)+',target='+m['target']+(',readonly' if m['readOnly'] else ''))
                    def sub(m):
                        kind,key=m.groups()
                        if kind=="input":return stable_inputs[key]
                        if kind=="output":return outpaths[key]
                        if kind=="include":return str(stable_includes[int(key)])
                        raise ValueError("unknown command placeholder")
                    return re.sub(r"\{(input|output|include):([^}]+)\}",sub,arg)
                command=[executable,*[expand(a) for a in s["command"][1:]]]
                staging_elapsed=time.monotonic()-staging_started
                with self.lock:self.stats['stagingSeconds']+=staging_elapsed
                self.event("stage-start",unit=id,phase=s["phase"],cacheKey=key,cacheReason="input/recipe absent or cached output corrupt")
                producer_started=time.monotonic();proc=subprocess.Popen(command,cwd=execution_root,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
                with self.lock:self.stats['subprocesses']+=1
                deadline=time.monotonic()+float(s.get("timeoutSeconds",120))
                try:
                    while True:
                        rss=resident_bytes(proc.pid)
                        with self.lock:
                            if rss is not None:
                                self.running_rss[proc.pid]=rss;self.stats["rssSamples"]+=1
                                observed=sum(self.running_rss.values())
                                self.stats["peakObservedDirectProcessRssBytes"]=max(self.stats["peakObservedDirectProcessRssBytes"] or 0,observed)
                                if observed>self.memory*1024*1024:raise ValueError("observed direct-process RSS exceeds configured memory budget")
                        try:stdout,stderr=proc.communicate(timeout=.05);break
                        except subprocess.TimeoutExpired:
                            if self.cancel.is_set() or time.monotonic()>deadline:
                                raise ValueError("cancelled" if self.cancel.is_set() else "bounded stage timeout")
                finally:
                    if proc.poll() is None:proc.kill();proc.communicate()
                    with self.lock:self.running_rss.pop(proc.pid,None)
                diagnostic=dict(exitCode=proc.returncode,stdout=stdout.decode(errors="replace"),stderr=stderr.decode(errors="replace"))
                producer_elapsed=time.monotonic()-producer_started
                with self.lock:self.stats['producerSeconds']+=producer_elapsed
                diagnostic.update(stagingSeconds=staging_elapsed,producerSeconds=producer_elapsed)
                if s.get('measurementsOutput'):
                    measurement=s['measurementsOutput']
                    if measurement not in outpaths:raise ValueError('measurements output must be a declared output artifact')
                    measurement_path=Path(outpaths[measurement])
                    if measurement_path.exists():diagnostic['producerMeasurements']=json.loads(measurement_path.read_bytes())
                if proc.returncode:
                    result=dict(id=id,phase=s["phase"],state="rejected",cacheKey=key,recipe=recipe,inputs=inputs,outputs={},diagnostic=diagnostic,elapsedSeconds=time.monotonic()-start)
                    return result
                for k,path in stable_inputs.items():
                    if digest(Path(path).read_bytes())!=inputs[k]:raise ValueError('producer changed frozen input: '+k)
                for (expected,_),path in zip(includes,stable_includes):
                    actual=digest(encode([(p.relative_to(path).as_posix(),digest(p.read_bytes())) for p in directory_files(path)]))
                    if actual!=expected:raise ValueError('producer changed complete frozen scan directory')
                publish=Path(tempfile.mkdtemp(prefix=".publish-",dir=self.cache));outputs={}
                for k,path in outpaths.items():
                    raw=Path(path).read_bytes();sha=digest(raw);name=digest(k.encode())+".bin";atomic(publish/name,raw);outputs[k]=dict(sha256=sha,file=name)
                # Reject source promotions while work was running. Outputs from
                # declared dependencies are frozen just like original inputs.
                with self.lock:tracked=list(self.blobs.items())
                for path,(_,_,stat) in tracked:
                    actual=path.stat()
                    if (actual.st_size,actual.st_mtime_ns)!=stat:raise ValueError("input changed during stage: "+str(path))
                atomic(publish/"record.json",encode(dict(key=key,recipe=recipe,inputs=inputs,outputs=outputs,diagnostic=diagnostic)))
                if entry.exists():self.remove(entry,self.cache)
                os.replace(publish,entry)
                result=dict(id=id,phase=s["phase"],state="success",cacheKey=key,cacheReason="published verified output digests",recipe=recipe,inputs=inputs,outputs={k:dict(sha256=v["sha256"],path=str(entry/v["file"])) for k,v in outputs.items()},diagnostic=diagnostic,elapsedSeconds=time.monotonic()-start)
                return result
            finally:
                # Cleanup is secondary to the compiler/build outcome. Attempt every
                # owned lock release even when Windows refuses a directory removal.
                for path,parent in [(work,self.cache),(visible_lock if visible_owned else None,None),(lock,None)]:
                    if path is None:continue
                    try:
                        if parent is None:path.rmdir()
                        elif path.exists():self.remove(path,parent)
                    except (OSError,ValueError) as e:
                        warning=dict(path=str(path),reason=str(e));cleanup_warnings.append(warning)
                        self.event("cleanup-warning",unit=id,**warning)
                if result is not None and cleanup_warnings:result["cleanupWarnings"]=cleanup_warnings
        except (OSError,ValueError,KeyError,IndexError) as e:
            result=dict(id=id,phase=s["phase"],state="refused",reason=str(e),inputs=inputs,outputs={},elapsedSeconds=time.monotonic()-start)
            result['failurePhase']='producer' if producer_started is not None else 'staging' if staging_started is not None else 'setup'
            result['setupError']='windows-path-limit' if isinstance(e,OSError) and getattr(e,'winerror',None) in (206,3) and len(str(getattr(e,'filename','')))>240 else 'filesystem' if isinstance(e,OSError) else 'configuration-or-evidence'
            result['stagingSeconds']=staging_elapsed or (time.monotonic()-staging_started if staging_started is not None else 0)
            result['producerSeconds']=producer_elapsed or (time.monotonic()-producer_started if producer_started is not None else 0)
            if recipe is not None:result.update(recipe=recipe,cacheKey=key)
            result["diagnostic"]={**(diagnostic or {}),"reason":str(e),"failurePhase":result['failurePhase'],"setupError":result['setupError'],"stagingSeconds":result['stagingSeconds'],"producerSeconds":result['producerSeconds']}
            if cleanup_warnings:result["cleanupWarnings"]=cleanup_warnings
            return result
    def run(self):
        pending=list(self.stages);active={};used=0
        with ThreadPoolExecutor(max_workers=self.cpu) as pool:
            try:
                while pending or active:
                    for s in list(pending):
                        cost=int(s.get("memoryMb",128))
                        if len(active)<self.cpu and used+cost<=self.memory and all(d in self.results for d in s.get("dependsOn",[])):
                            pending.remove(s);active[pool.submit(self.run_stage,s)]=(s,cost);used+=cost
                            self.stats["peakScheduledMemoryMb"]=max(self.stats["peakScheduledMemoryMb"],used)
                    if not active:break
                    finished,_=wait(active,timeout=.1,return_when=FIRST_COMPLETED)
                    for future in finished:
                        s,cost=active.pop(future);used-=cost;self.results[s["id"]]=future.result();self.save();self.event("stage-complete",unit=s["id"],state=self.results[s["id"]]["state"])
            except KeyboardInterrupt:
                self.cancel.set()
                for future,(s,_) in active.items():self.results[s["id"]]=future.result()
                for s in pending:self.results[s["id"]]=dict(id=s["id"],phase=s["phase"],state="unexamined",reason="cancelled")
        changed=[]
        for path,(expected,_,_) in self.blobs.items():
            try:raw=path.read_bytes();self.stats["bytesRead"]+=len(raw);same=digest(raw)==expected
            except OSError:same=False
            if not same:changed.append(str(path))
        for root,expected in self.include_roots.items():
            try:current=digest(encode([(p.relative_to(root).as_posix(),digest(p.read_bytes())) for p in directory_files(root)]))
            except (OSError,ValueError):current=None
            if current!=expected:changed.append(str(root))
        if changed:
            for r in self.results.values():
                if r["state"] in ("success","cached"):r["state"]="refused";r["reason"]="inputs changed during job: "+", ".join(changed)
        self.save();self.remove(self.scratch,self.output);self.event("complete")
        return 0 if all(r["state"] in ("success","cached") for r in self.results.values()) else 1
    def save(self):
        artifacts={};stages=[];builds=[]
        def artifact(id,role,path,sha):
            record=dict(id=id,role=role,location=str(path),sha256=sha)
            if id in artifacts and artifacts[id]["sha256"]!=sha:raise ValueError("artifact id refers to different bytes across stages: "+id)
            artifacts[id]=record
        for s in self.stages:
            if s["id"] not in self.results:continue
            r=self.results[s["id"]];recipe=r.get("recipe")
            if not recipe:continue
            id=s["id"];recipe_path=self.output/(id+".recipe.json");raw=encode(recipe);atomic(recipe_path,raw)
            recipe_id=id+":recipe";artifact(recipe_id,"recipe",recipe_path,digest(raw));parents=[]
            for key,sha in r.get("inputs",{}).items():
                name=s["inputs"][key]
                if name.startswith("stage:"):
                    _,stage,parent=name.split(":",2);path=self.results[stage]["outputs"][parent]["path"]
                    if key!=parent:raise ValueError("shared stage input must retain its artifact id")
                else:path=str((self.root/name).resolve())
                artifact(key,"input",path,sha);parents.append(key)
            for path,sha in recipe["tools"].items():
                key="tool:"+sha;artifact(key,"tool",path,sha);parents.append(key)
            for name,sha in zip(s.get("includeDirectories",[]),recipe["includes"]):
                path=(self.root/name).resolve();key="include:"+digest(str(path).encode());artifact(key,"include-directory",path,sha);parents.append(key)
            outputs=[]
            for key,v in r["outputs"].items():artifact(key,"output",v["path"],v["sha256"]);outputs.append(key)
            if not outputs:
                diagnostic_path=self.output/(id+".diagnostic.json");raw=encode(r.get("diagnostic",dict(reason=r.get("reason"))));atomic(diagnostic_path,raw)
                key=id+":diagnostic";artifact(key,"diagnostic",diagnostic_path,digest(raw));outputs.append(key)
            stages.append(dict(id=id,parents=sorted(set(parents)),outputs=outputs,recipe=recipe_id,result="success" if r["state"] in ("success","cached") else r["state"]))
            if s.get("unit") and s["phase"] in ("prepare","compile","link","validate"):builds.append(dict(unit=s["unit"],stage=id,phase=s["phase"]))
        atomic(self.report_path,encode(dict(format="binviz-build-batch",schemaVersion=1,adapter=VERSION,preflight=self.preflight,evidence=dict(artifacts=list(artifacts.values()),stages=stages),builds=builds,stages=[self.results[s["id"]] for s in self.stages if s["id"] in self.results],unexamined=len(self.stages)-len(self.results),statistics=dict(**self.stats,wallSeconds=time.monotonic()-self.started))))
    def remove(self,path,parent):
        resolved=path.resolve();parent=parent.resolve()
        if resolved==parent or parent not in resolved.parents or path.is_symlink():raise ValueError("refused cleanup outside verified job/cache directory")
        shutil.rmtree(path)

if __name__=="__main__":
    try:
        if len(sys.argv)!=3:raise ValueError(__doc__)
        sys.exit(Batch(sys.argv[1],sys.argv[2]).run())
    except (OSError,ValueError,KeyError) as e:
        if len(sys.argv)==3:
            report_path=Path(sys.argv[2]).resolve()
            if not report_path.exists():
                diagnostic=dict(reason=str(e),failurePhase='setup',setupError='windows-path-limit' if isinstance(e,OSError) and getattr(e,'winerror',None)==206 else 'filesystem' if isinstance(e,OSError) else 'configuration-or-evidence')
                try:atomic(report_path,encode(dict(format='binviz-build-batch',schemaVersion=1,evidence=dict(artifacts=[],stages=[]),builds=[],stages=[],unexamined=0,setupDiagnostic=diagnostic,statistics=dict(subprocesses=0,stagingSeconds=0,producerSeconds=0))))
                except OSError:pass
        print(str(e),file=sys.stderr);sys.exit(1)
