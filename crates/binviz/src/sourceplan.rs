//! Exact source-buffer mappings and composition against one immutable basis.
use crate::{
    adapters::{Candidate, Edit, EditPlan},
    compilerfacts::SourceSpan,
    evidence::{hex, sha256},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BufferMap {
    pub from: String,
    pub to: String,
    pub from_sha256: String,
    pub to_sha256: String,
    pub ranges: Vec<Mapping>,
    pub review_artifact: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Mapping {
    pub from_start: u64,
    pub from_end: u64,
    pub to_start: u64,
    pub to_end: u64,
}
pub fn map_span(map: &BufferMap, span: &SourceSpan, files: &BTreeMap<String, Vec<u8>>) -> Result<SourceSpan, String> {
    let a = files.get(&map.from).ok_or("source mapping input missing")?;
    let b = files.get(&map.to).ok_or("source mapping output missing")?;
    if sha256(a) != map.from_sha256
        || sha256(b) != map.to_sha256
        || files
            .get(&map.review_artifact)
            .and_then(|b| serde_json::from_slice::<Value>(b).ok())
            != Some(json!(map))
    {
        return Err("source mapping buffers or exact review drifted".into());
    }
    if span.artifact != map.from {
        return Err("span belongs to a different compiler buffer".into());
    }
    let mut previous = (0, 0);
    let mut matches = vec![];
    for r in &map.ranges {
        if r.from_start < previous.0
            || r.to_start < previous.1
            || r.from_end <= r.from_start
            || r.to_end <= r.to_start
            || r.from_end - r.from_start != r.to_end - r.to_start
        {
            return Err("source mapping ranges overlap or are not exact byte correspondences".into());
        }
        let index = |n| usize::try_from(n).map_err(|_| "source mapping offset exceeds host buffer");
        let left = a
            .get(index(r.from_start)?..index(r.from_end)?)
            .ok_or("source range outside buffer")?;
        if b.get(index(r.to_start)?..index(r.to_end)?) != Some(left) {
            return Err("source mapping bytes differ; compiler re-extraction required".into());
        }
        previous = (r.from_end, r.to_end);
        if span.start >= r.from_start
            && span.end <= r.from_end
            && (span.start != span.end || span.start < r.from_end || r.from_end == a.len() as u64)
        {
            matches.push(SourceSpan {
                artifact: map.to.clone(),
                start: r.to_start + span.start - r.from_start,
                end: r.to_start + span.end - r.from_start,
                editable: span.editable,
            });
        }
    }
    if matches.len() != 1 {
        return Err("compiler span is unmappable or ambiguous in the selected buffers".into());
    }
    Ok(matches.remove(0))
}
/// Every plan is validated on the same input, before any edits are applied.
pub fn compose(plans: &[EditPlan], files: &BTreeMap<String, Vec<u8>>) -> Result<EditPlan, String> {
    let mut result = EditPlan {
        format: "binviz-edit-plan".into(),
        schema_version: 1,
        candidates: vec![],
        refused: vec![],
        already_applied: vec![],
        dependencies: BTreeMap::new(),
        references: vec![],
    };
    let mut grouped: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    for p in plans {
        if p.format != "binviz-edit-plan" || p.schema_version != 1 || !p.refused.is_empty() {
            return Err("composition requires complete non-refused exact plans".into());
        }
        for (id, pin) in &p.dependencies {
            if files.get(id).is_none_or(|b| sha256(b) != *pin)
                || result
                    .dependencies
                    .insert(id.clone(), pin.clone())
                    .is_some_and(|old| old != *pin)
            {
                return Err("composed dependency drift/conflict".into());
            }
        }
        result.references.extend(p.references.clone());
        result.already_applied.extend(p.already_applied.clone());
        for c in &p.candidates {
            crate::adapters::apply(c, files.get(&c.artifact).ok_or("composition basis missing")?)?;
            grouped.entry(c.artifact.clone()).or_default().extend(c.edits.clone());
        }
    }
    for (artifact, mut edits) in grouped {
        edits.sort_by_key(|e| (hex(&e.start).unwrap_or(u64::MAX), hex(&e.end).unwrap_or(u64::MAX)));
        let mut merged: Vec<Edit> = vec![];
        for e in edits {
            if let Some(last) = merged.last_mut() {
                if e.start == last.start
                    && e.end == last.end
                    && e.before == last.before
                    && e.replacement == last.replacement
                {
                    last.calls.extend(e.calls);
                    last.policies.extend(e.policies);
                    continue;
                }
                if e.start == last.start
                    && e.end == last.end
                    && e.start == e.end
                    && e.before.is_empty()
                    && last.before.is_empty()
                {
                    if e.calls.iter().any(|c| last.calls.contains(c)) {
                        return Err(
                            "two helper lowerings target the same compiler call; explicit stages required".into(),
                        );
                    }
                    last.replacement.push_str(&e.replacement);
                    last.calls.extend(e.calls);
                    last.policies.extend(e.policies);
                    continue;
                }
                if hex(&last.end)? > hex(&e.start)? || last.start == e.start {
                    return Err("composed transformations overlap; explicit staged buffer mapping required".into());
                }
            }
            merged.push(e);
        }
        let bytes = &files[&artifact];
        let source = crate::adapters::apply_edits(bytes, &merged)?;
        result.candidates.push(Candidate {
            artifact,
            before_sha256: sha256(bytes),
            after_sha256: sha256(source.as_bytes()),
            edits: merged,
            source,
        });
    }
    Ok(result)
}

/// Map an already validated candidate onto a separately reviewed source buffer.
/// The original plan/reviews remain immutable; the new candidate records both pins.
pub fn map_plan(plan: &EditPlan, map: &BufferMap, files: &BTreeMap<String, Vec<u8>>) -> Result<EditPlan, String> {
    compose(std::slice::from_ref(plan), files)?;
    if plan.candidates.iter().filter(|c| c.artifact == map.from).count() != 1 {
        return Err("mapping needs exactly one candidate on its original compiler buffer".into());
    }
    let mut out = plan.clone();
    for c in &mut out.candidates {
        if c.artifact != map.from {
            continue;
        }
        crate::adapters::apply(c, files.get(&map.from).ok_or("mapping basis missing")?)?;
        for e in &mut c.edits {
            let s = map_span(
                map,
                &SourceSpan {
                    artifact: map.from.clone(),
                    start: hex(&e.start)?,
                    end: hex(&e.end)?,
                    editable: true,
                },
                files,
            )?;
            e.start = format!("0x{:x}", s.start);
            e.end = format!("0x{:x}", s.end);
        }
        let basis = &files[&map.to];
        c.artifact = map.to.clone();
        c.before_sha256 = sha256(basis);
        c.source = crate::adapters::apply_edits(basis, &c.edits)?;
        c.after_sha256 = sha256(c.source.as_bytes());
    }
    for id in [&map.from, &map.to, &map.review_artifact] {
        out.dependencies
            .insert(id.clone(), sha256(files.get(id).ok_or("mapping dependency missing")?));
    }
    out.references
        .push(json!({"kind":"reviewed-source-buffer-map","mapping":map}));
    Ok(out)
}

/// Shared CLI/MCP entry point; returns a reviewable plan, never writes C source.
pub fn request(
    w: &crate::workspace::Workspace,
    input: &Value,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<EditPlan, String> {
    match input["action"].as_str() {
        Some("compose") => {
            let plans: Vec<EditPlan> = serde_json::from_value(input["plans"].clone()).map_err(|e| e.to_string())?;
            if plans.is_empty() {
                return Err("composition requires plans".into());
            }
            compose(&plans, files)
        }
        Some("map") => {
            let plan: EditPlan = serde_json::from_value(input["plan"].clone()).map_err(|e| e.to_string())?;
            let map: BufferMap = serde_json::from_value(input["mapping"].clone()).map_err(|e| e.to_string())?;
            map_plan(&plan, &map, files)
        }
        Some("rename-declarator") => rename_declarator(
            &w.compiler_facts,
            input["definition"].as_str().ok_or("definition ID required")?,
            input["name"].as_str().ok_or("replacement name required")?,
            files,
        ),
        _ => Err("source plan action must be compose, map or rename-declarator".into()),
    }
}

/// Rename only the compiler-located identifier, even within a comma declaration.
pub fn rename_declarator(
    facts: &crate::compilerfacts::CompilerFacts,
    id: &str,
    name: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<EditPlan, String> {
    if name.is_empty()
        || !name
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
    {
        return Err("replacement must be a C identifier".into());
    }
    if facts
        .evidence
        .verify(Some(files))?
        .iter()
        .any(|c| c.state != crate::evidence::IdentityState::Verified)
    {
        return Err("declarator compiler provenance is stale".into());
    }
    let d = facts
        .definitions
        .iter()
        .find(|d| d.id == id)
        .ok_or("unknown compiler declarator")?;
    let span = d
        .name_span
        .as_ref()
        .filter(|s| s.editable)
        .ok_or("compiler did not provide an editable identifier span")?;
    let bytes = files.get(&span.artifact).ok_or("compiler source buffer missing")?;
    let e = Edit {
        start: format!("0x{:x}", span.start),
        end: format!("0x{:x}", span.end),
        before: d.name.clone(),
        replacement: name.into(),
        kind: "rename-declarator".into(),
        calls: vec![],
        policies: vec![],
    };
    let source = crate::adapters::apply_edits(bytes, std::slice::from_ref(&e))?;
    Ok(EditPlan {
        format: "binviz-edit-plan".into(),
        schema_version: 1,
        candidates: vec![Candidate {
            artifact: span.artifact.clone(),
            before_sha256: sha256(bytes),
            after_sha256: sha256(source.as_bytes()),
            source,
            edits: vec![e],
        }],
        refused: vec![],
        already_applied: vec![],
        dependencies: facts
            .evidence
            .artifacts
            .iter()
            .map(|a| (a.id.clone(), a.sha256.clone()))
            .collect(),
        references: vec![json!({"kind":"individual-compiler-declarator","definition":id,"span":span})],
    })
}
