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
    /// Helpers the compiler made: outlined code, block copy/destroy helpers,
    /// global initializers, Swift runtime helpers.
    CompilerGenerated,
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
            GroupKind::CompilerGenerated => "compiler-generated",
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

/// Objective-C metadata named after its class (the class object, its ivars,
/// method, property and protocol lists, categories).
const OBJC_METADATA: [&str; 24] = [
    "OBJC_CLASS_$_",
    "OBJC_METACLASS_$_",
    "OBJC_IVAR_$_",
    "OBJC_CLASS_RO_$_",
    "OBJC_METACLASS_RO_$_",
    "OBJC_$_PROP_LIST_",
    "OBJC_$_CLASS_PROP_LIST_",
    "OBJC_$_CATEGORY_CLASS_METHODS_",
    "OBJC_$_CATEGORY_INSTANCE_METHODS_",
    "OBJC_$_CATEGORY_",
    "OBJC_$_INSTANCE_VARIABLES_",
    "OBJC_$_INSTANCE_METHODS_",
    "OBJC_$_CLASS_METHODS_",
    "OBJC_$_PROTOCOL_INSTANCE_METHODS_OPT_",
    "OBJC_$_PROTOCOL_CLASS_METHODS_OPT_",
    "OBJC_$_PROTOCOL_INSTANCE_METHODS_",
    "OBJC_$_PROTOCOL_CLASS_METHODS_",
    "OBJC_$_PROTOCOL_METHOD_TYPES_",
    "OBJC_$_CLASS_PROTOCOLS_",
    "OBJC_$_PROTOCOL_REFS_",
    "OBJC_PROTOCOL_$_",
    "OBJC_CLASS_PROTOCOLS_$_",
    "OBJC_CATEGORY_PROTOCOLS_$_",
    "OBJC_LABEL_PROTOCOL_$_",
];

/// Names the compiler gives its helpers, as C writes them (a Mach-O
/// symbol has one more underscore in front).
const COMPILER_GENERATED: [&str; 8] = [
    "OUTLINED_FUNCTION",
    "globalinit_",
    "block_",
    "__Block_",
    "__copy_",
    "__destroy",
    "__swift_",
    "objectdestroy.",
];

/// A class name as Objective-C sees it: Swift classes exposed to it have
/// mangled names (`_TtC5MyApp14ViewController`), or as binviz writes them
/// (`MyApp.ViewController`), which belong to a Swift module.
fn class_owner(class: &str) -> (GroupKind, String) {
    if let Some((module, _)) = class.split_once('.') {
        return (GroupKind::SwiftModule, module.to_string());
    }
    let t = class.trim_start_matches('_');
    if let Some(rest) = t.strip_prefix("Tt")
        && let Some(rest) = rest.strip_prefix(|c: char| c.is_ascii_uppercase())
    {
        // `s` is the standard library: _TtCs12_SwiftObject.
        if rest.starts_with('s') && !rest[1..].starts_with(|c: char| c.is_ascii_digit()) {
            return (GroupKind::SwiftModule, "Swift (standard library)".into());
        }
        let rest = rest.strip_prefix('s').unwrap_or(rest);
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if let Ok(n) = rest[..digits].parse::<usize>()
            && let Some(module) = rest.get(digits..digits + n)
        {
            return (GroupKind::SwiftModule, module.to_string());
        }
    }
    (GroupKind::ObjcClass, t.to_string())
}

/// The owner of a symbol, from its name.
pub fn group_of(raw: &str, display: &str, source: SymbolSource) -> (GroupKind, String) {
    if source == SymbolSource::Discovered {
        return (GroupKind::Unnamed, "(unnamed functions)".into());
    }
    // Objective-C methods: -[Class(Category) selector], also as part of a
    // longer name (the blocks inside a method: __23-[Class selector]_block_invoke).
    if let Some(at) = display.find("-[").or_else(|| display.find("+[")) {
        let rest = &display[at + 2..];
        let class = rest.split([' ', ']']).next().unwrap_or(rest);
        let class = class.split('(').next().unwrap_or(class);
        if !class.is_empty() {
            return class_owner(class);
        }
    }
    // Objective-C metadata, named after its class (and category: Class_$_Category).
    let bare = raw.trim_start_matches('_');
    let bare = bare.strip_prefix("l_").unwrap_or(bare).trim_start_matches('_');
    for prefix in OBJC_METADATA {
        if let Some(rest) = bare.strip_prefix(prefix) {
            let class = rest.split('.').next().unwrap_or(rest);
            let class = class.split("_$_").next().unwrap_or(class);
            return class_owner(class);
        }
    }
    // What code loads to send a message or name a class or protocol (see crate::objc).
    if ["@selector(", "@class(", "@protocol("]
        .iter()
        .any(|p| raw.starts_with(p))
    {
        return (GroupKind::ObjcClass, "(Objective-C references)".into());
    }
    // The Objective-C runtime's functions and message stubs (objc_msgSend$selector).
    if bare.starts_with("objc_") {
        return (GroupKind::ObjcClass, "(Objective-C runtime)".into());
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
    // Helpers the compiler made.
    let c_name = raw.strip_prefix('_').unwrap_or(raw);
    if COMPILER_GENERATED
        .iter()
        .any(|p| raw.starts_with(p) || c_name.starts_with(p))
    {
        return (GroupKind::CompilerGenerated, "(compiler-generated)".into());
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
    /// Every sized function and data symbol in the file, with the bytes it
    /// accounts for: overlapping ones (aliases, nested labels) count once, and
    /// zero-filled data (bss) takes no room in the file.
    fn for_each_sized(&self, mut f: impl FnMut(&crate::Sym<'_>, u64)) {
        let mut last_end = 0u64;
        for s in self.symbols.in_range(0, u64::MAX) {
            if s.size == 0 || !matches!(s.kind, SymbolKind::Function | SymbolKind::Data | SymbolKind::Unknown) {
                continue;
            }
            if let Some(sec) = s.section.and_then(|i| self.sections.get(i as usize))
                && (sec.file_offset.is_none() || sec.file_size == 0)
            {
                continue;
            }
            let start = s.address.max(last_end);
            let end = s.address + s.size;
            if end <= start {
                continue;
            }
            last_end = end;
            f(&s, end - start);
        }
    }

    /// What the binary's size is made of, all of it, to compare with another
    /// build (see [`crate::diff`]).
    pub fn size_snapshot(&self, name: &str) -> crate::diff::SizeSnapshot {
        let mut owners: HashMap<(GroupKind, String), (u64, u64)> = HashMap::new();
        let mut symbols: HashMap<String, (u64, bool)> = HashMap::new();
        self.for_each_sized(|s, bytes| {
            let code = s.kind == SymbolKind::Function;
            let display = s.display_name();
            let owner = owners.entry(group_of(s.name(), &display, s.source)).or_default();
            if code {
                owner.0 += bytes;
            } else {
                owner.1 += bytes;
            }
            // Functions recovered without names are called after their addresses: no match across builds.
            if s.source != SymbolSource::Discovered {
                symbols.entry(display.into_owned()).or_insert((0, code)).0 += bytes;
            }
        });
        crate::diff::SizeSnapshot {
            name: name.to_string(),
            file_size: self.data.len() as u64,
            by_kind: self.composition(),
            sections: self
                .sections
                .iter()
                .map(|s| crate::diff::SectionBytes {
                    name: match &s.segment_name {
                        Some(seg) if !seg.is_empty() => format!("{seg},{}", s.name),
                        _ => s.name.clone(),
                    },
                    file: if s.file_offset.is_some() { s.file_size } else { 0 },
                    memory: if s.loaded { s.size } else { 0 },
                })
                .collect(),
            owners: owners
                .into_iter()
                .map(|((kind, name), (code, data))| crate::diff::OwnerBytes { kind, name, code, data })
                .collect(),
            symbols: symbols
                .into_iter()
                .map(|(name, (bytes, code))| crate::diff::SymbolBytes { name, bytes, code })
                .collect(),
        }
    }

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
        self.for_each_sized(|s, bytes| {
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
        });
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
        // Blocks belong to the method they are in; metadata to its class (and
        // a Swift class's to its module); runtime stubs together.
        assert_eq!(
            g("___23-[Foo bar]_block_invoke", "___23-[Foo bar]_block_invoke"),
            (GroupKind::ObjcClass, "Foo".into())
        );
        assert_eq!(
            g("__OBJC_$_INSTANCE_METHODS_Foo", "__OBJC_$_INSTANCE_METHODS_Foo"),
            (GroupKind::ObjcClass, "Foo".into())
        );
        assert_eq!(
            g("l_OBJC_$_CATEGORY_INSTANCE_METHODS_Foo_$_Extra", ""),
            (GroupKind::ObjcClass, "Foo".into())
        );
        assert_eq!(
            g("_OBJC_CLASS_$__TtC5MyApp14ViewController", ""),
            (GroupKind::SwiftModule, "MyApp".into())
        );
        assert_eq!(
            g(
                "-[_TtC5MyApp14ViewController viewDidLoad]",
                "-[_TtC5MyApp14ViewController viewDidLoad]"
            ),
            (GroupKind::SwiftModule, "MyApp".into())
        );
        assert_eq!(
            g("_objc_msgSend$tableView:cellForRowAtIndexPath:", ""),
            (GroupKind::ObjcClass, "(Objective-C runtime)".into())
        );
        // As binviz names a stripped binary's methods and metadata.
        assert_eq!(
            g(
                "-[MyApp.ViewController viewDidLoad]",
                "-[MyApp.ViewController viewDidLoad]"
            ),
            (GroupKind::SwiftModule, "MyApp".into())
        );
        assert_eq!(
            g("__OBJC_$_PROTOCOL_INSTANCE_METHODS_OPT_Delegate", ""),
            (GroupKind::ObjcClass, "Delegate".into())
        );
        assert_eq!(
            g("__OBJC_CATEGORY_PROTOCOLS_$_NSObject_$_Extras", ""),
            (GroupKind::ObjcClass, "NSObject".into())
        );
        assert_eq!(
            g("@selector(hello)", "@selector(hello)"),
            (GroupKind::ObjcClass, "(Objective-C references)".into())
        );
        // What the compiler made.
        for name in [
            "_OUTLINED_FUNCTION_12",
            "___copy_helper_block_e8_32s",
            "___swift_allocate_value_buffer",
            "_globalinit_33_A1",
        ] {
            assert_eq!(
                g(name, name),
                (GroupKind::CompilerGenerated, "(compiler-generated)".into()),
                "{name}"
            );
        }
        // A C function that merely starts like one isn't.
        assert_eq!(
            g("_destroyWindow", "_destroyWindow"),
            (GroupKind::Other, "(other)".into())
        );
        assert_eq!(
            group_of("sub_1000", "sub_1000", SymbolSource::Discovered),
            (GroupKind::Unnamed, "(unnamed functions)".into())
        );
    }
}
