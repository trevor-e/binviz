//! Debug info that arrives in another form (a PDB, an assembler's debug file),
//! written out as DWARF, so that everything binviz shows from DWARF shows
//! from it too: each module a compilation unit with its line program, its
//! functions subprograms (with their parameters, locals and blocks) and its
//! globals variables; types, when there are any, in a unit of their own that
//! the others refer to.

use std::collections::BTreeMap;

use gimli::write::{
    Address, AttributeValue, Dwarf, EndianVec, Expression, LineProgram, LineString, Reference, Sections, Unit,
    UnitEntryId, UnitId,
};
use gimli::{Encoding, Format, LineEncoding, RunTimeEndian, SectionId};

use crate::error::{Error, Result};

/// A line record: where its code is, and the line it came from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Row {
    pub address: u64,
    pub end: u64,
    /// A key into [`Module::files`].
    pub file: u32,
    pub line: u32,
}

/// A type, by its place in the list given to [`write`].
pub(crate) type TyId = usize;

/// A type to write.
#[derive(Debug, Clone)]
pub(crate) enum Ty {
    Base {
        name: String,
        size: u64,
        encoding: gimli::DwAte,
    },
    /// A pointer (or C++ reference) to a type, or to `void` (`to` None).
    Pointer {
        to: Option<TyId>,
        size: u64,
        reference: bool,
    },
    Const(Option<TyId>),
    Volatile(Option<TyId>),
    Array {
        of: TyId,
        count: Option<u64>,
    },
    /// A struct, class or union (`tag`) and its members; a `declaration` when only its name is known.
    Record {
        tag: gimli::DwTag,
        name: String,
        size: u64,
        members: Vec<Member>,
        declaration: bool,
    },
    Enum {
        name: String,
        size: u64,
        of: Option<TyId>,
        values: Vec<(String, i64)>,
    },
    Function {
        returns: Option<TyId>,
        params: Vec<Option<TyId>>,
    },
}

/// A member of a struct, class or union, or a base class it derives from.
#[derive(Debug, Clone)]
pub(crate) struct Member {
    pub name: String,
    pub ty: Option<TyId>,
    pub offset: u64,
    /// A bitfield's (bits, first bit).
    pub bits: Option<(u8, u8)>,
    pub base: bool,
}

/// Where a variable's value is (register numbers are DWARF's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Loc {
    Addr(u64),
    Reg(u16),
    /// At a register's value plus an offset (the stack, the frame).
    RegOffset(u16, i64),
}

#[derive(Debug, Clone)]
pub(crate) struct Var {
    pub name: String,
    pub ty: Option<TyId>,
    pub location: Option<Loc>,
    pub param: bool,
}

/// A lexical block: its code, the variables declared in it, the blocks in it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Block {
    pub address: u64,
    pub size: u64,
    pub vars: Vec<Var>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Function {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub external: bool,
    pub returns: Option<TyId>,
    /// Its parameters and locals, and the blocks inside it.
    pub scope: Block,
}

#[derive(Debug, Clone)]
pub(crate) struct Global {
    pub name: String,
    pub address: u64,
    pub external: bool,
    pub ty: Option<TyId>,
}

/// One compilation unit's worth.
#[derive(Debug, Default)]
pub(crate) struct Module {
    /// Its name where it came from (an object file), when no source names it.
    pub name: String,
    /// Its main source file, when known; else the one most of its lines are in.
    pub main: Option<String>,
    pub producer: String,
    pub language: Option<gimli::DwLang>,
    /// Source files, by the key rows use.
    pub files: BTreeMap<u32, String>,
    pub rows: Vec<Row>,
    pub functions: Vec<Function>,
    pub variables: Vec<Global>,
}

impl Module {
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty() && self.variables.is_empty() && self.rows.is_empty()
    }
}

/// A `DW_OP` expression for a location.
fn expression(loc: Loc) -> Expression {
    let mut e = Expression::new();
    match loc {
        Loc::Addr(a) => e.op_addr(Address::Constant(a)),
        Loc::Reg(r) => e.op_reg(gimli::Register(r)),
        Loc::RegOffset(r, o) => e.op_breg(gimli::Register(r), o),
    }
    e
}

fn set_name(unit: &mut Unit, id: UnitEntryId, name: &str) {
    if !name.is_empty() {
        unit.get_mut(id)
            .set(gimli::DW_AT_name, AttributeValue::String(name.as_bytes().to_vec()));
    }
}

/// Writes the types into `unit`; each one's DIE.
fn write_types(unit: &mut Unit, types: &[Ty]) -> Vec<UnitEntryId> {
    let root = unit.root();
    // A DIE for each first, so that they can refer to each other in any order.
    let ids: Vec<UnitEntryId> = types
        .iter()
        .map(|t| {
            let tag = match t {
                Ty::Base { .. } => gimli::DW_TAG_base_type,
                Ty::Pointer { reference: true, .. } => gimli::DW_TAG_reference_type,
                Ty::Pointer { .. } => gimli::DW_TAG_pointer_type,
                Ty::Const(_) => gimli::DW_TAG_const_type,
                Ty::Volatile(_) => gimli::DW_TAG_volatile_type,
                Ty::Array { .. } => gimli::DW_TAG_array_type,
                Ty::Record { tag, .. } => *tag,
                Ty::Enum { .. } => gimli::DW_TAG_enumeration_type,
                Ty::Function { .. } => gimli::DW_TAG_subroutine_type,
            };
            unit.add(root, tag)
        })
        .collect();
    let set_type = |unit: &mut Unit, entry: UnitEntryId, t: Option<TyId>| {
        if let Some(&target) = t.and_then(|t| ids.get(t)) {
            unit.get_mut(entry)
                .set(gimli::DW_AT_type, AttributeValue::UnitRef(target));
        }
    };
    for (t, &id) in types.iter().zip(&ids) {
        match t {
            Ty::Base { name, size, encoding } => {
                set_name(unit, id, name);
                let e = unit.get_mut(id);
                e.set(gimli::DW_AT_byte_size, AttributeValue::Udata(*size));
                e.set(gimli::DW_AT_encoding, AttributeValue::Encoding(*encoding));
            }
            Ty::Pointer { to, size, .. } => {
                unit.get_mut(id)
                    .set(gimli::DW_AT_byte_size, AttributeValue::Udata(*size));
                set_type(unit, id, *to);
            }
            Ty::Const(of) | Ty::Volatile(of) => set_type(unit, id, *of),
            Ty::Array { of, count } => {
                set_type(unit, id, Some(*of));
                let range = unit.add(id, gimli::DW_TAG_subrange_type);
                if let Some(n) = count {
                    unit.get_mut(range).set(gimli::DW_AT_count, AttributeValue::Udata(*n));
                }
            }
            Ty::Record {
                name,
                size,
                members,
                declaration,
                ..
            } => {
                set_name(unit, id, name);
                if *declaration {
                    unit.get_mut(id)
                        .set(gimli::DW_AT_declaration, AttributeValue::Flag(true));
                    continue;
                }
                unit.get_mut(id)
                    .set(gimli::DW_AT_byte_size, AttributeValue::Udata(*size));
                for m in members {
                    let tag = if m.base {
                        gimli::DW_TAG_inheritance
                    } else {
                        gimli::DW_TAG_member
                    };
                    let child = unit.add(id, tag);
                    set_name(unit, child, &m.name);
                    set_type(unit, child, m.ty);
                    let e = unit.get_mut(child);
                    match m.bits {
                        Some((bits, first)) => {
                            e.set(gimli::DW_AT_bit_size, AttributeValue::Udata(u64::from(bits)));
                            e.set(
                                gimli::DW_AT_data_bit_offset,
                                AttributeValue::Udata(m.offset * 8 + u64::from(first)),
                            );
                        }
                        None => e.set(gimli::DW_AT_data_member_location, AttributeValue::Udata(m.offset)),
                    }
                }
            }
            Ty::Enum { name, size, of, values } => {
                set_name(unit, id, name);
                unit.get_mut(id)
                    .set(gimli::DW_AT_byte_size, AttributeValue::Udata(*size));
                set_type(unit, id, *of);
                for (v, value) in values {
                    let child = unit.add(id, gimli::DW_TAG_enumerator);
                    set_name(unit, child, v);
                    unit.get_mut(child)
                        .set(gimli::DW_AT_const_value, AttributeValue::Sdata(*value));
                }
            }
            Ty::Function { returns, params } => {
                unit.get_mut(id)
                    .set(gimli::DW_AT_prototyped, AttributeValue::Flag(true));
                set_type(unit, id, *returns);
                for p in params {
                    let child = unit.add(id, gimli::DW_TAG_formal_parameter);
                    set_type(unit, child, *p);
                }
            }
        }
    }
    ids
}

/// A block's variables and blocks, under `parent`.
fn write_scope(
    unit: &mut Unit,
    parent: UnitEntryId,
    b: &Block,
    refer: &dyn Fn(Option<TyId>) -> Option<AttributeValue>,
) {
    for v in &b.vars {
        let tag = if v.param {
            gimli::DW_TAG_formal_parameter
        } else {
            gimli::DW_TAG_variable
        };
        let id = unit.add(parent, tag);
        set_name(unit, id, &v.name);
        let e = unit.get_mut(id);
        if let Some(t) = refer(v.ty) {
            e.set(gimli::DW_AT_type, t);
        }
        if let Some(loc) = v.location {
            e.set(gimli::DW_AT_location, AttributeValue::Exprloc(expression(loc)));
        }
    }
    for inner in &b.blocks {
        let id = unit.add(parent, gimli::DW_TAG_lexical_block);
        let e = unit.get_mut(id);
        e.set(
            gimli::DW_AT_low_pc,
            AttributeValue::Address(Address::Constant(inner.address)),
        );
        e.set(gimli::DW_AT_high_pc, AttributeValue::Udata(inner.size.max(1)));
        write_scope(unit, id, inner, refer);
    }
}

/// The DWARF sections describing `modules`, whose types are `types`; `from`
/// says where they came from (for errors).
pub(crate) fn write(modules: Vec<Module>, types: &[Ty], from: &str) -> Result<Vec<(SectionId, Vec<u8>)>> {
    let encoding = Encoding {
        format: Format::Dwarf32,
        version: 4,
        address_size: 8,
    };
    let mut dwarf = Dwarf::new();
    // The types, in a unit of their own the others refer to.
    let mut type_ids: Vec<UnitEntryId> = Vec::new();
    let mut types_unit: Option<UnitId> = None;
    if !types.is_empty() {
        let id = dwarf.units.add(Unit::new(encoding, LineProgram::none()));
        let unit = dwarf.units.get_mut(id);
        let root = unit.root();
        unit.get_mut(root).set(
            gimli::DW_AT_name,
            AttributeValue::String(format!("(types from the {from})").into_bytes()),
        );
        type_ids = write_types(unit, types);
        types_unit = Some(id);
    }
    let refer = |t: Option<TyId>| -> Option<AttributeValue> {
        Some(AttributeValue::DebugInfoRef(Reference::Entry(
            types_unit?,
            *type_ids.get(t?)?,
        )))
    };
    for mut m in modules.into_iter().filter(|m| !m.is_empty()) {
        m.rows.sort_by_key(|r| r.address);
        // The unit is named after its main source: the file most of its lines
        // are in (of two, the one listed first).
        let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
        for r in &m.rows {
            *counts.entry(r.file).or_default() += 1;
        }
        let main = m.main.clone().or_else(|| {
            counts
                .iter()
                .max_by_key(|&(f, n)| (*n, std::cmp::Reverse(*f)))
                .and_then(|(f, _)| m.files.get(f).cloned())
        });
        let unit_name = main.unwrap_or_else(|| m.name.clone());
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
        for (key, name) in &m.files {
            let name = if name.is_empty() { "?".to_string() } else { name.clone() };
            ids.insert(*key, program.add_file(LineString::String(name.into_bytes()), dir, None));
        }
        // Runs of contiguous code, each a sequence.
        let mut i = 0;
        while i < m.rows.len() {
            let start = m.rows[i].address;
            program.begin_sequence(Some(Address::Constant(start)));
            let mut end = start;
            while i < m.rows.len() && m.rows[i].address <= end.max(start) {
                let r = m.rows[i];
                if let Some(&file) = ids.get(&r.file) {
                    program.row().address_offset = r.address - start;
                    program.row().file = file;
                    program.row().line = u64::from(r.line);
                    program.generate_row();
                }
                end = end.max(r.end).max(r.address + 1);
                i += 1;
            }
            program.end_sequence(end - start);
        }
        let mut unit = Unit::new(encoding, program);
        let root = unit.root();
        let low = m
            .functions
            .iter()
            .map(|f| f.address)
            .chain(m.rows.first().map(|r| r.address))
            .min();
        let high = m
            .functions
            .iter()
            .map(|f| f.address + f.size)
            .chain(m.rows.iter().map(|r| r.end))
            .max();
        {
            let e = unit.get_mut(root);
            e.set(gimli::DW_AT_name, AttributeValue::String(unit_name.into_bytes()));
            e.set(gimli::DW_AT_producer, AttributeValue::String(m.producer.into_bytes()));
            e.set(
                gimli::DW_AT_language,
                AttributeValue::Language(m.language.unwrap_or(gimli::DW_LANG_C)),
            );
            e.set(gimli::DW_AT_stmt_list, AttributeValue::LineProgramRef);
            if let (Some(lo), Some(hi)) = (low, high) {
                e.set(gimli::DW_AT_low_pc, AttributeValue::Address(Address::Constant(lo)));
                e.set(gimli::DW_AT_high_pc, AttributeValue::Udata(hi.saturating_sub(lo)));
            }
        }
        for f in &m.functions {
            let id = unit.add(root, gimli::DW_TAG_subprogram);
            set_name(&mut unit, id, &f.name);
            let e = unit.get_mut(id);
            e.set(
                gimli::DW_AT_low_pc,
                AttributeValue::Address(Address::Constant(f.address)),
            );
            e.set(gimli::DW_AT_high_pc, AttributeValue::Udata(f.size.max(1)));
            e.set(gimli::DW_AT_external, AttributeValue::Flag(f.external));
            if let Some(t) = refer(f.returns) {
                e.set(gimli::DW_AT_type, t);
            }
            write_scope(&mut unit, id, &f.scope, &refer);
        }
        for g in &m.variables {
            let id = unit.add(root, gimli::DW_TAG_variable);
            set_name(&mut unit, id, &g.name);
            let e = unit.get_mut(id);
            e.set(
                gimli::DW_AT_location,
                AttributeValue::Exprloc(expression(Loc::Addr(g.address))),
            );
            e.set(gimli::DW_AT_external, AttributeValue::Flag(g.external));
            if let Some(t) = refer(g.ty) {
                e.set(gimli::DW_AT_type, t);
            }
        }
        dwarf.units.add(unit);
    }
    let mut sections = Sections::new(EndianVec::new(RunTimeEndian::Little));
    dwarf
        .write(&mut sections)
        .map_err(|e| Error::new(format!("writing the {from}'s DWARF: {e}")))?;
    let mut out = Vec::new();
    sections
        .for_each(|id, data| {
            out.push((id, data.slice().to_vec()));
            Ok::<(), gimli::write::Error>(())
        })
        .map_err(|e| Error::new(format!("writing the {from}'s DWARF: {e}")))?;
    Ok(out)
}
