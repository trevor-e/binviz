//! Notes files: the same JSON the web UI exports and imports.
//!
//! ```json
//! { "format": "binviz-annotations", "version": 1, "file": "App", "fingerprint": "…",
//!   "annotations": [ { "address": "0x100004000", "size": "0x40", "name": "main",
//!                      "comment": "…", "reviewed": true,
//!                      "decomp": { "state": "matched", "percent": 100, "attempts": 1,
//!                                  "since": 1790000000, "source": "src/main.c" } } ] }
//! ```

use std::path::Path;

use binviz::{Annotation, Decomp, DecompState};
use serde_json::{Value, json};

fn number(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => {
            let t = s.trim();
            match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                Some(h) => u64::from_str_radix(h, 16).ok(),
                None => u64::from_str_radix(t, 16).ok(),
            }
        }
        _ => None,
    }
}

/// Reads a notes file. Returns the annotations and the fingerprint recorded in it.
pub fn load(path: &Path) -> Result<(Vec<Annotation>, Option<String>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let list = json
        .get("annotations")
        .or(Some(&json))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let notes = list
        .iter()
        .filter_map(|a| {
            Some(Annotation {
                address: number(a.get("address")?)?,
                size: a.get("size").and_then(number).unwrap_or(0),
                name: a.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                comment: a.get("comment").and_then(Value::as_str).unwrap_or("").to_string(),
                reviewed: a.get("reviewed").and_then(Value::as_bool).unwrap_or(false),
                kind: a.get("kind").and_then(Value::as_str).map(str::to_string), decomp: a.get("decomp").and_then(decomp),
                ctype: a.get("type").and_then(Value::as_str).map(str::to_string),
            })
        })
        .collect();
    let fingerprint = json.get("fingerprint").and_then(Value::as_str).map(str::to_string);
    Ok((notes, fingerprint))
}

fn decomp(v: &Value) -> Option<Decomp> {
    let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Some(Decomp {
        state: DecompState::parse(v.get("state")?.as_str()?)?,
        percent: v.get("percent").and_then(Value::as_f64).map(|p| p as f32),
        attempts: v.get("attempts").and_then(Value::as_u64).unwrap_or(0) as u32,
        by: text("by"),
        since: v.get("since").and_then(Value::as_u64).unwrap_or(0),
        source: text("source"),
    })
}

fn decomp_json(d: &Decomp) -> Value {
    let mut o = json!({ "state": d.state.as_str() });
    if let Some(p) = d.percent {
        o["percent"] = json!((p as f64 * 100.0).round() / 100.0);
    }
    if d.attempts > 0 {
        o["attempts"] = json!(d.attempts);
    }
    if !d.by.is_empty() {
        o["by"] = json!(d.by);
    }
    if d.since > 0 {
        o["since"] = json!(d.since);
    }
    if !d.source.is_empty() {
        o["source"] = json!(d.source);
    }
    o
}

pub fn save(path: &Path, file: &str, fingerprint: &str, notes: &[Annotation]) -> Result<(), String> {
    let list: Vec<Value> = notes
        .iter()
        .map(|a| {
            let mut o = json!({ "address": format!("{:#x}", a.address) });
            if a.size > 0 {
                o["size"] = json!(format!("{:#x}", a.size));
            }
            if !a.name.is_empty() {
                o["name"] = json!(a.name);
            }
            if !a.comment.is_empty() {
                o["comment"] = json!(a.comment);
            }
            if a.reviewed {
                o["reviewed"] = json!(true);
            }
            if let Some(d) = &a.decomp {
                o["decomp"] = decomp_json(d);
            }
            o
        })
        .collect();
    let doc = json!({
        "format": "binviz-annotations",
        "version": 1,
        "file": file,
        "fingerprint": fingerprint,
        "annotations": list,
    });
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    // Write then rename, so a crash never leaves a half-written file.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}
