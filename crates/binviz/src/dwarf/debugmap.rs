//! Mach-O debug maps. An executable linked without dsymutil carries no DWARF
//! of its own: the DWARF stays in the object files it was linked from, and
//! STABS entries in its symbol table (the debug map) name those objects and
//! say where each of their functions and static variables ended up. Moving
//! the objects' DWARF to the executable's addresses is what dsymutil does,
//! and what this does too: every DIE, range list, location list and line
//! table row goes where its code went, and what the linker dead-stripped is
//! left out.

use std::collections::{HashMap, HashSet};

use gimli::write::{self, Address, AttributeValue, ConvertLineRow};
use gimli::{constants, read};
use object::{Object, ObjectSection, ObjectSymbol};
use serde::Serialize;

use super::DebugInfo;
use crate::binary::Binary;
use crate::error::{Error, Result, bail};
use crate::util;

/// An object file a debug map names.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugMapObject {
    pub index: u32,
    /// Where it was when the binary was linked: `/…/main.o`, or for a member
    /// of a static library, the library (`/…/libfoo.a`).
    pub path: String,
    /// The library member: `foo.o`.
    pub member: Option<String>,
    /// When it was last modified, as the linker recorded it (seconds since
    /// 1970; 0 if not recorded). A newer object no longer matches the binary.
    pub modified: u64,
    /// Functions and static variables the binary has from it.
    pub symbols: u32,
}

impl DebugMapObject {
    /// The file to look for: `main.o`, `libfoo.a`.
    pub fn file_name(&self) -> &str {
        self.path.rsplit(['/', '\\']).next().unwrap_or(&self.path)
    }

    /// `/…/main.o`, `/…/libfoo.a(foo.o)`
    pub fn display(&self) -> String {
        match &self.member {
            Some(m) => format!("{}({m})", self.path),
            None => self.path.clone(),
        }
    }
}

/// Gives a debug map's object (for a library member, the library): its
/// bytes, `None` when it can't be found, or why it can't be used.
pub type ReadObject<'a> = dyn FnMut(&DebugMapObject) -> std::result::Result<Option<Vec<u8>>, String> + 'a;

/// What reading a debug map's object files came to.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugMapReport {
    pub objects: u32,
    /// Objects whose DWARF was linked.
    pub linked: u32,
    /// Objects that weren't found.
    pub missing: Vec<String>,
    /// Objects found but not usable, and why.
    pub failed: Vec<String>,
    /// Compilation units in the linked DWARF.
    pub units: u32,
}

impl DebugMapReport {
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "DWARF from {} of {} object files (debug map), {} units",
            self.linked, self.objects, self.units
        );
        if !self.missing.is_empty() {
            out.push_str(&format!("; {} not found", self.missing.len()));
        }
        if !self.failed.is_empty() {
            out.push_str(&format!("; {} unusable", self.failed.len()));
        }
        out
    }
}

/// Where an object's code and data went in the binary: (object start,
/// object end, binary start), sorted and not overlapping.
struct Placement {
    ranges: Vec<(u64, u64, u64)>,
    /// The functions' ranges in the binary, sorted: what the object's units cover.
    code: Vec<(u64, u64)>,
}

impl Placement {
    fn find(&self, a: u64) -> Option<(u64, u64, u64)> {
        let i = self.ranges.partition_point(|r| r.0 <= a).checked_sub(1)?;
        let r = self.ranges[i];
        (a < r.1).then_some(r)
    }

    fn map(&self, a: u64) -> Option<u64> {
        self.find(a).map(|r| r.2 + (a - r.0))
    }

    /// An end address (the first byte after).
    fn map_end(&self, end: u64) -> Option<u64> {
        self.map(end.checked_sub(1)?).map(|a| a + 1)
    }

    /// A range, placed where its start went (and cut where that piece ends).
    fn map_range(&self, begin: u64, end: u64) -> Option<(u64, u64)> {
        let r = self.find(begin)?;
        let b = r.2 + (begin - r.0);
        Some((b, b + end.min(r.1).saturating_sub(begin)))
    }
}

/// Places an object's functions and variables: by name, from the debug map's
/// entries (`entries`: name, binary address, size) and, for global
/// variables, the binary's symbol table (`global`).
fn placement(
    obj: &object::File<'_>,
    entries: &[(&[u8], u64, u64)],
    global: &dyn Fn(&[u8]) -> Option<u64>,
) -> Placement {
    // Mach-O symbols have no sizes: each reaches the next symbol of its section, or the section's end.
    let mut starts: HashMap<object::SectionIndex, Vec<u64>> = HashMap::new();
    let mut symbols: HashMap<&[u8], (u64, object::SectionIndex, bool)> = HashMap::new();
    for s in obj.symbols() {
        let (Some(section), Ok(name)) = (s.section_index(), s.name_bytes()) else {
            continue;
        };
        if s.is_undefined() || name.is_empty() {
            continue;
        }
        let data = s.kind() == object::SymbolKind::Data;
        symbols.entry(name).or_insert((s.address(), section, data));
        starts.entry(section).or_default().push(s.address());
    }
    for list in starts.values_mut() {
        list.sort_unstable();
        list.dedup();
    }
    let extent = |address: u64, section: object::SectionIndex| -> u64 {
        let next = starts
            .get(&section)
            .and_then(|list| list.get(list.partition_point(|&a| a <= address)).copied());
        let end = next.or_else(|| obj.section_by_index(section).ok().map(|s| s.address() + s.size()));
        end.map_or(1, |e| e.saturating_sub(address).max(1))
    };
    let mut ranges = Vec::new();
    let mut code = Vec::new();
    let mut placed: HashSet<&[u8]> = HashSet::new();
    for &(name, address, size) in entries {
        let Some(&(start, section, _)) = symbols.get(name) else {
            continue;
        };
        // Functions come with their size (N_FUN); static variables don't (N_STSYM).
        let len = if size > 0 { size } else { extent(start, section) };
        ranges.push((start, start + len, address));
        if size > 0 {
            code.push((address, address + size));
        }
        placed.insert(name);
    }
    // Global variables (N_GSYM) are where the binary's symbol table says.
    for (&name, &(start, section, data)) in &symbols {
        if data
            && !placed.contains(name)
            && let Some(address) = global(name)
        {
            ranges.push((start, start + extent(start, section), address));
        }
    }
    ranges.sort_unstable();
    // Keep the first of overlapping pieces.
    let mut kept: Vec<(u64, u64, u64)> = Vec::with_capacity(ranges.len());
    for r in ranges {
        if kept.last().is_none_or(|k| r.0 >= k.1) {
            kept.push(r);
        }
    }
    code.sort_unstable();
    Placement { ranges: kept, code }
}

/// Whether a DIE's code or data went into the binary: then it, and what it
/// needs, is kept.
fn is_placed<R: read::Reader<Offset = usize>>(
    unit: read::UnitRef<'_, R>,
    entry: &read::DebuggingInformationEntry<R>,
    placement: &Placement,
) -> bool {
    if let Ok(mut ranges) = unit.die_ranges(entry)
        && let Ok(Some(r)) = ranges.next()
    {
        return placement.find(r.begin).is_some();
    }
    // A variable at a fixed address.
    if let Some(read::AttributeValue::Exprloc(expr)) = entry.attr_value(constants::DW_AT_location) {
        let mut ops = expr.operations(unit.encoding());
        while let Ok(Some(op)) = ops.next() {
            let address = match op {
                read::Operation::Address { address } => Some(address),
                read::Operation::AddressIndex { index } => unit.address(index).ok(),
                _ => continue,
            };
            return address.is_some_and(|a| placement.find(a).is_some());
        }
    }
    false
}

/// An attribute with its addresses moved; `None` when what it describes
/// wasn't placed.
fn convert_attribute<R: read::Reader<Offset = usize>>(
    unit: &mut write::ConvertUnit<'_, R>,
    read_unit: read::UnitRef<'_, R>,
    attr: &read::Attribute<R>,
    placement: &Placement,
) -> Option<AttributeValue> {
    let strict = |a: u64| placement.map(a).map(Address::Constant);
    let value = attr.value();
    // An end address (DWARF 2 and 3); later versions give a length, kept as it is.
    if attr.name() == constants::DW_AT_high_pc
        && let Ok(Some(end)) = read_unit.attr_address(value.clone())
    {
        return Some(AttributeValue::Address(Address::Constant(placement.map_end(end)?)));
    }
    // Range and location lists: each entry placed by where it starts.
    if let Ok(Some(mut ranges)) = read_unit.attr_ranges(value.clone()) {
        let mut list = Vec::new();
        while let Ok(Some(r)) = ranges.next() {
            if let Some((begin, end)) = placement.map_range(r.begin, r.end) {
                list.push(write::Range::StartEnd {
                    begin: Address::Constant(begin),
                    end: Address::Constant(end),
                });
            }
        }
        return (!list.is_empty()).then(|| AttributeValue::RangeListRef(unit.unit.ranges.add(write::RangeList(list))));
    }
    if let Ok(Some(mut locations)) = read_unit.attr_locations(value) {
        let mut list = Vec::new();
        while let Ok(Some(l)) = locations.next() {
            let Some((begin, end)) = placement.map_range(l.range.begin, l.range.end) else {
                continue;
            };
            let Ok(data) = unit.convert_expression(read_unit, l.data, &strict) else {
                continue;
            };
            list.push(write::Location::StartEnd {
                begin: Address::Constant(begin),
                end: Address::Constant(end),
                data,
            });
        }
        return (!list.is_empty())
            .then(|| AttributeValue::LocationListRef(unit.unit.locations.add(write::LocationList(list))));
    }
    // Everything else, with addresses (DW_AT_low_pc, DW_OP_addr) moved.
    unit.convert_attribute_value(read_unit, attr, &strict).ok()
}

/// A unit's line table with each row moved where its code went: a sequence
/// per placed piece, rows of dead-stripped code left out.
fn relink_lines<R: read::Reader<Offset = usize>>(
    unit: &mut write::ConvertUnit<'_, R>,
    placement: &Placement,
) -> Result<(), write::ConvertError> {
    let Some(mut lines) = unit.read_line_program(None, None)? else {
        return Ok(());
    };
    let mut base = 0u64;
    let mut current: Option<(u64, u64, u64)> = None;
    while let Some(row) = lines.read_row()? {
        match row {
            ConvertLineRow::SetAddress(address) => base = address,
            ConvertLineRow::Row(mut row) => {
                let at = base.wrapping_add(row.address_offset);
                let piece = placement.find(at);
                if piece != current {
                    if let Some(c) = current.take() {
                        lines.end_sequence(c.1 - c.0);
                    }
                    if let Some(p) = piece {
                        lines.begin_sequence(Some(Address::Constant(p.2)));
                        current = Some(p);
                    }
                }
                if let Some(c) = current {
                    row.address_offset = at - c.0;
                    lines.generate_row(row);
                }
            }
            ConvertLineRow::EndSequence(length) => {
                if let Some(c) = current.take() {
                    lines.end_sequence(base.wrapping_add(length).clamp(c.0, c.1) - c.0);
                }
                base = 0;
            }
        }
    }
    if let Some(c) = current.take() {
        lines.end_sequence(c.1 - c.0);
    }
    let (program, files) = lines.program();
    unit.set_line_program(program, files);
    Ok(())
}

/// Converts one object's DWARF into `out`, moved to the binary's addresses.
fn link_object(
    obj: &object::File<'_>,
    placement: &Placement,
    out: &mut write::Dwarf,
) -> Result<(), write::ConvertError> {
    let endian = if obj.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };
    // Mach-O objects hold their DWARF's addresses in place (their relocations are
    // relative to sections), so it is read as it is.
    let dwarf = read::Dwarf::load(|id| -> Result<_, gimli::Error> {
        let data = obj
            .section_by_name(id.name())
            .and_then(|s| s.data().ok())
            .unwrap_or(&[]);
        Ok(gimli::EndianSlice::new(data, endian))
    })?;
    let mut filter = write::FilterUnitSection::new(&dwarf)?;
    while let Some(mut unit) = filter.read_unit()? {
        let mut entry = unit.null_entry();
        while unit.read_entry(&mut entry)? {
            if is_placed(unit.read_unit, &entry, placement) {
                unit.require_entry(entry.offset);
            }
        }
    }
    let mut convert = out.convert_with_filter(filter)?;
    while let Some((mut unit, root)) = convert.read_unit()? {
        relink_lines(&mut unit, placement)?;
        let root_id = unit.unit.root();
        for attr in root.attrs.iter() {
            if matches!(
                attr.name(),
                constants::DW_AT_low_pc
                    | constants::DW_AT_high_pc
                    | constants::DW_AT_ranges
                    | constants::DW_AT_entry_pc
            ) {
                continue;
            }
            if let Some(value) = convert_attribute(&mut unit, root.read_unit, attr, placement) {
                unit.unit.get_mut(root_id).set(attr.name(), value);
            }
        }
        // The unit covers the object's functions, wherever they went.
        if !placement.code.is_empty() {
            let list = placement
                .code
                .iter()
                .map(|&(begin, end)| write::Range::StartEnd {
                    begin: Address::Constant(begin),
                    end: Address::Constant(end),
                })
                .collect();
            let ranges = unit.unit.ranges.add(write::RangeList(list));
            let root = unit.unit.get_mut(root_id);
            root.set(constants::DW_AT_low_pc, AttributeValue::Address(Address::Constant(0)));
            root.set(constants::DW_AT_ranges, AttributeValue::RangeListRef(ranges));
        }
        let mut entry = root;
        while let Some(id) = unit.read_entry(&mut entry)? {
            let Some(id) = id else { continue };
            let id = unit.add_entry(Some(id), &entry);
            for attr in entry.attrs.iter() {
                if let Some(value) = convert_attribute(&mut unit, entry.read_unit, attr, placement) {
                    unit.unit.get_mut(id).set(attr.name(), value);
                }
            }
        }
    }
    Ok(())
}

/// The modification times `N_OSO` entries record, in the order of the debug map's objects.
fn object_times(bin: &Binary) -> Vec<u64> {
    let b = util::Bytes::new(&bin.data, bin.endian);
    let mut out = Vec::new();
    let Some(ncmds) = b.u32(16) else { return out };
    let mut off: u64 = if bin.is64 { 32 } else { 28 };
    for _ in 0..ncmds.min(65536) {
        let (Some(cmd), Some(size)) = (b.u32(off), b.u32(off + 4)) else {
            break;
        };
        if cmd == object::macho::LC_SYMTAB.0 {
            let (Some(symoff), Some(nsyms), Some(stroff)) = (b.u32(off + 8), b.u32(off + 12), b.u32(off + 16)) else {
                break;
            };
            let entry = if bin.is64 { 16 } else { 12 };
            for i in 0..nsyms as u64 {
                let at = symoff as u64 + i * entry;
                if b.u8(at + 4) != Some(object::macho::N_OSO.0) {
                    continue;
                }
                // Only named ones start an object.
                let named = b
                    .u32(at)
                    .and_then(|strx| b.u8(stroff as u64 + strx as u64))
                    .is_some_and(|c| c != 0);
                if named {
                    out.push(
                        if bin.is64 {
                            b.u64(at + 8)
                        } else {
                            b.u32(at + 8).map(u64::from)
                        }
                        .unwrap_or(0),
                    );
                }
            }
            break;
        }
        if size < 8 {
            break;
        }
        off += size as u64;
    }
    out
}

/// The member `name` of a static library.
fn archive_member(bytes: Vec<u8>, name: &str) -> Result<std::sync::Arc<[u8]>> {
    let archive = crate::Container::parse(bytes)?;
    let member = archive
        .members()
        .iter()
        .find(|m| m.name == name || m.name.trim_end_matches('/') == name)
        .ok_or_else(|| Error::new(format!("no member {name}")))?;
    archive.member_data(member.index)
}

impl Binary {
    /// The object files a Mach-O debug map names (none for other binaries):
    /// where the DWARF of a binary linked without dsymutil was left.
    pub fn debug_map(&self) -> Vec<DebugMapObject> {
        let Ok(file) = object::File::parse(&*self.data) else {
            return Vec::new();
        };
        if file.format() != object::BinaryFormat::MachO {
            return Vec::new();
        }
        let map = file.object_map();
        let mut counts = vec![0u32; map.objects().len()];
        for s in map.symbols() {
            if let Some(c) = counts.get_mut(s.object_index()) {
                *c += 1;
            }
        }
        let times = object_times(self);
        map.objects()
            .iter()
            .enumerate()
            .map(|(i, o)| DebugMapObject {
                index: i as u32,
                path: util::lossy(o.path()),
                member: o.member().map(util::lossy),
                modified: times.get(i).copied().unwrap_or(0),
                symbols: counts[i],
            })
            .collect()
    }

    /// [`Self::attach_debug_map`] with the objects read from disk: where
    /// they were when the binary was linked, or by name from `folders` (the
    /// binary's own, a build's intermediates). Objects changed since the
    /// binary was linked are not used.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn attach_debug_map_from_disk(&mut self, folders: &[std::path::PathBuf]) -> Result<DebugMapReport> {
        use std::time::UNIX_EPOCH;
        self.attach_debug_map(&mut |o| {
            let recorded = std::path::PathBuf::from(&o.path);
            let candidates = std::iter::once(recorded).chain(folders.iter().map(|f| f.join(o.file_name())));
            let mut stale = None;
            for path in candidates {
                let Ok(meta) = std::fs::metadata(&path) else { continue };
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                if o.modified != 0 && modified != 0 && modified != o.modified {
                    stale = Some(path);
                    continue;
                }
                if let Ok(bytes) = std::fs::read(&path) {
                    return Ok(Some(bytes));
                }
            }
            match stale {
                Some(path) => Err(format!("{} changed after the binary was linked", path.display())),
                None => Ok(None),
            }
        })
    }

    /// Links the DWARF of the debug map's object files to this binary's
    /// addresses, as dsymutil would, and uses it as the binary's debug info.
    /// `read` gives an object's bytes (for a library member, the library's),
    /// `None` when it can't be found, or why it can't be used.
    pub fn attach_debug_map(&mut self, read: &mut ReadObject<'_>) -> Result<DebugMapReport> {
        let objects = self.debug_map();
        if objects.is_empty() {
            bail!("this binary has no debug map");
        }
        let data = self.data.clone();
        let file = object::File::parse(&*data)?;
        let map = file.object_map();
        let mut entries: Vec<Vec<(&[u8], u64, u64)>> = vec![Vec::new(); objects.len()];
        for s in map.symbols() {
            if let Some(list) = entries.get_mut(s.object_index()) {
                list.push((s.name(), s.address(), s.size()));
            }
        }
        let global = |name: &[u8]| -> Option<u64> {
            let s = self.symbols.by_name(std::str::from_utf8(name).ok()?)?;
            (s.defined && s.address != 0).then_some(s.address)
        };
        let mut out = write::Dwarf::new();
        let mut report = DebugMapReport {
            objects: objects.len() as u32,
            ..Default::default()
        };
        for o in &objects {
            let bytes = match read(o) {
                Ok(Some(bytes)) => bytes,
                Ok(None) => {
                    report.missing.push(o.display());
                    continue;
                }
                Err(why) => {
                    report.failed.push(format!("{}: {why}", o.display()));
                    continue;
                }
            };
            let bytes: std::sync::Arc<[u8]> = match &o.member {
                Some(member) => match archive_member(bytes, member) {
                    Ok(b) => b,
                    Err(e) => {
                        report.failed.push(format!("{}: {e}", o.display()));
                        continue;
                    }
                },
                None => bytes.into(),
            };
            let obj = match object::File::parse(&*bytes) {
                Ok(obj) if obj.architecture() == self.arch => obj,
                Ok(obj) => {
                    report
                        .failed
                        .push(format!("{}: it is for {:?}", o.display(), obj.architecture()));
                    continue;
                }
                Err(e) => {
                    report.failed.push(format!("{}: {e}", o.display()));
                    continue;
                }
            };
            let placement = placement(&obj, &entries[o.index as usize], &global);
            if placement.ranges.is_empty() {
                // Nothing of it was kept.
                continue;
            }
            match link_object(&obj, &placement, &mut out) {
                Ok(()) => report.linked += 1,
                Err(e) => report.failed.push(format!("{}: {e}", o.display())),
            }
        }
        if report.linked == 0 {
            let why = match (report.missing.first(), report.failed.first()) {
                (_, Some(f)) => f.clone(),
                (Some(m), None) => format!("{m} was not found"),
                (None, None) => "none of them has DWARF for this binary's code".into(),
            };
            bail!("no DWARF from the debug map's {} object files: {why}", objects.len());
        }
        let endian = match self.endian {
            util::Endian::Little => gimli::RunTimeEndian::Little,
            util::Endian::Big => gimli::RunTimeEndian::Big,
        };
        let mut sections = write::Sections::new(write::EndianVec::new(endian));
        out.write(&mut sections)
            .map_err(|e| Error::new(format!("writing the linked DWARF: {e}")))?;
        let mut linked = Vec::new();
        sections
            .for_each(|id, data| {
                linked.push((id, data.slice().to_vec()));
                Ok::<(), gimli::write::Error>(())
            })
            .map_err(|e| Error::new(format!("writing the linked DWARF: {e}")))?;
        let debug = DebugInfo::from_sections(linked, endian, "debug map", &self.sections, self.arch)?;
        report.units = debug.units().len() as u32;
        self.summary.has_dwarf = true;
        self.debug = Some(debug);
        // DWARF can name functions a stripped binary no longer does.
        self.rebuild_static_symbols();
        Ok(report)
    }
}

impl From<write::ConvertError> for Error {
    fn from(e: write::ConvertError) -> Self {
        Error::new(format!("DWARF: {e}"))
    }
}
