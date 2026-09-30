//! Notes files: the JSON the web UI exports and imports, the MCP server keeps
//! next to a binary (`<binary>.binviz-notes.json`), and the CLI reads with
//! `--notes`.
//!
//! ```json
//! { "format": "binviz-annotations", "version": 1, "file": "App", "fingerprint": "…",
//!   "annotations": [ { "address": "0x100004000", "size": "0x40", "name": "main",
//!                      "comment": "…", "reviewed": true, "author": "agent",
//!                      "decomp": { "state": "matched", "percent": 100, "attempts": 1,
//!                                  "since": 1790000000, "source": "src/main.c" } } ] }
//! ```
//!
//! A bare list of annotations reads too, addresses as numbers or in hex.

use serde_json::{Value, json};

use crate::model::{Annotation, Decomp, DecompState};

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

/// The notes in a notes file's text, and the fingerprint of the build they
/// were saved for, if it says.
pub fn parse(text: &str) -> Result<(Vec<Annotation>, Option<String>), String> {
    let json: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
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
                kind: a.get("kind").and_then(Value::as_str).map(str::to_string),
                decomp: a.get("decomp").and_then(decomp),
                ctype: a.get("type").and_then(Value::as_str).map(str::to_string),
                author: a.get("author").and_then(Value::as_str).unwrap_or("").to_string(),
            })
        })
        .collect();
    let fingerprint = json.get("fingerprint").and_then(Value::as_str).map(str::to_string);
    Ok((notes, fingerprint))
}

/// A notes file's text for `notes` on `file` (the build `fingerprint` names).
pub fn document(file: &str, fingerprint: &str, notes: &[Annotation]) -> String {
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
            if !a.author.is_empty() {
                o["author"] = json!(a.author);
            }
            if let Some(kind) = &a.kind {
                o["kind"] = json!(kind);
            }
            if let Some(d) = &a.decomp {
                o["decomp"] = decomp_json(d);
            }
            if let Some(t) = &a.ctype {
                o["type"] = json!(t);
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
    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notes_file_reads_back_as_it_was_written() {
        let notes = vec![Annotation {
            address: 0x8001_0000,
            size: 0x40,
            name: "entity_step".into(),
            comment: "walks the list".into(),
            reviewed: true,
            kind: Some("function".into()),
            decomp: Some(Decomp {
                state: DecompState::Matched,
                percent: Some(100.0),
                attempts: 2,
                by: "agent-1".into(),
                since: 1_790_000_000,
                source: "src/entity.c".into(),
            }),
            ctype: Some("int (Entity *e, int dx)".into()),
            author: "agent".into(),
        }];
        let text = document("game.exe", "abc", &notes);
        assert_eq!(parse(&text), Ok((notes, Some("abc".into()))));
        // A bare list, addresses as numbers.
        let (list, fingerprint) = parse(r#"[{ "address": 2147549184, "name": "f" }]"#).unwrap();
        assert_eq!(
            (list[0].address, list[0].name.as_str(), fingerprint),
            (0x8001_0000, "f", None)
        );
        assert!(list[0].author.is_empty());
    }
}
