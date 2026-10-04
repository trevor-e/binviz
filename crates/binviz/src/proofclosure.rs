//! Scoped linked-provider, shared-memory and callback/stack certificates.
//! A review selects a finite model; actual bytes and execution reports validate it.
use crate::{
    evidence::{IdentityState, hex, sha256},
    linkevidence::{ImportIdentity, ModuleReport, Region},
    workspace::Workspace,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub dependencies: BTreeMap<String, String>,
    pub review_artifact: String,
    pub claim: Claim,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Claim {
    ServiceChain {
        module: String,
        root: u32,
        edges: Vec<Edge>,
        service: ImportIdentity,
        obligations: Vec<Obligation>,
    },
    SharedInitialization {
        modules: Vec<String>,
        memory_import: String,
        memory_bytes: String,
        maximum_bytes: String,
        live_regions: Vec<LiveRegion>,
        obligations: Vec<Obligation>,
    },
    CallbackStack {
        module: String,
        roots: Vec<u32>,
        indirect_targets: Vec<Indirect>,
        external_frames: Vec<ExternalFrame>,
        recursion_bounds: BTreeMap<u32, usize>,
        interrupt_roots: Vec<u32>,
        interrupt_depth: usize,
        stack_bytes: String,
    },
    ScalarVarargs {
        call: String,
        promoted_types: Vec<crate::compilerfacts::CompilerType>,
        argument_home_bytes: String,
        native_home_prefix: crate::evidence::NativeSpan,
        obligations: Vec<Obligation>,
    },
    MissingInputs {
        call: String,
        candidate_call: String,
        obligations: Vec<Obligation>,
    },
    DataClosure {
        module: String,
        objects: Vec<String>,
        bindings: Vec<DataBinding>,
        obligations: Vec<Obligation>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataBinding {
    pub symbol: String,
    pub export: String,
    pub bytes: String,
    pub external_profile: Option<String>,
    pub alias_of: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edge {
    pub caller: u32,
    pub offset: String,
    pub target: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Obligation {
    pub campaign: String,
    pub cases: Vec<String>,
    pub checkpoint: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveRegion {
    pub module: String,
    pub region: Region,
    pub established_before: String,
    pub restoration: Option<Obligation>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Indirect {
    pub caller: u32,
    pub offset: String,
    pub targets: Vec<u32>,
    pub return_use: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExternalFrame {
    pub function: u32,
    pub bytes: String,
    pub profile: String,
}

pub(crate) fn obligation(o: &Obligation, files: &BTreeMap<String, Vec<u8>>) -> Result<Value, String> {
    if o.cases.is_empty() || o.checkpoint.trim().is_empty() {
        return Err("execution obligation needs exact cases and checkpoint".into());
    }
    let raw = files.get(&o.campaign).ok_or("execution obligation campaign missing")?;
    let report = crate::campaign::audit_report(raw, Some(files))?;
    if report["observationsBound"] != true
        || report["unexamined"] != 0
        || report["failed"] != 0
        || report["refused"] != 0
        || report["identityChecks"]
            .as_array()
            .is_none_or(|a| a.iter().any(|i| i["state"] != "verified"))
    {
        return Err("execution obligation is unbound, stale, incomplete or failed".into());
    }
    let input = &report["input"];
    let roles = if input["configuration"].get("baseline").is_some() {
        ["baseline", "candidate"]
    } else {
        ["native", "wasm"]
    };
    for id in &o.cases {
        let case = input["cases"]
            .as_array()
            .and_then(|c| c.iter().find(|c| c["id"] == *id))
            .ok_or("required executed case missing")?;
        for role in roles {
            if !matches!(case[role]["status"].as_str(), Some("executed" | "exception"))
                || case[role]["checkpoints"].get(&o.checkpoint).is_none()
            {
                return Err("required actual execution checkpoint missing".into());
            }
        }
    }
    Ok(json!({"campaign":o.campaign,"cases":o.cases,"checkpoint":o.checkpoint,"runners":report["runners"]}))
}
fn span(r: &Region) -> Result<(u64, u64), String> {
    let s = hex(&r.start)?;
    let e = s.checked_add(hex(&r.bytes)?).ok_or("region overflow")?;
    if e == s {
        return Err("empty region".into());
    }
    Ok((s, e))
}
fn overlap(a: (u64, u64), b: (u64, u64)) -> bool {
    a.0 < b.1 && b.0 < a.1
}
fn restoration(
    o: &Obligation,
    live: &LiveRegion,
    module: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    obligation(o, files)?;
    let raw: Value = serde_json::from_slice(&files[&o.campaign]).map_err(|e| e.to_string())?;
    let input = if raw["format"] == "binviz-campaign-report" {
        &raw["input"]
    } else {
        &raw
    };
    let roles = if input["configuration"].get("baseline").is_some() {
        ["baseline", "candidate"]
    } else {
        ["native", "wasm"]
    };
    for id in &o.cases {
        let case = input["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == *id)
            .unwrap();
        for role in roles {
            let checkpoint = &case[role]["checkpoints"][&o.checkpoint];
            if checkpoint["module"] != module
                || checkpoint["region"] != live.region.name
                || checkpoint["start"] != live.region.start
                || checkpoint["phase"] != "after-instantiation-restoration"
            {
                return Err("restoration checkpoint differs from exact module/range/phase".into());
            }
            let before = crate::campaign::bytes(
                checkpoint["before"]
                    .as_str()
                    .ok_or("saved pre-instantiation bytes missing")?,
            )?;
            let after = crate::campaign::bytes(checkpoint["after"].as_str().ok_or("post-restoration bytes missing")?)?;
            if before.len() as u64 != hex(&live.region.bytes)? || before != after {
                return Err("shared-memory saved/restored bytes differ or omit part of the protected range".into());
            }
            let events = case[role]["events"]
                .as_array()
                .ok_or("restoration event ordering missing")?;
            let save = events
                .iter()
                .position(|e| e["kind"] == "checkpoint" && e["provider"] == format!("{}:saved", o.checkpoint));
            let init = events
                .iter()
                .position(|e| e["kind"] == "instantiate" && e["provider"] == module);
            let restore = events
                .iter()
                .position(|e| e["kind"] == "checkpoint" && e["provider"] == o.checkpoint);
            if !matches!((save,init,restore),(Some(a),Some(b),Some(c)) if a<b&&b<c) {
                return Err("save/instantiate/restore phases are missing or out of order".into());
            }
        }
    }
    Ok(())
}
fn module<'a>(id: &str, modules: &'a BTreeMap<String, ModuleReport>) -> Result<&'a ModuleReport, String> {
    let m = modules.get(id).ok_or("selected linked module not inspected")?;
    if !m.problems.is_empty() {
        return Err("selected module has decoding frontiers".into());
    }
    Ok(m)
}
/// The observed WASM stack adjustment is authoritative for the linked stack.
/// LLVM alloca size is separately reported and never substituted for a frame.
fn frame(f: &crate::linkevidence::LinkedFunction) -> Result<u64, String> {
    let mut largest = 0;
    let mut allocations = 0;
    let mut active = false;
    for op in &f.stack_pointer_operations {
        if op["operation"] != "global.set" {
            continue;
        }
        let context = op["context"].as_array().ok_or("stack operation context missing")?;
        let at = context
            .iter()
            .position(|i| i["offset"] == op["offset"])
            .ok_or("stack operation absent from context")?;
        let mut prev = &context[..at];
        if prev.last().is_some_and(|i| i["operation"] == "local.tee") {
            prev = &prev[..prev.len() - 1];
        }
        let Some(arithmetic) = prev.last() else {
            return Err("dynamic stack adjustment".into());
        };
        if arithmetic["operation"] == "local.get" {
            let local = arithmetic["immediate"].clone();
            if f.stack_pointer_operations.iter().any(|get| {
                get["operation"] == "global.get"
                    && get["controlBefore"] != true
                    && get["offset"].as_str().and_then(|s| hex(s).ok()).is_some_and(|at| {
                        f.stack_pointer_operations
                            .iter()
                            .find(|op| op["operation"] == "global.set")
                            .and_then(|op| op["offset"].as_str())
                            .and_then(|s| hex(s).ok())
                            .is_some_and(|first| at < first)
                    })
                    && get["localWrites"]
                        .as_array()
                        .is_some_and(|w| w.iter().filter(|v| v["local"] == local).count() == 1)
                    && get["context"].as_array().is_some_and(|c| {
                        c.windows(2).any(|p| {
                            p[0]["offset"] == get["offset"]
                                && matches!(p[1]["operation"].as_str(), Some("local.set" | "local.tee"))
                                && p[1]["immediate"] == local
                        })
                    })
            }) {
                active = false;
                continue;
            }
        }
        if !matches!(arithmetic["operation"].as_str(), Some("i32.sub" | "i32.add")) {
            return Err("dynamic stack assignment needs a compiler frame certificate".into());
        }
        let c = prev.get(prev.len().saturating_sub(2)).ok_or("stack constant missing")?;
        if c["operation"] != "i32.const" {
            return Err("dynamic stack adjustment".into());
        }
        let immediate = c["immediate"].as_str().unwrap_or("");
        let n = immediate
            .strip_prefix("I32(")
            .and_then(|s| s.strip_suffix(')'))
            .and_then(|s| s.parse::<i32>().ok())
            .ok_or("unsupported stack constant")?;
        let mut operand = &prev[..prev.len().saturating_sub(2)];
        if operand.last().is_some_and(|i| i["operation"] == "local.tee") {
            operand = &operand[..operand.len() - 1];
        }
        if !operand.last().is_some_and(|i| {
            i["operation"] == "global.get"
                && f.stack_pointer_operations
                    .iter()
                    .any(|get| get["operation"] == "global.get" && get["offset"] == i["offset"])
        }) {
            return Err("stack adjustment operand is not the actual stack pointer".into());
        }
        if arithmetic["operation"] == "i32.sub" {
            allocations += 1;
            if allocations > 1 || op["controlBefore"] == true || n < 0 {
                return Err(
                    "non-prologue or repeated stack allocation requires a path-sensitive frame certificate".into(),
                );
            }
            active = true;
        } else if n < 0 || largest != n as u64 {
            return Err("stack restore differs from constant frame reservation".into());
        } else {
            active = false;
        }
        largest = largest.max(n.unsigned_abs() as u64);
    }
    if allocations > 0 {
        if active || f.stack_pointer_operations.iter().any(|op| op["hasControl"] == true) {
            return Err("stack restoration paths require a path-sensitive frame certificate".into());
        }
        let last = f
            .stack_pointer_operations
            .iter()
            .rev()
            .find(|op| op["operation"] == "global.set")
            .unwrap();
        let restored_at = hex(last["offset"].as_str().ok_or("stack restoration offset missing")?)?;
        if last["exits"].as_array().is_none_or(|exits| {
            exits
                .iter()
                .any(|at| at.as_str().and_then(|s| hex(s).ok()).is_none_or(|at| at < restored_at))
        }) {
            return Err("stack restoration does not precede every return".into());
        }
    }
    Ok(largest)
}
fn callback_stack(
    m: &ModuleReport,
    roots: &[u32],
    indirect: &[Indirect],
    external: &[ExternalFrame],
    bounds: &BTreeMap<u32, usize>,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(u64, BTreeSet<u32>), String> {
    fn walk(
        m: &ModuleReport,
        index: u32,
        indirect: &[Indirect],
        external: &[ExternalFrame],
        bounds: &BTreeMap<u32, usize>,
        files: &BTreeMap<String, Vec<u8>>,
        visits: &mut BTreeMap<u32, usize>,
        seen: &mut BTreeSet<u32>,
        budget: &mut usize,
    ) -> Result<u64, String> {
        if *budget == 0 {
            return Err("call closure state budget exhausted".into());
        }
        *budget -= 1;
        let count = visits.get(&index).copied().unwrap_or(0);
        if count > 0 && !bounds.contains_key(&index) {
            return Err("recursive call closure requires a reviewed finite depth".into());
        }
        if let Some(&bound) = bounds.get(&index) {
            if bound == 0 {
                return Err("zero recursion bound".into());
            }
            if count >= bound {
                return Ok(0);
            }
        }
        let f = m
            .functions
            .get(index as usize)
            .ok_or("callback target outside module")?;
        seen.insert(index);
        if f.imported {
            let p = external
                .iter()
                .find(|p| p.function == index)
                .ok_or("host import lacks stack/reentrancy profile")?;
            let profile: Value = serde_json::from_slice(files.get(&p.profile).ok_or("host stack profile missing")?)
                .map_err(|e| e.to_string())?;
            if profile["function"] != index
                || profile["parameters"] != json!(f.parameters)
                || profile["results"] != json!(f.results)
                || profile["stackBytes"] != p.bytes
                || profile["restoresStack"] != true
            {
                return Err("host stack profile differs from selected import signature/stack effects".into());
            }
            return hex(&p.bytes);
        }
        visits.insert(index, count + 1);
        let mut targets: BTreeSet<_> = f.calls.iter().map(|c| c.target).collect();
        for offset in &f.indirect_calls {
            let records: Vec<_> = indirect
                .iter()
                .filter(|c| c.caller == index && c.offset == *offset)
                .collect();
            if records.len() != 1 {
                return Err("indirect edge needs one complete reviewed target set".into());
            }
            let contract = records[0];
            if contract.targets.is_empty()
                || !matches!(contract.return_use.as_str(), "full-word" | "discarded" | "void")
            {
                return Err("callback needs finite targets and used-return contract".into());
            }
            let actual = f
                .indirect_signatures
                .get(offset)
                .ok_or("indirect reference type is not resolved")?;
            let mut signature = None;
            for target in &contract.targets {
                let t = m
                    .functions
                    .get(*target as usize)
                    .ok_or("callback target outside module")?;
                let sig = (&t.parameters, &t.results);
                if json!(t.parameters) != actual["parameters"] || json!(t.results) != actual["results"] {
                    return Err("callback target differs from actual call_indirect wire type".into());
                }
                if signature.is_some_and(|s| s != sig) {
                    return Err("callback targets have incompatible wire signatures".into());
                }
                signature = Some(sig);
                if contract.return_use == "void" && !t.results.is_empty() {
                    return Err("callback return contract disagrees with actual signature".into());
                }
                if contract.return_use == "full-word" && t.results != ["i32"] {
                    return Err("full-word callback return requires an actual i32 result".into());
                }
                targets.insert(*target);
            }
        }
        for contract in indirect.iter().filter(|i| i.caller == index) {
            let actual = f
                .indirect_signatures
                .get(&contract.offset)
                .ok_or("unused or unsupported indirect contract")?;
            for initializer in &m.table_initializers {
                if initializer["table"] == actual["table"] {
                    for target in initializer["functions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_u64)
                    {
                        let t = m
                            .functions
                            .get(target as usize)
                            .ok_or("table initializer target outside module")?;
                        if json!(t.parameters) == actual["parameters"]
                            && json!(t.results) == actual["results"]
                            && !contract.targets.contains(&(target as u32))
                        {
                            return Err("callback set omits actual compatible table initializer".into());
                        }
                    }
                }
            }
        }
        let mut child = 0;
        for t in targets {
            child = child.max(walk(m, t, indirect, external, bounds, files, visits, seen, budget)?)
        }
        visits.insert(index, count);
        frame(f)?
            .checked_add(child)
            .ok_or_else(|| "stack closure overflow".into())
    }
    if roots.is_empty() {
        return Err("call closure needs roots".into());
    }
    let mut seen = BTreeSet::new();
    let mut maximum = 0;
    let mut budget = 100_000;
    for r in roots {
        maximum = maximum.max(walk(
            m,
            *r,
            indirect,
            external,
            bounds,
            files,
            &mut BTreeMap::new(),
            &mut seen,
            &mut budget,
        )?)
    }
    Ok((maximum, seen))
}
fn check(
    w: &Workspace,
    r: &Request,
    files: &BTreeMap<String, Vec<u8>>,
    modules: &BTreeMap<String, ModuleReport>,
) -> Result<Value, String> {
    let require = |id: &str| {
        if r.dependencies.contains_key(id) {
            Ok(())
        } else {
            Err(format!("certificate omits dependency {id}"))
        }
    };
    let obligations = |items: &[Obligation]| -> Result<Vec<Value>, String> {
        items
            .iter()
            .map(|o| {
                require(&o.campaign)?;
                obligation(o, files)
            })
            .collect()
    };
    match &r.claim {
        Claim::DataClosure {
            module: id,
            objects,
            bindings,
            obligations: items,
        } => {
            require(id)?;
            let m = module(id, modules)?;
            for artifact in objects.iter().chain(std::iter::once(id)) {
                for ancestor in w.evidence.dependency_closure(artifact)? {
                    require(&ancestor)?;
                }
            }
            if objects.is_empty()
                || bindings.is_empty()
                || bindings.iter().map(|b| &b.symbol).collect::<BTreeSet<_>>().len() != bindings.len()
            {
                return Err("data closure needs distinct bindings and actual objects".into());
            }
            let mut referenced = BTreeSet::new();
            for object in objects {
                require(object)?;
                let o = crate::linkevidence::inspect(files.get(object).ok_or("data object missing")?)?;
                for s in o.data_symbols {
                    let name = s["name"].as_str().ok_or("unnamed data symbol")?.to_owned();
                    referenced.insert(name.clone());
                    let b = bindings.iter().find(|b| b.symbol == name).ok_or_else(|| {
                        format!("object DATA reference {name} has no real definition or reviewed external provider")
                    })?;
                    if let Some(bytes) = s["storage"]["bytes"].as_str() {
                        if hex(&b.bytes)? < hex(bytes)? {
                            return Err("DATA reservation is smaller than actual object definition".into());
                        }
                    }
                }
            }
            let mut ranges: Vec<(&DataBinding, u64, u64)> = vec![];
            for b in bindings {
                if !referenced.contains(&b.symbol) {
                    return Err("data binding is absent from actual object symbols".into());
                }
                let start = hex(m
                    .globals
                    .get(&b.export)
                    .ok_or("DATA lacks an actual linked address export")?)?;
                let size = hex(&b.bytes)?;
                if size == 0 {
                    return Err("empty DATA reservation".into());
                }
                let end = start.checked_add(size).ok_or("DATA overflow")?;
                if m.memory_min_bytes.len() != 1
                    || m.memory_min_bytes
                        .first()
                        .and_then(|n| hex(n).ok())
                        .is_none_or(|n| end > n)
                {
                    return Err("DATA backing lies outside actual module memory".into());
                }
                if let Some(profile) = &b.external_profile {
                    require(profile)?;
                    let p: Value = serde_json::from_slice(&files[profile]).map_err(|e| e.to_string())?;
                    if p["symbol"] != b.symbol
                        || p["address"] != format!("0x{start:x}")
                        || p["bytes"] != b.bytes
                        || p["provider"].as_str().is_none_or(str::is_empty)
                        || items.is_empty()
                    {
                        return Err(
                            "external DATA provider needs exact backing profile and executed obligations".into(),
                        );
                    }
                } else if start == 0
                    || m.globals
                        .get("__data_end")
                        .and_then(|s| hex(s).ok())
                        .is_none_or(|n| end > n)
                {
                    return Err("DATA resolved to zero or lacks actual allocated backing extent".into());
                }
                for (other, a, z) in &ranges {
                    if overlap((start, end), (*a, *z))
                        && !(b.alias_of.as_ref() == Some(&other.symbol) || other.alias_of.as_ref() == Some(&b.symbol))
                    {
                        return Err("distinct required DATA backing locations overlap".into());
                    }
                }
                ranges.push((b, start, end));
            }
            for b in bindings {
                if let Some(alias) = &b.alias_of {
                    if !ranges.iter().any(|(a, s, e)| {
                        a.symbol == *alias
                            && m.globals.get(&b.export).and_then(|v| hex(v).ok()) == Some(*s)
                            && hex(&b.bytes).ok() == Some(e - s)
                    }) {
                        return Err("DATA alias lacks identical actual backing lineage".into());
                    }
                }
            }
            Ok(
                json!({"objects":objects,"referencedData":referenced,"backing":ranges,"obligations":obligations(items)?}),
            )
        }
        Claim::ServiceChain {
            module: id,
            root,
            edges,
            service,
            obligations: items,
        } => {
            require(id)?;
            let m = module(id, modules)?;
            let mut index = *root;
            if edges.is_empty() || items.is_empty() {
                return Err("service chain requires actual edges and service obligations".into());
            }
            for e in edges {
                hex(&e.offset)?;
                if e.caller != index
                    || !m
                        .functions
                        .get(index as usize)
                        .is_some_and(|f| f.calls.iter().any(|c| c.offset == e.offset && c.target == e.target))
                {
                    return Err("selected wrapper/provider chain differs from actual linked call".into());
                }
                index = e.target;
            }
            if m.functions.get(index as usize).and_then(|f| f.import_identity.as_ref()) != Some(service) {
                return Err("service module/field/signature differs from actual selected import".into());
            }
            for o in items {
                let campaign = crate::campaign::audit_report(
                    files.get(&o.campaign).ok_or("service campaign missing")?,
                    Some(files),
                )?;
                let config = &campaign["input"]["configuration"];
                if !["baseline", "candidate", "native", "wasm"]
                    .iter()
                    .any(|role| config[*role]["profile"]["moduleArtifact"] == *id)
                {
                    return Err("service obligation did not execute the selected linked module identity".into());
                }
                for case in campaign["input"]["cases"]
                    .as_array()
                    .ok_or("service observations missing")?
                    .iter()
                    .filter(|c| o.cases.iter().any(|id| c["id"] == *id))
                {
                    if !["baseline", "candidate", "native", "wasm"].iter().any(|role| {
                        case[*role]["events"].as_array().is_some_and(|events| {
                            events.iter().any(|e| {
                                e["provider"] == service.field
                                    && e["arguments"]
                                        .as_array()
                                        .is_some_and(|a| a.len() == service.parameters.len())
                            })
                        })
                    }) {
                        return Err(
                            "service obligation lacks an actual selected provider event with its true arity".into(),
                        );
                    }
                }
            }
            Ok(json!({"selectedImport":service,"edges":edges,"obligations":obligations(items)?}))
        }
        Claim::SharedInitialization {
            modules: ids,
            memory_import,
            memory_bytes,
            maximum_bytes,
            live_regions,
            obligations: items,
        } => {
            let size = hex(memory_bytes)?;
            let max = hex(maximum_bytes)?;
            if size > max || ids.is_empty() || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
                return Err("invalid shared memory instance/order".into());
            }
            let mut writes = vec![];
            let mut stack_regions = vec![];
            let mut sharing = None;
            for (phase, id) in ids.iter().enumerate() {
                require(id)?;
                let m = module(id, modules)?;
                if m.memory_imports != [memory_import.clone()]
                    || m.memory_min_bytes.len() != 1
                    || hex(&m.memory_min_bytes[0])? > size
                    || m.memory_max_bytes[0]
                        .as_ref()
                        .is_some_and(|n| hex(n).is_ok_and(|n| max > n))
                {
                    return Err("shared instance identity/minimum/maximum incompatible with module".into());
                }
                if sharing.as_ref().is_some_and(|s| *s != m.shared_memory) {
                    return Err("module memory sharing modes differ for the selected shared instance".into());
                }
                sharing = Some(m.shared_memory.clone());
                if m.start_function.is_some() && items.is_empty() {
                    return Err("start function effects require initialization execution evidence".into());
                }
                for live in live_regions {
                    let established = ids
                        .iter()
                        .position(|id| *id == live.established_before)
                        .ok_or("unknown live-region establishment phase")?;
                    if m.start_function.is_some() && established <= phase {
                        let restore = live.restoration.as_ref().ok_or(
                            "start-function effects on live memory require exact execution restoration evidence",
                        )?;
                        require(&restore.campaign)?;
                        restoration(restore, live, id, files)?;
                    }
                }
                let layouts: Vec<_> = w.layouts.iter().filter(|l| l.module == *id).collect();
                if layouts.len() != 1 {
                    return Err("shared initialization needs one reviewed layout per module".into());
                }
                for layout in layouts {
                    if !r.dependencies.keys().any(|key| {
                        files.get(key).and_then(|b| serde_json::from_slice::<Value>(b).ok()) == Some(json!(layout))
                    }) {
                        return Err("shared initialization layout is absent from pinned reviewed dependencies".into());
                    }
                    if !layout.verify(m)?.is_empty() {
                        return Err("shared initialization layout is inconsistent with actual module".into());
                    }
                    for region in layout.reserved.iter().filter(|s| s.name == "stack") {
                        stack_regions.push((id, span(region)?));
                    }
                }
                for write in &m.initialization_writes {
                    let s = hex(write.address.as_ref().ok_or("unknown active initialization offset")?)?;
                    let end = s.checked_add(hex(&write.bytes)?).ok_or("initialization overflow")?;
                    if end > size {
                        return Err("initialization outside shared memory".into());
                    }
                    for live in live_regions {
                        if !ids.contains(&live.module) {
                            return Err("protected region owner is outside instantiation order".into());
                        }
                        let established = ids
                            .iter()
                            .position(|id| *id == live.established_before)
                            .ok_or("unknown live-region establishment phase")?;
                        if established <= phase && overlap((s, end), span(&live.region)?) {
                            let restore = live
                                .restoration
                                .as_ref()
                                .ok_or("later initialization overwrites live shared memory")?;
                            require(&restore.campaign)?;
                            restoration(restore, live, id, files)?;
                        }
                    }
                    writes.push(json!({"phase":phase,"module":id,"write":write}));
                }
            }
            for (i, a) in stack_regions.iter().enumerate() {
                for b in &stack_regions[i + 1..] {
                    if overlap(a.1, b.1) {
                        return Err("cross-module stack reservations overlap".into());
                    }
                }
            }
            Ok(
                json!({"instantiationOrder":ids,"writes":writes,"stackRegions":stack_regions,"obligations":obligations(items)?}),
            )
        }
        Claim::CallbackStack {
            module: id,
            roots,
            indirect_targets,
            external_frames,
            recursion_bounds,
            interrupt_roots,
            interrupt_depth,
            stack_bytes,
        } => {
            require(id)?;
            for f in external_frames {
                require(&f.profile)?
            }
            let m = module(id, modules)?;
            let (normal, mut seen) =
                callback_stack(m, roots, indirect_targets, external_frames, recursion_bounds, files)?;
            let irq = if interrupt_roots.is_empty() {
                0
            } else {
                if *interrupt_depth == 0 {
                    return Err("interrupt roots need a reviewed nesting bound".into());
                }
                let (n, visited) = callback_stack(
                    m,
                    interrupt_roots,
                    indirect_targets,
                    external_frames,
                    recursion_bounds,
                    files,
                )?;
                seen.extend(visited);
                n
            };
            if indirect_targets.iter().any(|i| {
                !seen.contains(&i.caller)
                    || !m
                        .functions
                        .get(i.caller as usize)
                        .is_some_and(|f| f.indirect_calls.contains(&i.offset))
            }) {
                return Err("reviewed indirect contract is outside selected normal/interrupt closure".into());
            }
            let maximum = irq
                .checked_mul(*interrupt_depth as u64)
                .and_then(|n| normal.checked_add(n))
                .ok_or("interrupt stack overflow")?;
            if maximum > hex(stack_bytes)? {
                return Err("complete call/callback/interrupt stack exceeds reservation".into());
            }
            if !w.layouts.iter().any(|l| {
                l.module == *id
                    && l.reserved
                        .iter()
                        .any(|s| s.name == "stack" && hex(&s.bytes).ok() == hex(stack_bytes).ok())
            }) {
                return Err("stack limit is not the actual checked layout reservation".into());
            }
            Ok(
                json!({"reachableFunctions":seen,"normalBytes":format!("0x{normal:x}"),"interruptBytes":format!("0x{irq:x}"),"maximumBytes":format!("0x{maximum:x}"),"reservedBytes":stack_bytes,"model":"reviewed finite callbacks/recursion/interrupt nesting"}),
            )
        }
        Claim::ScalarVarargs {
            call,
            promoted_types,
            argument_home_bytes,
            native_home_prefix,
            obligations: items,
        } => {
            let c = w
                .compiler_facts
                .calls
                .iter()
                .find(|c| c.id == *call)
                .ok_or("unknown vararg call")?;
            require(&c.span.artifact)?;
            if c.arguments.iter().map(|a| &a.promoted).collect::<Vec<_>>() != promoted_types.iter().collect::<Vec<_>>()
                || promoted_types
                    .iter()
                    .any(|t| !matches!(t.category.as_str(), "integer" | "pointer") || t.bits.is_none_or(|b| b > 32))
            {
                return Err("vararg profile differs from actual scalar promoted arguments".into());
            }
            if hex(argument_home_bytes)? < (c.arguments.len().max(4) * 4) as u64 || items.is_empty() {
                return Err("vararg profile needs complete O32 home prefix and execution evidence".into());
            }
            let binding = w
                .bindings
                .iter()
                .find(|b| b.call == *call)
                .ok_or("vararg provider correspondence missing")?;
            require(&binding.module)?;
            require(&binding.correspondence_artifact)?;
            let provider = w
                .inventory
                .units
                .iter()
                .flat_map(|u| &u.functions)
                .find(|f| f.id == binding.provider)
                .ok_or("vararg native provider missing")?;
            require(
                provider
                    .analysis_artifact
                    .as_ref()
                    .ok_or("vararg full native extent missing")?,
            )?;
            if provider.identity.analysis_extent.as_ref().is_none_or(|s| !s.exact) {
                return Err("vararg provider prefix/shared-tail extent is not exact".into());
            }
            let definition = w
                .compiler_facts
                .definitions
                .iter()
                .find(|d| d.id == binding.definition)
                .unwrap();
            let fixed = definition.abi_parameters.len();
            if !definition.variadic
                || fixed >= 4
                || c.arguments.len() < fixed
                || !native_home_prefix.exact
                || native_home_prefix.start != provider.identity.entry
                || hex(&native_home_prefix.bytes)? != (4 - fixed) as u64 * 4
            {
                return Err(
                    "vararg native home prefix must cover actual incoming O32 register stores at provider entry".into(),
                );
            }
            let extent = provider.identity.analysis_extent.as_ref().unwrap();
            let offset = hex(&provider.identity.entry)?
                .checked_sub(hex(&extent.start)?)
                .ok_or("vararg entry precedes extent")? as usize;
            let bytes = &files[provider.analysis_artifact.as_ref().unwrap()];
            for (slot, arg) in (fixed..4).enumerate() {
                let word = bytes
                    .get(offset + slot * 4..offset + slot * 4 + 4)
                    .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                    .ok_or("vararg home prefix outside full native extent")?;
                if word >> 26 != 43
                    || (word >> 21) & 31 != 29
                    || (word >> 16) & 31 != (arg + 4) as u32
                    || word & 0xffff != (arg * 4) as u32
                {
                    return Err("actual native instructions differ from reviewed argument-home prefix".into());
                }
            }
            Ok(
                json!({"call":call,"promotedTypes":promoted_types,"argumentHomeBytes":argument_home_bytes,"nativeHomePrefix":native_home_prefix,"obligations":obligations(items)?}),
            )
        }
        Claim::MissingInputs {
            call,
            candidate_call,
            obligations: items,
        } => {
            let a = w
                .compiler_facts
                .calls
                .iter()
                .find(|c| c.id == *call)
                .ok_or("original missing-input call absent")?;
            let b = w
                .compiler_facts
                .calls
                .iter()
                .find(|c| c.id == *candidate_call)
                .ok_or("compiler-checked source candidate call absent")?;
            require(&a.span.artifact)?;
            require(&b.span.artifact)?;
            if a.id == b.id
                || a.callee != b.callee
                || a.arguments.len() >= b.arguments.len()
                || a.result_consumed != b.result_consumed
                || a.result_type != b.result_type
                || items.is_empty()
            {
                return Err("missing-input candidate needs an explicit longer call with the same selected callee/result contract and execution evidence".into());
            }
            for (old, new) in a.arguments.iter().zip(&b.arguments) {
                let text = |s: &crate::compilerfacts::SourceSpan| {
                    files
                        .get(&s.artifact)
                        .and_then(|b| b.get(s.start as usize..s.end as usize))
                };
                if old.promoted != new.promoted || text(&old.span).is_none() || text(&old.span) != text(&new.span) {
                    return Err("missing-input source candidate changes an existing argument expression/type".into());
                }
            }
            let binding = w
                .bindings
                .iter()
                .find(|b| b.call == *call)
                .ok_or("missing-input native/provider correspondence absent")?;
            require(&binding.module)?;
            require(&binding.correspondence_artifact)?;
            let definition = w
                .compiler_facts
                .definitions
                .iter()
                .find(|d| d.id == binding.definition)
                .unwrap();
            if definition.variadic
                || b.arguments.len() != definition.abi_parameters.len()
                || b.arguments
                    .iter()
                    .zip(&definition.abi_parameters)
                    .any(|(a, t)| a.promoted.category != t.category || a.promoted.bits != t.bits)
            {
                return Err("missing-input candidate does not satisfy the selected true compiler ABI".into());
            }
            Ok(
                json!({"originalCall":call,"candidateCall":candidate_call,"introducedArguments":&b.arguments[a.arguments.len()..],"obligations":obligations(items)?}),
            )
        }
    }
}
pub fn inspect(
    w: &Workspace,
    files: &BTreeMap<String, Vec<u8>>,
    modules: &BTreeMap<String, ModuleReport>,
) -> Result<Vec<Value>, String> {
    let checks = w.evidence.verify(Some(files))?;
    let mut ids = BTreeSet::new();
    let mut out = vec![];
    for r in &w.proof_closures {
        if r.id.is_empty() || !ids.insert(&r.id) {
            return Err("empty/duplicate proof closure identity".into());
        }
        let result = (|| {
            if r.dependencies.is_empty() {
                return Err("proof closure requires pinned dependencies".into());
            }
            for (id, pin) in &r.dependencies {
                if files.get(id).is_none_or(|b| sha256(b) != *pin)
                    || !checks.iter().any(|c| c.id == *id && c.state == IdentityState::Verified)
                {
                    return Err(format!("proof dependency {id} is stale or unverified"));
                }
            }
            if !checks
                .iter()
                .any(|c| c.id == r.review_artifact && c.state == IdentityState::Verified)
                || files
                    .get(&r.review_artifact)
                    .and_then(|b| serde_json::from_slice::<Value>(b).ok())
                    != Some(json!(r))
            {
                return Err("proof review artifact missing, stale or differs from exact request".into());
            }
            check(w, r, files, modules)
        })();
        out.push(match result {
            Ok(details) => json!({"id":r.id,"state":"verified","details":details,"request":r}),
            Err(reason) => json!({"id":r.id,"state":"refused","reasons":[reason],"request":r}),
        });
    }
    Ok(out)
}
