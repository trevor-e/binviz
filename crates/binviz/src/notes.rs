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
//!
//! Beside the file, a journal (`<notes>.journal`, a line of JSON per change)
//! keeps every change any writer makes, appended and never rewritten: a
//! session's `annotate`, an agent's `mark`, one line each, with who made it
//! and when. The JSON file is a fold of the journal (its `journal` field
//! says how many lines it holds), and reading the notes applies the lines
//! after that, so several agents writing at once lose nothing: a change is
//! in the journal before the file is rewritten, and whichever rewrite lands
//! last folded every line that was there. Delete the journal only when no
//! session is writing (everything in it is in the file by then).
//!
//! ```text
//! {"t":1790000000123,"by":"agent-2","set":{"address":"0x80010000","name":"entity_step"}}
//! {"t":1790000000456,"by":"","remove":{"address":"0x80010040","size":"0x0"}}
//! ```

use std::path::{Path, PathBuf};

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
        compiler: text("compiler"),
        flags: text("flags"),
        sdk: text("sdk"),
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
    for (key, value) in [("compiler", &d.compiler), ("flags", &d.flags), ("sdk", &d.sdk)] {
        if !value.is_empty() {
            o[key] = json!(value);
        }
    }
    o
}

fn annotation(a: &Value) -> Option<Annotation> {
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
}

/// The notes in a notes file's text, and the fingerprint of the build they
/// were saved for, if it says.
pub fn parse(text: &str) -> Result<(Vec<Annotation>, Option<String>), String> {
    parse_document(text).map(|(notes, fingerprint, _)| (notes, fingerprint))
}

/// [`parse`], and how many lines of the journal the file has folded in.
pub fn parse_document(text: &str) -> Result<(Vec<Annotation>, Option<String>, usize), String> {
    let json: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let list = json
        .get("annotations")
        .or(Some(&json))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let notes = list.iter().filter_map(annotation).collect();
    let fingerprint = json.get("fingerprint").and_then(Value::as_str).map(str::to_string);
    let folded = json.get("journal").and_then(Value::as_u64).unwrap_or(0) as usize;
    Ok((notes, fingerprint, folded))
}

/// Where a notes file's journal is: `<notes>.journal`.
pub fn journal_path(notes: &Path) -> PathBuf {
    let mut name = notes.as_os_str().to_owned();
    name.push(".journal");
    PathBuf::from(name)
}

/// One change to the notes, as the journal keeps it.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Change {
    /// A note added or changed (the whole note, as it is now).
    Set(Annotation),
    /// The note at (address, size) removed.
    Remove(u64, u64),
}

/// The journal lines for `changes`, made by `by` now.
pub fn journal_lines(changes: &[Change]) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let mut out = String::new();
    for c in changes {
        let line = match c {
            Change::Set(a) => json!({ "t": now, "by": a.author, "set": annotation_json(a) }),
            Change::Remove(address, size) => {
                json!({ "t": now, "by": "", "remove": { "address": format!("{address:#x}"), "size": format!("{size:#x}") } })
            }
        };
        out.push_str(&line.to_string());
        out.push('\n');
    }
    out
}

/// The changes between two lists of notes, by (address, size): what turns
/// `before` into `after`.
pub fn changes_between(before: &[Annotation], after: &[Annotation]) -> Vec<Change> {
    let key = |a: &Annotation| (a.address, a.size);
    let mut out = Vec::new();
    for a in after {
        if before.iter().find(|b| key(b) == key(a)) != Some(a) {
            out.push(Change::Set(a.clone()));
        }
    }
    for b in before {
        if !after.iter().any(|a| key(a) == key(b)) {
            out.push(Change::Remove(b.address, b.size));
        }
    }
    out
}

/// Applies the journal's lines from `from` on to `notes` (a line that
/// doesn't read is skipped), and returns how many lines the journal has.
pub fn apply_journal(notes: &mut Vec<Annotation>, journal: &str, from: usize) -> usize {
    let mut lines = 0;
    for line in journal.lines() {
        if line.trim().is_empty() {
            continue;
        }
        lines += 1;
        if lines <= from {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(a) = v.get("set").and_then(annotation) {
            match notes.iter_mut().find(|n| n.address == a.address && n.size == a.size) {
                Some(n) => *n = a,
                None => notes.push(a),
            }
        } else if let Some(r) = v.get("remove")
            && let Some(address) = r.get("address").and_then(number)
        {
            let size = r.get("size").and_then(number).unwrap_or(0);
            notes.retain(|n| !(n.address == address && n.size == size));
        }
    }
    lines
}

/// The notes as a notes file and its journal together hold: the file's
/// notes with the journal lines it hasn't folded applied. A missing file
/// with a journal reads as the journal's changes alone. Returns the notes,
/// the fingerprint, and the journal's line count (what a rewrite of the
/// file would have folded in).
pub fn read(path: &Path) -> Result<(Vec<Annotation>, Option<String>, usize), String> {
    let (mut notes, fingerprint, folded) = match std::fs::read_to_string(path) {
        Ok(text) => parse_document(&text).map_err(|e| format!("{}: {e}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), None, 0),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let lines = match std::fs::read_to_string(journal_path(path)) {
        Ok(journal) => apply_journal(&mut notes, &journal, folded),
        Err(_) => 0,
    };
    Ok((notes, fingerprint, lines.max(folded)))
}

/// Writes `after` as the notes file at `path`, for `file` (the build
/// `fingerprint` names): what changed since `before` (as [`read`] gave it,
/// with `folded` journal lines) goes to the journal first, then the file is
/// rewritten (written whole, then renamed into place) folding those lines
/// too. Returns the journal lines appended.
pub fn write(
    path: &Path,
    file: &str,
    fingerprint: &str,
    before: &[Annotation],
    after: &[Annotation],
    folded: usize,
) -> Result<usize, String> {
    let changes = changes_between(before, after);
    let lines = append_journal(path, &changes).map_err(|e| format!("{}: {e}", journal_path(path).display()))?;
    let text = document_folding(file, fingerprint, after, folded + lines);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(lines)
}

/// Appends `changes` to the notes file's journal, one line each: one write,
/// which the file system keeps whole against other writers appending.
pub fn append_journal(path: &Path, changes: &[Change]) -> std::io::Result<usize> {
    use std::io::Write as _;
    if changes.is_empty() {
        return Ok(0);
    }
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(journal_path(path))?;
    f.write_all(journal_lines(changes).as_bytes())?;
    Ok(changes.len())
}

fn annotation_json(a: &Annotation) -> Value {
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
}

/// A notes file's text for `notes` on `file` (the build `fingerprint` names).
pub fn document(file: &str, fingerprint: &str, notes: &[Annotation]) -> String {
    document_folding(file, fingerprint, notes, 0)
}

/// [`document`], recording that the first `journal` lines of the journal
/// are folded into it.
pub fn document_folding(file: &str, fingerprint: &str, notes: &[Annotation], journal: usize) -> String {
    let list: Vec<Value> = notes.iter().map(annotation_json).collect();
    let mut doc = json!({
        "format": "binviz-annotations",
        "version": 1,
        "file": file,
        "fingerprint": fingerprint,
        "annotations": list,
    });
    if journal > 0 {
        doc["journal"] = json!(journal);
    }
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
                compiler: "gcc 2.8.1 + maspsx".into(),
                flags: "-O2 -G0".into(),
                sdk: "Psy-Q 4.6".into(),
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

    #[test]
    fn the_journal_folds_into_the_file() {
        let dir = std::env::temp_dir().join(format!("binviz-journal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("game.binviz-notes.json");
        let a = Annotation {
            address: 0x8001_0000,
            name: "step".into(),
            author: "agent-1".into(),
            ..Default::default()
        };
        let b = Annotation {
            address: 0x8001_0040,
            name: "draw".into(),
            author: "agent-2".into(),
            ..Default::default()
        };
        // The file holds a; agent 2 appends b to the journal (its rewrite lost, say).
        std::fs::write(&path, document("game", "abc", std::slice::from_ref(&a))).unwrap();
        assert_eq!(append_journal(&path, &[Change::Set(b.clone())]).unwrap(), 1);
        let (notes, fp, lines) = read(&path).unwrap();
        assert_eq!(
            (notes.as_slice(), fp.as_deref(), lines),
            ([a.clone(), b.clone()].as_slice(), Some("abc"), 1)
        );
        // Written back folding that line; a later removal in the journal applies on top.
        std::fs::write(&path, document_folding("game", "abc", &notes, lines)).unwrap();
        append_journal(
            &path,
            &[
                Change::Remove(0x8001_0000, 0),
                Change::Set(Annotation {
                    comment: "the loop".into(),
                    ..b.clone()
                }),
            ],
        )
        .unwrap();
        let (notes, _, lines) = read(&path).unwrap();
        assert_eq!(lines, 3);
        assert_eq!(notes.len(), 1);
        assert_eq!(
            (notes[0].name.as_str(), notes[0].comment.as_str()),
            ("draw", "the loop")
        );
        // A line that doesn't read is skipped; the changes between two lists.
        let mut notes = vec![a.clone()];
        assert_eq!(
            apply_journal(&mut notes, "not json\n{\"set\":{\"address\":\"0x10\"}}\n", 0),
            2
        );
        assert_eq!(notes.len(), 2);
        let changes = changes_between(
            &[a.clone(), b.clone()],
            &[Annotation {
                reviewed: true,
                ..a.clone()
            }],
        );
        assert_eq!(
            changes,
            [
                Change::Set(Annotation { reviewed: true, ..a }),
                Change::Remove(b.address, 0)
            ]
        );
        // No file yet: the journal alone.
        let _ = std::fs::remove_file(&path);
        let (notes, _, _) = read(&path).unwrap();
        assert_eq!(notes.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
