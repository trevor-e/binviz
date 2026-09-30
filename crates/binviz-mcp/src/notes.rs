//! Notes files: the same JSON the web UI exports and imports.
//!
//! ```json
//! { "format": "binviz-annotations", "version": 1, "file": "App", "fingerprint": "…",
//!   "annotations": [ { "address": "0x100004000", "size": "0x40", "name": "main",
//!                      "comment": "…", "reviewed": true, "author": "agent" } ] }
//! ```
//!
//! Several writers may share one: agents in other sessions, the web UI
//! following it. Every change goes to the file's append-only journal first
//! (`<notes>.journal`, see [`binviz::notes`]), then the file is rewritten as
//! a fold of it, under a lock ([`Lock`]) when one can be made; a session
//! picks up what others wrote when either file's modification time or size
//! ([`stamp`]) changes. A change is never lost: a rewrite that loses the
//! race is behind a journal line the next read applies.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use binviz::Annotation;

/// Reads a notes file with its journal. Returns the annotations, the
/// fingerprint recorded in the file, and how many journal lines they hold.
pub fn load(path: &Path) -> Result<(Vec<Annotation>, Option<String>, usize), String> {
    binviz::notes::read(path)
}

/// Writes the notes as the file, folding the journal's first `folded` lines.
pub fn save(path: &Path, file: &str, fingerprint: &str, notes: &[Annotation], folded: usize) -> Result<(), String> {
    let text = binviz::notes::document_folding(file, fingerprint, notes, folded);
    // Write then rename, so a crash never leaves a half-written file.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Appends what changed to the notes file's journal: the number of lines written.
pub fn journal(path: &Path, before: &[Annotation], after: &[Annotation]) -> Result<usize, String> {
    let changes = binviz::notes::changes_between(before, after);
    binviz::notes::append_journal(path, &changes)
        .map_err(|e| format!("{}: {e}", binviz::notes::journal_path(path).display()))
}

/// When a notes file (or its journal) last changed, as far as telling goes:
/// the modification time and size of each.
pub fn stamp(path: &Path) -> Option<(SystemTime, u64, u64)> {
    let m = std::fs::metadata(path).ok()?;
    let journal = std::fs::metadata(binviz::notes::journal_path(path)).map_or(0, |j| j.len());
    Some((m.modified().ok()?, m.len(), journal))
}

/// Held while notes are read, changed and written back, so that two writers
/// don't lose each other's notes: `<notes>.lock`, gone when dropped.
pub struct Lock(PathBuf);

impl Lock {
    /// Waits for the notes file's lock (a few seconds at most; one left by a
    /// session that died is broken after ten). `None` when no lock can be
    /// made there (a read-only folder): the notes are then saved without.
    pub fn acquire(notes: &Path) -> Result<Option<Lock>, String> {
        let mut name = notes.as_os_str().to_owned();
        name.push(".lock");
        let path = PathBuf::from(name);
        let started = Instant::now();
        loop {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Some(Lock(path))),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .is_some_and(|age| age > Duration::from_secs(10));
                    if stale {
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    if started.elapsed() > Duration::from_secs(5) {
                        return Err(format!(
                            "{} is being saved by another session; try again",
                            notes.display()
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(_) => return Ok(None),
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
