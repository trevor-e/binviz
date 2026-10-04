//! Physical ownership over explicitly configured members; no symbol guessing.
use crate::evidence::{EvidenceManifest, FunctionId, IdentityCheck, IdentityState, NativeSpan, UnitId, hex};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Inventory {
    pub schema_version: u32,
    pub units: Vec<Unit>,
    #[serde(default)]
    pub decisions: Vec<Decision>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Unit {
    pub identity: UnitId,
    pub member_artifact: String,
    pub architecture: String,
    pub functions: Vec<Function>,
    #[serde(default)]
    pub gaps: Vec<Gap>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Function {
    pub id: String,
    pub name: String,
    pub identity: FunctionId,
    pub analysis_artifact: Option<String>,
    pub matching_artifact: Option<String>,
    /// A fragment belongs to this primary id and is not an independent definition.
    pub owner: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub exclusions: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Gap {
    pub span: NativeSpan,
    pub kind: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decision {
    pub id: String,
    pub left: String,
    pub right: String,
    /// retire-left / retire-right; an overlap is never silently turned into an alias.
    pub action: String,
    pub reason: String,
    pub dependencies: Vec<String>,
    pub review_artifact: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collision {
    pub left: String,
    pub right: String,
    pub kind: String,
    pub reason: String,
    pub decision: Option<String>,
    pub resolved: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedFunction {
    pub function: Function,
    pub state: IdentityState,
    pub reasons: Vec<String>,
    pub retired: bool,
    pub analysis_bytes: Option<String>,
    pub matching_bytes: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedUnit {
    pub unit: UnitId,
    pub architecture: String,
    pub state: IdentityState,
    pub reasons: Vec<String>,
    pub functions: Vec<OwnedFunction>,
    pub gaps: Vec<Gap>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryReport {
    pub schema_version: u32,
    pub units: Vec<VerifiedUnit>,
    pub collisions: Vec<Collision>,
    pub identity_checks: Vec<IdentityCheck>,
}

fn range(span: &NativeSpan) -> Result<(u64, u64), String> {
    let start = hex(&span.start)?;
    Ok((start, start.checked_add(hex(&span.bytes)?).ok_or("extent overflow")?))
}
fn overlaps(a: (u64, u64), b: (u64, u64)) -> bool {
    a.0 < b.1 && b.0 < a.1
}
pub fn verified(checks: &[IdentityCheck], id: &str) -> bool {
    checks.iter().any(|c| c.id == id && c.state == IdentityState::Verified)
}
impl Inventory {
    pub fn validate(&self, evidence: &EvidenceManifest) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("inventory requires schemaVersion 1".into());
        }
        evidence.validate()?;
        let artifacts: BTreeSet<_> = evidence.artifacts.iter().map(|a| a.id.as_str()).collect();
        let mut units = BTreeSet::new();
        let mut functions = BTreeMap::new();
        for u in &self.units {
            if u.identity.id.is_empty()
                || !units.insert(&u.identity.id)
                || !artifacts.contains(u.identity.asset.as_str())
                || !artifacts.contains(u.member_artifact.as_str())
            {
                return Err("duplicate/empty unit or unknown member/asset artifact".into());
            }
            let size = hex(&u.identity.member_size)?;
            let offset = hex(&u.identity.member_offset)?;
            let base = hex(&u.identity.load_address)?;
            if size == 0
                || offset.checked_add(size).is_none()
                || base.checked_add(size).is_none()
                || u.identity.context.is_empty()
            {
                return Err("invalid physical member range/context".into());
            }
            for f in &u.functions {
                if f.id.is_empty() || f.name.is_empty() || functions.insert(f.id.as_str(), f).is_some() {
                    return Err("duplicate/empty physical function identity".into());
                }
                u.identity.validate_function(&f.identity)?;
                if !matches!(
                    f.identity.role.as_str(),
                    "primary" | "shared-fragment" | "excluded" | "assembly"
                ) {
                    return Err("unknown ownership role".into());
                }
                if f.identity.role == "excluded" && f.exclusions.is_empty() {
                    return Err("excluded function needs a reason".into());
                }
                for (span, artifact) in [
                    (&f.identity.analysis_extent, &f.analysis_artifact),
                    (&f.identity.matching_extent, &f.matching_artifact),
                ] {
                    if artifact.as_ref().is_some_and(|id| !artifacts.contains(id.as_str()))
                        || (span.is_none() && artifact.is_some())
                    {
                        return Err("unknown native artifact or artifact without extent".into());
                    }
                }
            }
            for gap in &u.gaps {
                let r = range(&gap.span)?;
                if r.0 < base
                    || r.1 > base + size
                    || r.0 == r.1
                    || !matches!(gap.kind.as_str(), "data" | "unknown" | "assembly")
                    || gap.reason.is_empty()
                {
                    return Err("invalid gap span/kind/reason".into());
                }
            }
        }
        for f in functions.values() {
            if f.identity.role == "shared-fragment" {
                let owner = f
                    .owner
                    .as_deref()
                    .and_then(|id| functions.get(id))
                    .ok_or("shared fragment needs an existing owner")?;
                let (a, b) = (
                    f.identity.analysis_extent.as_ref().ok_or("fragment needs extent")?,
                    owner.identity.analysis_extent.as_ref().ok_or("owner needs extent")?,
                );
                let (a, b) = (range(a)?, range(b)?);
                if owner.identity.unit != f.identity.unit
                    || !matches!(owner.identity.role.as_str(), "primary" | "assembly")
                    || a.0 < b.0
                    || a.1 > b.1
                {
                    return Err("fragment must be inside its primary owner in the same physical unit".into());
                }
            } else if f.owner.is_some() {
                return Err("only shared fragments have an owner".into());
            }
        }
        let mut decisions = BTreeSet::new();
        for d in &self.decisions {
            if d.id.is_empty()
                || !decisions.insert(&d.id)
                || d.left == d.right
                || !functions.contains_key(d.left.as_str())
                || !functions.contains_key(d.right.as_str())
                || !matches!(d.action.as_str(), "retire-left" | "retire-right")
                || d.reason.is_empty()
                || d.dependencies.is_empty()
                || !artifacts.contains(d.review_artifact.as_str())
                || d.dependencies.iter().any(|id| !artifacts.contains(id.as_str()))
            {
                return Err("invalid ownership decision/scope/dependencies".into());
            }
        }
        Ok(())
    }

    /// Derive member/slice bytes from the actual asset, comparing them to any
    /// independently supplied bytes. Each asset was read once by the caller.
    pub fn materialize(
        &self,
        evidence: &EvidenceManifest,
        supplied: &BTreeMap<String, Vec<u8>>,
    ) -> Result<BTreeMap<String, Vec<u8>>, String> {
        self.validate(evidence)?;
        let mut files = supplied.clone();
        fn put(files: &mut BTreeMap<String, Vec<u8>>, id: &str, bytes: &[u8]) -> Result<(), String> {
            if let Some(existing) = files.get(id) {
                if existing != bytes {
                    return Err(format!("supplied artifact {id} differs from its physical asset slice"));
                }
            } else {
                files.insert(id.into(), bytes.to_vec());
            }
            Ok(())
        }
        for u in &self.units {
            if let Some(asset) = supplied.get(&u.identity.asset) {
                let start = usize::try_from(hex(&u.identity.member_offset)?)
                    .map_err(|_| "offset exceeds host address space")?;
                let size =
                    usize::try_from(hex(&u.identity.member_size)?).map_err(|_| "member exceeds host address space")?;
                let slice = asset
                    .get(start..start.checked_add(size).ok_or("member overflow")?)
                    .ok_or("physical member crosses supplied asset")?;
                put(&mut files, &u.member_artifact, slice)?;
            }
            if let Some(member) = files.get(&u.member_artifact).cloned() {
                if member.len() as u64 != hex(&u.identity.member_size)? {
                    return Err("member byte length differs from configured size".into());
                }
                for f in &u.functions {
                    for (span, artifact) in [
                        (&f.identity.analysis_extent, &f.analysis_artifact),
                        (&f.identity.matching_extent, &f.matching_artifact),
                    ] {
                        if let (Some(span), Some(id)) = (span, artifact) {
                            let start = usize::try_from(hex(&span.start)? - hex(&u.identity.load_address)?)
                                .map_err(|_| "slice offset too large")?;
                            let size = usize::try_from(hex(&span.bytes)?).map_err(|_| "slice too large")?;
                            put(
                                &mut files,
                                id,
                                member.get(start..start + size).ok_or("native slice crosses member")?,
                            )?;
                        }
                    }
                }
            }
        }
        Ok(files)
    }

    pub fn analyze(
        &self,
        evidence: &EvidenceManifest,
        supplied: Option<&BTreeMap<String, Vec<u8>>>,
    ) -> Result<InventoryReport, String> {
        self.validate(evidence)?;
        let files = supplied.map(|s| self.materialize(evidence, s)).transpose()?;
        let checks = evidence.verify(files.as_ref())?;
        let mut collisions = vec![];
        let mut retired = BTreeSet::new();
        for (i, u) in self.units.iter().enumerate() {
            for other in &self.units[i + 1..] {
                if u.identity.asset == other.identity.asset
                    && overlaps(
                        (
                            hex(&u.identity.member_offset)?,
                            hex(&u.identity.member_offset)? + hex(&u.identity.member_size)?,
                        ),
                        (
                            hex(&other.identity.member_offset)?,
                            hex(&other.identity.member_offset)? + hex(&other.identity.member_size)?,
                        ),
                    )
                {
                    collisions.push(Collision {
                        left: u.identity.id.clone(),
                        right: other.identity.id.clone(),
                        kind: "physical-member-overlap".into(),
                        reason: "configured physical members overlap in the same asset".into(),
                        decision: None,
                        resolved: false,
                    });
                }
                if u.identity.context == other.identity.context
                    && overlaps(
                        (
                            hex(&u.identity.load_address)?,
                            hex(&u.identity.load_address)? + hex(&u.identity.member_size)?,
                        ),
                        (
                            hex(&other.identity.load_address)?,
                            hex(&other.identity.load_address)? + hex(&other.identity.member_size)?,
                        ),
                    )
                {
                    collisions.push(Collision {
                        left: u.identity.id.clone(),
                        right: other.identity.id.clone(),
                        kind: "load-context-overlap".into(),
                        reason: "virtual ranges overlap in the same configured live context".into(),
                        decision: None,
                        resolved: false,
                    });
                }
            }
            for (i, gap) in u.gaps.iter().enumerate() {
                for f in &u.functions {
                    if f.identity
                        .analysis_extent
                        .as_ref()
                        .is_some_and(|s| overlaps(range(s).unwrap(), range(&gap.span).unwrap()))
                    {
                        collisions.push(Collision {
                            left: u.identity.id.clone(),
                            right: f.id.clone(),
                            kind: "gap-function-overlap".into(),
                            reason: "configured gap/data also belongs to a function extent".into(),
                            decision: None,
                            resolved: false,
                        });
                    }
                }
                for other in &u.gaps[i + 1..] {
                    if overlaps(range(&gap.span)?, range(&other.span)?) {
                        collisions.push(Collision {
                            left: u.identity.id.clone(),
                            right: u.identity.id.clone(),
                            kind: "gap-overlap".into(),
                            reason: "configured gap/data ranges overlap".into(),
                            decision: None,
                            resolved: false,
                        });
                    }
                }
            }
            let mut aliases = BTreeMap::new();
            for f in &u.functions {
                for name in std::iter::once(&f.name).chain(&f.aliases) {
                    if let Some(other) = aliases.insert(name, &f.id) {
                        if other != &f.id {
                            collisions.push(Collision {
                                left: other.clone(),
                                right: f.id.clone(),
                                kind: "alias-conflict".into(),
                                reason: format!("label {name} identifies multiple functions in one physical unit"),
                                decision: None,
                                resolved: false,
                            });
                        }
                    }
                }
            }
            for (i, a) in u.functions.iter().enumerate() {
                for b in &u.functions[i + 1..] {
                    if a.owner.as_deref() == Some(&b.id) || b.owner.as_deref() == Some(&a.id) {
                        continue;
                    }
                    let Some((sa, sb)) = a
                        .identity
                        .analysis_extent
                        .as_ref()
                        .zip(b.identity.analysis_extent.as_ref())
                    else {
                        continue;
                    };
                    if !overlaps(range(sa)?, range(sb)?) {
                        continue;
                    }
                    let decisions: Vec<_> = self
                        .decisions
                        .iter()
                        .filter(|d| (d.left == a.id && d.right == b.id) || (d.left == b.id && d.right == a.id))
                        .collect();
                    let decision = (decisions.len() == 1).then(|| decisions[0]);
                    let resolved = decision.is_some_and(|d| {
                        d.dependencies.contains(&u.identity.asset)
                            && [a.analysis_artifact.as_ref(), b.analysis_artifact.as_ref()]
                                .into_iter()
                                .flatten()
                                .all(|id| d.dependencies.contains(id))
                            && d.dependencies.iter().all(|id| verified(&checks, id))
                            && verified(&checks, &d.review_artifact)
                            && files
                                .as_ref()
                                .and_then(|f| f.get(&d.review_artifact))
                                .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok())
                                == serde_json::to_value(d).ok()
                    });
                    if let Some(d) = decision.filter(|_| resolved) {
                        retired.insert(if d.action == "retire-left" {
                            d.left.clone()
                        } else {
                            d.right.clone()
                        });
                    }
                    collisions.push(Collision { left: a.id.clone(), right: b.id.clone(), kind: "function-overlap".into(), reason: "native analysis ranges overlap; independent providers require a current retirement decision".into(), decision: decision.map(|d| d.id.clone()), resolved });
                }
            }
        }
        let mut units = vec![];
        for u in &self.units {
            let ids = [&u.identity.asset, &u.member_artifact];
            let mut state = combined_state(&checks, &ids);
            let mut reasons = dependency_reasons(&checks, &ids);
            if collisions
                .iter()
                .any(|c| !c.resolved && (c.left == u.identity.id || c.right == u.identity.id))
            {
                if state == IdentityState::Verified {
                    state = IdentityState::Unverified;
                }
                reasons.push("physical ownership collision".into());
            }
            let mut functions = vec![];
            let mut covered = vec![];
            for f in &u.functions {
                let mut refs = ids.to_vec();
                refs.extend(f.analysis_artifact.iter());
                let mut state = combined_state(&checks, &refs);
                let mut reasons = dependency_reasons(&checks, &refs);
                if f.identity.analysis_extent.as_ref().is_none_or(|s| !s.exact) || f.analysis_artifact.is_none() {
                    state = IdentityState::Unverified;
                    reasons.push("exact analysis extent and native artifact required for proof".into());
                }
                if !f.exclusions.is_empty() || f.identity.role == "excluded" {
                    state = IdentityState::Unsupported;
                    reasons.extend(f.exclusions.clone());
                }
                if f.identity.role == "shared-fragment" {
                    reasons.push("shared fragment is not an independent callable definition".into());
                }
                if retired.contains(&f.id) {
                    reasons.push("retired by current ownership decision; previous evidence retained".into());
                }
                if collisions.iter().any(|c| {
                    !c.resolved
                        && (c.left == f.id || c.right == f.id || c.left == u.identity.id || c.right == u.identity.id)
                }) {
                    if state == IdentityState::Verified {
                        state = IdentityState::Unverified;
                    }
                    reasons.push("unresolved ownership collision".into());
                }
                if let Some(span) = &f.identity.analysis_extent {
                    covered.push(range(span)?);
                }
                functions.push(OwnedFunction {
                    function: f.clone(),
                    state,
                    reasons,
                    retired: retired.contains(&f.id),
                    analysis_bytes: f.identity.analysis_extent.as_ref().map(|s| s.bytes.clone()),
                    matching_bytes: f.identity.matching_extent.as_ref().map(|s| s.bytes.clone()),
                });
            }
            let mut gaps = u.gaps.clone();
            for gap in &gaps {
                covered.push(range(&gap.span)?);
            }
            covered.sort_unstable();
            let mut at = hex(&u.identity.load_address)?;
            let end = at + hex(&u.identity.member_size)?;
            for (lo, hi) in covered.into_iter().chain(std::iter::once((end, end))) {
                if lo > at {
                    gaps.push(Gap {
                        span: NativeSpan {
                            start: format!("0x{at:x}"),
                            bytes: format!("0x{:x}", lo - at),
                            exact: true,
                        },
                        kind: "unknown".into(),
                        reason: "no configured function/data ownership".into(),
                    });
                }
                at = at.max(hi);
            }
            units.push(VerifiedUnit {
                unit: u.identity.clone(),
                architecture: u.architecture.clone(),
                state,
                reasons,
                functions,
                gaps,
            });
        }
        Ok(InventoryReport {
            schema_version: 1,
            units,
            collisions,
            identity_checks: checks,
        })
    }
}

pub fn combined_state(checks: &[IdentityCheck], ids: &[&String]) -> IdentityState {
    let states: Vec<_> = ids
        .iter()
        .map(|id| {
            checks
                .iter()
                .find(|c| &c.id == *id)
                .map_or(IdentityState::Unverified, |c| c.state.clone())
        })
        .collect();
    for state in [
        IdentityState::Missing,
        IdentityState::Stale,
        IdentityState::Unsupported,
        IdentityState::Unverified,
    ] {
        if states.contains(&state) {
            return state;
        }
    }
    IdentityState::Verified
}
pub fn dependency_reasons(checks: &[IdentityCheck], ids: &[&String]) -> Vec<String> {
    ids.iter()
        .flat_map(|id| {
            checks
                .iter()
                .filter(move |c| &c.id == *id && c.state != IdentityState::Verified)
                .map(|c| format!("{}: {:?} {}", c.id, c.state, c.reasons.join("; ")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::{ArtifactRef, sha256};
    pub fn fixture() -> (Inventory, EvidenceManifest, BTreeMap<String, Vec<u8>>) {
        let bytes = vec![0u8; 64];
        let files = BTreeMap::from([("asset".into(), bytes.clone())]);
        let artifacts = vec![
            ("asset", bytes.clone()),
            ("member", bytes.clone()),
            ("native", bytes[..32].to_vec()),
        ]
        .into_iter()
        .map(|(id, b)| ArtifactRef {
            id: id.into(),
            role: id.into(),
            location: id.into(),
            sha256: sha256(&b),
            normalized_sha256: None,
        })
        .collect();
        let f = Function {
            id: "unit:entry".into(),
            name: "entry".into(),
            identity: FunctionId {
                unit: "unit".into(),
                entry: "0x80010000".into(),
                role: "primary".into(),
                analysis_extent: Some(NativeSpan {
                    start: "0x80010000".into(),
                    bytes: "0x20".into(),
                    exact: true,
                }),
                matching_extent: Some(NativeSpan {
                    start: "0x80010000".into(),
                    bytes: "0x8".into(),
                    exact: true,
                }),
            },
            analysis_artifact: Some("native".into()),
            matching_artifact: None,
            owner: None,
            aliases: vec![],
            exclusions: vec![],
        };
        (
            Inventory {
                schema_version: 1,
                units: vec![Unit {
                    identity: UnitId {
                        id: "unit".into(),
                        asset: "asset".into(),
                        member_offset: "0x0".into(),
                        member_size: "0x40".into(),
                        load_address: "0x80010000".into(),
                        context: "overlay-a".into(),
                    },
                    member_artifact: "member".into(),
                    architecture: "ps1-mipsel".into(),
                    functions: vec![f],
                    gaps: vec![],
                }],
                decisions: vec![],
            },
            EvidenceManifest {
                artifacts,
                stages: vec![],
            },
            files,
        )
    }
    #[test]
    fn physical_bytes_are_verified_and_short_matching_extents_stay_separate() {
        let (i, e, files) = fixture();
        let r = i.analyze(&e, Some(&files)).unwrap();
        assert_eq!(r.units[0].functions[0].state, IdentityState::Verified);
        assert_eq!(r.units[0].functions[0].analysis_bytes.as_deref(), Some("0x20"));
        assert_eq!(r.units[0].functions[0].matching_bytes.as_deref(), Some("0x8"));
        assert_eq!(r.units[0].gaps[0].span.bytes, "0x20");
    }
    #[test]
    fn enclosing_provider_collisions_preserve_the_previous_record() {
        let (mut i, mut e, mut files) = fixture();
        let mut b = i.units[0].functions[0].clone();
        b.id = "old".into();
        b.name = "old".into();
        b.analysis_artifact = None;
        b.identity.entry = "0x80010008".into();
        b.identity.matching_extent = None;
        b.identity.analysis_extent = Some(NativeSpan {
            start: "0x80010008".into(),
            bytes: "0x18".into(),
            exact: true,
        });
        i.units[0].functions.push(b);
        assert!(!i.analyze(&e, Some(&files)).unwrap().collisions[0].resolved);
        i.decisions.push(Decision {
            id: "review".into(),
            left: "unit:entry".into(),
            right: "old".into(),
            action: "retire-right".into(),
            reason: "old fragment omits initialization".into(),
            dependencies: vec!["asset".into(), "native".into()],
            review_artifact: "review-record".into(),
        });
        let bytes = serde_json::to_vec(&i.decisions[0]).unwrap();
        e.artifacts.push(ArtifactRef {
            id: "review-record".into(),
            role: "review".into(),
            location: "review.json".into(),
            sha256: sha256(&bytes),
            normalized_sha256: None,
        });
        files.insert("review-record".into(), bytes);
        let r = i.analyze(&e, Some(&files)).unwrap();
        assert!(r.collisions[0].resolved);
        assert!(r.units[0].functions[1].retired);
        let mut changed = files;
        changed.get_mut("asset").unwrap()[0] = 1;
        let r = i.analyze(&e, Some(&changed)).unwrap();
        assert!(!r.collisions[0].resolved);
    }
    #[test]
    fn fragments_are_not_definitions_and_bad_extents_are_refused() {
        let (mut i, e, files) = fixture();
        let mut b = i.units[0].functions[0].clone();
        b.id = "fragment".into();
        b.name = "fragment".into();
        b.identity.role = "shared-fragment".into();
        b.owner = Some("unit:entry".into());
        i.units[0].functions.push(b);
        assert!(i.analyze(&e, Some(&files)).unwrap().collisions.is_empty());
        i.units[0].functions[0].identity.analysis_extent.as_mut().unwrap().bytes = "0x41".into();
        assert!(i.validate(&e).is_err());
    }
}
