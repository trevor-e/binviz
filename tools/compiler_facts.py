#!/usr/bin/env python3
"""Maintained Clang C fact adapter. No source rewriting or native ABI allowances.

Usage: python tools/compiler_facts.py config.json output/facts.json
Config: schemaVersion=1, compiler, target, flags, units=[{id, source}].
Paths are relative to config.json; progress JSONL goes to stderr, facts to disk.
Prepared text and immutable recipes are retained beside the output report.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
from contextlib import contextmanager

VERSION = "binviz-clang-facts-v1"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def atomic(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as f:
        f.write(data)
        f.flush()
        os.fsync(f.fileno())
        name = f.name
    os.replace(name, path)


def json_bytes(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def run(argv, cwd=None, input_data=None):
    p = subprocess.run(argv, cwd=cwd, input=input_data, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=120)
    if p.returncode:
        raise RuntimeError(p.stderr.decode("utf-8", errors="replace")[-12000:])
    return p.stdout


def cacheable_flags(flags):
    """Do not cache unknown options with hidden plugin/response-file inputs."""
    paired = False
    for flag in flags:
        if paired:
            paired = False
            continue
        if flag in ("-I", "-D", "-U"):
            paired = True
        elif not re.fullmatch(r"(?:-std=.*|-W.*|-w|-O[0-3sgz]|-[DIU].+|-f(?:no-)?(?:builtin|freestanding|signed-char|unsigned-char|short-wchar|short-enums|common|wrapv|strict-aliasing|delete-null-pointer-checks)|-m(?:32|64|soft-float|hard-float|fp32|fp64|(?:abi|cpu|arch)=.*))", flag):
            return False
    return not paired


@contextmanager
def cache_lock(root, key):
    lock = root / (key + ".lock")
    deadline = time.monotonic() + 150
    while True:
        try:
            lock.mkdir()
            break
        except FileExistsError:
            if time.monotonic() > deadline:
                raise RuntimeError("extraction cache writer timed out; no stale output accepted")
            time.sleep(.05)
    try:
        yield root / (key + ".json")
    finally:
        lock.rmdir()


def read_cached_facts(path, inputs):
    try:
        record = json.loads(path.read_bytes())
        payload = record["facts"]
        if record["inputs"] != inputs or record["sha256"] != digest(json_bytes(payload)):
            return None
        required = {"definitions", "calls", "addressReferences", "gaps"}
        optional = {"intrinsics", "compilerStorage", "storageIr"}
        if not required <= set(payload) or set(payload) - required - optional:
            return None
        if ("compilerStorage" in payload) != ("storageIr" in payload):
            return None
        if "storageIr" in payload and not isinstance(payload["storageIr"], str):
            return None
        if not all(isinstance(v, list) for k, v in payload.items() if k != "storageIr"):
            return None
        return payload
    except (OSError, ValueError, KeyError, TypeError):
        return None


def compiler_type(value, macros, aliases=None):
    """Lower only compiler-provided canonical spellings; retain all raw types."""
    raw = value.get("qualType", "<unknown>")
    canonical = value.get("desugaredQualType", raw)
    # TypedefDecl supplies the underlying compiler type. Expand identifiers only
    # in the retained canonical spelling; the written type is never replaced.
    def expand(text, visiting=frozenset()):
        pieces=[];end=0;previous=None
        for token in re.finditer(r"\b[A-Za-z_]\w*\b",text):
            name=token[0];replacement=name
            # Tag namespaces are distinct from typedef names, including RECT.
            if previous not in ("struct","union","enum") and name in (aliases or {}) and name not in visiting:
                replacement=expand(aliases[name],visiting|{name})
            pieces.extend((text[end:token.start()],replacement));end=token.end();previous=name
        pieces.append(text[end:]);return "".join(pieces)
    canonical=expand(canonical)
    canonical = re.sub(r"\*\s+(?=\*)", "*", canonical)
    plain = " ".join(x for x in canonical.split() if x not in ("const", "volatile", "restrict"))
    bits = signed = None
    category = "unsupported"
    widths = {"char": int(macros.get("__CHAR_BIT__", "8")),
              "short": int(macros.get("__SIZEOF_SHORT__", "0")) * 8,
              "int": int(macros.get("__SIZEOF_INT__", "0")) * 8,
              "long": int(macros.get("__SIZEOF_LONG__", "0")) * 8,
              "long long": int(macros.get("__SIZEOF_LONG_LONG__", "0")) * 8}
    if plain == "void":
        category = "void"
    elif plain.endswith("*") or ("(*)" in plain and "[" not in plain):
        category, bits = "pointer", int(macros.get("__SIZEOF_POINTER__", "0")) * 8
    elif plain in ("_Bool", "bool"):
        category, bits, signed = "integer", widths["char"], False
    else:
        base = plain.replace("unsigned ", "").replace("signed ", "")
        base = {"short int": "short", "long int": "long", "long long int": "long long",
                "unsigned": "int", "signed": "int"}.get(base, base)
        if base in widths:
            category, bits = "integer", widths[base]
            signed = not plain.startswith("unsigned")
            if plain == "char":
                signed = "__CHAR_UNSIGNED__" not in macros
    return dict(spelling=raw, canonical=canonical, category=category, bits=bits, signed=signed)


def children(node):
    return node.get("inner", [])


def unwrap(node):
    while node.get("kind") in ("ImplicitCastExpr", "CStyleCastExpr", "ParenExpr", "ConstantExpr") and children(node):
        node = children(node)[0]
    return node


def source_span(node, artifact, size):
    bounds = node.get("range", {})
    begin, end = bounds.get("begin", {}), bounds.get("end", {})
    editable = not any("spellingLoc" in v or "expansionLoc" in v for v in (begin, end))
    begin = begin.get("expansionLoc", begin)
    end = end.get("expansionLoc", end)
    start, finish = begin.get("offset"), end.get("offset")
    if start is None or finish is None or not 0 <= start < finish + end.get("tokLen", 1) <= size:
        raise ValueError("Clang node lacks an exact prepared UTF-8 byte span")
    return dict(artifact=artifact, start=start, end=finish + end.get("tokLen", 1), editable=editable)


def extract(ast, unit, artifact, prepared, macros):
    definitions, calls, references, gaps, intrinsics = [], [], [], [], []
    nodes, ids = {}, {}
    size = len(prepared)
    aliases = {}

    def typedefs(node):
        if node.get("kind") == "TypedefDecl":
            value = node.get("type", {})
            aliases[node["name"]] = value.get("desugaredQualType", value.get("qualType", node["name"]))
        for child in children(node):
            typedefs(child)

    typedefs(ast)
    def typed(value):
        return compiler_type(value, macros, aliases)

    def gap(reason):
        gaps.append(dict(unit=unit, stage="fact-extraction", reason=reason))

    def collect(node):
        if node.get("kind") == "FunctionDecl" and not node.get("isImplicit"):
            try:
                span = source_span(node, artifact, size)
            except ValueError as e:
                gap(f"declaration {node.get('name')}: {e}")
            else:
                name = node["name"]
                stable = f"{unit}:decl:{span['start']}:{span['end']}:{name}"
                ids[node["id"]] = stable
                nodes[node["id"]] = node
                param_nodes = [p for p in children(node) if p.get("kind") == "ParmVarDecl"]
                params = [typed(p.get("type", {})) for p in param_nodes]
                ftype = node.get("type", {}).get("qualType", "")
                abi_params = params
                if ftype.count("(") == 1 and ftype.endswith(")"):
                    # This lowers Clang's printed function type, not C source.
                    # K&R FunctionDecl types carry promoted incoming parameters,
                    # while ParmVarDecl retains the narrowed local variable type.
                    contract_types = ftype.split("(", 1)[1][:-1].strip()
                    if contract_types in ("", "void"):
                        abi_params = []
                    else:
                        abi_params = [typed(dict(qualType=t.strip())) for t in contract_types.split(",") if t.strip() != "..."]
                # Clang prints function return type before its parameter list.
                # Complex declarators are deliberately retained as unsupported.
                raw_result = ftype.split("(", 1)[0].strip()
                canonical_result = node.get("type", {}).get("desugaredQualType", ftype).split("(", 1)[0].strip()
                result_type = typed(dict(qualType=raw_result, desugaredQualType=canonical_result))
                if ftype.count("(") > 1:
                    result_type = dict(spelling=ftype, canonical=ftype, category="unsupported", bits=None, signed=None)
                body = any(c.get("kind") == "CompoundStmt" for c in children(node))
                definition = dict(id=stable, unit=unit, name=name,
                                  linkage="internal" if node.get("storageClass") == "static" else "external",
                                  hasBody=body, oldStyle=ftype.endswith("()"),
                                  variadic=bool(node.get("variadic", False)),
                                  returnType=result_type,
                                  parameters=params, abiParameters=abi_params, span=span, declarationGroup=f"{unit}:group:{span['start']}")
                location=node.get("loc",{});offset=location.get("offset")
                if isinstance(offset,int) and prepared[offset:offset+len(name.encode())]==name.encode():
                    definition["nameSpan"]=dict(artifact=artifact,start=offset,end=offset+len(name.encode()),editable=span["editable"])
                definitions.append(definition)
        for child in children(node):
            collect(child)

    collect(ast)

    def walk(node, caller, consumed=True, direct_ref=None):
        kind = node.get("kind")
        inner = children(node)
        if kind == "FunctionDecl":
            caller = ids.get(node.get("id"))
        if kind == "CallExpr" and caller and inner:
            ref = unwrap(inner[0])
            declaration = ref.get("referencedDecl", {})
            if ref.get("kind") == "DeclRefExpr" and declaration.get("kind") == "FunctionDecl":
                try:
                    span = source_span(node, artifact, size)
                    args = []
                    for arg in inner[1:]:
                        original = arg
                        # Drop implicit conversions, preserve explicit casts' resulting type.
                        while original.get("kind") in ("ImplicitCastExpr", "ParenExpr") and children(original):
                            original = children(original)[0]
                        args.append(dict(supplied=typed(original.get("type", {})),
                                         promoted=typed(arg.get("type", {})),
                                         span=source_span(arg, artifact, size)))
                    if declaration.get("name","").startswith("__builtin_"):
                        name=declaration["name"];effect={"__builtin_trap":"trap","__builtin_debugtrap":"debug-trap","__builtin_unreachable":"unreachable"}.get(name,"unsupported")
                        intrinsics.append(dict(id=f"{unit}:intrinsic:{span['start']}:{span['end']}:{name}",unit=unit,caller=caller,name=name,effect=effect,span=span,arguments=args,resultType=typed(node.get("type",{}))))
                        if effect=="unsupported":gap("unsupported compiler intrinsic "+name+" in "+caller+"; lowering/effects require a target-specific profile")
                    else:
                        target = ids[declaration["id"]]
                        calls.append(dict(id=f"{unit}:call:{span['start']}:{span['end']}", unit=unit,
                                      caller=caller, callee=declaration["name"], declaration=target,
                                      span=span, arguments=args, resultConsumed=consumed,
                                      resultType=typed(node.get("type", {}))))
                        calls[-1]["calleeSpan"]=source_span(ref,artifact,size)
                except (KeyError, ValueError) as e:
                    gap(f"direct call {declaration.get('name','unnamed')} in {caller}: unresolved declaration/provenance ({type(e).__name__})")
                direct_ref = ref.get("id")
            else:
                gap(f"indirect/unsupported call in {caller}; target set unresolved")
        if kind == "DeclRefExpr" and node.get("id") != direct_ref:
            ref = node.get("referencedDecl", {})
            if ref.get("kind") == "FunctionDecl":
                try:
                    references.append(dict(unit=unit, caller=caller or "", declaration=ids[ref["id"]],
                                           span=source_span(node, artifact, size)))
                except (KeyError, ValueError) as e:
                    gap(f"address reference: {e}")
        for i, child in enumerate(inner):
            use = True
            if kind == "CompoundStmt":
                use = False
            elif kind in ("ImplicitCastExpr", "ParenExpr", "ConstantExpr", "LabelStmt", "AttributedStmt"):
                use = consumed
            elif kind in ("CaseStmt","DefaultStmt"):
                use = consumed if i == len(inner)-1 else True
            elif kind == "CStyleCastExpr":
                use = node.get("type", {}).get("qualType") != "void" and consumed
            elif kind == "BinaryOperator" and node.get("opcode") == ",":
                use = consumed if i == len(inner) - 1 else False
            elif kind in ("IfStmt", "WhileStmt", "SwitchStmt"):
                use = i == 0
            elif kind == "DoStmt":
                use = i == len(inner) - 1
            elif kind == "ForStmt":
                use = i == 2  # init, condition-variable, condition, increment, body
            elif kind == "CallExpr":
                use = True
            walk(child, caller, use, direct_ref)

    walk(ast, None)
    return definitions, calls, references, gaps, intrinsics


def storage_observations(ir, unit, artifact):
    """Record compiler IR allocation/lifetime instructions without inferring source lifetimes."""
    import re
    functions=[];current=None
    def extent(ty):
        ty=ty.strip()
        scalar=re.fullmatch(r"i(\d+)",ty)
        if scalar:return (int(scalar[1])+7)//8
        if ty in ("ptr","float"):return 4
        if ty=="double":return 8
        array=re.fullmatch(r"\[(\d+) x (.*)\]",ty)
        if array:
            n=extent(array[2]);return None if n is None else int(array[1])*n
        return None
    for line_number,line in enumerate(ir.splitlines(),1):
        match=re.match(r'define .*?@("[^"]+"|[^ (]+)\(',line)
        if match:
            current=dict(unit=unit,name=match[1].strip('"'),irArtifact=artifact,allocations=[],lifetimes=[],frontiers=[]);functions.append(current)
        if current is None:continue
        allocation=re.match(r"\s*(%[^ ]+) = alloca (.*)",line)
        if allocation:
            parts=allocation[2].split(", ");ty=parts[0];size=extent(ty);alignment=next((int(p[6:]) for p in parts if p.startswith("align ")),None)
            count=next((p for p in parts[1:] if re.match(r"i\d+ ",p)),None)
            if count:
                constant=re.fullmatch(r"i\d+ (\d+)",count)
                size=None if size is None or constant is None else size*int(constant[1])
            current["allocations"].append(dict(local=allocation[1],llvmType=ty,bytes=None if size is None else hex(size),alignment=alignment,line=line_number,instruction=line.strip()))
            if size is None:current["frontiers"].append("dynamic or aggregate allocation needs compiler layout: "+allocation[1])
        lifetime=re.search(r"@llvm\.lifetime\.(start|end)[^(]*\(i64 (-?\d+), (.*)\)",line)
        if lifetime:current["lifetimes"].append(dict(operation=lifetime[1],bytes=None if int(lifetime[2])<0 else hex(int(lifetime[2])),object=lifetime[3],line=line_number,instruction=line.strip()))
        if "@llvm.stacksave" in line or "@llvm.stackrestore" in line:current["frontiers"].append("dynamic stack save/restore at IR line "+str(line_number))
        if line=="}":current=None
    return functions

def produce(config_path, output):
    config_path, output = Path(config_path).resolve(), Path(output).resolve()
    config = json.loads(config_path.read_text(encoding="utf-8"))
    if config.get("schemaVersion") != 1 or not config.get("units"):
        raise ValueError("config requires schemaVersion 1 and nonempty units")
    for u in config["units"]:
        if not re.fullmatch(r"[A-Za-z0-9_.-]+", u["id"]):
            raise ValueError("unit id must contain only letters, digits, underscore, dot or dash")
    if len({u["id"] for u in config["units"]}) != len(config["units"]):
        raise ValueError("duplicate unit id")
    cwd = config_path.parent
    requested_compiler = config.get("compiler", "clang")
    compiler = shutil.which(requested_compiler) or str((cwd / requested_compiler).resolve())
    flags = config.get("flags", ["-std=gnu89"])
    if not isinstance(flags, list) or not all(isinstance(x, str) for x in flags):
        raise ValueError("flags must be a string array")
    target = config["target"]
    base = [compiler, "--target=" + target, *flags]
    cache = (cwd / config["cache"]).resolve() if config.get("cache") else None
    if cache is not None:
        if cwd not in cache.parents:
            raise ValueError("extraction cache must be a separate directory within the configuration root")
        cache.mkdir(parents=True, exist_ok=True)
    cache_enabled = cache is not None and cacheable_flags(flags)
    cache_reason = ("extraction cache not configured" if cache is None else
                    "unknown flag may reference undeclared external inputs" if not cache_enabled else "not examined")
    statistics = dict(cacheHits=0, cacheMisses=0, extractionSubprocesses=0, preparationSubprocesses=0,
                      bootstrapSubprocesses=0, cacheReason=cache_reason)
    cache_units = []
    artifacts, stages = [], []
    result = dict(format="binviz-compiler-facts", schemaVersion=1, units=[], definitions=[], calls=[],
                  addressReferences=[], gaps=[], compilerStorage=[], intrinsics=[], evidence=dict(artifacts=artifacts, stages=stages))
    started = time.monotonic()

    def event(stage, unit=None, **extra):
        print(json.dumps(dict(stage=stage, unit=unit, total=len(config["units"]),
                              processed=len(result["units"]),
                              accepted=sum(u["status"] == "success" for u in result["units"]),
                              rejected=sum(u["status"] == "failed" for u in result["units"]),
                              unexamined=len(config["units"])-len(result["units"])+sum(u["status"] == "unexamined" for u in result["units"]),
                              **statistics,
                              elapsedSeconds=time.monotonic()-started, **extra)), file=sys.stderr, flush=True)

    def artifact(id, role, path, data=None):
        data = path.read_bytes() if data is None else data
        record = dict(id=id, role=role, location=os.path.relpath(path, output.parent).replace("\\", "/"),
                      sha256=digest(data))
        if role in ("source", "prepared"):
            try:
                record["normalizedSha256"] = digest(data.decode("utf-8").replace("\r\n", "\n").encode("utf-8"))
            except UnicodeDecodeError:
                pass  # Preserve raw identity; compiler diagnostics report invalid input.
        artifacts.append(record)
        return id

    event("input-verification")
    bootstrap_error = None
    version, macros_text, layout = "unavailable", "", ""
    try:
        statistics["bootstrapSubprocesses"] += 1
        version = run([compiler, "--version"]).decode("utf-8", errors="replace")
        statistics["bootstrapSubprocesses"] += 1
        macros_text = run([*base, "-dM", "-E", "-x", "c", "-"], cwd, b"").decode("utf-8")
        statistics["bootstrapSubprocesses"] += 1
        ir = run([*base, "-S", "-emit-llvm", "-x", "c", "-o", "-", "-"], cwd, b"").decode("utf-8")
        layout = next(line.split('"')[1] for line in ir.splitlines() if line.startswith("target datalayout"))
        target = next(line.split('"')[1] for line in ir.splitlines() if line.startswith("target triple"))
    except (OSError, RuntimeError, subprocess.TimeoutExpired, StopIteration) as e:
        bootstrap_error = str(e) or "compiler target data layout unavailable"
    macros = dict(line[len("#define "):].split(" ", 1) for line in macros_text.splitlines() if line.startswith("#define ") and " " in line[len("#define "):])
    kept = {k: v for k, v in macros.items() if k.startswith("__SIZEOF_") or k in ("__CHAR_BIT__", "__CHAR_UNSIGNED__")}
    result["compiler"] = dict(binary="compiler", version=version, target=target, flags=flags,
                              profile="c32-scalar" if kept.get("__SIZEOF_POINTER__") == "4" and kept.get("__SIZEOF_INT__") == "4" else "unsupported", targetMacros=kept,
                              adapter=VERSION, adapterSha256=digest(Path(__file__).read_bytes()), workingDirectory=str(cwd), dataLayout=layout)
    try:
        artifact("compiler", "compiler", Path(compiler))
    except OSError:
        artifact("compiler", "missing-compiler", Path(compiler), b"")
    artifact("adapter", "adapter", Path(__file__))
    artifact("configuration", "compiler-configuration", config_path)
    recipe_path = output.parent / (output.stem + ".artifacts") / "recipe.json"
    atomic(recipe_path, json_bytes(result["compiler"]))
    artifact("recipe", "recipe", recipe_path)
    cancelled = False
    for u in config["units"]:
        id = u["id"]
        source = (cwd / u["source"]).resolve()
        tu = dict(id=id, source=f"{id}:source", prepared=None, status="failed")
        try:
            artifact(tu["source"], "source", source)
            if bootstrap_error:
                raise RuntimeError("compiler configuration unavailable: " + bootstrap_error)
            event("preparation", id)
            # Line-free prepared text gives exact UTF-8 spans even across headers.
            # -H records actual selected preprocessing dependencies (not guessed includes).
            statistics["preparationSubprocesses"] += 1
            pp = subprocess.run([*base, "-E", "-P", "-H", "-x", "c", str(source)], cwd=cwd,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
            if pp.returncode:
                raise RuntimeError(pp.stderr.decode("utf-8", errors="replace"))
            parents = [tu["source"], "compiler", "adapter", "configuration"]
            for line in pp.stderr.decode("utf-8", errors="replace").splitlines():
                match = re.match(r"^\.+ (.+)$", line)
                if match:
                    dependency = (cwd / match[1]).resolve()
                    dep_id = "header:" + digest(str(dependency).encode("utf-8"))
                    if not any(a["id"] == dep_id for a in artifacts):
                        artifact(dep_id, "header", dependency)
                    if dep_id not in parents:
                        parents.append(dep_id)
            prepared = pp.stdout
            prepared_path = recipe_path.parent / (id + ".prepared.i")
            atomic(prepared_path, prepared)
            tu["prepared"] = artifact(f"{id}:prepared", "prepared", prepared_path, prepared)
            stages.append(dict(id=f"{id}:prepare", parents=parents, outputs=[tu["prepared"]], recipe="recipe", result="success"))
            event("fact-extraction", id)
            # CPP has consumed defines/includes; retain target, dialect and semantic flags.
            # cpp-output prevents a second preprocessing pass from reapplying
            # command-line defines after a source #undef or expanding escaped tokens.
            inputs = dict(unit=id, preparedArtifact=tu["prepared"], preparedPath=str(prepared_path),
                          storageFacts=bool(config.get("storageFacts",False)),
                          preparedSha256=digest(prepared), compiler=result["compiler"],
                          dependencies={a["id"]: a["sha256"] for a in artifacts if a["id"] in parents and a["id"] != "configuration"},
                          environmentDigests={k: digest(v.encode("utf-8")) for k, v in sorted(os.environ.items())})
            cache_key = digest(json_bytes(inputs))

            def extract_facts():
                statistics["extractionSubprocesses"] += 1
                ast_bytes = run([*base, "-Xclang", "-ast-dump=json", "-fsyntax-only", "-x", "cpp-output", str(prepared_path)], cwd)
                defs, calls, refs, gaps, intrinsics = extract(json.loads(ast_bytes), id, tu["prepared"], prepared, kept)
                payload=dict(definitions=defs, calls=calls, addressReferences=refs, gaps=gaps)
                if intrinsics:payload["intrinsics"]=intrinsics
                if config.get("storageFacts",False):
                    statistics["extractionSubprocesses"] += 1
                    ir=run([*base,"-S","-emit-llvm","-o","-","-x","cpp-output",str(prepared_path)],cwd).decode("utf-8")
                    payload["compilerStorage"]=storage_observations(ir,id,id+":llvm-ir")
                    payload["storageIr"]=ir
                return payload

            payload = None
            hit = False
            reason = cache_reason
            if cache_enabled:
                with cache_lock(cache, cache_key) as entry:
                    payload = read_cached_facts(entry, inputs)
                    hit = payload is not None
                    reason = ("current prepared/compiler/adapter/environment and output digests verified" if hit else
                              "cached output missing or corrupt" if entry.exists() else "semantic input key not cached")
                    if payload is None:
                        payload = extract_facts()
                    # Recheck selected inputs for hits as well as fresh extractions.
                    for a in artifacts:
                        if a["id"] in parents and a["id"] != "configuration":
                            if digest((output.parent / a["location"]).read_bytes()) != a["sha256"]:
                                raise RuntimeError("input changed during extraction: " + a["id"])
                    if not hit:
                        atomic(entry, json_bytes(dict(inputs=inputs, sha256=digest(json_bytes(payload)), facts=payload)))
            else:
                payload = extract_facts()
            statistics["cacheHits" if hit else "cacheMisses"] += 1
            statistics["cacheReason"] = reason
            cache_units.append(dict(unit=id, state="cached" if hit else "extracted", key=cache_key if cache_enabled else None, reason=reason))
            fact_payload={k:v for k,v in payload.items() if k!="storageIr"}
            outputs=[]
            if "storageIr" in payload:
                ir_path=recipe_path.parent/(id+".llvm.ll");atomic(ir_path,payload["storageIr"].encode("utf-8"))
                outputs.append(artifact(id+":llvm-ir","compiler-ir",ir_path))
            for key, values in fact_payload.items():
                result[key].extend(values)
            fact_path = recipe_path.parent / (id + ".facts.json")
            atomic(fact_path, json_bytes(fact_payload))
            fact_id = artifact(f"{id}:facts", "compiler-facts", fact_path)
            stages.append(dict(id=f"{id}:extract", parents=[tu["prepared"], "compiler", "adapter"], outputs=[fact_id,*outputs], recipe="recipe", result="success"))
            tu["status"] = "success"
        except KeyboardInterrupt:
            cancelled = True
            result["gaps"].append(dict(unit=id, stage="cancelled", reason="extraction cancelled; partial coverage retained"))
        except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as e:
            result["gaps"].append(dict(unit=id, stage="extraction", reason=str(e)))
            # Missing source has no bytes to identify: retain a missing-file artifact.
            if not any(a["id"] == tu["source"] for a in artifacts):
                artifact(tu["source"], "missing-source", source, b"")
        result["units"].append(tu)
        event("unit-complete", id, unitAccepted=tu["status"] == "success", gaps=len(result["gaps"]), cancelled=cancelled)
        if cancelled:
            for pending in config["units"][len(result["units"]):]:
                source = (cwd / pending["source"]).resolve()
                source_id = artifact(pending["id"] + ":source", "source", source, source.read_bytes() if source.exists() else b"")
                result["units"].append(dict(id=pending["id"], source=source_id, prepared=None, status="unexamined"))
                result["gaps"].append(dict(unit=pending["id"], stage="cancelled", reason="unexamined after cancellation"))
            break
    atomic(output, json_bytes(result))
    atomic(recipe_path.parent / "cache-report.json", json_bytes(dict(statistics=statistics, units=cache_units,
           elapsedSeconds=time.monotonic()-started, scope="AST/fact extraction only; preprocessing and target discovery always run")))
    event("complete", findingsFile=str(output), gaps=len(result["gaps"]), cancelled=cancelled)
    return 1 if result["gaps"] else 0


if __name__ == "__main__":
    try:
        if len(sys.argv) != 3:
            raise ValueError(__doc__)
        sys.exit(produce(sys.argv[1], sys.argv[2]))
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as e:
        print(str(e), file=sys.stderr)
        sys.exit(1)
