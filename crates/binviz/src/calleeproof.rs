//! Recomputed, content-bound no-consumption certificates for one native GPR.
//! A surviving return is retained at the caller, never promoted to a kill.
use crate::{
    evidence::{IdentityState, hex},
    inventory::{InventoryReport, verified},
    mipsaudit::{
        self, AuditMode, AuditPolicy, AuditReport, CalleeEffect, CalleeSummary, Endpoint, ExactExtent, Outcome,
        ReturnUse,
    },
    workspace::Workspace,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub provider: String,
    pub register: u8,
    pub dependencies: Vec<String>,
    pub review_artifact: String,
    pub native_sha256: String,
    pub extent: crate::evidence::NativeSpan,
    #[serde(default)]
    pub callees: Vec<String>,
    #[serde(default)]
    pub mode: AuditMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loop_bounds: Vec<crate::mipsaudit::LoopBound>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub indirect_targets: BTreeMap<String, crate::mipsaudit::ReviewedTargets>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub id: String,
    pub provider: String,
    pub register: u8,
    pub state: String,
    pub reasons: Vec<String>,
    pub native_sha256: Option<String>,
    pub extent: Option<crate::evidence::NativeSpan>,
    pub audit: Option<AuditReport>,
    pub summary: Option<CalleeSummary>,
    pub explanation: serde_json::Value,
}
pub(crate) fn validate(w: &Workspace) -> Result<(), String> {
    let artifacts: BTreeSet<_> = w.evidence.artifacts.iter().map(|a| &a.id).collect();
    let functions: BTreeSet<_> = w
        .inventory
        .units
        .iter()
        .flat_map(|u| &u.functions)
        .map(|f| &f.id)
        .collect();
    let ids: BTreeSet<_> = w.callee_certificates.iter().map(|r| &r.id).collect();
    if ids.len() != w.callee_certificates.len() {
        return Err("duplicate callee certificate id".into());
    }
    let mut pending = BTreeMap::new();
    for r in &w.callee_certificates {
        if r.id.is_empty()
            || !functions.contains(&r.provider)
            || !(1..32).contains(&r.register)
            || !artifacts.contains(&r.review_artifact)
            || r.dependencies.is_empty()
            || r.dependencies.iter().any(|id| !artifacts.contains(id))
            || r.loop_bounds.iter().any(|b| !r.dependencies.contains(&b.evidence))
            || r.indirect_targets
                .values()
                .any(|t| !r.dependencies.contains(&t.evidence))
            || r.callees.iter().any(|id| !ids.contains(id))
            || r.native_sha256.len() != 64
            || !r.native_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || !r.extent.exact
            || hex(&r.extent.start)? & 3 != 0
            || hex(&r.extent.bytes)? == 0
            || hex(&r.extent.bytes)? & 3 != 0
        {
            return Err("invalid callee certificate identity/dependencies".into());
        }
        pending.insert(&r.id, r.callees.iter().collect::<BTreeSet<_>>());
    }
    let mut done = BTreeSet::new();
    while !pending.is_empty() {
        let ready: Vec<_> = pending
            .iter()
            .filter(|(_, deps)| deps.is_subset(&done))
            .map(|(id, _)| *id)
            .collect();
        if ready.is_empty() {
            return Err("recursive callee certificates need a separate termination proof".into());
        }
        for id in ready {
            pending.remove(id);
            done.insert(id);
        }
    }
    Ok(())
}
pub(crate) fn inspect(
    w: &Workspace,
    inventory: &InventoryReport,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<Report>, String> {
    validate(w)?;
    let functions: BTreeMap<_, _> = inventory
        .units
        .iter()
        .flat_map(|u| &u.functions)
        .map(|f| (&f.function.id, f))
        .collect();
    let mut reports: BTreeMap<String, Report> = BTreeMap::new();
    while reports.len() < w.callee_certificates.len() {
        for r in &w.callee_certificates {
            if reports.contains_key(&r.id) || r.callees.iter().any(|id| !reports.contains_key(id)) {
                continue;
            }
            let f = functions[&r.provider];
            let mut reasons = vec![];
            if inventory
                .units
                .iter()
                .find(|u| u.unit.id == f.function.identity.unit)
                .is_none_or(|u| u.architecture != "ps1-mipsel")
            {
                reasons.push("callee certificate requires explicit PS1 little-endian architecture".into());
            }
            let review = files
                .get(&r.review_artifact)
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok());
            if !verified(&inventory.identity_checks, &r.review_artifact)
                || review != Some(serde_json::to_value(r).unwrap())
            {
                reasons.push("callee review artifact is missing, stale or differs from the request".into());
            }
            if f.state != IdentityState::Verified || f.retired {
                reasons.push("callee physical identity or exact native extent is not current".into());
            }
            for id in &r.dependencies {
                if !verified(&inventory.identity_checks, id) {
                    reasons.push(format!("callee dependency {id} is not current"));
                }
            }
            let native = f.function.analysis_artifact.as_ref();
            if native.is_none_or(|id| !r.dependencies.contains(id)) {
                reasons.push("callee proof omits its native bytes dependency".into());
            }
            let mut callees = BTreeMap::new();
            let mut cross_register_callees: BTreeMap<u32, Vec<CalleeSummary>> = BTreeMap::new();
            for id in &r.callees {
                let child = &reports[id];
                let req = w.callee_certificates.iter().find(|c| &c.id == id).unwrap();
                if child.summary.is_none()
                    || !r.dependencies.contains(&req.review_artifact)
                    || req.dependencies.iter().any(|d| !r.dependencies.contains(d))
                {
                    reasons.push(format!(
                        "callee certificate {id} is refused, register differs or dependency closure is incomplete"
                    ));
                    continue;
                }
                let target = u32::try_from(hex(&functions[&child.provider].function.identity.entry)?)
                    .map_err(|_| "callee target exceeds PS1 word")?;
                let group = cross_register_callees.entry(target).or_default();
                if group.iter().any(|s| s.register == child.register) {
                    reasons.push("ambiguous same-target/register certificates".into());
                }
                group.push(child.summary.clone().unwrap());
                if child.register == r.register && callees.insert(target, child.summary.clone().unwrap()).is_some() {
                    reasons.push("ambiguous same-address callee certificates".into());
                }
            }
            let extent = f.function.identity.analysis_extent.clone();
            let bytes = native.and_then(|id| files.get(id));
            if bytes.is_none_or(|b| crate::evidence::sha256(b) != r.native_sha256)
                || serde_json::to_value(&extent).ok() != Some(serde_json::to_value(Some(&r.extent)).unwrap())
            {
                reasons.push("reviewed native hash/extent differs from the supplied provider".into());
            }
            let mut audit = None;
            let mut summary = None;
            let mut indirect_targets = BTreeMap::new();
            for (pc, targets) in &r.indirect_targets {
                let pc = u32::try_from(hex(pc)?).map_err(|_| "reviewed indirect PC exceeds PS1 word")?;
                let expected = serde_json::json!({"provider":r.provider,"nativeSha256":r.native_sha256,"extent":r.extent,"pc":format!("0x{pc:x}"),"targets":targets.targets});
                if files
                    .get(&targets.evidence)
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok())
                    != Some(expected)
                {
                    reasons
                        .push("indirect target review differs from native identity, extent or complete targets".into());
                }
                indirect_targets.insert(pc, targets.clone());
            }
            if reasons.is_empty() {
                if let (Some(span), Some(bytes)) = (&extent, bytes) {
                    if !span.exact || bytes.len() % 4 != 0 || hex(&span.bytes)? != bytes.len() as u64 {
                        return Err("callee proof needs complete exact aligned extent bytes".into());
                    }
                    let words: Vec<_> = bytes
                        .chunks_exact(4)
                        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                        .collect();
                    let start = u32::try_from(hex(&span.start)?).map_err(|_| "callee extent exceeds PS1 word")?;
                    let policy = AuditPolicy {
                        return_use: ReturnUse::Discarded,
                        callees,
                        mode: r.mode,
                        cross_register_callees,
                        loop_bounds: r.loop_bounds.clone(),
                        indirect_targets,
                        ..AuditPolicy::default()
                    };
                    let result = mipsaudit::audit(
                        ExactExtent { start, words: &words },
                        u32::try_from(hex(&f.function.identity.entry)?).map_err(|_| "callee entry exceeds PS1 word")?,
                        r.register,
                        &policy,
                    )
                    .map_err(|e| e.to_string())?;
                    if result.outcome == Outcome::Dead && result.reads.is_empty() && result.frontiers.is_empty() {
                        let effect = if result.endpoints.iter().all(|e| e.endpoint == Some(Endpoint::Killed)) {
                            CalleeEffect::Killed
                        } else {
                            CalleeEffect::NotConsumedMayWrite
                        };
                        summary = Some(CalleeSummary {
                            register: r.register,
                            effect,
                            evidence: r.review_artifact.clone(),
                            instruction_path: vec![],
                        });
                    } else {
                        reasons.push("native audit has a consumption or unresolved frontier".into());
                    }
                    audit = Some(result);
                } else {
                    reasons.push("exact native extent bytes are unavailable".into());
                }
            }
            reports.insert(
                r.id.clone(),
                Report {
                    id: r.id.clone(),
                    provider: r.provider.clone(),
                    register: r.register,
                    state: if reasons.is_empty() { "verified" } else { "refused" }.into(),
                    reasons,
                    native_sha256: bytes.map(|b| crate::evidence::sha256(b)),
                    extent,
                    explanation:serde_json::json!({"certificate":r.id,"register":r.register,"nativeSha256":bytes.map(|b|crate::evidence::sha256(b)),"effect":summary.as_ref().map(|s|&s.effect),"witnesses":audit.as_ref().map(|a|serde_json::json!({"reads":a.reads,"endpoints":a.endpoints,"frontiers":a.frontiers})),"derivedFrom":"recomputed-native-audit"}),
                    audit,
                    summary,
                },
            );
        }
    }
    Ok(w.callee_certificates
        .iter()
        .map(|r| reports.remove(&r.id).unwrap())
        .collect())
}
pub(crate) fn select(
    w: &Workspace,
    reports: &[Report],
    ids: &[String],
    register: u8,
) -> Result<BTreeMap<u32, CalleeSummary>, String> {
    let mut summaries = BTreeMap::new();
    for id in ids {
        let report = reports
            .iter()
            .find(|r| &r.id == id)
            .ok_or("unknown scoped callee certificate")?;
        if report.register != register {
            continue;
        }
        let summary = report
            .summary
            .clone()
            .ok_or_else(|| format!("callee certificate {id} refused: {}", report.reasons.join("; ")))?;
        let function = w
            .inventory
            .units
            .iter()
            .flat_map(|u| &u.functions)
            .find(|f| f.id == report.provider)
            .unwrap();
        let target = u32::try_from(hex(&function.identity.entry)?).map_err(|_| "callee target exceeds PS1 word")?;
        if summaries.insert(target, summary).is_some() {
            return Err("ambiguous scoped same-address callees".into());
        }
    }
    Ok(summaries)
}
