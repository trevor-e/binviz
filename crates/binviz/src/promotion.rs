//! Reviewable before/candidate transitions. Planning never overwrites accepted
//! artifacts or silently changes policy dependencies; historical inputs survive.
use crate::evidence::{EvidenceManifest, IdentityCheck, IdentityState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub replacements: Vec<Replacement>,
    pub prerequisites: Vec<String>,
    pub affected_consumers: Vec<String>,
    pub scope_changes: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Replacement {
    pub accepted: String,
    pub candidate: String,
    pub baseline_stage: String,
    pub candidate_stage: String,
    pub baseline_recipe_sha256: String,
    pub candidate_recipe_sha256: String,
}
pub fn validate(e: &EvidenceManifest, requests: &[Request]) -> Result<(), String> {
    let artifacts: BTreeSet<_> = e.artifacts.iter().map(|a| a.id.as_str()).collect();
    let mut ids = BTreeSet::new();
    for p in requests {
        if p.id.is_empty()
            || !ids.insert(&p.id)
            || p.replacements.is_empty()
            || p.prerequisites.is_empty()
            || p.prerequisites.iter().any(|id| !artifacts.contains(id.as_str()))
        {
            return Err("invalid promotion identity or prerequisites".into());
        }
        let mut accepted = BTreeSet::new();
        let mut candidates = BTreeSet::new();
        for r in &p.replacements {
            if r.accepted == r.candidate
                || !accepted.insert(&r.accepted)
                || !candidates.insert(&r.candidate)
                || !artifacts.contains(r.accepted.as_str())
                || !artifacts.contains(r.candidate.as_str())
            {
                return Err("promotion requires distinct, unique accepted and candidate artifacts".into());
            }
            for (stage, output, digest) in [
                (&r.baseline_stage, &r.accepted, &r.baseline_recipe_sha256),
                (&r.candidate_stage, &r.candidate, &r.candidate_recipe_sha256),
            ] {
                if digest.len() != 64
                    || !digest.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                    || !e.stages.iter().any(|s| s.id == *stage && s.outputs.contains(output))
                {
                    return Err("promotion requires exact producer stages and recipe digests".into());
                }
            }
            if r.baseline_stage == r.candidate_stage {
                return Err("candidate must use a separate stage namespace".into());
            }
        }
        if accepted.iter().any(|id| candidates.contains(id)) {
            return Err("promotion candidate overlaps accepted namespace".into());
        }
    }
    Ok(())
}
pub fn plans(e: &EvidenceManifest, checks: &[IdentityCheck], requests: &[Request]) -> Result<Vec<Value>, String> {
    validate(e, requests)?;
    let current = |id: &str| checks.iter().any(|c| c.id == id && c.state == IdentityState::Verified);
    let mut out = Vec::new();
    for p in requests {
        let mut reasons = Vec::new();
        let mut transitions = Vec::new();
        let mut invalidated: BTreeSet<String> = p.replacements.iter().map(|r| r.accepted.clone()).collect();
        loop {
            let before = invalidated.len();
            for s in &e.stages {
                if s.parents
                    .iter()
                    .chain(std::iter::once(&s.recipe))
                    .any(|a| invalidated.contains(a))
                {
                    invalidated.insert(s.id.clone());
                    invalidated.extend(s.outputs.iter().cloned());
                }
            }
            if invalidated.len() == before {
                break;
            }
        }
        // The replacements themselves are retained as history, not erased.
        let affected: Vec<_> = e
            .stages
            .iter()
            .filter(|s| invalidated.contains(&s.id) && !p.replacements.iter().any(|r| r.baseline_stage == s.id))
            .map(|s| s.id.clone())
            .collect();
        if affected.iter().any(|id| !p.affected_consumers.contains(id))
            || p.affected_consumers.iter().any(|id| !affected.contains(id))
        {
            reasons.push("affectedConsumers differs from the exact dependent-stage closure".to_string());
        }
        for id in &p.prerequisites {
            if !current(id) {
                reasons.push(format!("prerequisite {id} is not current"));
            }
        }
        for r in &p.replacements {
            let mut recipes = BTreeMap::new();
            for (kind, stage, output, digest) in [
                ("baseline", &r.baseline_stage, &r.accepted, &r.baseline_recipe_sha256),
                (
                    "candidate",
                    &r.candidate_stage,
                    &r.candidate,
                    &r.candidate_recipe_sha256,
                ),
            ] {
                let s = e.stages.iter().find(|s| s.id == *stage).unwrap();
                let recipe = e.artifacts.iter().find(|a| a.id == s.recipe).unwrap();
                if !current(stage) || !current(output) || s.result != "success" {
                    reasons.push(format!(
                        "{kind} stage/output {stage}/{output} is not independently current"
                    ));
                }
                if recipe.sha256 != *digest {
                    reasons.push(format!(
                        "{kind} flags, namespace or transformation recipe differs from the pinned plan"
                    ));
                }
                recipes.insert(kind, json!({"stage":s,"recipe":recipe}));
            }
            let before = e.artifacts.iter().find(|a| a.id == r.accepted).unwrap();
            let after = e.artifacts.iter().find(|a| a.id == r.candidate).unwrap();
            if before.location == after.location {
                reasons.push("candidate output shares the accepted artifact location".into());
            }
            transitions.push(json!({"accepted":before,"candidate":after,"contentChanged":before.sha256!=after.sha256,"reproduction":recipes}));
        }
        out.push(json!({"id":p.id,"state":if reasons.is_empty(){"ready-for-review"}else{"refused"},"reasons":reasons,"replacements":transitions,"affectedConsumers":affected,"scopeChanges":p.scope_changes,"historicalArtifactsRetained":true,"publication":"none; rebuild both namespaces with build-batch, review exact replacements, then regenerate consumers and policies","recoverableOutcome":"accepted inputs remain unchanged; candidate outputs and stage records remain separately inspectable"}));
    }
    Ok(out)
}
