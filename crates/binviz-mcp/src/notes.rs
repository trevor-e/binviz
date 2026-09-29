//! Notes files on disk: `binviz::notes` reads and writes their JSON.

use std::path::Path;

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
