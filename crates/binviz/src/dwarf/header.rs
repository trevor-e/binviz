//! C headers from debug info: the structures, unions, enums and typedefs of
//! a binary's DWARF (or PDB), and prototypes of its external functions, in an
//! order C accepts, with forward declarations for what is only pointed at.
//! How the types become C is `ctypes.rs`'s business: explicit padding,
//! overlapping members as unions, bit fields in explicit storage units,
//! `#pragma pack` only where natural alignment can't give the layout.
//!
//! Every structure is followed by `_Static_assert`s on its size and on its
//! members' offsets, so compiling the header checks it against the debug
//! info: `clang -fsyntax-only -m32 -x c header.h` for a 32-bit target.

use std::collections::HashSet;
use std::fmt::Write as _;

use serde::Serialize;

use super::DebugInfo;
use super::ctypes::{CType, Convention, Item, Kind, Types};

/// A C header made from a binary's debug info.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CHeader {
    pub text: String,
    /// Structures and unions defined.
    pub structs: u32,
    pub enums: u32,
    pub typedefs: u32,
    /// Function prototypes.
    pub functions: u32,
    /// Names asked for that no type or function of the debug info has.
    pub not_found: Vec<String>,
}

impl DebugInfo {
    /// A C header of the types and external functions the debug info
    /// describes; or, given names, of those types and functions (by their
    /// name in the source, `geo::Rect`, or in the header, `geo__Rect`; `struct
    /// X` works too, and a C++ name without its namespaces when only one type
    /// has it) with everything they need: the types they hold defined, those
    /// they only point at declared.
    pub fn c_header(&self, names: &[&str]) -> CHeader {
        let types = self.c_types();
        let mut w = Writer::new(types);
        let mut not_found = Vec::new();
        if names.is_empty() {
            for e in 0..types.entities.len() {
                w.root(e);
            }
        } else {
            for name in names {
                let found = select(types, name);
                if found.is_empty() {
                    not_found.push(name.to_string());
                }
                for e in found {
                    w.root(e);
                }
            }
        }
        let source = match self.source() {
            "embedded" => "the binary's own DWARF".to_string(),
            s => s.rsplit(['/', '\\']).next().unwrap_or(s).to_string(),
        };
        w.write(&source, &not_found);
        CHeader {
            text: w.out,
            structs: w.counts[0],
            enums: w.counts[1],
            typedefs: w.counts[2],
            functions: w.counts[3],
            not_found,
        }
    }
}

/// The entities a name asks for: by their name in the source or in C,
/// else a C++ name without its namespaces.
fn select(types: &Types, name: &str) -> Vec<usize> {
    let mut wanted = name.trim();
    for word in ["struct ", "class ", "union ", "enum "] {
        wanted = wanted.strip_prefix(word).unwrap_or(wanted).trim();
    }
    let named = |e: &&super::ctypes::Entity| !e.name.is_empty() || !e.source.is_empty();
    let exact: Vec<usize> = types
        .entities
        .iter()
        .enumerate()
        .filter(|(_, e)| named(e) && (e.source == wanted || e.name == wanted))
        .map(|(i, _)| i)
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    let suffix = format!("::{wanted}");
    types
        .entities
        .iter()
        .enumerate()
        .filter(|(_, e)| e.source.ends_with(&suffix))
        .map(|(i, _)| i)
        .collect()
}

struct Writer<'a> {
    types: &'a Types,
    /// What to write in full, by entity.
    define: Vec<bool>,
    /// Structures and unions pointed at (or never defined): declared first.
    declare: Vec<bool>,
    /// Entities in the order they were asked for or found needed.
    order: Vec<usize>,
    out: String,
    /// Structures and unions, enums, typedefs, functions written.
    counts: [u32; 4],
}

impl<'a> Writer<'a> {
    fn new(types: &'a Types) -> Writer<'a> {
        let n = types.entities.len();
        Writer {
            types,
            define: vec![false; n],
            declare: vec![false; n],
            order: Vec::new(),
            out: String::new(),
            counts: [0; 4],
        }
    }

    /// An entity asked for: written in full, with what it needs. A typedef
    /// asked for brings the definition of what it names, a function those of
    /// the types it takes and returns by value (which writing it needs).
    fn root(&mut self, e: usize) {
        let ent = &self.types.entities[e];
        if ent.inline() {
            return;
        }
        match (&ent.kind, &ent.target) {
            (Kind::Typedef, target) => {
                let target = target.clone();
                self.need(&target, true);
            }
            (Kind::Function, CType::Function(f)) => {
                let f = f.clone();
                self.need(&f.ret, true);
                for p in &f.params {
                    self.need(p, true);
                }
            }
            _ => {}
        }
        if !ent.hidden() {
            self.add(e);
        }
    }

    /// Marks an entity to be written in full, and what it needs.
    fn add(&mut self, e: usize) {
        let ent = &self.types.entities[e];
        if matches!(ent.kind, Kind::Struct | Kind::Union) && self.types.layout(e).is_none() {
            // Never defined: declared only.
            self.declare[e] = true;
            return;
        }
        if self.define[e] {
            return;
        }
        self.define[e] = true;
        self.order.push(e);
        for (ty, complete) in self.uses(e) {
            self.need(&ty, complete);
        }
    }

    /// The types an entity's definition names: (type, whether it must be
    /// complete there).
    fn uses(&self, e: usize) -> Vec<(CType, bool)> {
        let ent = &self.types.entities[e];
        let mut out = Vec::new();
        match ent.kind {
            Kind::Struct | Kind::Union => {
                if let Some(l) = self.types.layout(e) {
                    item_types(&l.items, &mut out);
                }
            }
            Kind::Typedef | Kind::Function => out.push((ent.target.clone(), false)),
            Kind::Enum => {}
        }
        out
    }

    /// What a use of a type needs: the definitions of what it holds (when
    /// `complete`) and of the typedefs and enums it names; declarations of
    /// what it points at.
    fn need(&mut self, ty: &CType, complete: bool) {
        match ty {
            CType::Named(e) => {
                let e = *e;
                let ent = &self.types.entities[e];
                match ent.kind {
                    Kind::Struct | Kind::Union if ent.inline() => {
                        // Written in place: what it holds is held here.
                        if let Some(l) = self.types.layout(e) {
                            let mut uses = Vec::new();
                            item_types(&l.items, &mut uses);
                            for (t, c) in uses {
                                self.need(&t, c);
                            }
                        }
                    }
                    Kind::Struct | Kind::Union => {
                        if complete {
                            self.add(e);
                        } else {
                            self.declare[e] = true;
                        }
                    }
                    Kind::Enum => self.add(e),
                    Kind::Typedef if ent.hidden() => {
                        let target = ent.target.clone();
                        self.need(&target, complete);
                    }
                    Kind::Typedef => {
                        let target = ent.target.clone();
                        self.add(e);
                        if complete {
                            self.need(&target, true);
                        }
                    }
                    Kind::Function => {}
                }
            }
            CType::Pointer(t, _) => self.need(t, false),
            CType::Array(t, _) => self.need(t, true),
            CType::Qualified(_, t) => self.need(t, complete),
            CType::Function(f) => {
                self.need(&f.ret, false);
                for p in &f.params {
                    self.need(p, false);
                }
            }
            CType::Void | CType::Base(..) | CType::Sized(..) => {}
        }
    }

    /// The entities that must come before another's definition: typedefs it
    /// names, and structures it holds.
    fn before(&self, e: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for (ty, complete) in self.uses(e) {
            self.before_type(&ty, complete, &mut out);
        }
        out
    }

    fn before_type(&self, ty: &CType, complete: bool, out: &mut Vec<usize>) {
        match ty {
            CType::Named(e) => {
                let ent = &self.types.entities[*e];
                match ent.kind {
                    Kind::Struct | Kind::Union if ent.inline() => {
                        if let Some(l) = self.types.layout(*e) {
                            let mut uses = Vec::new();
                            item_types(&l.items, &mut uses);
                            for (t, c) in uses {
                                self.before_type(&t, c, out);
                            }
                        }
                    }
                    Kind::Struct | Kind::Union if complete => out.push(*e),
                    Kind::Typedef if ent.hidden() => self.before_type(&ent.target, complete, out),
                    Kind::Typedef => {
                        out.push(*e);
                        if complete {
                            self.before_type(&ent.target, true, out);
                        }
                    }
                    _ => {}
                }
            }
            CType::Pointer(t, _) => self.before_type(t, false, out),
            CType::Array(t, _) => self.before_type(t, true, out),
            CType::Qualified(_, t) => self.before_type(t, complete, out),
            CType::Function(f) => {
                self.before_type(&f.ret, false, out);
                for p in &f.params {
                    self.before_type(p, false, out);
                }
            }
            _ => {}
        }
    }

    /// The structures, unions and typedefs to define, each after what it needs.
    fn ordered(&self) -> Vec<usize> {
        let mut state = vec![0u8; self.types.entities.len()];
        let mut out = Vec::new();
        let mut roots: Vec<usize> = self.order.clone();
        roots.sort_unstable();
        for root in roots {
            let kind = self.types.entities[root].kind;
            if !matches!(kind, Kind::Struct | Kind::Union | Kind::Typedef) || state[root] != 0 {
                continue;
            }
            // Depth first, without recursion: (entity, its dependencies left,
            // last first so that they are taken in order).
            let pending = |e: usize| {
                let mut deps = self.before(e);
                deps.reverse();
                deps
            };
            let mut stack: Vec<(usize, Vec<usize>)> = vec![(root, pending(root))];
            state[root] = 1;
            while let Some((e, deps)) = stack.last_mut() {
                match deps.pop() {
                    Some(d) => {
                        if state[d] == 0 && self.define[d] {
                            state[d] = 1;
                            stack.push((d, pending(d)));
                        }
                    }
                    None => {
                        let e = *e;
                        state[e] = 2;
                        out.push(e);
                        stack.pop();
                    }
                }
            }
        }
        out
    }

    fn write(&mut self, source: &str, not_found: &[String]) {
        let types = self.types;
        let mut text = String::new();
        let bits = types.pointer_size * 8;
        // `long` and `long double` are written as the binary's C has them,
        // and not every compiler for the target makes them that size.
        let mut sizes = String::new();
        if types.long_is_64 {
            sizes.push_str("\n * The target's long must be 8 bytes, as the binary's (64-bit Windows's is 4).");
        }
        if let Some(size) = types.long_double {
            let _ = write!(
                sizes,
                "\n * The target's long double must be {size} bytes, as the binary's (MSVC's is 8)."
            );
        }
        let _ = writeln!(
            text,
            "/* Types and functions from {source}, written by binviz as C.\n *\n * Every gap in a structure is a padding member, and _Static_assert checks\n * each structure's size and its members' offsets, so compiling this header\n * checks it against the debug info on any compiler for a {bits}-bit target\n * (clang -fsyntax-only{} -x c). C++ classes are structures, with their\n * bases embedded first; names are flattened (geo::Rect is geo__Rect).{sizes} */",
            if bits == 32 { " -m32" } else { "" }
        );
        if !not_found.is_empty() {
            let _ = writeln!(text, "\n/* Not found: {}. */", not_found.join(", "));
        }
        let _ = writeln!(text, "\n#ifndef BINVIZ_TYPES_H\n#define BINVIZ_TYPES_H");
        // Elsewhere, offsetof as <stddef.h> has it on MSVC, without including
        // <stddef.h>, whose typedefs (size_t, wchar_t) the header may have too.
        let _ = writeln!(
            text,
            "\n#if defined(__GNUC__) || defined(__clang__)\n#define BINVIZ_OFFSETOF(type, member) __builtin_offsetof(type, member)\n#else\n#define BINVIZ_OFFSETOF(type, member) ((unsigned long long)&((type *)0)->member)\n#endif"
        );
        let defined = self.ordered();
        let enums: Vec<usize> = (0..types.entities.len())
            .filter(|&e| {
                self.define[e] && types.entities[e].kind == Kind::Enum && !types.entities[e].enumerators.is_empty()
            })
            .collect();
        let functions: Vec<usize> = (0..types.entities.len())
            .filter(|&e| self.define[e] && types.entities[e].kind == Kind::Function)
            .collect();
        // The calling conventions used, whose macros come first.
        let mut conventions = HashSet::new();
        for &e in defined.iter().chain(&functions) {
            for (ty, _) in self.uses(e) {
                conventions_of(types, &ty, &mut conventions, 0);
            }
        }
        if !conventions.is_empty() {
            let mut list: Vec<Convention> = conventions.into_iter().collect();
            list.sort_by_key(|c| c.macro_name());
            let _ = writeln!(
                text,
                "\n/* x86 calling conventions: GCC's and clang's attributes, MSVC's keywords\n * (its C compiler has no __thiscall), nothing elsewhere. */"
            );
            let (gnu, msvc): (Vec<String>, Vec<String>) = list
                .iter()
                .map(|c| {
                    let (attribute, keyword) = match c {
                        Convention::Stdcall => ("stdcall", "__stdcall"),
                        Convention::Fastcall => ("fastcall", "__fastcall"),
                        Convention::Thiscall => ("thiscall", ""),
                        Convention::Pascal => ("", "__pascal"),
                        Convention::Vectorcall => ("vectorcall", "__vectorcall"),
                    };
                    let gnu = if attribute.is_empty() {
                        String::new()
                    } else {
                        format!("__attribute__(({attribute}))")
                    };
                    (
                        format!("#define {} {gnu}", c.macro_name()).trim_end().to_string(),
                        format!("#define {} {keyword}", c.macro_name()).trim_end().to_string(),
                    )
                })
                .unzip();
            let none: Vec<String> = list.iter().map(|c| format!("#define {}", c.macro_name())).collect();
            let _ = writeln!(
                text,
                "#if (defined(__GNUC__) || defined(__clang__)) && defined(__i386__)\n{}\n#elif defined(_MSC_VER) && defined(_M_IX86)\n{}\n#else\n{}\n#endif",
                gnu.join("\n"),
                msvc.join("\n"),
                none.join("\n")
            );
        }
        // Declared first: what is pointed at, and what is never defined.
        let declared: Vec<usize> = (0..types.entities.len())
            .filter(|&e| self.declare[e] && !types.entities[e].inline())
            .collect();
        if !declared.is_empty() {
            let _ = writeln!(text);
            for e in declared {
                let ent = &types.entities[e];
                let _ = write!(text, "{} {};", ent.kind.keyword(), ent.name);
                if types.layout(e).is_none() {
                    let why = if ent.complete {
                        "of no size"
                    } else {
                        "never defined in the debug info"
                    };
                    let _ = write!(text, " /* {}{why} */", source_comment(ent));
                } else if !ent.source.is_empty() && ent.source != ent.name {
                    let _ = write!(text, " /* {} */", ent.source);
                }
                let _ = writeln!(text);
            }
        }
        for e in enums {
            self.counts[1] += 1;
            self.write_enum(&mut text, e);
        }
        for e in defined {
            match types.entities[e].kind {
                Kind::Typedef => {
                    self.counts[2] += 1;
                    self.write_typedef(&mut text, e);
                }
                _ => {
                    self.counts[0] += 1;
                    self.write_struct(&mut text, e);
                }
            }
        }
        if !functions.is_empty() {
            let _ = writeln!(text);
            for e in functions {
                self.counts[3] += 1;
                let ent = &types.entities[e];
                let decl = types.declare(&ent.target, &ent.name, &ent.params, &mut |i| {
                    self.inline_body(i, 0, 0, 0)
                });
                let _ = write!(text, "{decl};");
                if ent.source != ent.name {
                    let _ = write!(text, " /* {} */", ent.source);
                }
                let _ = writeln!(text);
            }
        }
        let _ = writeln!(text, "\n#endif");
        self.out = text;
    }

    fn write_enum(&self, text: &mut String, e: usize) {
        let ent = &self.types.entities[e];
        let _ = write!(text, "\nenum {} {{", ent.name);
        let _ = writeln!(text, "{}", comment(&[&source_note(ent), &size_note(ent.size)]));
        for (_, name, value) in &ent.enumerators {
            let _ = writeln!(text, "    {name} = {value},");
        }
        let _ = writeln!(text, "}};");
        if ent.size == Some(4) {
            let _ = writeln!(
                text,
                "_Static_assert(sizeof(enum {0}) == 4, \"enum {0} is 4 bytes\");",
                ent.name
            );
        }
    }

    fn write_typedef(&self, text: &mut String, e: usize) {
        let ent = &self.types.entities[e];
        let decl = self
            .types
            .declare(&ent.target, &ent.name, &[], &mut |i| self.inline_body(i, 0, 0, 0));
        let _ = write!(text, "\ntypedef {decl};");
        if !ent.source.is_empty() && ent.source != ent.name {
            let _ = write!(text, " /* {} */", ent.source);
        }
        let _ = writeln!(text);
    }

    fn write_struct(&self, text: &mut String, e: usize) {
        let types = self.types;
        let ent = &types.entities[e];
        let Some(layout) = types.layout(e) else { return };
        let tag = format!("{} {}", ent.kind.keyword(), ent.name);
        let _ = writeln!(text);
        if let Some(p) = layout.pack {
            let _ = writeln!(
                text,
                "/* Packed: natural alignment can't give this layout. */\n#pragma pack(push, {p})"
            );
        }
        let _ = writeln!(
            text,
            "{tag} {{{}",
            comment(&[&source_note(ent), &size_note(Some(layout.size))])
        );
        for (at, note) in &layout.notes {
            let _ = writeln!(text, "    /* {at:#x}: {note} */");
        }
        self.write_items(text, &layout.items, 1, 0, width(layout.size));
        let _ = writeln!(text, "}};");
        if layout.pack.is_some() {
            let _ = writeln!(text, "#pragma pack(pop)");
        }
        let _ = writeln!(
            text,
            "_Static_assert(sizeof({tag}) == {0:#x}, \"{tag} is {0:#x} bytes\");",
            layout.size
        );
        if !layout.union {
            let mut members = Vec::new();
            self.offsets(&layout.items, "", 0, &mut members, 0);
            for (path, at) in members {
                let _ = writeln!(
                    text,
                    "_Static_assert(BINVIZ_OFFSETOF({tag}, {path}) == {at:#x}, \"{}.{path} is at {at:#x}\");",
                    ent.name
                );
            }
        }
    }

    /// Members whose offset can be asserted: (path, offset). Members of
    /// anonymous structures and unions are named directly, those of a named
    /// member of anonymous type through it.
    fn offsets(&self, items: &[Item], prefix: &str, base: u64, out: &mut Vec<(String, u64)>, depth: u32) {
        for item in items {
            match item {
                Item::Field(f) => {
                    let path = format!("{prefix}{}", f.name);
                    out.push((path.clone(), base + f.offset));
                    // Into a member of a type written in place.
                    let mut element = &f.ty;
                    let mut index = String::new();
                    while let CType::Array(t, dims) = element {
                        index.push_str(&"[0]".repeat(dims.len()));
                        element = t;
                    }
                    if let CType::Named(e) = element
                        && self.types.entities[*e].inline()
                        && depth < 8
                        && let Some(l) = self.types.layout(*e)
                        && !l.union
                    {
                        self.offsets(&l.items, &format!("{path}{index}."), base + f.offset, out, depth + 1);
                    }
                }
                Item::Group(g) => self.offsets(&g.items, prefix, base, out, depth),
                Item::Bits(_) | Item::Pad { .. } => {}
            }
        }
    }

    /// Items at an indent, their offsets shown from `base` in `digits` hex digits.
    fn write_items(&self, text: &mut String, items: &[Item], indent: usize, base: u64, digits: usize) {
        let pad = "    ".repeat(indent);
        let at = |o: u64| format!("/* {:#0w$x} */", base + o, w = digits + 2);
        let blank = " ".repeat(digits + 8);
        for item in items {
            match item {
                Item::Field(f) => {
                    let decl = self.types.declare(&f.ty, &f.name, &[], &mut |i| {
                        self.inline_body(i, indent, base + f.offset, digits)
                    });
                    let note = match (&f.note, &f.ty) {
                        (Some(n), _) => format!(" /* {n} */"),
                        (None, CType::Sized(e, ..)) => {
                            format!(" /* enum {} */", self.types.entities[*e].name)
                        }
                        _ => String::new(),
                    };
                    let _ = writeln!(text, "{pad}{} {decl};{note}", at(f.offset));
                }
                Item::Bits(b) => {
                    for (i, f) in b.fields.iter().enumerate() {
                        let decl = self
                            .types
                            .declare(&f.ty, f.name.as_deref().unwrap_or(""), &[], &mut |_| String::new());
                        let lead = if i == 0 { at(b.offset) } else { blank.clone() };
                        let _ = writeln!(text, "{pad}{lead} {decl} : {};", f.width);
                    }
                }
                Item::Group(g) => {
                    let _ = writeln!(
                        text,
                        "{pad}{} {} {{",
                        at(g.offset),
                        if g.union { "union" } else { "struct" }
                    );
                    self.write_items(text, &g.items, indent + 1, base, digits);
                    let _ = writeln!(text, "{pad}}};");
                }
                Item::Pad { offset, size, name } => {
                    let _ = writeln!(text, "{pad}{} char {name}[{size}];", at(*offset));
                }
            }
        }
    }

    /// An anonymous structure or union written where it is used, at an
    /// indent, its members' offsets shown from `base` in `digits` hex digits
    /// (0: as many as it needs).
    fn inline_body(&self, e: usize, indent: usize, base: u64, digits: usize) -> String {
        let ent = &self.types.entities[e];
        let Some(layout) = self.types.layout(e) else {
            return format!("{} {{ }}", ent.kind.keyword());
        };
        let digits = digits.max(width(base + layout.size));
        let mut body = String::new();
        let _ = writeln!(body, "{} {{", ent.kind.keyword());
        self.write_items(&mut body, &layout.items, indent + 1, base, digits);
        let _ = write!(body, "{}}}", "    ".repeat(indent));
        body
    }
}

/// The types a layout's members name: (type, whether it must be complete).
fn item_types(items: &[Item], out: &mut Vec<(CType, bool)>) {
    for item in items {
        match item {
            Item::Field(f) => out.push((f.ty.clone(), true)),
            Item::Bits(b) => out.extend(b.fields.iter().map(|f| (f.ty.clone(), true))),
            Item::Group(g) => item_types(&g.items, out),
            Item::Pad { .. } => {}
        }
    }
}

fn conventions_of(types: &Types, ty: &CType, out: &mut HashSet<Convention>, depth: u32) {
    if depth > 32 {
        return;
    }
    match ty {
        CType::Function(f) => {
            if let Some(c) = f.convention {
                out.insert(c);
            }
            conventions_of(types, &f.ret, out, depth + 1);
            for p in &f.params {
                conventions_of(types, p, out, depth + 1);
            }
        }
        CType::Pointer(t, _) | CType::Array(t, _) | CType::Qualified(_, t) => conventions_of(types, t, out, depth + 1),
        CType::Named(e) if types.entities[*e].inline() => {
            if let Some(l) = types.layout(*e) {
                let mut uses = Vec::new();
                item_types(&l.items, &mut uses);
                for (t, _) in uses {
                    conventions_of(types, &t, out, depth + 1);
                }
            }
        }
        _ => {}
    }
}

/// Hex digits of the largest offset in something of `size` bytes.
fn width(size: u64) -> usize {
    format!("{:x}", size.saturating_sub(1).max(0xf)).len()
}

/// The source's name when it isn't the C one.
fn source_note(ent: &super::ctypes::Entity) -> String {
    if !ent.source.is_empty() && ent.source != ent.name {
        ent.source.clone()
    } else {
        String::new()
    }
}

fn source_comment(ent: &super::ctypes::Entity) -> String {
    let n = source_note(ent);
    if n.is_empty() { n } else { format!("{n}: ") }
}

fn size_note(size: Option<u64>) -> String {
    size.map_or(String::new(), |s| format!("{s:#x} bytes"))
}

/// ` /* a, b */` of the parts that aren't empty.
fn comment(parts: &[&str]) -> String {
    let parts: Vec<&str> = parts.iter().copied().filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!(" /* {} */", parts.join(", "))
    }
}
