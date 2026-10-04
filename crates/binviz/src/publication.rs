//! Recompute only a selected matching scope and propose exact note changes.
use crate::{
    evidence::{IdentityState, hex, sha256},
    matching, notes, queue,
    workspace::Workspace,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub unit: String,
    pub notes_artifact: String,
    pub notes_sha256: String,
    pub review_artifact: String,
    pub dependencies: BTreeMap<String, String>,
    pub functions: Vec<Selection>,
    pub build: String,
    pub now: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub function: String,
    pub definition: String,
    pub source: String,
    pub object: String,
    pub producer: String,
    pub symbol: String,
}
pub(crate) fn ancestors(w: &Workspace, stage: &str) -> BTreeSet<String> {
    let mut pending = vec![stage.to_owned()];
    let mut out = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !out.insert(id.clone()) {
            continue;
        }
        if let Some(s) = w.evidence.stages.iter().find(|s| s.id == id) {
            pending.extend(s.parents.iter().cloned());
            pending.push(s.recipe.clone());
        }
        if let Some(s) = w.evidence.stages.iter().find(|s| s.outputs.contains(&id)) {
            pending.push(s.id.clone());
        }
    }
    out
}
pub fn plan(w: &Workspace, r: &Request, supplied: &BTreeMap<String, Vec<u8>>) -> Result<Value, String> {
    w.validate()?;
    let files = w.inventory.materialize(&w.evidence, supplied)?;
    let inventory = w.inventory.analyze(&w.evidence, Some(&files))?;
    let current = |id: &str| {
        inventory
            .identity_checks
            .iter()
            .any(|c| c.id == id && c.state == IdentityState::Verified)
    };
    if r.functions.is_empty() || r.dependencies.is_empty() {
        return Err("matching publication needs exact selections and pinned dependencies".into());
    }
    for (id, pin) in &r.dependencies {
        if !current(id) || files.get(id).is_none_or(|b| sha256(b) != *pin) {
            return Err(format!("matching publication dependency {id} is not current"));
        }
    }
    if !current(&r.review_artifact)
        || files
            .get(&r.review_artifact)
            .and_then(|b| serde_json::from_slice::<Value>(b).ok())
            != Some(json!(r))
    {
        return Err("publication review differs from exact request".into());
    }
    let raw = files.get(&r.notes_artifact).ok_or("notes artifact missing")?;
    if !current(&r.notes_artifact) || sha256(raw) != r.notes_sha256 {
        return Err("notes changed since publication review".into());
    }
    let (before, fingerprint) = notes::parse(std::str::from_utf8(raw).map_err(|_| "notes not UTF-8")?)?;
    let unit = w
        .inventory
        .units
        .iter()
        .find(|u| u.identity.id == r.unit)
        .ok_or("publication unit missing")?;
    if unit.architecture != "ps1-mipsel" {
        return Err("scoped publication currently requires PS1 MIPS objects".into());
    }
    let member = files
        .get(&unit.member_artifact)
        .ok_or("current native member missing")?;
    if !current(&unit.member_artifact) || !r.dependencies.contains_key(&unit.member_artifact) {
        return Err("publication omits current physical member".into());
    }
    let mut binary = crate::Binary::parse_psx_overlay(member.clone(), hex(&unit.identity.load_address)?, None)
        .map_err(|e| e.to_string())?;
    let labels: Vec<_> = unit
        .functions
        .iter()
        .filter(|f| f.owner.is_none())
        .map(|f| crate::Annotation {
            address: hex(&f.identity.entry).unwrap(),
            name: f.name.clone(),
            ..Default::default()
        })
        .collect();
    binary.set_annotations(labels);
    let mut selected = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut ranges = vec![];
    let mut rows = vec![];
    let mut scores = vec![];
    for s in &r.functions {
        if !selected.insert(&s.function) {
            return Err("duplicate publication function".into());
        }
        let owned = inventory
            .units
            .iter()
            .find(|u| u.unit.id == r.unit)
            .and_then(|u| u.functions.iter().find(|f| f.function.id == s.function))
            .ok_or("selected physical function missing")?;
        if owned.state != IdentityState::Verified || owned.retired || owned.function.owner.is_some() {
            return Err("selected physical ownership is not current".into());
        }
        let span = owned
            .function
            .identity
            .matching_extent
            .as_ref()
            .filter(|s| s.exact)
            .ok_or("exact matching extent missing")?;
        let d = w
            .compiler_facts
            .definitions
            .iter()
            .find(|d| d.id == s.definition && d.has_body)
            .ok_or("selected compiler definition missing")?;
        if d.name != s.symbol {
            return Err("compiled symbol differs from selected actual definition".into());
        }
        let tu = w
            .compiler_facts
            .units
            .iter()
            .find(|u| u.id == d.unit)
            .ok_or("definition TU missing")?;
        if s.source != tu.source && tu.prepared.as_ref() != Some(&s.source) {
            return Err("publication source differs from compiler provenance".into());
        }
        for id in [&s.source, &s.object, &d.span.artifact] {
            if !current(id) || !r.dependencies.contains_key(id) {
                return Err(format!("publication omits current definition/source/object {id}"));
            }
        }
        let producer = w
            .evidence
            .stages
            .iter()
            .find(|p| p.id == s.producer && p.result == "success" && p.outputs.contains(&s.object))
            .ok_or("object producer missing or rejected")?;
        if !current(&producer.id)
            || !ancestors(w, &producer.id).contains(&s.source)
            || !ancestors(w, &producer.id).contains(&d.span.artifact)
        {
            return Err("object producer does not bind selected actual source/prepared definition".into());
        }
        if ancestors(w, &producer.id)
            .iter()
            .any(|id| w.evidence.artifacts.iter().any(|a| a.id == *id) && !r.dependencies.contains_key(id))
        {
            return Err("publication omits a frozen producer input/tool/recipe dependency".into());
        }
        let object = matching::object_functions(&files[&s.object]).map_err(|e| e.to_string())?;
        let functions: Vec<_> = object.iter().filter(|f| f.name == s.symbol).collect();
        if functions.len() != 1 {
            return Err("object symbol missing or ambiguous".into());
        }
        let f = functions[0];
        for rel in f.relocs.values().filter(|r| !r.section_symbol) {
            if binary.object_symbol_address(&rel.symbol).is_none() {
                if let Some(address) = matching::address_in_name(&rel.symbol) {
                    let mut labels = binary.annotations().to_vec();
                    labels.push(crate::Annotation {
                        address,
                        name: rel.symbol.clone(),
                        ..Default::default()
                    });
                    binary.set_annotations(labels);
                } else {
                    return Err(format!("strict relocation target {} is unresolved", rel.symbol));
                }
            }
        }
        let start = hex(&span.start)?;
        let end = start.checked_add(hex(&span.bytes)?).ok_or("matching extent overflow")?;
        let score = binary.match_range(start, end, f).map_err(|e| e.to_string())?;
        if !score.exact() {
            return Err(format!(
                "selected function {} is not an exact current match ({:.2}%)",
                s.function, score.percent
            ));
        }
        sources.insert(s.source.clone());
        ranges.push((start, end));
        scores.push(matching::FunctionProgress {
            address: hex(&owned.function.identity.entry)?,
            name: d.name.clone(),
            unit: s.source.clone(),
            size: end - start,
            percent: score.percent,
        });
        rows.push(json!({"selection":s,"extent":span,"score":score.score(&s.source),"producer":producer}));
    }
    ranges.sort();
    let mut covered = 0;
    let mut last = 0;
    for (start, end) in ranges {
        covered += end.saturating_sub(start.max(last));
        last = last.max(end)
    }
    let mut after = before.clone();
    let (newly, lost) = queue::record_scores(&mut after, &scores, &queue::BuildInfo::parse(&r.build)?, r.now);
    let fingerprint = fingerprint.unwrap_or_else(|| binary.summary().fingerprint.clone());
    let proposed = notes::document(&r.unit, &fingerprint, &after);
    let changed_addresses: BTreeSet<_> = scores.iter().map(|s| s.address).collect();
    let untouched = before
        .iter()
        .filter(|a| !changed_addresses.contains(&a.address))
        .count();
    Ok(
        json!({"id":r.id,"state":"ready-for-publication","notesArtifact":r.notes_artifact,"beforeSha256":r.notes_sha256,"afterSha256":sha256(proposed.as_bytes()),"proposedNotes":proposed,"selected":rows,"distinctSourceRows":sources.len(),"uniquePhysicalFunctions":selected.len(),"coveredBytes":format!("0x{covered:x}"),"untouchedNoteRows":untouched,"outsideScope":"retained verbatim; cached matches outside this scope are not revalidated","newlyMatched":newly,"lost":lost,"request":r}),
    )
}
pub fn plans(w: &Workspace, files: &BTreeMap<String, Vec<u8>>) -> Vec<Value> {
    w.matching_publications
        .iter()
        .map(|r| match plan(w, r, files) {
            Ok(v) => v,
            Err(reason) => json!({"id":r.id,"state":"refused","reasons":[reason],"request":r}),
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn publish(w: &Workspace, r: &Request, root: &std::path::Path) -> Result<Value, String> {
    let files = w.evidence.read_artifacts(root);
    let mut planned = plan(w, r, &files)?;
    let path = root.join(
        &w.evidence
            .artifacts
            .iter()
            .find(|a| a.id == r.notes_artifact)
            .ok_or("notes identity missing")?
            .location,
    );
    let (before, fingerprint, folded) = notes::read(&path)?;
    let raw_before = notes::parse(std::str::from_utf8(&files[&r.notes_artifact]).map_err(|_| "notes not UTF-8")?)?.0;
    if json!(before) != json!(raw_before) {
        return Err("notes journal has newer changes; regenerate publication request".into());
    }
    let current = w.evidence.read_artifacts(root);
    if plan(w, r, &current)? != planned {
        return Err("publication inputs changed before note write".into());
    }
    let after = notes::parse(planned["proposedNotes"].as_str().ok_or("proposed notes missing")?)?.0;
    notes::write(
        &path,
        &r.unit,
        &fingerprint.unwrap_or_default(),
        &before,
        &after,
        folded,
    )?;
    planned["state"] = json!("published");
    planned["notesPath"] = json!(path);
    Ok(planned)
}
