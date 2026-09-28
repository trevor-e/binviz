//! On-demand decoding of regions with many entries (tables, string tables,
//! functions in code sections). Only the entries under the cursor, or in the
//! visible hex window, are ever decoded.

use std::ops::Range;

use super::fields::FieldValue;
use super::{Ctx, Node, dwarf, elf, macho, pe};
use crate::model::RegionKind;
use crate::util;

/// A decoded entry of a region.
#[derive(Clone, Debug, Default)]
pub(crate) struct Entry {
    pub start: u64,
    pub end: u64,
    pub name: String,
    pub value: Option<String>,
    pub note: Option<String>,
    pub fields: Vec<FieldValue>,
    pub kind: Option<RegionKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableKind {
    ElfSectionHeaders,
    ElfSymbols,
    ElfRel,
    ElfRela,
    ElfDynamic,
    ElfVersym,
    MachSymbols,
    MachRelocs,
    MachIndirectSymbols,
    MachDataInCode,
    PeImportDescriptors,
    PeDelayImportDescriptors,
    PeThunks,
    PeExportAddresses,
    PeExportNames,
    PeExportOrdinals,
    PeRuntimeFunctions,
    PeDebugDirectory,
    PeSectionRelocs,
    /// Address-sized words, annotated with the symbol they point to.
    Pointers,
}

/// Fixed-size records.
#[derive(Clone, Debug)]
pub(crate) struct Table {
    pub size: u64,
    pub kind: TableKind,
    /// Associated string table (file offset, size), if entries refer to names.
    pub strtab: Option<(u64, u64)>,
    /// Associated table for cross references (e.g. the symbol table used by relocations):
    /// (file offset, entry size, count, strtab offset, strtab size).
    pub link: Option<LinkedTable>,
    /// Extra per-table parameter (e.g. the VA of the first entry, or first index).
    pub param: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LinkedTable {
    pub offset: u64,
    pub entry_size: u64,
    pub count: u64,
    pub strtab: Option<(u64, u64)>,
}

impl Table {
    pub fn new(kind: TableKind, size: u64) -> Table {
        Table {
            size,
            kind,
            strtab: None,
            link: None,
            param: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Decoder {
    Table(Table),
    /// NUL-terminated strings; the first `skip` bytes are a header.
    Strings {
        skip: u64,
    },
    /// Functions and objects from the symbol table. `address` is the VA of the
    /// node's first byte.
    Symbols {
        address: u64,
    },
    /// ELF note records.
    ElfNotes {
        align: u64,
    },
    /// PE base relocation blocks.
    PeBaseRelocs,
    /// PE hint/name table entries.
    PeHintNames,
    /// COFF symbol table, whose records may be followed by auxiliary records.
    CoffSymbols {
        strtab: Option<(u64, u64)>,
    },
    /// .debug_info: unit headers and DIEs down to single attributes.
    DwarfInfo,
    /// .debug_abbrev: abbreviation declarations.
    DwarfAbbrev,
    /// .debug_line: line program headers.
    DwarfLine,
}

impl Decoder {
    /// Decoder for a DWARF section, by name (ELF/PE `.debug_x`, Mach-O `__debug_x`).
    pub fn for_dwarf_section(name: &str) -> Option<Decoder> {
        match name.trim_start_matches('.').trim_start_matches("__") {
            "debug_info" => Some(Decoder::DwarfInfo),
            "debug_abbrev" => Some(Decoder::DwarfAbbrev),
            "debug_line" => Some(Decoder::DwarfLine),
            "debug_str" | "debug_line_str" => Some(Decoder::Strings { skip: 0 }),
            _ => None,
        }
    }
}

/// Upper bound on entries produced for a single span request, so a zoomed-out
/// view of a huge table stays cheap.
const MAX_BOUNDS: usize = 4096;

impl Decoder {
    pub fn count(&self, ctx: &Ctx, node: &Node) -> Option<u32> {
        match self {
            Decoder::Table(t) if t.size > 0 => Some(((node.end - node.start) / t.size) as u32),
            Decoder::Symbols { address } => {
                let symbols = ctx.symbols?;
                let n = symbols.in_range(*address, address + (node.end - node.start)).count();
                Some(n as u32)
            }
            Decoder::CoffSymbols { .. } => Some(self.starts(ctx, node).len() as u32),
            _ => None,
        }
    }

    /// Entries are variable-size and found by walking from the first one.
    fn walked(&self) -> bool {
        matches!(
            self,
            Decoder::ElfNotes { .. }
                | Decoder::PeBaseRelocs
                | Decoder::CoffSymbols { .. }
                | Decoder::DwarfAbbrev
                | Decoder::DwarfLine
                | Decoder::Strings { .. }
        )
    }

    /// The entry starting exactly at `pos` (variable-size decoders).
    fn decode_at(&self, ctx: &Ctx, node: &Node, pos: u64) -> Option<Entry> {
        match self {
            Decoder::ElfNotes { align } => elf::note_entry(ctx, pos, node.end, *align),
            Decoder::PeBaseRelocs => pe::base_reloc_block(ctx, pos, node.end),
            Decoder::CoffSymbols { strtab } => pe::coff_symbol_entry(ctx, node, *strtab, pos),
            Decoder::DwarfAbbrev => dwarf::abbrev_entry(ctx, node, pos),
            Decoder::DwarfLine => dwarf::line_entry(ctx, node, pos),
            Decoder::Strings { skip } => string_at(ctx, node, *skip, pos),
            _ => None,
        }
    }

    /// Length of the entry at `pos`, without decoding it where that is cheap.
    fn entry_len(&self, ctx: &Ctx, node: &Node, pos: u64) -> Option<u64> {
        let b = &ctx.bytes;
        let len = match self {
            Decoder::CoffSymbols { .. } => 18 * (1 + b.u8(pos + 17)? as u64),
            Decoder::PeBaseRelocs => b.u32(pos + 4)? as u64,
            Decoder::DwarfLine => match b.u32(pos)? {
                0xffff_ffff => 12 + b.u64(pos + 4)?,
                n => 4 + n as u64,
            },
            Decoder::Strings { skip } => {
                if pos < node.start + skip {
                    return Some(node.start + skip - pos);
                }
                let data = &b.data[pos as usize..node.end as usize];
                data.iter().position(|&c| c == 0).map_or(data.len(), |i| i + 1) as u64
            }
            _ => {
                let e = self.decode_at(ctx, node, pos)?;
                e.end.saturating_sub(pos)
            }
        };
        (len > 0).then_some(len)
    }

    /// Where each entry of a variable-size region starts, relative to the node (built once).
    fn starts<'n>(&self, ctx: &Ctx, node: &'n Node) -> &'n [u32] {
        node.starts.get_or_init(|| {
            let mut out = Vec::new();
            let mut pos = node.start;
            while pos < node.end && pos - node.start <= u32::MAX as u64 {
                out.push((pos - node.start) as u32);
                match self.entry_len(ctx, node, pos) {
                    Some(len) => pos += len,
                    None => break,
                }
            }
            out.shrink_to_fit();
            out
        })
    }

    /// Index of the entry containing `offset`, for variable-size decoders.
    fn index_of(&self, ctx: &Ctx, node: &Node, offset: u64) -> Option<usize> {
        let starts = self.starts(ctx, node);
        starts
            .partition_point(|&s| node.start + s as u64 <= offset)
            .checked_sub(1)
    }

    pub fn entry_at(&self, ctx: &Ctx, node: &Node, offset: u64) -> Option<Entry> {
        if offset < node.start || offset >= node.end {
            return None;
        }
        match self {
            Decoder::Table(t) => {
                if t.size == 0 {
                    return None;
                }
                let index = (offset - node.start) / t.size;
                let start = node.start + index * t.size;
                if start + t.size > node.end {
                    return Some(Entry {
                        start,
                        end: node.end,
                        name: "Trailing bytes".into(),
                        note: Some("Not a whole table entry".into()),
                        ..Entry::default()
                    });
                }
                Some(table_entry(ctx, t, index, start))
            }
            // Strings are found by walking back to the previous NUL: no index needed.
            Decoder::Strings { skip } => string_at(ctx, node, *skip, offset),
            Decoder::Symbols { address } => symbol_at(ctx, node, *address, offset),
            Decoder::PeHintNames => pe::hint_name_at(ctx, node, offset),
            Decoder::DwarfInfo => dwarf::info_entry(ctx, node, offset),
            _ if self.walked() => {
                let i = self.index_of(ctx, node, offset)?;
                let e = self.decode_at(ctx, node, node.start + self.starts(ctx, node)[i] as u64)?;
                (offset < e.end).then_some(e)
            }
            _ => None,
        }
    }

    /// Entries `first..first+count`, in order.
    pub fn entries(&self, ctx: &Ctx, node: &Node, first: u32, count: u32) -> Vec<Entry> {
        let (first, count) = (first as usize, count as usize);
        match self {
            Decoder::Table(t) if t.size > 0 => {
                let total = ((node.end - node.start) / t.size) as usize;
                (first..total.min(first + count))
                    .map(|i| table_entry(ctx, t, i as u64, node.start + i as u64 * t.size))
                    .collect()
            }
            Decoder::Symbols { address } => {
                let Some(symbols) = ctx.symbols else { return Vec::new() };
                symbols
                    .in_range(*address, address + (node.end - node.start))
                    .skip(first)
                    .take(count)
                    .filter_map(|s| symbol_at(ctx, node, *address, node.start + (s.address.max(*address) - address)))
                    .collect()
            }
            _ if self.walked() => {
                let starts = self.starts(ctx, node);
                starts
                    .iter()
                    .skip(first)
                    .take(count)
                    .filter_map(|&s| self.decode_at(ctx, node, node.start + s as u64))
                    .collect()
            }
            _ => {
                let mut out = Vec::new();
                let mut pos = node.start;
                let mut index = 0;
                while pos < node.end && out.len() < count {
                    let Some(e) = self.entry_at(ctx, node, pos) else { break };
                    if e.end <= pos {
                        break;
                    }
                    pos = e.end;
                    if index >= first {
                        out.push(e);
                    }
                    index += 1;
                }
                out
            }
        }
    }

    /// Entry boundaries overlapping `range`: (start, end, entry index, kind override).
    pub fn bounds(&self, ctx: &Ctx, node: &Node, range: Range<u64>) -> Vec<(u64, u64, u64, Option<RegionKind>)> {
        let mut out = Vec::new();
        match self {
            Decoder::DwarfInfo => return dwarf::info_bounds(ctx, node, range),
            Decoder::Table(t) => {
                if t.size == 0 {
                    return out;
                }
                let first = (range.start.saturating_sub(node.start)) / t.size;
                let mut i = first;
                loop {
                    let start = node.start + i * t.size;
                    if start >= range.end || start + t.size > node.end || out.len() >= MAX_BOUNDS {
                        break;
                    }
                    out.push((start, start + t.size, i, None));
                    i += 1;
                }
            }
            Decoder::Symbols { address } => {
                let Some(symbols) = ctx.symbols else { return out };
                let lo = address + (range.start - node.start);
                let hi = address + (range.end - node.start);
                let mut prev_end = 0;
                for s in symbols.in_range(lo, hi).take(MAX_BOUNDS) {
                    let size = s.size.max(1);
                    let start = node.start + s.address.saturating_sub(*address);
                    let end = (start + size).min(node.end);
                    if start < prev_end {
                        continue;
                    }
                    prev_end = end;
                    out.push((start, end, s.index as u64, None));
                }
            }
            Decoder::Strings { skip } => {
                // Walk backwards to the start of the string containing range.start.
                let data = ctx.bytes.data;
                let lo = node.start + skip;
                let mut pos = range.start.max(lo);
                while pos > lo && data[(pos - 1) as usize] != 0 {
                    pos -= 1;
                }
                let mut index = 0;
                while pos < range.end.min(node.end) && out.len() < MAX_BOUNDS {
                    let mut end = pos;
                    while end < node.end && data[end as usize] != 0 {
                        end += 1;
                    }
                    let end = (end + 1).min(node.end);
                    out.push((pos, end, index, None));
                    index += 1;
                    pos = end;
                }
            }
            _ if self.walked() => {
                let starts = self.starts(ctx, node);
                let first = self.index_of(ctx, node, range.start.max(node.start)).unwrap_or(0);
                for (i, &s) in starts.iter().enumerate().skip(first) {
                    let start = node.start + s as u64;
                    if start >= range.end || out.len() >= MAX_BOUNDS {
                        break;
                    }
                    let end = starts.get(i + 1).map_or(node.end, |&n| node.start + n as u64);
                    let kind = match self {
                        // Only DWARF entries override the region's kind.
                        Decoder::DwarfAbbrev | Decoder::DwarfLine => {
                            self.decode_at(ctx, node, start).and_then(|e| e.kind)
                        }
                        _ => None,
                    };
                    out.push((start, end, i as u64, kind));
                }
            }
            _ => {}
        }
        out
    }
}

fn table_entry(ctx: &Ctx, t: &Table, index: u64, offset: u64) -> Entry {
    let mut e = match t.kind {
        TableKind::ElfSectionHeaders
        | TableKind::ElfSymbols
        | TableKind::ElfRel
        | TableKind::ElfRela
        | TableKind::ElfDynamic
        | TableKind::ElfVersym => elf::table_entry(ctx, t, index, offset),
        TableKind::MachSymbols | TableKind::MachRelocs | TableKind::MachIndirectSymbols | TableKind::MachDataInCode => {
            macho::table_entry(ctx, t, index, offset)
        }
        TableKind::PeImportDescriptors
        | TableKind::PeDelayImportDescriptors
        | TableKind::PeThunks
        | TableKind::PeExportAddresses
        | TableKind::PeExportNames
        | TableKind::PeExportOrdinals
        | TableKind::PeRuntimeFunctions
        | TableKind::PeDebugDirectory
        | TableKind::PeSectionRelocs => pe::table_entry(ctx, t, index, offset),
        TableKind::Pointers => pointer_entry(ctx, t, index, offset),
    };
    e.start = offset;
    e.end = offset + t.size;
    e
}

fn pointer_entry(ctx: &Ctx, t: &Table, index: u64, offset: u64) -> Entry {
    let value = ctx.bytes.uint(offset, t.size as u32).unwrap_or(0);
    let target = ctx.symbolize(value);
    Entry {
        name: format!("[{index}]"),
        value: Some(match &target {
            Some(s) => format!("{} → {s}", util::hex(value)),
            None => util::hex(value),
        }),
        note: t
            .param
            .checked_add(index * t.size)
            .filter(|_| t.param != 0)
            .map(|va| format!("slot at {}", util::hex(va))),
        ..Entry::default()
    }
}

fn string_at(ctx: &Ctx, node: &Node, skip: u64, offset: u64) -> Option<Entry> {
    let lo = node.start + skip;
    if offset < lo {
        return Some(Entry {
            start: node.start,
            end: lo,
            name: "Table header".into(),
            value: ctx
                .bytes
                .u32(node.start)
                .map(|v| format!("size {}", util::hex(v as u64))),
            ..Entry::default()
        });
    }
    let data = ctx.bytes.data;
    let mut start = offset;
    while start > lo && data[(start - 1) as usize] != 0 {
        start -= 1;
    }
    let mut end = offset;
    while end < node.end && data[end as usize] != 0 {
        end += 1;
    }
    let text = &data[start as usize..end as usize];
    let end = (end + 1).min(node.end);
    Some(Entry {
        start,
        end,
        name: format!("String @{}", util::hex(start - lo)),
        value: Some(util::quote(text)),
        note: Some(format!("{} bytes + NUL", text.len())),
        ..Entry::default()
    })
}

fn symbol_at(ctx: &Ctx, node: &Node, address: u64, offset: u64) -> Option<Entry> {
    let symbols = ctx.symbols?;
    let va = address + (offset - node.start);
    let found = symbols.lookup(va)?;
    let sym = symbols.get(found.index)?;
    let start = node.start + sym.address.checked_sub(address)?;
    let end = (start + sym.size.max(1)).min(node.end);
    if offset >= end {
        return None;
    }
    let what = match sym.kind {
        crate::model::SymbolKind::Function => "Function",
        crate::model::SymbolKind::Data => "Object",
        crate::model::SymbolKind::Tls => "TLS object",
        _ => "Symbol",
    };
    Some(Entry {
        start,
        end,
        name: format!("{what} {}", sym.display_name()),
        value: Some(format!(
            "{}..{}",
            util::hex(sym.address),
            util::hex(sym.address + sym.size)
        )),
        note: Some(format!(
            "{} bytes{}{}",
            sym.size,
            if sym.size_inferred {
                " (size inferred from next symbol)"
            } else {
                ""
            },
            if sym.demangled().is_some() {
                format!(", mangled: {}", sym.name())
            } else {
                String::new()
            }
        )),
        ..Entry::default()
    })
}
