//! PDB files: the debug info Microsoft's toolchains (and clang, and rustc for
//! the MSVC targets) keep beside a Windows binary. Read with the `pdb` crate
//! and turned into DWARF, so that everything binviz shows from DWARF shows
//! from a PDB too: each module (an object file) becomes a compilation unit,
//! its procedures subprograms and its globals variables, and its line
//! records the unit's line program. Procedures and public symbols also name
//! the binary's functions and data.
//!
//! The type records (the TPI stream) become DWARF types in one more unit,
//! after the modules', which the modules' functions and variables refer to
//! across units (`DW_FORM_ref_addr`, which binviz's readers follow). A PDB's
//! types are already merged across its modules, so each is written once
//! rather than again in every unit that uses it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use gimli::write::{
    Address, AttributeValue, DebugInfoRef, Dwarf, EndianVec, Expression, LineProgram, LineString, Sections, Unit,
    UnitEntryId, UnitId,
};
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

/// A module's procedure: where its code is, its type record, and the names
/// its parameters are given.
struct Procedure {
    name: String,
    address: u64,
    size: u64,
    global: bool,
    type_index: pdb::TypeIndex,
    params: Vec<String>,
}

/// CodeView's number for x86's EBP, which older MSVC addresses parameters from.
const CV_REG_EBP: u16 = 22;

/// A module DIE's type attribute, set once the types' unit has its id.
type Fixup = (UnitEntryId, gimli::DwAt, UnitEntryId);

/// Reads a PDB for a binary loaded at `image_base` whose addresses (and
/// pointers) are `address_size` bytes.
pub(crate) fn convert(data: &[u8], image_base: u64, address_size: u8) -> Result<Converted> {
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
        address_size,
    };
    // Without type records (a stripped PDB), there are just no types.
    let tpi = pdb.type_information().ok();
    let mut types = tpi.as_ref().map(|t| Types::new(t, encoding));
    let mut fixups: Vec<(UnitId, Fixup)> = Vec::new();
    // Typedefs (S_UDT): name and type, from the modules and the global symbols.
    let mut typedefs: BTreeSet<(Vec<u8>, u32)> = BTreeSet::new();
    let mut languages: BTreeMap<u16, u32> = BTreeMap::new();
    let mut dwarf = Dwarf::new();
    let mut symbols = Vec::new();
    let mut modules = 0;
    let mut iter = dbi.modules().map_err(pdb_err)?;
    while let Some(module) = iter.next().map_err(pdb_err)? {
        let Some(mi) = pdb.module_info(&module).map_err(pdb_err)? else {
            continue;
        };
        // The module's procedures, globals and what compiled it.
        let mut procedures: Vec<Procedure> = Vec::new();
        let mut globals = Vec::new();
        let mut producer = None;
        let mut lang = None;
        if let Ok(mut syms) = mi.symbols() {
            // The procedure each open scope belongs to (None for blocks and
            // inlined calls), innermost last.
            let mut scopes: Vec<Option<usize>> = Vec::new();
            while let Ok(Some(s)) = syms.next() {
                let mut opened = None;
                let param_of = |scopes: &[Option<usize>]| scopes.last().copied().flatten();
                match s.parse() {
                    Ok(pdb::SymbolData::Procedure(p)) => {
                        if let Some(a) = at(p.offset) {
                            opened = Some(procedures.len());
                            procedures.push(Procedure {
                                name: p.name.to_string().into_owned(),
                                address: a,
                                size: u64::from(p.len),
                                global: p.global,
                                type_index: p.type_index,
                                params: Vec::new(),
                            });
                        }
                    }
                    // Parameters come first, in order: locals marked as such
                    // (clang, and MSVC since 2015), or older MSVC's slots above
                    // the frame pointer.
                    Ok(pdb::SymbolData::Local(l)) if l.flags.isparam => {
                        if let Some(i) = param_of(&scopes) {
                            procedures[i].params.push(l.name.to_string().into_owned());
                        }
                    }
                    Ok(pdb::SymbolData::RegisterRelative(r)) if r.register.0 == CV_REG_EBP && r.offset > 0 => {
                        if let Some(i) = param_of(&scopes) {
                            procedures[i].params.push(r.name.to_string().into_owned());
                        }
                    }
                    Ok(pdb::SymbolData::UserDefinedType(u)) if scopes.is_empty() => {
                        typedefs.insert((u.name.as_bytes().to_vec(), u.type_index.0));
                    }
                    Ok(pdb::SymbolData::Data(d)) => {
                        if let Some(a) = at(d.offset) {
                            globals.push((d.name.to_string().into_owned(), a, d.global, d.type_index));
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
                if s.starts_scope() {
                    scopes.push(opened);
                } else if s.ends_scope() {
                    scopes.pop();
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
        if let Some(l) = lang {
            *languages.entry(l.0).or_default() += 1;
        }
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
            .map(|p| p.address)
            .chain(rows.first().map(|r| r.address))
            .min();
        let high = procedures
            .iter()
            .map(|p| p.address + p.size)
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
        // Type attributes of this unit's DIEs, pointing into the types' unit.
        let mut refs: Vec<Fixup> = Vec::new();
        for p in &procedures {
            let id = unit.add(root, gimli::DW_TAG_subprogram);
            let e = unit.get_mut(id);
            e.set(gimli::DW_AT_name, AttributeValue::String(p.name.clone().into_bytes()));
            e.set(
                gimli::DW_AT_low_pc,
                AttributeValue::Address(Address::Constant(p.address)),
            );
            e.set(gimli::DW_AT_high_pc, AttributeValue::Udata(p.size.max(1)));
            e.set(gimli::DW_AT_external, AttributeValue::Flag(p.global));
            if let Some(sig) = types.as_mut().and_then(|t| t.signature(p.type_index)) {
                add_signature(&mut unit, id, &sig, &p.params, &mut refs);
            }
            symbols.push((p.name.clone(), p.address, p.size, SymbolKind::Function));
        }
        for (name, address, global, type_index) in &globals {
            let id = unit.add(root, gimli::DW_TAG_variable);
            let e = unit.get_mut(id);
            e.set(gimli::DW_AT_name, AttributeValue::String(name.clone().into_bytes()));
            let mut location = Expression::new();
            location.op_addr(Address::Constant(*address));
            e.set(gimli::DW_AT_location, AttributeValue::Exprloc(location));
            e.set(gimli::DW_AT_external, AttributeValue::Flag(*global));
            if let Some(t) = types.as_mut().and_then(|t| t.die(*type_index)) {
                refs.push((id, gimli::DW_AT_type, t));
            }
            symbols.push((name.clone(), *address, 0, SymbolKind::Data));
        }
        let unit_id = dwarf.units.add(unit);
        fixups.extend(refs.into_iter().map(|r| (unit_id, r)));
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
                Ok(pdb::SymbolData::UserDefinedType(u)) => {
                    typedefs.insert((u.name.as_bytes().to_vec(), u.type_index.0));
                }
                _ => {}
            }
        }
    }
    for (name, a, kind) in data.into_iter().chain(publics) {
        if described.insert(a) {
            symbols.push((name, a, 0, kind));
        }
    }
    if let Some(mut types) = types {
        // The language most modules are in.
        let lang = languages
            .iter()
            .max_by_key(|&(l, n)| (*n, std::cmp::Reverse(*l)))
            .map_or(gimli::DW_LANG_C, |(l, _)| gimli::DwLang(*l));
        let cplusplus = !matches!(
            lang,
            gimli::DW_LANG_C | gimli::DW_LANG_C89 | gimli::DW_LANG_C99 | gimli::DW_LANG_C11
        );
        for (name, index) in typedefs {
            types.typedef(&name, pdb::TypeIndex(index), cplusplus);
        }
        if let Some(unit) = types.finish(lang) {
            let types_id = dwarf.units.add(unit);
            for (unit_id, (entry, attr, target)) in fixups {
                dwarf.units.get_mut(unit_id).get_mut(entry).set(
                    attr,
                    AttributeValue::DebugInfoRef(DebugInfoRef::Entry(types_id, target)),
                );
            }
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

/// Gives a subprogram its return type, calling convention and parameters.
/// `names` are the parameters' names from the procedure's symbols, used when
/// there is one for each parameter (`this` aside).
fn add_signature(unit: &mut Unit, id: UnitEntryId, sig: &Signature, names: &[String], refs: &mut Vec<Fixup>) {
    let e = unit.get_mut(id);
    e.set(gimli::DW_AT_prototyped, AttributeValue::Flag(true));
    if let Some(cc) = sig.convention {
        e.set(gimli::DW_AT_calling_convention, AttributeValue::CallingConvention(cc));
    }
    if let Some(r) = sig.ret {
        refs.push((id, gimli::DW_AT_type, r));
    }
    let mut names: Vec<&String> = names.iter().filter(|n| n.as_str() != "this").collect();
    if names.len() != sig.params.iter().filter(|p| !p.1).count() {
        names.clear();
    }
    let mut named = names.into_iter();
    for &(ty, this) in &sig.params {
        let param = unit.add(id, gimli::DW_TAG_formal_parameter);
        let e = unit.get_mut(param);
        if this {
            e.set(gimli::DW_AT_name, AttributeValue::String(b"this".to_vec()));
            e.set(gimli::DW_AT_artificial, AttributeValue::Flag(true));
        } else if let Some(n) = named.next() {
            e.set(gimli::DW_AT_name, AttributeValue::String(n.as_bytes().to_vec()));
        }
        if let Some(t) = ty {
            refs.push((param, gimli::DW_AT_type, t));
        }
    }
    if sig.varargs {
        unit.add(id, gimli::DW_TAG_unspecified_parameters);
    }
}

/// A procedure type's DIEs: what it returns, its parameters (`this` flagged),
/// whether it takes more (`...`), and its calling convention when not C's.
struct Signature {
    ret: Option<UnitEntryId>,
    params: Vec<(Option<UnitEntryId>, bool)>,
    varargs: bool,
    convention: Option<gimli::DwCc>,
}

/// CodeView's calling conventions as DWARF writes them for x86 (the
/// vendor values LLVM uses); C's own convention needs none.
fn convention(attributes: pdb::FunctionAttributes) -> Option<gimli::DwCc> {
    match attributes.calling_convention() {
        0x02 | 0x03 => Some(gimli::DwCc(0xb2)), // DW_CC_BORLAND_pascal
        0x04 | 0x05 => Some(gimli::DwCc(0xb3)), // DW_CC_BORLAND_msfastcall
        0x07 | 0x08 => Some(gimli::DwCc(0xb1)), // DW_CC_BORLAND_stdcall
        0x0b => Some(gimli::DwCc(0xb5)),        // DW_CC_BORLAND_thiscall
        0x18 => Some(gimli::DwCc(0xc0)),        // DW_CC_LLVM_vectorcall
        _ => None,
    }
}

/// Whether a type's name is one a compiler gives an anonymous type
/// (`<unnamed-tag>`, `S::<unnamed-type-u>`, `__unnamed`).
fn is_anonymous(name: &[u8]) -> bool {
    let last = match name.windows(2).rposition(|w| w == b"::") {
        Some(i) => &name[i + 2..],
        None => name,
    };
    last.is_empty()
        || last.starts_with(b"<unnamed")
        || last.starts_with(b"<anonymous")
        || last.starts_with(b"__unnamed")
}

/// A primitive type's name, encoding and size in bytes (`None`: void).
fn primitive(kind: pdb::PrimitiveKind) -> Option<(&'static str, gimli::DwAte, u64)> {
    use pdb::PrimitiveKind as K;
    let (signed, unsigned, float, boolean) = (
        gimli::DW_ATE_signed,
        gimli::DW_ATE_unsigned,
        gimli::DW_ATE_float,
        gimli::DW_ATE_boolean,
    );
    Some(match kind {
        K::Char => ("signed char", gimli::DW_ATE_signed_char, 1),
        K::RChar => ("char", gimli::DW_ATE_signed_char, 1),
        K::UChar => ("unsigned char", gimli::DW_ATE_unsigned_char, 1),
        K::WChar => ("wchar_t", unsigned, 2),
        K::RChar16 => ("char16_t", gimli::DW_ATE_UTF, 2),
        K::RChar32 => ("char32_t", gimli::DW_ATE_UTF, 4),
        K::I8 => ("__int8", signed, 1),
        K::U8 => ("unsigned __int8", unsigned, 1),
        K::Short => ("short", signed, 2),
        K::UShort => ("unsigned short", unsigned, 2),
        K::I16 => ("__int16", signed, 2),
        K::U16 => ("unsigned __int16", unsigned, 2),
        K::Long => ("long", signed, 4),
        K::ULong => ("unsigned long", unsigned, 4),
        K::I32 => ("int", signed, 4),
        K::U32 => ("unsigned int", unsigned, 4),
        K::Quad | K::I64 => ("long long", signed, 8),
        K::UQuad | K::U64 => ("unsigned long long", unsigned, 8),
        K::Octa | K::I128 => ("__int128", signed, 16),
        K::UOcta | K::U128 => ("unsigned __int128", unsigned, 16),
        K::F16 => ("_Float16", float, 2),
        K::F32 | K::F32PP => ("float", float, 4),
        K::F48 => ("__float48", float, 6),
        K::F64 => ("double", float, 8),
        K::F80 => ("long double", float, 10),
        K::F128 => ("__float128", float, 16),
        K::Complex32 => ("_Complex float", gimli::DW_ATE_complex_float, 8),
        K::Complex64 => ("_Complex double", gimli::DW_ATE_complex_float, 16),
        K::Complex80 => ("_Complex long double", gimli::DW_ATE_complex_float, 20),
        K::Complex128 => ("_Complex __float128", gimli::DW_ATE_complex_float, 32),
        K::Bool8 => ("bool", boolean, 1),
        K::Bool16 => ("__bool16", boolean, 2),
        K::Bool32 => ("__bool32", boolean, 4),
        K::Bool64 => ("__bool64", boolean, 8),
        _ => return None,
    })
}

/// How deep the DIE for one type may make the DIEs for others (a pointer to
/// an array of pointers...) before giving up on a corrupt record.
const MAX_DEPTH: u32 = 200;

/// A PDB's type records turned into the DIEs of a unit of their own.
struct Types<'t> {
    finder: pdb::TypeFinder<'t>,
    unit: Unit,
    pointer_size: u8,
    /// Each type index's DIE (`None`: void, or a record that isn't a type).
    dies: HashMap<u32, Option<UnitEntryId>>,
    /// Complete classes, unions and enums by unique name (or name, for types
    /// without one): what their forward references stand for.
    definitions: HashMap<Vec<u8>, pdb::TypeIndex>,
    /// Aggregates whose members are still to be added: a work list rather than
    /// recursion, so that long chains of types don't exhaust the stack.
    pending: Vec<(pdb::TypeIndex, UnitEntryId)>,
    /// `void **`, the type of a vtable pointer.
    vtable: Option<UnitEntryId>,
    depth: u32,
}

impl<'t> Types<'t> {
    /// Reads every type record, and makes the DIEs of every complete class,
    /// union and enum (what they use is made as they need it).
    fn new(tpi: &'t pdb::TypeInformation<'_>, encoding: Encoding) -> Types<'t> {
        let mut finder = tpi.finder();
        let mut iter = tpi.iter();
        let mut complete = Vec::new();
        let mut definitions = HashMap::new();
        while let Ok(Some(item)) = iter.next() {
            finder.update(&iter);
            let (name, unique, forward) = match item.parse() {
                Ok(pdb::TypeData::Class(c)) => (c.name, c.unique_name, c.properties.forward_reference()),
                Ok(pdb::TypeData::Union(u)) => (u.name, u.unique_name, u.properties.forward_reference()),
                Ok(pdb::TypeData::Enumeration(e)) => (e.name, e.unique_name, e.properties.forward_reference()),
                _ => continue,
            };
            if forward {
                continue;
            }
            complete.push(item.index());
            if !is_anonymous(name.as_bytes()) {
                let key = unique.unwrap_or(name).as_bytes().to_vec();
                definitions.entry(key).or_insert(item.index());
            }
        }
        let mut types = Types {
            finder,
            unit: Unit::new(encoding, LineProgram::none()),
            pointer_size: encoding.address_size,
            dies: HashMap::new(),
            definitions,
            pending: Vec::new(),
            vtable: None,
            depth: 0,
        };
        for index in complete {
            types.die(index);
        }
        types.fill_pending();
        types
    }

    fn parse(&self, index: pdb::TypeIndex) -> Option<pdb::TypeData<'t>> {
        self.finder.find(index).ok()?.parse().ok()
    }

    fn add(&mut self, tag: gimli::DwTag) -> UnitEntryId {
        let root = self.unit.root();
        self.unit.add(root, tag)
    }

    fn set(&mut self, id: UnitEntryId, attr: gimli::DwAt, value: AttributeValue) {
        self.unit.get_mut(id).set(attr, value);
    }

    fn set_type(&mut self, id: UnitEntryId, ty: Option<UnitEntryId>) {
        if let Some(t) = ty {
            self.set(id, gimli::DW_AT_type, AttributeValue::UnitRef(t));
        }
    }

    /// The DIE of a type, made on first use (`None` for void).
    fn die(&mut self, index: pdb::TypeIndex) -> Option<UnitEntryId> {
        if let Some(&id) = self.dies.get(&index.0) {
            return id;
        }
        if self.depth > MAX_DEPTH {
            return None;
        }
        // Until it is made, a record that leads back to itself reads as void.
        self.dies.insert(index.0, None);
        self.depth += 1;
        let id = self.make(index);
        self.depth -= 1;
        self.dies.insert(index.0, id);
        id
    }

    fn make(&mut self, index: pdb::TypeIndex) -> Option<UnitEntryId> {
        match self.parse(index)? {
            pdb::TypeData::Primitive(p) => match p.indirection {
                None => self.base(p.kind),
                // `T *` for a primitive T is written in the index itself.
                Some(indirection) => {
                    use pdb::Indirection as I;
                    let size = match indirection {
                        I::Near16 => 2,
                        I::Far16 | I::Huge16 | I::Near32 => 4,
                        I::Far32 => 6,
                        I::Near64 => 8,
                        I::Near128 => 16,
                    };
                    let to = self.die(pdb::TypeIndex(index.0 & 0xff));
                    Some(self.pointer(gimli::DW_TAG_pointer_type, to, size))
                }
            },
            pdb::TypeData::Class(c) => {
                let tag = match c.kind {
                    pdb::ClassKind::Class => gimli::DW_TAG_class_type,
                    pdb::ClassKind::Struct | pdb::ClassKind::Interface => gimli::DW_TAG_structure_type,
                };
                let forward = c.properties.forward_reference();
                self.aggregate(index, tag, c.name, c.unique_name, forward, c.size)
            }
            pdb::TypeData::Union(u) => {
                let forward = u.properties.forward_reference();
                self.aggregate(index, gimli::DW_TAG_union_type, u.name, u.unique_name, forward, u.size)
            }
            pdb::TypeData::Enumeration(e) => {
                if e.properties.forward_reference()
                    && let Some(def) = self.definition(e.name, e.unique_name)
                    && def != index
                {
                    return self.die(def);
                }
                let underlying = self.die(e.underlying_type);
                let size = self.size_of(e.underlying_type).unwrap_or(4);
                let id = self.add(gimli::DW_TAG_enumeration_type);
                self.name(id, e.name.as_bytes());
                self.set(id, gimli::DW_AT_byte_size, AttributeValue::Udata(size));
                self.set_type(id, underlying);
                if e.properties.forward_reference() {
                    self.set(id, gimli::DW_AT_declaration, AttributeValue::Flag(true));
                }
                // CodeView may write a negative value of a signed type as
                // unsigned (clang writes an int's -1 as 4294967295).
                let signed = match self.parse(e.underlying_type) {
                    Some(pdb::TypeData::Primitive(p)) => primitive(p.kind)
                        .filter(|(_, enc, _)| matches!(*enc, gimli::DW_ATE_signed | gimli::DW_ATE_signed_char))
                        .map(|(_, _, size)| size),
                    _ => None,
                };
                for field in self.fields(Some(e.fields)) {
                    if let pdb::TypeData::Enumerate(v) = field {
                        let mut n = match v.value {
                            pdb::Variant::U8(x) => i128::from(x),
                            pdb::Variant::U16(x) => i128::from(x),
                            pdb::Variant::U32(x) => i128::from(x),
                            pdb::Variant::U64(x) => i128::from(x),
                            pdb::Variant::I8(x) => i128::from(x),
                            pdb::Variant::I16(x) => i128::from(x),
                            pdb::Variant::I32(x) => i128::from(x),
                            pdb::Variant::I64(x) => i128::from(x),
                        };
                        if let Some(size) = signed.filter(|&s| s < 16) {
                            let bits = 8 * size as u32;
                            if n >= 1 << (bits - 1) && n < 1 << bits {
                                n -= 1 << bits;
                            }
                        }
                        let value = match i64::try_from(n) {
                            Ok(s) if s < 0 => AttributeValue::Sdata(s),
                            _ => AttributeValue::Udata(n as u64),
                        };
                        let e = self.unit.add(id, gimli::DW_TAG_enumerator);
                        self.set(e, gimli::DW_AT_name, AttributeValue::String(v.name.as_bytes().to_vec()));
                        self.set(e, gimli::DW_AT_const_value, value);
                    }
                }
                Some(id)
            }
            pdb::TypeData::Pointer(p) => {
                let attributes = p.attributes;
                let tag = match attributes.pointer_mode() {
                    pdb::PointerMode::Pointer => gimli::DW_TAG_pointer_type,
                    pdb::PointerMode::LValueReference => gimli::DW_TAG_reference_type,
                    pdb::PointerMode::RValueReference => gimli::DW_TAG_rvalue_reference_type,
                    pdb::PointerMode::Member | pdb::PointerMode::MemberFunction => gimli::DW_TAG_ptr_to_member_type,
                };
                let size = match attributes.size() {
                    0 => u64::from(self.pointer_size),
                    n => u64::from(n),
                };
                let to = self.die(p.underlying_type);
                let id = self.pointer(tag, to, size);
                if let Some(class) = p.containing_class.and_then(|c| self.die(c)) {
                    self.set(id, gimli::DW_AT_containing_type, AttributeValue::UnitRef(class));
                }
                // A pointer that is itself const or volatile (`T *const`).
                self.qualified(Some(id), attributes.is_const(), attributes.is_volatile())
            }
            pdb::TypeData::Modifier(m) => {
                let to = self.die(m.underlying_type);
                self.qualified(to, m.constant, m.volatile)
            }
            pdb::TypeData::Array(a) => Some(self.array(a)),
            pdb::TypeData::Procedure(p) => {
                let sig = self.procedure(p.return_type, None, p.argument_list, p.attributes);
                Some(self.subroutine(&sig))
            }
            pdb::TypeData::MemberFunction(m) => {
                let sig = self.procedure(Some(m.return_type), m.this_pointer_type, m.argument_list, m.attributes);
                Some(self.subroutine(&sig))
            }
            // Only a member has a bit field's type; elsewhere it is its storage type.
            pdb::TypeData::Bitfield(b) => self.die(b.underlying_type),
            _ => None,
        }
    }

    /// Names a DIE, unless the name is a compiler's for an anonymous type.
    fn name(&mut self, id: UnitEntryId, name: &[u8]) {
        if !is_anonymous(name) {
            self.set(id, gimli::DW_AT_name, AttributeValue::String(name.to_vec()));
        }
    }

    /// The complete type a forward reference stands for.
    fn definition(&self, name: pdb::RawString<'_>, unique: Option<pdb::RawString<'_>>) -> Option<pdb::TypeIndex> {
        self.definitions.get(unique.unwrap_or(name).as_bytes()).copied()
    }

    /// A class, structure or union: its DIE now, its members once the work
    /// list gets to it. A forward reference is its definition, or when there
    /// is none, a declaration.
    fn aggregate(
        &mut self,
        index: pdb::TypeIndex,
        tag: gimli::DwTag,
        name: pdb::RawString<'_>,
        unique: Option<pdb::RawString<'_>>,
        forward: bool,
        size: u64,
    ) -> Option<UnitEntryId> {
        if forward
            && let Some(def) = self.definition(name, unique)
            && def != index
        {
            return self.die(def);
        }
        let id = self.add(tag);
        self.name(id, name.as_bytes());
        if forward {
            self.set(id, gimli::DW_AT_declaration, AttributeValue::Flag(true));
        } else {
            self.set(id, gimli::DW_AT_byte_size, AttributeValue::Udata(size));
            self.pending.push((index, id));
        }
        Some(id)
    }

    fn pointer(&mut self, tag: gimli::DwTag, to: Option<UnitEntryId>, size: u64) -> UnitEntryId {
        let id = self.add(tag);
        self.set(id, gimli::DW_AT_byte_size, AttributeValue::Udata(size));
        self.set_type(id, to);
        id
    }

    /// `const` and `volatile` around a type (`None`: void).
    fn qualified(&mut self, to: Option<UnitEntryId>, constant: bool, volatile: bool) -> Option<UnitEntryId> {
        let mut to = to;
        for (tag, on) in [
            (gimli::DW_TAG_volatile_type, volatile),
            (gimli::DW_TAG_const_type, constant),
        ] {
            if on {
                let id = self.add(tag);
                self.set_type(id, to);
                to = Some(id);
            }
        }
        to
    }

    fn base(&mut self, kind: pdb::PrimitiveKind) -> Option<UnitEntryId> {
        // Windows' error code is a typedef of a long.
        if kind == pdb::PrimitiveKind::HRESULT {
            let long = self.die(pdb::TypeIndex(0x12));
            let id = self.add(gimli::DW_TAG_typedef);
            self.set(id, gimli::DW_AT_name, AttributeValue::String(b"HRESULT".to_vec()));
            self.set_type(id, long);
            return Some(id);
        }
        let (name, encoding, size) = primitive(kind)?;
        let id = self.add(gimli::DW_TAG_base_type);
        self.set(id, gimli::DW_AT_name, AttributeValue::String(name.as_bytes().to_vec()));
        self.set(id, gimli::DW_AT_encoding, AttributeValue::Encoding(encoding));
        self.set(id, gimli::DW_AT_byte_size, AttributeValue::Udata(size));
        Some(id)
    }

    /// An array, with nested arrays (`float m[4][4]`) folded into one with a
    /// dimension each, as C compilers write them. CodeView gives sizes in
    /// bytes, so each dimension's count is a size divided by the next one's.
    fn array(&mut self, a: pdb::ArrayType) -> UnitEntryId {
        // Sizes in bytes, the whole array's first.
        let mut sizes: Vec<u64> = a.dimensions.iter().rev().map(|&d| u64::from(d)).collect();
        let mut element = a.element_type;
        for _ in 0..16 {
            match self.parse(element) {
                Some(pdb::TypeData::Array(inner)) => {
                    sizes.extend(inner.dimensions.iter().rev().map(|&d| u64::from(d)));
                    element = inner.element_type;
                }
                _ => break,
            }
        }
        let element_size = self.size_of(element);
        let to = self.die(element);
        let id = self.add(gimli::DW_TAG_array_type);
        self.set_type(id, to);
        if let Some(&total) = sizes.first() {
            self.set(id, gimli::DW_AT_byte_size, AttributeValue::Udata(total));
        }
        for (i, &size) in sizes.iter().enumerate() {
            let per = match sizes.get(i + 1) {
                Some(&next) => Some(next),
                None => element_size,
            };
            let count = match per {
                _ if size == 0 => Some(0),
                Some(per) if per > 0 && size % per == 0 => Some(size / per),
                _ => None,
            };
            let range = self.unit.add(id, gimli::DW_TAG_subrange_type);
            if let Some(n) = count {
                self.set(range, gimli::DW_AT_count, AttributeValue::Udata(n));
            }
        }
        id
    }

    /// A type's size in bytes, from its records.
    fn size_of(&self, index: pdb::TypeIndex) -> Option<u64> {
        let mut index = index;
        for _ in 0..MAX_DEPTH {
            match self.parse(index)? {
                pdb::TypeData::Primitive(p) => {
                    return match p.indirection {
                        Some(pdb::Indirection::Near64) => Some(8),
                        Some(pdb::Indirection::Near128) => Some(16),
                        Some(pdb::Indirection::Near16) => Some(2),
                        Some(pdb::Indirection::Far32) => Some(6),
                        Some(_) => Some(4),
                        None if p.kind == pdb::PrimitiveKind::HRESULT => Some(4),
                        None => primitive(p.kind).map(|(_, _, size)| size),
                    };
                }
                pdb::TypeData::Class(c) if c.properties.forward_reference() => {
                    index = self.definition(c.name, c.unique_name).filter(|&d| d != index)?;
                }
                pdb::TypeData::Class(c) => return Some(c.size),
                pdb::TypeData::Union(u) if u.properties.forward_reference() => {
                    index = self.definition(u.name, u.unique_name).filter(|&d| d != index)?;
                }
                pdb::TypeData::Union(u) => return Some(u.size),
                pdb::TypeData::Enumeration(e) => index = e.underlying_type,
                pdb::TypeData::Pointer(p) => {
                    return Some(match p.attributes.size() {
                        0 => u64::from(self.pointer_size),
                        n => u64::from(n),
                    });
                }
                pdb::TypeData::Modifier(m) => index = m.underlying_type,
                pdb::TypeData::Bitfield(b) => index = b.underlying_type,
                pdb::TypeData::Array(a) => return a.dimensions.last().map(|&d| u64::from(d)),
                _ => return None,
            }
        }
        None
    }

    /// The records of a field list and the lists it continues into.
    fn fields(&self, list: Option<pdb::TypeIndex>) -> Vec<pdb::TypeData<'t>> {
        let mut out = Vec::new();
        let mut next = list;
        for _ in 0..10_000 {
            let Some(index) = next else { break };
            let Some(pdb::TypeData::FieldList(list)) = self.parse(index) else {
                break;
            };
            out.extend(list.fields);
            next = list.continuation;
        }
        out
    }

    /// A procedure type's DIEs.
    fn procedure(
        &mut self,
        ret: Option<pdb::TypeIndex>,
        this: Option<pdb::TypeIndex>,
        args: pdb::TypeIndex,
        attributes: pdb::FunctionAttributes,
    ) -> Signature {
        let ret = ret.and_then(|r| self.die(r));
        let mut params = Vec::new();
        if let Some(t) = this {
            params.push((self.die(t), true));
        }
        let mut varargs = false;
        if let Some(pdb::TypeData::ArgumentList(list)) = self.parse(args) {
            for (i, &arg) in list.arguments.iter().enumerate() {
                // A last argument of no type is `...`.
                if arg.0 == 0 && i + 1 == list.arguments.len() {
                    varargs = true;
                } else {
                    params.push((self.die(arg), false));
                }
            }
        }
        Signature {
            ret,
            params,
            varargs,
            convention: convention(attributes),
        }
    }

    /// The signature of a procedure's type record, if it is one.
    fn signature(&mut self, index: pdb::TypeIndex) -> Option<Signature> {
        let sig = match self.parse(index)? {
            pdb::TypeData::Procedure(p) => self.procedure(p.return_type, None, p.argument_list, p.attributes),
            pdb::TypeData::MemberFunction(m) => {
                self.procedure(Some(m.return_type), m.this_pointer_type, m.argument_list, m.attributes)
            }
            _ => return None,
        };
        self.fill_pending();
        Some(sig)
    }

    fn subroutine(&mut self, sig: &Signature) -> UnitEntryId {
        let id = self.add(gimli::DW_TAG_subroutine_type);
        self.set(id, gimli::DW_AT_prototyped, AttributeValue::Flag(true));
        self.set_type(id, sig.ret);
        if let Some(cc) = sig.convention {
            self.set(
                id,
                gimli::DW_AT_calling_convention,
                AttributeValue::CallingConvention(cc),
            );
        }
        for &(ty, this) in &sig.params {
            let param = self.unit.add(id, gimli::DW_TAG_formal_parameter);
            self.set_type(param, ty);
            if this {
                self.set(param, gimli::DW_AT_artificial, AttributeValue::Flag(true));
            }
        }
        if sig.varargs {
            self.unit.add(id, gimli::DW_TAG_unspecified_parameters);
        }
        id
    }

    /// The type of a vtable pointer: `void **`.
    fn vtable_pointer(&mut self) -> UnitEntryId {
        if let Some(id) = self.vtable {
            return id;
        }
        let size = u64::from(self.pointer_size);
        let inner = self.pointer(gimli::DW_TAG_pointer_type, None, size);
        let id = self.pointer(gimli::DW_TAG_pointer_type, Some(inner), size);
        self.vtable = Some(id);
        id
    }

    /// Adds the members of the aggregates on the work list, which may add more.
    fn fill_pending(&mut self) {
        while let Some((index, id)) = self.pending.pop() {
            self.fill(index, id);
        }
    }

    /// An aggregate's members, bases, vtable pointer and static members, in
    /// the order its field list gives them.
    fn fill(&mut self, index: pdb::TypeIndex, id: UnitEntryId) {
        let list = match self.parse(index) {
            Some(pdb::TypeData::Class(c)) => c.fields,
            Some(pdb::TypeData::Union(u)) => Some(u.fields),
            _ => None,
        };
        let mut vbptr = false;
        for field in self.fields(list) {
            match field {
                pdb::TypeData::Member(m) => {
                    // A bit field's type is its storage type, and it says which bits it takes.
                    let bits = match self.parse(m.field_type) {
                        Some(pdb::TypeData::Bitfield(b)) => Some(b),
                        _ => None,
                    };
                    let member = self.unit.add(id, gimli::DW_TAG_member);
                    let ty = self.die(bits.map_or(m.field_type, |b| b.underlying_type));
                    self.name(member, m.name.as_bytes());
                    self.set_type(member, ty);
                    match bits {
                        Some(b) => {
                            self.set(member, gimli::DW_AT_bit_size, AttributeValue::Udata(b.length.into()));
                            self.set(
                                member,
                                gimli::DW_AT_data_bit_offset,
                                AttributeValue::Udata(m.offset * 8 + u64::from(b.position)),
                            );
                        }
                        None => self.set(
                            member,
                            gimli::DW_AT_data_member_location,
                            AttributeValue::Udata(m.offset),
                        ),
                    }
                }
                pdb::TypeData::StaticMember(s) => {
                    let member = self.unit.add(id, gimli::DW_TAG_member);
                    let ty = self.die(s.field_type);
                    self.name(member, s.name.as_bytes());
                    self.set_type(member, ty);
                    self.set(member, gimli::DW_AT_external, AttributeValue::Flag(true));
                    self.set(member, gimli::DW_AT_declaration, AttributeValue::Flag(true));
                }
                pdb::TypeData::BaseClass(b) => {
                    let base = self.unit.add(id, gimli::DW_TAG_inheritance);
                    let ty = self.die(b.base_class);
                    self.set_type(base, ty);
                    self.set(
                        base,
                        gimli::DW_AT_data_member_location,
                        AttributeValue::Udata(b.offset.into()),
                    );
                }
                // A virtual base sits where the most derived class puts it,
                // which only its table of offsets says; the class holds a
                // pointer to that table.
                pdb::TypeData::VirtualBaseClass(v) => {
                    let base = self.unit.add(id, gimli::DW_TAG_inheritance);
                    let ty = self.die(v.base_class);
                    self.set_type(base, ty);
                    self.set(
                        base,
                        gimli::DW_AT_virtuality,
                        AttributeValue::Virtuality(gimli::DW_VIRTUALITY_virtual),
                    );
                    if v.direct && !vbptr {
                        vbptr = true;
                        let member = self.unit.add(id, gimli::DW_TAG_member);
                        let ty = self.die(v.base_pointer);
                        self.set(member, gimli::DW_AT_name, AttributeValue::String(b"__vbptr".to_vec()));
                        self.set_type(member, ty);
                        self.set(
                            member,
                            gimli::DW_AT_data_member_location,
                            AttributeValue::Udata(v.base_pointer_offset.into()),
                        );
                        self.set(member, gimli::DW_AT_artificial, AttributeValue::Flag(true));
                    }
                }
                // The class's own vtable pointer, first in the object.
                pdb::TypeData::VirtualFunctionTablePointer(_) => {
                    let member = self.unit.add(id, gimli::DW_TAG_member);
                    let ty = self.vtable_pointer();
                    self.set(member, gimli::DW_AT_name, AttributeValue::String(b"__vfptr".to_vec()));
                    self.set_type(member, Some(ty));
                    self.set(member, gimli::DW_AT_data_member_location, AttributeValue::Udata(0));
                    self.set(member, gimli::DW_AT_artificial, AttributeValue::Flag(true));
                }
                // Nested types are types of their own, named in full (`Outer::Inner`).
                pdb::TypeData::Nested(n) => {
                    self.die(n.nested_type);
                }
                _ => {}
            }
        }
    }

    /// A typedef (from an `S_UDT` record). In C++, where every class has one
    /// of its own name, one that only repeats the name of the class, union
    /// or enum it names is left out; in C it is what lets code say `size2_t`
    /// rather than `struct size2_t`.
    fn typedef(&mut self, name: &[u8], index: pdb::TypeIndex, cplusplus: bool) {
        let own = match self.parse(index) {
            Some(pdb::TypeData::Class(c)) => Some(c.name),
            Some(pdb::TypeData::Union(u)) => Some(u.name),
            Some(pdb::TypeData::Enumeration(e)) => Some(e.name),
            _ => None,
        };
        if (cplusplus && own.is_some_and(|n| n.as_bytes() == name)) || name.is_empty() {
            return;
        }
        let ty = self.die(index);
        let id = self.add(gimli::DW_TAG_typedef);
        self.set(id, gimli::DW_AT_name, AttributeValue::String(name.to_vec()));
        self.set_type(id, ty);
        self.fill_pending();
    }

    /// The unit, once everything is in it (`None` if nothing is).
    fn finish(mut self, lang: gimli::DwLang) -> Option<Unit> {
        self.fill_pending();
        let root = self.unit.root();
        self.unit.get(root).children().next()?;
        let e = self.unit.get_mut(root);
        e.set(gimli::DW_AT_name, AttributeValue::String(b"(types)".to_vec()));
        e.set(
            gimli::DW_AT_producer,
            AttributeValue::String(b"binviz, from the PDB's type records".to_vec()),
        );
        e.set(gimli::DW_AT_language, AttributeValue::Language(lang));
        Some(self.unit)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_same_every_time() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/bin/pdbdemo.pdb");
        let data = std::fs::read(path).unwrap();
        let first = super::convert(&data, 0x1_4000_0000, 8).unwrap();
        assert_eq!(first.modules, 2);
        for _ in 0..4 {
            let again = super::convert(&data, 0x1_4000_0000, 8).unwrap();
            assert_eq!(again.sections, first.sections);
            assert_eq!(again.symbols, first.symbols);
        }
    }

    #[test]
    fn compilers_names_for_anonymous_types() {
        for name in [
            "<unnamed-tag>",
            "S::<unnamed-type-u>",
            "__unnamed",
            "<anonymous-struct>",
            "",
        ] {
            assert!(super::is_anonymous(name.as_bytes()), "{name}");
        }
        for name in ["Shape", "std::vector<int,std::allocator<int> >", "enum2$<Option<u8> >"] {
            assert!(!super::is_anonymous(name.as_bytes()), "{name}");
        }
    }
}
