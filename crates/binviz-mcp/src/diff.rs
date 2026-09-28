//! What changed in size between two builds on disk: two binaries, or two
//! folders or zips.

use std::path::Path;

use binviz::package::{DiskPackage, is_package_path, load_binary};
use serde_json::Value;

use crate::tools::{Server, int, string};

/// Binaries of a folder read for its snapshot, at most.
const SNAPSHOT_BINARIES: usize = 300;

impl Server {
    pub(crate) fn diff_functions(&mut self, args: &Value) -> Result<String, String> {
        let old = string(args, "old").ok_or("old is required: the earlier build's path")?;
        let new = string(args, "new").ok_or("new is required: the later build's path")?;
        let top = int(args, "top", 40, 1000) as usize;
        let load = |p: &str| -> Result<binviz::Binary, String> {
            let data = binviz::read_file(Path::new(p)).map_err(|e| format!("{p}: {e}"))?;
            let (mut bin, _) = load_binary(data).map_err(|e| format!("{p}: {e}"))?;
            bin.demangle_swift_with_tool();
            Ok(bin)
        };
        let (a, b) = (load(old)?, load(new)?);
        let d = a.diff_functions(&b);
        let Some(want) = string(args, "function") else {
            return Ok(d.to_text(top));
        };
        let address = crate::tools::address_of(&a, want)?;
        let pair = d
            .pairs
            .iter()
            .find(|p| address >= p.old.address && address < p.old.address + p.old.size.max(1))
            .ok_or_else(|| format!("{want} has no match in the later build (removed, or not a function)"))?;
        Ok(format!(
            "{} {:#x} → {} {:#x}: {:?}, {:.0}% similar, matched by {:?}. Old on the left; = the same, ~ changed, - removed, + added.\n\n{}",
            pair.old.name,
            pair.old.address,
            pair.new.name,
            pair.new.address,
            pair.status,
            pair.similarity * 100.0,
            pair.how,
            binviz::fndiff::code_text(&a.diff_function_code(pair.old.address, &b, pair.new.address))
        ))
    }

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
