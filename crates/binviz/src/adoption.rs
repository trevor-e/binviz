//! Reviewed adoption records join existing parsers, producer graphs and campaigns.
//! Historical evidence stays immutable. A transition never changes a policy pin.
use crate::{
    evidence::{IdentityState, sha256},
    workspace::{Workspace, WorkspaceReport},
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
    SnapshotOverlay {
        inventory: String,
        members: Vec<SnapshotMember>,
    },
    DependencyTransition {
        before: String,
        after: String,
        changed_fields: Vec<String>,
        semantic_inputs: Vec<ArtifactPair>,
        consumers: Vec<String>,
        provider_change: Option<ProviderChange>,
    },
    SdkCatalog {
        unit: String,
        libraries: BTreeMap<String, String>,
        entries: Vec<SdkEntry>,
    },
    SourceMaps {
        maps: Vec<crate::sourceplan::BufferMap>,
        spans: Vec<crate::compilerfacts::SourceSpan>,
    },
    ImportInventory {
        module: String,
        bindings: Vec<HostBinding>,
        internally_resolved: Vec<String>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostBinding {
    pub identity: crate::linkevidence::ImportIdentity,
    pub authority: String,
    pub profile_artifact: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotMember {
    pub path: String,
    pub input: String,
    pub completed_output: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactPair {
    pub before: String,
    pub after: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderChange {
    pub providers: Vec<ArtifactPair>,
    pub obligations: Vec<crate::proofclosure::Obligation>,
    pub caller_review: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdkEntry {
    pub native: String,
    pub recovered_source: String,
    pub compiler_facts: String,
    pub definition: String,
    pub upstream_revision: String,
    pub upstream_file: String,
    pub upstream_source: String,
    pub license: String,
    pub portable: Option<PortableProvider>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableProvider {
    pub module: String,
    pub function: u32,
    pub obligations: Vec<crate::proofclosure::Obligation>,
}

fn require(r: &Request, id: &str, files: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    if files.get(id).is_none_or(|b| r.dependencies.get(id) != Some(&sha256(b))) {
        return Err(format!("missing exact dependency pin: {id}"));
    }
    Ok(())
}
/// Pin every actual producer ancestor, including recipe/tool and complete scan indexes.
pub(crate) fn producer_closure(
    w: &Workspace,
    r: &Request,
    id: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<String>, String> {
    fn visit(
        w: &Workspace,
        r: &Request,
        id: &str,
        files: &BTreeMap<String, Vec<u8>>,
        seen: &mut BTreeSet<String>,
        stages: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        if !seen.insert(id.into()) {
            return Ok(());
        }
        require(r, id, files)?;
        let producers: Vec<_> = w
            .evidence
            .stages
            .iter()
            .filter(|s| s.outputs.iter().any(|o| o == id))
            .collect();
        if producers.len() > 1 {
            return Err("ambiguous producer closure".into());
        }
        if let Some(s) = producers.first() {
            if s.result != "success" {
                return Err(format!("producer {} is not completed", s.id));
            }
            stages.insert(s.id.clone());
            visit(w, r, &s.recipe, files, seen, stages)?;
            for parent in &s.parents {
                visit(w, r, parent, files, seen, stages)?;
            }
        }
        Ok(())
    }
    let mut stages = BTreeSet::new();
    visit(w, r, id, files, &mut BTreeSet::new(), &mut stages)?;
    Ok(stages.into_iter().collect())
}
fn changes(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    if a == b {
        return;
    }
    if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
        let keys: BTreeSet<_> = a.keys().chain(b.keys()).collect();
        for key in keys {
            let escaped = key.replace('~', "~0").replace('/', "~1");
            let p = format!("{path}/{escaped}");
            match (a.get(key), b.get(key)) {
                (Some(a), Some(b)) => changes(a, b, &p, out),
                _ => out.push(p),
            }
        }
    } else {
        out.push(if path.is_empty() { "/".into() } else { path.into() });
    }
}
fn path_ok(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path.split('/').all(|p| !matches!(p, "" | "." | ".."))
}
fn check(
    w: &Workspace,
    report: &WorkspaceReport,
    r: &Request,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<Value, String> {
    match &r.claim {
        Claim::ImportInventory {
            module,
            bindings,
            internally_resolved,
        } => {
            producer_closure(w, r, module, files)?;
            let m = crate::linkevidence::inspect(&files[module])?;
            if !m.problems.is_empty() {
                return Err("import inventory module is malformed or partially decoded".into());
            }
            let mut sequence = vec![];
            let mut used = BTreeSet::new();
            for f in m.functions.iter().filter(|f| f.imported) {
                let identity = f.import_identity.as_ref().ok_or("import identity missing")?;
                let matching: Vec<_> = bindings
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.identity == *identity)
                    .collect();
                if matching.len() != 1 {
                    return Err("actual import signature has missing or ambiguous host authority binding".into());
                }
                let (index, b) = matching[0];
                used.insert(index);
                require(r, &b.profile_artifact, files)?;
                if !matches!(
                    b.authority.as_str(),
                    "software-model" | "hardware-oracle" | "controlled-fixture" | "host-runtime"
                ) {
                    return Err("import host authority missing".into());
                }
                if serde_json::from_slice::<Value>(&files[&b.profile_artifact]).ok() != Some(json!(b)) {
                    return Err("host authority profile differs from selected import type".into());
                }
                sequence.push(json!({"functionIndex":f.index,"identity":identity,"authority":b.authority,"profileArtifact":b.profile_artifact}));
            }
            if used.len() != bindings.len() {
                return Err("host binding names an import absent from the actual linked module".into());
            }
            for name in internally_resolved {
                if m.functions
                    .iter()
                    .any(|f| f.import_identity.as_ref().is_some_and(|i| i.field == *name))
                    || !m.functions.iter().any(|f| !f.imported && f.names.contains(name))
                {
                    return Err("internally resolved endpoint is not an actual linked body".into());
                }
            }
            let mut multiplicity = BTreeMap::new();
            for f in &sequence {
                let key = format!(
                    "{}.{}",
                    f["identity"]["module"].as_str().unwrap(),
                    f["identity"]["field"].as_str().unwrap()
                );
                *multiplicity.entry(key).or_insert(0usize) += 1;
            }
            Ok(
                json!({"moduleSha256":m.sha256,"reader":"binviz-shared-wasm-reader","imports":m.import_inventory,"functionBindings":sequence,"multiplicity":multiplicity,"internallyResolved":internally_resolved,"inputUnchanged":sha256(&files[module])==m.sha256}),
            )
        }
        Claim::SourceMaps { maps, spans } => {
            if maps.is_empty() || spans.is_empty() {
                return Err("source mapping needs exact compiler spans".into());
            }
            let mut mapped = vec![];
            for m in maps {
                for id in [&m.from, &m.to, &m.review_artifact] {
                    require(r, id, files)?;
                }
            }
            for s in spans {
                let matches: Vec<_> = maps.iter().filter(|m| m.from == s.artifact).collect();
                if matches.len() != 1 {
                    return Err("ambiguous compiler source buffer map".into());
                }
                let known = w
                    .compiler_facts
                    .calls
                    .iter()
                    .any(|c| serde_json::to_value(&c.span).ok() == serde_json::to_value(s).ok())
                    || w.compiler_facts.definitions.iter().any(|d| {
                        serde_json::to_value(&d.span).ok() == serde_json::to_value(s).ok()
                            || d.name_span
                                .as_ref()
                                .is_some_and(|n| serde_json::to_value(n).ok() == serde_json::to_value(s).ok())
                    });
                if !known {
                    return Err("mapping span does not belong to current compiler facts".into());
                }
                mapped.push(json!({"original":s,"mapped":crate::sourceplan::map_span(matches[0],s,files)?}));
            }
            Ok(json!({"spans":mapped,"maps":maps}))
        }
        Claim::SnapshotOverlay { inventory, members } => {
            require(r, inventory, files)?;
            let declared: Vec<String> = serde_json::from_slice(&files[inventory])
                .map_err(|_| "snapshot inventory must list all repository-relative member paths")?;
            let mut paths = BTreeSet::new();
            let mut out = vec![];
            for m in members {
                if !path_ok(&m.path) || !paths.insert(m.path.clone()) {
                    return Err("unsafe or duplicate snapshot overlay member".into());
                }
                producer_closure(w, r, &m.input, files)?;
                let output = if let Some(id) = &m.completed_output {
                    let stages = producer_closure(w, r, id, files)?;
                    if stages.is_empty() {
                        return Err("completed snapshot replacement needs an actual successful producer".into());
                    }
                    Some(json!({"artifact":id,"sha256":sha256(&files[id]),"producerClosure":stages}))
                } else {
                    None
                };
                out.push(json!({"path":m.path,"historicalInput":{"artifact":m.input,"sha256":sha256(&files[&m.input])},"completedOutput":output,"selectedArtifact":m.completed_output.as_ref().unwrap_or(&m.input)}));
            }
            if declared.len() != paths.len() || declared.iter().any(|p| !paths.contains(p)) {
                return Err("snapshot overlay does not cover the complete frozen inventory".into());
            }
            Ok(json!({"members":out,"historicalInputsPreserved":true,"exportRequiresFreshDirectory":true}))
        }
        Claim::DependencyTransition {
            before,
            after,
            changed_fields,
            semantic_inputs,
            consumers,
            provider_change,
        } => {
            let old = producer_closure(w, r, before, files)?;
            let new = producer_closure(w, r, after, files)?;
            if old.is_empty() || new.is_empty() {
                return Err("dependency transition needs both completed producer bases".into());
            }
            if semantic_inputs.is_empty() {
                return Err("transition needs the complete semantic input correspondence".into());
            }
            let mut semantic_before = BTreeSet::new();
            let mut semantic_after = BTreeSet::new();
            for p in semantic_inputs {
                producer_closure(w, r, &p.before, files)?;
                producer_closure(w, r, &p.after, files)?;
                semantic_before.insert(p.before.clone());
                semantic_after.insert(p.after.clone());
                if files[&p.before] != files[&p.after] {
                    let selected = provider_change
                        .as_ref()
                        .is_some_and(|c| c.providers.iter().any(|v| v.before == p.before && v.after == p.after));
                    if !selected {
                        return Err(
                            "changed source, prototype, prepared CPP, object, tool or build recipe needs a new proof"
                                .into(),
                        );
                    }
                    let a = crate::linkevidence::inspect(&files[&p.before])
                        .map_err(|_| "provider transition requires actual WASM objects/modules")?;
                    let b = crate::linkevidence::inspect(&files[&p.after])
                        .map_err(|_| "provider transition requires actual WASM objects/modules")?;
                    let abi = |m: &crate::linkevidence::ModuleReport| {
                        m.functions.iter().map(|f|json!({"names":f.names,"parameters":f.parameters,"results":f.results,"import":f.import_identity})).collect::<Vec<_>>()
                    };
                    if !a.problems.is_empty() || !b.problems.is_empty() || abi(&a) != abi(&b) {
                        return Err("changed provider has a changed or unresolved actual ABI".into());
                    }
                    for pair in provider_change
                        .as_ref()
                        .unwrap()
                        .providers
                        .iter()
                        .filter(|v| v.before == p.before && v.after == p.after)
                    {
                        let covered = provider_change.as_ref().unwrap().obligations.iter().any(|o| {
                            files
                                .get(&o.campaign)
                                .and_then(|bytes| crate::campaign::audit_report(bytes, Some(files)).ok())
                                .is_some_and(|v| {
                                    v["input"]["configuration"]["baseline"]["profile"]["moduleArtifact"] == pair.before
                                        && v["input"]["configuration"]["candidate"]["profile"]["moduleArtifact"]
                                            == pair.after
                                })
                        });
                        if !covered {
                            return Err(
                                "changed provider campaign did not execute both selected module identities".into(),
                            );
                        }
                    }
                }
            }
            for (stages, semantic) in [(&old, &semantic_before), (&new, &semantic_after)] {
                for s in w.evidence.stages.iter().filter(|s| stages.contains(&s.id)) {
                    for id in s.parents.iter().chain(std::iter::once(&s.recipe)) {
                        if !semantic.contains(id) {
                            return Err(format!("transition omits semantic producer input {id}"));
                        }
                    }
                }
            }
            let mut actual = vec![];
            match (
                serde_json::from_slice::<Value>(&files[before]),
                serde_json::from_slice::<Value>(&files[after]),
            ) {
                (Ok(a), Ok(b)) => changes(&a, &b, "", &mut actual),
                _ => {
                    if files[before] != files[after] {
                        actual.push("$bytes".into())
                    }
                }
            }
            let mut declared = changed_fields.clone();
            declared.sort();
            declared.dedup();
            actual.sort();
            if declared != actual
                || declared.iter().any(|p| {
                    !matches!(
                        p.as_str(),
                        "/dependencies" | "/dependencyStamp" | "/producerStamp" | "/provenance"
                    ) && !p.starts_with("/dependencies/")
                })
            {
                return Err("transition changes unreviewed semantic fields; a new proof is required".into());
            }
            let mut affected = BTreeSet::from([before.clone()]);
            let mut actual_consumers = BTreeSet::new();
            loop {
                let n = affected.len();
                for s in &w.evidence.stages {
                    if s.parents
                        .iter()
                        .chain(std::iter::once(&s.recipe))
                        .any(|p| affected.contains(p))
                    {
                        actual_consumers.insert(s.id.clone());
                        affected.extend(s.outputs.clone());
                    }
                }
                if affected.len() == n {
                    break;
                }
            }
            if consumers.iter().collect::<BTreeSet<_>>() != actual_consumers.iter().collect()
                || consumers.len() != actual_consumers.len()
            {
                return Err("reviewed transition omits or invents an affected consumer".into());
            }
            let provider = if let Some(p) = provider_change {
                require(r, &p.caller_review, files)?;
                if p.obligations.is_empty() {
                    return Err("changed provider requires its own executed campaign and caller review".into());
                }
                if p.providers.is_empty()
                    || p.providers.iter().any(|p| {
                        !semantic_inputs
                            .iter()
                            .any(|s| s.before == p.before && s.after == p.after)
                    })
                {
                    return Err("changed provider review needs exact semantic provider pairs".into());
                }
                let expected = json!({"before":before,"after":after,"consumers":consumers,"providers":p.providers,"obligations":p.obligations});
                if serde_json::from_slice::<Value>(&files[&p.caller_review]).ok() != Some(expected) {
                    return Err("changed provider caller review differs from actual closure".into());
                }
                Some(
                    p.obligations
                        .iter()
                        .map(|o| {
                            require(r, &o.campaign, files)?;
                            crate::proofclosure::obligation(o, files)
                        })
                        .collect::<Result<Vec<_>, String>>()?,
                )
            } else {
                None
            };
            Ok(
                json!({"before":before,"after":after,"changedFields":actual,"semanticInputs":semantic_inputs,"consumerClosure":consumers,"oldProducerClosure":old,"currentProducerClosure":new,"providerCampaigns":provider,"policyPinsUnchanged":true}),
            )
        }
        Claim::SdkCatalog {
            unit,
            libraries,
            entries,
        } => {
            let u = w
                .inventory
                .units
                .iter()
                .find(|u| u.identity.id == *unit)
                .ok_or("SDK catalog unit missing")?;
            require(r, &u.member_artifact, files)?;
            if report
                .inventory
                .units
                .iter()
                .find(|x| x.unit.id == *unit)
                .is_none_or(|x| x.state != IdentityState::Verified)
            {
                return Err("SDK unit ownership is unresolved".into());
            }
            let binary = if matches!(
                u.architecture.as_str(),
                "mips" | "mipsel" | "ps1" | "psx" | "mips-r3000a" | "ps1-mipsel"
            ) {
                crate::Binary::parse_psx_overlay(
                    files[&u.member_artifact].clone(),
                    crate::evidence::hex(&u.identity.load_address)?,
                    None,
                )
            } else {
                crate::Binary::parse(files[&u.member_artifact].clone())
            }
            .map_err(|e| e.to_string())?;
            let mut sigs = crate::sigs::SignatureSet::default();
            for (id, name) in libraries {
                require(r, id, files)?;
                sigs.add_file(name, &files[id]).map_err(|e| e.to_string())?;
            }
            if libraries.is_empty() || sigs.signatures.is_empty() {
                return Err("SDK catalog needs parsed library signatures".into());
            }
            let matches = binary.identify_sdk(&sigs);
            let mut joined = vec![];
            let mut seen = BTreeSet::new();
            for e in entries {
                if !seen.insert(&e.native) {
                    return Err("duplicate SDK catalog native identity".into());
                }
                let f = u
                    .functions
                    .iter()
                    .find(|f| f.id == e.native && f.owner.is_none())
                    .ok_or("SDK catalog native owner missing")?;
                let address = crate::evidence::hex(&f.identity.entry)?;
                let candidates: Vec<_> = matches.matches.iter().filter(|m| m.address == address).collect();
                if candidates.is_empty() {
                    return Err("SDK entry has no independently matched native library bytes".into());
                }
                for id in [&e.recovered_source, &e.compiler_facts, &e.upstream_source, &e.license] {
                    require(r, id, files)?;
                }
                if !path_ok(&e.upstream_file)
                    || !(e.upstream_revision.len() == 40 || e.upstream_revision.len() == 64)
                    || !e.upstream_revision.bytes().all(|b| b.is_ascii_hexdigit())
                    || files[&e.license].is_empty()
                {
                    return Err("SDK upstream file, immutable revision or license evidence missing".into());
                }
                let facts = crate::compilerfacts::CompilerFacts::parse(&files[&e.compiler_facts])?;
                if facts
                    .evidence
                    .verify(Some(files))?
                    .iter()
                    .any(|c| c.state != IdentityState::Verified)
                {
                    return Err("SDK source ABI provenance stale".into());
                }
                let d = facts
                    .definitions
                    .iter()
                    .find(|d| d.id == e.definition && d.has_body)
                    .ok_or("recovered SDK definition missing")?;
                if !candidates.iter().any(|m| m.name == d.name || m.also.contains(&d.name)) {
                    return Err(
                        "recovered SDK definition differs from the actual library match or retained aliases".into(),
                    );
                }
                if !facts
                    .evidence
                    .dependency_closure(&d.span.artifact)?
                    .contains(&e.recovered_source)
                {
                    return Err(
                        "recovered SDK source is not part of the selected compiler definition's producer lineage"
                            .into(),
                    );
                }
                for a in &facts.evidence.artifacts {
                    require(r, &a.id, files)?;
                }
                let portable = if let Some(p) = &e.portable {
                    producer_closure(w, r, &p.module, files)?;
                    let m = report
                        .modules
                        .get(&p.module)
                        .ok_or("portable SDK module was not inspected")?;
                    let parameters = d
                        .abi_parameters
                        .iter()
                        .map(|t| {
                            if matches!(t.category.as_str(), "integer" | "pointer") && t.bits.is_some_and(|n| n <= 32) {
                                "i32"
                            } else {
                                "unsupported"
                            }
                        })
                        .collect::<Vec<_>>();
                    let results = if d.return_type.category == "void" {
                        vec![]
                    } else if matches!(d.return_type.category.as_str(), "integer" | "pointer")
                        && d.return_type.bits.is_some_and(|n| n <= 32)
                    {
                        vec!["i32"]
                    } else {
                        vec!["unsupported"]
                    };
                    if !m.problems.is_empty()
                        || d.variadic
                        || m.functions.get(p.function as usize).is_none_or(|f| {
                            f.imported
                                || !f.names.contains(&d.name)
                                || f.parameters != parameters
                                || f.results != results
                        })
                        || p.obligations.is_empty()
                    {
                        return Err(
                            "portable SDK provider needs actual linked definition, compiler ABI and executed campaign"
                                .into(),
                        );
                    }
                    let obligations = p
                        .obligations
                        .iter()
                        .map(|o| {
                            require(r, &o.campaign, files)?;
                            crate::proofclosure::obligation(o, files)
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    Some(json!({"provider":p,"campaigns":obligations,"runtimeServices":m.imports}))
                } else {
                    None
                };
                let admitted: Vec<_> = w
                    .policies
                    .iter()
                    .filter(|p| p.provider == e.native || p.definition == e.definition)
                    .map(|p| &p.id)
                    .collect();
                joined.push(json!({"native":e.native,"matches":candidates,"sourceAbi":d,"metadata":e,"portable":portable,"existingScopedPolicies":admitted,"ambiguous":candidates.len()!=1||candidates.iter().any(|c|!c.also.is_empty())}));
            }
            Ok(
                json!({"matcherReport":matches,"entries":joined,"librarySources":binary.library_sources(),"upstreamProvenance":"reviewed pinned source; revision labels are not a network attestation"}),
            )
        }
    }
}
pub fn inspect(
    w: &Workspace,
    report: &WorkspaceReport,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<Value>, String> {
    let checks = w.evidence.verify(Some(files))?;
    let mut seen = BTreeSet::new();
    let mut results = vec![];
    for r in &w.adoption_requests {
        if r.id.is_empty() || !seen.insert(&r.id) {
            return Err("empty/duplicate adoption identity".into());
        }
        let result = (|| {
            if r.dependencies.is_empty() {
                return Err("adoption request needs pinned complete dependencies".into());
            }
            for id in r.dependencies.keys() {
                require(r, id, files)?;
                if !checks.iter().any(|c| c.id == *id && c.state == IdentityState::Verified) {
                    return Err(format!("adoption dependency {id} is not current"));
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
                return Err("adoption review differs from exact request".into());
            }
            check(w, report, r, files)
        })();
        results.push(match result {
            Ok(details) => json!({"id":r.id,"state":"verified","details":details,"request":r}),
            Err(reason) => json!({"id":r.id,"state":"refused","reasons":[reason],"request":r}),
        });
    }
    Ok(results)
}
