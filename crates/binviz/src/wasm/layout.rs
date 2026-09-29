//! The layout of a WebAssembly module: its header, then every section with
//! its id, its size and what it holds. The entries of a section (types,
//! imports, bodies, data segments, names…) are decoded on demand
//! ([`Decoder::Wasm`]), so a module with a hundred thousand functions costs
//! no more nodes than one with ten; hovering a byte names the entry and
//! the field it is in.

use super::read::{
    self, Body, Const, Element, Export, Global, Import, ImportDesc, Limits, Mode, Module, Reader, Sec, Segment, Table,
    name_subsection, read_type_entry, section_name,
};
use super::{MEMORY_BASE, custom_kind, import_text};
use crate::layout::decode::Entry;
use crate::layout::fields::FieldValue;
use crate::layout::{Builder, Ctx, Decoder, Node};
use crate::model::RegionKind;
use crate::util::{self, hex};

/// A run of entries of one kind, decoded on demand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Vector {
    Types,
    Imports,
    Functions,
    Tables,
    Memories,
    Globals,
    Exports,
    Elements,
    Bodies,
    Data,
    Tags,
    /// A name map of the name section; the subsection's id says what it names.
    Names(u8),
    /// An indirect name map: local (or label, or field) names per function (or type).
    IndirectNames(u8),
    Producers,
    Features,
    /// The subsections of `linking` or `dylink.0`.
    Subsections,
    Relocations,
}

fn field(start: u64, end: u64, name: &'static str, raw: u64, value: String) -> FieldValue {
    FieldValue {
        start,
        end,
        name,
        raw,
        value,
    }
}

/// A LEB128 number at `pos` as a field, and where it ends.
fn number(
    data: &[u8],
    pos: u64,
    end: u64,
    name: &'static str,
    text: impl Fn(u64) -> String,
) -> Option<(FieldValue, u64)> {
    let mut r = Reader::new(data, pos, end);
    let v = r.u64()?;
    Some((field(pos, r.pos(), name, v, text(v)), r.pos()))
}

/// A name (its length, then its bytes) at `pos` as a field, and where it ends.
fn name(data: &[u8], pos: u64, end: u64, label: &'static str) -> Option<(FieldValue, u64)> {
    let mut r = Reader::new(data, pos, end);
    let text = util::quote(r.name()?);
    Some((field(pos, r.pos(), label, 0, text), r.pos()))
}

fn sizes(n: u64) -> String {
    if n == 1 { "1 byte".into() } else { format!("{n} bytes") }
}

fn section_title(s: &Sec) -> String {
    match s.id {
        read::CUSTOM => format!("Custom section \"{}\"", s.name),
        read::DATA_COUNT => "Data count section".into(),
        id if id <= read::TAG => {
            let n = section_name(id);
            format!("{}{} section", n[..1].to_uppercase(), &n[1..])
        }
        id => format!("Section {id}"),
    }
}

/// What each section is for, in a sentence.
fn section_note(s: &Sec) -> Option<&'static str> {
    Some(match s.id {
        read::TYPE => "The function signatures (and GC types) the module uses, by type index",
        read::IMPORT => "What the host provides: functions, a memory, a table, globals, each by module and field name",
        read::FUNCTION => "Each defined function's type; their bodies are in the code section, in the same order",
        read::TABLE => "Tables of references: the function table is what call_indirect calls through",
        read::MEMORY => "Linear memory: its size in pages, initially and at most",
        read::GLOBAL => "Global variables of WebAssembly itself (the stack pointer, say), each with its initial value",
        read::EXPORT => "What the host can reach: functions, the memory, the table, globals, by name",
        read::START => "The function run when the module is instantiated",
        read::ELEMENT => "Element segments: the functions written into a table, reachable through call_indirect",
        read::DATA_COUNT => "How many data segments there are, so the code can refer to them before they are read",
        read::CODE => "The functions' code: each body's size, its locals, then its instructions",
        read::DATA => {
            "Data segments: bytes written into linear memory at instantiation (active) or by memory.init (passive)"
        }
        read::TAG => "Exception tags, each with the type of the values it throws",
        _ => match s.name.as_str() {
            "name" => "Names for functions, locals, globals and data segments, for debuggers and stack traces",
            "producers" => "The languages and tools that made the module",
            "target_features" => "The WebAssembly features the code was compiled for",
            "sourceMappingURL" => "Where the module's source map is",
            "external_debug_info" => "Where the module's DWARF is, kept in a separate file",
            "build_id" => "An identifier of this build",
            "linking" => "An object file's symbol table and segment information, for the linker",
            n if n.starts_with("reloc.") => "Relocations for the linker: places in a section that name a symbol",
            n if n.starts_with(".debug") => {
                "DWARF debug info; its code addresses count from the code section's contents"
            }
            _ => return None,
        },
    })
}

/// Lays out the whole module.
pub(crate) fn build(b: &mut Builder<'_>) {
    let Some(m) = b.ctx.wasm else { return };
    if let Some(h) = b.region(0, 8, RegionKind::Header, "WebAssembly module header") {
        b.add_field_nodes(
            h,
            &[
                field(0, 4, "Magic", 0, "\"\\0asm\"".into()),
                field(4, 8, "Version", 1, "1".into()),
            ],
        );
    }
    for s in &m.sections {
        section(b, m, s);
    }
}

fn section(b: &mut Builder<'_>, m: &Module, s: &Sec) {
    let data = b.ctx.bytes.data;
    let kind = match s.id {
        read::CODE => RegionKind::Code,
        read::DATA => RegionKind::Data,
        read::IMPORT | read::EXPORT => RegionKind::Linking,
        read::CUSTOM => custom_kind(&s.name),
        _ => RegionKind::Metadata,
    };
    let Some(node) = b.region(s.start, s.end - s.start, kind, section_title(s)) else {
        return;
    };
    if let Some(note) = section_note(s) {
        b.set_note(Some(node), note);
    }
    // The data section is its segments' sections; the others are one each.
    if s.id != read::DATA
        && let Some(i) = b
            .ctx
            .sections
            .iter()
            .position(|x| x.file_offset == Some(s.content) && x.name == s.name)
    {
        b.set_section(Some(node), i as u32);
    }
    let mut fields = vec![
        field(
            s.start,
            s.start + 1,
            "Section id",
            u64::from(s.id),
            format!("{} ({})", s.id, section_name(s.id)),
        ),
        field(
            s.start + 1,
            s.payload,
            "Section size",
            s.end - s.payload,
            sizes(s.end - s.payload),
        ),
    ];
    if s.id == read::CUSTOM {
        fields.push(field(s.payload, s.content, "Name", 0, util::quote(s.name.as_bytes())));
    }
    let vector = match s.id {
        read::TYPE => Some((Vector::Types, "Types")),
        read::IMPORT => Some((Vector::Imports, "Imports")),
        read::FUNCTION => Some((Vector::Functions, "Function types")),
        read::TABLE => Some((Vector::Tables, "Tables")),
        read::MEMORY => Some((Vector::Memories, "Memories")),
        read::GLOBAL => Some((Vector::Globals, "Globals")),
        read::EXPORT => Some((Vector::Exports, "Exports")),
        read::ELEMENT => Some((Vector::Elements, "Element segments")),
        read::CODE => Some((Vector::Bodies, "Function bodies")),
        read::DATA => Some((Vector::Data, "Data segments")),
        read::TAG => Some((Vector::Tags, "Tags")),
        _ => None,
    };
    if let Some((v, title)) = vector {
        entries(b, node, &mut fields, s.content, s.end, kind, v, title);
    } else {
        match s.id {
            read::START => {
                if let Some((mut f, _)) = number(data, s.content, s.end, "Start function", |v| v.to_string()) {
                    if let Some(n) = m.function_address(f.raw as u32).and_then(|a| b.ctx.symbolize(a)) {
                        f.value = format!("{} ({n})", f.raw);
                    }
                    fields.push(f);
                }
            }
            read::DATA_COUNT => {
                if let Some((f, _)) = number(data, s.content, s.end, "Data segments", |v| v.to_string()) {
                    fields.push(f);
                }
            }
            read::CUSTOM => custom(b, node, &mut fields, s),
            _ => {
                b.child(node, s.content, s.end - s.content, kind, "Contents");
            }
        }
    }
    b.add_field_nodes(node, &fields);
}

/// A vector's count (added to `fields`), then its entries as a child
/// region decoded on demand.
#[allow(clippy::too_many_arguments)]
fn entries(
    b: &mut Builder<'_>,
    parent: u32,
    fields: &mut Vec<FieldValue>,
    start: u64,
    end: u64,
    kind: RegionKind,
    v: Vector,
    title: &str,
) {
    let data = b.ctx.bytes.data;
    let Some((count, at)) = number(data, start, end, "Count", |v| v.to_string()) else {
        return;
    };
    let n = count.raw;
    fields.push(count);
    if let Some(id) = b.child(parent, at, end - at, kind, title) {
        b.set_decoder(Some(id), Decoder::Wasm(v));
        b.set_value(Some(id), n.to_string());
    }
}

fn custom(b: &mut Builder<'_>, node: u32, fields: &mut Vec<FieldValue>, s: &Sec) {
    let data = b.ctx.bytes.data;
    let (start, end) = (s.content, s.end);
    let kind = custom_kind(&s.name);
    match s.name.as_str() {
        n if n.starts_with(".debug") => {
            if let Some(id) = b.child(node, start, end - start, kind, format!("{n} contents")) {
                if let Some(d) = Decoder::for_dwarf_section(n) {
                    b.set_decoder(Some(id), d);
                }
                if matches!(
                    n,
                    ".debug_info" | ".debug_line" | ".debug_ranges" | ".debug_loc" | ".debug_addr"
                ) {
                    b.set_note(
                        Some(id),
                        "Addresses in it read as binviz places them: code at file offsets (DWARF counts from the code section's contents), memory at 0x80000000 and up",
                    );
                }
            }
        }
        "name" => {
            let mut pos = start;
            while pos < end {
                let mut r = Reader::new(data, pos, end);
                let (Some(id), Some(size)) = (r.u8(), r.u32()) else {
                    break;
                };
                let body = r.pos();
                let sub_end = (body + u64::from(size)).min(end);
                let Some(sub) = b.child(
                    node,
                    pos,
                    sub_end - pos,
                    kind,
                    format!("Subsection {id}: {}", name_subsection(id)),
                ) else {
                    break;
                };
                let mut f = vec![
                    field(
                        pos,
                        pos + 1,
                        "Subsection id",
                        u64::from(id),
                        format!("{id} ({})", name_subsection(id)),
                    ),
                    field(
                        pos + 1,
                        body,
                        "Subsection size",
                        u64::from(size),
                        sizes(u64::from(size)),
                    ),
                ];
                match id {
                    0 => {
                        if let Some((n, _)) = name(data, body, sub_end, "Module name") {
                            f.push(n);
                        }
                    }
                    2 | 3 | 10 => entries(b, sub, &mut f, body, sub_end, kind, Vector::IndirectNames(id), "Names"),
                    1 | 4..=9 | 11 => entries(b, sub, &mut f, body, sub_end, kind, Vector::Names(id), "Names"),
                    _ => {}
                }
                b.add_field_nodes(sub, &f);
                pos = sub_end;
            }
        }
        "producers" => entries(b, node, fields, start, end, kind, Vector::Producers, "Fields"),
        "target_features" => entries(b, node, fields, start, end, kind, Vector::Features, "Features"),
        "sourceMappingURL" | "external_debug_info" => {
            if let Some((f, _)) = name(data, start, end, "URL") {
                fields.push(f);
            }
        }
        "build_id" => {
            let mut r = Reader::new(data, start, end);
            if let Some(id) = r.name() {
                fields.push(field(start, r.pos(), "Build ID", 0, util::hex_compact(id)));
            }
        }
        "linking" => {
            if let Some((f, at)) = number(data, start, end, "Version", |v| v.to_string()) {
                fields.push(f);
                if let Some(id) = b.child(node, at, end - at, kind, "Subsections") {
                    b.set_decoder(Some(id), Decoder::Wasm(Vector::Subsections));
                }
            }
        }
        n if n.starts_with("reloc.") => {
            if let Some((f, at)) = number(data, start, end, "Section", |v| format!("section {v}")) {
                fields.push(f);
                entries(b, node, fields, at, end, kind, Vector::Relocations, "Relocations");
            }
        }
        n if n.starts_with("dylink") => {
            if let Some(id) = b.child(node, start, end - start, kind, "Subsections") {
                b.set_decoder(Some(id), Decoder::Wasm(Vector::Subsections));
            }
        }
        _ => {
            b.child(node, start, end - start, kind, "Contents");
        }
    }
}

/// Relocation types that carry an addend.
fn has_addend(t: u8) -> bool {
    matches!(t, 3 | 4 | 5 | 8 | 9 | 11 | 14 | 15 | 16 | 17 | 21 | 22 | 23 | 25)
}

fn relocation_name(t: u8) -> &'static str {
    const NAMES: [&str; 27] = [
        "R_WASM_FUNCTION_INDEX_LEB",
        "R_WASM_TABLE_INDEX_SLEB",
        "R_WASM_TABLE_INDEX_I32",
        "R_WASM_MEMORY_ADDR_LEB",
        "R_WASM_MEMORY_ADDR_SLEB",
        "R_WASM_MEMORY_ADDR_I32",
        "R_WASM_TYPE_INDEX_LEB",
        "R_WASM_GLOBAL_INDEX_LEB",
        "R_WASM_FUNCTION_OFFSET_I32",
        "R_WASM_SECTION_OFFSET_I32",
        "R_WASM_TAG_INDEX_LEB",
        "R_WASM_MEMORY_ADDR_REL_SLEB",
        "R_WASM_TABLE_INDEX_REL_SLEB",
        "R_WASM_GLOBAL_INDEX_I32",
        "R_WASM_MEMORY_ADDR_LEB64",
        "R_WASM_MEMORY_ADDR_SLEB64",
        "R_WASM_MEMORY_ADDR_I64",
        "R_WASM_MEMORY_ADDR_REL_SLEB64",
        "R_WASM_TABLE_INDEX_SLEB64",
        "R_WASM_TABLE_INDEX_I64",
        "R_WASM_TABLE_NUMBER_LEB",
        "R_WASM_MEMORY_ADDR_TLS_SLEB",
        "R_WASM_FUNCTION_OFFSET_I64",
        "R_WASM_MEMORY_ADDR_LOCREL_I32",
        "R_WASM_TABLE_INDEX_REL_SLEB64",
        "R_WASM_MEMORY_ADDR_TLS_SLEB64",
        "R_WASM_FUNCTION_INDEX_I32",
    ];
    NAMES.get(t as usize).copied().unwrap_or("unknown")
}

/// How long the entry at `pos` is, read without describing it.
pub(crate) fn entry_len(ctx: &Ctx, v: Vector, pos: u64, end: u64) -> Option<u64> {
    let mut r = Reader::new(ctx.bytes.data, pos, end);
    match v {
        Vector::Types => {
            read_type_entry(&mut r)?;
        }
        Vector::Imports => {
            Import::read(&mut r)?;
        }
        Vector::Functions => {
            r.u32()?;
        }
        Vector::Tables => {
            Table::read(&mut r)?;
        }
        Vector::Memories => {
            Limits::read(&mut r)?;
        }
        Vector::Globals => {
            Global::read(&mut r)?;
        }
        Vector::Exports => {
            Export::read(&mut r)?;
        }
        Vector::Elements => {
            Element::read(&mut r)?;
        }
        Vector::Bodies => {
            Body::read(&mut r)?;
        }
        Vector::Data => {
            Segment::read(&mut r)?;
        }
        Vector::Tags => {
            r.u8()?;
            r.u32()?;
        }
        Vector::Names(_) => {
            r.u32()?;
            r.name()?;
        }
        Vector::IndirectNames(_) => {
            r.u32()?;
            for _ in 0..r.count()? {
                r.u32()?;
                r.name()?;
            }
        }
        Vector::Producers => {
            r.name()?;
            for _ in 0..r.count()? {
                r.name()?;
                r.name()?;
            }
        }
        Vector::Features => {
            r.u8()?;
            r.name()?;
        }
        Vector::Subsections => {
            r.u8()?;
            let n = r.u32()?;
            r.skip(u64::from(n))?;
        }
        Vector::Relocations => {
            let t = r.u8()?;
            r.u32()?;
            r.u32()?;
            if has_addend(t) {
                r.sleb(64)?;
            }
        }
    }
    (r.pos() > pos).then(|| r.pos() - pos)
}

/// The function whose body or entry is at `address`, by name.
fn named(ctx: &Ctx, address: Option<u64>) -> Option<String> {
    ctx.symbolize(address?)
}

/// `123 (name)`, or `123`.
fn with_name(index: impl std::fmt::Display, name: Option<String>) -> String {
    match name {
        Some(n) => format!("{index} ({n})"),
        None => index.to_string(),
    }
}

/// A const expression at `r` as a field.
fn expr_field(r: &mut Reader, name: &'static str) -> Option<(FieldValue, Const)> {
    let start = r.pos();
    let c = Const::read(r)?;
    Some((field(start, r.pos(), name, 0, c.to_string()), c))
}

/// Describes the entry at `pos`, the `index`th of its vector.
pub(crate) fn entry(ctx: &Ctx, node: &Node, v: Vector, pos: u64, index: usize) -> Option<Entry> {
    let data = ctx.bytes.data;
    let m = ctx.wasm?;
    let end = node.end;
    let mut r = Reader::new(data, pos, end);
    let index = index as u32;
    let mut e = Entry {
        start: pos,
        ..Entry::default()
    };
    match v {
        Vector::Types => {
            let types = read_type_entry(&mut r)?;
            match types.as_slice() {
                [one] => {
                    e.name = format!("Type {index}");
                    e.value = Some(one.as_ref().map_or("struct or array type".into(), ToString::to_string));
                    e.note = m.names.types.get(&index).cloned();
                }
                many => {
                    e.name = "Recursive type group".into();
                    e.value = Some(format!("{} types", many.len()));
                }
            }
        }
        Vector::Imports => {
            let (module, at) = name(data, pos, end, "Module")?;
            let (fieldname, at) = name(data, at, end, "Field")?;
            let mut rd = Reader::new(data, at, end);
            let kind = rd.u8()?;
            let desc_start = rd.pos();
            let imp = Import::read(&mut r)?;
            let desc_end = r.pos();
            let what = import_text(m, &imp.desc);
            e.name = format!("Import {index}: {}.{}", imp.module, imp.field);
            e.value = Some(match (&imp.desc, named(ctx, Some(pos))) {
                (ImportDesc::Func(_) | ImportDesc::Global(..), Some(n))
                    if n != format!("{}.{}", imp.module, imp.field) =>
                {
                    format!("{what}, named {n}")
                }
                _ => what.clone(),
            });
            e.fields = vec![
                module,
                fieldname,
                field(at, desc_start, "Kind", u64::from(kind), imp.desc.kind().name().into()),
                field(desc_start, desc_end, "Description", 0, what),
            ];
        }
        Vector::Functions => {
            let t = r.u32()?;
            let f = m.imported_funcs + index;
            e.name = format!("Function {f}");
            let sig = m
                .types
                .get(t as usize)
                .and_then(Option::as_ref)
                .map(ToString::to_string);
            e.value = Some(match sig {
                Some(s) => format!("type {t}: {s}"),
                None => format!("type {t}"),
            });
            e.note = named(ctx, m.function_address(f));
        }
        Vector::Tables => {
            let t = Table::read(&mut r)?;
            let n = m.tables.iter().filter(|t| t.import.is_some()).count() as u32 + index;
            e.name = format!("Table {n}");
            e.value = Some(format!(
                "{}, {} entries{}",
                t.ty,
                t.limits.min,
                t.limits.max.map(|x| format!(", at most {x}")).unwrap_or_default()
            ));
        }
        Vector::Memories => {
            let l = Limits::read(&mut r)?;
            let n = m.memories.iter().filter(|m| m.import.is_some()).count() as u32 + index;
            e.name = format!("Memory {n}");
            e.value = Some(super::memory_text(&l));
        }
        Vector::Globals => {
            let g = Global::read(&mut r)?;
            let n = m.globals.iter().filter(|g| g.import.is_some()).count() as u32 + index;
            e.name = match named(ctx, Some(pos)) {
                Some(s) => format!("Global {n} {s}"),
                None => format!("Global {n}"),
            };
            e.value = Some(format!(
                "{}{} = {}",
                if g.mutable { "mutable " } else { "" },
                g.ty,
                g.init.map_or_else(String::new, |c| c.to_string())
            ));
        }
        Vector::Exports => {
            let x = Export::read(&mut r)?;
            e.name = format!("Export \"{}\"", x.name);
            let address = match x.kind {
                read::Kind::Func => m.function_address(x.index),
                read::Kind::Global => m.global_address(x.index),
                _ => None,
            };
            e.value = Some(format!("{} {}", x.kind.name(), with_name(x.index, named(ctx, address))));
        }
        Vector::Elements => {
            let el = Element::read(&mut Reader::new(data, pos, end))?;
            let flags = r.u32()?;
            let mut fields = vec![field(pos, r.pos(), "Flags", u64::from(flags), flags.to_string())];
            if flags & 3 == 2 {
                let at = r.pos();
                let t = r.u32()?;
                fields.push(field(at, r.pos(), "Table", u64::from(t), t.to_string()));
            }
            if flags & 1 == 0 {
                fields.push(expr_field(&mut r, "Offset")?.0);
            }
            if flags & 3 != 0 {
                let at = r.pos();
                if flags & 4 != 0 {
                    read::ValType::read(&mut r)?;
                } else {
                    r.u8()?;
                }
                fields.push(field(at, r.pos(), "Element kind", 0, "funcref".into()));
            }
            let at = r.pos();
            let n = r.u32()?;
            fields.push(field(at, r.pos(), "Count", u64::from(n), n.to_string()));
            let names: Vec<String> = el
                .items
                .iter()
                .take(8)
                .map(|(f, _)| match f {
                    Some(f) => with_name(f, named(ctx, m.function_address(*f))),
                    None => "null".into(),
                })
                .collect();
            let mut list = names.join(", ");
            if el.items.len() > 8 {
                list.push_str(&format!(", … ({} more)", el.items.len() - 8));
            }
            fields.push(field(r.pos(), el.entry.end, "Functions", n.into(), list));
            e.name = format!("Element segment {index}");
            e.value = Some(match el.mode {
                Mode::Active { index: t, offset } => format!("table {t} from {offset}: {}", plural(el.items.len())),
                Mode::Passive => format!("passive: {}", plural(el.items.len())),
                Mode::Declarative => format!("declarative: {}", plural(el.items.len())),
            });
            e.fields = fields;
            r = Reader::new(data, el.entry.end, end);
        }
        Vector::Bodies => {
            let body = Body::read(&mut r)?;
            let f = m.function_at(body.start).unwrap_or(m.imported_funcs + index);
            e.name = match named(ctx, Some(body.start)) {
                Some(n) => format!("Function {f} {n}"),
                None => format!("Function {f}"),
            };
            e.value = m.func_type(f).map(ToString::to_string);
            let mut locals = Reader::new(data, body.start, body.code);
            let decls = read::read_locals(&mut locals).unwrap_or_default();
            let count: u64 = decls.iter().map(|(n, _)| u64::from(*n)).sum();
            e.note = Some(format!(
                "{} of code, {}",
                sizes(body.end - body.start),
                if count == 1 {
                    "1 local".into()
                } else {
                    format!("{count} locals")
                }
            ));
            e.fields = vec![
                field(
                    body.entry,
                    body.start,
                    "Body size",
                    body.end - body.start,
                    sizes(body.end - body.start),
                ),
                field(
                    body.start,
                    body.code,
                    "Locals",
                    count,
                    super::analysis::locals_text(&decls),
                ),
                field(
                    body.code,
                    body.end,
                    "Code",
                    0,
                    "instructions (see the disassembly)".into(),
                ),
            ];
            e.kind = Some(RegionKind::Code);
        }
        Vector::Data => {
            let seg = Segment::read(&mut Reader::new(data, pos, end))?;
            let flags = r.u32()?;
            let mut fields = vec![field(
                pos,
                r.pos(),
                "Flags",
                u64::from(flags),
                match flags {
                    0 => "0 (active, memory 0)".into(),
                    1 => "1 (passive)".into(),
                    2 => "2 (active, memory given)".into(),
                    n => n.to_string(),
                },
            )];
            if flags == 2 {
                let at = r.pos();
                let mem = r.u32()?;
                fields.push(field(at, r.pos(), "Memory", u64::from(mem), mem.to_string()));
            }
            if flags != 1 {
                fields.push(expr_field(&mut r, "Offset")?.0);
            }
            let at = r.pos();
            let len = r.u32()?;
            fields.push(field(at, r.pos(), "Size", u64::from(len), sizes(u64::from(len))));
            let bytes = &data[seg.bytes.start as usize..seg.bytes.end as usize];
            let preview = &bytes[..bytes.len().min(32)];
            let shown = if preview
                .iter()
                .all(|&b| b == 0 || (0x20..0x7f).contains(&b) || b == b'\n')
            {
                util::quote(preview)
            } else {
                util::hex_bytes(preview)
            };
            fields.push(field(
                seg.bytes.start,
                seg.bytes.end,
                "Bytes",
                0,
                if bytes.len() > 32 {
                    format!("{shown} …")
                } else {
                    shown
                },
            ));
            let name = m.segment_name(index);
            e.name = format!("Data segment {index} {name}");
            e.value = Some(match (seg.mode, m.segment_address(index)) {
                (Mode::Active { .. }, Some(a)) => {
                    format!(
                        "{} at memory {:#x} (address {a:#x})",
                        sizes(u64::from(len)),
                        a - MEMORY_BASE
                    )
                }
                (Mode::Active { index: mem, offset }, None) => {
                    format!("{} for memory {mem} at {offset}", sizes(u64::from(len)))
                }
                _ => format!("{}, passive: written by memory.init", sizes(u64::from(len))),
            });
            e.kind = Some(super::segment_kind(&name));
            e.fields = fields;
            r = Reader::new(data, seg.entry.end, end);
        }
        Vector::Tags => {
            r.u8()?;
            let t = r.u32()?;
            e.name = format!(
                "Tag {}",
                m.tags.iter().filter(|t| t.import.is_some()).count() as u32 + index
            );
            e.value = Some(format!("type {t}"));
        }
        Vector::Names(sub) => {
            let at = r.pos();
            let i = r.u32()?;
            let what = match sub {
                1 => "function",
                4 => "type",
                5 => "table",
                6 => "memory",
                7 => "global",
                8 => "element segment",
                9 => "data segment",
                11 => "tag",
                _ => "entity",
            };
            let (n, _) = name(data, r.pos(), end, "Name")?;
            r.name()?;
            e.name = format!("Name of {what} {i}");
            e.value = Some(n.value.clone());
            e.fields = vec![field(at, n.start, "Index", u64::from(i), i.to_string()), n];
        }
        Vector::IndirectNames(sub) => {
            let owner = r.u32()?;
            let mut names = Vec::new();
            for _ in 0..r.count()? {
                let i = r.u32()?;
                names.push(format!("{i} {}", util::lossy(r.name()?)));
            }
            let what = match sub {
                2 => "Local names of function",
                3 => "Label names of function",
                _ => "Field names of type",
            };
            e.name = format!("{what} {owner}");
            let shown: Vec<String> = names.iter().take(12).cloned().collect();
            e.value = Some(if names.len() > 12 {
                format!("{}, … ({} more)", shown.join(", "), names.len() - 12)
            } else {
                shown.join(", ")
            });
        }
        Vector::Producers => {
            let key = util::lossy(r.name()?);
            let mut values = Vec::new();
            for _ in 0..r.count()? {
                let n = util::lossy(r.name()?);
                let v = util::lossy(r.name()?);
                values.push(if v.is_empty() { n } else { format!("{n} {v}") });
            }
            e.name = format!("Field \"{key}\"");
            e.value = Some(values.join(", "));
        }
        Vector::Features => {
            let prefix = r.u8()? as char;
            let feature = util::lossy(r.name()?);
            e.name = format!("Feature {prefix}{feature}");
            e.value = Some(
                match prefix {
                    '+' => "used",
                    '-' => "must not be used",
                    _ => "required",
                }
                .into(),
            );
        }
        Vector::Subsections => {
            let t = r.u8()?;
            let n = r.u32()?;
            r.skip(u64::from(n))?;
            let linking = matches!(t, 5..=8) && ctx.sections.iter().any(|s| s.name == "linking");
            let what = match (linking, t) {
                (true, 5) => "segment info",
                (true, 6) => "init functions",
                (true, 7) => "COMDAT info",
                (true, 8) => "symbol table",
                (false, 1) => "memory info",
                (false, 2) => "needed libraries",
                (false, 3) => "export info",
                (false, 4) => "import info",
                _ => "subsection",
            };
            e.name = format!("Subsection {t}: {what}");
            e.value = Some(sizes(u64::from(n)));
        }
        Vector::Relocations => {
            let t = r.u8()?;
            let offset = r.u32()?;
            let target = r.u32()?;
            let addend = if has_addend(t) { r.sleb(64) } else { None };
            e.name = format!("Relocation {index}");
            e.value = Some(format!(
                "{} at {} → {target}{}",
                relocation_name(t),
                hex(u64::from(offset)),
                addend
                    .filter(|&a| a != 0)
                    .map(|a| format!(" {a:+}"))
                    .unwrap_or_default()
            ));
        }
    }
    e.end = r.pos().max(pos + 1);
    Some(e)
}

fn plural(n: usize) -> String {
    if n == 1 {
        "1 function".into()
    } else {
        format!("{n} functions")
    }
}
