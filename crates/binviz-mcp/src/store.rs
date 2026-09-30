//! The store of matched functions kept across projects (`binviz::store`):
//! recording this project's matches into it, and asking it for the
//! functions here that another project already matched.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use binviz::store::{Entry, Store};
use serde_json::Value;

use crate::queue::{function_start, name, now};
use crate::tools::{Open, count, int, string};

/// Where the sources are, when the caller doesn't say: the folder above the
/// notes file's own (`notes/x.json` in a repository whose C is under `src/`),
/// else the notes file's folder.
fn default_root(o: &Open) -> Option<PathBuf> {
    let dir = o.notes.as_ref()?.parent()?;
    if dir.file_name().is_some_and(|n| n == "notes") {
        dir.parent().map(Path::to_path_buf)
    } else {
        Some(dir.to_path_buf())
    }
}

fn open_store(args: &Value) -> Result<Store, String> {
    match string(args, "store") {
        Some(dir) => Ok(Store::at(dir)),
        None => Store::open_default().ok_or_else(|| "no place for the store: set BINVIZ_STORE".to_string()),
    }
}

/// `store_record`: keeps this project's matched functions, with their C and build.
pub(crate) fn record(o: &Open, args: &Value) -> Result<String, String> {
    let root = match string(args, "source_root") {
        Some(r) => PathBuf::from(r),
        None => default_root(o).ok_or("source_root is required: the folder the notes' source files are relative to")?,
    };
    let project = string(args, "project").unwrap_or(&o.label);
    let store = open_store(args)?;
    let rec = o.bin.store_entries(&root, project, now());
    let dry = args.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
    let mut kept = 0;
    let mut failed = Vec::new();
    for e in &rec.entries {
        if dry {
            kept += 1;
        } else {
            match store.add(e) {
                Ok(()) => kept += 1,
                Err(err) => failed.push(format!("{}: {err}", e.name)),
            }
        }
    }
    let mut out = format!(
        "{} {} matched function{} of {project} {} {} (sources under {}).",
        if dry { "Would keep" } else { "Kept" },
        count(kept as u64),
        if kept == 1 { "" } else { "s" },
        if dry { "in" } else { "in the store at" },
        store.dir().display(),
        root.display()
    );
    if !rec.skipped.is_empty() {
        let _ = write!(out, "\n{} not kept:", rec.skipped.len());
        for (address, name, why) in rec.skipped.iter().take(40) {
            let _ = write!(out, "\n  {name} ({address:#x}): {why}");
        }
        if rec.skipped.len() > 40 {
            let _ = write!(out, "\n  … {} more", rec.skipped.len() - 40);
        }
    }
    for f in failed {
        let _ = write!(out, "\nNOT written: {f}");
    }
    Ok(out)
}

fn describe(e: &Entry, with_c: bool) -> String {
    let mut out = format!(
        "{} in {}, {} instructions, built with {}",
        e.name,
        e.project,
        e.instructions,
        if e.compiler.is_empty() {
            "an unrecorded build".to_string()
        } else {
            e.build()
        }
    );
    if !e.source.is_empty() {
        let _ = write!(out, " ({})", e.source);
    }
    if with_c {
        let _ = write!(out, "\n```c\n{}\n```", e.c);
    }
    out
}

/// What the store has for the function at `start`, worded for a context:
/// empty when it has nothing (or there is no store).
pub(crate) fn context_section(o: &Open, start: u64) -> String {
    let Some(store) = Store::open_default() else {
        return String::new();
    };
    let Some(k) = o.bin.function_key(start) else {
        return String::new();
    };
    let found = store.find(&k.key);
    if found.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\nMatched in another project, the same code apart from where it was linked (a candidate: compile it here and match_function it):\n",
    );
    for e in found.iter().take(3) {
        let _ = writeln!(out, "  {}", describe(e, true).replace('\n', "\n  "));
    }
    out
}

/// `store_lookup`: the functions here that another project matched.
pub(crate) fn lookup(o: &Open, args: &Value) -> Result<String, String> {
    let store = open_store(args)?;
    let want = string(args, "compiler").map(str::to_lowercase);
    let build_ok = |e: &Entry| want.as_ref().is_none_or(|w| e.build().to_lowercase().contains(w));
    if let Some(at) = string(args, "at") {
        let start = function_start(&o.bin, at)?;
        let Some(k) = o.bin.function_key(start) else {
            return Ok(format!(
                "{} has too little code to look up (under {} instructions).",
                name(&o.bin, start),
                binviz::store::MIN_INSTRUCTIONS
            ));
        };
        let found: Vec<Entry> = store.find(&k.key).into_iter().filter(|e| build_ok(e)).collect();
        if found.is_empty() {
            return Ok(format!(
                "Nothing in the store at {} matches {} ({start:#x}).",
                store.dir().display(),
                name(&o.bin, start)
            ));
        }
        let mut out = format!(
            "{} ({start:#x}) matches {} in the store, the same code apart from where it was linked. A candidate: compile it here and match_function it.\n",
            name(&o.bin, start),
            if found.len() == 1 {
                "1 function".to_string()
            } else {
                format!("{} functions", found.len())
            }
        );
        for e in found.iter().take(5) {
            let _ = writeln!(out, "\n{}", describe(e, true));
        }
        return Ok(out);
    }
    let index = store.index();
    let total: usize = index.values().map(Vec::len).sum();
    if total == 0 {
        return Ok(format!(
            "The store at {} is empty: store_record adds a project's matched functions.",
            store.dir().display()
        ));
    }
    let limit = int(args, "limit", 50, 1000) as usize;
    let mut hits = o.bin.store_hits(&index);
    for h in &mut hits {
        h.entries.retain(|e| build_ok(e));
    }
    hits.retain(|h| !h.entries.is_empty());
    let mut out = format!(
        "{} of this binary's unmatched functions have a match in the store ({} entries at {}):\n",
        count(hits.len() as u64),
        count(total as u64),
        store.dir().display()
    );
    for h in hits.iter().take(limit) {
        let e = &h.entries[0];
        let _ = writeln!(
            out,
            "  {} ({:#x}, {} instructions) = {} in {}, {}{}",
            h.name,
            h.address,
            h.instructions,
            e.name,
            e.project,
            e.build(),
            if h.entries.len() > 1 {
                format!(" (+{} more)", h.entries.len() - 1)
            } else {
                String::new()
            }
        );
    }
    if hits.len() > limit {
        let _ = writeln!(out, "  … {} more", hits.len() - limit);
    }
    if !hits.is_empty() {
        out.push_str("\nstore_lookup with at: shows one's C. A hit is a candidate: match_function decides.");
    }
    Ok(out)
}
