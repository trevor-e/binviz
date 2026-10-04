//! Storage descriptions are reviewed inputs, never an inferred safety proof.
//! Native observations reuse mipsflow; linked observations reuse the WASM reader.
use crate::{
    campaign::ByteRange,
    evidence::{IdentityState, hex},
    workspace::Workspace,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageDescription {
    pub id: String,
    pub caller: String,
    pub artifact: String,
    pub dependencies: Vec<String>,
    pub objects: Vec<Object>,
    pub accesses: Vec<Access>,
    pub frontiers: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Object {
    pub id: String,
    pub source: String,
    pub bytes: String,
    pub alignment: String,
    pub reserved_bytes: String,
    pub initialized: Vec<ByteRange>,
    pub lifetime: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Access {
    pub object: String,
    pub call: String,
    pub provider: String,
    pub kind: String,
    /// Negative offsets are meaningful and must not be silently clamped.
    pub offset: i64,
    pub bytes: Option<String>,
    pub rationale: String,
}

pub fn validate(w: &Workspace) -> Result<(), String> {
    let artifacts: BTreeSet<_> = w.evidence.artifacts.iter().map(|a| &a.id).collect();
    let mut ids = BTreeSet::new();
    for d in &w.storage {
        if d.id.is_empty()
            || !ids.insert(&d.id)
            || !artifacts.contains(&d.artifact)
            || d.dependencies.is_empty()
            || d.dependencies.iter().any(|a| !artifacts.contains(a))
            || !w
                .compiler_facts
                .definitions
                .iter()
                .any(|c| c.id == d.caller && c.has_body)
        {
            return Err("invalid storage identity, caller or dependency".into());
        }
        let mut objects = BTreeSet::new();
        for o in &d.objects {
            let size = hex(&o.bytes)?;
            let alignment = hex(&o.alignment)?;
            if o.id.is_empty()
                || !objects.insert(&o.id)
                || o.source.is_empty()
                || o.lifetime.is_empty()
                || size == 0
                || !alignment.is_power_of_two()
                || hex(&o.reserved_bytes)? < size
            {
                return Err("invalid object size, reservation, alignment or lifetime".into());
            }
            let mut initialized = Vec::new();
            for r in &o.initialized {
                let at = hex(&r.offset)?;
                let end = at.checked_add(hex(&r.bytes)?).ok_or("initialization overflow")?;
                if at == end || end > size || initialized.iter().any(|&(s, e)| at < e && s < end) {
                    return Err("initialization outside object or overlapping".into());
                }
                initialized.push((at, end));
            }
        }
        for a in &d.accesses {
            if !objects.contains(&a.object)
                || !matches!(a.kind.as_str(), "read" | "write" | "read-write")
                || a.rationale.trim().is_empty()
                || !w.bindings.iter().any(|b| {
                    b.call == a.call
                        && b.provider == a.provider
                        && w.compiler_facts
                            .calls
                            .iter()
                            .any(|c| c.id == a.call && c.caller == d.caller)
                })
            {
                return Err("access needs an exact caller/site/provider and rationale".into());
            }
            if let Some(size) = &a.bytes {
                if hex(size)? == 0 {
                    return Err("empty provider access".into());
                }
            }
        }
    }
    Ok(())
}

pub fn describe(
    w: &Workspace,
    files: &BTreeMap<String, Vec<u8>>,
    report: &crate::workspace::WorkspaceReport,
) -> Result<Value, String> {
    let checks = &report.inventory.identity_checks;
    let current = |id: &str| checks.iter().any(|c| c.id == id && c.state == IdentityState::Verified);
    let mut descriptions = Vec::new();
    for d in &w.storage {
        let mut reasons = Vec::new();
        if !current(&d.artifact)
            || files
                .get(&d.artifact)
                .and_then(|b| serde_json::from_slice::<Value>(b).ok())
                != Some(json!(d))
        {
            reasons.push("storage description does not match its current review artifact".to_string());
        }
        for id in &d.dependencies {
            if !current(id) {
                reasons.push(format!("storage dependency {id} is not current"));
            }
        }
        let caller = report.callers.iter().find(|c| c.id == d.caller);
        let mut accesses = Vec::new();
        for a in &d.accesses {
            let call = caller.and_then(|c| c.calls.iter().find(|c| c.call.id == a.call));
            let binding = w.bindings.iter().find(|b| b.call == a.call).unwrap();
            let required = [&binding.object, &binding.module, &binding.correspondence_artifact];
            if required.iter().any(|id| !d.dependencies.contains(id)) {
                reasons.push(format!(
                    "{} omits selected provider object/module/correspondence lineage",
                    a.call
                ));
            }
            if call.is_none_or(|c| !c.reasons.is_empty()) {
                reasons.push(format!("{} selected provider correspondence is unresolved", a.call));
            }
            let object = d.objects.iter().find(|o| o.id == a.object).unwrap();
            let size = hex(&object.bytes)?;
            let end = a
                .bytes
                .as_ref()
                .map(|n| hex(n).map(|n| i128::from(a.offset) + i128::from(n)))
                .transpose()?;
            let outside = a.offset < 0 || end.is_some_and(|end| end > i128::from(size));
            let initialized = if matches!(a.kind.as_str(), "read" | "read-write") && a.offset >= 0 {
                end.map(|end| {
                    let mut ranges: Vec<_> = object
                        .initialized
                        .iter()
                        .map(|r| {
                            (
                                hex(&r.offset).unwrap(),
                                hex(&r.offset).unwrap() + hex(&r.bytes).unwrap(),
                            )
                        })
                        .collect();
                    ranges.sort();
                    let mut at = a.offset as u64;
                    for (start, limit) in ranges {
                        if start <= at && limit > at {
                            at = limit;
                        }
                    }
                    i128::from(at) >= end
                })
            } else {
                None
            };
            accesses.push(json!({"access":a,"objectBytes":object.bytes,"reservedBytes":object.reserved_bytes,"outsideObject":outside,"readInitialized":initialized,"state":if outside{"out-of-bounds"}else if end.is_none(){"unknown-access-extent"}else if initialized==Some(false){"reads-uninitialized-bytes"}else{"within-described-object"}}));
        }
        descriptions.push(json!({"description":d,"identityState":if reasons.is_empty(){"verified"}else{"unverified"},"authority":"reviewed description; content identity does not prove access completeness","reasons":reasons,"accesses":accesses,"closureComplete":false,"frontiers":d.frontiers}));
    }
    let mut native = Vec::new();
    for unit in &report.inventory.units {
        for f in &unit.functions {
            if unit.architecture != "ps1-mipsel" || f.retired {
                continue;
            }
            let Some(span) = &f.function.identity.analysis_extent else {
                continue;
            };
            let Some(raw) = f.function.analysis_artifact.as_ref().and_then(|id| files.get(id)) else {
                continue;
            };
            if raw.len() % 4 != 0 {
                continue;
            }
            let words: Vec<_> = raw
                .chunks_exact(4)
                .map(|b| crate::cpu::mips::MipsWord(u32::from_le_bytes(b.try_into().unwrap())))
                .collect();
            let base = hex(&span.start)?;
            let flow = crate::mipsflow::scan(&words, base, &|_| false, None);
            let transitions:Vec<_>=words.iter().enumerate().filter(|(_,w)|w.writes()==Some(29)).map(|(i,w)|json!({"pc":format!("0x{:x}",base+i as u64*4),"word":format!("0x{:08x}",w.0),"delta":if matches!(w.op(),8|9)&&w.rs()==29{Some(w.simm())}else{None}})).collect();
            let slots: Vec<_> = flow
                .slots
                .iter()
                .map(|s| json!({"offset":s.offset,"bytes":s.width,"store":s.store}))
                .collect();
            native.push(json!({"function":f.function.id,"unit":unit.unit.id,"extent":span,"identityState":f.state,"spTransitions":transitions,"inferredFrameReservation":format!("0x{:x}",flow.frame),"stackSlots":slots,"savedRegisters":flow.saved,"addressTakenOffsets":flow.taken,"authority":"address-order observations from existing mipsflow; no path or closure proof","upperBound":null}));
        }
    }
    let linked:Vec<_>=report.modules.iter().map(|(id,module)|json!({"artifact":id,"sha256":module.sha256,"functions":module.functions.iter().map(|f|json!({"index":f.index,"names":f.names,"stackPointerOperations":f.stack_pointer_operations,"directCalls":f.calls,"indirectCalls":f.indirect_calls,"imported":f.imported})).collect::<Vec<_>>(),"upperBound":null,"frontiers":["dynamic frames, recursion, indirect/host calls and IRQ/reentrancy require complete context evidence"]})).collect();
    Ok(
        json!({"objects":descriptions,"native":native,"linked":linked,"compilerAllocationsAndLifetimes":w.compiler_facts.compiler_storage,"closureCertificates":report.proof_closures,"closureComplete":false}),
    )
}
