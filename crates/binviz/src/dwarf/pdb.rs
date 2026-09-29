//! PDB files: the debug info Microsoft's toolchains (and clang, and rustc for
//! the MSVC targets) keep beside a Windows binary. Read with the `pdb` crate
//! and turned into DWARF, so that everything binviz shows from DWARF shows
//! from a PDB too: each module (an object file) becomes a compilation unit,
//! its procedures subprograms and its globals variables, and its line
//! records the unit's line program. Procedures and public symbols also name
//! the binary's functions and data.

use std::collections::BTreeMap;

use gimli::write::{Address, AttributeValue, Dwarf, EndianVec, Expression, LineProgram, LineString, Sections, Unit};
use gimli::{Encoding, Format, LineEncoding, RunTimeEndian, SectionId};
use pdb::FallibleIterator;

use crate::error::{Error, Result};
use crate::model::SymbolKind;

/// Whether a file is a PDB (an MSF 7.0 container).
pub(crate) fn is_pdb(data: &[u8]) -> bool {
    data.starts_with(b"Microsoft C/C++ MSF 7.00\r\n\x1aDS")
}

/// A PDB read and turned into DWARF.
pub(crate) struct Converted {
    pub sections: Vec<(SectionId, Vec<u8>)>,
    /// Procedures (with their sizes) and data, then public symbols: (name, address, size, kind).
    pub symbols: Vec<(String, u64, u64, SymbolKind)>,
    /// Its GUID and age, the way the binary's build ID is written.
    pub id: String,
    pub modules: u32,
}

fn pdb_err(e: pdb::Error) -> Error {
    Error::new(format!("PDB: {e}"))
}

fn language(l: pdb::SourceLanguage) -> gimli::DwLang {
    match l {
        pdb::SourceLanguage::C => gimli::DW_LANG_C99,
        pdb::SourceLanguage::Cpp => gimli::DW_LANG_C_plus_plus,
        pdb::SourceLanguage::Fortran => gimli::DW_LANG_Fortran95,
        pdb::SourceLanguage::Masm => gimli::DW_LANG_Mips_Assembler,
        pdb::SourceLanguage::Pascal => gimli::DW_LANG_Pascal83,
        pdb::SourceLanguage::Cobol => gimli::DW_LANG_Cobol85,
        pdb::SourceLanguage::CSharp => gimli::DW_LANG_C_plus_plus,
        pdb::SourceLanguage::Java => gimli::DW_LANG_Java,
        pdb::SourceLanguage::D => gimli::DW_LANG_D,
        _ => gimli::DW_LANG_C,
    }
}

/// A line record: where its code is, and the line it came from.
struct Row {
    address: u64,
    end: u64,
    file: u32,
    line: u32,
}

/// Reads a PDB for a binary loaded at `image_base`.
pub(crate) fn convert(data: &[u8], image_base: u64) -> Result<Converted> {
    let mut pdb = pdb::PDB::open(std::io::Cursor::new(data)).map_err(pdb_err)?;
    let info = pdb.pdb_information().map_err(pdb_err)?;
    let dbi = pdb.debug_information().map_err(pdb_err)?;
    let age = dbi.age().unwrap_or(info.age);
    let id = format!(
        "{}{age:X}",
        info.guid.as_hyphenated().to_string().to_uppercase().replace('-', "")
    );
    let map = pdb.address_map().map_err(pdb_err)?;
    let strings = pdb.string_table().ok();
    let at = |o: pdb::PdbInternalSectionOffset| o.to_rva(&map).map(|r| image_base + u64::from(r.0));
    let encoding = Encoding {
        format: Format::Dwarf32,
        version: 4,
        address_size: 8,
    };
    let mut dwarf = Dwarf::new();
    let mut symbols = Vec::new();
    let mut modules = 0;
    let mut iter = dbi.modules().map_err(pdb_err)?;
    while let Some(module) = iter.next().map_err(pdb_err)? {
        let Some(mi) = pdb.module_info(&module).map_err(pdb_err)? else {
            continue;
        };
        // The module's procedures, globals and what compiled it.
        let mut procedures = Vec::new();
        let mut globals = Vec::new();
        let mut producer = None;
        let mut lang = None;
        if let Ok(mut syms) = mi.symbols() {
            while let Ok(Some(s)) = syms.next() {
                match s.parse() {
                    Ok(pdb::SymbolData::Procedure(p)) => {
                        if let Some(a) = at(p.offset) {
                            procedures.push((p.name.to_string().into_owned(), a, u64::from(p.len), p.global));
                        }
                    }
                    Ok(pdb::SymbolData::Data(d)) => {
                        if let Some(a) = at(d.offset) {
                            globals.push((d.name.to_string().into_owned(), a, d.global));
                        }
                    }
                    Ok(pdb::SymbolData::CompileFlags(c)) => {
                        let version = c.version_string.to_string().into_owned();
                        // Rust's language code is newer than the crate's list: the compiler says.
                        lang = Some(if version.contains("rustc") {
                            gimli::DW_LANG_Rust
                        } else {
                            language(c.language)
                        });
                        producer = Some(version);
                    }
                    _ => {}
                }
            }
        }
        // Its line records, and their files.
        let mut rows = Vec::new();
        // Ordered, as everything here is, so a PDB always reads the same.
        let mut files: BTreeMap<u32, String> = BTreeMap::new();
        if let Ok(program) = mi.line_program() {
            let mut lines = program.lines();
            while let Ok(Some(l)) = lines.next() {
                let Some(address) = at(l.offset) else { continue };
                let key = l.file_index.0;
                if let std::collections::btree_map::Entry::Vacant(e) = files.entry(key) {
                    let name = program
                        .get_file_info(l.file_index)
                        .ok()
                        .and_then(|f| {
                            strings
                                .as_ref()
                                .and_then(|t| f.name.to_string_lossy(t).ok())
                                .map(|n| n.into_owned())
                        })
                        .unwrap_or_else(|| format!("file{key}"));
                    e.insert(name);
                }
                rows.push(Row {
                    address,
                    end: address + u64::from(l.length.unwrap_or(0)),
                    file: key,
                    line: l.line_start,
                });
            }
        }
        if procedures.is_empty() && globals.is_empty() && rows.is_empty() {
            continue;
        }
        modules += 1;
        rows.sort_by_key(|r| r.address);
        // The unit is named after its main source: the file most of its lines
        // are in (of two, the one the module lists first).
        let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
        for r in &rows {
            *counts.entry(r.file).or_default() += 1;
        }
        let main = counts
            .iter()
            .max_by_key(|&(f, n)| (*n, std::cmp::Reverse(*f)))
            .and_then(|(f, _)| files.get(f).cloned());
        let module_name = module.module_name().into_owned();
        let unit_name = main.clone().unwrap_or_else(|| module_name.clone());
        let mut program = LineProgram::new(
            encoding,
            LineEncoding::default(),
            LineString::String(b"".to_vec()),
            None,
            LineString::String(unit_name.clone().into_bytes()),
            None,
        );
        let dir = program.default_directory();
        let mut ids = BTreeMap::new();
        for (key, name) in &files {
            let name = if name.is_empty() { "?".to_string() } else { name.clone() };
            ids.insert(*key, program.add_file(LineString::String(name.into_bytes()), dir, None));
        }
        // Runs of contiguous code, each a sequence.
        let mut i = 0;
        while i < rows.len() {
            let start = rows[i].address;
            program.begin_sequence(Some(Address::Constant(start)));
            let mut end = start;
            while i < rows.len() && rows[i].address <= end.max(start) {
                let r = &rows[i];
                program.row().address_offset = r.address - start;
                program.row().file = ids[&r.file];
                program.row().line = u64::from(r.line);
                program.generate_row();
                end = end.max(r.end).max(r.address + 1);
                i += 1;
            }
            program.end_sequence(end - start);
        }
        let mut unit = Unit::new(encoding, program);
        let root = unit.root();
        let low = procedures
            .iter()
            .map(|p| p.1)
            .chain(rows.first().map(|r| r.address))
            .min();
        let high = procedures
            .iter()
            .map(|p| p.1 + p.2)
            .chain(rows.iter().map(|r| r.end))
            .max();
        {
            let e = unit.get_mut(root);
            e.set(gimli::DW_AT_name, AttributeValue::String(unit_name.into_bytes()));
            e.set(
                gimli::DW_AT_producer,
                AttributeValue::String(
                    format!(
                        "{} (from {module_name}, in a PDB)",
                        producer.unwrap_or_else(|| "?".into())
                    )
                    .into_bytes(),
                ),
            );
            e.set(
                gimli::DW_AT_language,
                AttributeValue::Language(lang.unwrap_or(gimli::DW_LANG_C)),
            );
            e.set(gimli::DW_AT_stmt_list, AttributeValue::LineProgramRef);
            if let (Some(lo), Some(hi)) = (low, high) {
                e.set(gimli::DW_AT_low_pc, AttributeValue::Address(Address::Constant(lo)));
                e.set(gimli::DW_AT_high_pc, AttributeValue::Udata(hi.saturating_sub(lo)));
            }
        }
        for (name, address, size, global) in &procedures {
            let id = unit.add(root, gimli::DW_TAG_subprogram);
            let e = unit.get_mut(id);
            e.set(gimli::DW_AT_name, AttributeValue::String(name.clone().into_bytes()));
            e.set(
                gimli::DW_AT_low_pc,
                AttributeValue::Address(Address::Constant(*address)),
            );
            e.set(gimli::DW_AT_high_pc, AttributeValue::Udata((*size).max(1)));
            e.set(gimli::DW_AT_external, AttributeValue::Flag(*global));
            symbols.push((name.clone(), *address, *size, SymbolKind::Function));
        }
        for (name, address, global) in &globals {
            let id = unit.add(root, gimli::DW_TAG_variable);
            let e = unit.get_mut(id);
            e.set(gimli::DW_AT_name, AttributeValue::String(name.clone().into_bytes()));
            let mut location = Expression::new();
            location.op_addr(Address::Constant(*address));
            e.set(gimli::DW_AT_location, AttributeValue::Exprloc(location));
            e.set(gimli::DW_AT_external, AttributeValue::Flag(*global));
            symbols.push((name.clone(), *address, 0, SymbolKind::Data));
        }
        dwarf.units.add(unit);
    }
    // The global symbols: data the modules don't list, then the linker's public
    // names (mangled, or plain for C) for whatever nothing else names.
    let mut described: std::collections::HashSet<u64> = symbols.iter().map(|s| s.1).collect();
    let (mut data, mut publics) = (Vec::new(), Vec::new());
    if let Ok(globals) = pdb.global_symbols() {
        let mut syms = globals.iter();
        while let Ok(Some(s)) = syms.next() {
            match s.parse() {
                Ok(pdb::SymbolData::Public(p)) => {
                    if let Some(a) = at(p.offset) {
                        let kind = if p.function || p.code {
                            SymbolKind::Function
                        } else {
                            SymbolKind::Data
                        };
                        publics.push((p.name.to_string().into_owned(), a, kind));
                    }
                }
                Ok(pdb::SymbolData::Data(d)) => {
                    if let Some(a) = at(d.offset) {
                        data.push((d.name.to_string().into_owned(), a, SymbolKind::Data));
                    }
                }
                _ => {}
            }
        }
    }
    for (name, a, kind) in data {
        if described.insert(a) {
            symbols.push((name, a, 0, kind));
        }
    }
    // Several public names at one address are functions the linker folded into one: keep them all.
    for (name, a, kind) in publics {
        if !described.contains(&a) {
            symbols.push((name, a, 0, kind));
        }
    }
    let mut sections = Sections::new(EndianVec::new(RunTimeEndian::Little));
    dwarf
        .write(&mut sections)
        .map_err(|e| Error::new(format!("writing the PDB's DWARF: {e}")))?;
    let mut out = Vec::new();
    sections
        .for_each(|id, data| {
            out.push((id, data.slice().to_vec()));
            Ok::<(), gimli::write::Error>(())
        })
        .map_err(|e| Error::new(format!("writing the PDB's DWARF: {e}")))?;
    Ok(Converted {
        sections: out,
        symbols,
        id,
        modules,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_same_every_time() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/bin/pdbdemo.pdb");
        let data = std::fs::read(path).unwrap();
        let first = super::convert(&data, 0x1_4000_0000).unwrap();
        assert_eq!(first.modules, 2);
        for _ in 0..4 {
            let again = super::convert(&data, 0x1_4000_0000).unwrap();
            assert_eq!(again.sections, first.sections);
            assert_eq!(again.symbols, first.symbols);
        }
    }
}
