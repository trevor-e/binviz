//! What changed in size between two builds: of a binary (kinds of bytes,
//! sections, owners, symbols), or of a folder of them (files, kinds of
//! content, each binary, and owners across all the binaries).
//!
//! A build is first reduced to a snapshot ([`crate::Binary::size_snapshot`],
//! [`crate::package::DiskPackage::snapshot`]), so the two builds needn't be
//! open at once; [`diff_binaries`] and [`diff_folders`] compare snapshots.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::model::RegionKind;
use crate::package::FileCategory;
use crate::size::GroupKind;

/// What a binary's size is made of, kept to compare with another build.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeSnapshot {
    pub name: String,
    pub file_size: u64,
    pub by_kind: Vec<(RegionKind, u64)>,
    pub sections: Vec<SectionBytes>,
    pub owners: Vec<OwnerBytes>,
    /// Named symbols, same-named ones summed (functions recovered without a
    /// name are left out: their names are their addresses).
    pub symbols: Vec<SymbolBytes>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionBytes {
    pub name: String,
    pub file: u64,
    pub memory: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerBytes {
    pub kind: GroupKind,
    pub name: String,
    pub code: u64,
    pub data: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolBytes {
    pub name: String,
    pub bytes: u64,
    pub code: bool,
}

/// A folder's sizes, kept to compare with another build.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSnapshot {
    pub name: String,
    pub files: Vec<FileBytes>,
    /// The binaries read.
    pub binaries: Vec<BinarySnapshot>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileBytes {
    pub path: String,
    pub bytes: u64,
    pub category: FileCategory,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinarySnapshot {
    pub path: String,
    pub snapshot: SizeSnapshot,
}

// --- Differences ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub name: String,
    pub old: u64,
    pub new: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindChange {
    pub kind: RegionKind,
    pub old: u64,
    pub new: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryChange {
    pub category: FileCategory,
    pub old: u64,
    pub new: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerChange {
    pub kind: GroupKind,
    pub name: String,
    pub old: u64,
    pub new: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolChange {
    pub name: String,
    pub code: bool,
    pub old: u64,
    pub new: u64,
}

/// What changed in a binary, the biggest changes first.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeDiff {
    pub old_name: String,
    pub new_name: String,
    pub old_size: u64,
    pub new_size: u64,
    pub by_kind: Vec<KindChange>,
    /// Sections whose size changed.
    pub sections: Vec<Change>,
    /// Owners whose size changed (the biggest `top`).
    pub owners: Vec<OwnerChange>,
    pub owners_changed: u32,
    /// Symbols that came, went or changed size (the biggest `top`).
    pub symbols: Vec<SymbolChange>,
    pub symbols_added: u32,
    pub symbols_removed: u32,
    pub symbols_changed: u32,
}

/// What changed in a folder, the biggest changes first.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDiff {
    pub old_name: String,
    pub new_name: String,
    /// Every file but debug files.
    pub old_size: u64,
    pub new_size: u64,
    pub categories: Vec<CategoryChange>,
    /// Files that came, went or changed size (the biggest `top`).
    pub files: Vec<Change>,
    pub files_added: u32,
    pub files_removed: u32,
    pub files_changed: u32,
    /// The binaries of either build, with what changed inside those both have.
    pub binaries: Vec<BinaryChange>,
    /// Owners across all the binaries whose size changed (the biggest `top`).
    pub owners: Vec<OwnerChange>,
    pub owners_changed: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryChange {
    pub path: String,
    pub name: String,
    pub old: u64,
    pub new: u64,
    pub diff: Option<SizeDiff>,
}

fn delta(old: u64, new: u64) -> i128 {
    new as i128 - old as i128
}

/// Changes between two keyed sums, biggest first; unchanged ones left out.
fn changes<K: std::hash::Hash + Eq + Clone>(old: &HashMap<K, u64>, new: &HashMap<K, u64>) -> Vec<(K, u64, u64)> {
    let mut out: Vec<(K, u64, u64)> = old
        .iter()
        .map(|(k, &o)| (k.clone(), o, new.get(k).copied().unwrap_or(0)))
        .chain(
            new.iter()
                .filter(|(k, _)| !old.contains_key(*k))
                .map(|(k, &n)| (k.clone(), 0, n)),
        )
        .filter(|(_, o, n)| o != n)
        .collect();
    out.sort_by_key(|&(_, o, n)| std::cmp::Reverse(delta(o, n).abs()));
    out
}

fn owner_sums<'a>(owners: impl Iterator<Item = &'a OwnerBytes>) -> HashMap<(GroupKind, String), u64> {
    let mut sums = HashMap::new();
    for o in owners {
        *sums.entry((o.kind, o.name.clone())).or_default() += o.code + o.data;
    }
    sums
}

fn owner_changes(
    old: &HashMap<(GroupKind, String), u64>,
    new: &HashMap<(GroupKind, String), u64>,
    top: usize,
) -> (Vec<OwnerChange>, u32) {
    let all = changes(old, new);
    let count = all.len() as u32;
    let list = all
        .into_iter()
        .take(top)
        .map(|((kind, name), old, new)| OwnerChange { kind, name, old, new })
        .collect();
    (list, count)
}

/// What changed between two builds of a binary; lists hold the `top` biggest changes.
pub fn diff_binaries(old: &SizeSnapshot, new: &SizeSnapshot, top: usize) -> SizeDiff {
    let kinds = |s: &SizeSnapshot| s.by_kind.iter().copied().collect::<HashMap<RegionKind, u64>>();
    let (old_kinds, new_kinds) = (kinds(old), kinds(new));
    let mut by_kind: Vec<KindChange> = old_kinds
        .keys()
        .chain(new_kinds.keys().filter(|k| !old_kinds.contains_key(*k)))
        .map(|&kind| KindChange {
            kind,
            old: old_kinds.get(&kind).copied().unwrap_or(0),
            new: new_kinds.get(&kind).copied().unwrap_or(0),
        })
        .collect();
    by_kind.sort_by_key(|c| (std::cmp::Reverse(delta(c.old, c.new).abs()), std::cmp::Reverse(c.new)));

    // Sections: bytes in the file, or in memory for those with none there (zero-filled data).
    let sections = |s: &SizeSnapshot| -> HashMap<String, u64> {
        let mut m = HashMap::new();
        for sec in &s.sections {
            *m.entry(sec.name.clone()).or_default() += if sec.file > 0 { sec.file } else { sec.memory };
        }
        m
    };
    let sections = changes(&sections(old), &sections(new))
        .into_iter()
        .map(|(name, old, new)| Change { name, old, new })
        .collect();

    let (owners, owners_changed) = owner_changes(&owner_sums(old.owners.iter()), &owner_sums(new.owners.iter()), top);

    let symbols = |s: &SizeSnapshot| -> HashMap<String, (u64, bool)> {
        s.symbols.iter().map(|x| (x.name.clone(), (x.bytes, x.code))).collect()
    };
    let (old_syms, new_syms) = (symbols(old), symbols(new));
    let (mut added, mut removed, mut changed) = (0, 0, 0);
    let mut list: Vec<SymbolChange> = Vec::new();
    for (name, &(o, code)) in &old_syms {
        match new_syms.get(name) {
            None => removed += 1,
            Some(&(n, _)) if n != o => changed += 1,
            Some(_) => continue,
        }
        let n = new_syms.get(name).map_or(0, |x| x.0);
        list.push(SymbolChange {
            name: name.clone(),
            code,
            old: o,
            new: n,
        });
    }
    for (name, &(n, code)) in new_syms.iter().filter(|(k, _)| !old_syms.contains_key(*k)) {
        added += 1;
        list.push(SymbolChange {
            name: name.clone(),
            code,
            old: 0,
            new: n,
        });
    }
    list.sort_by(|a, b| {
        delta(b.old, b.new)
            .abs()
            .cmp(&delta(a.old, a.new).abs())
            .then_with(|| a.name.cmp(&b.name))
    });
    list.truncate(top);
    SizeDiff {
        old_name: old.name.clone(),
        new_name: new.name.clone(),
        old_size: old.file_size,
        new_size: new.file_size,
        by_kind,
        sections,
        owners,
        owners_changed,
        symbols: list,
        symbols_added: added,
        symbols_removed: removed,
        symbols_changed: changed,
    }
}

fn strip(path: &str, n: usize) -> &str {
    let mut p = path;
    for _ in 0..n {
        p = p.split_once('/').map_or("", |(_, rest)| rest);
    }
    p
}

/// How many leading folders to take off each side's paths so the most
/// match: `MyApp-4.1/…` against `MyApp-4.2/…`, `Payload/My.app/…` (an .ipa)
/// against `My.app/…` (a dropped folder).
fn alignment(old: &[&str], new: &[&str]) -> (usize, usize) {
    let mut best = ((0, 0), 0);
    for total in 0..=4 {
        for k in 0..=total.min(2) {
            let j = total - k;
            if j > 2 {
                continue;
            }
            let olds: HashSet<&str> = old.iter().map(|p| strip(p, k)).filter(|p| !p.is_empty()).collect();
            let hits = new.iter().filter(|p| olds.contains(strip(p, j))).count();
            if hits > best.1 {
                best = ((k, j), hits);
            }
        }
    }
    best.0
}

/// What changed between two builds of a folder; lists hold the `top` biggest changes.
pub fn diff_folders(old: &FolderSnapshot, new: &FolderSnapshot, top: usize) -> FolderDiff {
    let shipped = |f: &&FileBytes| f.category != FileCategory::DebugSymbols;
    let old_files: Vec<&FileBytes> = old.files.iter().filter(shipped).collect();
    let new_files: Vec<&FileBytes> = new.files.iter().filter(shipped).collect();
    let (k, j) = alignment(
        &old_files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        &new_files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
    );
    let sums = |files: &[&FileBytes], n: usize| -> HashMap<String, u64> {
        let mut m = HashMap::new();
        for f in files {
            *m.entry(strip(&f.path, n).to_string()).or_default() += f.bytes;
        }
        m
    };
    let (old_by_path, new_by_path) = (sums(&old_files, k), sums(&new_files, j));
    let all = changes(&old_by_path, &new_by_path);
    let files_added = all.iter().filter(|(_, o, _)| *o == 0).count() as u32;
    let files_removed = all.iter().filter(|(_, _, n)| *n == 0).count() as u32;
    let files_changed = all.len() as u32 - files_added - files_removed;
    let files = all
        .into_iter()
        .take(top)
        .map(|(name, old, new)| Change { name, old, new })
        .collect();

    let by_category = |files: &[&FileBytes]| -> HashMap<FileCategory, u64> {
        let mut m = HashMap::new();
        for f in files {
            *m.entry(f.category).or_default() += f.bytes;
        }
        m
    };
    let (oc, nc) = (by_category(&old_files), by_category(&new_files));
    let mut categories: Vec<CategoryChange> = oc
        .keys()
        .chain(nc.keys().filter(|c| !oc.contains_key(*c)))
        .map(|&category| CategoryChange {
            category,
            old: oc.get(&category).copied().unwrap_or(0),
            new: nc.get(&category).copied().unwrap_or(0),
        })
        .collect();
    categories.sort_by_key(|c| (std::cmp::Reverse(delta(c.old, c.new).abs()), std::cmp::Reverse(c.new)));

    // Binaries by path (else by name, when only one has it).
    let find_old = |path: &str, name: &str| -> Option<&BinarySnapshot> {
        old.binaries.iter().find(|b| strip(&b.path, k) == path).or_else(|| {
            let same: Vec<&BinarySnapshot> = old.binaries.iter().filter(|b| b.snapshot.name == name).collect();
            (same.len() == 1).then(|| same[0])
        })
    };
    let mut matched: HashSet<*const BinarySnapshot> = HashSet::new();
    let mut binaries = Vec::new();
    for b in &new.binaries {
        let path = strip(&b.path, j);
        let before = find_old(path, &b.snapshot.name);
        if let Some(o) = before {
            matched.insert(o as *const _);
        }
        binaries.push(BinaryChange {
            path: path.to_string(),
            name: b.snapshot.name.clone(),
            old: before.map_or(0, |o| o.snapshot.file_size),
            new: b.snapshot.file_size,
            diff: before.map(|o| diff_binaries(&o.snapshot, &b.snapshot, top)),
        });
    }
    for o in old.binaries.iter().filter(|o| !matched.contains(&(*o as *const _))) {
        binaries.push(BinaryChange {
            path: strip(&o.path, k).to_string(),
            name: o.snapshot.name.clone(),
            old: o.snapshot.file_size,
            new: 0,
            diff: None,
        });
    }
    binaries.sort_by_key(|b| (std::cmp::Reverse(delta(b.old, b.new).abs()), std::cmp::Reverse(b.new)));

    let (owners, owners_changed) = owner_changes(
        &owner_sums(old.binaries.iter().flat_map(|b| &b.snapshot.owners)),
        &owner_sums(new.binaries.iter().flat_map(|b| &b.snapshot.owners)),
        top,
    );
    FolderDiff {
        old_name: old.name.clone(),
        new_name: new.name.clone(),
        old_size: old_files.iter().map(|f| f.bytes).sum(),
        new_size: new_files.iter().map(|f| f.bytes).sum(),
        categories,
        files,
        files_added,
        files_removed,
        files_changed,
        binaries,
        owners,
        owners_changed,
    }
}

// --- As text ------------------------------------------------------------------------

fn human(n: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64 / 1024.0;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < UNITS.len() {
        v /= 1024.0;
        u += 1;
    }
    format!("{v:.1} {}", UNITS[u])
}

fn signed(old: u64, new: u64) -> String {
    match new.cmp(&old) {
        std::cmp::Ordering::Greater => format!("+{}", human(new - old)),
        std::cmp::Ordering::Less => format!("-{}", human(old - new)),
        std::cmp::Ordering::Equal => "same".into(),
    }
}

fn percent(old: u64, new: u64) -> String {
    if old == 0 {
        return "new".into();
    }
    format!("{:+.1}%", delta(old, new) as f64 * 100.0 / old as f64)
}

/// "added", "removed", or "old → new".
fn then_now(old: u64, new: u64) -> String {
    match (old, new) {
        (0, n) => format!("added, {}", human(n)),
        (o, 0) => format!("removed, was {}", human(o)),
        (o, n) => format!("{} → {}", human(o), human(n)),
    }
}

fn group_label(k: GroupKind) -> &'static str {
    k.label()
}

impl SizeDiff {
    pub fn to_text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{} → {}: {} → {} ({}, {})",
            self.old_name,
            self.new_name,
            human(self.old_size),
            human(self.new_size),
            signed(self.old_size, self.new_size),
            percent(self.old_size, self.new_size)
        );
        self.body(&mut out, "");
        out
    }

    fn body(&self, out: &mut String, pad: &str) {
        use std::fmt::Write as _;
        let kinds: Vec<&KindChange> = self.by_kind.iter().filter(|c| c.old != c.new).collect();
        if !kinds.is_empty() {
            let _ = writeln!(out, "\n{pad}By kind of bytes:");
            for c in kinds {
                let _ = writeln!(
                    out,
                    "{pad}  {:>10}  {:<18} {}",
                    signed(c.old, c.new),
                    format!("{:?}", c.kind).to_lowercase(),
                    then_now(c.old, c.new)
                );
            }
        }
        if !self.sections.is_empty() {
            let _ = writeln!(out, "\n{pad}Sections:");
            for c in self.sections.iter().take(20) {
                let _ = writeln!(
                    out,
                    "{pad}  {:>10}  {:<24} {}",
                    signed(c.old, c.new),
                    c.name,
                    then_now(c.old, c.new)
                );
            }
        }
        if !self.owners.is_empty() {
            let _ = writeln!(out, "\n{pad}Owners ({} changed):", self.owners_changed);
            for c in &self.owners {
                let _ = writeln!(
                    out,
                    "{pad}  {:>10}  {:<40} {:<26} {}",
                    signed(c.old, c.new),
                    c.name,
                    group_label(c.kind),
                    then_now(c.old, c.new)
                );
            }
        }
        if !self.symbols.is_empty() {
            let _ = writeln!(
                out,
                "\n{pad}Symbols: {} added, {} removed, {} changed size; the biggest changes:",
                self.symbols_added, self.symbols_removed, self.symbols_changed
            );
            for c in &self.symbols {
                let _ = writeln!(
                    out,
                    "{pad}  {:>10}  {}  ({})",
                    signed(c.old, c.new),
                    c.name,
                    then_now(c.old, c.new)
                );
            }
        }
    }
}

impl FolderDiff {
    pub fn to_text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{} → {}: {} → {} ({}, {}); files: {} added, {} removed, {} changed size",
            self.old_name,
            self.new_name,
            human(self.old_size),
            human(self.new_size),
            signed(self.old_size, self.new_size),
            percent(self.old_size, self.new_size),
            self.files_added,
            self.files_removed,
            self.files_changed
        );
        let cats: Vec<&CategoryChange> = self.categories.iter().filter(|c| c.old != c.new).collect();
        if !cats.is_empty() {
            let _ = writeln!(out, "\nBy kind of content:");
            for c in cats {
                let _ = writeln!(
                    out,
                    "  {:>10}  {:<20} {}",
                    signed(c.old, c.new),
                    c.category.label(),
                    then_now(c.old, c.new)
                );
            }
        }
        if !self.binaries.is_empty() {
            let _ = writeln!(out, "\nBinaries:");
            for b in &self.binaries {
                let _ = writeln!(
                    out,
                    "  {:>10}  {:<24} {}  {}",
                    signed(b.old, b.new),
                    b.name,
                    then_now(b.old, b.new),
                    b.path
                );
            }
        }
        if !self.owners.is_empty() {
            let _ = writeln!(out, "\nOwners across the binaries ({} changed):", self.owners_changed);
            for c in &self.owners {
                let _ = writeln!(
                    out,
                    "  {:>10}  {:<40} {:<26} {}",
                    signed(c.old, c.new),
                    c.name,
                    group_label(c.kind),
                    then_now(c.old, c.new)
                );
            }
        }
        if !self.files.is_empty() {
            let _ = writeln!(out, "\nFiles:");
            for c in &self.files {
                let _ = writeln!(
                    out,
                    "  {:>10}  {}  ({})",
                    signed(c.old, c.new),
                    c.name,
                    then_now(c.old, c.new)
                );
            }
        }
        for b in &self.binaries {
            if let Some(d) = b
                .diff
                .as_ref()
                .filter(|d| d.old_size != d.new_size || !d.symbols.is_empty())
            {
                let _ = writeln!(out, "\n── {} ({}) ──", b.name, then_now(d.old_size, d.new_size));
                d.body(&mut out, "  ");
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(name: &str, size: u64, owners: &[(&str, u64)], symbols: &[(&str, u64)]) -> SizeSnapshot {
        SizeSnapshot {
            name: name.into(),
            file_size: size,
            by_kind: vec![(RegionKind::Code, size / 2)],
            sections: vec![SectionBytes {
                name: "__TEXT,__text".into(),
                file: size / 2,
                memory: size / 2,
            }],
            owners: owners
                .iter()
                .map(|&(n, b)| OwnerBytes {
                    kind: GroupKind::SwiftModule,
                    name: n.into(),
                    code: b,
                    data: 0,
                })
                .collect(),
            symbols: symbols
                .iter()
                .map(|&(n, b)| SymbolBytes {
                    name: n.into(),
                    bytes: b,
                    code: true,
                })
                .collect(),
        }
    }

    #[test]
    fn binaries_compared() {
        let old = snap(
            "App",
            1000,
            &[("Core", 300), ("UI", 200)],
            &[("a", 100), ("b", 50), ("gone", 30)],
        );
        let new = snap(
            "App",
            1400,
            &[("Core", 500), ("UI", 200), ("New", 80)],
            &[("a", 100), ("b", 90), ("fresh", 200)],
        );
        let d = diff_binaries(&old, &new, 10);
        assert_eq!((d.old_size, d.new_size), (1000, 1400));
        assert_eq!(d.owners_changed, 2);
        assert_eq!(
            (d.owners[0].name.as_str(), d.owners[0].old, d.owners[0].new),
            ("Core", 300, 500)
        );
        assert_eq!((d.symbols_added, d.symbols_removed, d.symbols_changed), (1, 1, 1));
        let names: Vec<&str> = d.symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["fresh", "b", "gone"], "biggest change first");
        assert_eq!((d.sections[0].old, d.sections[0].new), (500, 700));
        assert!(d.to_text().contains("+400.0 B") || d.to_text().contains("+400 B"));
    }

    #[test]
    fn folders_line_up_whatever_their_roots() {
        let file = |p: &str, b: u64, c: FileCategory| FileBytes {
            path: p.into(),
            bytes: b,
            category: c,
        };
        let old = FolderSnapshot {
            name: "MyApp-4.1".into(),
            files: vec![
                file("Payload/My.app/My", 1000, FileCategory::Binaries),
                file("Payload/My.app/Assets.car", 500, FileCategory::AssetCatalogs),
                file("Payload/My.app/old.png", 40, FileCategory::Images),
            ],
            binaries: vec![BinarySnapshot {
                path: "Payload/My.app/My".into(),
                snapshot: snap("My", 1000, &[("Core", 300)], &[]),
            }],
        };
        // The new build is a dropped .app folder: no Payload/.
        let new = FolderSnapshot {
            name: "My.app".into(),
            files: vec![
                file("My.app/My", 1200, FileCategory::Binaries),
                file("My.app/Assets.car", 500, FileCategory::AssetCatalogs),
                file("My.app/new.png", 60, FileCategory::Images),
                file(
                    "My.app.dSYM/Contents/Resources/DWARF/My",
                    9000,
                    FileCategory::DebugSymbols,
                ),
            ],
            binaries: vec![BinarySnapshot {
                path: "My.app/My".into(),
                snapshot: snap("My", 1200, &[("Core", 450)], &[]),
            }],
        };
        let d = diff_folders(&old, &new, 10);
        assert_eq!((d.old_size, d.new_size), (1540, 1760), "debug files aren't counted");
        assert_eq!((d.files_added, d.files_removed, d.files_changed), (1, 1, 1));
        assert_eq!(d.files[0].name, "My.app/My");
        assert_eq!(d.binaries.len(), 1);
        assert_eq!((d.binaries[0].old, d.binaries[0].new), (1000, 1200));
        assert!(d.binaries[0].diff.is_some());
        assert_eq!((d.owners[0].old, d.owners[0].new), (300, 450));
    }
}
