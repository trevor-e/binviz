//! Comparable observations from configured native/WASM runners; no new emulator.
use crate::evidence::hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Memory {
    pub id: String,
    pub start: String,
    pub data: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceEvent {
    pub kind: String,
    pub pc: String,
    pub provider: Option<String>,
    pub profile: Option<String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub address: Option<String>,
    pub value: Option<String>,
    #[serde(default)]
    pub delay_slot: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Execution {
    pub status: String,
    pub reason: Option<String>,
    pub instructions: u64,
    pub return_word: Option<String>,
    pub ram: Vec<Memory>,
    pub registers: BTreeMap<String, String>,
    pub events: Vec<TraceEvent>,
    pub checkpoints: BTreeMap<String, Value>,
    pub coverage: Vec<String>,
    #[serde(default)]
    pub local_objects: Vec<LocalObject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant: Option<Participant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exception: Option<Exception>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub void_profile: Option<VoidProfile>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_state: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VoidProfile {
    pub compiler_facts: String,
    pub definition: String,
    pub module: String,
    pub function: u32,
    pub build_evidence: String,
    pub review_artifact: String,
}
/// Roles identify what ran; two WASM participants never imply native device authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Participant {
    pub role: String,
    pub architecture: String,
    pub runner_kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exception {
    pub kind: String,
    pub operation: String,
    pub guest_pc: Option<String>,
    /// State recorded at the exception, before any runner recovery or cleanup.
    pub checkpoint: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ByteRange {
    pub offset: String,
    pub bytes: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalObject {
    pub id: String,
    pub base: String,
    pub bytes: String,
    pub data: String,
    pub initialized: Vec<ByteRange>,
    pub first_event: usize,
    pub last_event: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointerArgument {
    pub event: usize,
    pub argument: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorrespondingObject {
    pub id: String,
    pub max_bytes: String,
    pub rationale: String,
    pub pointer_arguments: Vec<PointerArgument>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryMask {
    pub region: String,
    pub range: ByteRange,
    pub rationale: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Comparison {
    pub return_mask: String,
    pub ram: Vec<String>,
    pub registers: Vec<String>,
    pub event_kinds: Vec<String>,
    pub required_checkpoints: Vec<String>,
    pub expected_frontiers: Vec<String>,
    #[serde(default)]
    pub masks: Vec<MemoryMask>,
    #[serde(default)]
    pub local_objects: Vec<CorrespondingObject>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<Participant>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub void_profiles: Vec<VoidProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_states: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseDiff {
    pub state: String,
    pub differences: Vec<Value>,
    pub native_executed: bool,
    pub wasm_executed: bool,
    pub coverage: Vec<String>,
    pub exclusions: Vec<MemoryMask>,
    pub unspecified_local_bytes: BTreeMap<String, String>,
}

pub fn bytes(text: &str) -> Result<Vec<u8>, String> {
    let raw = text.strip_prefix("0x").ok_or("memory bytes require 0x hex")?;
    if raw.len() % 2 != 0 {
        return Err("memory hex has an odd byte count".into());
    }
    raw.as_bytes()
        .chunks_exact(2)
        .map(|b| {
            u8::from_str_radix(std::str::from_utf8(b).map_err(|_| "non-ASCII memory hex")?, 16)
                .map_err(|_| "invalid memory hex".into())
        })
        .collect()
}
fn range(r: &ByteRange, size: usize) -> Result<std::ops::Range<usize>, String> {
    let start = usize::try_from(hex(&r.offset)?).map_err(|_| "range exceeds host")?;
    let end = start
        .checked_add(usize::try_from(hex(&r.bytes)?).map_err(|_| "range exceeds host")?)
        .ok_or("range overflow")?;
    if end > size || start == end {
        return Err("range empty or outside observation".into());
    }
    Ok(start..end)
}
fn validate(e: &Execution) -> Result<(), String> {
    if !matches!(
        e.status.as_str(),
        "executed" | "exception" | "unsupported" | "refused" | "failed"
    ) {
        return Err("unknown execution status".into());
    }
    if let Some(p) = &e.participant {
        if p.role.trim().is_empty() || p.architecture.trim().is_empty() || p.runner_kind.trim().is_empty() {
            return Err("participant requires role, architecture and runner kind".into());
        }
    }
    if e.status == "exception" {
        let x = e.exception.as_ref().ok_or("exception observation missing")?;
        if e.participant.is_none()
            || x.kind.trim().is_empty()
            || x.operation.trim().is_empty()
            || !e.checkpoints.contains_key(&x.checkpoint)
            || (e.instructions == 0 && !e.events.iter().any(|e| e.kind == "exception"))
            || e.return_word.is_some()
        {
            return Err("exception needs actual runner identity, mapped operation and exception-point state".into());
        }
        if let Some(pc) = &x.guest_pc {
            hex(pc)?;
        }
    } else if e.exception.is_some() {
        return Err("exception metadata requires exception outcome".into());
    }
    if e.status == "executed"
        && ((e.return_word.is_none() && e.void_profile.is_none())
            || (e.instructions == 0 && !e.events.iter().any(|v| v.kind == "return")))
    {
        return Err("executed case needs observed execution and return bits".into());
    }
    if e.void_profile.is_some() && (e.return_word.is_some() || e.status != "executed") {
        return Err("void execution requires an actual return event/execution and no fabricated return word".into());
    }
    if let Some(word) = &e.return_word {
        if hex(word)? > u32::MAX as u64 {
            return Err("return exceeds 32-bit profile".into());
        }
    }
    for word in e.registers.values() {
        hex(word)?;
    }
    let mut ids = BTreeSet::new();
    for m in &e.ram {
        hex(&m.start)?
            .checked_add(bytes(&m.data)?.len() as u64)
            .ok_or("RAM address overflow")?;
        if !ids.insert(&m.id) {
            return Err("duplicate RAM observation".into());
        }
    }
    ids.clear();
    for o in &e.local_objects {
        if !ids.insert(&o.id)
            || hex(&o.bytes)? != bytes(&o.data)?.len() as u64
            || o.first_event > o.last_event
            || o.last_event > e.events.len()
        {
            return Err("invalid local object extent/lifetime".into());
        }
        hex(&o.base)?
            .checked_add(hex(&o.bytes)?)
            .ok_or("local object address overflow")?;
        let mut initialized = BTreeSet::new();
        for r in &o.initialized {
            for byte in range(r, bytes(&o.data)?.len())? {
                if !initialized.insert(byte) {
                    return Err("overlapping initialization ranges".into());
                }
            }
        }
    }
    for event in &e.events {
        if !matches!(
            event.kind.as_str(),
            "instruction"
                | "load"
                | "store"
                | "call"
                | "return"
                | "device"
                | "checkpoint"
                | "exception"
                | "instantiate"
        ) {
            return Err("unknown trace operation is an explicit frontier".into());
        }
        hex(&event.pc)?;
        for word in &event.arguments {
            hex(word)?;
        }
        for word in [&event.address, &event.value].into_iter().flatten() {
            hex(word)?;
        }
        if matches!(event.kind.as_str(), "call" | "instantiate")
            && (!matches!(event.profile.as_deref(), Some("real" | "controlled"))
                || event.provider.as_ref().is_none_or(String::is_empty))
        {
            return Err("call trace requires actual provider identity and real/controlled profile".into());
        }
    }
    Ok(())
}
/// Full return bits are the default profile supplied by the campaign. Masks and
/// local correspondences are explicit, bounded and returned with every result.
pub fn compare(config: &Comparison, native: &Execution, wasm: &Execution) -> Result<CaseDiff, String> {
    validate(native)?;
    validate(wasm)?;
    let mask = hex(&config.return_mask)?;
    if mask > u32::MAX as u64 {
        return Err("return mask exceeds 32-bit profile".into());
    }
    if config.event_kinds.iter().any(|k| {
        !matches!(
            k.as_str(),
            "instruction"
                | "load"
                | "store"
                | "call"
                | "return"
                | "device"
                | "checkpoint"
                | "exception"
                | "instantiate"
        )
    }) {
        return Err("unknown requested trace operation".into());
    }
    let mut objects = BTreeSet::new();
    let mut pointers = BTreeSet::new();
    for rule in &config.local_objects {
        if !objects.insert(&rule.id) {
            return Err("duplicate local correspondence".into());
        }
        for p in &rule.pointer_arguments {
            if !pointers.insert((p.event, p.argument)) {
                return Err("conflicting pointer correspondence".into());
            }
            for e in [native, wasm] {
                if !matches!(e.status.as_str(), "executed" | "exception") {
                    continue;
                }
                let event = e.events.get(p.event).ok_or("pointer correspondence event missing")?;
                if !config.event_kinds.contains(&event.kind) || p.argument >= event.arguments.len() {
                    return Err("pointer correspondence must select a compared event argument".into());
                }
            }
        }
    }
    let mut out = CaseDiff {
        state: "passed".into(),
        differences: vec![],
        native_executed: matches!(native.status.as_str(), "executed" | "exception"),
        wasm_executed: matches!(wasm.status.as_str(), "executed" | "exception"),
        coverage: vec![],
        exclusions: config.masks.clone(),
        unspecified_local_bytes: BTreeMap::new(),
    };
    if !config.participants.is_empty()
        && (config.participants.len() != 2
            || native.participant.as_ref() != config.participants.first()
            || wasm.participant.as_ref() != config.participants.get(1))
    {
        return Err("actual campaign participants differ from configured roles/architectures/runners".into());
    }
    if !out.native_executed || !out.wasm_executed {
        out.state = if native.status == "unsupported"
            && wasm.status == "unsupported"
            && native.reason == wasm.reason
            && native
                .reason
                .as_ref()
                .is_some_and(|r| config.expected_frontiers.contains(r))
        {
            "expected-frontier"
        } else {
            "refused"
        }
        .into();
        out.differences.push(json!({"kind":"execution-frontier","native":native.status,"wasm":wasm.status,"nativeReason":native.reason,"wasmReason":wasm.reason}));
        return Ok(out);
    }
    let mut diff = |kind: &str, id: &str, a: Value, b: Value| {
        if a != b {
            out.differences.push(json!({"kind":kind,"id":id,"native":a,"wasm":b}));
        }
    };
    diff("outcome", "execution", json!(native.status), json!(wasm.status));
    for name in &config.provider_states {
        let a = native
            .provider_state
            .get(name)
            .ok_or("required native provider final state missing")?;
        let b = wasm
            .provider_state
            .get(name)
            .ok_or("required WASM provider final state missing")?;
        for state in [a, b] {
            if !matches!(state["authority"].as_str(), Some("software-model" | "hardware-oracle"))
                || ["data", "control"].iter().any(|k| {
                    state[*k].as_array().is_none_or(|v| {
                        v.len() != 32 || v.iter().any(|n| n.as_u64().is_none_or(|n| n > u32::MAX as u64))
                    })
                })
            {
                return Err("provider authority or complete register state missing".into());
            }
        }
        diff("native-provider-data", name, a["data"].clone(), b["data"].clone());
        diff(
            "native-provider-control",
            name,
            a["control"].clone(),
            b["control"].clone(),
        );
    }
    if native.void_profile.is_some() || wasm.void_profile.is_some() {
        if config.void_profiles.len() != 2
            || native.void_profile.as_ref() != config.void_profiles.first()
            || wasm.void_profile.as_ref() != config.void_profiles.get(1)
        {
            return Err("void result profiles differ from independently selected campaign profiles".into());
        }
    } else if !config.void_profiles.is_empty() {
        return Err("selected void providers require explicit void execution records".into());
    }
    if let (Some(a), Some(b)) = (&native.exception, &wasm.exception) {
        diff("exception", "kind", json!(a.kind), json!(b.kind));
        diff("exception", "mapped operation", json!(a.operation), json!(b.operation));
        diff(
            "exception-state",
            "checkpoint",
            native.checkpoints[&a.checkpoint].clone(),
            wasm.checkpoints[&b.checkpoint].clone(),
        );
    }
    if native.status == "executed"
        && wasm.status == "executed"
        && native.void_profile.is_none()
        && wasm.void_profile.is_none()
    {
        diff(
            "return",
            "used bits",
            json!(format!("0x{:08x}", hex(native.return_word.as_ref().unwrap())? & mask)),
            json!(format!("0x{:08x}", hex(wasm.return_word.as_ref().unwrap())? & mask)),
        );
    }
    for name in &config.registers {
        let a = native
            .registers
            .get(name)
            .ok_or("required native register checkpoint missing")?;
        let b = wasm
            .registers
            .get(name)
            .ok_or("required WASM register checkpoint missing")?;
        diff(
            "register",
            name,
            json!(format!("0x{:x}", hex(a)?)),
            json!(format!("0x{:x}", hex(b)?)),
        );
    }
    for id in &config.required_checkpoints {
        let a = native
            .checkpoints
            .get(id)
            .ok_or("required native event checkpoint missing")?;
        let b = wasm
            .checkpoints
            .get(id)
            .ok_or("required WASM event checkpoint missing")?;
        diff("checkpoint", id, a.clone(), b.clone());
    }
    for id in &config.ram {
        let a = native
            .ram
            .iter()
            .find(|m| m.id == *id)
            .ok_or("required native RAM region missing")?;
        let b = wasm
            .ram
            .iter()
            .find(|m| m.id == *id)
            .ok_or("required WASM RAM region missing")?;
        let mut a_bytes = bytes(&a.data)?;
        let mut b_bytes = bytes(&b.data)?;
        diff(
            "ram-base",
            id,
            json!(format!("0x{:x}", hex(&a.start)?)),
            json!(format!("0x{:x}", hex(&b.start)?)),
        );
        if a_bytes.len() != b_bytes.len() {
            diff("ram-size", id, json!(a_bytes.len()), json!(b_bytes.len()));
            continue;
        }
        let mut excluded = BTreeSet::new();
        for m in config.masks.iter().filter(|m| m.region == *id) {
            if m.rationale.trim().is_empty() {
                return Err("memory exclusion requires exact rationale".into());
            }
            for i in range(&m.range, a_bytes.len())? {
                if !excluded.insert(i) {
                    return Err("memory exclusions overlap".into());
                }
                a_bytes[i] = 0;
                b_bytes[i] = 0;
            }
        }
        for (at, (a, b)) in a_bytes.iter().zip(&b_bytes).enumerate() {
            diff("ram-byte", &format!("{id}+0x{at:x}"), json!(a), json!(b));
        }
    }
    if config.masks.iter().any(|m| !config.ram.contains(&m.region)) {
        return Err("mask references a region that is not compared".into());
    }
    let normalize = |e: &Execution| -> Result<Vec<Value>, String> {
        e.events
            .iter()
            .enumerate()
            .filter(|(_, event)| config.event_kinds.contains(&event.kind))
            .map(|(index, event)| {
                let mut value = serde_json::to_value(event).map_err(|e| e.to_string())?;
                for rule in &config.local_objects {
                    for p in rule.pointer_arguments.iter().filter(|p| p.event == index) {
                        let object = e
                            .local_objects
                            .iter()
                            .find(|o| o.id == rule.id)
                            .ok_or("pointer correspondence object missing")?;
                        if index < object.first_event || index >= object.last_event {
                            return Err("pointer observed outside corresponding object's lifetime".into());
                        }
                        let word = hex(event.arguments.get(p.argument).ok_or("pointer argument missing")?)?;
                        let base = hex(&object.base)?;
                        let size = hex(&object.bytes)?;
                        if word < base || word >= base.checked_add(size).ok_or("pointer object overflow")? {
                            return Err("pointer outside its narrowly corresponding local object".into());
                        }
                        value["arguments"][p.argument] = json!({"object":rule.id,"offset":format!("0x{:x}",word-base)});
                    }
                }
                Ok(value)
            })
            .collect()
    };
    diff(
        "ordered-events",
        "trace",
        json!(normalize(native)?),
        json!(normalize(wasm)?),
    );
    for rule in &config.local_objects {
        if rule.rationale.trim().is_empty() || hex(&rule.max_bytes)? == 0 {
            return Err("local correspondence needs a bounded extent and rationale".into());
        }
        let a = native
            .local_objects
            .iter()
            .find(|o| o.id == rule.id)
            .ok_or("native local object missing")?;
        let b = wasm
            .local_objects
            .iter()
            .find(|o| o.id == rule.id)
            .ok_or("WASM local object missing")?;
        if hex(&a.bytes)? > hex(&rule.max_bytes)? || hex(&b.bytes)? > hex(&rule.max_bytes)? {
            return Err("observed local exceeds the reviewed correspondence bound".into());
        }
        diff(
            "local-extent",
            &rule.id,
            json!(format!("0x{:x}", hex(&a.bytes)?)),
            json!(format!("0x{:x}", hex(&b.bytes)?)),
        );
        diff(
            "local-lifetime",
            &rule.id,
            json!([a.first_event, a.last_event]),
            json!([b.first_event, b.last_event]),
        );
        diff(
            "local-initialization",
            &rule.id,
            json!(a.initialized),
            json!(b.initialized),
        );
        let a_data = bytes(&a.data)?;
        let b_data = bytes(&b.data)?;
        let mut initialized = 0;
        for r in &a.initialized {
            let range = range(r, a_data.len())?;
            if range.end > b_data.len() {
                return Err("corresponding initialized bytes exceed WASM object".into());
            }
            initialized += range.len();
            diff(
                "local-bytes",
                &format!("{}+{}", rule.id, r.offset),
                json!(&a_data[range.clone()]),
                json!(&b_data[range]),
            );
        }
        out.unspecified_local_bytes
            .insert(rule.id.clone(), format!("0x{:x}", a_data.len() - initialized));
    }
    for (i, a) in config.local_objects.iter().enumerate() {
        for b in &config.local_objects[i + 1..] {
            let relationship = |e: &Execution| -> Result<Value, String> {
                let x = e
                    .local_objects
                    .iter()
                    .find(|o| o.id == a.id)
                    .ok_or("corresponding local missing")?;
                let y = e
                    .local_objects
                    .iter()
                    .find(|o| o.id == b.id)
                    .ok_or("corresponding local missing")?;
                let xb = hex(&x.base)?;
                let yb = hex(&y.base)?;
                let overlap = xb < yb + hex(&y.bytes)? && yb < xb + hex(&x.bytes)?;
                Ok(if overlap {
                    json!({"overlap":true,"relativeOffset":(yb as i128-xb as i128).to_string()})
                } else {
                    json!({"overlap":false})
                })
            };
            let n = relationship(native)?;
            let w = relationship(wasm)?;
            if n != w {
                out.differences
                    .push(json!({"kind":"local-alias","objects":[a.id,b.id],"native":n,"wasm":w}));
            }
        }
    }
    // Only observed runner coverage is counted. Refused/unexecuted cases above
    // never contribute their requested paths to the executed set.
    out.coverage = native
        .coverage
        .iter()
        .chain(&wasm.coverage)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if !out.differences.is_empty() {
        out.state = "failed".into();
    }
    Ok(out)
}

/// Imported observations are useful, but only supplied current artifacts can
/// establish their content and runner lineage. No stored "passed" label is read.
pub fn audit_report(bytes: &[u8], files: Option<&BTreeMap<String, Vec<u8>>>) -> Result<Value, String> {
    let report: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let report = if report["format"] == "binviz-campaign-report" {
        report["input"].clone()
    } else {
        report
    };
    if report["format"] != "binviz-campaign" || report["schemaVersion"] != 1 {
        return Err("requires binviz-campaign schemaVersion 1".into());
    }
    let manifest: crate::evidence::EvidenceManifest =
        serde_json::from_value(report["evidence"].clone()).map_err(|e| e.to_string())?;
    let checks = manifest.verify(files)?;
    let bound = files
        .and_then(|f| f.get("campaign-observations"))
        .and_then(|b| serde_json::from_slice::<Value>(b).ok())
        .is_some_and(|v| {
            [
                "comparison",
                "cases",
                "runners",
                "requested",
                "unexamined",
                "configuration",
            ]
            .iter()
            .all(|key| v[*key] == report[*key])
        });
    let config: Comparison = serde_json::from_value(report["comparison"].clone()).map_err(|e| e.to_string())?;
    let roles = if report["configuration"].get("baseline").is_some() {
        ["baseline", "candidate"]
    } else {
        ["native", "wasm"]
    };
    for role in roles {
        if let Some(providers) = report["configuration"][role]["nativeProviders"].as_object() {
            for (name, p) in providers {
                let supplied = files.ok_or("native provider needs frozen backend/source/compiler/profile")?;
                if p["kind"] != "cop2"
                    || !matches!(p["authority"].as_str(), Some("software-model" | "hardware-oracle"))
                    || p["operations"]
                        != json!([
                            "read-data",
                            "write-data",
                            "read-control",
                            "write-control",
                            "command",
                            "load",
                            "store"
                        ])
                    || !config.provider_states.contains(name)
                {
                    return Err("native provider selection lacks complete compared COP2 state".into());
                }
                for key in ["artifact", "sourceArtifact", "compilerArtifact", "profileArtifact"] {
                    let id = p[key].as_str().ok_or("native provider input identity missing")?;
                    if !checks
                        .iter()
                        .any(|c| c.id == id && c.state == crate::evidence::IdentityState::Verified)
                    {
                        return Err("native provider input is stale or unverified".into());
                    }
                }
                let profile: Value = serde_json::from_slice(
                    supplied
                        .get(p["profileArtifact"].as_str().unwrap())
                        .ok_or("provider profile missing")?,
                )
                .map_err(|e| e.to_string())?;
                if profile["selection"] != *p
                    || profile["operationBudget"].as_u64().is_none_or(|n| n == 0)
                    || profile["commands"].as_array().is_none_or(Vec::is_empty)
                {
                    return Err(
                        "native provider profile differs from reviewed commands/source/compiler selection".into(),
                    );
                }
                for key in ["artifact", "sourceArtifact", "compilerArtifact"] {
                    let id = p[key].as_str().unwrap();
                    if supplied
                        .get(id)
                        .is_none_or(|b| profile["sha256"][id] != crate::evidence::sha256(b))
                    {
                        return Err("native provider backend/source/compiler profile drift".into());
                    }
                }
                if report["runners"]
                    .as_object()
                    .is_none_or(|workers| workers.values().any(|w| w[role]["nativeProviders"][name] != *p))
                {
                    return Err("actual runner did not install the selected native provider".into());
                }
                for case in report["cases"].as_array().ok_or("provider campaign cases missing")? {
                    if case.get("runnerFailure").is_some() {
                        continue;
                    }
                    let e = &case[role];
                    let state = &e["providerState"][name];
                    let events: Vec<_> = e["events"]
                        .as_array()
                        .ok_or("provider trace missing")?
                        .iter()
                        .filter(|v| v["provider"] == p["artifact"])
                        .collect();
                    if state["authority"] != p["authority"]
                        || state["profile"] != p["profileArtifact"]
                        || state["operations"].as_u64() != Some(events.len() as u64)
                        || events.len() as u64 > profile["operationBudget"].as_u64().unwrap()
                    {
                        return Err(
                            "native provider trace/final state/budget is not bound to the selected backend".into(),
                        );
                    }
                    for event in events {
                        let op = event["arguments"][0]
                            .as_str()
                            .and_then(|s| hex(s).ok())
                            .ok_or("provider operation missing")?;
                        if op > 6 || event["kind"] != "device" || event["profile"] != p["authority"] {
                            return Err("unknown provider operation or authority".into());
                        }
                        if op == 4 {
                            let word = event["arguments"][1]
                                .as_str()
                                .and_then(|s| hex(s).ok())
                                .ok_or("COP2 command missing")?;
                            if !profile["commands"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|c| c.as_u64() == Some(word))
                            {
                                return Err("native provider encountered an unreviewed command".into());
                            }
                        }
                    }
                }
            }
        }
    }
    for p in &config.void_profiles {
        let supplied = files.ok_or("void profile inputs must be independently verified")?;
        for id in [&p.compiler_facts, &p.module, &p.build_evidence, &p.review_artifact] {
            if !checks
                .iter()
                .any(|c| c.id == *id && c.state == crate::evidence::IdentityState::Verified)
            {
                return Err(format!("void profile input {id} is not current"));
            }
        }
        let facts = crate::compilerfacts::CompilerFacts::parse(
            supplied.get(&p.compiler_facts).ok_or("void compiler facts missing")?,
        )?;
        if facts
            .evidence
            .verify(Some(supplied))?
            .iter()
            .any(|c| c.state != crate::evidence::IdentityState::Verified)
        {
            return Err("void compiler provenance is incomplete".into());
        }
        let d = facts
            .definitions
            .iter()
            .find(|d| d.id == p.definition && d.has_body && d.return_type.category == "void")
            .ok_or("selected compiler provider is not a genuine void body")?;
        let raw: Value = serde_json::from_slice(&supplied[&p.build_evidence]).map_err(|e| e.to_string())?;
        let build: crate::evidence::EvidenceManifest =
            serde_json::from_value(raw.get("evidence").unwrap_or(&raw).clone()).map_err(|e| e.to_string())?;
        if build
            .verify(Some(supplied))?
            .iter()
            .any(|c| c.state != crate::evidence::IdentityState::Verified)
        {
            return Err("void build provenance is incomplete".into());
        }
        let mut ancestry = BTreeSet::from([p.module.clone()]);
        let mut completed = false;
        loop {
            let n = ancestry.len();
            for s in &build.stages {
                if s.outputs.iter().any(|o| ancestry.contains(o)) {
                    if s.result != "success" {
                        return Err("void compiler/link producer did not complete".into());
                    }
                    completed = true;
                    ancestry.extend(s.parents.clone());
                    ancestry.insert(s.recipe.clone());
                }
            }
            if n == ancestry.len() {
                break;
            }
        }
        if !completed
            || !ancestry.contains(&d.span.artifact)
            || !build.artifacts.iter().any(|a| {
                a.id == d.span.artifact
                    && facts
                        .evidence
                        .artifacts
                        .iter()
                        .any(|f| f.id == a.id && f.sha256 == a.sha256)
            })
        {
            return Err("void linked body lacks exact compiler buffer producer lineage".into());
        }
        let m = crate::linkevidence::inspect(supplied.get(&p.module).ok_or("void module missing")?)?;
        if !m.problems.is_empty()
            || m.functions
                .get(p.function as usize)
                .is_none_or(|f| f.imported || !f.results.is_empty() || !f.names.contains(&d.name))
        {
            return Err("actual linked provider is not the selected void definition".into());
        }
        if supplied
            .get(&p.review_artifact)
            .and_then(|b| serde_json::from_slice::<Value>(b).ok())
            != Some(json!(p))
        {
            return Err("void result review differs from exact selected compiler/linkage profile".into());
        }
    }
    let cases = report["cases"].as_array().ok_or("campaign cases missing")?;
    let requested = report["requested"].as_u64().ok_or("requested case count missing")?;
    let unexamined = report["unexamined"].as_u64().ok_or("unexamined case count missing")?;
    if (cases.len() as u64).checked_add(unexamined) != Some(requested) {
        return Err("campaign requested count differs from observed plus unexamined cases".into());
    }
    let mut comparisons = vec![];
    let mut coverage = BTreeSet::new();
    let mut executed = 0;
    let mut failed = 0;
    let mut refused = 0;
    let mut ids = BTreeSet::new();
    for case in cases {
        let id = case["id"].as_str().ok_or("case id missing")?;
        if !ids.insert(id) {
            return Err("duplicate campaign case id".into());
        }
        let result = (|| -> Result<CaseDiff, String> {
            if let Some(reason) = case["runnerFailure"].as_str() {
                return Err(reason.into());
            }
            let roles = if report["configuration"].get("baseline").is_some() {
                ["baseline", "candidate"]
            } else {
                ["native", "wasm"]
            };
            let native: Execution = serde_json::from_value(case[roles[0]].clone()).map_err(|e| e.to_string())?;
            let wasm: Execution = serde_json::from_value(case[roles[1]].clone()).map_err(|e| e.to_string())?;
            compare(&config, &native, &wasm)
        })();
        let result = match result {
            Ok(d) => {
                executed += usize::from(d.native_executed && d.wasm_executed);
                failed += usize::from(d.state == "failed");
                refused += usize::from(d.state == "refused");
                coverage.extend(d.coverage.iter().cloned());
                json!(d)
            }
            Err(reason) => {
                let runner_failed = case["runnerFailure"].is_string();
                if runner_failed {
                    failed += 1;
                } else {
                    refused += 1;
                }
                json!({"state":if runner_failed{"failed"}else{"refused"},"reason":reason,"coverage":[]})
            }
        };
        comparisons.push(json!({"id":id,"result":result}));
    }
    let first_failure = comparisons
        .iter()
        .find(|c| c["result"]["state"] == "failed" || c["result"]["state"] == "refused")
        .cloned();
    Ok(
        json!({"format":"binviz-campaign-report","schemaVersion":1,"identityChecks":checks,"observationsBound":bound,"requested":report["requested"],"unexamined":report["unexamined"],"executedPairs":executed,"failed":failed,"refused":refused,"coverage":coverage,"comparisons":comparisons,"firstFailure":first_failure,"statistics":report["statistics"],"runners":report["runners"],"input":report}),
    )
}
