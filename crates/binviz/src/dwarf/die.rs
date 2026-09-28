//! Browsing DIE trees and rendering attributes.

use gimli::{AttributeValue, Reader, UnitOffset};
use serde::Serialize;

use super::{DebugInfo, R, expr};
use crate::model::SourceLoc;
use crate::util;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DieSummary {
    pub unit: u32,
    /// Offset within the unit.
    pub offset: u64,
    /// Offset within the section (.debug_info or .debug_types).
    pub section_offset: u64,
    pub tag: String,
    pub name: Option<String>,
    pub has_children: bool,
    /// Short description: type, address range, size...
    pub detail: Option<String>,
    pub low_pc: Option<u64>,
    pub high_pc: Option<u64>,
}

/// Something an attribute value points at.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Link {
    Die { unit: u32, offset: u64 },
    Address { address: u64 },
    Source { file: u32, line: u32 },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttrInfo {
    pub name: String,
    pub form: String,
    pub value: String,
    pub link: Option<Link>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DieDetails {
    pub die: DieSummary,
    pub attributes: Vec<AttrInfo>,
    /// Ancestors from the unit root down to the parent.
    pub parents: Vec<DieSummary>,
    pub ranges: Vec<[u64; 2]>,
    pub type_name: Option<String>,
    pub decl: Option<SourceLoc>,
    /// Section offsets of the DIE's encoded bytes.
    pub byte_start: u64,
    pub byte_end: u64,
    pub section: String,
}

type Die = gimli::DebuggingInformationEntry<R>;

fn tag_name(tag: gimli::DwTag) -> String {
    tag.static_string()
        .map_or_else(|| format!("DW_TAG_{:#x}", tag.0), str::to_string)
}

fn to_string(r: R) -> String {
    r.to_string_lossy().map(|c| c.into_owned()).unwrap_or_default()
}

/// A DIE's name, following `DW_AT_abstract_origin` / `DW_AT_specification`.
/// With `linkage`, prefers the (demangled) linkage name, which is fully qualified.
pub(crate) fn die_name(unit: &gimli::UnitRef<'_, R>, die: &Die, linkage: bool) -> Option<String> {
    let mut current = die.clone();
    for _ in 0..4 {
        if linkage {
            for at in [gimli::DW_AT_linkage_name, gimli::DW_AT_MIPS_linkage_name] {
                if let Some(v) = current.attr_value(at)
                    && let Ok(s) = unit.attr_string(v)
                {
                    let raw = to_string(s);
                    return Some(util::demangle(&raw).unwrap_or(raw));
                }
            }
        }
        if let Some(v) = current.attr_value(gimli::DW_AT_name)
            && let Ok(s) = unit.attr_string(v)
        {
            return Some(to_string(s));
        }
        let next = current
            .attr_value(gimli::DW_AT_abstract_origin)
            .or_else(|| current.attr_value(gimli::DW_AT_specification));
        match next {
            Some(AttributeValue::UnitRef(o)) => current = unit.entry(o).ok()?,
            _ => return None,
        }
    }
    None
}

impl DebugInfo {
    pub(crate) fn resolve_ref(&self, unit: u32, value: AttributeValue<R>) -> Option<(u32, UnitOffset)> {
        match value {
            AttributeValue::UnitRef(o) => Some((unit, o)),
            AttributeValue::DebugInfoRef(o) => {
                let u = self.unit_for_info_offset(o.0 as u64)?;
                Some((u, o.to_unit_offset(&self.units[u as usize].header)?))
            }
            AttributeValue::DebugTypesRef(sig) => {
                self.units.iter().enumerate().find_map(|(i, u)| match u.header.type_() {
                    gimli::UnitType::Type {
                        type_signature,
                        type_offset,
                    }
                    | gimli::UnitType::SplitType {
                        type_signature,
                        type_offset,
                    } if type_signature == sig => Some((i as u32, type_offset)),
                    _ => None,
                })
            }
            _ => None,
        }
    }

    fn summary_of(&self, unit_idx: u32, unit: &gimli::UnitRef<'_, R>, die: &Die) -> DieSummary {
        let section_offset = die.offset().to_unit_section_offset(&unit.header).0 as u64;
        let (low, high) = pc_range(unit, die);
        let tag = die.tag();
        let detail = match tag {
            gimli::DW_TAG_compile_unit | gimli::DW_TAG_partial_unit | gimli::DW_TAG_type_unit => {
                unit.comp_dir.clone().map(to_string)
            }
            gimli::DW_TAG_subprogram | gimli::DW_TAG_inlined_subroutine | gimli::DW_TAG_lexical_block => {
                match (low, high) {
                    (Some(l), Some(h)) => Some(format!("{l:#x}..{h:#x}")),
                    _ if die.attr(gimli::DW_AT_ranges).is_some() => Some("multiple ranges".into()),
                    _ if die.attr_value(gimli::DW_AT_inline).is_some() => Some("inline (abstract)".into()),
                    _ if die.attr_value(gimli::DW_AT_declaration).is_some() => Some("declaration".into()),
                    _ => None,
                }
            }
            gimli::DW_TAG_variable | gimli::DW_TAG_formal_parameter | gimli::DW_TAG_member | gimli::DW_TAG_typedef => {
                die.attr_value(gimli::DW_AT_type)
                    .and_then(|v| self.resolve_ref(unit_idx, v))
                    .map(|(u, o)| self.type_name(u, o, 0))
            }
            gimli::DW_TAG_structure_type
            | gimli::DW_TAG_class_type
            | gimli::DW_TAG_union_type
            | gimli::DW_TAG_enumeration_type
            | gimli::DW_TAG_base_type => die
                .attr_value(gimli::DW_AT_byte_size)
                .and_then(|v| v.udata_value())
                .map(|s| format!("{s} bytes")),
            gimli::DW_TAG_pointer_type
            | gimli::DW_TAG_const_type
            | gimli::DW_TAG_array_type
            | gimli::DW_TAG_reference_type => Some(self.type_name(unit_idx, die.offset(), 0)),
            gimli::DW_TAG_enumerator => die.attr_value(gimli::DW_AT_const_value).map(|v| match v {
                AttributeValue::Sdata(s) => s.to_string(),
                other => other.udata_value().map_or_else(String::new, |u| u.to_string()),
            }),
            _ => None,
        };
        DieSummary {
            unit: unit_idx,
            offset: die.offset().0 as u64,
            section_offset,
            tag: tag_name(tag),
            name: die_name(unit, die, false),
            has_children: die.has_children(),
            detail,
            low_pc: low,
            high_pc: high,
        }
    }

    /// Children of a DIE (or of the unit root when `offset` is None).
    pub fn die_children(&self, unit_idx: u32, offset: Option<u64>) -> Vec<DieSummary> {
        let Some(unit) = self.unit(unit_idx) else {
            return Vec::new();
        };
        let offset = offset.map_or_else(|| unit.header.root_offset(), |o| UnitOffset(o as usize));
        let Ok(mut cursor) = unit.entries_at_offset(offset) else {
            return Vec::new();
        };
        if !matches!(cursor.next_entry(), Ok(true)) {
            return Vec::new();
        }
        if !cursor.current().is_some_and(|d| d.has_children()) {
            return Vec::new();
        }
        let mut out = Vec::new();
        if cursor.next_entry().is_err() {
            return out;
        }
        while let Some(child) = cursor.current() {
            out.push(self.summary_of(unit_idx, &unit, child));
            if out.len() >= 50_000 || !matches!(cursor.next_sibling(), Ok(Some(_))) {
                break;
            }
        }
        out
    }

    /// The root DIE of a unit.
    pub fn unit_root(&self, unit_idx: u32) -> Option<DieSummary> {
        let unit = self.unit(unit_idx)?;
        let die = unit.entry(unit.header.root_offset()).ok()?;
        Some(self.summary_of(unit_idx, &unit, &die))
    }

    /// Ancestors of a DIE, from the root down (found by a walk over the unit).
    fn parents(&self, unit_idx: u32, target: UnitOffset) -> Vec<DieSummary> {
        let Some(unit) = self.unit(unit_idx) else {
            return Vec::new();
        };
        let mut stack: Vec<(isize, UnitOffset)> = Vec::new();
        let mut cursor = unit.entries();
        while let Ok(Some(die)) = cursor.next_dfs() {
            let depth = die.depth();
            while stack.last().is_some_and(|&(d, _)| d >= depth) {
                stack.pop();
            }
            if die.offset() == target {
                return stack
                    .iter()
                    .filter_map(|&(_, o)| unit.entry(o).ok().map(|d| self.summary_of(unit_idx, &unit, &d)))
                    .collect();
            }
            if die.offset() > target {
                break;
            }
            if die.has_children() {
                stack.push((depth, die.offset()));
            }
        }
        Vec::new()
    }

    pub fn die(&self, unit_idx: u32, offset: u64) -> Option<DieDetails> {
        let unit = self.unit(unit_idx)?;
        let off = UnitOffset(offset as usize);
        let die = unit.entry(off).ok()?;
        let summary = self.summary_of(unit_idx, &unit, &die);
        // The DIE's bytes end where the next entry starts.
        let mut cursor = unit.entries_at_offset(off).ok()?;
        cursor.next_entry().ok()?;
        let next = cursor.next_offset();
        let byte_start = off.to_unit_section_offset(&unit.header).0 as u64;
        let byte_end = next.to_unit_section_offset(&unit.header).0 as u64;

        let low_pc = pc_range(&unit, &die).0;
        let attributes = die
            .attrs()
            .iter()
            .map(|a| self.format_attr(unit_idx, &unit, a, low_pc))
            .collect();
        let mut ranges = Vec::new();
        if let Ok(mut iter) = unit.die_ranges(&die) {
            while let Ok(Some(r)) = iter.next() {
                if ranges.len() >= 256 {
                    break;
                }
                ranges.push([r.begin, r.end]);
            }
        }
        let type_name = die
            .attr_value(gimli::DW_AT_type)
            .and_then(|v| self.resolve_ref(unit_idx, v))
            .map(|(u, o)| self.type_name(u, o, 0));
        let decl = match (
            die.attr_value(gimli::DW_AT_decl_file),
            die.attr_value(gimli::DW_AT_decl_line),
        ) {
            (Some(AttributeValue::FileIndex(f)), line) => self.files.unit_file(unit_idx, f).map(|file| SourceLoc {
                file,
                path: self.files.files[file as usize].path.clone(),
                line: line.and_then(|l| l.udata_value()).unwrap_or(0) as u32,
                column: die
                    .attr_value(gimli::DW_AT_decl_column)
                    .and_then(|c| c.udata_value())
                    .unwrap_or(0) as u32,
            }),
            _ => None,
        };
        Some(DieDetails {
            die: summary,
            attributes,
            parents: self.parents(unit_idx, off),
            ranges,
            type_name,
            decl,
            byte_start,
            byte_end,
            section: unit.header.section().name().to_string(),
        })
    }

    pub(crate) fn format_attr(
        &self,
        unit_idx: u32,
        unit: &gimli::UnitRef<'_, R>,
        attr: &gimli::Attribute<R>,
        low_pc: Option<u64>,
    ) -> AttrInfo {
        let name = attr
            .name()
            .static_string()
            .map_or_else(|| format!("DW_AT_{:#x}", attr.name().0), str::to_string);
        let form = attr
            .form()
            .static_string()
            .map_or_else(|| format!("{:#x}", attr.form().0), str::to_string);
        let mut link = None;
        let value = match attr.value() {
            AttributeValue::Addr(a) => {
                link = Some(Link::Address { address: a });
                format!("{a:#x}")
            }
            AttributeValue::DebugAddrIndex(i) => match unit.address(i) {
                Ok(a) => {
                    link = Some(Link::Address { address: a });
                    format!("{a:#x} (index {})", i.0)
                }
                Err(e) => format!("index {} <{e}>", i.0),
            },
            v @ (AttributeValue::Data1(_)
            | AttributeValue::Data2(_)
            | AttributeValue::Data4(_)
            | AttributeValue::Data8(_)
            | AttributeValue::Udata(_))
                if attr.name() == gimli::DW_AT_high_pc =>
            {
                let n = v.udata_value().unwrap_or(0);
                match low_pc {
                    Some(l) => {
                        link = Some(Link::Address { address: l + n });
                        format!("{:#x} (low_pc + {n:#x})", l + n)
                    }
                    None => format!("+{n:#x}"),
                }
            }
            AttributeValue::Data1(v) => format!("{v} ({v:#x})"),
            AttributeValue::Data2(v) => format!("{v} ({v:#x})"),
            AttributeValue::Data4(v) => format!("{v} ({v:#x})"),
            AttributeValue::Data8(v) => format!("{v} ({v:#x})"),
            AttributeValue::Data16(v) => format!("{v:#x}"),
            AttributeValue::Sdata(v) => v.to_string(),
            AttributeValue::Udata(v) => {
                if attr.name() == gimli::DW_AT_decl_line || attr.name() == gimli::DW_AT_call_line {
                    v.to_string()
                } else {
                    format!("{v} ({v:#x})")
                }
            }
            AttributeValue::Block(b) => format!(
                "[{} bytes] {}",
                b.len(),
                util::hex_bytes(&b.to_slice().map(|c| c.into_owned()).unwrap_or_default()[..b.len().min(32)])
            ),
            AttributeValue::Exprloc(e) => expr::format(&e, unit.encoding(), self.arch),
            AttributeValue::Flag(f) => f.to_string(),
            AttributeValue::SecOffset(o) => format!("{o:#x}"),
            v @ (AttributeValue::UnitRef(_) | AttributeValue::DebugInfoRef(_) | AttributeValue::DebugTypesRef(_)) => {
                match self.resolve_ref(unit_idx, v.clone()) {
                    Some((u, o)) => {
                        let target = self.unit(u).and_then(|tu| tu.entry(o).ok().map(|d| (tu, d)));
                        let global = self.units[u as usize].header.offset().0 + o.0;
                        link = Some(Link::Die {
                            unit: u,
                            offset: o.0 as u64,
                        });
                        match target {
                            Some((tu, d)) => {
                                let label = if is_type_tag(d.tag()) {
                                    self.type_name(u, o, 0)
                                } else {
                                    die_name(&tu, &d, false).unwrap_or_else(|| tag_name(d.tag()))
                                };
                                format!("<{global:#x}> {label}")
                            }
                            None => format!("<{global:#x}>"),
                        }
                    }
                    None => format!("{v:?}"),
                }
            }
            AttributeValue::DebugLineRef(o) => format!("line program at {:#x}", o.0),
            v @ (AttributeValue::LocationListsRef(_) | AttributeValue::DebugLocListsIndex(_)) => {
                match unit.attr_locations(v) {
                    Ok(Some(mut iter)) => {
                        let mut parts = Vec::new();
                        while let Ok(Some(e)) = iter.next() {
                            if parts.len() >= 32 {
                                parts.push("…".to_string());
                                break;
                            }
                            parts.push(format!(
                                "[{:#x}, {:#x}): {}",
                                e.range.begin,
                                e.range.end,
                                expr::format(&e.data, unit.encoding(), self.arch)
                            ));
                        }
                        if parts.is_empty() {
                            "(empty location list)".into()
                        } else {
                            parts.join("\n")
                        }
                    }
                    Ok(None) => String::new(),
                    Err(e) => format!("<{e}>"),
                }
            }
            v @ (AttributeValue::RangeListsRef(_) | AttributeValue::DebugRngListsIndex(_)) => match unit.attr_ranges(v)
            {
                Ok(Some(mut iter)) => {
                    let mut parts = Vec::new();
                    while let Ok(Some(r)) = iter.next() {
                        if parts.len() >= 32 {
                            parts.push("…".to_string());
                            break;
                        }
                        if link.is_none() {
                            link = Some(Link::Address { address: r.begin });
                        }
                        parts.push(format!("[{:#x}, {:#x})", r.begin, r.end));
                    }
                    parts.join(" ")
                }
                Ok(None) => String::new(),
                Err(e) => format!("<{e}>"),
            },
            v @ (AttributeValue::String(_)
            | AttributeValue::DebugStrRef(_)
            | AttributeValue::DebugStrRefSup(_)
            | AttributeValue::DebugStrOffsetsIndex(_)
            | AttributeValue::DebugLineStrRef(_)) => match unit.attr_string(v) {
                Ok(s) => {
                    let s = to_string(s);
                    if matches!(attr.name(), gimli::DW_AT_linkage_name | gimli::DW_AT_MIPS_linkage_name)
                        && let Some(d) = util::demangle(&s)
                    {
                        format!("{} → {d}", util::quote(s.as_bytes()))
                    } else {
                        util::quote(s.as_bytes())
                    }
                }
                Err(e) => format!("<{e}>"),
            },
            AttributeValue::Encoding(v) => v.static_string().unwrap_or("?").to_string(),
            AttributeValue::DecimalSign(v) => v.static_string().unwrap_or("?").to_string(),
            AttributeValue::Endianity(v) => v.static_string().unwrap_or("?").to_string(),
            AttributeValue::Accessibility(v) => v.static_string().unwrap_or("?").to_string(),
            AttributeValue::Visibility(v) => v.static_string().unwrap_or("?").to_string(),
            AttributeValue::Virtuality(v) => v.static_string().unwrap_or("?").to_string(),
            AttributeValue::Language(v) => v.static_string().unwrap_or("?").to_string(),
            AttributeValue::AddressClass(v) => v.static_string().unwrap_or("?").to_string(),
            AttributeValue::IdentifierCase(v) => v.static_string().unwrap_or("?").to_string(),
            AttributeValue::CallingConvention(v) => v.static_string().unwrap_or("?").to_string(),
            AttributeValue::Inline(v) => v.static_string().unwrap_or("?").to_string(),
            AttributeValue::Ordering(v) => v.static_string().unwrap_or("?").to_string(),
            AttributeValue::FileIndex(i) => match self.files.unit_file(unit_idx, i) {
                Some(f) => {
                    link = Some(Link::Source { file: f, line: 0 });
                    format!("{i}: {}", self.files.files[f as usize].path)
                }
                None => i.to_string(),
            },
            AttributeValue::DwoId(id) => format!("{:#018x}", id.0),
            other => format!("{other:?}"),
        };
        AttrInfo {
            name,
            form,
            value,
            link,
        }
    }

    /// A C-like rendering of the type at `offset`.
    pub(crate) fn type_name(&self, unit_idx: u32, offset: UnitOffset, depth: u32) -> String {
        if depth > 8 {
            return "…".into();
        }
        let Some(unit) = self.unit(unit_idx) else {
            return "?".into();
        };
        let Ok(die) = unit.entry(offset) else { return "?".into() };
        let name = die
            .attr_value(gimli::DW_AT_name)
            .and_then(|v| unit.attr_string(v).ok())
            .map(to_string);
        let inner = || match die
            .attr_value(gimli::DW_AT_type)
            .and_then(|v| self.resolve_ref(unit_idx, v))
        {
            Some((u, o)) => self.type_name(u, o, depth + 1),
            None => "void".to_string(),
        };
        match die.tag() {
            gimli::DW_TAG_pointer_type => name.unwrap_or_else(|| format!("{} *", inner())),
            gimli::DW_TAG_reference_type => name.unwrap_or_else(|| format!("{} &", inner())),
            gimli::DW_TAG_rvalue_reference_type => name.unwrap_or_else(|| format!("{} &&", inner())),
            gimli::DW_TAG_const_type => format!("const {}", inner()),
            gimli::DW_TAG_volatile_type => format!("volatile {}", inner()),
            gimli::DW_TAG_restrict_type => format!("{} restrict", inner()),
            gimli::DW_TAG_atomic_type => format!("_Atomic {}", inner()),
            gimli::DW_TAG_ptr_to_member_type => format!("{} ::*", inner()),
            gimli::DW_TAG_array_type => {
                if let Some(n) = name {
                    return n;
                }
                let mut dims = String::new();
                if let Ok(mut cursor) = unit.entries_at_offset(offset) {
                    let _ = cursor.next_entry();
                    if cursor.current().is_some_and(|d| d.has_children()) && cursor.next_entry().is_ok() {
                        while let Some(child) = cursor.current() {
                            if child.tag() == gimli::DW_TAG_subrange_type {
                                let count = child
                                    .attr_value(gimli::DW_AT_count)
                                    .and_then(|v| v.udata_value())
                                    .or_else(|| {
                                        child
                                            .attr_value(gimli::DW_AT_upper_bound)
                                            .and_then(|v| v.udata_value())
                                            .map(|u| u + 1)
                                    });
                                dims.push_str(&match count {
                                    Some(c) => format!("[{c}]"),
                                    None => "[]".into(),
                                });
                            }
                            if !matches!(cursor.next_sibling(), Ok(Some(_))) {
                                break;
                            }
                        }
                    }
                }
                format!("{}{}", inner(), if dims.is_empty() { "[]".into() } else { dims })
            }
            gimli::DW_TAG_subroutine_type => {
                let ret = inner();
                let mut params = Vec::new();
                if let Ok(mut cursor) = unit.entries_at_offset(offset) {
                    let _ = cursor.next_entry();
                    if cursor.current().is_some_and(|d| d.has_children()) && cursor.next_entry().is_ok() {
                        while let Some(child) = cursor.current() {
                            if child.tag() == gimli::DW_TAG_formal_parameter {
                                params.push(
                                    match child
                                        .attr_value(gimli::DW_AT_type)
                                        .and_then(|v| self.resolve_ref(unit_idx, v))
                                    {
                                        Some((u, o)) => self.type_name(u, o, depth + 1),
                                        None => "?".into(),
                                    },
                                );
                            } else if child.tag() == gimli::DW_TAG_unspecified_parameters {
                                params.push("...".into());
                            }
                            if !matches!(cursor.next_sibling(), Ok(Some(_))) {
                                break;
                            }
                        }
                    }
                }
                format!("{ret} (*)({})", params.join(", "))
            }
            gimli::DW_TAG_structure_type => name.unwrap_or_else(|| "struct <anonymous>".into()),
            gimli::DW_TAG_class_type => name.unwrap_or_else(|| "class <anonymous>".into()),
            gimli::DW_TAG_union_type => name.unwrap_or_else(|| "union <anonymous>".into()),
            gimli::DW_TAG_enumeration_type => name.unwrap_or_else(|| "enum <anonymous>".into()),
            _ => name.unwrap_or_else(|| tag_name(die.tag())),
        }
    }

    /// The address range of the function (DW_TAG_subprogram) covering `address`.
    pub fn function_range(&self, address: u64) -> Option<(u64, u64)> {
        let (unit_idx, offset) = self.function_die_at(address)?;
        let unit = self.unit(unit_idx)?;
        let die = unit.entry(UnitOffset(offset as usize)).ok()?;
        let mut iter = unit.die_ranges(&die).ok()?;
        while let Ok(Some(r)) = iter.next() {
            if address >= r.begin && address < r.end {
                return Some((r.begin, r.end));
            }
        }
        None
    }

    /// The DW_TAG_subprogram covering `address` (the function, not an inlined callee).
    pub fn function_die_at(&self, address: u64) -> Option<(u32, u64)> {
        let unit_idx = self.unit_at(address)?;
        let unit = self.unit(unit_idx)?;
        let mut cursor = unit.entries();
        while let Ok(Some(die)) = cursor.next_dfs() {
            if die.tag() != gimli::DW_TAG_subprogram {
                continue;
            }
            if let Ok(mut iter) = unit.die_ranges(die) {
                while let Ok(Some(r)) = iter.next() {
                    if address >= r.begin && address < r.end {
                        return Some((unit_idx, die.offset().0 as u64));
                    }
                }
            }
        }
        None
    }

    /// The innermost DIE whose address ranges cover `address` (subprogram,
    /// inlined subroutine or lexical block), for jumping from code to DWARF.
    pub fn die_at(&self, address: u64) -> Option<(u32, u64)> {
        let unit_idx = self.unit_at(address)?;
        let unit = self.unit(unit_idx)?;
        let mut best = None;
        let mut cursor = unit.entries();
        let mut skip_below: Option<isize> = None;
        while let Ok(Some(die)) = cursor.next_dfs() {
            let depth = die.depth();
            if let Some(d) = skip_below {
                if depth > d {
                    continue;
                }
                skip_below = None;
            }
            if !matches!(
                die.tag(),
                gimli::DW_TAG_subprogram | gimli::DW_TAG_inlined_subroutine | gimli::DW_TAG_lexical_block
            ) {
                continue;
            }
            let mut covered = false;
            let mut has_ranges = false;
            if let Ok(mut iter) = unit.die_ranges(die) {
                while let Ok(Some(r)) = iter.next() {
                    has_ranges = true;
                    if address >= r.begin && address < r.end {
                        covered = true;
                        break;
                    }
                }
            }
            if covered {
                best = Some((unit_idx, die.offset().0 as u64));
            } else if has_ranges && die.has_children() {
                // Nothing below a non-matching scope can match.
                skip_below = Some(depth);
            }
        }
        best
    }

    /// Case-insensitive name search over all DIEs.
    pub fn search(&self, query: &str, limit: usize) -> Vec<DieSummary> {
        let needle = query.to_lowercase();
        let mut out = Vec::new();
        for i in 0..self.units.len() {
            let unit = self.units[i].unit_ref(&self.dwarf);
            let mut cursor = unit.entries();
            while let Ok(Some(die)) = cursor.next_dfs() {
                let Some(v) = die.attr_value(gimli::DW_AT_name) else {
                    continue;
                };
                let Ok(s) = unit.attr_string(v) else { continue };
                let s = to_string(s);
                if s.to_lowercase().contains(&needle) {
                    out.push(self.summary_of(i as u32, &unit, die));
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
        }
        out
    }
}

fn is_type_tag(tag: gimli::DwTag) -> bool {
    matches!(
        tag,
        gimli::DW_TAG_base_type
            | gimli::DW_TAG_pointer_type
            | gimli::DW_TAG_reference_type
            | gimli::DW_TAG_rvalue_reference_type
            | gimli::DW_TAG_const_type
            | gimli::DW_TAG_volatile_type
            | gimli::DW_TAG_restrict_type
            | gimli::DW_TAG_atomic_type
            | gimli::DW_TAG_array_type
            | gimli::DW_TAG_structure_type
            | gimli::DW_TAG_class_type
            | gimli::DW_TAG_union_type
            | gimli::DW_TAG_enumeration_type
            | gimli::DW_TAG_typedef
            | gimli::DW_TAG_subroutine_type
            | gimli::DW_TAG_ptr_to_member_type
            | gimli::DW_TAG_unspecified_type
    )
}

/// (low_pc, high_pc) of a DIE that has a contiguous range.
fn pc_range(unit: &gimli::UnitRef<'_, R>, die: &Die) -> (Option<u64>, Option<u64>) {
    let low = die
        .attr_value(gimli::DW_AT_low_pc)
        .and_then(|v| unit.attr_address(v).ok().flatten());
    let high = match (low, die.attr_value(gimli::DW_AT_high_pc)) {
        (_, Some(AttributeValue::Addr(a))) => Some(a),
        (_, Some(v @ AttributeValue::DebugAddrIndex(_))) => unit.attr_address(v).ok().flatten(),
        (Some(l), Some(v)) => v.udata_value().map(|n| l + n),
        _ => None,
    };
    (low, high)
}

/// A named DIE, for search.
pub(crate) struct NamedDie {
    /// Qualified with enclosing namespaces and types (`ns::Type::member`);
    /// functions use their demangled linkage name when they have one.
    pub name: String,
    pub unit: u32,
    pub offset: u64,
    pub tag: gimli::DwTag,
    /// Entry point of a function, or address of a static variable.
    pub address: Option<u64>,
}

/// Functions with code, static variables, types, members and enumerators by
/// qualified name, one per (name, tag, address).
pub(crate) fn build_name_index(debug: &DebugInfo) -> Vec<NamedDie> {
    let mut out: Vec<NamedDie> = Vec::new();
    let mut seen: std::collections::HashMap<(String, u16, Option<u64>), usize> = std::collections::HashMap::new();
    for (ui, u) in debug.units.iter().enumerate() {
        let unit = u.unit_ref(&debug.dwarf);
        // Enclosing named scopes: (depth, name).
        let mut scopes: Vec<(isize, String)> = Vec::new();
        let mut cursor = unit.entries();
        while let Ok(Some(die)) = cursor.next_dfs() {
            let depth = die.depth();
            while scopes.last().is_some_and(|&(d, _)| d >= depth) {
                scopes.pop();
            }
            let tag = die.tag();
            let scope_tag = matches!(
                tag,
                gimli::DW_TAG_namespace
                    | gimli::DW_TAG_structure_type
                    | gimli::DW_TAG_class_type
                    | gimli::DW_TAG_union_type
                    | gimli::DW_TAG_enumeration_type
            );
            let own_name = die
                .attr_value(gimli::DW_AT_name)
                .and_then(|v| unit.attr_string(v).ok())
                .map(to_string);
            let qualify = |name: &str| {
                let mut q = String::new();
                for (_, s) in &scopes {
                    q.push_str(s);
                    q.push_str("::");
                }
                q.push_str(name);
                q
            };
            let entry = match tag {
                gimli::DW_TAG_subprogram => {
                    // Only concrete functions (not declarations or abstract inline instances).
                    let address = pc_range(&unit, die).0.filter(|&a| a != 0).or_else(|| {
                        let mut r = unit.die_ranges(die).ok()?;
                        r.next().ok().flatten().map(|r| r.begin).filter(|&a| a != 0)
                    });
                    address.and_then(|a| {
                        let name = die_name(&unit, die, true)?;
                        let name = if name.contains("::") || scopes.is_empty() {
                            name
                        } else {
                            qualify(&name)
                        };
                        Some((name, Some(a)))
                    })
                }
                gimli::DW_TAG_variable => {
                    let address = match die.attr_value(gimli::DW_AT_location) {
                        Some(AttributeValue::Exprloc(e)) => super::attribution::static_address(&unit, e),
                        _ => None,
                    };
                    address.filter(|&a| a != 0).and_then(|a| {
                        let name = die_name(&unit, die, true)?;
                        let name = if name.contains("::") || scopes.is_empty() {
                            name
                        } else {
                            qualify(&name)
                        };
                        Some((name, Some(a)))
                    })
                }
                gimli::DW_TAG_structure_type
                | gimli::DW_TAG_class_type
                | gimli::DW_TAG_union_type
                | gimli::DW_TAG_enumeration_type
                | gimli::DW_TAG_typedef
                | gimli::DW_TAG_base_type
                | gimli::DW_TAG_member
                | gimli::DW_TAG_enumerator => own_name.as_deref().map(|n| (qualify(n), None)),
                _ => None,
            };
            if scope_tag && die.has_children() {
                scopes.push((depth, own_name.clone().unwrap_or_else(|| "{anon}".into())));
            }
            let Some((name, address)) = entry else { continue };
            if name.is_empty() {
                continue;
            }
            let declaration = die.attr_value(gimli::DW_AT_declaration).is_some();
            let key = (name.clone(), tag.0, address);
            match seen.get(&key) {
                // Prefer a complete type over a forward declaration.
                Some(&i) => {
                    if !declaration && out[i].address.is_none() && is_type_tag(tag) {
                        out[i].unit = ui as u32;
                        out[i].offset = die.offset().0 as u64;
                    }
                    continue;
                }
                None => {
                    seen.insert(key, out.len());
                }
            }
            out.push(NamedDie {
                name,
                unit: ui as u32,
                offset: die.offset().0 as u64,
                tag,
                address,
            });
            if out.len() >= 2_000_000 {
                return out;
            }
        }
    }
    out
}
