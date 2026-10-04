//! Reviewable edits over verified compiler spans. No lexical C discovery.
use crate::{
    compilerfacts::{CompilerType, SourceSpan},
    evidence::{hex, sha256},
    workspace::Workspace,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edit {
    pub start: String,
    pub end: String,
    pub before: String,
    pub replacement: String,
    pub kind: String,
    pub calls: Vec<String>,
    pub policies: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Candidate {
    pub artifact: String,
    pub before_sha256: String,
    pub after_sha256: String,
    pub edits: Vec<Edit>,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Refusal {
    pub call: String,
    pub reasons: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditPlan {
    pub format: String,
    pub schema_version: u32,
    pub candidates: Vec<Candidate>,
    pub refused: Vec<Refusal>,
    pub already_applied: Vec<String>,
    pub dependencies: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<serde_json::Value>,
}

fn c_type(t: &CompilerType) -> Result<&str, String> {
    if !matches!(t.category.as_str(), "integer" | "pointer" | "void")
        || t.spelling.is_empty()
        || !t
            .spelling
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b' ' | b'*'))
    {
        return Err("type needs a compiler-provided complex declarator lowering".into());
    }
    Ok(&t.spelling)
}
fn text<'a>(files: &'a BTreeMap<String, Vec<u8>>, span: &SourceSpan) -> Result<&'a str, String> {
    if !span.editable {
        return Err("compiler span is not editable (macro/implicit reference)".into());
    }
    let source = files.get(&span.artifact).ok_or("source bytes missing")?;
    std::str::from_utf8(
        source
            .get(
                usize::try_from(span.start).map_err(|_| "span exceeds host")?
                    ..usize::try_from(span.end).map_err(|_| "span exceeds host")?,
            )
            .ok_or("source span exceeds bytes")?,
    )
    .map_err(|_| "span is not UTF-8 aligned".into())
}
/// Wrappers evaluate every supplied argument once, preserving the original C
/// argument-order freedom. Only reviewed trailing words are ignored in the body.
pub fn plan(w: &Workspace, files: &BTreeMap<String, Vec<u8>>, policies: &[String]) -> Result<EditPlan, String> {
    let report = w.analyze(Some(files))?;
    let mut result = EditPlan {
        format: "binviz-edit-plan".into(),
        schema_version: 1,
        candidates: vec![],
        refused: vec![],
        already_applied: vec![],
        dependencies: BTreeMap::new(),
        references: vec![],
    };
    let selected: BTreeSet<_> = policies.iter().collect();
    for id in &selected {
        if !w.policies.iter().any(|p| &p.id == *id) {
            return Err(format!("unknown exact policy {id}"));
        }
    }
    let mut changes: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    let mut helpers: BTreeMap<(String, u64), Vec<(String, String, String)>> = BTreeMap::new();
    for caller in &report.callers {
        for site in &caller.calls {
            for decision in site
                .policies
                .iter()
                .filter(|d| selected.is_empty() || selected.contains(&d.policy))
            {
                if decision.state == "applied" {
                    result.already_applied.push(site.call.id.clone());
                    continue;
                }
                if decision.state != "eligible" {
                    result.refused.push(Refusal {
                        call: site.call.id.clone(),
                        reasons: decision.reasons.clone(),
                    });
                    continue;
                }
                let policy = w.policies.iter().find(|p| p.id == decision.policy).unwrap();
                let build = (|| -> Result<(Edit, String), String> {
                    if !matches!(
                        policy.kind.as_str(),
                        "extra-word-nonuse"
                            | "scalar-varargs"
                            | "missing-inputs"
                            | "discarded-result"
                            | "direct-binding"
                    ) {
                        return Err(
                            "this representation requires a reviewed return/hidden-result/callback lowering".into(),
                        );
                    }
                    let binding = site.binding.as_ref().ok_or("provider binding missing")?;
                    let definition = w
                        .compiler_facts
                        .definitions
                        .iter()
                        .find(|d| d.id == binding.definition)
                        .unwrap();
                    let discard = policy.kind == "discarded-result";
                    if discard && site.call.result_consumed {
                        return Err("discarded-result lowering cannot replace a consumed result".into());
                    }
                    if !discard && definition.return_type != site.call.result_type {
                        return Err("result conversion needs its own verified used-return closure".into());
                    }
                    if matches!(policy.kind.as_str(), "discarded-result" | "direct-binding") {
                        if definition.variadic || site.call.arguments.len() != definition.abi_parameters.len() {
                            return Err("direct binding requires the actual true formal count".into());
                        }
                        if !caller.extraction_gaps.is_empty()
                            || w.compiler_facts
                                .address_references
                                .iter()
                                .any(|r| r.declaration == site.call.declaration)
                        {
                            return Err(
                                "selected linkage edit has incomplete references or stored function addresses".into(),
                            );
                        }
                        if site
                            .call
                            .callee_span
                            .as_ref()
                            .is_none_or(|s| text(files, s).ok() != Some(site.call.callee.as_str()))
                        {
                            return Err("exact compiler callee designator/source-buffer identity missing".into());
                        }
                    }
                    if !definition.name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                        return Err("provider is not a C identifier".into());
                    }
                    if definition.linkage == "internal"
                        && (definition.unit != caller.unit || definition.span.start >= caller.source_span.start)
                    {
                        return Err(
                            "internal provider declaration is not in scope before the wrapper insertion".into(),
                        );
                    }
                    let call = &site.call;
                    let candidate_call = if policy.kind == "missing-inputs" {
                        let id = policy
                            .closure_certificates
                            .iter()
                            .find_map(|id| {
                                w.proof_closures
                                    .iter()
                                    .find(|r| r.id == *id)
                                    .and_then(|r| match &r.claim {
                                        crate::proofclosure::Claim::MissingInputs {
                                            call: original,
                                            candidate_call,
                                            ..
                                        } if *original == call.id => Some(candidate_call),
                                        _ => None,
                                    })
                            })
                            .ok_or("missing-input source candidate absent")?;
                        Some(
                            w.compiler_facts
                                .calls
                                .iter()
                                .find(|c| c.id == *id)
                                .ok_or("candidate compiler call absent")?,
                        )
                    } else {
                        None
                    };
                    let artifact = &call.span.artifact;
                    if !caller.source_span.editable
                        || caller.source_span.start > call.span.start
                        || caller.source_span.end < call.span.end
                        || caller.source_span.artifact != *artifact
                        || call.arguments.iter().any(|a| {
                            a.span.artifact != *artifact || a.span.start < call.span.start || a.span.end > call.span.end
                        })
                    {
                        return Err("call/argument/insertion spans have different compiler provenance".into());
                    }
                    let before = text(files, &call.span)?.to_owned();
                    let original = std::str::from_utf8(files.get(artifact).ok_or("source missing")?)
                        .map_err(|_| "source is not UTF-8")?;
                    let name = format!("binviz_bridge_{}", &sha256(call.id.as_bytes())[..16]);
                    if original.contains(&name) {
                        return Err("generated bridge identifier already occurs in input".into());
                    }
                    let mut args = call
                        .arguments
                        .iter()
                        .map(|a| text(files, &a.span))
                        .collect::<Result<Vec<_>, _>>()?;
                    if let Some(candidate) = candidate_call {
                        args.extend(
                            candidate.arguments[call.arguments.len()..]
                                .iter()
                                .map(|a| text(files, &a.span))
                                .collect::<Result<Vec<_>, _>>()?,
                        );
                    }
                    let helper_arguments = candidate_call.map_or(&call.arguments, |c| &c.arguments);
                    let actual = helper_arguments
                        .iter()
                        .enumerate()
                        .map(|(i, a)| Ok(format!("{} arg{i}", c_type(&a.promoted)?)))
                        .collect::<Result<Vec<_>, String>>()?
                        .join(", ");
                    let mut formal = definition
                        .abi_parameters
                        .iter()
                        .map(c_type)
                        .collect::<Result<Vec<_>, _>>()?
                        .join(", ");
                    if definition.variadic {
                        formal.push_str(if formal.is_empty() { "..." } else { ", ..." });
                    }
                    let mut used = definition
                        .abi_parameters
                        .iter()
                        .enumerate()
                        .map(|(i, t)| Ok(format!("({})arg{i}", c_type(t)?)))
                        .collect::<Result<Vec<_>, String>>()?
                        .join(", ");
                    if definition.variadic {
                        let extras = (definition.abi_parameters.len()..helper_arguments.len())
                            .map(|i| format!("arg{i}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        if !extras.is_empty() {
                            if !used.is_empty() {
                                used.push_str(", ");
                            }
                            used.push_str(&extras);
                        }
                    }
                    let ret = c_type(&definition.return_type)?;
                    let declaration = format!(
                        "extern {ret} {}({});",
                        definition.name,
                        if formal.is_empty() { "void" } else { &formal }
                    );
                    let expression = format!("{}({used})", definition.name);
                    let ignored = (if definition.variadic {
                        call.arguments.len()
                    } else {
                        definition.abi_parameters.len()
                    }..call.arguments.len())
                        .map(|i| format!("(void)arg{i}; "))
                        .collect::<String>();
                    let body = if discard {
                        format!("(void){expression};")
                    } else if definition.return_type.category == "void" {
                        format!("{expression};")
                    } else {
                        format!("return {expression};")
                    };
                    let helper_ret = if discard { "void" } else { ret };
                    let helper = format!(
                        "\n/* Binviz direct-call adapter: {}. Argument expressions are evaluated once. */\nstatic {helper_ret} {name}({actual}) {{ {declaration} {ignored}{body} }}\n",
                        call.id
                    );
                    Ok((
                        Edit {
                            start: format!("0x{:x}", call.span.start),
                            end: format!("0x{:x}", call.span.end),
                            before,
                            replacement: format!("{name}({})", args.join(", ")),
                            kind: "direct-call".into(),
                            calls: vec![call.id.clone()],
                            policies: vec![policy.id.clone()],
                        },
                        helper,
                    ))
                })();
                match build {
                    Err(reason) => result.refused.push(Refusal {
                        call: site.call.id.clone(),
                        reasons: vec![reason],
                    }),
                    Ok((edit, helper)) => {
                        let declaration = w
                            .compiler_facts
                            .definitions
                            .iter()
                            .find(|d| d.id == site.call.declaration)
                            .unwrap();
                        result.references.push(serde_json::json!({"call":site.call.id,"nativeIdentity":site.call.callee,"binding":site.binding,"declaration":declaration,"directSpan":site.call.callee_span,"addressReferences":w.compiler_facts.address_references.iter().filter(|r|r.declaration==declaration.id).collect::<Vec<_>>(),"sourceBuffer":site.call.span.artifact,"complete":caller.extraction_gaps.is_empty()}));
                        let edits = changes.entry(site.call.span.artifact.clone()).or_default();
                        if edits.iter().any(|e| e.calls == edit.calls) {
                            continue;
                        }
                        edits.push(edit);
                        helpers
                            .entry((caller.source_span.artifact.clone(), caller.source_span.start))
                            .or_default()
                            .push((site.call.id.clone(), policy.id.clone(), helper));
                        for id in policy
                            .dependencies
                            .iter()
                            .chain(std::iter::once(&policy.review_artifact))
                        {
                            let a = w.evidence.artifacts.iter().find(|a| a.id == *id).unwrap();
                            result.dependencies.insert(id.clone(), a.sha256.clone());
                        }
                    }
                }
            }
        }
    }
    for ((artifact, at), mut list) in helpers {
        list.sort();
        changes.entry(artifact).or_default().push(Edit {
            start: format!("0x{at:x}"),
            end: format!("0x{at:x}"),
            before: String::new(),
            replacement: list.iter().map(|r| r.2.as_str()).collect(),
            kind: "typed-bridge-declaration".into(),
            calls: list.iter().map(|r| r.0.clone()).collect(),
            policies: list.iter().map(|r| r.1.clone()).collect(),
        });
    }
    for (artifact, mut edits) in changes {
        edits.sort_by_key(|e| (hex(&e.start).unwrap(), hex(&e.end).unwrap()));
        if edits
            .windows(2)
            .any(|es| hex(&es[0].end).unwrap() > hex(&es[1].start).unwrap())
        {
            result.refused.push(Refusal {
                call: artifact,
                reasons: vec!["overlapping compiler edits; no candidate emitted".into()],
            });
            continue;
        }
        let before = files.get(&artifact).ok_or("source missing")?;
        let source = apply_edits(before, &edits)?;
        result.candidates.push(Candidate {
            artifact,
            before_sha256: sha256(before),
            after_sha256: sha256(source.as_bytes()),
            edits,
            source,
        });
    }
    result.already_applied.sort();
    result.already_applied.dedup();
    Ok(result)
}
pub(crate) fn apply_edits(bytes: &[u8], edits: &[Edit]) -> Result<String, String> {
    let original = std::str::from_utf8(bytes).map_err(|_| "prepared C is not UTF-8")?;
    let mut out = original.to_owned();
    let mut previous = bytes.len() as u64;
    for e in edits.iter().rev() {
        let start = hex(&e.start)?;
        let end = hex(&e.end)?;
        if start > end || end > previous {
            return Err("overlapping/unordered/out-of-bounds edit plan".into());
        }
        let start = usize::try_from(start).map_err(|_| "edit start exceeds host")?;
        let end = usize::try_from(end).map_err(|_| "edit end exceeds host")?;
        if original.get(start..end) != Some(&e.before) {
            return Err("edit before bytes differ or span is not UTF-8 aligned".into());
        }
        out.replace_range(start..end, &e.replacement);
        previous = start as u64;
    }
    Ok(out)
}
/// Re-validate every byte edit, including on imported plans. Idempotent application
/// recognizes the exact candidate digest; drift never becomes another edit.
pub fn apply(candidate: &Candidate, bytes: &[u8]) -> Result<(String, bool), String> {
    if sha256(candidate.source.as_bytes()) != candidate.after_sha256 {
        return Err("candidate payload differs from plan digest".into());
    }
    if sha256(bytes) == candidate.after_sha256 {
        return Ok((candidate.source.clone(), true));
    }
    if sha256(bytes) != candidate.before_sha256 {
        return Err("input changed since plan creation".into());
    }
    let source = apply_edits(bytes, &candidate.edits)?;
    if source != candidate.source {
        return Err("candidate differs from exact edit reconstruction".into());
    }
    Ok((source, false))
}
