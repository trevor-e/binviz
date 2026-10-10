//! Optional provider observations. Identity checks do not authenticate a provider
//! or turn its pseudocode/dataflow into native proof, annotations or matching credit.
use crate::{
    Binary,
    evidence::{hex, sha256},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Observation {
    pub format: String,
    pub schema_version: u32,
    pub provider: Provider,
    pub target_sha256: String,
    pub architecture: String,
    pub address_space: String,
    pub entry: String,
    /// Provider-selected extent; it never changes binviz's function boundaries.
    pub start: String,
    pub bytes: String,
    pub native_sha256: String,
    pub pseudocode: Option<String>,
    pub dataflow: Option<Value>,
    pub evidence_ids: Vec<String>,
    pub unknowns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Provider {
    pub name: String,
    pub version: String,
    pub profile_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub format: String,
    pub schema_version: u32,
    pub observation_sha256: String,
    pub identity_state: String,
    pub authority: String,
    pub limitations: Vec<String>,
    pub observation: Observation,
}

fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn inspect(binary: &Binary, bytes: &[u8]) -> Result<Report, String> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("observation exceeds 32 MiB".into());
    }
    let observation: Observation = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if observation.format != "binviz-analysis-observation" || observation.schema_version != 1 {
        return Err("expected binviz-analysis-observation schemaVersion 1".into());
    }
    if observation.provider.name.trim().is_empty()
        || observation.provider.version.trim().is_empty()
        || !digest(&observation.provider.profile_sha256)
        || !digest(&observation.target_sha256)
        || !digest(&observation.native_sha256)
    {
        return Err("observation requires provider name/version and exact lowercase SHA-256 identities".into());
    }
    if observation.address_space != "default" || observation.architecture != binary.summary().arch {
        return Err("observation address space or architecture differs from the loaded target".into());
    }
    if observation.target_sha256 != sha256(binary.data()) {
        return Err("observation target digest mismatch".into());
    }
    let entry = hex(&observation.entry)?;
    let start = hex(&observation.start)?;
    let size = hex(&observation.bytes)?;
    let end = start.checked_add(size).ok_or("observation extent overflow")?;
    if size == 0 || size > 16 * 1024 * 1024 || entry < start || entry >= end {
        return Err("observation extent must contain entry and be between 1 byte and 16 MiB".into());
    }
    let offset = binary
        .address_to_offset(start)
        .ok_or("observation has no file-backed start")?;
    let offset_end = offset.checked_add(size).ok_or("observation file extent overflow")?;
    let span = binary
        .data()
        .get(
            usize::try_from(offset).map_err(|e| e.to_string())?
                ..usize::try_from(offset_end).map_err(|e| e.to_string())?,
        )
        .ok_or("observation extends outside target bytes")?;
    // Check every mapping, including internal holes and section/segment boundaries.
    // The explicit extent ceiling bounds the work for an imported record.
    if (0..size).any(|delta| binary.address_to_offset(start + delta) != Some(offset + delta)) {
        return Err("observation extent is not contiguously file-backed at these addresses".into());
    }
    if observation.native_sha256 != sha256(span) {
        return Err("observation native extent digest mismatch".into());
    }
    if observation.pseudocode.as_ref().is_none_or(|s| s.trim().is_empty())
        && observation.dataflow.as_ref().is_none_or(Value::is_null)
    {
        return Err("observation needs pseudocode or dataflow".into());
    }
    Ok(Report {
        format: "binviz-analysis-observation-report".into(), schema_version: 1,
        observation_sha256: sha256(bytes), identity_state: "verified".into(),
        authority: "provider-observation".into(),
        limitations: vec![
            "Verified identity binds supplied target/extent bytes; provider execution and profile authenticity are not established.".into(),
            "Pseudocode and dataflow are provider-derived observations, not original source or runtime execution.".into(),
            "Import grants no native proof, function-boundary changes, policy eligibility or matching credit.".into(),
        ], observation,
    })
}
