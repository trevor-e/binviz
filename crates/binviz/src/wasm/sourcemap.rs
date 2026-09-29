//! Source maps for WebAssembly, as `emcc -gsource-map` writes them: a JSON
//! file (version 3) whose `mappings` say, for the offset in the module where
//! each instruction starts, the source file, line and column it came from.
//! The whole module is one line of the "generated" text: each segment's
//! column is a byte offset in the module, binviz's address.
//!
//! A source map is read into DWARF, as [`crate::dwarf::pdb`] reads a PDB:
//! the functions whose code comes mostly from one source file make a
//! compilation unit named after it, each function a subprogram and a
//! sequence of the unit's line program. Source lines then show wherever
//! DWARF's do: the disassembly, `inspect`, stack traces. A source map says
//! nothing of inlining, so each frame is one function.

use std::collections::BTreeMap;

use gimli::write::{
    Address, AttributeValue, Dwarf, EndianVec, LineProgram, LineString, Range, RangeList, Sections, Unit,
};
use gimli::{Encoding, Format, LineEncoding, RunTimeEndian};
use serde_json::Value;

use super::read::Module;
use crate::dwarf::DebugInfo;
use crate::error::{Error, Result, bail};
use crate::model::Section;
use crate::symbols::SymbolTable;

/// Whether a file is a source map: a JSON object with `mappings`.
pub(crate) fn looks_like(data: &[u8]) -> bool {
    let text = data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(data);
    let start = text.iter().position(|b| !b.is_ascii_whitespace());
    start.is_some_and(|i| text[i] == b'{') && memchr::memmem::find(text, b"\"mappings\"").is_some()
}

/// A base64 digit's value.
fn base64(c: u8) -> Option<i64> {
    Some(match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    } as i64)
}

/// The numbers of a segment of `mappings`: base64 VLQ, each digit five bits
/// of the number (least significant first) and a continuation bit, the
/// number's sign in its lowest bit.
pub(crate) fn vlq(segment: &str) -> Option<Vec<i64>> {
    let mut out = Vec::new();
    let mut value = 0i64;
    let mut shift = 0u32;
    for c in segment.bytes() {
        let d = base64(c)?;
        if shift > 60 {
            return None;
        }
        value |= (d & 0x1f) << shift;
        if d & 0x20 != 0 {
            shift += 5;
            continue;
        }
        out.push(if value & 1 != 0 { -(value >> 1) } else { value >> 1 });
        value = 0;
        shift = 0;
    }
    (shift == 0).then_some(out)
}

/// A place in the module and where its code came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Mapping {
    /// The byte offset in the module.
    pub address: u64,
    /// Index into the source map's `sources`; `None` for code that came from no source.
    pub source: Option<u32>,
    /// 1-based, as DWARF counts them.
    pub line: u32,
    pub column: u32,
}

/// The mappings of a `mappings` string, in address order. The numbers of a
/// segment are deltas from the segment before (the column from the one
/// before on the same line, which for a module is every segment).
pub(crate) fn mappings(text: &str) -> Result<Vec<Mapping>> {
    let mut out = Vec::new();
    let (mut source, mut line, mut column) = (0i64, 0i64, 0i64);
    for (l, group) in text.split(';').enumerate() {
        let mut address = 0i64;
        for segment in group.split(',').filter(|s| !s.is_empty()) {
            let n = vlq(segment).ok_or_else(|| Error::new(format!("mappings: can't read segment {segment:?}")))?;
            address += n[0];
            if l > 0 {
                // A module is one line; further lines are text a module can't have.
                continue;
            }
            let mapped = n.len() >= 4;
            if mapped {
                source += n[1];
                line += n[2];
                column += n[3];
            }
            out.push(Mapping {
                address: address.max(0) as u64,
                source: mapped.then_some(source.max(0) as u32),
                line: if mapped { (line + 1).max(0) as u32 } else { 0 },
                column: if mapped { (column + 1).max(0) as u32 } else { 0 },
            });
        }
    }
    out.sort_by_key(|m| m.address);
    Ok(out)
}

/// A source file's language, by its extension.
fn language(path: &str) -> gimli::DwLang {
    match path.rsplit('.').next().unwrap_or("") {
        "c" | "h" => gimli::DW_LANG_C99,
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "C" => gimli::DW_LANG_C_plus_plus,
        "rs" => gimli::DW_LANG_Rust,
        _ => gimli::DW_LANG_C,
    }
}

/// Reads a source map for module `m` (whose functions `symbols` name) into DWARF.
pub(crate) fn load(
    data: &[u8],
    name: &str,
    m: &Module,
    symbols: &SymbolTable,
    sections: &[Section],
) -> Result<DebugInfo> {
    let map: Value = serde_json::from_slice(data).map_err(|e| Error::new(format!("{name}: not JSON: {e}")))?;
    if map.get("sections").is_some() {
        bail!("{name} is an index map (a source map of sections); binviz reads plain source maps");
    }
    let Some(text) = map.get("mappings").and_then(Value::as_str) else {
        bail!("{name} has no mappings");
    };
    let root = map.get("sourceRoot").and_then(Value::as_str).unwrap_or("");
    let sources: Vec<String> = map
        .get("sources")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|s| {
                    let s = s.as_str().unwrap_or("?");
                    if root.is_empty() || s.starts_with('/') || s.contains("://") {
                        s.to_string()
                    } else {
                        format!("{}/{s}", root.trim_end_matches('/'))
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let rows = mappings(text)?;
    if rows.iter().all(|r| r.source.is_none()) {
        bail!("{name} maps no code to a source");
    }
    // Each function's rows, and the source most of them are from.
    let mut functions: Vec<(u32, u64, u64, Vec<Mapping>)> = Vec::new();
    for (f, body) in m.functions_in(0, u64::MAX) {
        let first = rows.partition_point(|r| r.address < body.start);
        let own: Vec<Mapping> = rows[first..]
            .iter()
            .take_while(|r| r.address < body.end)
            .copied()
            .collect();
        if own.iter().any(|r| r.source.is_some()) {
            functions.push((f, body.start, body.end, own));
        }
    }
    let main_source = |rows: &[Mapping]| {
        let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
        for r in rows {
            if let Some(s) = r.source {
                *counts.entry(s).or_default() += 1;
            }
        }
        counts
            .iter()
            .max_by_key(|&(s, n)| (*n, std::cmp::Reverse(*s)))
            .map(|(s, _)| *s)
    };
    let mut units: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, f) in functions.iter().enumerate() {
        if let Some(s) = main_source(&f.3) {
            units.entry(s).or_default().push(i);
        }
    }
    let encoding = Encoding {
        format: Format::Dwarf32,
        version: 4,
        address_size: 4,
    };
    let mut dwarf = Dwarf::new();
    let source_name = |s: u32| sources.get(s as usize).cloned().unwrap_or_else(|| format!("source{s}"));
    for (source, members) in &units {
        let unit_name = source_name(*source);
        let mut program = LineProgram::new(
            encoding,
            LineEncoding::default(),
            LineString::String(Vec::new()),
            None,
            LineString::String(unit_name.clone().into_bytes()),
            None,
        );
        let dir = program.default_directory();
        let mut files = BTreeMap::new();
        for &i in members {
            for r in &functions[i].3 {
                if let Some(s) = r.source {
                    files.entry(s).or_insert_with(|| {
                        program.add_file(LineString::String(source_name(s).into_bytes()), dir, None)
                    });
                }
            }
        }
        // A sequence per function, starting where its first mapping does.
        let mut ranges = Vec::new();
        for &i in members {
            let (_, start, end, rows) = &functions[i];
            let Some(first) = rows.first() else { continue };
            program.begin_sequence(Some(Address::Constant(first.address)));
            let mut file = None;
            for (k, r) in rows.iter().enumerate() {
                // Of several mappings at one offset, the last.
                if rows.get(k + 1).is_some_and(|n| n.address == r.address) {
                    continue;
                }
                file = r.source.and_then(|s| files.get(&s).copied()).or(file);
                let Some(id) = file else { continue };
                program.row().address_offset = r.address - first.address;
                program.row().file = id;
                program.row().line = u64::from(r.line);
                program.row().column = u64::from(r.column);
                program.generate_row();
            }
            program.end_sequence(end - first.address);
            ranges.push(Range::StartLength {
                begin: Address::Constant(*start),
                length: end - start,
            });
        }
        let mut unit = Unit::new(encoding, program);
        let range_list = unit.ranges.add(RangeList(ranges));
        let root = unit.root();
        {
            let e = unit.get_mut(root);
            e.set(
                gimli::DW_AT_name,
                AttributeValue::String(unit_name.clone().into_bytes()),
            );
            e.set(
                gimli::DW_AT_producer,
                AttributeValue::String(format!("the source map {name}").into_bytes()),
            );
            e.set(gimli::DW_AT_language, AttributeValue::Language(language(&unit_name)));
            e.set(gimli::DW_AT_stmt_list, AttributeValue::LineProgramRef);
            e.set(gimli::DW_AT_low_pc, AttributeValue::Address(Address::Constant(0)));
            e.set(gimli::DW_AT_ranges, AttributeValue::RangeListRef(range_list));
        }
        for &i in members {
            let (f, start, end, _) = &functions[i];
            let function = symbols
                .at(*start)
                .map_or_else(|| format!("func[{f}]"), |s| s.name().to_string());
            let id = unit.add(root, gimli::DW_TAG_subprogram);
            let e = unit.get_mut(id);
            e.set(gimli::DW_AT_name, AttributeValue::String(function.into_bytes()));
            e.set(gimli::DW_AT_low_pc, AttributeValue::Address(Address::Constant(*start)));
            e.set(gimli::DW_AT_high_pc, AttributeValue::Udata(end - start));
        }
        dwarf.units.add(unit);
    }
    let mut out = Sections::new(EndianVec::new(RunTimeEndian::Little));
    dwarf
        .write(&mut out)
        .map_err(|e| Error::new(format!("writing {name}'s lines as DWARF: {e}")))?;
    let mut bytes = Vec::new();
    out.for_each(|id, data| {
        bytes.push((id, data.slice().to_vec()));
        Ok::<(), gimli::write::Error>(())
    })
    .map_err(|e| Error::new(format!("writing {name}'s lines as DWARF: {e}")))?;
    let arch = if m.memories.first().is_some_and(|m| m.limits.is64) {
        object::Architecture::Wasm64
    } else {
        object::Architecture::Wasm32
    };
    DebugInfo::from_sections(bytes, RunTimeEndian::Little, name, sections, arch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vlq_numbers() {
        assert_eq!(vlq("AAAA"), Some(vec![0, 0, 0, 0]));
        assert_eq!(vlq("C"), Some(vec![1]));
        assert_eq!(vlq("D"), Some(vec![-1]));
        // 16 needs a continuation: 0b100000 → "gB".
        assert_eq!(vlq("gB"), Some(vec![16]));
        assert_eq!(vlq("2HAAC"), Some(vec![123, 0, 0, 1]));
        // Five digits: 30 | 31 << 5 | 15 << 10 | 31 << 15 | 3 << 20, halved (positive).
        assert_eq!(vlq("+/v/D"), Some(vec![2_088_959]));
        assert_eq!(vlq("!"), None);
        assert_eq!(vlq("g"), None, "a continuation with nothing after it");
    }

    #[test]
    fn mapping_segments() {
        // Offsets 10, 12 and 20: two lines of source 0, then no source.
        let m = mappings("UAAA,EACA,Q").unwrap();
        assert_eq!(
            m,
            vec![
                Mapping {
                    address: 10,
                    source: Some(0),
                    line: 1,
                    column: 1
                },
                Mapping {
                    address: 12,
                    source: Some(0),
                    line: 2,
                    column: 1
                },
                Mapping {
                    address: 20,
                    source: None,
                    line: 0,
                    column: 0
                },
            ]
        );
        assert!(looks_like(br#" {"version":3,"mappings":"AAAA"}"#));
        assert!(!looks_like(b"\0asm"));
    }
}
