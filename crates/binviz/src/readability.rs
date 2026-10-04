//! Accept game-owned readability candidates using full objects and original scores.
use crate::{
    evidence::{IdentityState, sha256},
    workspace::Workspace,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub review_artifact: String,
    pub dependencies: BTreeMap<String, String>,
    pub pairs: Vec<Pair>,
    pub affected_callers: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pair {
    pub function: String,
    pub baseline_definition: String,
    pub candidate_definition: String,
    pub baseline_object: String,
    pub candidate_object: String,
    pub baseline_stage: String,
    pub candidate_stage: String,
    pub baseline_percent: f32,
}
fn visible(recipe: &Value, stage: &crate::evidence::StageRecord) -> Result<Value, String> {
    if recipe["compilerVisibleNamespace"].as_str().is_none_or(str::is_empty) {
        return Err("readability stages require a stable compiler-visible namespace".into());
    }
    let slots = recipe["inputSlots"].as_object().ok_or("input slot mapping missing")?;
    let mut names = BTreeMap::new();
    for (id, slot) in slots {
        let slot = slot.as_str().filter(|s| !s.is_empty()).ok_or("invalid visible slot")?;
        let name = recipe["inputNames"][id]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("visible input filename missing")?;
        if names.insert(slot.to_owned(), name.to_owned()).is_some() {
            return Err("duplicate compiler-visible input slot".into());
        }
    }
    let mut command = recipe["command"].as_array().ok_or("build command missing")?.clone();
    for arg in &mut command {
        if let Some(s) = arg.as_str() {
            let mut s = s.to_owned();
            for (id, slot) in recipe["inputSlots"].as_object().ok_or("input slot mapping missing")? {
                s = s.replace(
                    &format!("{{input:{id}}}"),
                    &format!("{{input:{}}}", slot.as_str().ok_or("invalid visible slot")?),
                );
            }
            for id in &stage.outputs {
                s = s.replace(&format!("{{output:{id}}}"), "{output:object}");
            }
            *arg = json!(s);
        }
    }
    Ok(
        json!({"namespace":recipe["compilerVisibleNamespace"],"command":command,"inputNames":names,"tools":recipe["tools"],"environmentDigests":recipe["environmentDigests"],"includes":recipe["includes"],"outputs":recipe["outputs"].as_object().ok_or("outputs missing")?.values().collect::<Vec<_>>()}),
    )
}
fn check(w: &Workspace, r: &Request, files: &BTreeMap<String, Vec<u8>>) -> Result<Value, String> {
    let checks = w.evidence.verify(Some(files))?;
    if r.pairs.is_empty() || r.dependencies.is_empty() {
        return Err("readability acceptance needs selected pairs and frozen inputs".into());
    }
    for (id, pin) in &r.dependencies {
        if files.get(id).is_none_or(|b| sha256(b) != *pin)
            || !checks.iter().any(|c| c.id == *id && c.state == IdentityState::Verified)
        {
            return Err(format!("readability input {id} drifted or is unverified"));
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
        return Err("readability review differs from exact candidate request".into());
    }
    let inventory = w.inventory.analyze(&w.evidence, Some(files))?;
    let mut rows = vec![];
    let mut identities = BTreeSet::new();
    let mut affected = BTreeSet::new();
    for p in &r.pairs {
        if p.baseline_definition == p.candidate_definition
            || p.baseline_object == p.candidate_object
            || p.baseline_stage == p.candidate_stage
        {
            return Err("readability requires separate baseline and candidate namespaces".into());
        }
        if !identities.insert(&p.function) {
            return Err("duplicate native readability identity".into());
        }
        let native = inventory
            .units
            .iter()
            .flat_map(|u| &u.functions)
            .find(|f| f.function.id == p.function)
            .ok_or("native readability identity missing")?;
        if native.state != IdentityState::Verified || native.retired || native.function.owner.is_some() {
            return Err("native readability ownership not current".into());
        }
        let definitions: Result<Vec<_>, _> = [&p.baseline_definition, &p.candidate_definition]
            .iter()
            .map(|id| {
                w.compiler_facts
                    .definitions
                    .iter()
                    .find(|d| d.id == **id && d.has_body)
                    .ok_or("readability compiler body missing")
            })
            .collect();
        let d = definitions?;
        if d[0].name != d[1].name
            || d[0].linkage != d[1].linkage
            || d[0].abi_parameters != d[1].abi_parameters
            || d[0].return_type != d[1].return_type
            || d[0].variadic != d[1].variadic
        {
            return Err("canonical prepared symbol identity or compiler ABI changed".into());
        }
        for d in &d {
            if !r.dependencies.contains_key(&d.span.artifact) {
                return Err("readability omits prepared definition dependency".into());
            }
            affected.insert(d.id.clone());
        }
        if files.get(&p.baseline_object).is_none() || files.get(&p.candidate_object) != files.get(&p.baseline_object) {
            return Err("complete native object bytes differ or are missing".into());
        }
        let mut recipes = vec![];
        for (stage, object, definition) in [
            (&p.baseline_stage, &p.baseline_object, d[0]),
            (&p.candidate_stage, &p.candidate_object, d[1]),
        ] {
            if !r.dependencies.contains_key(object) {
                return Err("readability omits compiled object dependency".into());
            }
            let s = w
                .evidence
                .stages
                .iter()
                .find(|s| s.id == *stage && s.outputs.contains(object) && s.result == "success")
                .ok_or("readability successful object producer missing")?;
            if !checks
                .iter()
                .any(|c| c.id == s.id && c.state == IdentityState::Verified)
                || !crate::publication::ancestors(w, &s.id).contains(&definition.span.artifact)
            {
                return Err("readability producer is stale or omits current prepared source".into());
            }
            if crate::publication::ancestors(w, &s.id)
                .iter()
                .any(|id| w.evidence.artifacts.iter().any(|a| a.id == *id) && !r.dependencies.contains_key(id))
            {
                return Err("readability omits frozen source/header/tool/recipe inputs".into());
            }
            let recipe: Value = serde_json::from_slice(files.get(&s.recipe).ok_or("readability recipe missing")?)
                .map_err(|e| e.to_string())?;
            recipes.push(visible(&recipe, s)?);
        }
        if recipes[0] != recipes[1] {
            return Err(
                "baseline/candidate compiler-visible paths, flags, includes, tools or environment differ".into(),
            );
        }
        let unit = w
            .inventory
            .units
            .iter()
            .find(|u| u.identity.id == native.function.identity.unit)
            .unwrap();
        if unit.architecture != "ps1-mipsel" {
            return Err("original readability scores currently require PS1 MIPS objects".into());
        }
        let mut binary = crate::Binary::parse_psx_overlay(
            files[&unit.member_artifact].clone(),
            crate::evidence::hex(&unit.identity.load_address)?,
            None,
        )
        .map_err(|e| e.to_string())?;
        binary.set_annotations(
            unit.functions
                .iter()
                .map(|f| crate::Annotation {
                    address: crate::evidence::hex(&f.identity.entry).unwrap(),
                    name: f.name.clone(),
                    ..Default::default()
                })
                .collect(),
        );
        let span = native
            .function
            .identity
            .matching_extent
            .as_ref()
            .filter(|s| s.exact)
            .ok_or("exact original matching range missing")?;
        let object = crate::matching::object_functions(&files[&p.baseline_object]).map_err(|e| e.to_string())?;
        let f = crate::matching::find_function(&object, &d[0].name).ok_or("canonical object symbol missing")?;
        for rel in f.relocs.values().filter(|r| !r.section_symbol) {
            if binary.object_symbol_address(&rel.symbol).is_none() {
                let address = crate::matching::address_in_name(&rel.symbol)
                    .ok_or("strict readability relocation target unresolved")?;
                let mut labels = binary.annotations().to_vec();
                labels.push(crate::Annotation {
                    address,
                    name: rel.symbol.clone(),
                    ..Default::default()
                });
                binary.set_annotations(labels);
            }
        }
        let start = crate::evidence::hex(&span.start)?;
        let end = start
            .checked_add(crate::evidence::hex(&span.bytes)?)
            .ok_or("readability matching extent overflow")?;
        let score = binary.match_range(start, end, f).map_err(|e| e.to_string())?;
        if score.percent != p.baseline_percent {
            return Err("recomputed original-code score differs from pinned baseline".into());
        }
        rows.push(json!({"pair":p,"completeObjectEqual":true,"baselineScore":score.score(&d[0].unit),"candidateScore":score.score(&d[1].unit),"compilerVisibleRecipe":recipes[0],"matchingCreditChange":0}));
    }
    if affected != r.affected_callers.iter().cloned().collect() {
        return Err("affected compiler caller scope differs from accepted pair definitions".into());
    }
    Ok(
        json!({"id":r.id,"state":"ready-for-review","pairs":rows,"affectedCallers":affected,"publication":"candidate source only; regenerate downstream source-bound evidence","linkedOrGameplayProof":false}),
    )
}
pub fn plans(w: &Workspace, files: &BTreeMap<String, Vec<u8>>) -> Vec<Value> {
    w.readability_batches
        .iter()
        .map(|r| match check(w, r, files) {
            Ok(v) => v,
            Err(reason) => json!({"id":r.id,"state":"refused","reasons":[reason],"request":r}),
        })
        .collect()
}
