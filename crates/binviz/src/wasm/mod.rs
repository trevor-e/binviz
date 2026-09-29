//! WebAssembly modules, such as the `.wasm` a game's browser port ships
//! (compiled with Emscripten, or clang and `wasm-ld`): read section by
//! section into the same model as other binaries, so that a module opens,
//! disassembles, size-diffs and has its stack traces symbolicated like any
//! executable. Objects (`clang --target=wasm32 -c`) open too.
//!
//! ## Addresses
//!
//! A module has no load address. binviz gives it one address space with
//! two parts:
//!
//! - **The module's bytes are at their file offsets.** Every section the
//!   engine reads (all but the custom sections) is loaded where it is in the
//!   file, so an address there and its file offset are the same number. A
//!   function is at its body's locals declarations: where engines and DWARF
//!   (`DW_AT_low_pc`) say a function starts, and so the numbers browsers
//!   print in stack frames (`wasm-function[12]:0x1a2b` is an offset in the
//!   module, inside function 12's body) need no translating. An imported
//!   function or global is at its entry in the import section and a defined
//!   global at its entry in the global section, so that calls to imports
//!   and uses of globals have somewhere to point.
//! - **Linear memory is at `0x80000000` plus its address.** Each data
//!   segment the module writes into memory 0 when it is instantiated is a
//!   section there, its bytes those in the file. Above them, the zero-filled
//!   variables are a section with no bytes, as bss is: `.bss` up to the last
//!   variable known, or knowing none, `bss and stack` up to where the stack
//!   pointer starts. So the string the code passes as `i32.const 1024` is at
//!   `0x80000400`, and the global variables DWARF or the exports describe
//!   get symbols there.
//!
//! DWARF counts code addresses from the start of the code section's
//! contents, and data addresses in linear memory; both are moved to these
//! addresses as the DWARF is read (see [`dwarf`]). A source map counts in
//! module offsets already (see [`sourcemap`]).

pub(crate) mod analysis;
pub(crate) mod code;
pub(crate) mod dwarf;
pub(crate) mod layout;
pub(crate) mod read;
pub(crate) mod sourcemap;

use std::collections::HashMap;
use std::sync::Arc;

use object::Architecture;

use crate::binary::Binary;
use crate::error::Result;
use crate::layout::Machine;
use crate::model::{Export, Format, Import, Property, RegionKind, Section, Summary, SymbolKind, SymbolSource};
use crate::symbols::{Binding, Builder as SymbolBuilder, NewSym};
use crate::util::{self, Endian};
pub(crate) use read::Module;
use read::{Const, ImportDesc, Kind, Mode};

/// Where linear memory 0 starts among binviz's addresses: above any module's
/// bytes, and low enough that a 32-bit DWARF address can hold it.
pub(crate) const MEMORY_BASE: u64 = 0x8000_0000;

/// Whether `data` is a WebAssembly module.
pub(crate) fn is_wasm(data: &[u8]) -> bool {
    read::is_module(data)
}

impl Module {
    /// A function's address: its body's start, or its entry in the import section.
    pub fn function_address(&self, index: u32) -> Option<u64> {
        let f = self.funcs.get(index as usize)?;
        match (f.import, f.body) {
            (Some(i), _) => self.imports.get(i as usize).map(|i| i.entry.start),
            (None, Some(b)) => Some(b.start),
            _ => None,
        }
    }

    /// A global's address: its entry in the global (or import) section.
    pub fn global_address(&self, index: u32) -> Option<u64> {
        self.globals.get(index as usize).map(|g| g.entry.start)
    }

    /// Whether memory 0 is there and addressed with 32-bit numbers, so that
    /// its addresses have a place among binviz's.
    pub fn memory_mapped(&self) -> bool {
        self.memories.first().is_some_and(|m| !m.limits.is64)
    }

    /// binviz's address for an address in linear memory 0.
    pub fn memory_address(&self, linear: u64) -> Option<u64> {
        (self.memory_mapped() && linear < 1 << 32).then_some(MEMORY_BASE + linear)
    }

    /// Where a data segment is written in memory 0, when the module says
    /// (an active segment with a constant offset, or a passive one its start
    /// function copies to a constant address): binviz's address.
    pub fn segment_address(&self, index: u32) -> Option<u64> {
        match self.data.get(index as usize)?.mode {
            Mode::Active { index: 0, offset } => self.memory_address(offset.address()? & 0xFFFF_FFFF),
            Mode::Passive => self.memory_address(*self.copied.get(&index)?),
            _ => None,
        }
    }

    /// A data segment's name: the name section's, an object's, else `data[N]`.
    pub fn segment_name(&self, index: u32) -> String {
        self.names
            .data
            .get(&index)
            .or_else(|| self.segment_names.get(&index))
            .cloned()
            .unwrap_or_else(|| format!("data[{index}]"))
    }

    /// The stack pointer's initial value: the first mutable i32 global the
    /// module defines (`__stack_pointer`, where `wasm-ld` and Emscripten put it).
    fn stack_top(&self) -> Option<u64> {
        self.globals
            .iter()
            .find(|g| g.import.is_none() && g.mutable && g.ty == read::ValType::I32)
            .and_then(|g| g.init?.address())
    }

    /// Memory above the data segments that holds the program's zero-filled
    /// variables: up to the last variable known there (`vars_end`), else,
    /// knowing none, up to where the stack pointer starts when that is above
    /// the data (`wasm-ld` puts the stack after the data unless told
    /// `--stack-first`), so the stack too. The heap, beyond, is no section.
    /// (name, start, end), binviz's addresses.
    fn zero_filled(&self, vars_end: u64) -> Option<(&'static str, u64, u64)> {
        let memory = self.memories.first().filter(|_| self.memory_mapped())?;
        let size = MEMORY_BASE + memory.limits.min.saturating_mul(memory.limits.page_size()).min(1 << 32);
        let start = (0..self.data.len() as u32)
            .filter_map(|i| {
                let bytes = &self.data[i as usize].bytes;
                Some(self.segment_address(i)? + (bytes.end - bytes.start))
            })
            .max()
            .unwrap_or(MEMORY_BASE);
        let (name, end) = match self.stack_top().and_then(|s| self.memory_address(s)) {
            _ if vars_end > start => (".bss", vars_end),
            Some(top) if top > start => ("bss and stack", top),
            _ => return None,
        };
        let end = end.min(size);
        (end > start).then_some((name, start, end))
    }

    /// The first name each function is exported under.
    fn export_names(&self, kind: Kind) -> HashMap<u32, &str> {
        let mut out = HashMap::new();
        for e in self.exports.iter().filter(|e| e.kind == kind) {
            out.entry(e.index).or_insert(e.name.as_str());
        }
        out
    }

    /// An object's names for its functions or globals (its `linking` section).
    fn link_names(&self, kind: u8) -> HashMap<u32, &str> {
        self.symbols
            .iter()
            .filter(|s| s.kind == kind)
            .filter_map(|s| Some((s.index, s.name.as_deref()?)))
            .collect()
    }
}

/// A data segment's kind, by the name the linker gave it.
pub(crate) fn segment_kind(name: &str) -> RegionKind {
    if name.starts_with(".rodata") {
        RegionKind::Rodata
    } else if name.starts_with(".tdata") || name.starts_with(".tbss") {
        RegionKind::Tls
    } else {
        RegionKind::Data
    }
}

/// A custom section's kind, by its name.
pub(crate) fn custom_kind(name: &str) -> RegionKind {
    if name.starts_with(".debug") || name == "sourceMappingURL" || name == "external_debug_info" {
        RegionKind::Debug
    } else if name == "name" {
        RegionKind::Symbols
    } else if name == "linking" || name.starts_with("dylink") {
        RegionKind::Linking
    } else if name.starts_with("reloc.") {
        RegionKind::Relocations
    } else {
        RegionKind::Notes
    }
}

/// The module's sections as binviz's: each section at its contents, but
/// for the data section, which becomes a section per data segment where it
/// is written in memory.
fn sections(m: &Module) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut push = |name: String, kind, address, size, file_offset: Option<u64>, flags: &str, perms: &str, loaded| {
        out.push(Section {
            index: out.len() as u32,
            name,
            segment_name: None,
            kind,
            address,
            size,
            file_offset,
            file_size: if file_offset.is_some() { size } else { 0 },
            align: 1,
            flags: flags.into(),
            perms: perms.into(),
            compressed: false,
            segment: None,
            loaded,
        });
    };
    for s in &m.sections {
        let size = s.end - s.content;
        match s.id {
            read::DATA => {
                for (i, seg) in m.data.iter().enumerate() {
                    let name = m.segment_name(i as u32);
                    let kind = segment_kind(&name);
                    let at = m.segment_address(i as u32);
                    let flags = match seg.mode {
                        Mode::Active { .. } => "active",
                        _ => "passive",
                    };
                    let perms = if kind == RegionKind::Rodata { "r--" } else { "rw-" };
                    let len = seg.bytes.end - seg.bytes.start;
                    push(
                        name,
                        kind,
                        at.unwrap_or(seg.bytes.start),
                        len,
                        Some(seg.bytes.start),
                        flags,
                        perms,
                        at.is_some(),
                    );
                }
            }
            read::CODE => push(
                s.name.clone(),
                RegionKind::Code,
                s.content,
                size,
                Some(s.content),
                "",
                "r-x",
                true,
            ),
            read::IMPORT | read::EXPORT => push(
                s.name.clone(),
                RegionKind::Linking,
                s.content,
                size,
                Some(s.content),
                "",
                "r--",
                true,
            ),
            read::CUSTOM => push(
                s.name.clone(),
                custom_kind(&s.name),
                s.content,
                size,
                Some(s.content),
                "custom",
                "---",
                false,
            ),
            _ => push(
                s.name.clone(),
                RegionKind::Metadata,
                s.content,
                size,
                Some(s.content),
                "",
                "r--",
                true,
            ),
        }
    }
    out
}

/// A variable in memory: (address, name, size, where the name comes from).
type Variable = (u64, String, u64, SymbolSource);

/// The variables in memory 0, one per address: an object's data symbols,
/// then DWARF's (which know their sizes), then exported globals that hold
/// an address (`--export-all` exports every C variable so), in address order.
fn variables(m: &Module, debug: Option<&crate::dwarf::DebugInfo>) -> Vec<Variable> {
    let mut data: HashMap<u64, (String, u64, SymbolSource)> = HashMap::new();
    // Not the compiler's private labels (`.L.str`): a string reads better as its text.
    let private = |s: &read::LinkSymbol| s.name.as_deref().is_some_and(|n| n.starts_with(".L"));
    for s in m.symbols.iter().filter(|s| s.kind == 1 && !private(s)) {
        if let (Some(name), Some((seg, offset, size))) = (&s.name, s.data)
            && let Some(at) = m.segment_address(seg)
        {
            data.entry(at + offset)
                .or_insert((name.clone(), size, SymbolSource::Symtab));
        }
    }
    if let Some(d) = debug {
        // Not the unnamed ones (string literals), for the same reason.
        for g in d.globals().iter().filter(|g| g.name != format!("var_{:x}", g.address)) {
            data.entry(g.address)
                .or_insert((g.name.clone(), g.size, SymbolSource::Dwarf));
        }
    }
    for e in m.exports.iter().filter(|e| e.kind == Kind::Global) {
        let Some(g) = m.globals.get(e.index as usize).filter(|g| holds_address(m, g)) else {
            continue;
        };
        if let Some(Const::I32(v)) = g.init
            && let Some(at) = m.memory_address(u64::from(v as u32))
        {
            data.entry(at).or_insert((e.name.clone(), 0, SymbolSource::Export));
        }
    }
    // In the memory: a variable the linker dropped keeps a tombstone for its address.
    let size = m
        .memories
        .first()
        .map_or(0, |mem| mem.limits.min.saturating_mul(mem.limits.page_size()));
    let mut out: Vec<Variable> = data
        .into_iter()
        .filter(|(a, _)| (MEMORY_BASE..MEMORY_BASE.saturating_add(size)).contains(a))
        .map(|(a, (n, s, src))| (a, n, s, src))
        .collect();
    out.sort_by_key(|v| v.0);
    out
}

/// Whether a global holds an address in memory rather than being a
/// variable of its own: immutable, its value a constant there.
fn holds_address(m: &Module, g: &read::Global) -> bool {
    matches!((g.mutable, g.init), (false, Some(Const::I32(v))) if m.memory_address(u64::from(v as u32)).is_some())
}

/// The loaded section holding `address`.
fn section_of(sections: &[Section], address: u64) -> Option<u32> {
    sections
        .iter()
        .find(|s| s.loaded && address >= s.address && address < s.address + s.size.max(1))
        .map(|s| s.index)
}

/// The module's functions, globals and variables as symbols. A function is
/// named by the name section, else an object's symbol table, else the name
/// section of the module its debug info came from (`debug_names`), else
/// DWARF, else its export, else `func[N]`; an imported one by the name
/// sections, else `module.field`. Globals likewise, but for DWARF.
fn symbols(
    m: &Module,
    sections: &[Section],
    debug: Option<&crate::dwarf::DebugInfo>,
    debug_names: Option<&read::Names>,
    variables: &[Variable],
) -> SymbolBuilder {
    let mut out = SymbolBuilder::default();
    let exported = m.export_names(Kind::Func);
    let linked = m.link_names(0);
    let dwarf: HashMap<u64, String> = debug
        .map(|d| d.subprograms().into_iter().map(|(name, a, _)| (a, name)).collect())
        .unwrap_or_default();
    for (i, f) in m.funcs.iter().enumerate() {
        let i = i as u32;
        let named = m
            .names
            .functions
            .get(&i)
            .map(String::as_str)
            .or(linked.get(&i).copied());
        let from_debug = debug_names.and_then(|d| d.functions.get(&i)).map(String::as_str);
        if let Some(imp) = f.import.and_then(|imp| m.imports.get(imp as usize)) {
            let fallback = format!("{}.{}", imp.module, imp.field);
            out.push(NewSym {
                name: named.or(from_debug).unwrap_or(&fallback),
                address: imp.entry.start,
                size: imp.entry.end - imp.entry.start,
                kind: SymbolKind::Function,
                binding: Binding::Global,
                section: section_of(sections, imp.entry.start),
                source: SymbolSource::Import,
                defined: true,
                plain: false,
            });
            continue;
        }
        let Some(b) = f.body else { continue };
        let generic = format!("func[{i}]");
        let (name, source) = match (named, from_debug, dwarf.get(&b.start), exported.get(&i)) {
            (Some(n), ..) => (n, SymbolSource::Symtab),
            (None, Some(n), ..) => (n, SymbolSource::DebugFile),
            (None, None, Some(n), _) => (n.as_str(), SymbolSource::Dwarf),
            (None, None, None, Some(n)) => (*n, SymbolSource::Export),
            // Numbered, not named: a number doesn't name the same function in another build.
            (None, None, None, None) => (generic.as_str(), SymbolSource::Discovered),
        };
        out.push(NewSym {
            name,
            address: b.start,
            size: b.end - b.start,
            kind: SymbolKind::Function,
            binding: if exported.contains_key(&i) {
                Binding::Global
            } else {
                Binding::Local
            },
            section: section_of(sections, b.start),
            source,
            defined: true,
            plain: false,
        });
    }
    let exported_globals = m.export_names(Kind::Global);
    let linked_globals = m.link_names(2);
    for (i, g) in m.globals.iter().enumerate() {
        let i = i as u32;
        // An exported global that holds a variable's address in memory (as
        // `--export-all` exports every C variable) names that variable, not itself.
        if !m.names.globals.contains_key(&i) && exported_globals.contains_key(&i) && holds_address(m, g) {
            continue;
        }
        let imported = g.import.and_then(|imp| m.imports.get(imp as usize));
        let fallback = imported.map(|imp| format!("{}.{}", imp.module, imp.field));
        let (name, source) = match (
            m.names.globals.get(&i).or(fallback.as_ref()).map(String::as_str),
            linked_globals.get(&i),
            debug_names.and_then(|d| d.globals.get(&i)),
            exported_globals.get(&i),
        ) {
            (Some(n), ..) => (n, SymbolSource::Symtab),
            (None, Some(n), ..) => (*n, SymbolSource::Symtab),
            (None, None, Some(n), _) => (n.as_str(), SymbolSource::DebugFile),
            (None, None, None, Some(n)) => (*n, SymbolSource::Export),
            _ => continue,
        };
        out.push(NewSym {
            name,
            address: g.entry.start,
            size: g.entry.end - g.entry.start,
            kind: SymbolKind::Data,
            binding: Binding::Global,
            section: section_of(sections, g.entry.start),
            source: if imported.is_some() {
                SymbolSource::Import
            } else {
                source
            },
            defined: true,
            plain: false,
        });
    }
    for (address, name, size, source) in variables {
        let Some(section) = section_of(sections, *address) else {
            continue;
        };
        out.push(NewSym {
            name,
            address: *address,
            size: *size,
            kind: SymbolKind::Data,
            binding: Binding::Global,
            section: Some(section),
            source: *source,
            defined: true,
            plain: *source == SymbolSource::Dwarf,
        });
    }
    out
}

/// At most `max` names, then how many more.
fn some_of(names: &[String], max: usize) -> String {
    let mut s = names.iter().take(max).cloned().collect::<Vec<_>>().join(", ");
    if names.len() > max {
        s.push_str(&format!(", … ({} more)", names.len() - max));
    }
    s
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Bytes as KiB or MiB when they are that many.
fn bytes(n: u64) -> String {
    if n >= 1 << 20 && n.is_multiple_of(1 << 20) {
        format!("{} MiB", n >> 20)
    } else if n >= 1 << 10 && n.is_multiple_of(1 << 10) {
        format!("{} KiB", n >> 10)
    } else {
        format!("{n} bytes")
    }
}

/// A memory's size: its pages and what they come to.
pub(crate) fn memory_text(l: &read::Limits) -> String {
    let page = l.page_size();
    let mut s = format!(
        "{} ({})",
        plural(l.min as usize, "page", "pages"),
        bytes(l.min.saturating_mul(page))
    );
    match l.max {
        Some(max) => s.push_str(&format!(", at most {}", bytes(max.saturating_mul(page)))),
        None => s.push_str(", no maximum"),
    }
    if l.shared {
        s.push_str(", shared");
    }
    if l.is64 {
        s.push_str(", 64-bit");
    }
    s
}

/// The facts `info` shows: the functions, imports, exports, start function,
/// function table, memory, data and producers, and where the debug info is.
fn properties(m: &Module, symbols: &crate::symbols::SymbolTable) -> Vec<Property> {
    let mut out = Vec::new();
    let mut prop = |key: &str, value: String| {
        if !value.is_empty() {
            out.push(Property { key: key.into(), value });
        }
    };
    let name_of = |f: u32| {
        m.function_address(f)
            .and_then(|a| symbols.at(a))
            .map_or_else(|| format!("func[{f}]"), |s| s.display_name().into_owned())
    };
    prop("Version", "1".into());
    if let Some(n) = &m.names.module {
        prop("Module name", n.clone());
    }
    prop(
        "Functions",
        format!(
            "{} defined, {} imported",
            m.funcs.len() - m.imported_funcs as usize,
            m.imported_funcs
        ),
    );
    // Imports and exports: functions by name, the rest by kind.
    let by_kind = |kinds: Vec<(Kind, String)>| {
        let mut parts = Vec::new();
        for kind in [Kind::Func, Kind::Memory, Kind::Table, Kind::Global, Kind::Tag] {
            let names: Vec<String> = kinds
                .iter()
                .filter(|(k, _)| *k == kind)
                .map(|(_, n)| n.clone())
                .collect();
            if names.is_empty() {
                continue;
            }
            let label = match (kind, names.len()) {
                (Kind::Func, n) => plural(n, "function", "functions"),
                (Kind::Memory, n) => plural(n, "memory", "memories"),
                (k, n) => plural(n, k.name(), &format!("{}s", k.name())),
            };
            parts.push(format!("{label} ({})", some_of(&names, 8)));
        }
        parts.join("; ")
    };
    prop(
        "Imports",
        by_kind(
            m.imports
                .iter()
                .map(|i| (i.desc.kind(), format!("{}.{}", i.module, i.field)))
                .collect(),
        ),
    );
    prop(
        "Exports",
        by_kind(m.exports.iter().map(|e| (e.kind, e.name.clone())).collect()),
    );
    if let Some(s) = m.start {
        prop("Start function", format!("{} (function {s})", name_of(s)));
    }
    // The tables call_indirect goes through, and what each one holds.
    for (t, table) in m.tables.iter().enumerate() {
        let items: Vec<u32> = m
            .elements
            .iter()
            .filter(|e| matches!(e.mode, Mode::Active { index, .. } if index as usize == t))
            .flat_map(|e| e.items.iter().filter_map(|i| i.0))
            .collect();
        let names: Vec<String> = items.iter().map(|&f| name_of(f)).collect();
        let what = if items.is_empty() {
            "no functions placed by element segments".to_string()
        } else {
            format!(
                "{} reachable through call_indirect ({})",
                plural(items.len(), "function", "functions"),
                some_of(&names, 8)
            )
        };
        let size = match table.limits.max {
            Some(max) if max == table.limits.min => format!("{} entries", table.limits.min),
            Some(max) => format!("{} to {max} entries", table.limits.min),
            None => format!("{} entries or more", table.limits.min),
        };
        prop(&format!("Table {t}"), format!("{}, {size}: {what}", table.ty));
    }
    for (i, mem) in m.memories.iter().enumerate() {
        let origin = match mem.import.and_then(|i| m.imports.get(i as usize)) {
            Some(imp) => format!(", imported as {}.{}", imp.module, imp.field),
            None => String::new(),
        };
        prop(&format!("Memory {i}"), format!("{}{origin}", memory_text(&mem.limits)));
    }
    if !m.tags.is_empty() {
        let types: Vec<String> = m
            .tags
            .iter()
            .map(|t| match m.types.get(t.type_index as usize).and_then(Option::as_ref) {
                Some(ty) => ty.to_string(),
                None => format!("type {}", t.type_index),
            })
            .collect();
        prop("Exception tags", format!("{}: {}", m.tags.len(), some_of(&types, 8)));
    }
    if !m.data.is_empty() {
        let total: u64 = m.data.iter().map(|d| d.bytes.end - d.bytes.start).sum();
        let placed: Vec<String> = (0..m.data.len() as u32)
            .map(|i| match (m.segment_address(i), &m.data[i as usize].mode) {
                (Some(a), Mode::Passive) => format!("{} (passive) at {a:#x}", m.segment_name(i)),
                (Some(a), _) => format!("{} at {a:#x}", m.segment_name(i)),
                // In a memory binviz doesn't place: another one, or a 64-bit one.
                (None, Mode::Active { index, offset }) if offset.address().is_some() => {
                    format!("{} (in memory {index})", m.segment_name(i))
                }
                // A side module's, at `__memory_base`.
                (None, Mode::Active { .. }) => format!("{} (at a global's value)", m.segment_name(i)),
                (None, _) => format!("{} (passive)", m.segment_name(i)),
            })
            .collect();
        prop(
            "Data",
            format!(
                "{}, {total} bytes: {}",
                plural(m.data.len(), "segment", "segments"),
                some_of(&placed, 8)
            ),
        );
    }
    for (field, values) in &m.producers {
        let list: Vec<String> = values
            .iter()
            .map(|(n, v)| if v.is_empty() { n.clone() } else { format!("{n} {v}") })
            .collect();
        prop(&format!("Producers: {field}"), list.join(", "));
    }
    if !m.features.is_empty() {
        prop(
            "Target features",
            m.features
                .iter()
                .map(|(p, f)| format!("{p}{f}"))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    if let Some(u) = &m.source_map_url {
        prop("Source map", u.clone());
    }
    if let Some(u) = &m.external_debug_info {
        prop("External debug info", u.clone());
    }
    if m.is_object() {
        prop(
            "Object",
            format!(
                "a relocatable object: {} in its linking section",
                plural(m.symbols.len(), "symbol", "symbols")
            ),
        );
    }
    if !m.problems.is_empty() {
        prop("Problems", some_of(&m.problems, 4));
    }
    out
}

impl Binary {
    /// A WebAssembly module read into the model: sections (the data
    /// section as its segments, placed in memory), functions, globals and
    /// variables as symbols, imports and exports, the DWARF of its
    /// `.debug_*` sections at binviz's addresses, and the layout of every
    /// byte.
    pub(crate) fn from_wasm(data: Arc<[u8]>) -> Result<Binary> {
        let module = Module::parse(&data)?;
        let mut sections = sections(&module);
        let arch = if module.memories.first().is_some_and(|m| m.limits.is64) {
            Architecture::Wasm64
        } else {
            Architecture::Wasm32
        };
        let (debug, debug_note) = match dwarf::load(&data, &module, &module, &sections, arch, "embedded") {
            Ok(d) => (d, None),
            Err(e) => (None, Some(e.to_string())),
        };
        let variables = variables(&module, debug.as_ref());
        let vars_end = variables.iter().map(|v| v.0 + v.2.max(1)).max().unwrap_or(0);
        if let Some((name, start, end)) = module.zero_filled(vars_end) {
            sections.push(Section {
                index: sections.len() as u32,
                name: name.into(),
                segment_name: None,
                kind: RegionKind::Bss,
                address: start,
                size: end - start,
                file_offset: None,
                file_size: 0,
                align: 1,
                flags: String::new(),
                perms: "rw-".into(),
                compressed: false,
                segment: None,
                loaded: true,
            });
        }
        let symbols = symbols(&module, &sections, debug.as_ref(), None, &variables).finish_unindexed();
        let imports = module
            .imports
            .iter()
            .map(|i| Import {
                library: i.module.clone(),
                demangled: util::demangle(&i.field),
                name: i.field.clone(),
                ordinal: None,
                address: Some(i.entry.start),
            })
            .collect();
        let exports = module
            .exports
            .iter()
            .map(|e| {
                let address = match e.kind {
                    Kind::Func => module.function_address(e.index),
                    Kind::Global => module.global_address(e.index),
                    Kind::Memory => module.memories.get(e.index as usize).map(|m| m.entry.start),
                    Kind::Table => module.tables.get(e.index as usize).map(|t| t.entry.start),
                    Kind::Tag => module.tags.get(e.index as usize).map(|t| t.entry.start),
                };
                Export {
                    name: e.name.clone(),
                    demangled: util::demangle(&e.name),
                    address: address.unwrap_or(0),
                    ordinal: None,
                    forwarder: None,
                }
            })
            .collect();
        // Where it starts: the start function, else a WASI command's `_start`.
        let entry = module
            .start
            .or_else(|| {
                module
                    .exports
                    .iter()
                    .find(|e| e.kind == Kind::Func && e.name == "_start")
                    .map(|e| e.index)
            })
            .and_then(|f| module.function_address(f));
        let build_id = module.build_id.as_deref().map(util::hex_compact);
        let kind = if module.is_object() {
            "Relocatable object"
        } else if module.sections.iter().any(|s| s.name.starts_with("dylink")) {
            "Side module (shared library)"
        } else {
            "Module"
        };
        let mut summary = Summary {
            format: Format::Wasm,
            format_name: "WebAssembly".into(),
            kind: kind.into(),
            // As toolchains name the target: a 64-bit memory makes it wasm64.
            arch: if arch == Architecture::Wasm64 {
                "wasm64"
            } else {
                "wasm32"
            }
            .into(),
            bits: if arch == Architecture::Wasm64 { 64 } else { 32 },
            little_endian: true,
            file_size: data.len() as u64,
            entry,
            image_base: None,
            build_id: build_id.clone(),
            debug_link: module
                .external_debug_info
                .clone()
                .or_else(|| module.source_map_url.clone()),
            has_dwarf: debug.is_some(),
            has_symbols: !symbols.is_empty(),
            synthetic_addresses: false,
            section_count: sections.len() as u32,
            segment_count: 0,
            symbol_count: symbols.len() as u32,
            properties: Vec::new(),
            fingerprint: crate::binary::fingerprint(&data, build_id.as_deref()),
        };
        if let Some(note) = debug_note {
            summary.properties.push(Property {
                key: "DWARF".into(),
                value: format!("can't be read: {note}"),
            });
        }
        let mut binary = Binary {
            data: data.clone(),
            summary,
            sections,
            segments: Vec::new(),
            symbols,
            imports,
            exports,
            layout: crate::layout::Layout::empty(),
            machine: Machine::Other,
            arch,
            is64: arch == Architecture::Wasm64,
            endian: Endian::Little,
            image_base: 0,
            debug,
            discovered: Vec::new(),
            code_tables: Vec::new(),
            debug_symbols: Default::default(),
            annotations: Vec::new(),
            strings: std::sync::OnceLock::new(),
            coverage: std::sync::OnceLock::new(),
            xrefs: std::sync::OnceLock::new(),
            pointers: std::sync::OnceLock::new(),
            objc: std::sync::OnceLock::new(),
            similar: std::sync::OnceLock::new(),
            rom: None,
            wasm: Some(module),
        };
        binary.rebuild_static_symbols();
        let mut props = properties(binary.wasm.as_ref().expect("a module"), &binary.symbols);
        props.append(&mut binary.summary.properties);
        binary.summary.properties = props;
        binary.layout = binary.build_layout(Format::Wasm);
        Ok(binary)
    }

    /// The files a WebAssembly module says its debug info is in: its
    /// `external_debug_info` (DWARF, from `emcc -gseparate-dwarf`) and its
    /// `sourceMappingURL` (a source map, from `emcc -gsource-map`), as the
    /// module writes them (a path or a URL). Attach them with
    /// [`Binary::attach_debug_file`].
    pub fn wasm_debug_files(&self) -> Vec<String> {
        let Some(m) = &self.wasm else { return Vec::new() };
        [&m.external_debug_info, &m.source_map_url]
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }

    /// The WebAssembly function with this index (as `wasm-function[N]` in a
    /// stack trace names it): its address.
    pub fn wasm_function_address(&self, index: u32) -> Option<u64> {
        self.wasm.as_ref()?.function_address(index)
    }

    /// Attaches a WebAssembly module's separate debug info: a source map,
    /// or a module with DWARF (`emcc -gseparate-dwarf`'s `.debug.wasm`, or
    /// an unstripped build of the same code).
    pub(crate) fn attach_wasm_debug(&mut self, name: &str, data: &[u8]) -> Result<()> {
        let module = self.wasm.as_ref().expect("a WebAssembly module");
        if sourcemap::looks_like(data) {
            let debug = sourcemap::load(data, name, module, &self.symbols, &self.sections)?;
            self.summary.has_dwarf = true;
            self.debug = Some(debug);
            self.rebuild_static_symbols();
            return Ok(());
        }
        if !is_wasm(data) {
            crate::error::bail!(
                "{name} is neither a source map nor a WebAssembly module: a WebAssembly module's debug info is one of those"
            );
        }
        let other = Module::parse(data)?;
        // The same code, which its DWARF's addresses count from. A build ID
        // can't say as much: wasm-ld's hashes the whole module, so a relink
        // with --strip-debug has another. A file of DWARF alone needs the
        // same build ID.
        let code = |m: &Module, d: &'_ [u8]| {
            m.section(read::CODE)
                .and_then(|s| d.get(s.payload as usize..s.end as usize))
                .map(<[u8]>::to_vec)
        };
        match (code(&other, data), &other.build_id) {
            (Some(theirs), _) if Some(&theirs) == code(module, &self.data).as_ref() => {}
            (Some(_), _) => crate::error::bail!("{name} does not match this module (its code section differs)"),
            (None, Some(id)) if module.build_id.as_ref() == Some(id) => {}
            (None, _) => crate::error::bail!(
                "{name} has no code to compare with this module's, and not its build ID: it may not match"
            ),
        }
        let debug = dwarf::load(&Arc::from(data), &other, module, &self.sections, self.arch, name)?;
        if debug.is_none() && other.names.functions.is_empty() {
            crate::error::bail!("{name} has neither DWARF nor function names");
        }
        if let Some(debug) = debug {
            self.summary.has_dwarf = true;
            self.debug = Some(debug);
        }
        // The symbols again: its name section names what this module's
        // doesn't, and its DWARF the rest, variables too.
        let module = self.wasm.as_ref().expect("a WebAssembly module");
        let variables = variables(module, self.debug.as_ref());
        self.symbols = symbols(
            module,
            &self.sections,
            self.debug.as_ref(),
            Some(&other.names),
            &variables,
        )
        .finish_unindexed();
        self.rebuild_static_symbols();
        Ok(())
    }
}

/// What an import is, in a sentence fragment: `function (i32) -> ()`, `memory 2 pages …`.
pub(crate) fn import_text(m: &Module, desc: &ImportDesc) -> String {
    match desc {
        ImportDesc::Func(t) => match m.types.get(*t as usize).and_then(Option::as_ref) {
            Some(ty) => format!("function {ty}"),
            None => format!("function of type {t}"),
        },
        ImportDesc::Table(ty, l) => format!("table of {ty}, {} entries", l.min),
        ImportDesc::Memory(l) => format!("memory, {}", memory_text(l)),
        ImportDesc::Global(ty, mutable) => format!("{}global {ty}", if *mutable { "mutable " } else { "" }),
        ImportDesc::Tag(t) => format!("tag of type {t}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_module_as_a_binary() {
        let data = read::tests::sample();
        let bin = Binary::parse(data.clone()).unwrap();
        let s = bin.summary();
        assert_eq!(s.format, Format::Wasm);
        assert_eq!(s.arch, "wasm32");
        let names: Vec<&str> = bin.sections().iter().map(|s| s.name.as_str()).collect();
        // The data section is its segment, in memory. Nothing is zero-filled:
        // the stack pointer starts below the segment's end.
        assert_eq!(
            names,
            [
                "type", "import", "function", "table", "memory", "global", "export", "element", "code", "data[0]",
                "name"
            ]
        );
        let seg = &bin.sections()[9];
        assert_eq!((seg.address, seg.size, seg.loaded), (MEMORY_BASE + 1024, 3, true));
        assert_eq!(bin.address_to_offset(MEMORY_BASE + 1024), seg.file_offset);
        // Functions at their bodies, named by the name section, else by export, else numbered.
        let add = bin.symbols().by_name("add").unwrap();
        let m = bin.wasm.as_ref().unwrap();
        assert_eq!(add.address, m.funcs[1].body.unwrap().start);
        let log = bin.symbols().by_name("log").unwrap();
        assert_eq!(log.source, SymbolSource::Import);
        assert_eq!(log.address, m.imports[0].entry.start);
        assert!(bin.symbols().by_name("func[2]").is_some());
        assert_eq!(bin.imports()[0].library, "env");
        assert_eq!(bin.exports()[0].address, add.address);
        // Every byte laid out.
        let mut cursor = 0;
        for span in bin.spans(0, data.len() as u64) {
            assert_eq!(span.start, cursor);
            cursor = span.end;
        }
        assert_eq!(cursor, data.len() as u64);
        assert!(
            bin.composition().iter().all(|(k, _)| *k != RegionKind::Unknown),
            "{:?}",
            bin.composition()
        );
    }
}
