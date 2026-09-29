//! C types from debug info: the structures, unions, enums, typedefs and
//! functions a binary's DWARF describes (a PDB's too, once read into DWARF),
//! each once however many units repeat it, with C names and a layout C can
//! reproduce whatever the compiler. The header writer (`header.rs`) prints
//! them; [`DebugInfo::struct_field`] names the member at an offset the way the
//! header declares it.
//!
//! A structure's members are laid out explicitly: padding members fill every
//! gap, members that overlap (a PDB flattens anonymous unions into their
//! parent) are regrouped as unions, and bit fields are put in storage units
//! that Microsoft's rules and System V's both place the same way. Only a
//! structure that natural alignment can't lay out (a packed one) gets a
//! `#pragma pack`, the largest that works.
//!
//! C++ comes out as C: classes are structures, bases are embedded first (or
//! their members are, where the derived class reuses a base's tail padding), a
//! vtable pointer is `void **__vftable`, and qualified names are flattened
//! (`geo::Rect` is `geo__Rect`).

use std::collections::{HashMap, HashSet};

use gimli::{AttributeValue, Reader as _, Section as _, UnitOffset};
use serde::Serialize;

use super::die::{member_bit_offset, member_offset};
use super::{DebugInfo, R};

type Die = gimli::DebuggingInformationEntry<R>;

/// Qualifiers of a type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct Quals {
    pub constant: bool,
    pub volatile: bool,
}

impl Quals {
    pub(crate) fn words(self) -> &'static str {
        match (self.constant, self.volatile) {
            (true, true) => "const volatile",
            (true, false) => "const",
            (false, true) => "volatile",
            (false, false) => "",
        }
    }
}

/// x86 calling conventions other than C's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Convention {
    Stdcall,
    Fastcall,
    Thiscall,
    Pascal,
    Vectorcall,
}

impl Convention {
    /// The macro the header defines for it.
    pub(crate) fn macro_name(self) -> &'static str {
        match self {
            Convention::Stdcall => "BINVIZ_STDCALL",
            Convention::Fastcall => "BINVIZ_FASTCALL",
            Convention::Thiscall => "BINVIZ_THISCALL",
            Convention::Pascal => "BINVIZ_PASCAL",
            Convention::Vectorcall => "BINVIZ_VECTORCALL",
        }
    }

    /// From `DW_AT_calling_convention`'s vendor values (LLVM's for x86).
    pub(crate) fn from_dwarf(cc: gimli::DwCc) -> Option<Convention> {
        Some(match cc.0 {
            0xb1 => Convention::Stdcall,
            0xb2 => Convention::Pascal,
            0xb3 => Convention::Fastcall,
            0xb5 => Convention::Thiscall,
            0xc0 => Convention::Vectorcall,
            _ => return None,
        })
    }
}

/// A function type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Func {
    pub ret: CType,
    pub params: Vec<CType>,
    pub varargs: bool,
    /// Declared with its parameters (C's `f(void)` rather than K&R's `f()`).
    pub prototyped: bool,
    pub convention: Option<Convention>,
}

/// A type as C spells it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum CType {
    Void,
    /// A base type: its C spelling and size.
    Base(&'static str, u64),
    /// A structure, union, enum or typedef of the model.
    Named(usize),
    /// An enum whose size isn't C's (an int's), kept as an integer of its size.
    Sized(usize, &'static str, u64),
    Pointer(Box<CType>, Quals),
    Array(Box<CType>, Vec<Option<u64>>),
    Function(Box<Func>),
    Qualified(Quals, Box<CType>),
}

impl CType {
    /// Bytes C can't type otherwise (a 16-byte integer on a 32-bit target, a
    /// pointer to a member function).
    pub(crate) fn bytes(n: u64) -> CType {
        CType::Array(Box::new(CType::Base("unsigned char", 1)), vec![Some(n)])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    Struct,
    Union,
    Enum,
    Typedef,
    Function,
}

impl Kind {
    /// The keyword that introduces a tag of this kind.
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            Kind::Union => "union",
            Kind::Enum => "enum",
            _ => "struct",
        }
    }
}

/// A structure, union, enum, typedef or function of the header.
#[derive(Debug, Clone)]
pub(crate) struct Entity {
    pub kind: Kind,
    /// Its name in the source, qualified (`geo::Rect`); empty for an anonymous type.
    pub source: String,
    /// Its name in C: a tag, a typedef or a function name. Empty for an
    /// anonymous structure or union, which is written where it is used.
    pub name: String,
    pub unit: u32,
    pub die: UnitOffset,
    /// Structures and unions: whether a definition was found.
    pub complete: bool,
    pub size: Option<u64>,
    /// Typedefs: what they name. Functions: their type.
    pub target: CType,
    /// Enums: each enumerator's name in the source and in C, and its value.
    pub enumerators: Vec<(String, String, i128)>,
    /// Functions: their parameters' names in C (empty where there is none).
    pub params: Vec<String>,
    /// A C++ `enum class`, whose enumerators take the enum's name as a prefix.
    pub scoped: bool,
}

impl Entity {
    /// Written in place where it is used: an anonymous structure or union.
    pub(crate) fn inline(&self) -> bool {
        matches!(self.kind, Kind::Struct | Kind::Union) && self.name.is_empty()
    }

    /// A typedef left out (its name was taken): its uses name what it names.
    pub(crate) fn hidden(&self) -> bool {
        self.kind == Kind::Typedef && self.name.is_empty()
    }
}

/// A member of a structure or union, as C declares it.
#[derive(Debug, Clone)]
pub(crate) struct Field {
    pub name: String,
    pub ty: CType,
    pub offset: u64,
    pub size: u64,
    /// What it stands for when it isn't a plain member ("base class geo::Shape").
    pub note: Option<String>,
}

/// A bit field, or a filler (no name) that keeps the next one in place.
#[derive(Debug, Clone)]
pub(crate) struct BitField {
    pub name: Option<String>,
    /// Declared with a type of its storage unit's size.
    pub ty: CType,
    /// Its first bit in the unit, and its width.
    pub first: u64,
    pub width: u64,
}

/// Bit fields that share one storage unit, which they fill to its end.
#[derive(Debug, Clone)]
pub(crate) struct Bits {
    pub offset: u64,
    /// The unit's size in bytes.
    pub size: u64,
    pub fields: Vec<BitField>,
}

/// Members written as a structure or union of their own, in place: a C11
/// anonymous member, or members that overlap.
#[derive(Debug, Clone)]
pub(crate) struct Group {
    pub union: bool,
    pub offset: u64,
    pub size: u64,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub(crate) enum Item {
    Field(Field),
    Bits(Bits),
    Group(Group),
    /// Bytes no member uses: `char _pad_1c[4];`.
    Pad {
        offset: u64,
        size: u64,
        name: String,
    },
}

impl Item {
    pub(crate) fn offset(&self) -> u64 {
        match self {
            Item::Field(f) => f.offset,
            Item::Bits(b) => b.offset,
            Item::Group(g) => g.offset,
            Item::Pad { offset, .. } => *offset,
        }
    }

    pub(crate) fn size(&self) -> u64 {
        match self {
            Item::Field(f) => f.size,
            Item::Bits(b) => b.size,
            Item::Group(g) => g.size,
            Item::Pad { size, .. } => *size,
        }
    }
}

/// A structure's or union's members as C declares them, in offset order,
/// every gap filled. Offsets are from the start of the structure.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    pub union: bool,
    pub size: u64,
    pub items: Vec<Item>,
    /// The `#pragma pack` it needs, when natural alignment can't lay it out.
    pub pack: Option<u64>,
    /// Its alignment, as laid out.
    pub align: u64,
    /// What the debug info describes that the layout leaves as padding or
    /// spreads out (virtual bases, members of types never defined, bases
    /// whose members are taken in): (offset, what).
    pub notes: Vec<(u64, String)>,
}

/// The C types of a binary's debug info.
pub(crate) struct Types {
    pub entities: Vec<Entity>,
    /// Structures' and unions' layouts, by entity (for those defined).
    pub layouts: Vec<Option<Layout>>,
    pub pointer_size: u64,
    /// Some type is a `long` of 64 bits, which only an LP64 target (not
    /// 64-bit Windows) compiles to the same size.
    pub long_is_64: bool,
    /// The size of the binary's `long double` when some type is one (x87's
    /// 80 bits in 12 or 16 bytes, or a 128-bit float), which compilers for
    /// one CPU disagree on (MSVC's is a double).
    pub long_double: Option<u64>,
    /// Each type or function DIE's entity.
    by_die: HashMap<(u32, usize), usize>,
    /// Structures, unions and typedefs by source name and by C name.
    by_name: HashMap<String, Vec<usize>>,
}

/// Words C reserves, which a name from the debug info can't be.
const KEYWORDS: &[&str] = &[
    "auto",
    "break",
    "case",
    "char",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extern",
    "float",
    "for",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "register",
    "restrict",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "struct",
    "switch",
    "typedef",
    "union",
    "unsigned",
    "void",
    "volatile",
    "while",
    "alignas",
    "alignof",
    "bool",
    "constexpr",
    "false",
    "nullptr",
    "static_assert",
    "thread_local",
    "true",
    "typeof",
    "typeof_unqual",
    "asm",
    "offsetof",
];

/// A C identifier for a name from the debug info: `geo::Rect` is `geo__Rect`;
/// otherwise its words joined by underscores, a pointer (`*`) the word `p`
/// and a reference (`&`) `r`, and the rest (template brackets, operators)
/// dropped: `std::vector<int, std::allocator<int> >` is
/// `std__vector_int_std__allocator_int`.
pub(crate) fn c_identifier(name: &str) -> String {
    let mut out = String::new();
    // Whether the next word needs an underscore before it.
    let mut apart = false;
    let word = |out: &mut String, apart: &mut bool, w: &str| {
        if *apart && !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
        out.push_str(w);
        *apart = false;
    };
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ':' if chars.peek() == Some(&':') => {
                chars.next();
                out.push_str("__");
                apart = false;
            }
            c if c.is_ascii_alphanumeric() || c == '_' => word(&mut out, &mut apart, c.encode_utf8(&mut [0; 4])),
            '*' | '&' => {
                apart = true;
                word(&mut out, &mut apart, if c == '*' { "p" } else { "r" });
                apart = true;
            }
            _ => apart = true,
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    if KEYWORDS.contains(&out.as_str()) || out.starts_with("BINVIZ_") {
        out.push('_');
    }
    out
}

/// A type's own name, without its scopes or template arguments: `Shape` for
/// `geo::Shape`, `_Vector_impl` for `std::_Vector_base<int>::_Vector_impl`.
fn own_name(source: &str) -> &str {
    let b = source.as_bytes();
    let mut depth = 0i32;
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'<' | b'(' => depth += 1,
            b'>' | b')' => depth -= 1,
            b':' if depth == 0 && b.get(i + 1) == Some(&b':') => {
                start = i + 2;
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let rest = &source[start..];
    rest.split('<').next().unwrap_or(rest)
}

/// A name not yet in `taken` (the name, or it numbered), which it then is.
fn unique(taken: &mut HashSet<String>, base: &str) -> String {
    if taken.insert(base.to_string()) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}_{n}"))
        .find(|n| taken.insert(n.clone()))
        .expect("a free name")
}

/// The strictest alignment any ABI gives a scalar of this size: the largest
/// power of two that divides it, at most 16.
fn strict_align(size: u64) -> u64 {
    if size == 0 {
        1
    } else {
        1 << size.trailing_zeros().min(4)
    }
}

/// The C spelling of an integer of `size` bytes.
fn integer(size: u64, signed: bool, pointer_size: u64) -> Option<&'static str> {
    Some(match (size, signed) {
        (1, true) => "signed char",
        (1, false) => "unsigned char",
        (2, true) => "short",
        (2, false) => "unsigned short",
        (4, true) => "int",
        (4, false) => "unsigned int",
        (8, true) => "long long",
        (8, false) => "unsigned long long",
        (16, true) if pointer_size == 8 => "__int128",
        (16, false) if pointer_size == 8 => "unsigned __int128",
        _ => return None,
    })
}

/// A base type in C, from its encoding and size: the spelling follows the
/// size, not the source's name for it, except that `long` stays where it is
/// as wide as a pointer (everywhere but 64-bit Windows).
fn base_type(name: &str, encoding: gimli::DwAte, size: u64, pointer_size: u64) -> CType {
    let long = matches!(
        name,
        "long" | "long int" | "signed long" | "long signed int" | "unsigned long" | "long unsigned int"
    ) && size == pointer_size;
    let spelled = match encoding {
        gimli::DW_ATE_boolean if size == 1 => Some("_Bool"),
        gimli::DW_ATE_boolean => integer(size, false, pointer_size),
        gimli::DW_ATE_signed_char | gimli::DW_ATE_unsigned_char if name == "char" && size == 1 => Some("char"),
        gimli::DW_ATE_signed | gimli::DW_ATE_signed_char if long => Some("long"),
        gimli::DW_ATE_unsigned | gimli::DW_ATE_unsigned_char if long => Some("unsigned long"),
        gimli::DW_ATE_signed | gimli::DW_ATE_signed_char => integer(size, true, pointer_size),
        gimli::DW_ATE_float => match size {
            4 => Some("float"),
            8 => Some("double"),
            12 | 16 => Some("long double"),
            _ => None,
        },
        gimli::DW_ATE_complex_float => match size {
            8 => Some("float _Complex"),
            16 => Some("double _Complex"),
            24 | 32 => Some("long double _Complex"),
            _ => None,
        },
        _ => integer(size, false, pointer_size),
    };
    match spelled {
        Some(s) => CType::Base(s, size),
        None if size == 0 => CType::Void,
        None => CType::bytes(size),
    }
}

fn is_signed(ty: &CType) -> bool {
    match ty {
        CType::Base(s, _) | CType::Sized(_, s, _) => !s.starts_with("unsigned") && *s != "_Bool",
        _ => true,
    }
}

fn string_attr(unit: &gimli::UnitRef<'_, R>, die: &Die, at: gimli::DwAt) -> Option<String> {
    let v = die.attr_value(at)?;
    let s = unit.attr_string(v).ok()?;
    s.to_string_lossy().ok().map(|c| c.into_owned())
}

/// A type DIE found while walking the units.
struct Found {
    unit: u32,
    offset: UnitOffset,
    kind: Kind,
    /// Qualified name; `None` for an anonymous type.
    name: Option<String>,
    declaration: bool,
    size: Option<u64>,
    /// Its members' names and offsets (enumerators' values): what tells two
    /// types of one name apart.
    shape: String,
    scoped: bool,
}

/// A structure's member while its layout is worked out: offsets are from
/// the start of the structure, bits too.
#[derive(Debug, Clone)]
enum Piece {
    Field(Field),
    Bit {
        name: Option<String>,
        ty: CType,
        first: u64,
        width: u64,
    },
    Group {
        union: bool,
        offset: u64,
        size: u64,
        pieces: Vec<Piece>,
    },
}

impl Piece {
    /// Where it starts and ends, in bits.
    fn bits(&self) -> (u64, u64) {
        match self {
            Piece::Field(f) => (f.offset * 8, (f.offset + f.size) * 8),
            Piece::Bit { first, width, .. } => (*first, first + width),
            Piece::Group { offset, size, .. } => (offset * 8, (offset + size) * 8),
        }
    }
}

/// A layout's items as pieces again, moved by `delta` bytes: a base's or an
/// anonymous member's members, taken into another structure.
fn to_pieces(items: &[Item], delta: u64) -> Vec<Piece> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Field(f) => out.push(Piece::Field(Field {
                offset: f.offset + delta,
                ..f.clone()
            })),
            Item::Bits(b) => {
                for f in b.fields.iter().filter(|f| f.name.is_some()) {
                    out.push(Piece::Bit {
                        name: f.name.clone(),
                        ty: f.ty.clone(),
                        first: (b.offset + delta) * 8 + f.first,
                        width: f.width,
                    });
                }
            }
            Item::Group(g) => out.push(Piece::Group {
                union: g.union,
                offset: g.offset + delta,
                size: g.size,
                pieces: to_pieces(&g.items, delta),
            }),
            Item::Pad { .. } => {}
        }
    }
    out
}

/// Members that overlap, as one union: each alternative the members that
/// don't overlap each other, in order (a structure when there are several).
fn regroup(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut cluster: Vec<Piece> = Vec::new();
    let mut end = 0;
    for p in pieces {
        let (start, stop) = p.bits();
        if !cluster.is_empty() && start < end {
            end = end.max(stop);
            cluster.push(p);
            continue;
        }
        if !cluster.is_empty() {
            out.push(overlap(std::mem::take(&mut cluster)));
        }
        end = stop;
        cluster.push(p);
    }
    if !cluster.is_empty() {
        out.push(overlap(cluster));
    }
    out
}

fn overlap(mut cluster: Vec<Piece>) -> Piece {
    if cluster.len() == 1 {
        return cluster.pop().expect("one piece");
    }
    let start = cluster.iter().map(|p| p.bits().0).min().unwrap_or(0) / 8;
    let end = cluster.iter().map(|p| p.bits().1).max().unwrap_or(0).div_ceil(8);
    let mut alternatives: Vec<Vec<Piece>> = Vec::new();
    for p in cluster {
        let (first, _) = p.bits();
        match alternatives
            .iter_mut()
            .find(|a| a.last().is_some_and(|l| l.bits().1 <= first))
        {
            Some(a) => a.push(p),
            None => alternatives.push(vec![p]),
        }
    }
    let pieces = alternatives
        .into_iter()
        .map(|mut a| {
            let alone = a.len() == 1 && a[0].bits().0 == start * 8 && !matches!(a[0], Piece::Bit { .. });
            if alone {
                a.pop().expect("one piece")
            } else {
                let stop = a.iter().map(|p| p.bits().1).max().unwrap_or(0).div_ceil(8);
                Piece::Group {
                    union: false,
                    offset: start,
                    size: stop - start,
                    pieces: a,
                }
            }
        })
        .collect();
    Piece::Group {
        union: true,
        offset: start,
        size: end - start,
        pieces,
    }
}

/// Builds the model: finds the types, merges their copies, names them in C
/// and lays out the structures.
struct Builder<'a> {
    debug: &'a DebugInfo,
    found: Vec<Found>,
    /// Typedef DIEs: their index in `found`.
    typedef_dies: HashMap<(u32, usize), usize>,
    /// Each typedef's entity (`None` while its target is worked out, or for
    /// one without a name).
    typedef_entity: HashMap<usize, Option<usize>>,
    /// Typedef entities by name and target: copies in other units are one.
    typedefs: HashMap<(String, CType), usize>,
    /// Subprogram DIEs' qualified names, for definitions that point at their declaration.
    function_names: HashMap<(u32, usize), String>,
    /// External functions with code: DIE and qualified name.
    functions: Vec<(u32, UnitOffset, String)>,
    /// External functions with code named by a DIE later in the unit (an
    /// abstract instance after its out-of-line copy): DIE and that DIE.
    named_later: Vec<(u32, UnitOffset, (u32, UnitOffset))>,
    entities: Vec<Entity>,
    by_die: HashMap<(u32, usize), usize>,
    ctypes: HashMap<(u32, usize), CType>,
    pointer_size: u64,
    little: bool,
    depth: u32,
    layouts: Vec<Option<Layout>>,
    building: Vec<bool>,
}

impl DebugInfo {
    /// The C types of the debug info, worked out on first use.
    pub(crate) fn c_types(&self) -> &Types {
        self.c_types.get_or_init(|| Types::build(self))
    }
}

impl Types {
    fn build(debug: &DebugInfo) -> Types {
        // The address size most units have (a PDB's is its binary's).
        let mut sizes: HashMap<u8, usize> = HashMap::new();
        for u in &debug.units {
            *sizes.entry(u.header.address_size()).or_default() += 1;
        }
        let pointer_size = sizes
            .into_iter()
            .max_by_key(|&(s, n)| (n, s))
            .map_or(8, |(s, _)| u64::from(s));
        let mut b = Builder {
            debug,
            found: Vec::new(),
            typedef_dies: HashMap::new(),
            typedef_entity: HashMap::new(),
            typedefs: HashMap::new(),
            function_names: HashMap::new(),
            functions: Vec::new(),
            named_later: Vec::new(),
            entities: Vec::new(),
            by_die: HashMap::new(),
            ctypes: HashMap::new(),
            pointer_size,
            little: debug.dwarf.debug_info.reader().endian() == gimli::RunTimeEndian::Little,
            depth: 0,
            layouts: Vec::new(),
            building: Vec::new(),
        };
        b.walk();
        for (unit, offset, origin) in std::mem::take(&mut b.named_later) {
            if let Some(name) = b.function_names.get(&(origin.0, origin.1.0)) {
                b.functions.push((unit, offset, name.clone()));
            }
        }
        b.merge();
        for i in 0..b.found.len() {
            if b.found[i].kind == Kind::Typedef {
                b.typedef(i);
            }
        }
        b.enumerators();
        b.functions();
        b.name();
        b.layouts = vec![None; b.entities.len()];
        b.building = vec![false; b.entities.len()];
        for e in 0..b.entities.len() {
            b.ensure_layout(e);
        }
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, e) in b.entities.iter().enumerate() {
            if matches!(e.kind, Kind::Struct | Kind::Union | Kind::Typedef) {
                for n in [&e.source, &e.name] {
                    if !n.is_empty() {
                        let list = by_name.entry(n.clone()).or_default();
                        if list.last() != Some(&i) {
                            list.push(i);
                        }
                    }
                }
            }
        }
        // The sizes of C types that compilers for one target disagree on.
        let long_is_64 = b
            .ctypes
            .values()
            .any(|t| matches!(t, CType::Base("long" | "unsigned long", 8)));
        let long_double = b
            .ctypes
            .values()
            .filter_map(|t| match t {
                CType::Base("long double", size) => Some(*size),
                CType::Base("long double _Complex", size) => Some(size / 2),
                _ => None,
            })
            .max();
        Types {
            entities: b.entities,
            layouts: b.layouts,
            pointer_size,
            long_is_64,
            long_double,
            by_die: b.by_die,
            by_name,
        }
    }

    /// The entity a type or function DIE stands for.
    pub(crate) fn entity_at(&self, unit: u32, die: UnitOffset) -> Option<usize> {
        self.by_die.get(&(unit, die.0)).copied()
    }

    pub(crate) fn layout(&self, e: usize) -> Option<&Layout> {
        self.layouts.get(e)?.as_ref()
    }

    /// A type's size in bytes, as C lays it out.
    pub(crate) fn size_of(&self, ty: &CType) -> u64 {
        size_of(&self.entities, self.pointer_size, ty)
    }

    /// Follows typedefs and qualifiers to the type they stand for.
    pub(crate) fn resolve<'t>(&'t self, ty: &'t CType) -> &'t CType {
        resolve(&self.entities, ty)
    }

    /// A declaration in C of `name` with type `ty` (`int (*name)(void)`), or
    /// the type alone when `name` is empty. `params` names the parameters of
    /// the outermost function type; `inline` writes an anonymous structure or
    /// union where it is used.
    pub(crate) fn declare(
        &self,
        ty: &CType,
        name: &str,
        params: &[String],
        inline: &mut dyn FnMut(usize) -> String,
    ) -> String {
        let mut inner = name.to_string();
        let mut ty = ty;
        let mut params = Some(params);
        // A pointer to a function puts the calling convention inside its parentheses.
        let mut convention_done = false;
        loop {
            match ty {
                CType::Pointer(to, q) => {
                    let quals = q.words();
                    inner = match (quals.is_empty(), inner.is_empty()) {
                        (true, _) => format!("*{inner}"),
                        (false, true) => format!("*{quals}"),
                        (false, false) => format!("*{quals} {inner}"),
                    };
                    match &**to {
                        CType::Array(..) => inner = format!("({inner})"),
                        CType::Function(f) => {
                            inner = match f.convention {
                                Some(c) => format!("({} {inner})", c.macro_name()),
                                None => format!("({inner})"),
                            };
                            convention_done = true;
                        }
                        _ => {}
                    }
                    ty = to;
                }
                CType::Array(elem, dims) => {
                    for d in dims {
                        match d {
                            Some(n) => inner.push_str(&format!("[{n}]")),
                            None => inner.push_str("[]"),
                        }
                    }
                    ty = elem;
                }
                CType::Function(f) => {
                    if !convention_done && let Some(c) = f.convention {
                        inner = format!("{} {inner}", c.macro_name());
                    }
                    convention_done = false;
                    let names = params.take().unwrap_or(&[]);
                    let mut list: Vec<String> = f
                        .params
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let n = names.get(i).map_or("", String::as_str);
                            self.declare(p, n, &[], inline)
                        })
                        .collect();
                    if f.varargs && !list.is_empty() {
                        list.push("...".into());
                    }
                    let list = if list.is_empty() {
                        if f.prototyped && !f.varargs {
                            "void".to_string()
                        } else {
                            String::new()
                        }
                    } else {
                        list.join(", ")
                    };
                    inner = format!("{inner}({list})");
                    ty = &f.ret;
                }
                CType::Qualified(q, t) => {
                    let rest = self.declare(t, &inner, params.unwrap_or(&[]), inline);
                    return format!("{} {rest}", q.words());
                }
                _ => {
                    let spec = self.specifier(ty, inline);
                    return if inner.is_empty() {
                        spec
                    } else {
                        format!("{spec} {inner}")
                    };
                }
            }
        }
    }

    /// The type specifier of a base, named or void type.
    fn specifier(&self, ty: &CType, inline: &mut dyn FnMut(usize) -> String) -> String {
        match ty {
            CType::Base(s, _) | CType::Sized(_, s, _) => s.to_string(),
            CType::Named(e) => {
                let ent = &self.entities[*e];
                match ent.kind {
                    _ if ent.hidden() => self.specifier(&ent.target, inline),
                    Kind::Typedef | Kind::Function => ent.name.clone(),
                    _ if ent.inline() => inline(*e),
                    kind => format!("{} {}", kind.keyword(), ent.name),
                }
            }
            _ => "void".to_string(),
        }
    }

    /// A type in C, anonymous structures written `struct <anonymous>`.
    pub(crate) fn type_name(&self, ty: &CType) -> String {
        self.declare(ty, "", &[], &mut |e| {
            format!("{} <anonymous>", self.entities[e].kind.keyword())
        })
    }
}

fn size_of(entities: &[Entity], pointer_size: u64, ty: &CType) -> u64 {
    let mut ty = ty;
    for _ in 0..64 {
        match ty {
            CType::Void | CType::Function(_) => return 0,
            CType::Base(_, size) | CType::Sized(_, _, size) => return *size,
            CType::Pointer(..) => return pointer_size,
            CType::Array(elem, dims) => {
                let count: u64 = dims.iter().map(|d| d.unwrap_or(0)).product();
                return count.saturating_mul(size_of(entities, pointer_size, elem));
            }
            CType::Qualified(_, t) => ty = t,
            CType::Named(e) => {
                let e = &entities[*e];
                match e.kind {
                    Kind::Typedef => ty = &e.target,
                    Kind::Enum => return e.size.unwrap_or(4),
                    _ => return e.size.unwrap_or(0),
                }
            }
        }
    }
    0
}

fn resolve<'t>(entities: &'t [Entity], ty: &'t CType) -> &'t CType {
    let mut ty = ty;
    for _ in 0..64 {
        match ty {
            CType::Qualified(_, t) => ty = t,
            CType::Named(e) if entities[*e].kind == Kind::Typedef => ty = &entities[*e].target,
            _ => return ty,
        }
    }
    ty
}

/// The element type under arrays and qualifiers (not typedefs).
fn element(ty: &CType) -> &CType {
    let mut ty = ty;
    for _ in 0..64 {
        match ty {
            CType::Qualified(_, t) | CType::Array(t, _) => ty = t,
            _ => return ty,
        }
    }
    ty
}

impl Builder<'_> {
    /// Walks every unit, recording its types with their qualified names, and
    /// its external functions with code.
    fn walk(&mut self) {
        let debug = self.debug;
        for ui in 0..debug.units.len() as u32 {
            let Some(unit) = debug.unit(ui) else { continue };
            // Enclosing scopes: (depth, name, the type being read).
            let mut scopes: Vec<(isize, String, Option<usize>)> = Vec::new();
            let mut cursor = unit.entries();
            while let Ok(Some(die)) = cursor.next_dfs() {
                let depth = die.depth();
                while scopes.last().is_some_and(|s| s.0 >= depth) {
                    scopes.pop();
                }
                let tag = die.tag();
                let own = string_attr(&unit, die, gimli::DW_AT_name).filter(|n| !n.is_empty());
                let qualify = |name: &str| {
                    let mut q = String::new();
                    for (_, s, _) in &scopes {
                        if !s.is_empty() {
                            q.push_str(s);
                            q.push_str("::");
                        }
                    }
                    q.push_str(name);
                    q
                };
                let kind = match tag {
                    gimli::DW_TAG_structure_type | gimli::DW_TAG_class_type => Some(Kind::Struct),
                    gimli::DW_TAG_union_type => Some(Kind::Union),
                    gimli::DW_TAG_enumeration_type => Some(Kind::Enum),
                    gimli::DW_TAG_typedef => Some(Kind::Typedef),
                    _ => None,
                };
                let mut reading = None;
                if let Some(kind) = kind {
                    let index = self.found.len();
                    if kind == Kind::Typedef {
                        self.typedef_dies.insert((ui, die.offset().0), index);
                    } else {
                        reading = Some(index);
                    }
                    self.found.push(Found {
                        unit: ui,
                        offset: die.offset(),
                        kind,
                        name: own.as_deref().map(qualify),
                        declaration: die.attr_value(gimli::DW_AT_declaration).is_some(),
                        size: die.attr_value(gimli::DW_AT_byte_size).and_then(|v| v.udata_value()),
                        shape: String::new(),
                        scoped: die.attr_value(gimli::DW_AT_enum_class).is_some(),
                    });
                }
                // A member, base or enumerator adds to its type's shape.
                if matches!(
                    tag,
                    gimli::DW_TAG_member | gimli::DW_TAG_inheritance | gimli::DW_TAG_enumerator
                ) && let Some(&(d, _, Some(parent))) = scopes.last()
                    && d == depth - 1
                {
                    let at = member_offset(&unit, die).or_else(|| {
                        die.attr_value(gimli::DW_AT_data_bit_offset)
                            .and_then(|v| v.udata_value())
                    });
                    let value = match die.attr_value(gimli::DW_AT_const_value) {
                        Some(AttributeValue::Sdata(v)) => Some(i128::from(v)),
                        Some(v) => v.udata_value().map(i128::from),
                        None => None,
                    };
                    let shape = &mut self.found[parent].shape;
                    shape.push_str(own.as_deref().unwrap_or("?"));
                    shape.push_str(&format!("@{at:?}={value:?};"));
                }
                if tag == gimli::DW_TAG_subprogram {
                    self.subprogram(ui, die, own.as_deref().map(qualify));
                }
                if die.has_children() {
                    let scope = match tag {
                        gimli::DW_TAG_namespace
                        | gimli::DW_TAG_structure_type
                        | gimli::DW_TAG_class_type
                        | gimli::DW_TAG_union_type
                        | gimli::DW_TAG_enumeration_type
                        | gimli::DW_TAG_subprogram => own.clone().unwrap_or_default(),
                        _ => String::new(),
                    };
                    scopes.push((depth, scope, reading));
                }
            }
        }
    }

    /// Records a subprogram's qualified name, and the function if it is
    /// external and has code.
    fn subprogram(&mut self, ui: u32, die: &Die, qualified: Option<String>) {
        let debug = self.debug;
        let origin = die
            .attr_value(gimli::DW_AT_specification)
            .or_else(|| die.attr_value(gimli::DW_AT_abstract_origin))
            .and_then(|v| debug.resolve_ref(ui, v));
        let name = qualified.or_else(|| {
            let (u, o) = origin?;
            self.function_names.get(&(u, o.0)).cloned()
        });
        if let Some(n) = &name {
            self.function_names.insert((ui, die.offset().0), n.clone());
        }
        let flag = |d: &Die| match d.attr_value(gimli::DW_AT_external) {
            Some(AttributeValue::Flag(f)) => Some(f),
            Some(_) => Some(true),
            None => None,
        };
        let external = flag(die)
            .or_else(|| {
                let (u, o) = origin?;
                flag(&debug.unit(u)?.entry(o).ok()?)
            })
            .unwrap_or(false);
        let code = die.attr_value(gimli::DW_AT_low_pc).is_some() || die.attr_value(gimli::DW_AT_ranges).is_some();
        if external && code {
            match (name, origin) {
                (Some(n), _) => self.functions.push((ui, die.offset(), n)),
                // Its origin comes later in the unit: named after the walk.
                (None, Some(o)) => self.named_later.push((ui, die.offset(), o)),
                (None, None) => {}
            }
        }
    }

    /// Makes an entity of each distinct structure, union and enum: copies in
    /// other units (the same name, size and members) are one, and a
    /// declaration is the definition of its name.
    fn merge(&mut self) {
        let mut by_shape: HashMap<(Kind, String, u64, String), usize> = HashMap::new();
        let mut by_name: HashMap<(Kind, String), usize> = HashMap::new();
        let mut declarations = Vec::new();
        for i in 0..self.found.len() {
            let f = &self.found[i];
            if f.kind == Kind::Typedef {
                continue;
            }
            let Some(size) = f.size.filter(|_| !f.declaration) else {
                declarations.push(i);
                continue;
            };
            let name = f.name.clone().unwrap_or_default();
            let key = (f.kind, name.clone(), size, f.shape.clone());
            let (unit, offset) = (f.unit, f.offset);
            let e = match by_shape.get(&key) {
                Some(&e) => e,
                None => {
                    let e = self.new_entity(i, true);
                    by_shape.insert(key, e);
                    if !name.is_empty() {
                        by_name.entry((self.found[i].kind, name)).or_insert(e);
                    }
                    e
                }
            };
            self.by_die.insert((unit, offset.0), e);
        }
        for i in declarations {
            let f = &self.found[i];
            let key = (f.kind, f.name.clone().unwrap_or_default());
            let named = f.name.is_some();
            let (unit, offset) = (f.unit, f.offset);
            let e = match by_name.get(&key) {
                Some(&e) if named => e,
                _ => {
                    let e = self.new_entity(i, false);
                    if named {
                        by_name.insert(key, e);
                    }
                    e
                }
            };
            self.by_die.insert((unit, offset.0), e);
        }
    }

    fn new_entity(&mut self, found: usize, complete: bool) -> usize {
        let f = &self.found[found];
        self.entities.push(Entity {
            kind: f.kind,
            source: f.name.clone().unwrap_or_default(),
            name: String::new(),
            unit: f.unit,
            die: f.offset,
            complete,
            size: f.size.filter(|_| complete),
            target: CType::Void,
            enumerators: Vec::new(),
            params: Vec::new(),
            scoped: f.scoped,
        });
        self.entities.len() - 1
    }

    /// A typedef's entity: typedefs of one name and target are one.
    fn typedef(&mut self, found: usize) -> Option<usize> {
        if let Some(&e) = self.typedef_entity.get(&found) {
            return e;
        }
        self.typedef_entity.insert(found, None);
        let (unit, offset) = (self.found[found].unit, self.found[found].offset);
        let target = self.inner(unit, offset);
        let e = self.found[found].name.clone().map(|name| {
            let key = (name, target);
            match self.typedefs.get(&key) {
                Some(&e) => e,
                None => {
                    let e = self.new_entity(found, true);
                    self.entities[e].target = key.1.clone();
                    self.typedefs.insert(key, e);
                    e
                }
            }
        });
        self.typedef_entity.insert(found, e);
        if let Some(e) = e {
            self.by_die.insert((unit, offset.0), e);
        }
        e
    }

    /// The type a DIE's `DW_AT_type` names (void without one).
    fn inner(&mut self, unit: u32, offset: UnitOffset) -> CType {
        let target = self
            .debug
            .unit(unit)
            .and_then(|u| u.entry(offset).ok())
            .and_then(|d| d.attr_value(gimli::DW_AT_type))
            .and_then(|v| self.debug.resolve_ref(unit, v));
        match target {
            Some((u, o)) => self.ctype(u, o),
            None => CType::Void,
        }
    }

    /// The C type of a type DIE.
    fn ctype(&mut self, unit: u32, offset: UnitOffset) -> CType {
        if let Some(t) = self.ctypes.get(&(unit, offset.0)) {
            return t.clone();
        }
        // A chain of types that leads back to itself (broken DWARF) ends as void.
        if self.depth > 64 {
            return CType::Void;
        }
        self.depth += 1;
        let t = self.make_ctype(unit, offset);
        self.depth -= 1;
        self.ctypes.insert((unit, offset.0), t.clone());
        t
    }

    fn make_ctype(&mut self, unit_index: u32, offset: UnitOffset) -> CType {
        let debug = self.debug;
        let Some(unit) = debug.unit(unit_index) else {
            return CType::Void;
        };
        let Ok(die) = unit.entry(offset) else {
            return CType::Void;
        };
        let size = die.attr_value(gimli::DW_AT_byte_size).and_then(|v| v.udata_value());
        let pointer_size = self.pointer_size;
        match die.tag() {
            gimli::DW_TAG_base_type => {
                let name = string_attr(&unit, &die, gimli::DW_AT_name).unwrap_or_default();
                let encoding = match die.attr_value(gimli::DW_AT_encoding) {
                    Some(AttributeValue::Encoding(e)) => e,
                    _ => gimli::DW_ATE_signed,
                };
                base_type(&name, encoding, size.unwrap_or(0), pointer_size)
            }
            // C++'s `decltype(nullptr)` is a pointer; anything else is void.
            gimli::DW_TAG_unspecified_type => {
                let name = string_attr(&unit, &die, gimli::DW_AT_name).unwrap_or_default();
                if name.contains("nullptr") {
                    CType::Pointer(Box::new(CType::Void), Quals::default())
                } else {
                    CType::Void
                }
            }
            // References are pointers in C; a pointer of another size than
            // the target's (near, far) is its bytes.
            gimli::DW_TAG_pointer_type | gimli::DW_TAG_reference_type | gimli::DW_TAG_rvalue_reference_type => {
                match size {
                    Some(s) if s != pointer_size => CType::bytes(s),
                    _ => CType::Pointer(Box::new(self.inner(unit_index, offset)), Quals::default()),
                }
            }
            // A pointer to a member function is two words (Itanium), to a member one.
            gimli::DW_TAG_ptr_to_member_type => {
                let to = self.inner(unit_index, offset);
                let function = matches!(resolve(&self.entities, &to), CType::Function(_));
                CType::bytes(size.unwrap_or(if function { 2 * pointer_size } else { pointer_size }))
            }
            gimli::DW_TAG_const_type | gimli::DW_TAG_volatile_type => {
                let constant = die.tag() == gimli::DW_TAG_const_type;
                let add = |mut q: Quals| {
                    if constant {
                        q.constant = true;
                    } else {
                        q.volatile = true;
                    }
                    q
                };
                match self.inner(unit_index, offset) {
                    // A qualified pointer is `T *const`.
                    CType::Pointer(to, q) => CType::Pointer(to, add(q)),
                    CType::Qualified(q, t) => CType::Qualified(add(q), t),
                    // An array's qualifiers are its elements'.
                    CType::Array(elem, dims) => {
                        CType::Array(Box::new(CType::Qualified(add(Quals::default()), elem)), dims)
                    }
                    t @ CType::Function(_) => t,
                    t => CType::Qualified(add(Quals::default()), Box::new(t)),
                }
            }
            gimli::DW_TAG_restrict_type
            | gimli::DW_TAG_atomic_type
            | gimli::DW_TAG_immutable_type
            | gimli::DW_TAG_shared_type
            | gimli::DW_TAG_packed_type => self.inner(unit_index, offset),
            gimli::DW_TAG_typedef => match self.typedef_dies.get(&(unit_index, offset.0)).copied() {
                Some(found) => match self.typedef(found) {
                    Some(e) => CType::Named(e),
                    None => self.inner(unit_index, offset),
                },
                None => self.inner(unit_index, offset),
            },
            gimli::DW_TAG_structure_type | gimli::DW_TAG_class_type | gimli::DW_TAG_union_type => {
                match self.by_die.get(&(unit_index, offset.0)) {
                    Some(&e) => CType::Named(e),
                    None => CType::Void,
                }
            }
            gimli::DW_TAG_enumeration_type => {
                let Some(&e) = self.by_die.get(&(unit_index, offset.0)) else {
                    return CType::Void;
                };
                let size = self.entities[e].size.or(size).unwrap_or(4);
                let enumerated = self.entities[e].complete && die.has_children();
                if size == 4 && enumerated {
                    CType::Named(e)
                } else {
                    // An integer of its size, as signed as its underlying type.
                    let under = self.inner(unit_index, offset);
                    let signed = is_signed(resolve(&self.entities, &under));
                    match integer(size, signed, pointer_size) {
                        Some(s) => CType::Sized(e, s, size),
                        None => CType::bytes(size),
                    }
                }
            }
            gimli::DW_TAG_array_type => {
                let elem = self.inner(unit_index, offset);
                let mut dims = Vec::new();
                if let Ok(mut cursor) = unit.entries_at_offset(offset)
                    && matches!(cursor.next_entry(), Ok(true))
                    && cursor.current().is_some_and(|d| d.has_children())
                    && matches!(cursor.next_entry(), Ok(true))
                {
                    while let Some(child) = cursor.current() {
                        if child.tag() == gimli::DW_TAG_subrange_type {
                            dims.push(subrange_count(child));
                        }
                        if !matches!(cursor.next_sibling(), Ok(Some(_))) {
                            break;
                        }
                    }
                }
                if dims.is_empty() {
                    dims.push(None);
                }
                // An array whose size says otherwise than its bounds (a
                // vector, say) is its bytes.
                let length: Option<u64> = dims.iter().copied().product();
                let element_size = size_of(&self.entities, pointer_size, &elem);
                match (size, length) {
                    (Some(s), Some(n)) if n > 0 && s != n.saturating_mul(element_size) => CType::bytes(s),
                    _ => CType::Array(Box::new(elem), dims),
                }
            }
            gimli::DW_TAG_subroutine_type => CType::Function(Box::new(self.function_type(unit_index, offset).0)),
            _ => match size {
                Some(s) if s > 0 => CType::bytes(s),
                _ => CType::Void,
            },
        }
    }

    /// A function's type and its parameters' names, from a subprogram or a
    /// subroutine type (a definition keeps some of it on its declaration).
    fn function_type(&mut self, unit_index: u32, offset: UnitOffset) -> (Func, Vec<String>) {
        let debug = self.debug;
        let mut func = Func {
            ret: CType::Void,
            params: Vec::new(),
            varargs: false,
            prototyped: false,
            convention: None,
        };
        let Some(unit) = debug.unit(unit_index) else {
            return (func, Vec::new());
        };
        let Ok(die) = unit.entry(offset) else {
            return (func, Vec::new());
        };
        let origin = die
            .attr_value(gimli::DW_AT_specification)
            .or_else(|| die.attr_value(gimli::DW_AT_abstract_origin))
            .and_then(|v| debug.resolve_ref(unit_index, v));
        let attr = |at: gimli::DwAt| {
            die.attr_value(at).map(|v| (unit_index, v)).or_else(|| {
                let (u, o) = origin?;
                Some((u, debug.unit(u)?.entry(o).ok()?.attr_value(at)?))
            })
        };
        let ret = attr(gimli::DW_AT_type).and_then(|(u, v)| debug.resolve_ref(u, v));
        func.prototyped = attr(gimli::DW_AT_prototyped).is_some() || self.cplusplus(unit_index);
        if let Some((_, AttributeValue::CallingConvention(cc))) = attr(gimli::DW_AT_calling_convention) {
            func.convention = Convention::from_dwarf(cc);
        }
        // The out-of-line copy of an inlined function may leave out parameters
        // it doesn't use: its abstract instance lists them all.
        let (list_index, list_offset) = die
            .attr_value(gimli::DW_AT_abstract_origin)
            .and_then(|v| debug.resolve_ref(unit_index, v))
            .unwrap_or((unit_index, offset));
        let mut params = Vec::new();
        if let Some(unit) = debug.unit(list_index)
            && let Ok(mut cursor) = unit.entries_at_offset(list_offset)
            && matches!(cursor.next_entry(), Ok(true))
            && cursor.current().is_some_and(|d| d.has_children())
            && matches!(cursor.next_entry(), Ok(true))
        {
            while let Some(child) = cursor.current() {
                match child.tag() {
                    gimli::DW_TAG_formal_parameter => {
                        let origin = child
                            .attr_value(gimli::DW_AT_abstract_origin)
                            .and_then(|v| debug.resolve_ref(list_index, v))
                            .and_then(|(u, o)| Some((u, debug.unit(u)?, o)))
                            .and_then(|(u, unit, o)| Some((u, unit.entry(o).ok()?, unit)));
                        let ty = child
                            .attr_value(gimli::DW_AT_type)
                            .and_then(|v| debug.resolve_ref(list_index, v))
                            .or_else(|| {
                                let (u, d, _) = origin.as_ref()?;
                                debug.resolve_ref(*u, d.attr_value(gimli::DW_AT_type)?)
                            });
                        let name = string_attr(&unit, child, gimli::DW_AT_name).or_else(|| {
                            let (_, d, unit) = origin.as_ref()?;
                            string_attr(unit, d, gimli::DW_AT_name)
                        });
                        params.push((ty, name));
                    }
                    gimli::DW_TAG_unspecified_parameters => func.varargs = true,
                    _ => {}
                }
                if !matches!(cursor.next_sibling(), Ok(Some(_))) {
                    break;
                }
            }
        }
        func.ret = match ret {
            Some((u, o)) => self.ctype(u, o),
            None => CType::Void,
        };
        let mut names = Vec::new();
        for (ty, name) in params {
            let ty = match ty {
                Some((u, o)) => self.ctype(u, o),
                None => CType::Void,
            };
            // A parameter of void (Rust's `()`, which takes no argument) is
            // left out: in C, void is only ever the whole list, `(void)`.
            if *resolve(&self.entities, &ty) == CType::Void {
                continue;
            }
            func.params.push(ty);
            names.push(name.unwrap_or_default());
        }
        (func, names)
    }

    /// Whether a unit is C++ (whose functions always have prototypes).
    fn cplusplus(&self, unit: u32) -> bool {
        self.debug
            .units()
            .get(unit as usize)
            .and_then(|u| u.language.as_deref())
            .is_some_and(|l| l.starts_with("C_plus_plus"))
    }

    /// Each enum's enumerators, from its DIE.
    fn enumerators(&mut self) {
        let debug = self.debug;
        for e in 0..self.entities.len() {
            if self.entities[e].kind != Kind::Enum {
                continue;
            }
            let (ui, offset) = (self.entities[e].unit, self.entities[e].die);
            let Some(unit) = debug.unit(ui) else { continue };
            // A constant of a fixed size in a signed enum is sign-extended, and
            // one written unsigned but too large for the type is negative.
            let under = self.inner(ui, offset);
            let signed = under != CType::Void && is_signed(resolve(&self.entities, &under));
            let width = match size_of(&self.entities, self.pointer_size, &under) {
                0 => self.entities[e].size.unwrap_or(4),
                s => s,
            }
            .min(8) as u32
                * 8;
            let mut list = Vec::new();
            if let Ok(mut cursor) = unit.entries_at_offset(offset)
                && matches!(cursor.next_entry(), Ok(true))
                && cursor.current().is_some_and(|d| d.has_children())
                && matches!(cursor.next_entry(), Ok(true))
            {
                while let Some(child) = cursor.current() {
                    if child.tag() == gimli::DW_TAG_enumerator
                        && let Some(name) = string_attr(&unit, child, gimli::DW_AT_name)
                    {
                        let mut value = match child.attr_value(gimli::DW_AT_const_value) {
                            Some(AttributeValue::Sdata(v)) => i128::from(v),
                            Some(AttributeValue::Data1(v)) if signed => i128::from(v as i8),
                            Some(AttributeValue::Data2(v)) if signed => i128::from(v as i16),
                            Some(AttributeValue::Data4(v)) if signed => i128::from(v as i32),
                            Some(AttributeValue::Data8(v)) if signed => i128::from(v as i64),
                            Some(v) => v.udata_value().map_or(0, i128::from),
                            None => 0,
                        };
                        if signed && width > 0 && value >= 1 << (width - 1) && value < 1 << width {
                            value -= 1 << width;
                        }
                        list.push((name, String::new(), value));
                    }
                    if !matches!(cursor.next_sibling(), Ok(Some(_))) {
                        break;
                    }
                }
            }
            self.entities[e].enumerators = list;
        }
    }

    /// An entity for each external function, one per name and type.
    fn functions(&mut self) {
        let list = std::mem::take(&mut self.functions);
        let mut seen: HashMap<(String, CType), usize> = HashMap::new();
        for (unit, offset, name) in list {
            let (func, params) = self.function_type(unit, offset);
            let ty = CType::Function(Box::new(func));
            let key = (name.clone(), ty.clone());
            let e = match seen.get(&key) {
                Some(&e) => e,
                None => {
                    self.entities.push(Entity {
                        kind: Kind::Function,
                        source: name,
                        name: String::new(),
                        unit,
                        die: offset,
                        complete: true,
                        size: None,
                        target: ty,
                        enumerators: Vec::new(),
                        params,
                        scoped: false,
                    });
                    let e = self.entities.len() - 1;
                    seen.insert(key, e);
                    e
                }
            };
            self.by_die.insert((unit, offset.0), e);
        }
    }

    /// Names every entity in C: tags (structures, unions and enums) apart
    /// from ordinary identifiers (typedefs, enumerators, functions), as C
    /// keeps them; a name taken twice is numbered (`node_2`). An anonymous
    /// type named by a typedef takes the typedef's name as its tag.
    fn name(&mut self) {
        let mut named_by: HashMap<usize, usize> = HashMap::new();
        for t in 0..self.entities.len() {
            if self.entities[t].kind == Kind::Typedef
                && let CType::Named(e) = self.entities[t].target
                && self.entities[e].source.is_empty()
                && self.entities[e].kind != Kind::Typedef
            {
                named_by.entry(e).or_insert(t);
            }
        }
        let mut tags = HashSet::new();
        let mut anonymous_enums = 0;
        for e in 0..self.entities.len() {
            let ent = &self.entities[e];
            if !matches!(ent.kind, Kind::Struct | Kind::Union | Kind::Enum) {
                continue;
            }
            let base = if !ent.source.is_empty() {
                c_identifier(&ent.source)
            } else if let Some(&t) = named_by.get(&e) {
                c_identifier(&self.entities[t].source)
            } else if ent.kind == Kind::Enum {
                anonymous_enums += 1;
                format!("anon_enum_{anonymous_enums}")
            } else {
                // Written in place where it is used.
                continue;
            };
            self.entities[e].name = unique(&mut tags, &base);
        }
        // A typedef of a tag's own name (`typedef struct list list`, which a
        // PDB has for every structure of a C program) comes last, and gives
        // way to any other name rather than take a number: it is then left
        // out, and its uses name the tag.
        let same_as_tag = |entities: &[Entity], t: usize| {
            entities[t].kind == Kind::Typedef
                && matches!(entities[t].target, CType::Named(e)
                    if entities[e].kind != Kind::Typedef && entities[e].name == c_identifier(&entities[t].source))
        };
        let tag_named: Vec<usize> = (0..self.entities.len())
            .filter(|&t| same_as_tag(&self.entities, t))
            .collect();
        let mut ordinary = HashSet::new();
        for kind in [Kind::Typedef, Kind::Enum, Kind::Function] {
            for e in 0..self.entities.len() {
                if self.entities[e].kind != kind || tag_named.binary_search(&e).is_ok() {
                    continue;
                }
                if kind == Kind::Enum {
                    let prefix = self.entities[e].name.clone();
                    let scoped = self.entities[e].scoped;
                    for i in 0..self.entities[e].enumerators.len() {
                        let base = c_identifier(&self.entities[e].enumerators[i].0);
                        let base = if scoped || ordinary.contains(&base) {
                            format!("{prefix}__{base}")
                        } else {
                            base
                        };
                        self.entities[e].enumerators[i].1 = unique(&mut ordinary, &base);
                    }
                } else {
                    let base = c_identifier(&self.entities[e].source);
                    self.entities[e].name = unique(&mut ordinary, &base);
                }
            }
        }
        for t in tag_named {
            let base = c_identifier(&self.entities[t].source);
            if ordinary.insert(base.clone()) {
                self.entities[t].name = base;
            }
        }
        // Parameters may not be called what a typedef is.
        for e in 0..self.entities.len() {
            if self.entities[e].kind == Kind::Function {
                let mut taken = HashSet::new();
                let params: Vec<String> = self.entities[e]
                    .params
                    .iter()
                    .map(|p| {
                        let n = if p.is_empty() { String::new() } else { c_identifier(p) };
                        if n.is_empty() || ordinary.contains(&n) || n.starts_with("BINVIZ_") {
                            String::new()
                        } else {
                            unique(&mut taken, &n)
                        }
                    })
                    .collect();
                self.entities[e].params = params;
            }
        }
    }

    fn ensure_layout(&mut self, e: usize) {
        if e >= self.layouts.len() {
            self.layouts.resize(e + 1, None);
            self.building.resize(e + 1, false);
        }
        let ent = &self.entities[e];
        // A type of no size (Rust's zero-sized types, GNU C's empty
        // structures) has no layout C gives every compiler: it is declared only.
        if self.layouts[e].is_some()
            || self.building[e]
            || !matches!(ent.kind, Kind::Struct | Kind::Union)
            || !ent.complete
            || ent.size == Some(0)
        {
            return;
        }
        self.building[e] = true;
        let layout = self.build_layout(e);
        self.building[e] = false;
        self.layouts[e] = Some(layout);
    }

    /// The layout of the structure or union a type is, by value (through
    /// typedefs, qualifiers and arrays), worked out first: its alignment
    /// counts in a structure that holds it.
    fn ensure_by_value(&mut self, ty: &CType) -> Option<usize> {
        let mut ty = ty.clone();
        for _ in 0..64 {
            ty = match ty {
                CType::Qualified(_, t) | CType::Array(t, _) => *t,
                CType::Named(e) if self.entities[e].kind == Kind::Typedef => self.entities[e].target.clone(),
                CType::Named(e) if matches!(self.entities[e].kind, Kind::Struct | Kind::Union) => {
                    self.ensure_layout(e);
                    return Some(e);
                }
                _ => return None,
            };
        }
        None
    }

    fn build_layout(&mut self, e: usize) -> Layout {
        let debug = self.debug;
        let (ui, offset) = (self.entities[e].unit, self.entities[e].die);
        let union = self.entities[e].kind == Kind::Union;
        let size = self.entities[e].size.unwrap_or(0);
        let mut notes = Vec::new();
        let mut pieces: Vec<Piece> = Vec::new();
        // Bases, for later: (index in `pieces`, base entity, its source name).
        let mut bases: Vec<(usize, usize, String)> = Vec::new();
        if let Some(unit) = debug.unit(ui)
            && let Ok(mut cursor) = unit.entries_at_offset(offset)
            && matches!(cursor.next_entry(), Ok(true))
            && cursor.current().is_some_and(|d| d.has_children())
            && matches!(cursor.next_entry(), Ok(true))
        {
            while let Some(child) = cursor.current() {
                let tag = child.tag();
                let is_static = tag == gimli::DW_TAG_variable
                    || child.attr_value(gimli::DW_AT_external).is_some()
                    || child.attr_value(gimli::DW_AT_declaration).is_some();
                if matches!(tag, gimli::DW_TAG_member | gimli::DW_TAG_inheritance) && !is_static {
                    let name = string_attr(&unit, child, gimli::DW_AT_name).filter(|n| !n.is_empty());
                    let type_die = child
                        .attr_value(gimli::DW_AT_type)
                        .and_then(|v| debug.resolve_ref(ui, v));
                    let ty = match type_die {
                        Some((u, o)) => self.ctype(u, o),
                        None => CType::Void,
                    };
                    let at = member_offset(&unit, child).or(union.then_some(0));
                    let virtual_base = matches!(
                        child.attr_value(gimli::DW_AT_virtuality),
                        Some(AttributeValue::Virtuality(v)) if v != gimli::DW_VIRTUALITY_none
                    );
                    let bits = child.attr_value(gimli::DW_AT_bit_size).and_then(|v| v.udata_value());
                    let first = bits.and_then(|width| {
                        let storage = type_die.and_then(|(u, o)| debug.type_size(u, o, 0));
                        member_bit_offset(child, width, at, storage, self.little)
                    });
                    let artificial = child.attr_value(gimli::DW_AT_artificial).is_some();
                    let piece = self.piece(tag, name, ty, at, bits.zip(first), artificial, virtual_base, &mut notes);
                    if let Some((piece, base)) = piece {
                        if let Some(b) = base {
                            let source = self.entities[b].source.clone();
                            bases.push((pieces.len(), b, source));
                        }
                        pieces.push(piece);
                    }
                }
                if !matches!(cursor.next_sibling(), Ok(Some(_))) {
                    break;
                }
            }
        }
        // A base the members after it reach into (the Itanium ABI reuses a
        // base's tail padding) can't be embedded whole: its members are.
        for (index, b, source) in bases.into_iter().rev() {
            let (start, end) = pieces[index].bits();
            let shared = pieces
                .iter()
                .enumerate()
                .any(|(i, p)| i != index && p.bits().0 < end && p.bits().1 > start && p.bits().1 > p.bits().0);
            if shared && let Some(layout) = self.layouts[b].as_ref() {
                let flat = to_pieces(&layout.items, start / 8);
                notes.push((
                    start / 8,
                    format!("base class {source}: its members, as the members after it use its tail padding"),
                ));
                pieces.splice(index..=index, flat);
            }
        }
        // A member past the end: rustc describes a struct's unsized tail
        // (`ArcInner<[u8]>`'s `data`) and a function item (of no size) that
        // way. One right at the end is kept as an array of none of it (`u8
        // data[0]`); any other is left out.
        pieces.retain_mut(|p| {
            let (start, end) = p.bits();
            if end <= size * 8 {
                return true;
            }
            match p {
                Piece::Field(f) if start == size * 8 => {
                    let ty = std::mem::replace(&mut f.ty, CType::Void);
                    f.ty = CType::Array(Box::new(ty), vec![Some(0)]);
                    f.size = 0;
                    true
                }
                _ => {
                    let name = match p {
                        Piece::Field(f) => f.name.clone(),
                        Piece::Bit { name, .. } => name.clone().unwrap_or_default(),
                        Piece::Group { .. } => "an anonymous member".into(),
                    };
                    notes.push((start / 8, format!("{name}: past the end of the structure")));
                    false
                }
            }
        });
        pieces.sort_by_key(|p| p.bits().0);
        if !union {
            pieces = regroup(pieces);
        }
        let mut unaligned = false;
        let mut items = self.finish(pieces, union, 0, size, &mut unaligned);
        // Member names are unique in a structure, members of anonymous ones included.
        let mut taken = HashSet::new();
        rename(&mut items, &mut taken, false);
        rename(&mut items, &mut taken, true);
        notes.sort_by_key(|n| n.0);
        let mut layout = Layout {
            union,
            size,
            items,
            pack: None,
            align: 1,
            notes,
        };
        for pack in [16, 8, 4, 2, 1] {
            if unaligned && pack > 1 {
                continue;
            }
            if let Some(align) = self.fits(&layout.items, 0, union, size, pack) {
                layout.pack = (pack < 16).then_some(pack);
                layout.align = align;
                break;
            }
        }
        layout
    }

    /// One member as a piece of the layout, and the base entity when it is a base.
    #[allow(clippy::too_many_arguments)]
    fn piece(
        &mut self,
        tag: gimli::DwTag,
        name: Option<String>,
        ty: CType,
        at: Option<u64>,
        bits: Option<(u64, u64)>,
        artificial: bool,
        virtual_base: bool,
        notes: &mut Vec<(u64, String)>,
    ) -> Option<(Piece, Option<usize>)> {
        if tag == gimli::DW_TAG_inheritance {
            let b = self.ensure_by_value(&ty).filter(|_| !matches!(ty, CType::Array(..)));
            let source = b.map_or_else(|| self.type_name(&ty), |b| self.entities[b].source.clone());
            let Some(at) = at.filter(|_| !virtual_base) else {
                notes.push((
                    at.unwrap_or(0),
                    format!("virtual base class {source}: where it is, the most derived class says"),
                ));
                return None;
            };
            let b = b?;
            let layout = self.layouts[b].as_ref()?;
            // An empty base takes no room.
            if layout.items.iter().all(|i| matches!(i, Item::Pad { .. })) {
                return None;
            }
            let own = match self.entities[b].source.as_str() {
                "" => self.entities[b].name.clone(),
                s => c_identifier(own_name(s)),
            };
            let field = Field {
                name: format!("base_{own}"),
                ty,
                offset: at,
                size: layout.size,
                note: Some(format!("base class {source}")),
            };
            return Some((Piece::Field(field), Some(b)));
        }
        if let Some((width, first)) = bits {
            return Some((Piece::Bit { name, ty, first, width }, None));
        }
        let at = at?;
        let size = size_of(&self.entities, self.pointer_size, &ty);
        let vptr = artificial
            && name
                .as_deref()
                .is_some_and(|n| n.starts_with("_vptr") || n == "__vfptr" || n == "__vptr");
        if vptr {
            let void = CType::Pointer(Box::new(CType::Void), Quals::default());
            let field = Field {
                name: "__vftable".into(),
                ty: CType::Pointer(Box::new(void), Quals::default()),
                offset: at,
                size: self.pointer_size,
                note: Some("vtable pointer".into()),
            };
            return Some((Piece::Field(field), None));
        }
        let by_value = self.ensure_by_value(&ty);
        // A member of a type the debug info never defines: its bytes are padding.
        if let Some(s) = by_value
            && !self.entities[s].complete
        {
            notes.push((
                at,
                format!("{}: {}, never defined", name.unwrap_or_default(), self.type_name(&ty)),
            ));
            return None;
        }
        let Some(name) = name else {
            if size == 0 {
                return None;
            }
            // An anonymous member: its type's members, in place.
            if let Some(s) = by_value
                && !matches!(ty, CType::Array(..))
                && let Some(layout) = self.layouts[s].as_ref()
            {
                let group = Piece::Group {
                    union: layout.union,
                    offset: at,
                    size: layout.size,
                    pieces: to_pieces(&layout.items, at),
                };
                return Some((group, None));
            }
            let field = Field {
                name: format!("_unnamed_{at:x}"),
                ty,
                offset: at,
                size,
                note: None,
            };
            return Some((Piece::Field(field), None));
        };
        // Nothing of no size but an array of something (`char data[0]`) is worth declaring.
        let array = matches!(
            resolve(&self.entities, &ty),
            CType::Array(elem, _) if size_of(&self.entities, self.pointer_size, elem) > 0
        );
        if size == 0 && !array {
            notes.push((at, format!("{name}: {}, of no size", self.type_name(&ty))));
            return None;
        }
        let field = Field {
            name,
            ty,
            offset: at,
            size,
            note: None,
        };
        Some((Piece::Field(field), None))
    }

    /// A type's name in the source, for a note.
    fn type_name(&self, ty: &CType) -> String {
        match resolve(&self.entities, element(ty)) {
            CType::Named(e) => {
                let ent = &self.entities[*e];
                let name = if ent.source.is_empty() { &ent.name } else { &ent.source };
                format!("{} {name}", ent.kind.keyword())
            }
            _ => "a type".to_string(),
        }
    }

    /// Pieces as items: bit fields put in storage units, and every gap
    /// padded, from `start` for `size` bytes.
    fn finish(&self, pieces: Vec<Piece>, union: bool, start: u64, size: u64, unaligned: &mut bool) -> Vec<Item> {
        let mut items = Vec::new();
        if union {
            let whole = size;
            let mut largest = 0;
            for p in pieces {
                let item = match p {
                    Piece::Field(f) if f.offset == start => Item::Field(f),
                    Piece::Group {
                        union,
                        offset,
                        size,
                        pieces,
                    } if offset == start => Item::Group(Group {
                        union,
                        offset,
                        size,
                        items: self.finish(pieces, union, offset, size, unaligned),
                    }),
                    // Anything not at the start (a bit field) sits in a structure of its own.
                    p => Item::Group(Group {
                        union: false,
                        offset: start,
                        size: whole,
                        items: self.finish(vec![p], false, start, whole, unaligned),
                    }),
                };
                largest = largest.max(item.size());
                items.push(item);
            }
            // A union larger than its members (for its alignment) says so.
            if largest < size {
                items.push(Item::Pad {
                    offset: start,
                    size,
                    name: format!("_pad_{start:x}"),
                });
            }
            return items;
        }
        let end = start + size;
        let mut at = start;
        let pad = |items: &mut Vec<Item>, from: u64, to: u64| {
            if to > from {
                items.push(Item::Pad {
                    offset: from,
                    size: to - from,
                    name: format!("_pad_{from:x}"),
                });
            }
        };
        let mut i = 0;
        while i < pieces.len() {
            match &pieces[i] {
                Piece::Bit { .. } => {
                    let mut j = i;
                    while j < pieces.len() && matches!(pieces[j], Piece::Bit { .. }) {
                        j += 1;
                    }
                    let upper = pieces.get(j).map_or(end, |p| p.bits().0 / 8);
                    for unit in self.units(&pieces[i..j], at, upper, unaligned) {
                        pad(&mut items, at, unit.offset);
                        at = at.max(unit.offset + unit.size);
                        items.push(Item::Bits(unit));
                    }
                    i = j;
                }
                Piece::Field(f) => {
                    pad(&mut items, at, f.offset);
                    at = at.max(f.offset + f.size);
                    items.push(Item::Field(f.clone()));
                    i += 1;
                }
                Piece::Group {
                    union,
                    offset,
                    size,
                    pieces: inner,
                } => {
                    pad(&mut items, at, *offset);
                    at = at.max(offset + size);
                    items.push(Item::Group(Group {
                        union: *union,
                        offset: *offset,
                        size: *size,
                        items: self.finish(inner.clone(), *union, *offset, *size, unaligned),
                    }));
                    i += 1;
                }
            }
        }
        pad(&mut items, at, end);
        items
    }

    /// Bit fields in storage units, between byte `lower` (where the member
    /// before ends) and `upper` (where the one after starts). A unit starts
    /// at a multiple of its size, which is the first field's declared type's
    /// when that fits there, else one that does; it grows (up to 8 bytes) to
    /// take in a field that starts in it but reaches past its end, as System
    /// V lets an `int` field and a `long long` one share storage. Its fields
    /// are all declared with a type of its size, and it is filled to its
    /// end: Microsoft's rules and System V's then place it the same.
    fn units(&self, run: &[Piece], lower: u64, upper: u64, unaligned: &mut bool) -> Vec<Bits> {
        let bits: Vec<(&Option<String>, &CType, u64, u64)> = run
            .iter()
            .filter_map(|p| match p {
                Piece::Bit { name, ty, first, width } => Some((name, ty, *first, *width)),
                _ => None,
            })
            .collect();
        let mut out = Vec::new();
        let mut floor = lower;
        let mut k = 0;
        while k < bits.len() {
            let (_, ty, first, width) = bits[k];
            // One that starts before the last unit's end overlaps what is there: left out.
            if first < 8 * floor {
                k += 1;
                continue;
            }
            let declared = match size_of(&self.entities, self.pointer_size, ty) {
                s @ (1 | 2 | 4 | 8) => s,
                _ => 4,
            };
            // A unit of `s` bytes, at a multiple of `s`, holding bits `first..end`.
            let place = |s: u64, end: u64| {
                let u = first / (8 * s) * s;
                (u >= floor && u + s <= upper && end <= 8 * (u + s)).then_some(u)
            };
            let mut sizes = vec![declared];
            sizes.extend([8, 4, 2, 1].into_iter().filter(|&s| s < declared));
            sizes.extend([1, 2, 4, 8].into_iter().filter(|&s| s > declared));
            let (mut unit, mut s, aligned) = match sizes.iter().find_map(|&s| place(s, first + width).map(|u| (u, s))) {
                Some((u, s)) => (u, s, true),
                // No unit of its own alignment fits between its neighbours:
                // one from its first byte, which only a packed structure has.
                None => {
                    *unaligned = true;
                    let u = first / 8;
                    let need = (first + width).div_ceil(8) - u;
                    (u, [1, 2, 4, 8].into_iter().find(|&s| s >= need).unwrap_or(8), false)
                }
            };
            // The fields after it that start in the unit, which grows to hold them.
            let mut m = k + 1;
            while let Some(&(_, _, f, w)) = bits.get(m) {
                if f >= 8 * (unit + s) {
                    break;
                }
                if f + w > 8 * (unit + s) {
                    let grown = [2, 4, 8].into_iter().filter(|&g| g > s).find_map(|g| {
                        if aligned {
                            place(g, f + w).map(|u| (u, g))
                        } else {
                            (unit + g <= upper && f + w <= 8 * (unit + g)).then_some((unit, g))
                        }
                    });
                    match grown {
                        Some((u, g)) => (unit, s) = (u, g),
                        None => break,
                    }
                }
                m += 1;
            }
            let end = 8 * (unit + s);
            let filler = CType::Base(integer(s, false, 8).unwrap_or("unsigned int"), s);
            let mut fields = Vec::new();
            let mut at = 8 * unit;
            for &(name, ty, first, width) in &bits[k..m] {
                if first < at {
                    // Overlaps the field before it: left out.
                    continue;
                }
                if first > at {
                    fields.push(BitField {
                        name: None,
                        ty: filler.clone(),
                        first: at - 8 * unit,
                        width: first - at,
                    });
                }
                fields.push(BitField {
                    name: name.clone(),
                    ty: self.unit_type(ty, s),
                    first: first - 8 * unit,
                    width,
                });
                at = first + width;
            }
            if at < end {
                fields.push(BitField {
                    name: None,
                    ty: filler,
                    first: at - 8 * unit,
                    width: end - at,
                });
            }
            out.push(Bits {
                offset: unit,
                size: s,
                fields,
            });
            floor = unit + s;
            k = m;
        }
        out
    }

    /// A bit field's type in a unit of `size` bytes: its own when that is an
    /// integer or enum of the unit's size, else an integer of that size as
    /// signed as its own.
    fn unit_type(&self, ty: &CType, size: u64) -> CType {
        let own = resolve(&self.entities, ty);
        let integral = match own {
            CType::Base(s, _) => !s.contains("float") && !s.contains("double") && !s.contains("_Complex"),
            CType::Sized(..) => true,
            CType::Named(e) => self.entities[*e].kind == Kind::Enum,
            _ => false,
        };
        if integral && size_of(&self.entities, self.pointer_size, ty) == size {
            return ty.clone();
        }
        let signed = match own {
            CType::Named(_) => true,
            other => is_signed(other),
        };
        CType::Base(integer(size, signed, 8).unwrap_or("unsigned int"), size)
    }

    /// Whether items lay out at their offsets with every type aligned to its
    /// strictest alignment capped at `pack` (padding members fill the gaps),
    /// and if so, the alignment of the whole.
    fn fits(&self, items: &[Item], start: u64, union: bool, size: u64, pack: u64) -> Option<u64> {
        let mut align = 1;
        for item in items {
            let a = match item {
                Item::Field(f) => self.field_align(&f.ty, pack)?,
                Item::Bits(b) => b.size.min(pack),
                Item::Group(g) => self.fits(&g.items, g.offset, g.union, g.size, pack)?,
                Item::Pad { .. } => 1,
            };
            if !(item.offset() - start).is_multiple_of(a) {
                return None;
            }
            align = align.max(a);
        }
        let _ = union;
        size.is_multiple_of(align).then_some(align)
    }

    /// A member type's alignment under `pack`. An anonymous structure
    /// declared in place is laid out under the same pragma, so its members'
    /// alignments are checked too.
    fn field_align(&self, ty: &CType, pack: u64) -> Option<u64> {
        match element(ty) {
            CType::Named(e) if self.entities[*e].inline() => {
                let l = self.layouts.get(*e)?.as_ref()?;
                self.fits(&l.items, 0, l.union, l.size, pack)
            }
            _ => Some(self.align_of(ty).min(pack)),
        }
    }

    fn align_of(&self, ty: &CType) -> u64 {
        let mut ty = ty;
        for _ in 0..64 {
            match ty {
                CType::Void | CType::Function(_) => return 1,
                CType::Base(_, s) | CType::Sized(_, _, s) => return strict_align(*s),
                CType::Pointer(..) => return self.pointer_size,
                CType::Array(t, _) | CType::Qualified(_, t) => ty = t,
                CType::Named(e) => match self.entities[*e].kind {
                    Kind::Typedef => ty = &self.entities[*e].target,
                    Kind::Enum => return 4,
                    _ => return self.layouts.get(*e).and_then(|l| l.as_ref()).map_or(1, |l| l.align),
                },
            }
        }
        1
    }
}

/// The count of elements a subrange gives (`DW_AT_count`, or its bounds).
fn subrange_count(die: &Die) -> Option<u64> {
    if let Some(n) = die.attr_value(gimli::DW_AT_count).and_then(|v| v.udata_value()) {
        return Some(n);
    }
    let upper = die.attr_value(gimli::DW_AT_upper_bound)?;
    // An upper bound of -1: no elements (`int a[0]`).
    if matches!(upper, AttributeValue::Sdata(-1)) {
        return Some(0);
    }
    let lower = die
        .attr_value(gimli::DW_AT_lower_bound)
        .and_then(|v| v.udata_value())
        .unwrap_or(0);
    upper
        .udata_value()
        .and_then(|u| u.checked_sub(lower))
        .map(|n| n + 1)
        .filter(|&n| n < u64::from(u32::MAX))
}

/// Makes member names unique within a structure (anonymous members' members
/// included, which C puts in the same scope): real members first, then the
/// padding members, which give way.
fn rename(items: &mut [Item], taken: &mut HashSet<String>, pads: bool) {
    for item in items {
        match item {
            Item::Field(f) if !pads => f.name = unique(taken, &c_identifier(&f.name)),
            Item::Bits(b) if !pads => {
                for f in &mut b.fields {
                    if let Some(n) = &f.name {
                        f.name = Some(unique(taken, &c_identifier(n)));
                    }
                }
            }
            Item::Group(g) => rename(&mut g.items, taken, pads),
            Item::Pad { name, .. } if pads => *name = unique(taken, name),
            _ => {}
        }
    }
}

/// A structure, class or union, found by name.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructType {
    /// Its name in the debug info (`edict_s`, `geo::Rect`), or for an
    /// anonymous one the typedef's that names it.
    pub name: String,
    /// Its name in the header binviz writes (`geo__Rect`).
    pub c_name: String,
    /// "struct" or "union".
    pub kind: String,
    /// Its size in bytes.
    pub size: u64,
    /// Where it is described: its unit and DIE offset.
    pub unit: u32,
    pub die: u64,
}

/// The member at an offset in a structure.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldAt {
    /// How to reach it from the start of the object, as the header binviz
    /// writes declares it: `enemy`, `s.origin[1]`, `base_Shape.id`. Members of
    /// anonymous structures and unions are reached directly, as in C. When
    /// the offset is in padding, the aggregate that holds it (empty: the
    /// outermost).
    pub path: String,
    /// The member's type in C (`struct edict_s *`, `float`).
    pub type_name: String,
    /// Where the member starts in the object, and its size in bytes (for a
    /// bit field, its storage unit's; for padding, the gap's).
    pub offset: u64,
    pub size: u64,
    /// How far into the member the asked offset is (0 at its start).
    pub delta: u64,
    /// A bit field's first bit (from the start of the object) and width.
    pub bits: Option<(u64, u64)>,
    /// No member covers the offset.
    pub padding: bool,
}

impl FieldAt {
    /// The member as an annotation reads it, after the structure's name:
    /// `edict_t.enemy`, `edict_t.health+2`, `edict_t+0x21c`.
    pub fn label(&self, structure: &str) -> String {
        if self.padding || self.path.is_empty() {
            return format!("{structure}+{:#x}", self.offset + self.delta);
        }
        let mut out = format!("{structure}.{}", self.path);
        if self.delta > 0 {
            out.push_str(&format!("+{:#x}", self.delta));
        }
        out
    }
}

impl Types {
    /// The structure or union named `name` (see [`DebugInfo::find_struct`]).
    fn find(&self, name: &str) -> Option<usize> {
        let mut wanted = name.trim();
        loop {
            let before = wanted;
            for word in ["struct ", "class ", "union ", "const ", "volatile "] {
                wanted = wanted.strip_prefix(word).unwrap_or(wanted).trim_start();
            }
            wanted = wanted.trim_end_matches(['*', '&', ' ']);
            wanted = wanted.strip_suffix(" const").unwrap_or(wanted).trim_end();
            if wanted == before {
                break;
            }
        }
        let structure = |e: usize| -> Option<usize> {
            let ent = &self.entities[e];
            let ty = match ent.kind {
                Kind::Typedef => self.resolve(&ent.target),
                Kind::Struct | Kind::Union => return (ent.complete && self.layout(e).is_some()).then_some(e),
                _ => return None,
            };
            match ty {
                CType::Named(s) if self.layout(*s).is_some() => Some(*s),
                _ => None,
            }
        };
        if let Some(list) = self.by_name.get(wanted)
            && let Some(e) = list.iter().find_map(|&e| structure(e))
        {
            return Some(e);
        }
        // A C++ name without its namespaces (`Rect` for `geo::Rect`), when only one has it.
        let suffix = format!("::{wanted}");
        let mut matches = self
            .entities
            .iter()
            .enumerate()
            .filter(|(_, ent)| ent.source.ends_with(&suffix))
            .filter_map(|(e, _)| structure(e));
        let first = matches.next()?;
        matches.all(|e| e == first).then_some(first)
    }

    fn struct_type(&self, e: usize) -> StructType {
        let ent = &self.entities[e];
        let name = if ent.source.is_empty() {
            ent.name.clone()
        } else {
            ent.source.clone()
        };
        StructType {
            name,
            c_name: ent.name.clone(),
            kind: ent.kind.keyword().to_string(),
            size: ent.size.unwrap_or(0),
            unit: ent.unit,
            die: ent.die.0 as u64,
        }
    }

    /// The member at `offset` among `items` (of an object at `base`), under `path`.
    fn walk(&self, items: &[Item], offset: u64, base: u64, path: &str, depth: u32) -> Option<FieldAt> {
        let mut first = None;
        for item in items {
            let start = base + item.offset();
            if offset < start || offset >= start + item.size() {
                continue;
            }
            let found = match item {
                Item::Pad { size, .. } => FieldAt {
                    path: path.to_string(),
                    type_name: String::new(),
                    offset: start,
                    size: *size,
                    delta: offset - start,
                    bits: None,
                    padding: true,
                },
                Item::Field(f) => self.descend(&f.ty, join(path, &f.name), start, f.size, offset, depth),
                Item::Bits(b) => {
                    let byte = (offset - start) * 8;
                    let covering = b
                        .fields
                        .iter()
                        .find(|f| f.name.is_some() && f.first < byte + 8 && f.first + f.width > byte);
                    match covering {
                        Some(f) => FieldAt {
                            path: join(path, f.name.as_deref().unwrap_or("")),
                            type_name: self.type_name(&f.ty),
                            offset: start,
                            size: b.size,
                            delta: offset - start,
                            bits: Some((start * 8 + f.first, f.width)),
                            padding: false,
                        },
                        None => FieldAt {
                            path: path.to_string(),
                            type_name: String::new(),
                            offset: start,
                            size: b.size,
                            delta: offset - start,
                            bits: None,
                            padding: true,
                        },
                    }
                }
                Item::Group(g) => self.walk(&g.items, offset, base, path, depth)?,
            };
            // Of overlapping members (a union's), one that starts right there reads best.
            if found.delta == 0 && !found.padding {
                return Some(found);
            }
            if first.is_none() {
                first = Some(found);
            }
        }
        first
    }

    /// Into a member of type `ty` at `start`: its members or elements, down
    /// to the one at `offset`.
    fn descend(&self, ty: &CType, path: String, start: u64, size: u64, offset: u64, depth: u32) -> FieldAt {
        let leaf = |path: String| FieldAt {
            path,
            type_name: self.type_name(ty),
            offset: start,
            size,
            delta: offset - start,
            bits: None,
            padding: false,
        };
        if depth > 32 {
            return leaf(path);
        }
        match self.resolve(ty) {
            CType::Named(e) if matches!(self.entities[*e].kind, Kind::Struct | Kind::Union) => match self.layout(*e) {
                Some(l) => self
                    .walk(&l.items, offset, start, &path, depth + 1)
                    .unwrap_or_else(|| leaf(path)),
                None => leaf(path),
            },
            CType::Array(elem, dims) => {
                let each = self.size_of(elem);
                if each == 0 {
                    return leaf(path);
                }
                let index = (offset - start) / each;
                // Row-major: the last dimension varies fastest.
                let mut rest = index;
                let mut indexes = vec![0; dims.len()];
                for i in (0..dims.len()).rev() {
                    let n = dims[i].unwrap_or(0).max(1);
                    if i == 0 {
                        indexes[0] = rest;
                    } else {
                        indexes[i] = rest % n;
                        rest /= n;
                    }
                }
                let mut path = path;
                for i in indexes {
                    path.push_str(&format!("[{i}]"));
                }
                self.descend(elem, path, start + index * each, each, offset, depth + 1)
            }
            _ => leaf(path),
        }
    }
}

fn join(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}.{name}")
    }
}

impl DebugInfo {
    /// A structure, class or union by name: its name in the source (`edict_s`,
    /// or qualified, `geo::Rect`), its name in the header binviz writes
    /// (`geo__Rect`), or a typedef's that names it (`edict_t`; typedefs of
    /// typedefs are followed). A type spelled in full works too: `struct
    /// edict_s *` and `const edict_t *` find `edict_s`. A C++ name without its
    /// namespaces finds the one type that has it. Only structures that are
    /// defined (not just declared) are found.
    pub fn find_struct(&self, name: &str) -> Option<StructType> {
        let types = self.c_types();
        types.find(name).map(|e| types.struct_type(e))
    }

    /// The member of a structure at a byte offset: the path through nested
    /// structures, unions and arrays down to the innermost member there
    /// (`s.origin[1]`), with its type, where it starts and how far into it
    /// the offset is. Where members overlap (a union), the one that starts at
    /// the offset is preferred, else the first. `None` if the offset is past
    /// the structure's end.
    pub fn field_at(&self, st: &StructType, offset: u64) -> Option<FieldAt> {
        let types = self.c_types();
        let e = types.entity_at(st.unit, UnitOffset(st.die as usize))?;
        let layout = types.layout(e)?;
        if offset >= layout.size {
            return None;
        }
        Some(types.walk(&layout.items, offset, 0, "", 0).unwrap_or(FieldAt {
            path: String::new(),
            type_name: String::new(),
            offset: 0,
            size: layout.size,
            delta: offset,
            bits: None,
            padding: true,
        }))
    }

    /// [`Self::find_struct`] then [`Self::field_at`]: `struct_field("edict_t",
    /// 0x21c)` is the member `[esi+0x21c]` reads when `esi` points at an
    /// `edict_t`.
    pub fn struct_field(&self, name: &str, offset: u64) -> Option<FieldAt> {
        self.field_at(&self.find_struct(name)?, offset)
    }
}

#[cfg(test)]
mod tests {
    use gimli::write::{Address, AttributeValue as V, Dwarf, EndianVec, LineProgram, Sections, Unit, UnitEntryId};
    use gimli::{Encoding, Format, RunTimeEndian};

    use super::{c_identifier, own_name};
    use crate::dwarf::DebugInfo;

    /// A unit of made-up DWARF: the types a test describes.
    struct Made {
        unit: Unit,
        int: UnitEntryId,
        char: UnitEntryId,
        short: UnitEntryId,
    }

    impl Made {
        fn new(lang: gimli::DwLang) -> Made {
            let encoding = Encoding {
                format: Format::Dwarf32,
                version: 4,
                address_size: 4,
            };
            let mut unit = Unit::new(encoding, LineProgram::none());
            let root = unit.root();
            unit.get_mut(root).set(gimli::DW_AT_language, V::Language(lang));
            let mut made = Made {
                unit,
                int: root,
                char: root,
                short: root,
            };
            made.int = made.base("int", gimli::DW_ATE_signed, 4);
            made.char = made.base("char", gimli::DW_ATE_signed_char, 1);
            made.short = made.base("short", gimli::DW_ATE_signed, 2);
            made
        }

        fn base(&mut self, name: &str, encoding: gimli::DwAte, size: u64) -> UnitEntryId {
            let id = self.add(None, gimli::DW_TAG_base_type, Some(name));
            let e = self.unit.get_mut(id);
            e.set(gimli::DW_AT_encoding, V::Encoding(encoding));
            e.set(gimli::DW_AT_byte_size, V::Udata(size));
            id
        }

        fn add(&mut self, parent: Option<UnitEntryId>, tag: gimli::DwTag, name: Option<&str>) -> UnitEntryId {
            let parent = parent.unwrap_or(self.unit.root());
            let id = self.unit.add(parent, tag);
            if let Some(n) = name {
                self.unit
                    .get_mut(id)
                    .set(gimli::DW_AT_name, V::String(n.as_bytes().to_vec()));
            }
            id
        }

        fn aggregate(&mut self, tag: gimli::DwTag, name: Option<&str>, size: u64) -> UnitEntryId {
            let id = self.add(None, tag, name);
            self.unit.get_mut(id).set(gimli::DW_AT_byte_size, V::Udata(size));
            id
        }

        fn member(&mut self, parent: UnitEntryId, name: Option<&str>, ty: UnitEntryId, at: u64) -> UnitEntryId {
            let id = self.add(Some(parent), gimli::DW_TAG_member, name);
            let e = self.unit.get_mut(id);
            e.set(gimli::DW_AT_type, V::UnitRef(ty));
            e.set(gimli::DW_AT_data_member_location, V::Udata(at));
            id
        }

        fn inherit(&mut self, derived: UnitEntryId, base: UnitEntryId, at: u64) {
            let id = self.add(Some(derived), gimli::DW_TAG_inheritance, None);
            let e = self.unit.get_mut(id);
            e.set(gimli::DW_AT_type, V::UnitRef(base));
            e.set(gimli::DW_AT_data_member_location, V::Udata(at));
        }
    }

    fn load(units: Vec<Unit>) -> DebugInfo {
        let mut dwarf = Dwarf::new();
        for u in units {
            dwarf.units.add(u);
        }
        let mut sections = Sections::new(EndianVec::new(RunTimeEndian::Little));
        dwarf.write(&mut sections).unwrap();
        let mut out = Vec::new();
        sections
            .for_each(|id, data| {
                out.push((id, data.slice().to_vec()));
                Ok::<(), gimli::write::Error>(())
            })
            .unwrap();
        DebugInfo::from_sections(out, RunTimeEndian::Little, "test", &[], object::Architecture::I386).unwrap()
    }

    fn assert_has(text: &str, lines: &[&str]) {
        for line in lines {
            assert!(text.contains(line), "{line:?} missing in:\n{text}");
        }
    }

    #[test]
    fn names_become_c_identifiers() {
        assert_eq!(c_identifier("geo::Rect"), "geo__Rect");
        assert_eq!(
            c_identifier("std::vector<int, std::allocator<int> >"),
            "std__vector_int_std__allocator_int"
        );
        assert_eq!(c_identifier("*const str"), "p_const_str");
        assert_eq!(c_identifier("geo::Shape* const*"), "geo__Shape_p_const_p");
        assert_eq!(c_identifier("{closure_env#0}"), "closure_env_0");
        assert_eq!(c_identifier("__0"), "__0");
        assert_eq!(c_identifier("2d"), "_2d");
        assert_eq!(c_identifier("default"), "default_");
        assert_eq!(
            own_name("std::_Vector_base<int, std::allocator<int> >::_Vector_impl"),
            "_Vector_impl"
        );
        assert_eq!(own_name("std::vector<a::b>"), "vector");
    }

    #[test]
    fn a_base_whose_tail_padding_is_reused_is_taken_in() {
        // The Itanium ABI puts a derived class's member in its base's tail
        // padding when the base isn't a POD: that base can't be embedded whole.
        let mut m = Made::new(gimli::DW_LANG_C_plus_plus);
        let (int, char) = (m.int, m.char);
        let base = m.aggregate(gimli::DW_TAG_class_type, Some("Base"), 8);
        m.member(base, Some("x"), int, 0);
        m.member(base, Some("c"), char, 4);
        let derived = m.aggregate(gimli::DW_TAG_class_type, Some("Derived"), 8);
        m.inherit(derived, base, 0);
        m.member(derived, Some("d"), char, 5);
        // Where nothing reaches into it, the base is embedded whole.
        let holder = m.aggregate(gimli::DW_TAG_structure_type, Some("Holder"), 12);
        m.inherit(holder, base, 0);
        m.member(holder, Some("x"), int, 8);
        let d = load(vec![m.unit]);
        let h = d.c_header(&["Derived", "Holder"]).text;
        assert_has(
            &h,
            &[
                "/* 0x0: base class Base: its members, as the members after it use its tail padding */",
                "    /* 0x0 */ int x;\n    /* 0x4 */ char c;\n    /* 0x5 */ char d;\n    /* 0x6 */ char _pad_6[2];",
                "/* 0x0 */ struct Base base_Base; /* base class Base */",
                "/* 0x8 */ int x;",
            ],
        );
        assert_eq!(d.struct_field("Derived", 5).unwrap().path, "d");
        assert_eq!(d.struct_field("Holder", 4).unwrap().path, "base_Base.c");
    }

    #[test]
    fn members_that_overlap_are_a_union_and_dwarf_2_bit_fields_are_placed() {
        let mut m = Made::new(gimli::DW_LANG_C99);
        let (int, short) = (m.int, m.short);
        let s = m.aggregate(gimli::DW_TAG_structure_type, Some("S"), 12);
        m.member(s, Some("a"), int, 0);
        m.member(s, Some("lo"), short, 4);
        m.member(s, Some("hi"), short, 6);
        m.member(s, Some("whole"), int, 4);
        // DWARF 2 counts a bit field's bits from its storage unit's most significant end.
        let unsigned = m.base("unsigned int", gimli::DW_ATE_unsigned, 4);
        for (name, width, from_top) in [("f", 3, 29), ("g", 5, 24)] {
            let f = m.member(s, Some(name), unsigned, 8);
            let e = m.unit.get_mut(f);
            e.set(gimli::DW_AT_byte_size, V::Udata(4));
            e.set(gimli::DW_AT_bit_size, V::Udata(width));
            e.set(gimli::DW_AT_bit_offset, V::Udata(from_top));
        }
        let d = load(vec![m.unit]);
        let h = d.c_header(&["S"]).text;
        assert_has(
            &h,
            &[
                "/* 0x4 */ union {\n        /* 0x4 */ struct {\n            /* 0x4 */ short lo;\n            /* 0x6 */ short hi;\n        };\n        /* 0x4 */ int whole;\n    };",
                "/* 0x8 */ unsigned int f : 3;\n              unsigned int g : 5;\n              unsigned int : 24;",
                "_Static_assert(BINVIZ_OFFSETOF(struct S, hi) == 0x6,",
            ],
        );
        assert_eq!(d.struct_field("S", 6).unwrap().path, "hi");
        let f = d.struct_field("S", 8).unwrap();
        assert_eq!((f.path.as_str(), f.bits), ("f", Some((64, 3))));
    }

    #[test]
    fn copies_are_one_and_clashing_names_are_numbered() {
        // One struct in two units is one type; two different ones of one name are two.
        let mut units = Vec::new();
        for fields in [&["next"][..], &["next"], &["prev", "next"]] {
            let mut m = Made::new(gimli::DW_LANG_C99);
            let int = m.int;
            let s = m.aggregate(gimli::DW_TAG_structure_type, Some("node"), 4 * fields.len() as u64);
            for (i, f) in fields.iter().enumerate() {
                m.member(s, Some(f), int, 4 * i as u64);
            }
            units.push(m.unit);
        }
        // An `enum class`'s enumerators take its name, as does one whose name is taken.
        let mut m = Made::new(gimli::DW_LANG_C_plus_plus);
        for (name, scoped) in [("Color", true), ("Light", false), ("Mood", false)] {
            let int = m.int;
            let e = m.aggregate(gimli::DW_TAG_enumeration_type, Some(name), 4);
            m.unit.get_mut(e).set(gimli::DW_AT_type, V::UnitRef(int));
            if scoped {
                m.unit.get_mut(e).set(gimli::DW_AT_enum_class, V::Flag(true));
            }
            let red = m.add(Some(e), gimli::DW_TAG_enumerator, Some("Red"));
            m.unit.get_mut(red).set(gimli::DW_AT_const_value, V::Sdata(-2));
        }
        units.push(m.unit);
        let d = load(units);
        let h = d.c_header(&[]).text;
        assert_has(
            &h,
            &[
                "struct node { /* 0x4 bytes */",
                "struct node_2 { /* node, 0x8 bytes */",
                "Color__Red = -2,",
                "\n    Red = -2,",
                "Mood__Red = -2,",
            ],
        );
        assert_eq!(h.matches("struct node {").count(), 1, "{h}");
    }

    #[test]
    fn members_past_the_end_and_parameters_of_no_type() {
        // rustc puts a struct's unsized tail, and a function item, at or past its end.
        let mut m = Made::new(gimli::DW_LANG_Rust);
        let int = m.int;
        let pointer = m.add(None, gimli::DW_TAG_pointer_type, None);
        m.unit.get_mut(pointer).set(gimli::DW_AT_type, V::UnitRef(int));
        m.unit.get_mut(pointer).set(gimli::DW_AT_byte_size, V::Udata(4));
        let s = m.aggregate(gimli::DW_TAG_structure_type, Some("Tail"), 4);
        m.member(s, Some("len"), int, 0);
        m.member(s, Some("at_end"), pointer, 4);
        m.member(s, Some("beyond"), int, 8);
        // `()` takes no argument, and C has no parameter of type void.
        let unit_type = m.base("()", gimli::DW_ATE_unsigned, 0);
        let f = m.add(None, gimli::DW_TAG_subprogram, Some("takes_unit"));
        m.unit.get_mut(f).set(gimli::DW_AT_external, V::Flag(true));
        m.unit
            .get_mut(f)
            .set(gimli::DW_AT_low_pc, V::Address(Address::Constant(0x1000)));
        for (name, ty) in [("u", unit_type), ("n", int)] {
            let p = m.add(Some(f), gimli::DW_TAG_formal_parameter, Some(name));
            m.unit.get_mut(p).set(gimli::DW_AT_type, V::UnitRef(ty));
        }
        let d = load(vec![m.unit]);
        let h = d.c_header(&[]).text;
        assert_has(
            &h,
            &[
                "/* 0x4 */ int *at_end[0];",
                "/* 0x8: beyond: past the end of the structure */",
                "_Static_assert(sizeof(struct Tail) == 0x4,",
                "void takes_unit(int n);",
            ],
        );
        assert_eq!(d.struct_field("Tail", 3).unwrap().path, "len");
        assert!(d.struct_field("Tail", 4).is_none());
    }

    #[test]
    fn a_function_named_by_its_abstract_instance_after_it() {
        // clang writes an inlined function's out-of-line copy before its
        // abstract instance, and may leave out a parameter it doesn't use.
        let mut m = Made::new(gimli::DW_LANG_C99);
        let int = m.int;
        let copy = m.add(None, gimli::DW_TAG_subprogram, None);
        let abstract_instance = m.add(None, gimli::DW_TAG_subprogram, Some("pick"));
        let e = m.unit.get_mut(abstract_instance);
        e.set(gimli::DW_AT_external, V::Flag(true));
        e.set(gimli::DW_AT_prototyped, V::Flag(true));
        e.set(gimli::DW_AT_type, V::UnitRef(int));
        let mut params = Vec::new();
        for name in ["unused", "which"] {
            let p = m.add(Some(abstract_instance), gimli::DW_TAG_formal_parameter, Some(name));
            m.unit.get_mut(p).set(gimli::DW_AT_type, V::UnitRef(int));
            params.push(p);
        }
        let e = m.unit.get_mut(copy);
        e.set(gimli::DW_AT_abstract_origin, V::UnitRef(abstract_instance));
        e.set(gimli::DW_AT_low_pc, V::Address(Address::Constant(0x1000)));
        let p = m.add(Some(copy), gimli::DW_TAG_formal_parameter, None);
        m.unit
            .get_mut(p)
            .set(gimli::DW_AT_abstract_origin, V::UnitRef(params[1]));
        let d = load(vec![m.unit]);
        let h = d.c_header(&["pick"]);
        assert_eq!(h.functions, 1, "{}", h.text);
        assert_has(&h.text, &["int pick(int unused, int which);"]);
    }
}
