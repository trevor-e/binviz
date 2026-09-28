//! What changed in size between two builds on disk: two binaries, or two
//! folders or zips.

use std::path::Path;

use binviz::package::{DiskPackage, is_package_path, load_binary};
use serde_json::Value;

use crate::tools::{Server, int, string};

/// Binaries of a folder read for its snapshot, at most.
const SNAPSHOT_BINARIES: usize = 300;

impl Server {
    pub(crate) fn size_diff(&mut self, args: &Value) -> Result<String, String> {
        let old = string(args, "old").ok_or("old is required: the earlier build's path")?;
        let new = string(args, "new").ok_or("new is required: the later build's path")?;
        let top = int(args, "top", 30, 500) as usize;
        let (op, np) = (Path::new(old), Path::new(new));
        match (is_package_path(op), is_package_path(np)) {
            (true, true) => {
                let demangle = |b: &mut binviz::Binary| {
                    b.demangle_swift_with_tool();
                };
                let before = DiskPackage::open(op)?.snapshot(SNAPSHOT_BINARIES, demangle);
                let after = DiskPackage::open(np)?.snapshot(SNAPSHOT_BINARIES, demangle);
                Ok(binviz::diff::diff_folders(&before, &after, top).to_text())
            }
            (false, false) => {
                let snapshot = |p: &Path| -> Result<binviz::diff::SizeSnapshot, String> {
                    let data = binviz::read_file(p).map_err(|e| format!("{}: {e}", p.display()))?;
                    let (mut bin, _) = load_binary(data).map_err(|e| format!("{}: {e}", p.display()))?;
                    bin.demangle_swift_with_tool();
                    let name = p
                        .file_name()
                        .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
                    Ok(bin.size_snapshot(&name))
                };
                Ok(binviz::diff::diff_binaries(&snapshot(op)?, &snapshot(np)?, top).to_text())
            }
            _ => Err("compare like with like: two binaries, or two folders or zips".into()),
        }
    }
}
