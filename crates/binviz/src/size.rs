//! Where a binary's bytes go: by region kind, section, segment, symbol, and by
//! owner — Swift module, Objective-C class, C++ namespace or Rust crate, C
//! library prefix — worked out from symbol names, plus source files when there
//! is DWARF.

use std::collections::HashMap;

use serde::Serialize;

use crate::binary::Binary;
use crate::dwarf::attribution::AttributionMode;
use crate::model::{RegionKind, SymbolKind, SymbolSource};

/// What kind of owner a group of symbols has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroupKind {
    SwiftModule,
    ObjcClass,
    /// C++ namespace or Rust crate (first path component of the demangled name).
    Namespace,
    /// A C library's naming prefix (`sqlite3_`, `png_`...).
    CPrefix,
    /// Functions recovered without a name (`sub_...`).
    Unnamed,
    Other,
}

impl GroupKind {
    pub fn label(self) -> &'static str {
        match self {
            GroupKind::SwiftModule => "Swift module",
            GroupKind::ObjcClass => "Objective-C class",
            GroupKind::Namespace => "C++ namespace / Rust crate",
            GroupKind::CPrefix => "C prefix",
            GroupKind::Unnamed => "unnamed functions",
            GroupKind::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub kind: GroupKind,
    pub name: String,
    pub functions: u32,
    pub code_bytes: u64,
    pub data_symbols: u32,
    pub data_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sized {
    pub name: String,
    pub address: u64,
    pub size: u64,
    /// The size was inferred from the next symbol.
    pub approximate: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionSize {
    pub index: u32,
    pub name: String,
    pub kind: RegionKind,
    /// Bytes in the file.
    pub file_bytes: u64,
    /// Bytes in memory.
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeReport {
    pub file_size: u64,
    /// Every byte of the file by region kind, largest first.
    pub by_kind: Vec<(RegionKind, u64)>,
    /// Sections, largest in the file first.
    pub sections: Vec<SectionSize>,
    /// Totals per owner kind (all Swift modules together, ...), largest first.
    pub by_group_kind: Vec<(GroupKind, u64)>,
    /// The biggest owners, largest first.
    pub groups: Vec<Group>,
    pub group_count: u32,
    pub largest_functions: Vec<Sized>,
    pub largest_data: Vec<Sized>,
    /// Bytes of code and data covered by symbols at all.
    pub symbolized_bytes: u64,
    /// Source files by code + data bytes (needs DWARF).
    pub source_files: Vec<(String, u64)>,
    pub strings: u32,
    pub string_bytes: u64,
}

/// The owner of a symbol, from its name.
pub fn group_of(raw: &str, display: &str, source: SymbolSource) -> (GroupKind, String) {
    if source == SymbolSource::Discovered {
        return (GroupKind::Unnamed, "(unnamed functions)".into());
    }
    // Objective-C methods: -[Class(Category) selector]
    if let Some(rest) = display.strip_prefix("-[").or_else(|| display.strip_prefix("+[")) {
        let class = rest.split([' ', ']']).next().unwrap_or(rest);
        let class = class.split('(').next().unwrap_or(class);
        return (GroupKind::ObjcClass, class.to_string());
    }
    for prefix in [
        "_OBJC_CLASS_$_",
        "_OBJC_METACLASS_$_",
        "_OBJC_IVAR_$_",
        "OBJC_CLASS_$_",
        "OBJC_METACLASS_$_",
    ] {
        if let Some(rest) = raw.strip_prefix(prefix) {
            let class = rest.split('.').next().unwrap_or(rest);
            return (GroupKind::ObjcClass, class.to_string());
        }
    }
    // Swift: $s<len><module>... (also $S, $e; _T0 in Swift 3).
    let r = raw.trim_start_matches('_');
    let swift = ["$s", "$S", "$e", "T0"].iter().find_map(|p| r.strip_prefix(p));
    if let Some(rest) = swift {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0
            && let Ok(n) = rest[..digits].parse::<usize>()
            && let Some(module) = rest.get(digits..digits + n)
        {
            return (GroupKind::SwiftModule, module.to_string());
        }
        let module = if rest.starts_with("So") || rest.starts_with("SC") {
            "__C (imported from C/ObjC)"
        } else if rest.starts_with('S') {
            "Swift (standard library)"
        } else {
            "(swift)"
        };
        return (GroupKind::SwiftModule, module.into());
    }
    // Demangled C++ / Rust: the first path component.
    if display.contains("::") {
        let d = display.trim_start_matches(['<', '&', '*']);
        let d = d.strip_prefix("const ").unwrap_or(d);
        let first = d.split("::").next().unwrap_or(d);
        // `(anonymous namespace)`, `{closure}`... keep as they are.
        let first = first
            .split(['<', ' '])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(first);
        return (GroupKind::Namespace, first.to_string());
    }
    // C: a library prefix like `sqlite3_`.
    let plain = display.trim_start_matches('_');
    if let Some((prefix, _)) = plain.split_once('_')
        && prefix.len() >= 3
        && prefix.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return (GroupKind::CPrefix, format!("{prefix}_"));
    }
    (GroupKind::Other, "(other)".into())
}

impl Binary {
    /// Where the bytes go. `top` bounds each list.
    pub fn size_report(&self, top: usize) -> SizeReport {
        let top = top.max(1);
        let mut sections: Vec<SectionSize> = self
            .sections
            .iter()
            .map(|s| SectionSize {
                index: s.index,
                name: match &s.segment_name {
                    Some(seg) if !seg.is_empty() => format!("{seg},{}", s.name),
                    _ => s.name.clone(),
                },
                kind: s.kind,
                file_bytes: if s.file_offset.is_some() { s.file_size } else { 0 },
                memory_bytes: if s.loaded { s.size } else { 0 },
            })
            .collect();
        sections.sort_by_key(|s| std::cmp::Reverse(s.file_bytes.max(s.memory_bytes)));

        // Owners and the largest symbols, over the symbols used for lookups
        // (one per address, sized).
        let mut groups: HashMap<(GroupKind, String), Group> = HashMap::new();
        let mut functions: Vec<(u64, u32)> = Vec::new();
        let mut data: Vec<(u64, u32)> = Vec::new();
        let mut symbolized = 0u64;
        let mut last_end = 0u64;
        for s in self.symbols.in_range(0, u64::MAX) {
            if s.size == 0 || !matches!(s.kind, SymbolKind::Function | SymbolKind::Data | SymbolKind::Unknown) {
                continue;
            }
            // Overlapping symbols (aliases, nested labels) count once.
            let start = s.address.max(last_end);
            let end = s.address + s.size;
            if end <= start {
                continue;
            }
            let bytes = end - start;
            last_end = end;
            symbolized += bytes;
            let code = s.kind == SymbolKind::Function;
            let display = s.display_name();
            let (kind, name) = group_of(s.name(), &display, s.source);
            let g = groups.entry((kind, name.clone())).or_insert_with(|| Group {
                kind,
                name,
                functions: 0,
                code_bytes: 0,
                data_symbols: 0,
                data_bytes: 0,
            });
            if code {
                g.functions += 1;
                g.code_bytes += bytes;
                functions.push((s.size, s.index));
            } else {
                g.data_symbols += 1;
                g.data_bytes += bytes;
                data.push((s.size, s.index));
            }
        }
        let biggest = |v: &mut Vec<(u64, u32)>| -> Vec<Sized> {
            let n = top.min(v.len());
            if n > 0 {
                v.select_nth_unstable_by(n - 1, |a, b| b.cmp(a));
                v.truncate(n);
                v.sort_unstable_by(|a, b| b.cmp(a));
            }
            v.iter()
                .filter_map(|&(size, i)| {
                    let s = self.symbols.get(i)?;
                    Some(Sized {
                        name: s.display_name().into_owned(),
                        address: s.address,
                        size,
                        approximate: s.size_inferred,
                    })
                })
                .collect()
        };
        let largest_functions = biggest(&mut functions);
        let largest_data = biggest(&mut data);

        let mut by_group_kind: HashMap<GroupKind, u64> = HashMap::new();
        for g in groups.values() {
            *by_group_kind.entry(g.kind).or_default() += g.code_bytes + g.data_bytes;
        }
        let mut by_group_kind: Vec<(GroupKind, u64)> = by_group_kind.into_iter().collect();
        by_group_kind.sort_by_key(|&(k, n)| (std::cmp::Reverse(n), k));
        let group_count = groups.len() as u32;
        let mut groups: Vec<Group> = groups.into_values().collect();
        groups.sort_by(|a, b| {
            (b.code_bytes + b.data_bytes)
                .cmp(&(a.code_bytes + a.data_bytes))
                .then_with(|| a.name.cmp(&b.name))
        });
        groups.truncate(top);

        let source_files = self
            .attribution(AttributionMode::File)
            .map(|a| {
                a.contributors
                    .into_iter()
                    .take(top)
                    .map(|c| (c.path, c.code + c.data))
                    .collect()
            })
            .unwrap_or_default();
        let strings = self.string_index();
        SizeReport {
            file_size: self.data.len() as u64,
            by_kind: self.composition(),
            sections,
            by_group_kind,
            groups,
            group_count,
            largest_functions,
            largest_data,
            symbolized_bytes: symbolized,
            source_files,
            strings: strings.recs.len() as u32,
            string_bytes: strings.recs.iter().map(|r| r.len() as u64).sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_from_names() {
        let g = |raw: &str, d: &str| group_of(raw, d, SymbolSource::Symtab);
        assert_eq!(g("-[Foo bar:]", "-[Foo bar:]"), (GroupKind::ObjcClass, "Foo".into()));
        assert_eq!(
            g("+[Foo(Extra) make]", "+[Foo(Extra) make]"),
            (GroupKind::ObjcClass, "Foo".into())
        );
        assert_eq!(
            g("_OBJC_CLASS_$_Bar", "_OBJC_CLASS_$_Bar"),
            (GroupKind::ObjcClass, "Bar".into())
        );
        assert_eq!(
            g("_$s7MyStuff4ViewC4bodyyF", ""),
            (GroupKind::SwiftModule, "MyStuff".into())
        );
        assert_eq!(
            g("_$sSS5countSivg", ""),
            (GroupKind::SwiftModule, "Swift (standard library)".into())
        );
        assert_eq!(
            g("_ZN3geo4Rect4areaEv", "geo::Rect::area() const"),
            (GroupKind::Namespace, "geo".into())
        );
        assert_eq!(
            g("x", "<alloc::vec::Vec<u8> as core::fmt::Debug>::fmt"),
            (GroupKind::Namespace, "alloc".into())
        );
        assert_eq!(
            g("_sqlite3_open", "_sqlite3_open"),
            (GroupKind::CPrefix, "sqlite3_".into())
        );
        assert_eq!(g("main", "main"), (GroupKind::Other, "(other)".into()));
        assert_eq!(
            group_of("sub_1000", "sub_1000", SymbolSource::Discovered),
            (GroupKind::Unnamed, "(unnamed functions)".into())
        );
    }
}
