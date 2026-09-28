//! Folders and zips of binaries (an `.ipa`, an `.xcarchive`, a build folder,
//! zips inside them): every binary opened at once, each paired with its
//! debug file by UUID or build ID and attached the first time the binary is
//! used, and summaries across all of them.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use binviz::package::{BinaryKind, DiskPackage, PackageInfo, combine_owners, load_binary, update_loaded};
use serde_json::Value;

use crate::tools::{Open, Server, clip, count, human, int, pct, search, sidecar, string};

/// At most this many binaries of a folder are opened (executables and
/// libraries come first; object files last).
const MAX_OPEN: usize = 300;

pub(crate) struct OpenPackage {
    pub id: String,
    pub path: PathBuf,
    pub disk: DiskPackage,
    pub info: PackageInfo,
    /// Open ids of the binaries, by binary index (`None`: not opened, or unreadable).
    pub ids: Vec<Option<String>>,
    pub errors: Vec<String>,
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

impl Server {
    pub(crate) fn open_package(&mut self, path: &Path, args: &Value) -> Result<String, String> {
        let started = Instant::now();
        let disk = DiskPackage::open(path)?;
        let mut info = disk.info.clone();
        if info.binaries.is_empty() {
            return Err(format!(
                "{} holds no binaries ({} files); to open one file inside it, pass that file's path",
                path.display(),
                count(info.files)
            ));
        }
        // Opening the same folder again replaces it.
        if let Some(p) = self.packages.iter().position(|p| p.path == path) {
            self.close_package(p);
        }
        let package = self.packages.len();
        let mut package_id = stem(&info.name).replace(' ', "");
        let base = package_id.clone();
        let mut k = 2;
        while self.packages.iter().any(|p| p.id == package_id) {
            package_id = format!("{base}#{k}");
            k += 1;
        }
        let mut pkg = OpenPackage {
            id: package_id,
            path: path.to_path_buf(),
            disk,
            info: PackageInfo::default(),
            ids: vec![None; info.binaries.len()],
            errors: Vec::new(),
        };
        let mut notes_loaded = 0;
        for i in 0..info.binaries.len().min(MAX_OPEN) {
            let b = info.binaries[i].clone();
            let loaded = pkg
                .disk
                .read_shared(b.file)
                .and_then(|data| load_binary(data.clone()).map(|l| (l, data)).map_err(|e| e.to_string()));
            let ((bin, arch), data) = match loaded {
                Ok(x) => x,
                Err(e) => {
                    pkg.errors.push(format!("{}: {e}", b.path));
                    continue;
                }
            };
            // All slices' UUIDs, and a debug file the binary's debug link names.
            update_loaded(&mut info, i as u32, &data, &bin);
            let id = self.unique_id(&b.name.replace(' ', ""));
            let mut open = Open {
                id: id.clone(),
                path: path.join(&b.path),
                label: format!(
                    "{} › {}{}",
                    info.name,
                    b.path,
                    arch.map(|a| format!(" [{a}]")).unwrap_or_default()
                ),
                bin,
                notes: None,
                package: Some(package),
                pending_debug: None,
                debug_note: None,
            };
            let notes_path = string(args, "notes_file")
                .filter(|_| i == 0)
                .map(PathBuf::from)
                .unwrap_or_else(|| sidecar(path, Some(&id)));
            if crate::tools::load_notes(&mut open, &notes_path).is_some() {
                notes_loaded += 1;
            }
            open.notes = Some(notes_path);
            pkg.ids[i] = Some(id);
            self.open.push(open);
        }
        for b in &info.binaries {
            let (Some(d), Some(id)) = (b.debug, &pkg.ids[b.index as usize]) else {
                continue;
            };
            let debug = &info.debug_files[d as usize];
            if let Some(o) = self.open.iter_mut().find(|o| &o.id == id) {
                o.pending_debug = Some((debug.file, debug.path.clone()));
            }
        }
        pkg.info = info;
        let main = pkg.ids.iter().flatten().next().cloned();
        self.current = main.as_ref().and_then(|m| self.open.iter().position(|o| &o.id == m));
        self.packages.push(pkg);
        let mut out = format!(
            "Opened {} in {:.1} s.\n",
            path.display(),
            started.elapsed().as_secs_f64()
        );
        if notes_loaded > 0 {
            let _ = writeln!(out, "Loaded notes for {notes_loaded} binaries.");
        }
        out.push_str(&overview(&self.packages[package], 60));
        let _ = writeln!(
            out,
            "\nThe first binary{} is selected; pass `binary` (an id above) to work on another. Debug files attach the first time their binary is used. \
             folder_summary gives the size breakdown, largest and duplicate files, and (analyze: true) the code owners across all binaries; search with binary: \"all\" searches every binary.",
            main.map(|m| format!(" `{m}`")).unwrap_or_default()
        );
        Ok(out)
    }

    fn unique_id(&self, base: &str) -> String {
        let mut id = base.to_string();
        let mut k = 2;
        while self.open.iter().any(|o| o.id == id) {
            id = format!("{base}#{k}");
            k += 1;
        }
        id
    }

    pub(crate) fn close_package(&mut self, p: usize) {
        let pkg = self.packages.remove(p);
        let ids: Vec<&String> = pkg.ids.iter().flatten().collect();
        self.open.retain(|o| !ids.contains(&&o.id));
        for o in &mut self.open {
            o.package = o.package.and_then(|q| match q.cmp(&p) {
                std::cmp::Ordering::Less => Some(q),
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Greater => Some(q - 1),
            });
        }
        self.current = (!self.open.is_empty()).then(|| self.open.len() - 1);
    }

    /// Attaches an open binary's debug file from its folder, if one is waiting.
    pub(crate) fn attach_pending(&mut self, i: usize) {
        let Some((file, path)) = self.open[i].pending_debug.take() else {
            return;
        };
        let Some(p) = self.open[i].package else { return };
        let result = self.packages[p].disk.read_shared(file).and_then(|data| {
            self.open[i]
                .bin
                .attach_debug_file(&path, data)
                .map_err(|e| e.to_string())
        });
        self.open[i].debug_note = Some(match result {
            Ok(()) => format!("debug file attached: {path}"),
            Err(e) => format!("debug file {path} could not be attached: {e}"),
        });
    }

    /// The folder a request is about: `package` (an id or name), else the
    /// current binary's, else the last opened.
    fn package_for(&self, args: &Value) -> Result<usize, String> {
        if self.packages.is_empty() {
            return Err("no folder is open: open_binary a folder or a zip (an .ipa, a build…)".into());
        }
        if let Some(want) = string(args, "package") {
            return self
                .packages
                .iter()
                .position(|p| p.id == want || p.info.name == want || p.path.to_string_lossy() == want)
                .ok_or_else(|| {
                    let ids: Vec<&str> = self.packages.iter().map(|p| p.id.as_str()).collect();
                    format!("no open folder {want:?}; open: {}", ids.join(", "))
                });
        }
        Ok(self
            .current
            .and_then(|c| self.open.get(c))
            .and_then(|o| o.package)
            .unwrap_or(self.packages.len() - 1))
    }

    pub(crate) fn folder_summary(&mut self, args: &Value) -> Result<String, String> {
        let p = self.package_for(args)?;
        let top = int(args, "top", 25, 500) as usize;
        let mut out = overview(&self.packages[p], top);
        let pkg = &self.packages[p];
        let info = &pkg.info;
        let _ = writeln!(out, "\nLargest files:");
        for f in info.largest.iter().take(top) {
            let _ = writeln!(
                out,
                "  {:>10}{}  {}  [{}]",
                human(f.size),
                f.compressed_size
                    .map(|c| format!(" ({} zipped)", human(c)))
                    .unwrap_or_default(),
                clip(&f.path, 140),
                f.category.label()
            );
        }
        if info.duplicates.is_empty() {
            if info.compressed_size.is_none() {
                let _ = writeln!(
                    out,
                    "\n(Duplicate files are found in zips, whose directory has checksums.)"
                );
            }
        } else {
            let _ = writeln!(
                out,
                "\nDuplicate files: {} could be saved ({} groups; largest first):",
                human(info.duplicate_bytes),
                info.duplicates.len()
            );
            for d in info.duplicates.iter().take(top) {
                let _ = writeln!(
                    out,
                    "  {} × {} ({} wasted):",
                    d.paths.len(),
                    human(d.size),
                    human(d.wasted)
                );
                for path in d.paths.iter().take(6) {
                    let _ = writeln!(out, "      {}", clip(path, 140));
                }
            }
        }
        let unpaired: Vec<_> = info.debug_files.iter().filter(|d| d.binary.is_none()).collect();
        if !unpaired.is_empty() {
            let _ = writeln!(
                out,
                "\nDebug files that pair with no binary here (by UUID, build ID or debug link):"
            );
            for d in unpaired.iter().take(20) {
                let ids: Vec<String> = d.ids.iter().map(|x| format!("{} {}", x.arch, x.id)).collect();
                let _ = writeln!(out, "  {} ({})", d.path, ids.join(", "));
            }
        }
        if !pkg.errors.is_empty() {
            let _ = writeln!(out, "\nCould not read:");
            for e in &pkg.errors {
                let _ = writeln!(out, "  {e}");
            }
        }
        if !args.get("analyze").and_then(Value::as_bool).unwrap_or(false) {
            let _ = writeln!(
                out,
                "\nPass analyze: true to break the code and data of every binary down by owner (Swift modules, Objective-C classes, C++ namespaces…), with debug files attached."
            );
            return Ok(out);
        }
        // Owners across binaries: attach debug files first so stripped binaries have names.
        let ids: Vec<String> = pkg.ids.iter().flatten().cloned().collect();
        let started = Instant::now();
        let mut reports = Vec::new();
        for id in &ids {
            let Some(i) = self.open.iter().position(|o| &o.id == id) else {
                continue;
            };
            self.attach_pending(i);
            reports.push((id.clone(), self.open[i].bin.size_report(500)));
        }
        let _ = writeln!(
            out,
            "\nCode and data by owner, across {} binaries ({:.1} s):",
            reports.len(),
            started.elapsed().as_secs_f64()
        );
        let _ = writeln!(
            out,
            "Per binary (named bytes: covered by symbols from the file, its debug file, or DWARF):"
        );
        for (id, r) in &reports {
            let o = self.open.iter().find(|o| &o.id == id).expect("opened");
            let _ = writeln!(
                out,
                "  `{id}` {} — {} named ({}){}",
                human(r.file_size),
                human(r.symbolized_bytes),
                pct(r.symbolized_bytes, r.file_size),
                o.debug_note.as_deref().map(|n| format!("; {n}")).unwrap_or_default()
            );
        }
        let refs: Vec<(String, &binviz::SizeReport)> = reports.iter().map(|(id, r)| (id.clone(), r)).collect();
        let owners = combine_owners(&refs, top);
        let _ = writeln!(out, "\nLargest owners:");
        for o in &owners {
            let spread: Vec<String> = o
                .binaries
                .iter()
                .take(4)
                .map(|(b, bytes)| format!("{b} {}", human(*bytes)))
                .collect();
            let _ = writeln!(
                out,
                "  {:>10}  {:<48} {:<22} {}",
                human(o.bytes),
                clip(&o.name, 48),
                o.kind.label(),
                spread.join(", ")
            );
        }
        Ok(out)
    }

    /// `search` over every binary of the current folder (or every open binary).
    pub(crate) fn search_all(&mut self, args: &Value) -> Result<String, String> {
        let targets: Vec<usize> = match self.package_for(args) {
            Ok(p) => {
                let ids: Vec<String> = self.packages[p].ids.iter().flatten().cloned().collect();
                ids.iter()
                    .filter_map(|id| self.open.iter().position(|o| &o.id == id))
                    .collect()
            }
            Err(_) => (0..self.open.len()).collect(),
        };
        if targets.is_empty() {
            return Err("no binary is open: call open_binary first".into());
        }
        let mut out = String::new();
        let mut empty = Vec::new();
        for i in targets {
            self.attach_pending(i);
            let o = &self.open[i];
            let text = search(o, args)?;
            if text.starts_with("Nothing matches") {
                empty.push(o.id.clone());
                continue;
            }
            let _ = writeln!(out, "## `{}`\n{}", o.id, text.trim_end());
        }
        if !empty.is_empty() {
            let _ = writeln!(out, "\nNo matches in: {}", empty.join(", "));
        }
        Ok(out)
    }
}

/// The folder at a glance: the app it holds, its binaries with their debug files, and sizes by kind.
fn overview(pkg: &OpenPackage, top: usize) -> String {
    let info = &pkg.info;
    let mut out = format!(
        "{} ({}): {} files, {}{}{}.\n",
        info.name,
        info.kind,
        count(info.files),
        human(info.size),
        info.compressed_size
            .map(|c| format!(", {} compressed", human(c)))
            .unwrap_or_default(),
        if info.debug_size > 0 {
            format!(", plus {} of debug files", human(info.debug_size))
        } else {
            String::new()
        }
    );
    if let Some(a) = info.binaries.first().and_then(|b| b.bundle.as_ref()) {
        let _ = writeln!(
            out,
            "Bundle {}: {}{}{}{}",
            a.path,
            a.name.as_deref().unwrap_or("?"),
            a.bundle_id.as_deref().map(|b| format!(" — {b}")).unwrap_or_default(),
            match (&a.version, &a.build) {
                (Some(v), Some(b)) => format!(" {v} ({b})"),
                (Some(v), None) => format!(" {v}"),
                _ => String::new(),
            },
            a.min_os
                .as_deref()
                .map(|m| format!(", requires OS {m}+"))
                .unwrap_or_default()
        );
    }
    let _ = writeln!(
        out,
        "\nBinaries ({}; ids for the `binary` argument):",
        info.binaries.len()
    );
    let width = pkg.ids.iter().flatten().map(|i| i.len() + 2).max().unwrap_or(8);
    for b in info.binaries.iter().take(top.max(60)) {
        let id = format!(
            "`{}`",
            pkg.ids[b.index as usize]
                .as_deref()
                .unwrap_or(if (b.index as usize) < MAX_OPEN {
                    "(unreadable)"
                } else {
                    "(not opened)"
                })
        );
        let archs: Vec<&str> = b.ids.iter().map(|x| x.arch.as_str()).collect();
        let debug = match b.debug {
            Some(d) => format!("debug file {}", info.debug_files[d as usize].path),
            None if b.kind == BinaryKind::Debug => "a debug file itself".into(),
            None if info.debug_files.is_empty() => "no debug files here".into(),
            None => "no matching debug file".into(),
        };
        let _ = writeln!(
            out,
            "  {id:<width$}  {:<11} {:>10}  {} {:<12} {}  — {}",
            b.kind.label(),
            human(b.size),
            b.format,
            archs.join("+"),
            clip(&b.path, 90),
            debug
        );
    }
    if info.binaries.len() > top.max(60) {
        let _ = writeln!(out, "  … and {} more", info.binaries.len() - top.max(60));
    }
    let _ = writeln!(out, "\nBy kind of content:");
    for c in info.categories.iter().take(top) {
        let _ = writeln!(
            out,
            "  {:<20} {:>10} {:>6}  {:>6} files{}",
            c.category.label(),
            human(c.size),
            if c.category == binviz::package::FileCategory::DebugSymbols {
                String::new()
            } else {
                pct(c.size, info.size)
            },
            count(c.files),
            c.compressed_size
                .map(|z| format!("  ({} zipped)", human(z)))
                .unwrap_or_default()
        );
    }
    out
}
