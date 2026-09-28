//! DWARF debug information: units, DIE trees, line tables, and address →
//! source lookups (including inlined frames, via `addr2line`).

pub mod attribution;
mod check;
pub(crate) mod die;
mod explore;
mod expr;
mod lines;

use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

use gimli::{Reader as _, Section as _};
use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
use serde::Serialize;

use crate::error::Result;
use crate::model::{Frame, Section};
use crate::util;
pub use check::{DwarfCheck, DwarfProblem, Severity, UnitProblems};
pub use die::{AttrInfo, CodeLine, DieDetails, DieSummary, Link, MemberLayout};
pub use explore::{DiePage, ScopeInfo, ScopeVar, TagCount};
pub use lines::{FileLines, LineFileEntry, LineProgramInfo, LineRange, LineRow, SourceFile};
use lines::{FileTable, RowIndex};

pub(crate) type R = gimli::EndianArcSlice<gimli::RunTimeEndian>;

/// Where a unit's DWARF came from and what it describes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitInfo {
    pub index: u32,
    /// Offset of the unit header in its section.
    pub offset: u64,
    pub section: String,
    pub kind: String,
    pub version: u16,
    pub address_size: u8,
    pub dwarf64: bool,
    /// Size of the unit including its header.
    pub size: u64,
    pub name: Option<String>,
    pub comp_dir: Option<String>,
    pub producer: Option<String>,
    pub language: Option<String>,
    pub low_pc: u64,
    /// Address ranges covered by the unit (truncated to the first 64).
    pub ranges: Vec<[u64; 2]>,
    pub code_size: u64,
    pub dwo_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DwarfSection {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DwarfSummary {
    /// "embedded" or the name of the companion file the DWARF was loaded from.
    pub source: String,
    pub versions: Vec<u16>,
    pub unit_count: u32,
    pub sections: Vec<DwarfSection>,
    pub producers: Vec<String>,
    pub languages: Vec<String>,
    pub split_units: u32,
}

pub struct DebugInfo {
    pub(crate) dwarf: Arc<gimli::Dwarf<R>>,
    ctx: addr2line::Context<R>,
    pub(crate) units: Vec<gimli::Unit<R>>,
    infos: Vec<UnitInfo>,
    pub(crate) files: FileTable,
    rows: OnceLock<RowIndex>,
    source: String,
    sections: Vec<DwarfSection>,
    /// Address ranges of the binary's loaded sections; line sequences outside
    /// them belong to discarded code.
    code_ranges: Vec<(u64, u64)>,
    pub(crate) arch: object::Architecture,
    /// Per unit, unit-relative offsets of every DIE (null entries included), built on demand
    /// for byte-level decoding of .debug_info.
    die_starts: Vec<OnceLock<Vec<u32>>>,
    /// Named DIEs for search, built on demand.
    names: OnceLock<Vec<die::NamedDie>>,
    /// Global variables with static addresses, built on demand.
    globals: OnceLock<Vec<attribution::GlobalVar>>,
    /// Units that couldn't be read at all.
    load_problems: Vec<check::DwarfProblem>,
    /// The last flat DIE listing, so paging through it doesn't walk the unit again.
    listing: std::sync::Mutex<Option<explore::Listing>>,
}

impl DebugInfo {
    /// Loads DWARF from an object file. `sections` are the binary's sections
    /// (with synthetic addresses for relocatable objects), used to apply
    /// relocations to the debug sections of ELF/COFF objects and to filter
    /// line sequences of discarded code.
    pub(crate) fn load(
        file: &object::File<'_>,
        data: &Arc<[u8]>,
        source: &str,
        sections: &[Section],
    ) -> Result<Option<DebugInfo>> {
        let endian = if file.is_little_endian() {
            gimli::RunTimeEndian::Little
        } else {
            gimli::RunTimeEndian::Big
        };
        let whole = R::new(data.clone(), endian);
        let relocate = file.kind() == object::ObjectKind::Relocatable && file.format() != object::BinaryFormat::MachO;
        let mut found = Vec::new();
        let load = |id: gimli::SectionId| -> Result<R> {
            let Some(section) = file.section_by_name(id.name()) else {
                return Ok(R::new(Arc::from(&[][..]), endian));
            };
            let reader = if relocate && section.relocations().next().is_some() {
                let mut bytes = section.uncompressed_data()?.into_owned();
                apply_relocations(file, &section, &mut bytes, sections, endian);
                R::new(Arc::from(bytes), endian)
            } else {
                match section.uncompressed_data()? {
                    Cow::Borrowed(b) => {
                        // Share the file's buffer instead of copying.
                        let start = b.as_ptr() as usize - data.as_ptr() as usize;
                        whole.range(start..start + b.len())
                    }
                    Cow::Owned(v) => R::new(Arc::from(v), endian),
                }
            };
            Ok(reader)
        };
        let mut dwarf = gimli::Dwarf::load(|id| {
            let r = load(id);
            if let Ok(r) = &r
                && !r.bytes().is_empty()
            {
                found.push(DwarfSection {
                    name: id.name().to_string(),
                    size: r.bytes().len() as u64,
                });
            }
            r
        })?;
        if dwarf.debug_info.reader().bytes().is_empty() && dwarf.debug_line.reader().bytes().is_empty() {
            return Ok(None);
        }
        dwarf.file_type = gimli::DwarfFileType::Main;
        let dwarf = Arc::new(dwarf);
        let ctx = addr2line::Context::from_arc_dwarf(dwarf.clone())?;

        let mut units = Vec::new();
        let mut headers = Vec::new();
        let mut load_problems = Vec::new();
        // A bad header ends the walk: the length that leads to the next one can't be trusted.
        let mut next = 0u64;
        let mut iter = dwarf.units();
        loop {
            match iter.next() {
                Ok(Some(h)) => {
                    next = h.offset().0 as u64 + h.length_including_self() as u64;
                    headers.push(h);
                }
                Ok(None) => break,
                Err(e) => {
                    load_problems.push(check::load_problem(
                        ".debug_info",
                        next,
                        format!("can't read the unit header at {next:#x}: {e}; the units after it are skipped"),
                    ));
                    break;
                }
            }
        }
        next = 0;
        let mut type_iter = dwarf.type_units();
        loop {
            match type_iter.next() {
                Ok(Some(h)) => {
                    next = h.offset().0 as u64 + h.length_including_self() as u64;
                    headers.push(h);
                }
                Ok(None) => break,
                Err(e) => {
                    load_problems.push(check::load_problem(
                        ".debug_types",
                        next,
                        format!("can't read the type unit header at {next:#x}: {e}; the units after it are skipped"),
                    ));
                    break;
                }
            }
        }
        for h in headers {
            let (section, offset) = (h.section().name(), h.offset().0 as u64);
            match dwarf.unit(h) {
                Ok(u) => units.push(u),
                Err(e) => load_problems.push(check::load_problem(
                    section,
                    offset,
                    format!("can't read the unit at {offset:#x} (its abbreviations or line program header): {e}"),
                )),
            }
        }
        let infos = units
            .iter()
            .enumerate()
            .map(|(i, u)| unit_info(&dwarf, i as u32, u))
            .collect();
        let files = FileTable::build(&units, &dwarf);
        let code_ranges = sections
            .iter()
            .filter(|s| s.loaded && s.size > 0)
            .map(|s| (s.address, s.address + s.size))
            .collect();
        let die_starts = units.iter().map(|_| OnceLock::new()).collect();
        Ok(Some(DebugInfo {
            dwarf,
            ctx,
            units,
            infos,
            files,
            rows: OnceLock::new(),
            source: source.to_string(),
            sections: found,
            code_ranges,
            arch: file.architecture(),
            die_starts,
            names: OnceLock::new(),
            globals: OnceLock::new(),
            load_problems,
            listing: std::sync::Mutex::new(None),
        }))
    }

    /// Unit-relative offsets of every entry (DIEs and null entries) in a unit.
    pub(crate) fn die_starts(&self, unit: u32) -> &[u32] {
        let Some(cell) = self.die_starts.get(unit as usize) else {
            return &[];
        };
        cell.get_or_init(|| {
            let u = &self.units[unit as usize];
            let mut starts = Vec::new();
            let Ok(mut raw) = u.entries_raw(None) else {
                return starts;
            };
            while !raw.is_empty() {
                starts.push(raw.next_offset().0 as u32);
                match raw.read_abbreviation() {
                    Ok(Some(abbrev)) => {
                        if raw.skip_attributes(abbrev.attributes()).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
            starts
        })
    }

    /// Named DIEs (functions, variables, types...) for search; one per (name, tag).
    pub(crate) fn name_index(&self) -> &[die::NamedDie] {
        self.names.get_or_init(|| die::build_name_index(self))
    }

    /// Whether the DWARF came from the binary itself (so section offsets map to file bytes).
    pub(crate) fn is_embedded(&self) -> bool {
        self.source == "embedded"
    }

    pub(crate) fn is_valid_code_address(&self, address: u64) -> bool {
        self.code_ranges.is_empty() || self.code_ranges.iter().any(|&(s, e)| address >= s && address < e)
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn summary(&self) -> DwarfSummary {
        let mut versions: Vec<u16> = self.infos.iter().map(|u| u.version).collect();
        versions.sort_unstable();
        versions.dedup();
        let mut producers: Vec<String> = self.infos.iter().filter_map(|u| u.producer.clone()).collect();
        producers.sort();
        producers.dedup();
        let mut languages: Vec<String> = self.infos.iter().filter_map(|u| u.language.clone()).collect();
        languages.sort();
        languages.dedup();
        DwarfSummary {
            source: self.source.clone(),
            versions,
            unit_count: self.infos.len() as u32,
            sections: self.sections.clone(),
            producers,
            languages,
            split_units: self.infos.iter().filter(|u| u.dwo_name.is_some()).count() as u32,
        }
    }

    pub fn units(&self) -> &[UnitInfo] {
        &self.infos
    }

    pub(crate) fn unit(&self, index: u32) -> Option<gimli::UnitRef<'_, R>> {
        self.units.get(index as usize).map(|u| u.unit_ref(&self.dwarf))
    }

    /// The unit containing a `.debug_info` offset.
    pub(crate) fn unit_for_info_offset(&self, offset: u64) -> Option<u32> {
        // .debug_info units come first, in section order.
        let n = self
            .units
            .iter()
            .take_while(|u| u.header.section() == gimli::SectionId::DebugInfo)
            .count();
        let idx = self.units[..n].partition_point(|u| u.header.offset().0 as u64 <= offset);
        let i = idx.checked_sub(1)?;
        let h = &self.units[i].header;
        (offset < h.offset().0 as u64 + h.length_including_self() as u64).then_some(i as u32)
    }

    /// Unit whose address ranges contain `address`.
    pub fn unit_at(&self, address: u64) -> Option<u32> {
        self.infos
            .iter()
            .find(|u| u.ranges.iter().any(|r| address >= r[0] && address < r[1]))
            .map(|u| u.index)
            .or_else(|| {
                let lookup = self.ctx.find_dwarf_and_unit(address).skip_all_loads()?;
                let off = lookup.header.offset().0 as u64;
                self.infos
                    .iter()
                    .find(|u| u.offset == off && u.section == ".debug_info")
                    .map(|u| u.index)
            })
    }

    /// The (possibly inlined) call stack at `address`, innermost first.
    pub fn frames(&self, address: u64) -> Vec<Frame> {
        let mut out = Vec::new();
        let unit = self.unit_at(address);
        let Ok(mut iter) = self.ctx.find_frames(address).skip_all_loads() else {
            return out;
        };
        while let Ok(Some(f)) = iter.next() {
            let (function, demangled) = match &f.function {
                Some(name) => {
                    let raw = name.raw_name().map(|c| c.into_owned()).unwrap_or_default();
                    let dem = util::demangle(&raw);
                    (Some(raw), dem)
                }
                None => (None, None),
            };
            let (file, line, column) = match &f.location {
                Some(l) => (l.file.map(str::to_string), l.line, l.column),
                None => (None, None, None),
            };
            out.push(Frame {
                file_index: file.as_deref().and_then(|p| self.files.file_id(p)),
                function,
                demangled,
                file,
                line,
                column,
                unit,
                die: f.dw_die_offset.map(|o| o.0 as u64),
                inlined: false,
            });
        }
        let n = out.len();
        for (i, f) in out.iter_mut().enumerate() {
            f.inlined = i + 1 < n;
        }
        out
    }

    /// Functions described by DWARF (for binaries without a symbol table).
    pub(crate) fn subprograms(&self) -> Vec<(String, u64, u64)> {
        let mut out = Vec::new();
        for u in &self.units {
            let unit = u.unit_ref(&self.dwarf);
            let mut cursor = unit.entries();
            while let Ok(Some(entry)) = cursor.next_dfs() {
                if entry.tag() != gimli::DW_TAG_subprogram {
                    continue;
                }
                let Ok(mut ranges) = unit.die_ranges(entry) else {
                    continue;
                };
                let Ok(Some(r)) = ranges.next() else { continue };
                if r.begin == 0 || r.end <= r.begin {
                    continue;
                }
                let name = die::die_name(&unit, entry, true).unwrap_or_else(|| format!("sub_{:x}", r.begin));
                out.push((name, r.begin, r.end - r.begin));
            }
        }
        out
    }
}

fn unit_info(dwarf: &gimli::Dwarf<R>, index: u32, unit: &gimli::Unit<R>) -> UnitInfo {
    let h = &unit.header;
    let kind = match h.type_() {
        gimli::UnitType::Compilation => "compile",
        gimli::UnitType::Type { .. } => "type",
        gimli::UnitType::Partial => "partial",
        gimli::UnitType::Skeleton(_) => "skeleton",
        gimli::UnitType::SplitCompilation(_) => "split-compile",
        gimli::UnitType::SplitType { .. } => "split-type",
    };
    let unit_ref = unit.unit_ref(dwarf);
    let string = |r: &Option<R>| {
        r.as_ref()
            .and_then(|r| r.to_string_lossy().ok())
            .map(|c| c.into_owned())
    };
    let mut producer = None;
    let mut language = None;
    let mut dwo_name = None;
    let mut cursor = unit.entries();
    if let Ok(Some(root)) = cursor.next_dfs() {
        for attr in root.attrs() {
            match attr.name() {
                gimli::DW_AT_producer => {
                    producer = unit_ref
                        .attr_string(attr.value())
                        .ok()
                        .and_then(|s| s.to_string_lossy().ok().map(|c| c.into_owned()));
                }
                gimli::DW_AT_language => {
                    if let gimli::AttributeValue::Language(l) = attr.value() {
                        language = Some(match l {
                            // GNU as tags assembly units with this vendor constant.
                            gimli::DW_LANG_Mips_Assembler => "Assembler".to_string(),
                            _ => l.static_string().map_or_else(
                                || format!("{:#x}", l.0),
                                |s| s.trim_start_matches("DW_LANG_").to_string(),
                            ),
                        });
                    }
                }
                gimli::DW_AT_dwo_name | gimli::DW_AT_GNU_dwo_name => {
                    dwo_name = unit_ref
                        .attr_string(attr.value())
                        .ok()
                        .and_then(|s| s.to_string_lossy().ok().map(|c| c.into_owned()));
                }
                _ => {}
            }
        }
    }
    let mut ranges = Vec::new();
    let mut code_size = 0;
    if let Ok(mut iter) = unit_ref.unit_ranges() {
        while let Ok(Some(r)) = iter.next() {
            if r.end > r.begin {
                code_size += r.end - r.begin;
                if ranges.len() < 64 {
                    ranges.push([r.begin, r.end]);
                }
            }
        }
    }
    UnitInfo {
        index,
        offset: h.offset().0 as u64,
        section: h.section().name().to_string(),
        kind: kind.into(),
        version: h.version(),
        address_size: h.address_size(),
        dwarf64: h.format() == gimli::Format::Dwarf64,
        size: h.length_including_self() as u64,
        name: string(&unit.name),
        comp_dir: string(&unit.comp_dir),
        producer,
        language,
        low_pc: unit.low_pc,
        ranges,
        code_size,
        dwo_name,
    }
}

/// Applies relocations to a copy of a debug section of a relocatable ELF/COFF
/// object, so that addresses refer to the synthetic section layout.
fn apply_relocations(
    file: &object::File<'_>,
    section: &object::Section<'_, '_>,
    bytes: &mut [u8],
    sections: &[Section],
    endian: gimli::RunTimeEndian,
) {
    let base_of = |idx: object::SectionIndex| -> Option<u64> {
        // Our section list is in object's order; native index N is entry N-1 for
        // ELF (null section skipped) and COFF (1-based).
        let s = sections.get(idx.0.checked_sub(1)?)?;
        Some(if s.loaded { s.address } else { 0 })
    };
    for (offset, reloc) in section.relocations() {
        let (target, is_section_relative) = match reloc.target() {
            RelocationTarget::Symbol(idx) => {
                let Ok(sym) = file.symbol_by_index(idx) else { continue };
                let base = sym.section_index().and_then(base_of).unwrap_or(0);
                (base.wrapping_add(sym.address()), sym.address())
            }
            RelocationTarget::Section(idx) => (base_of(idx).unwrap_or(0), 0),
            _ => continue,
        };
        let size = (reloc.size() / 8) as usize;
        let Some(slot) = usize::try_from(offset).ok().and_then(|o| bytes.get_mut(o..o + size)) else {
            continue;
        };
        let read = |slot: &[u8]| -> u64 {
            let mut buf = [0u8; 8];
            match endian {
                gimli::RunTimeEndian::Little => {
                    buf[..slot.len()].copy_from_slice(slot);
                    u64::from_le_bytes(buf)
                }
                gimli::RunTimeEndian::Big => {
                    buf[8 - slot.len()..].copy_from_slice(slot);
                    u64::from_be_bytes(buf)
                }
            }
        };
        let addend = if reloc.has_implicit_addend() {
            read(slot) as i64
        } else {
            reloc.addend()
        };
        let value = match reloc.kind() {
            object::RelocationKind::Absolute => target.wrapping_add(addend as u64),
            // COFF SECREL: offset within the target section.
            object::RelocationKind::SectionOffset => is_section_relative.wrapping_add(addend as u64),
            _ => continue,
        };
        match (endian, size) {
            (gimli::RunTimeEndian::Little, 8) => slot.copy_from_slice(&value.to_le_bytes()),
            (gimli::RunTimeEndian::Little, 4) => slot.copy_from_slice(&(value as u32).to_le_bytes()),
            (gimli::RunTimeEndian::Big, 8) => slot.copy_from_slice(&value.to_be_bytes()),
            (gimli::RunTimeEndian::Big, 4) => slot.copy_from_slice(&(value as u32).to_be_bytes()),
            _ => {}
        }
    }
}

pub(crate) fn has_dwarf(file: &object::File<'_>) -> bool {
    [gimli::SectionId::DebugInfo, gimli::SectionId::DebugLine]
        .iter()
        .any(|id| file.section_by_name(id.name()).is_some())
}
