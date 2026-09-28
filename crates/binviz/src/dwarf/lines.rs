//! Line tables: the source file table, an address-sorted row index for
//! address → source lookups, and a per-file index for source → address.

use std::collections::HashMap;

use gimli::Reader;
use serde::Serialize;

use super::{DebugInfo, R};
use crate::model::SourceLoc;
use crate::util;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFile {
    pub id: u32,
    pub path: String,
    pub name: String,
    pub dir: String,
    /// Units whose line tables mention the file (first 32).
    pub units: Vec<u32>,
    /// DWARF 5 can embed the source text itself (`-gembed-source`).
    pub embedded_source: bool,
    pub md5: Option<String>,
}

/// One row of a line table, as a range of addresses mapping to a source position.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineRange {
    pub line: u32,
    pub column: u32,
    pub start: u64,
    pub end: u64,
    pub unit: u32,
    pub is_stmt: bool,
}

/// All rows for one file (sorted by line).
pub type FileLines = Vec<LineRange>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineFileEntry {
    pub index: u64,
    pub path: String,
    pub directory_index: u64,
    /// Global source file id.
    pub file: Option<u32>,
    pub md5: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineProgramInfo {
    pub unit: u32,
    pub offset: u64,
    pub version: u16,
    pub address_size: u8,
    pub dwarf64: bool,
    pub minimum_instruction_length: u8,
    pub maximum_operations_per_instruction: u8,
    pub default_is_stmt: bool,
    pub line_base: i8,
    pub line_range: u8,
    pub opcode_base: u8,
    pub include_directories: Vec<String>,
    pub files: Vec<LineFileEntry>,
    pub row_count: u32,
}

pub const FLAG_IS_STMT: u8 = 1;
pub const FLAG_BASIC_BLOCK: u8 = 2;
pub const FLAG_PROLOGUE_END: u8 = 4;
pub const FLAG_EPILOGUE_BEGIN: u8 = 8;
pub const FLAG_END_SEQUENCE: u8 = 16;

/// A raw line table row, in program order.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineRow {
    pub address: u64,
    pub file: Option<u32>,
    pub file_index: u64,
    pub line: u32,
    pub column: u32,
    /// Bit set of `FLAG_*`.
    pub flags: u8,
    pub discriminator: u64,
}

pub(crate) struct FileTable {
    pub files: Vec<SourceFile>,
    by_path: HashMap<String, u32>,
    /// Per unit: line-table file index → global file id.
    unit_files: Vec<Vec<u32>>,
}

#[derive(Clone, Copy)]
pub(crate) struct Row {
    pub start: u64,
    pub len: u32,
    pub file: u32,
    pub line: u32,
    pub column: u32,
    pub unit: u32,
    pub flags: u8,
}

pub(crate) struct RowIndex {
    rows: Vec<Row>,
    /// Per file: indices into `rows`, sorted by (line, start).
    by_file: Vec<Vec<u32>>,
}

const NO_FILE: u32 = u32::MAX;

fn row_flags(row: &gimli::LineRow) -> u8 {
    let bit = |set: bool, flag: u8| if set { flag } else { 0 };
    bit(row.is_stmt(), FLAG_IS_STMT)
        | bit(row.basic_block(), FLAG_BASIC_BLOCK)
        | bit(row.prologue_end(), FLAG_PROLOGUE_END)
        | bit(row.epilogue_begin(), FLAG_EPILOGUE_BEGIN)
        | bit(row.end_sequence(), FLAG_END_SEQUENCE)
}

/// Joins comp_dir, include directory and file name exactly like addr2line does,
/// so paths from both agree.
pub(crate) fn render_file(
    unit: &gimli::UnitRef<'_, R>,
    file: &gimli::FileEntry<R>,
    header: &gimli::LineProgramHeader<R>,
) -> Option<String> {
    let mut path = match &unit.comp_dir {
        Some(dir) => dir.to_string_lossy().ok()?.into_owned(),
        None => String::new(),
    };
    if file.directory_index() != 0
        && let Some(dir) = file.directory(header)
    {
        path_push(&mut path, &unit.attr_string(dir).ok()?.to_string_lossy().ok()?);
    }
    path_push(
        &mut path,
        &unit.attr_string(file.path_name()).ok()?.to_string_lossy().ok()?,
    );
    Some(path)
}

fn path_push(path: &mut String, p: &str) {
    let forward_root = |p: &str| p.starts_with('/') || p.get(1..3) == Some(":/");
    let backward_root = |p: &str| p.starts_with('\\') || p.get(1..3) == Some(":\\");
    if forward_root(p) || backward_root(p) {
        *path = p.to_string();
    } else {
        let sep = if backward_root(path) { '\\' } else { '/' };
        if !path.is_empty() && !path.ends_with(sep) {
            path.push(sep);
        }
        path.push_str(p);
    }
}

fn split_path(path: &str) -> (String, String) {
    match path.rfind(['/', '\\']) {
        Some(i) => (path[..i].to_string(), path[i + 1..].to_string()),
        None => (String::new(), path.to_string()),
    }
}

impl FileTable {
    pub fn build(debug_units: &[gimli::Unit<R>], dwarf: &gimli::Dwarf<R>) -> FileTable {
        let mut table = FileTable {
            files: Vec::new(),
            by_path: HashMap::new(),
            unit_files: Vec::new(),
        };
        for (ui, u) in debug_units.iter().enumerate() {
            let unit = u.unit_ref(dwarf);
            let mut map = Vec::new();
            if let Some(program) = &u.line_program {
                let header = program.header();
                let first = if header.version() >= 5 { 0 } else { 1 };
                if first == 1 {
                    map.push(NO_FILE);
                }
                let mut index = first;
                while let Some(file) = header.file(index) {
                    let id = match render_file(&unit, file, header) {
                        Some(path) => {
                            let md5 = header.file_has_md5().then(|| util::hex_compact(file.md5()));
                            let embedded = file
                                .source()
                                .is_some_and(|s| unit.attr_string(s).map(|r| !r.is_empty()).unwrap_or(false));
                            table.intern(path, ui as u32, md5, embedded)
                        }
                        None => NO_FILE,
                    };
                    map.push(id);
                    index += 1;
                }
            }
            table.unit_files.push(map);
        }
        table
    }

    fn intern(&mut self, path: String, unit: u32, md5: Option<String>, embedded: bool) -> u32 {
        if let Some(&id) = self.by_path.get(&path) {
            let f = &mut self.files[id as usize];
            if f.units.len() < 32 && f.units.last() != Some(&unit) {
                f.units.push(unit);
            }
            f.embedded_source |= embedded;
            return id;
        }
        let id = self.files.len() as u32;
        let (dir, name) = split_path(&path);
        self.files.push(SourceFile {
            id,
            path: path.clone(),
            name,
            dir,
            units: vec![unit],
            embedded_source: embedded,
            md5,
        });
        self.by_path.insert(path, id);
        id
    }

    pub fn file_id(&self, path: &str) -> Option<u32> {
        self.by_path.get(path).copied()
    }

    /// How many file indices a unit's line table defines (index 0 included).
    pub fn unit_files_len(&self, unit: u32) -> usize {
        self.unit_files.get(unit as usize).map_or(0, Vec::len)
    }

    pub fn unit_file(&self, unit: u32, index: u64) -> Option<u32> {
        let id = *self.unit_files.get(unit as usize)?.get(index as usize)?;
        (id != NO_FILE).then_some(id)
    }
}

impl RowIndex {
    pub fn build(debug: &DebugInfo) -> RowIndex {
        let mut rows = Vec::new();
        for (ui, u) in debug.units.iter().enumerate() {
            let Some(program) = &u.line_program else { continue };
            let mut seq: Vec<Row> = Vec::new();
            let mut iter = program.clone().rows();
            while let Ok(Some((_, row))) = iter.next_row() {
                let address = row.address();
                if row.end_sequence() {
                    // Discarded code (addresses of 0 or tombstones) produces bogus sequences.
                    let valid = seq.first().is_some_and(|f| debug.is_valid_code_address(f.start));
                    if valid {
                        for i in 0..seq.len() {
                            let end = seq.get(i + 1).map_or(address, |n| n.start);
                            seq[i].len = end.saturating_sub(seq[i].start).min(u32::MAX as u64) as u32;
                        }
                        rows.extend(seq.drain(..).filter(|r| r.len > 0));
                    }
                    seq.clear();
                    continue;
                }
                let flags = row_flags(row) & !FLAG_END_SEQUENCE;
                let r = Row {
                    start: address,
                    len: 0,
                    file: debug.files.unit_file(ui as u32, row.file_index()).unwrap_or(NO_FILE),
                    line: row.line().map_or(0, |l| l.get() as u32),
                    column: match row.column() {
                        gimli::ColumnType::LeftEdge => 0,
                        gimli::ColumnType::Column(c) => c.get() as u32,
                    },
                    unit: ui as u32,
                    flags,
                };
                // Several rows at one address: the last one wins (as in addr2line).
                match seq.last_mut() {
                    Some(last) if last.start == address => *last = r,
                    _ => seq.push(r),
                }
            }
        }
        rows.sort_by_key(|r| r.start);
        let mut by_file: Vec<Vec<u32>> = vec![Vec::new(); debug.files.files.len()];
        for (i, r) in rows.iter().enumerate() {
            if let Some(v) = by_file.get_mut(r.file as usize) {
                v.push(i as u32);
            }
        }
        for v in &mut by_file {
            v.sort_by_key(|&i| (rows[i as usize].line, rows[i as usize].start));
        }
        RowIndex { rows, by_file }
    }

    pub fn at(&self, address: u64) -> Option<&Row> {
        let idx = self.rows.partition_point(|r| r.start <= address);
        // Sequences from different units may overlap; look back a little.
        self.rows[..idx]
            .iter()
            .rev()
            .take(8)
            .find(|r| address < r.start + r.len as u64)
    }

    /// Every row, sorted by address.
    pub fn all(&self) -> &[Row] {
        &self.rows
    }

    pub fn in_range(&self, lo: u64, hi: u64) -> &[Row] {
        let a = self.rows.partition_point(|r| r.start + r.len as u64 <= lo);
        let b = self.rows.partition_point(|r| r.start < hi);
        &self.rows[a..b.max(a)]
    }
}

impl DebugInfo {
    pub fn source_files(&self) -> &[SourceFile] {
        &self.files.files
    }

    pub fn file_id(&self, path: &str) -> Option<u32> {
        self.files.file_id(path)
    }

    pub(crate) fn rows(&self) -> &RowIndex {
        self.rows.get_or_init(|| RowIndex::build(self))
    }

    fn loc(&self, r: &Row) -> Option<SourceLoc> {
        let f = self.files.files.get(r.file as usize)?;
        Some(SourceLoc {
            file: r.file,
            path: f.path.clone(),
            line: r.line,
            column: r.column,
        })
    }

    /// Source position of the line table row covering `address`.
    pub fn location(&self, address: u64) -> Option<SourceLoc> {
        let r = self.rows().at(address)?;
        self.loc(r)
    }

    /// Line table rows overlapping `lo..hi`, as (start, end, location).
    pub fn locations_in(&self, lo: u64, hi: u64) -> Vec<(u64, u64, SourceLoc)> {
        self.rows()
            .in_range(lo, hi)
            .iter()
            .filter_map(|r| Some((r.start, r.start + r.len as u64, self.loc(r)?)))
            .collect()
    }

    /// Every address range attributed to a source file, sorted by line.
    pub fn file_lines(&self, file: u32) -> FileLines {
        let rows = self.rows();
        let Some(idx) = rows.by_file.get(file as usize) else {
            return Vec::new();
        };
        idx.iter()
            .map(|&i| {
                let r = &rows.rows[i as usize];
                LineRange {
                    line: r.line,
                    column: r.column,
                    start: r.start,
                    end: r.start + r.len as u64,
                    unit: r.unit,
                    is_stmt: r.flags & FLAG_IS_STMT != 0,
                }
            })
            .collect()
    }

    /// For each source file (by id), how many distinct lines produced code.
    pub fn file_line_counts(&self) -> Vec<u32> {
        let rows = self.rows();
        rows.by_file
            .iter()
            .map(|idx| {
                let mut count = 0;
                let mut last = 0;
                // Indices are sorted by line, so distinct lines are runs.
                for &i in idx {
                    let line = rows.rows[i as usize].line;
                    if line != 0 && line != last {
                        count += 1;
                        last = line;
                    }
                }
                count
            })
            .collect()
    }

    /// Source text embedded in the line table (DWARF 5 `DW_LNCT_LLVM_source`).
    pub fn embedded_source(&self, file: u32) -> Option<String> {
        for &ui in &self.files.files.get(file as usize)?.units {
            let u = &self.units[ui as usize];
            let unit = u.unit_ref(&self.dwarf);
            let header = u.line_program.as_ref()?.header();
            let mut index = 0;
            while let Some(entry) = header.file(index) {
                if self.files.unit_file(ui, index) == Some(file)
                    && let Some(src) = entry.source()
                    && let Ok(s) = unit.attr_string(src)
                    && !s.is_empty()
                {
                    return s.to_string_lossy().ok().map(|c| c.into_owned());
                }
                index += 1;
            }
        }
        None
    }

    pub fn line_program(&self, unit: u32) -> Option<LineProgramInfo> {
        let u = self.units.get(unit as usize)?;
        let uref = u.unit_ref(&self.dwarf);
        let program = u.line_program.as_ref()?;
        let h = program.header();
        let include_directories = h
            .include_directories()
            .iter()
            .map(|d| {
                uref.attr_string(d.clone())
                    .ok()
                    .and_then(|s| s.to_string_lossy().ok().map(|c| c.into_owned()))
                    .unwrap_or_default()
            })
            .collect();
        let mut files = Vec::new();
        let first = if h.version() >= 5 { 0 } else { 1 };
        let mut index = first;
        while let Some(f) = h.file(index) {
            files.push(LineFileEntry {
                index,
                path: uref
                    .attr_string(f.path_name())
                    .ok()
                    .and_then(|s| s.to_string_lossy().ok().map(|c| c.into_owned()))
                    .unwrap_or_default(),
                directory_index: f.directory_index(),
                file: self.files.unit_file(unit, index),
                md5: h.file_has_md5().then(|| util::hex_compact(f.md5())),
            });
            index += 1;
        }
        let mut row_count = 0;
        let mut iter = program.clone().rows();
        while let Ok(Some(_)) = iter.next_row() {
            row_count += 1;
        }
        Some(LineProgramInfo {
            unit,
            offset: h.offset().0 as u64,
            version: h.version(),
            address_size: h.address_size(),
            dwarf64: h.format() == gimli::Format::Dwarf64,
            minimum_instruction_length: h.minimum_instruction_length(),
            maximum_operations_per_instruction: h.maximum_operations_per_instruction(),
            default_is_stmt: h.default_is_stmt(),
            line_base: h.line_base(),
            line_range: h.line_range(),
            opcode_base: h.opcode_base(),
            include_directories,
            files,
            row_count,
        })
    }

    /// Raw rows of a unit's line program, in program order.
    pub fn line_rows(&self, unit: u32, first: u32, count: u32) -> Vec<LineRow> {
        let Some(program) = self.units.get(unit as usize).and_then(|u| u.line_program.as_ref()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut iter = program.clone().rows();
        let mut i = 0u32;
        while let Ok(Some((_, row))) = iter.next_row() {
            if i >= first {
                if out.len() as u32 >= count {
                    break;
                }
                out.push(LineRow {
                    address: row.address(),
                    file: self.files.unit_file(unit, row.file_index()),
                    file_index: row.file_index(),
                    line: row.line().map_or(0, |l| l.get() as u32),
                    column: match row.column() {
                        gimli::ColumnType::LeftEdge => 0,
                        gimli::ColumnType::Column(c) => c.get() as u32,
                    },
                    flags: row_flags(row),
                    discriminator: row.discriminator(),
                });
            }
            i += 1;
        }
        out
    }
}
