//! Content identities and stage lineage. Importing records never verifies them.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Include-search snapshots bind names as well as contents: a newly shadowing
/// header invalidates preparation even if the previously selected file survives.
#[cfg(not(target_arch = "wasm32"))]
pub fn directory_index(root: &std::path::Path) -> Result<Vec<u8>, String> {
    fn walk(root: &std::path::Path, path: &std::path::Path, files: &mut Vec<(String, String)>) -> Result<(), String> {
        for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let p = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                walk(root, &p, files)?;
            } else if kind.is_symlink() && p.is_dir() {
                return Err("directory symlinks require an explicit include snapshot".into());
            } else if p.is_file() {
                let name = p
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("non-UTF-8 include path")?
                    .replace('\\', "/");
                files.push((name, sha256(&std::fs::read(p).map_err(|e| e.to_string())?)));
            }
        }
        Ok(())
    }
    let mut files = vec![];
    walk(root, root, &mut files)?;
    files.sort();
    serde_json::to_vec(&files).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactRef {
    pub id: String,
    pub role: String,
    pub location: String,
    pub sha256: String,
    #[serde(default)]
    pub normalized_sha256: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StageRecord {
    pub id: String,
    pub parents: Vec<String>,
    pub outputs: Vec<String>,
    /// Content-addressed recipe artifact, including flags and tool versions.
    pub recipe: String,
    pub result: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnitId {
    pub id: String,
    pub asset: String,
    pub member_offset: String,
    pub member_size: String,
    pub load_address: String,
    pub context: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FunctionId {
    pub unit: String,
    pub entry: String,
    pub role: String,
    pub analysis_extent: Option<NativeSpan>,
    pub matching_extent: Option<NativeSpan>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeSpan {
    pub start: String,
    pub bytes: String,
    pub exact: bool,
}

/// Hex strings are mandatory for new interchange addresses (including >2^53).
pub fn hex(value: &str) -> Result<u64, String> {
    value
        .strip_prefix("0x")
        .filter(|s| !s.is_empty())
        .and_then(|s| u64::from_str_radix(s, 16).ok())
        .ok_or_else(|| format!("expected exact 0x hex string: {value}"))
}
impl UnitId {
    pub fn validate_function(&self, function: &FunctionId) -> Result<(), String> {
        let start = hex(&self.load_address)?;
        let end = start
            .checked_add(hex(&self.member_size)?)
            .ok_or("unit range overflow")?;
        hex(&self.member_offset)?;
        let entry = hex(&function.entry)?;
        if function.unit != self.id || entry < start || entry >= end {
            return Err("function entry is outside its unit".into());
        }
        for span in [&function.analysis_extent, &function.matching_extent]
            .into_iter()
            .flatten()
        {
            let at = hex(&span.start)?;
            let size = hex(&span.bytes)?;
            let limit = at.checked_add(size).ok_or("function range overflow")?;
            if size == 0 || at < start || limit > end || entry < at || entry >= limit {
                return Err("function extent crosses its physical member or omits entry".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceManifest {
    pub artifacts: Vec<ArtifactRef>,
    pub stages: Vec<StageRecord>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdentityState {
    Unverified,
    Verified,
    Stale,
    Missing,
    Unsupported,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityCheck {
    pub id: String,
    pub state: IdentityState,
    /// Raw bytes can match even when a derived artifact's lineage is stale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_state: Option<IdentityState>,
    pub reasons: Vec<String>,
}
impl EvidenceManifest {
    /// Complete artifact ancestry, including recipes, tools and scan inventories.
    pub fn dependency_closure(&self, id: &str) -> Result<std::collections::BTreeSet<String>, String> {
        self.validate()?;
        if !self.artifacts.iter().any(|a| a.id == id) {
            return Err("unknown producer artifact".into());
        }
        let mut ids = std::collections::BTreeSet::from([id.to_owned()]);
        loop {
            let count = ids.len();
            for s in &self.stages {
                if s.outputs.iter().any(|o| ids.contains(o)) {
                    if s.result != "success" {
                        return Err(format!("producer {} did not complete", s.id));
                    }
                    ids.extend(s.parents.clone());
                    ids.insert(s.recipe.clone());
                }
            }
            if ids.len() == count {
                break;
            }
        }
        Ok(ids)
    }
    /// Desktop adapter: paths resolve relative to the report or explicit root.
    /// Each distinct file is read once. Read failures remain missing identities.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read_artifacts(&self, root: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
        let mut files = BTreeMap::new();
        let mut cache = BTreeMap::new();
        for a in &self.artifacts {
            let path = root.join(&a.location);
            let bytes = cache
                .entry((path.clone(), a.role == "include-directory"))
                .or_insert_with(|| {
                    if a.role == "include-directory" {
                        directory_index(&path).ok()
                    } else {
                        std::fs::read(path).ok()
                    }
                });
            if let Some(bytes) = bytes {
                files.insert(a.id.clone(), bytes.clone());
            }
        }
        files
    }
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        for a in &self.artifacts {
            if a.id.trim().is_empty() || a.location.trim().is_empty() || !ids.insert(a.id.as_str()) {
                return Err("empty/duplicate artifact identity or location".into());
            }
            for digest in std::iter::once(&a.sha256).chain(a.normalized_sha256.iter()) {
                if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
                    return Err(format!("invalid SHA-256 for {}", a.id));
                }
            }
        }
        let mut stages = BTreeSet::new();
        let mut producers = BTreeMap::new();
        for s in &self.stages {
            if s.id.is_empty() || ids.contains(s.id.as_str()) || !stages.insert(s.id.as_str()) || s.outputs.is_empty() {
                return Err("empty/duplicate stage identity or outputs".into());
            }
            for id in s
                .parents
                .iter()
                .chain(s.outputs.iter())
                .chain(std::iter::once(&s.recipe))
            {
                if !ids.contains(id.as_str()) {
                    return Err(format!("stage {} references unknown artifact {id}", s.id));
                }
            }
            for id in &s.outputs {
                if producers.insert(id, s).is_some() {
                    return Err(format!("multiple producers for {id}"));
                }
            }
        }
        fn visit<'a>(
            id: &'a str,
            producers: &BTreeMap<&String, &'a StageRecord>,
            visiting: &mut BTreeSet<&'a str>,
            done: &mut BTreeSet<&'a str>,
        ) -> Result<(), String> {
            if done.contains(id) {
                return Ok(());
            }
            if !visiting.insert(id) {
                return Err(format!("cyclic stage lineage at {id}"));
            }
            if let Some(s) = producers.get(&id.to_string()) {
                for parent in s.parents.iter().chain(std::iter::once(&s.recipe)) {
                    visit(parent, producers, visiting, done)?;
                }
            }
            visiting.remove(id);
            done.insert(id);
            Ok(())
        }
        let mut done = BTreeSet::new();
        for a in &self.artifacts {
            visit(&a.id, &producers, &mut BTreeSet::new(), &mut done)?;
        }
        Ok(())
    }
    /// None means no verification attempted; Some(empty) means all files missing.
    /// Supplied values are raw bytes, never trusted caller-supplied hashes.
    pub fn verify(&self, supplied: Option<&BTreeMap<String, Vec<u8>>>) -> Result<Vec<IdentityCheck>, String> {
        self.validate()?;
        let mut checks = BTreeMap::new();
        for a in &self.artifacts {
            let (state, reasons) = match supplied {
                None => (IdentityState::Unverified, vec!["artifact bytes not supplied".into()]),
                Some(files) => match files.get(&a.id) {
                    None => (IdentityState::Missing, vec![format!("missing {}", a.location)]),
                    Some(bytes) if sha256(bytes) != a.sha256 => {
                        (IdentityState::Stale, vec![format!("raw bytes changed: {}", a.location)])
                    }
                    Some(bytes)
                        if a.normalized_sha256.as_ref().is_some_and(|expected| {
                            std::str::from_utf8(bytes)
                                .map(|s| sha256(s.replace("\r\n", "\n").as_bytes()) != *expected)
                                .unwrap_or(true)
                        }) =>
                    {
                        (IdentityState::Stale, vec!["normalized text identity differs".into()])
                    }
                    Some(_) => (IdentityState::Verified, vec![]),
                },
            };
            checks.insert(
                a.id.clone(),
                IdentityCheck {
                    id: a.id.clone(),
                    content_state: Some(state.clone()),
                    state,
                    reasons,
                },
            );
        }
        // The DAG is validated; monotone invalidation reaches every dependent.
        for _ in 0..=self.stages.len() {
            for s in &self.stages {
                let mut reasons: Vec<_> = s
                    .parents
                    .iter()
                    .chain(std::iter::once(&s.recipe))
                    .filter(|id| checks[*id].state != IdentityState::Verified)
                    .map(|id| format!("dependency {id}: {:?}", checks[id].state))
                    .collect();
                if s.result != "success" {
                    reasons.push(format!("stage {} result: {}", s.id, s.result));
                }
                if reasons.is_empty() {
                    continue;
                }
                for id in &s.outputs {
                    let check = checks.get_mut(id).unwrap();
                    if check.state == IdentityState::Verified {
                        check.state = IdentityState::Stale;
                    }
                    for reason in &reasons {
                        if !check.reasons.contains(reason) {
                            check.reasons.push(reason.clone());
                        }
                    }
                }
            }
        }
        let mut result: Vec<_> = self.artifacts.iter().map(|a| checks.remove(&a.id).unwrap()).collect();
        for s in &self.stages {
            let dependencies: Vec<_> = s
                .parents
                .iter()
                .chain(s.outputs.iter())
                .chain(std::iter::once(&s.recipe))
                .collect();
            let reasons: Vec<_> = dependencies
                .iter()
                .filter_map(|id| {
                    let c = result.iter().find(|c| &c.id == *id).unwrap();
                    (c.state != IdentityState::Verified).then(|| format!("artifact {}: {:?}", c.id, c.state))
                })
                .chain((s.result != "success").then(|| format!("stage result: {}", s.result)))
                .collect();
            result.push(IdentityCheck {
                id: s.id.clone(),
                content_state: None,
                state: if reasons.is_empty() {
                    IdentityState::Verified
                } else if supplied.is_none() {
                    IdentityState::Unverified
                } else {
                    IdentityState::Stale
                },
                reasons,
            });
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identities_use_bytes_and_invalidate_only_dependents() {
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let artifact = |id: &str| ArtifactRef {
            id: id.into(),
            role: "test".into(),
            location: id.into(),
            sha256: sha256(id.as_bytes()),
            normalized_sha256: None,
        };
        let m = EvidenceManifest {
            artifacts: ["source", "recipe", "cpp", "facts", "unrelated"]
                .into_iter()
                .map(artifact)
                .collect(),
            stages: vec![
                StageRecord {
                    id: "prepare".into(),
                    parents: vec!["source".into()],
                    outputs: vec!["cpp".into()],
                    recipe: "recipe".into(),
                    result: "success".into(),
                },
                StageRecord {
                    id: "extract".into(),
                    parents: vec!["cpp".into()],
                    outputs: vec!["facts".into()],
                    recipe: "recipe".into(),
                    result: "success".into(),
                },
            ],
        };
        assert!(
            m.verify(None)
                .unwrap()
                .iter()
                .all(|c| c.state == IdentityState::Unverified)
        );
        let mut bytes: BTreeMap<_, _> = m
            .artifacts
            .iter()
            .map(|a| (a.id.clone(), a.id.as_bytes().to_vec()))
            .collect();
        assert!(
            m.verify(Some(&bytes))
                .unwrap()
                .iter()
                .all(|c| c.state == IdentityState::Verified)
        );
        bytes.insert("source".into(), b"source\r\n".to_vec());
        let result = m.verify(Some(&bytes)).unwrap();
        for id in ["source", "cpp", "facts", "prepare", "extract"] {
            assert_eq!(result.iter().find(|c| c.id == id).unwrap().state, IdentityState::Stale);
        }
        assert_eq!(
            result.iter().find(|c| c.id == "cpp").unwrap().content_state,
            Some(IdentityState::Verified)
        );
        assert_eq!(
            result.iter().find(|c| c.id == "unrelated").unwrap().state,
            IdentityState::Verified
        );
        bytes.remove("recipe");
        assert_eq!(m.verify(Some(&bytes)).unwrap()[1].state, IdentityState::Missing);
    }
    #[test]
    fn exact_member_spans_and_large_addresses() {
        let u = UnitId {
            id: "overlay-a".into(),
            asset: "disc".into(),
            member_offset: "0x10".into(),
            member_size: "0x138".into(),
            load_address: "0x20000000000001".into(),
            context: "a".into(),
        };
        let mut f = FunctionId {
            unit: u.id.clone(),
            entry: u.load_address.clone(),
            role: "primary".into(),
            analysis_extent: Some(NativeSpan {
                start: u.load_address.clone(),
                bytes: "0x138".into(),
                exact: true,
            }),
            matching_extent: None,
        };
        assert!(u.validate_function(&f).is_ok());
        f.analysis_extent.as_mut().unwrap().bytes = "0x13c".into();
        assert!(u.validate_function(&f).is_err());
        f.unit = "overlay-b".into();
        assert!(u.validate_function(&f).is_err());
    }

    #[test]
    fn cycles_and_output_corruption_are_refused() {
        let mut manifest = EvidenceManifest {
            artifacts: ["a", "b", "recipe"]
                .into_iter()
                .map(|id| ArtifactRef {
                    id: id.into(),
                    role: "test".into(),
                    location: id.into(),
                    sha256: sha256(id.as_bytes()),
                    normalized_sha256: None,
                })
                .collect(),
            stages: vec![StageRecord {
                id: "compile".into(),
                parents: vec!["a".into()],
                outputs: vec!["b".into()],
                recipe: "recipe".into(),
                result: "success".into(),
            }],
        };
        let bytes = BTreeMap::from([
            ("a".into(), b"a".to_vec()),
            ("b".into(), b"corrupt output".to_vec()),
            ("recipe".into(), b"recipe".to_vec()),
        ]);
        let result = manifest.verify(Some(&bytes)).unwrap();
        assert_eq!(result.iter().find(|c| c.id == "b").unwrap().state, IdentityState::Stale);
        assert_eq!(
            result.iter().find(|c| c.id == "compile").unwrap().state,
            IdentityState::Stale
        );
        manifest.stages.push(StageRecord {
            id: "cycle".into(),
            parents: vec!["b".into()],
            outputs: vec!["a".into()],
            recipe: "recipe".into(),
            result: "success".into(),
        });
        assert!(manifest.validate().unwrap_err().contains("cyclic"));
    }
}
