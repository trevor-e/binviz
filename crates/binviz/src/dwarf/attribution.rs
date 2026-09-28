//! Where each source file (or compilation unit) ended up in the binary: its
//! code, from the line tables, and its global variables, from DWARF.

use std::collections::{HashMap, HashSet};

use gimli::{AttributeValue, Operation, UnitOffset};
use serde::{Deserialize, Serialize};

use super::die::die_name;
use super::{DebugInfo, R};
use crate::binary::Binary;

/// A variable with a static address.
#[derive(Debug, Clone)]
pub(crate) struct GlobalVar {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub file: Option<u32>,
    pub line: u32,
    pub unit: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttributionMode {
    /// By the source file each line table row / variable declaration names.
    /// Inlined code counts toward the file it was written in (e.g. a header).
    File,
    /// By compilation unit (translation unit / codegen unit).
    Unit,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionShare {
    pub section: u32,
    pub code: u64,
    pub data: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contributor {
    /// Source file id or unit index, depending on the mode.
    pub id: u32,
    pub name: String,
    pub path: String,
    pub code: u64,
    pub data: u64,
    pub functions: u32,
    pub variables: u32,
    pub sections: Vec<SectionShare>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionTotal {
    pub section: u32,
    pub attributed: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribution {
    pub mode: AttributionMode,
    /// Largest first.
    pub contributors: Vec<Contributor>,
    pub sections: Vec<SectionTotal>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributedRange {
    pub start: u64,
    pub end: u64,
    pub section: Option<u32>,
    /// A variable rather than code.
    pub data: bool,
    /// Function or variable name.
    pub label: Option<String>,
    pub line: u32,
}

impl DebugInfo {
    pub(crate) fn globals(&self) -> &[GlobalVar] {
        self.globals.get_or_init(|| collect_globals(self))
    }

    /// Size of a type in bytes, following typedefs, qualifiers and arrays.
    pub(crate) fn type_size(&self, unit_idx: u32, offset: UnitOffset, depth: u32) -> Option<u64> {
        if depth > 16 {
            return None;
        }
        let unit = self.unit(unit_idx)?;
        let die = unit.entry(offset).ok()?;
        if let Some(size) = die.attr_value(gimli::DW_AT_byte_size).and_then(|v| v.udata_value()) {
            return Some(size);
        }
        let inner = || {
            die.attr_value(gimli::DW_AT_type)
                .and_then(|v| self.resolve_ref(unit_idx, v))
                .and_then(|(u, o)| self.type_size(u, o, depth + 1))
        };
        match die.tag() {
            gimli::DW_TAG_typedef
            | gimli::DW_TAG_const_type
            | gimli::DW_TAG_volatile_type
            | gimli::DW_TAG_restrict_type
            | gimli::DW_TAG_atomic_type => inner(),
            gimli::DW_TAG_pointer_type
            | gimli::DW_TAG_reference_type
            | gimli::DW_TAG_rvalue_reference_type
            | gimli::DW_TAG_ptr_to_member_type => Some(unit.header.address_size() as u64),
            gimli::DW_TAG_array_type => {
                let elem = inner()?;
                let mut count = 1u64;
                let mut cursor = unit.entries_at_offset(offset).ok()?;
                if !matches!(cursor.next_entry(), Ok(true)) {
                    return None;
                }
                if cursor.current()?.has_children() && matches!(cursor.next_entry(), Ok(true)) {
                    while let Some(child) = cursor.current() {
                        if child.tag() == gimli::DW_TAG_subrange_type {
                            let n = child
                                .attr_value(gimli::DW_AT_count)
                                .and_then(|v| v.udata_value())
                                .or_else(|| {
                                    child
                                        .attr_value(gimli::DW_AT_upper_bound)
                                        .and_then(|v| v.udata_value())
                                        .map(|u| u + 1)
                                })?;
                            count = count.saturating_mul(n);
                        }
                        if !matches!(cursor.next_sibling(), Ok(Some(_))) {
                            break;
                        }
                    }
                }
                Some(elem.saturating_mul(count))
            }
            _ => None,
        }
    }
}

/// The static address in a location expression (`DW_OP_addr` / `DW_OP_addrx`),
/// unless it is a thread-local offset.
pub(crate) fn static_address(unit: &gimli::UnitRef<'_, R>, expr: gimli::Expression<R>) -> Option<u64> {
    let mut ops = expr.operations(unit.encoding());
    let address = match ops.next().ok()?? {
        Operation::Address { address } => address,
        Operation::AddressIndex { index } => unit.address(index).ok()?,
        _ => return None,
    };
    while let Ok(Some(op)) = ops.next() {
        if matches!(op, Operation::TLS) {
            return None;
        }
    }
    Some(address)
}

fn collect_globals(debug: &DebugInfo) -> Vec<GlobalVar> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for ui in 0..debug.units.len() as u32 {
        let Some(unit) = debug.unit(ui) else { continue };
        let mut cursor = unit.entries();
        while let Ok(Some(die)) = cursor.next_dfs() {
            if die.tag() != gimli::DW_TAG_variable {
                continue;
            }
            let Some(AttributeValue::Exprloc(expr)) = die.attr_value(gimli::DW_AT_location) else {
                continue;
            };
            let Some(address) = static_address(&unit, expr) else {
                continue;
            };
            if address == 0 || !seen.insert(address) {
                continue;
            }
            // C++ out-of-class definitions keep the name and type on the declaration.
            let spec = match die.attr_value(gimli::DW_AT_specification) {
                Some(AttributeValue::UnitRef(o)) => unit.entry(o).ok(),
                _ => None,
            };
            let get = |at: gimli::DwAt| {
                die.attr_value(at)
                    .or_else(|| spec.as_ref().and_then(|s| s.attr_value(at)))
            };
            let size = get(gimli::DW_AT_type)
                .and_then(|v| debug.resolve_ref(ui, v))
                .and_then(|(u, o)| debug.type_size(u, o, 0))
                .unwrap_or(0);
            let file = match get(gimli::DW_AT_decl_file) {
                Some(AttributeValue::FileIndex(f)) => debug.files.unit_file(ui, f),
                _ => None,
            };
            let line = get(gimli::DW_AT_decl_line).and_then(|v| v.udata_value()).unwrap_or(0) as u32;
            let name = die_name(&unit, die, false)
                .or_else(|| spec.as_ref().and_then(|s| die_name(&unit, s, false)))
                .unwrap_or_else(|| format!("var_{address:x}"));
            out.push(GlobalVar {
                name,
                address,
                size,
                file,
                line,
                unit: ui,
            });
        }
    }
    out.sort_by_key(|g| g.address);
    out
}

/// Code ranges from the line tables, merged per key but never across a
/// function start (`starts`, sorted): (key, start, end, first line).
fn code_ranges(debug: &DebugInfo, mode: AttributionMode, starts: &[u64]) -> Vec<(u32, u64, u64, u32)> {
    let mut out: Vec<(u32, u64, u64, u32)> = Vec::new();
    for r in debug.rows().all() {
        let key = match mode {
            AttributionMode::File => r.file,
            AttributionMode::Unit => r.unit,
        };
        if key == u32::MAX {
            continue;
        }
        let end = r.start + r.len as u64;
        if let Some(last) = out.last_mut()
            && last.0 == key
            && last.2 == r.start
            && starts.binary_search(&r.start).is_err()
        {
            last.2 = end;
            continue;
        }
        out.push((key, r.start, end, r.line));
    }
    out
}

impl Binary {
    /// How the loaded sections break down by source file or compilation unit.
    pub fn attribution(&self, mode: AttributionMode) -> Option<Attribution> {
        let debug = self.debug.as_ref()?;
        #[derive(Default)]
        struct Acc {
            code: u64,
            data: u64,
            functions: HashSet<u32>,
            variables: u32,
            sections: HashMap<u32, (u64, u64)>,
        }
        let mut acc: HashMap<u32, Acc> = HashMap::new();
        let mut totals: HashMap<u32, u64> = HashMap::new();
        let sections = SectionLookup::new(&self.sections);
        for (key, start, end, _) in code_ranges(debug, mode, &self.function_starts()) {
            let Some(section) = sections.at(start) else { continue };
            let a = acc.entry(key).or_default();
            a.code += end - start;
            a.sections.entry(section.index).or_default().0 += end - start;
            *totals.entry(section.index).or_default() += end - start;
            if let Some(f) = self.symbols.lookup(start) {
                a.functions.insert(f.index);
            }
        }
        for g in debug.globals() {
            let key = match mode {
                AttributionMode::File => g.file,
                AttributionMode::Unit => Some(g.unit),
            };
            let (Some(key), Some(section)) = (key, sections.at(g.address)) else {
                continue;
            };
            let size = if g.size > 0 {
                g.size
            } else {
                self.symbols.at(g.address).map_or(1, |s| s.size.max(1))
            };
            let a = acc.entry(key).or_default();
            a.data += size;
            a.variables += 1;
            a.sections.entry(section.index).or_default().1 += size;
            *totals.entry(section.index).or_default() += size;
        }
        let mut contributors: Vec<Contributor> = acc
            .into_iter()
            .map(|(id, a)| {
                let (name, path) = self.contributor_name(debug, mode, id);
                let mut sections: Vec<SectionShare> = a
                    .sections
                    .into_iter()
                    .map(|(section, (code, data))| SectionShare { section, code, data })
                    .collect();
                sections.sort_by_key(|s| s.section);
                Contributor {
                    id,
                    name,
                    path,
                    code: a.code,
                    data: a.data,
                    functions: a.functions.len() as u32,
                    variables: a.variables,
                    sections,
                }
            })
            .collect();
        contributors.sort_by_key(|c| std::cmp::Reverse(c.code + c.data));
        let mut sections: Vec<SectionTotal> = totals
            .into_iter()
            .map(|(section, attributed)| SectionTotal { section, attributed })
            .collect();
        sections.sort_by_key(|s| s.section);
        Some(Attribution {
            mode,
            contributors,
            sections,
        })
    }

    /// Start addresses of every function, sorted.
    fn function_starts(&self) -> Vec<u64> {
        let mut v: Vec<u64> = self.symbols.functions().map(|s| s.address).collect();
        v.dedup();
        v
    }

    fn contributor_name(&self, debug: &DebugInfo, mode: AttributionMode, id: u32) -> (String, String) {
        match mode {
            AttributionMode::File => debug.source_files().get(id as usize).map_or_else(
                || (format!("file {id}"), String::new()),
                |f| (f.name.clone(), f.path.clone()),
            ),
            AttributionMode::Unit => {
                let u = debug.units().get(id as usize);
                let full = u.and_then(|u| u.name.clone()).unwrap_or_else(|| format!("unit {id}"));
                // rustc names units `path/lib.rs/@/crate.hash-cgu.N` (`\@\` on Windows).
                let (path, cgu) = match full.split_once("/@/").or_else(|| full.split_once("\\@\\")) {
                    Some((path, cgu)) => (path.to_string(), format!(" ({cgu})")),
                    None => (full.clone(), String::new()),
                };
                let base = path.rsplit(['/', '\\']).next().unwrap_or(&path).to_string();
                (format!("{base}{cgu}"), path)
            }
        }
    }

    /// The code and data ranges attributed to one file or unit.
    pub fn attributed_ranges(&self, mode: AttributionMode, id: u32) -> Vec<AttributedRange> {
        let Some(debug) = self.debug.as_ref() else {
            return Vec::new();
        };
        let sections = SectionLookup::new(&self.sections);
        let mut out = Vec::new();
        for (key, start, end, line) in code_ranges(debug, mode, &self.function_starts()) {
            if key != id {
                continue;
            }
            let label = self.symbols.lookup(start).map(|s| s.demangled.unwrap_or(s.name));
            out.push(AttributedRange {
                start,
                end,
                section: sections.at(start).map(|s| s.index),
                data: false,
                label,
                line,
            });
            if out.len() >= 50_000 {
                break;
            }
        }
        for g in debug.globals() {
            let key = match mode {
                AttributionMode::File => g.file,
                AttributionMode::Unit => Some(g.unit),
            };
            if key != Some(id) {
                continue;
            }
            let size = if g.size > 0 {
                g.size
            } else {
                self.symbols.at(g.address).map_or(1, |s| s.size.max(1))
            };
            out.push(AttributedRange {
                start: g.address,
                end: g.address + size,
                section: sections.at(g.address).map(|s| s.index),
                data: true,
                label: Some(g.name.clone()),
                line: g.line,
            });
        }
        out.sort_by_key(|r| r.start);
        out
    }
}

/// Loaded sections sorted by address, for many address lookups.
pub(crate) struct SectionLookup<'a> {
    sorted: Vec<&'a crate::model::Section>,
}

impl<'a> SectionLookup<'a> {
    pub(crate) fn new(sections: &'a [crate::model::Section]) -> Self {
        let mut sorted: Vec<&crate::model::Section> = sections.iter().filter(|s| s.loaded).collect();
        sorted.sort_by_key(|s| s.address);
        SectionLookup { sorted }
    }

    pub(crate) fn at(&self, address: u64) -> Option<&'a crate::model::Section> {
        let idx = self.sorted.partition_point(|s| s.address <= address);
        // Sections may overlap (e.g. .tbss); look back a little.
        self.sorted[..idx]
            .iter()
            .rev()
            .take(4)
            .find(|s| address - s.address < s.size.max(1))
            .copied()
    }
}
