//! Notes files: the same JSON the web UI exports and imports.
//!
//! ```json
//! { "format": "binviz-annotations", "version": 1, "file": "App", "fingerprint": "…",
//!   "annotations": [ { "address": "0x100004000", "size": "0x40", "name": "main",
//!                      "comment": "…", "reviewed": true, "author": "agent" } ] }
//! ```
//!
//! Several writers may share one: agents in other sessions, the web UI
//! following it. Each change is made on top of what the file holds then,
//! under a lock ([`Lock`]), and a session picks up what others wrote when
//! the file's modification time or size ([`stamp`]) changes.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use binviz::Annotation;

/// Reads a notes file. Returns the annotations and the fingerprint recorded in it.
pub fn load(path: &Path) -> Result<(Vec<Annotation>, Option<String>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    binviz::notes::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn save(path: &Path, file: &str, fingerprint: &str, notes: &[Annotation]) -> Result<(), String> {
    let text = binviz::notes::document(file, fingerprint, notes);
    // Write then rename, so a crash never leaves a half-written file.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// When a notes file last changed, as far as telling goes: its modification
/// time and size.
pub fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
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
