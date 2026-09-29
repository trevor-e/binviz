//! The WebAssembly binary format, read the way an engine reads it: the
//! sections in order, and what each declares (types, imports, functions,
//! tables, memories, globals, exports, the start function, element and data
//! segments, function bodies), with the file offset of everything binviz
//! points at: each entry, each body and its first instruction, each slot of
//! an element segment. Custom sections are read too when binviz knows them:
//! `name`, `producers`, `target_features`, `sourceMappingURL`,
//! `external_debug_info`, `build_id`, and in objects `linking`.
//!
//! Nothing here trusts a size or a count: every read is bounds-checked, a
//! section that can't be read to its end is noted in [`Module::problems`],
//! and the sections after it are still read.

use std::collections::HashMap;
use std::ops::Range;

use super::code;
use crate::error::{Result, bail};
use crate::util;

pub(crate) const CUSTOM: u8 = 0;
pub(crate) const TYPE: u8 = 1;
pub(crate) const IMPORT: u8 = 2;
pub(crate) const FUNCTION: u8 = 3;
pub(crate) const TABLE: u8 = 4;
pub(crate) const MEMORY: u8 = 5;
pub(crate) const GLOBAL: u8 = 6;
pub(crate) const EXPORT: u8 = 7;
pub(crate) const START: u8 = 8;
pub(crate) const ELEMENT: u8 = 9;
pub(crate) const CODE: u8 = 10;
pub(crate) const DATA: u8 = 11;
pub(crate) const DATA_COUNT: u8 = 12;
pub(crate) const TAG: u8 = 13;

/// A standard section's name, as the text format and the tools write it.
pub(crate) fn section_name(id: u8) -> &'static str {
    match id {
        CUSTOM => "custom",
        TYPE => "type",
        IMPORT => "import",
        FUNCTION => "function",
        TABLE => "table",
        MEMORY => "memory",
        GLOBAL => "global",
        EXPORT => "export",
        START => "start",
        ELEMENT => "element",
        CODE => "code",
        DATA => "data",
        DATA_COUNT => "datacount",
        TAG => "tag",
        _ => "unknown",
    }
}

/// Whether `data` is a WebAssembly module: the magic number `\0asm` and
/// version 1 (a component, the component model's container, is version
/// `0x1000d` and isn't read).
pub(crate) fn is_module(data: &[u8]) -> bool {
    data.len() >= 8 && data[..4] == *b"\0asm" && data[4..8] == [1, 0, 0, 0]
}

/// An unsigned LEB128 number of at most `bits` bits at the start of `bytes`:
/// its value and length. Longer encodings, padded with `0x80` bytes the way
/// linkers leave room for relocations, are read too, up to the most bytes
/// `bits` allows.
pub(crate) fn uleb(bytes: &[u8], bits: u32) -> Option<(u64, usize)> {
    let max = bits.div_ceil(7) as usize;
    let mut value = 0u64;
    for (i, &b) in bytes.iter().take(max).enumerate() {
        if i < 10 {
            value |= u64::from(b & 0x7f) << (7 * i as u32).min(63);
        }
        if b & 0x80 == 0 {
            let value = if bits < 64 { value & ((1 << bits) - 1) } else { value };
            return Some((value, i + 1));
        }
    }
    None
}

/// A signed LEB128 number of at most `bits` bits: its value and length.
pub(crate) fn sleb(bytes: &[u8], bits: u32) -> Option<(i64, usize)> {
    let max = bits.div_ceil(7) as usize;
    let mut value = 0i64;
    let mut shift = 0u32;
    for (i, &b) in bytes.iter().take(max).enumerate() {
        if shift < 64 {
            value |= i64::from(b & 0x7f) << shift;
        }
        shift += 7;
        if b & 0x80 == 0 {
            if shift < 64 && b & 0x40 != 0 {
                value |= -1i64 << shift;
            }
            return Some((value, i + 1));
        }
    }
    None
}

/// A cursor over a module's bytes, reading the binary format's encodings:
/// LEB128 numbers, names, fixed-size values. Reads stop at `end`, so a
/// section's reader never reads into the next one.
#[derive(Clone, Copy)]
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], pos: u64, end: u64) -> Reader<'a> {
        let end = usize::try_from(end).unwrap_or(usize::MAX).min(data.len());
        let pos = usize::try_from(pos).unwrap_or(usize::MAX).min(end);
        Reader { data, pos, end }
    }

    /// Where the next read starts, as a file offset.
    pub fn pos(&self) -> u64 {
        self.pos as u64
    }

    pub fn end(&self) -> u64 {
        self.end as u64
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.end
    }

    /// The bytes left to read.
    pub fn rest(&self) -> &'a [u8] {
        &self.data[self.pos..self.end]
    }

    pub fn u8(&mut self) -> Option<u8> {
        let b = *self.rest().first()?;
        self.pos += 1;
        Some(b)
    }

    pub fn peek(&self) -> Option<u8> {
        self.rest().first().copied()
    }

    pub fn bytes(&mut self, n: u64) -> Option<&'a [u8]> {
        let n = usize::try_from(n).ok()?;
        let b = self.rest().get(..n)?;
        self.pos += n;
        Some(b)
    }

    pub fn skip(&mut self, n: u64) -> Option<()> {
        self.bytes(n).map(|_| ())
    }

    pub fn uleb(&mut self, bits: u32) -> Option<u64> {
        let (v, n) = uleb(self.rest(), bits)?;
        self.pos += n;
        Some(v)
    }

    pub fn sleb(&mut self, bits: u32) -> Option<i64> {
        let (v, n) = sleb(self.rest(), bits)?;
        self.pos += n;
        Some(v)
    }

    pub fn u32(&mut self) -> Option<u32> {
        self.uleb(32).map(|v| v as u32)
    }

    pub fn u64(&mut self) -> Option<u64> {
        self.uleb(64)
    }

    /// A vector's length. Each element takes at least a byte, so a count
    /// larger than what is left is corrupt rather than a reason to allocate.
    pub fn count(&mut self) -> Option<u32> {
        let n = self.u32()?;
        (n as usize <= self.end - self.pos).then_some(n)
    }

    /// A name: its length, then its bytes (UTF-8).
    pub fn name(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()?;
        self.bytes(u64::from(n))
    }

    pub fn string(&mut self) -> Option<String> {
        self.name().map(util::lossy)
    }

    pub fn fixed<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.bytes(N as u64)?.try_into().ok()
    }
}

/// A heap type: what a reference points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum HeapType {
    Func,
    Extern,
    Any,
    Eq,
    I31,
    Struct,
    Array,
    Exn,
    None,
    NoFunc,
    NoExtern,
    NoExn,
    /// A type the type section defines.
    Index(u32),
}

impl HeapType {
    /// The abstract heap type a single byte stands for.
    fn from_code(b: u8) -> Option<HeapType> {
        Some(match b {
            0x70 => HeapType::Func,
            0x6F => HeapType::Extern,
            0x6E => HeapType::Any,
            0x6D => HeapType::Eq,
            0x6C => HeapType::I31,
            0x6B => HeapType::Struct,
            0x6A => HeapType::Array,
            0x69 => HeapType::Exn,
            0x71 => HeapType::None,
            0x73 => HeapType::NoFunc,
            0x72 => HeapType::NoExtern,
            0x74 => HeapType::NoExn,
            _ => return None,
        })
    }

    /// A heap type as the binary format writes one: a signed 33-bit number,
    /// a type index when positive, else the code of an abstract type.
    pub fn read(r: &mut Reader) -> Option<HeapType> {
        let v = r.sleb(33)?;
        if v >= 0 {
            Some(HeapType::Index(v as u32))
        } else {
            HeapType::from_code((v & 0x7f) as u8)
        }
    }

    fn name(self) -> String {
        match self {
            HeapType::Func => "func".into(),
            HeapType::Extern => "extern".into(),
            HeapType::Any => "any".into(),
            HeapType::Eq => "eq".into(),
            HeapType::I31 => "i31".into(),
            HeapType::Struct => "struct".into(),
            HeapType::Array => "array".into(),
            HeapType::Exn => "exn".into(),
            HeapType::None => "none".into(),
            HeapType::NoFunc => "nofunc".into(),
            HeapType::NoExtern => "noextern".into(),
            HeapType::NoExn => "noexn".into(),
            HeapType::Index(i) => i.to_string(),
        }
    }
}

impl std::fmt::Display for HeapType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name())
    }
}

/// A value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ValType {
    I32,
    I64,
    F32,
    F64,
    V128,
    Ref { nullable: bool, heap: HeapType },
}

impl ValType {
    /// A value type starting with byte `b` (already read).
    pub fn read_after(b: u8, r: &mut Reader) -> Option<ValType> {
        Some(match b {
            0x7F => ValType::I32,
            0x7E => ValType::I64,
            0x7D => ValType::F32,
            0x7C => ValType::F64,
            0x7B => ValType::V128,
            0x64 | 0x63 => ValType::Ref {
                nullable: b == 0x63,
                heap: HeapType::read(r)?,
            },
            // The shorthands: `funcref` is `(ref null func)`.
            _ => ValType::Ref {
                nullable: true,
                heap: HeapType::from_code(b)?,
            },
        })
    }

    pub fn read(r: &mut Reader) -> Option<ValType> {
        let b = r.u8()?;
        ValType::read_after(b, r)
    }

    /// Whether `b` starts a value type (as opposed to a type index, in a block type).
    pub fn starts(b: u8) -> bool {
        matches!(b, 0x7B..=0x7F | 0x63 | 0x64) || HeapType::from_code(b).is_some()
    }
}

impl std::fmt::Display for ValType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValType::I32 => f.write_str("i32"),
            ValType::I64 => f.write_str("i64"),
            ValType::F32 => f.write_str("f32"),
            ValType::F64 => f.write_str("f64"),
            ValType::V128 => f.write_str("v128"),
            ValType::Ref { nullable: true, heap } if !matches!(heap, HeapType::Index(_)) => write!(f, "{heap}ref"),
            ValType::Ref { nullable: true, heap } => write!(f, "(ref null {heap})"),
            ValType::Ref { nullable: false, heap } => write!(f, "(ref {heap})"),
        }
    }
}

/// A function's signature.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub(crate) struct FuncType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

impl std::fmt::Display for FuncType {
    /// `(i32, i32) -> i32`; `()` for none.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = |v: &[ValType]| v.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
        match self.results.as_slice() {
            [one] => write!(f, "({}) -> {one}", list(&self.params)),
            many => write!(f, "({}) -> ({})", list(&self.params), list(many)),
        }
    }
}

/// The size of a memory or a table: `min` units, at most `max`. A memory
/// counts 64 KiB pages unless it gives another page size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Limits {
    pub min: u64,
    pub max: Option<u64>,
    /// Shared between threads.
    pub shared: bool,
    /// Addressed with 64-bit numbers (memory64, table64).
    pub is64: bool,
    /// log2 of the page size, when not 64 KiB (the custom page sizes proposal).
    pub page_log2: Option<u32>,
}

impl Limits {
    pub fn read(r: &mut Reader) -> Option<Limits> {
        let flags = r.u8()?;
        let is64 = flags & 4 != 0;
        let mut number = || if is64 { r.u64() } else { r.u32().map(u64::from) };
        let min = number()?;
        let max = if flags & 1 != 0 { Some(number()?) } else { None };
        let page_log2 = if flags & 8 != 0 { Some(r.u32()?) } else { None };
        Some(Limits {
            min,
            max,
            shared: flags & 2 != 0,
            is64,
            page_log2,
        })
    }

    /// Bytes per page, for a memory.
    pub fn page_size(&self) -> u64 {
        1u64 << self.page_log2.unwrap_or(16).min(32)
    }
}

/// A constant expression's value, when it is a single instruction; `Expr`
/// for anything longer (an extended constant expression).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Const {
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
    V128,
    GlobalGet(u32),
    RefFunc(u32),
    RefNull,
    Expr,
}

impl Const {
    /// Reads a constant expression up to its `end`.
    pub fn read(r: &mut Reader) -> Option<Const> {
        let mut value = None;
        let mut count = 0;
        let mut depth = 0u32;
        loop {
            let insn = code::decode(r)?;
            if !insn.known {
                return None;
            }
            match insn.op {
                code::END if depth == 0 => break,
                code::END => depth -= 1,
                code::BLOCK | code::LOOP | code::IF => depth += 1,
                _ => {}
            }
            count += 1;
            value = Some(match (insn.op, &insn.imm) {
                (code::I32_CONST, code::Imm::I32(v)) => Const::I32(*v),
                (code::I64_CONST, code::Imm::I64(v)) => Const::I64(*v),
                (code::F32_CONST, code::Imm::F32(v)) => Const::F32(*v),
                (code::F64_CONST, code::Imm::F64(v)) => Const::F64(*v),
                (code::V128_CONST, _) => Const::V128,
                (code::GLOBAL_GET, code::Imm::Global(g)) => Const::GlobalGet(*g),
                (code::REF_FUNC, code::Imm::Func(f)) => Const::RefFunc(*f),
                (code::REF_NULL, _) => Const::RefNull,
                _ => Const::Expr,
            });
        }
        Some(match (count, value) {
            (1, Some(v)) => v,
            _ => Const::Expr,
        })
    }

    /// The value as an unsigned number, for an address or an offset.
    pub fn address(&self) -> Option<u64> {
        match *self {
            Const::I32(v) => Some(u64::from(v as u32)),
            Const::I64(v) => Some(v as u64),
            _ => None,
        }
    }
}

impl std::fmt::Display for Const {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Const::I32(v) => write!(f, "i32.const {v}"),
            Const::I64(v) => write!(f, "i64.const {v}"),
            Const::F32(v) => write!(f, "f32.const {:?}", f32::from_bits(*v)),
            Const::F64(v) => write!(f, "f64.const {:?}", f64::from_bits(*v)),
            Const::V128 => f.write_str("v128.const …"),
            Const::GlobalGet(g) => write!(f, "global.get {g}"),
            Const::RefFunc(i) => write!(f, "ref.func {i}"),
            Const::RefNull => f.write_str("ref.null"),
            Const::Expr => f.write_str("(an expression)"),
        }
    }
}

/// What an import or an export is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Func,
    Table,
    Memory,
    Global,
    Tag,
}

impl Kind {
    pub fn from_code(b: u8) -> Option<Kind> {
        Some(match b {
            0 => Kind::Func,
            1 => Kind::Table,
            2 => Kind::Memory,
            3 => Kind::Global,
            4 => Kind::Tag,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Func => "function",
            Kind::Table => "table",
            Kind::Memory => "memory",
            Kind::Global => "global",
            Kind::Tag => "tag",
        }
    }
}

/// What an import brings in.
#[derive(Clone, Debug)]
pub(crate) enum ImportDesc {
    Func(u32),
    Table(ValType, Limits),
    Memory(Limits),
    Global(ValType, bool),
    Tag(u32),
}

impl ImportDesc {
    pub fn kind(&self) -> Kind {
        match self {
            ImportDesc::Func(_) => Kind::Func,
            ImportDesc::Table(..) => Kind::Table,
            ImportDesc::Memory(_) => Kind::Memory,
            ImportDesc::Global(..) => Kind::Global,
            ImportDesc::Tag(_) => Kind::Tag,
        }
    }

    fn read(r: &mut Reader) -> Option<ImportDesc> {
        Some(match r.u8()? {
            0 => ImportDesc::Func(r.u32()?),
            1 => ImportDesc::Table(ValType::read(r)?, Limits::read(r)?),
            2 => ImportDesc::Memory(Limits::read(r)?),
            3 => ImportDesc::Global(ValType::read(r)?, r.u8()? & 1 != 0),
            4 => {
                r.u8()?;
                ImportDesc::Tag(r.u32()?)
            }
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Import {
    pub module: String,
    pub field: String,
    pub desc: ImportDesc,
    /// The entry in the import section.
    pub entry: Range<u64>,
}

/// A function body: where its entry in the code section, its locals and
/// its code start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Body {
    /// The body's size, before it.
    pub entry: u64,
    /// The locals declarations: where the function starts, as engines and
    /// DWARF count.
    pub start: u64,
    /// The first instruction.
    pub code: u64,
    pub end: u64,
}

/// A function of the function index space: the imported ones first.
#[derive(Clone, Debug)]
pub(crate) struct Func {
    pub type_index: u32,
    pub import: Option<u32>,
    pub body: Option<Body>,
}

#[derive(Clone, Debug)]
pub(crate) struct Table {
    pub ty: ValType,
    pub limits: Limits,
    pub import: Option<u32>,
    /// Its entry in the table or import section.
    pub entry: Range<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct Memory {
    pub limits: Limits,
    pub import: Option<u32>,
    pub entry: Range<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct Global {
    pub ty: ValType,
    pub mutable: bool,
    /// Its initial value, for one the module defines.
    pub init: Option<Const>,
    pub import: Option<u32>,
    pub entry: Range<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct Tag {
    pub type_index: u32,
    pub import: Option<u32>,
    pub entry: Range<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct Export {
    pub name: String,
    pub kind: Kind,
    pub index: u32,
}

/// When an element or data segment is used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mode {
    /// Written into a table (or memory) when the module is instantiated, at
    /// the offset the expression gives.
    Active { index: u32, offset: Const },
    /// Written by `table.init` or `memory.init`, where the code says.
    Passive,
    /// Declares functions `ref.func` may name; written nowhere.
    Declarative,
}

/// An element segment: functions for a table (the one `call_indirect` calls through).
#[derive(Clone, Debug)]
pub(crate) struct Element {
    pub mode: Mode,
    /// Each slot: the function it holds (`None` for a null reference or an
    /// expression binviz doesn't evaluate), and where the slot is in the file.
    pub items: Vec<(Option<u32>, u64)>,
    pub entry: Range<u64>,
}

/// A data segment: bytes for linear memory.
#[derive(Clone, Debug)]
pub(crate) struct Segment {
    pub mode: Mode,
    pub bytes: Range<u64>,
    pub entry: Range<u64>,
}

impl Import {
    /// An entry of the import section.
    pub fn read(r: &mut Reader) -> Option<Import> {
        let start = r.pos();
        let module = r.string()?;
        let field = r.string()?;
        let desc = ImportDesc::read(r)?;
        Some(Import {
            module,
            field,
            desc,
            entry: start..r.pos(),
        })
    }
}

impl Body {
    /// An entry of the code section: the body's size, then the body.
    pub fn read(r: &mut Reader) -> Option<Body> {
        let entry = r.pos();
        let size = r.u32()?;
        let start = r.pos();
        let end = start + u64::from(size);
        if end > r.end() {
            return None;
        }
        let mut locals = Reader::new(r.data, start, end);
        skip_locals(&mut locals)?;
        r.skip(u64::from(size))?;
        Some(Body {
            entry,
            start,
            code: locals.pos(),
            end,
        })
    }
}

impl Table {
    /// An entry of the table section.
    pub fn read(r: &mut Reader) -> Option<Table> {
        let start = r.pos();
        // A table with an initializer (the function references proposal): 0x40 0x00, then as usual.
        let with_init = r.peek()? == 0x40;
        if with_init {
            r.skip(2)?;
        }
        let ty = ValType::read(r)?;
        let limits = Limits::read(r)?;
        if with_init {
            Const::read(r)?;
        }
        Some(Table {
            ty,
            limits,
            import: None,
            entry: start..r.pos(),
        })
    }
}

impl Global {
    /// An entry of the global section.
    pub fn read(r: &mut Reader) -> Option<Global> {
        let start = r.pos();
        let ty = ValType::read(r)?;
        let mutable = r.u8()? & 1 != 0;
        let init = Const::read(r)?;
        Some(Global {
            ty,
            mutable,
            init: Some(init),
            import: None,
            entry: start..r.pos(),
        })
    }
}

impl Export {
    /// An entry of the export section.
    pub fn read(r: &mut Reader) -> Option<Export> {
        let name = r.string()?;
        let kind = Kind::from_code(r.u8()?)?;
        let index = r.u32()?;
        Some(Export { name, kind, index })
    }
}

impl Element {
    /// An entry of the element section, in any of its eight forms.
    pub fn read(r: &mut Reader) -> Option<Element> {
        let start = r.pos();
        let flags = r.u32()?;
        if flags > 7 {
            return None;
        }
        // Bit 0: passive or declarative (else active); bit 1: an explicit
        // table index (active) or declarative (else passive); bit 2: the
        // slots are expressions rather than function indices.
        let mode = match flags & 3 {
            0 => Mode::Active {
                index: 0,
                offset: Const::read(r)?,
            },
            2 => {
                let index = r.u32()?;
                Mode::Active {
                    index,
                    offset: Const::read(r)?,
                }
            }
            1 => Mode::Passive,
            _ => Mode::Declarative,
        };
        let exprs = flags & 4 != 0;
        // The element kind (0x00, funcref) or reference type, except in the MVP's forms 0 and 4.
        if flags & 3 != 0 {
            if exprs {
                ValType::read(r)?;
            } else {
                r.u8()?;
            }
        }
        let n = r.count()?;
        let mut items = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let at = r.pos();
            let f = if exprs {
                match Const::read(r)? {
                    Const::RefFunc(f) => Some(f),
                    _ => None,
                }
            } else {
                Some(r.u32()?)
            };
            items.push((f, at));
        }
        Some(Element {
            mode,
            items,
            entry: start..r.pos(),
        })
    }
}

impl Segment {
    /// An entry of the data section.
    pub fn read(r: &mut Reader) -> Option<Segment> {
        let start = r.pos();
        let mode = match r.u32()? {
            0 => Mode::Active {
                index: 0,
                offset: Const::read(r)?,
            },
            1 => Mode::Passive,
            2 => {
                let index = r.u32()?;
                Mode::Active {
                    index,
                    offset: Const::read(r)?,
                }
            }
            _ => return None,
        };
        let size = r.u32()?;
        let at = r.pos();
        r.skip(u64::from(size))?;
        Some(Segment {
            mode,
            bytes: at..r.pos(),
            entry: start..r.pos(),
        })
    }
}

/// A section as the file lays it out.
#[derive(Clone, Debug)]
pub(crate) struct Sec {
    pub id: u8,
    /// A custom section's name, else the standard name (`code`).
    pub name: String,
    /// The id byte.
    pub start: u64,
    /// After the size.
    pub payload: u64,
    /// After a custom section's name; the payload for the others.
    pub content: u64,
    pub end: u64,
}

/// The `name` section's names, by index.
#[derive(Clone, Debug, Default)]
pub(crate) struct Names {
    pub module: Option<String>,
    pub functions: HashMap<u32, String>,
    /// Per function: (local index, name).
    pub locals: HashMap<u32, Vec<(u32, String)>>,
    pub types: HashMap<u32, String>,
    pub tables: HashMap<u32, String>,
    pub memories: HashMap<u32, String>,
    pub globals: HashMap<u32, String>,
    pub elements: HashMap<u32, String>,
    pub data: HashMap<u32, String>,
    pub tags: HashMap<u32, String>,
}

/// The name section's subsections, by id.
pub(crate) fn name_subsection(id: u8) -> &'static str {
    match id {
        0 => "module name",
        1 => "function names",
        2 => "local names",
        3 => "label names",
        4 => "type names",
        5 => "table names",
        6 => "memory names",
        7 => "global names",
        8 => "element segment names",
        9 => "data segment names",
        10 => "field names",
        11 => "tag names",
        _ => "unknown",
    }
}

/// A symbol of an object file's `linking` section.
#[derive(Clone, Debug)]
pub(crate) struct LinkSymbol {
    /// 0 function, 1 data, 2 global, 3 section, 4 tag, 5 table.
    pub kind: u8,
    /// The function, global, tag, table or section it names.
    pub index: u32,
    pub name: Option<String>,
    /// For defined data: (segment, offset in it, size).
    pub data: Option<(u32, u64, u64)>,
}

/// `WASM_SYM_UNDEFINED`: the symbol is imported.
pub(crate) const SYM_UNDEFINED: u32 = 0x10;
/// `WASM_SYM_EXPLICIT_NAME`: an import's symbol carries its own name.
pub(crate) const SYM_EXPLICIT_NAME: u32 = 0x40;

/// A module read: its sections and everything they declare.
#[derive(Clone, Debug, Default)]
pub(crate) struct Module {
    pub sections: Vec<Sec>,
    /// Function types by type index (`None` for struct and array types).
    pub types: Vec<Option<FuncType>>,
    pub imports: Vec<Import>,
    /// The function index space: imports first, then the defined functions.
    pub funcs: Vec<Func>,
    pub imported_funcs: u32,
    pub tables: Vec<Table>,
    pub memories: Vec<Memory>,
    pub globals: Vec<Global>,
    pub tags: Vec<Tag>,
    pub exports: Vec<Export>,
    pub start: Option<u32>,
    pub elements: Vec<Element>,
    pub data_count: Option<u32>,
    pub data: Vec<Segment>,
    /// Where the code that sets memory up copies passive data segments:
    /// segment → linear address (see [`Module::copied_to`]).
    pub copied: HashMap<u32, u64>,
    pub names: Names,
    /// An object file's symbols (its `linking` section).
    pub symbols: Vec<LinkSymbol>,
    /// An object file's data segment names (its `linking` section).
    pub segment_names: HashMap<u32, String>,
    /// The `producers` section: (field, [(name, version)]).
    pub producers: Vec<(String, Vec<(String, String)>)>,
    /// The `target_features` section: (`+`, `-` or `=`, feature).
    pub features: Vec<(char, String)>,
    pub source_map_url: Option<String>,
    pub external_debug_info: Option<String>,
    pub build_id: Option<Vec<u8>>,
    /// Sections or entries that could not be read, as sentences.
    pub problems: Vec<String>,
    /// What `call_indirect` can reach, built on first use (see [`Module::indirect_targets`]).
    pub indirect: std::sync::OnceLock<Indirect>,
}

/// Each type's first index with the same signature, and how many functions
/// of each signature each table holds: (table, type) → count.
pub(crate) type Indirect = (Vec<u32>, HashMap<(u32, u32), u32>);

/// What a module's first bytes say it is, for finding modules among a
/// folder's files without reading them whole (see [`head`]).
#[derive(Debug, Default)]
pub(crate) struct Head {
    /// A relocatable object: it has a `linking` section.
    pub object: bool,
    /// A side module (a shared library): it has a `dylink.0` section.
    pub side_module: bool,
    /// DWARF and nothing to run: `.debug_*` sections among custom sections
    /// only, as `llvm-objcopy --only-keep-debug` leaves a module.
    pub debug_only: bool,
    /// Its first memory, imported or its own, is 64-bit: a wasm64 module.
    pub is64: bool,
    /// Its `build_id` section. wasm-ld writes it last, so it is here only
    /// when the bytes read reach the end of the module.
    pub build_id: Option<Vec<u8>>,
}

/// Reads a module's [`Head`] from as much of it as `data` holds, section by
/// section, looking into only the sections that say what it is. `None` when
/// `data` doesn't start a module.
pub(crate) fn head(data: &[u8]) -> Option<Head> {
    if !is_module(data) {
        return None;
    }
    let mut head = Head::default();
    let (mut standard, mut debug, mut memory) = (false, false, None);
    let len = data.len() as u64;
    let mut pos = 8;
    while pos < len {
        let mut r = Reader::new(data, pos, len);
        let (Some(id), Some(size)) = (r.u8(), r.u32()) else {
            break;
        };
        let end = r.pos() + u64::from(size);
        let mut s = Reader::new(data, r.pos(), end);
        match id {
            CUSTOM => match s.name() {
                Some(b"linking") => head.object = true,
                Some(b"build_id") => head.build_id = s.name().map(<[u8]>::to_vec),
                Some(n) if n.starts_with(b"dylink") => head.side_module = true,
                Some(n) if n.starts_with(b".debug") => debug = true,
                _ => {}
            },
            // Imported memories come first in the memory index space.
            IMPORT if memory.is_none() => memory = imported_memory(&mut s),
            MEMORY if memory.is_none() => {
                memory = s
                    .count()
                    .filter(|&n| n > 0)
                    .and_then(|_| Limits::read(&mut s))
                    .map(|l| l.is64);
            }
            _ => {}
        }
        standard |= id != CUSTOM;
        pos = end;
    }
    head.debug_only = debug && !standard;
    head.is64 = memory == Some(true);
    Some(head)
}

/// Whether the first memory an import section imports is 64-bit; `None`
/// when it imports none.
fn imported_memory(r: &mut Reader) -> Option<bool> {
    for _ in 0..r.count()? {
        r.name()?;
        r.name()?;
        if let ImportDesc::Memory(limits) = ImportDesc::read(r)? {
            return Some(limits.is64);
        }
    }
    None
}

impl Module {
    /// Reads a module. Fails only when `data` isn't one.
    pub fn parse(data: &[u8]) -> Result<Module> {
        if !is_module(data) {
            bail!("not a WebAssembly module (it needs the magic number \\0asm and version 1)");
        }
        let mut m = Module::default();
        let len = data.len() as u64;
        let mut pos = 8;
        while pos < len {
            let mut r = Reader::new(data, pos, len);
            let (Some(id), Some(size)) = (r.u8(), r.u32()) else {
                m.problems.push(format!("the section header at {pos:#x} is cut short"));
                break;
            };
            let payload = r.pos();
            let mut end = payload + u64::from(size);
            if end > len {
                m.problems.push(format!(
                    "the {} section at {pos:#x} runs {} bytes past the end of the file",
                    section_name(id),
                    end - len
                ));
                end = len;
            }
            let (name, content) = if id == CUSTOM {
                let mut n = Reader::new(data, payload, end);
                match n.name() {
                    Some(b) => (util::lossy(b), n.pos()),
                    None => (String::new(), end),
                }
            } else {
                (section_name(id).to_string(), payload)
            };
            m.sections.push(Sec {
                id,
                name,
                start: pos,
                payload,
                content,
                end,
            });
            pos = end;
        }
        // The standard sections first, in order; then the custom ones, whose
        // names and symbols refer to what the others define.
        for pass in [false, true] {
            for i in 0..m.sections.len() {
                let s = m.sections[i].clone();
                if (s.id == CUSTOM) != pass {
                    continue;
                }
                let mut r = Reader::new(data, s.content, s.end);
                let read = match s.id {
                    TYPE => m.read_types(&mut r),
                    IMPORT => m.read_imports(&mut r),
                    FUNCTION => m.read_functions(&mut r),
                    TABLE => m.read_tables(&mut r),
                    MEMORY => m.read_memories(&mut r),
                    GLOBAL => m.read_globals(&mut r),
                    EXPORT => m.read_exports(&mut r),
                    START => r.u32().map(|f| m.start = Some(f)),
                    ELEMENT => m.read_elements(&mut r),
                    CODE => m.read_code(&mut r),
                    DATA => m.read_data(&mut r),
                    DATA_COUNT => r.u32().map(|n| m.data_count = Some(n)),
                    TAG => m.read_tags(&mut r),
                    CUSTOM => m.read_custom(&s, &mut r),
                    _ => {
                        m.problems.push(format!(
                            "section {} at {:#x} has an id binviz doesn't know",
                            s.id, s.start
                        ));
                        Some(())
                    }
                };
                match read {
                    None => m.problems.push(format!(
                        "the {} section at {:#x} can't be read past {:#x}",
                        s.name,
                        s.start,
                        r.pos()
                    )),
                    Some(()) if s.id != CUSTOM && s.id <= TAG && !r.is_empty() => m.problems.push(format!(
                        "the {} section at {:#x} has {} bytes after its last entry",
                        s.name,
                        s.start,
                        r.end() - r.pos()
                    )),
                    Some(()) => {}
                }
            }
        }
        if m.funcs.iter().skip(m.imported_funcs as usize).any(|f| f.body.is_none()) {
            m.problems
                .push("the function section declares more functions than the code section has bodies".into());
        }
        m.copied = m.copied_to(data);
        Ok(m)
    }

    /// Where the passive data segments are copied into memory. A build with
    /// threads (shared memory) has only passive segments, and wasm-ld's
    /// `__wasm_init_memory`, its start function, copies each one whole with
    /// `memory.init` to a constant address: segment → that address.
    fn copied_to(&self, data: &[u8]) -> HashMap<u32, u64> {
        let mut out = HashMap::new();
        let init = self.start.or_else(|| {
            let named = self.names.functions.iter();
            named
                .filter(|(_, n)| *n == "__wasm_init_memory")
                .map(|(&i, _)| i)
                .next()
        });
        let Some(body) = init.and_then(|f| self.funcs.get(f as usize)?.body) else {
            return out;
        };
        let mut stack = code::Stack::default();
        for (_, insn) in &code::decode_body(data, body).insns {
            // memory.init: (to, from, length) on the stack, the length on top.
            if let (0xFC_0008, code::Imm::Two(segment, 0)) = (insn.op, &insn.imm)
                && let Some(seg) = self.data.get(*segment as usize)
                && seg.mode == Mode::Passive
                && let (Some(to), Some(0), Some(n)) = (stack.peek(2), stack.peek(1), stack.peek(0))
                && n == seg.bytes.end - seg.bytes.start
            {
                out.entry(*segment).or_insert(to);
            }
            stack.step(self, insn);
        }
        out
    }

    fn read_types(&mut self, r: &mut Reader) -> Option<()> {
        for _ in 0..r.count()? {
            self.types.extend(read_type_entry(r)?);
        }
        Some(())
    }

    fn read_imports(&mut self, r: &mut Reader) -> Option<()> {
        for _ in 0..r.count()? {
            let import = Import::read(r)?;
            let index = self.imports.len() as u32;
            let entry = import.entry.clone();
            match &import.desc {
                ImportDesc::Func(t) => {
                    self.funcs.push(Func {
                        type_index: *t,
                        import: Some(index),
                        body: None,
                    });
                    self.imported_funcs += 1;
                }
                ImportDesc::Table(ty, limits) => self.tables.push(Table {
                    ty: *ty,
                    limits: *limits,
                    import: Some(index),
                    entry,
                }),
                ImportDesc::Memory(limits) => self.memories.push(Memory {
                    limits: *limits,
                    import: Some(index),
                    entry,
                }),
                ImportDesc::Global(ty, mutable) => self.globals.push(Global {
                    ty: *ty,
                    mutable: *mutable,
                    init: None,
                    import: Some(index),
                    entry,
                }),
                ImportDesc::Tag(t) => self.tags.push(Tag {
                    type_index: *t,
                    import: Some(index),
                    entry,
                }),
            }
            self.imports.push(import);
        }
        Some(())
    }

    fn read_functions(&mut self, r: &mut Reader) -> Option<()> {
        for _ in 0..r.count()? {
            let type_index = r.u32()?;
            self.funcs.push(Func {
                type_index,
                import: None,
                body: None,
            });
        }
        Some(())
    }

    fn read_tables(&mut self, r: &mut Reader) -> Option<()> {
        for _ in 0..r.count()? {
            self.tables.push(Table::read(r)?);
        }
        Some(())
    }

    fn read_memories(&mut self, r: &mut Reader) -> Option<()> {
        for _ in 0..r.count()? {
            let start = r.pos();
            let limits = Limits::read(r)?;
            self.memories.push(Memory {
                limits,
                import: None,
                entry: start..r.pos(),
            });
        }
        Some(())
    }

    fn read_globals(&mut self, r: &mut Reader) -> Option<()> {
        for _ in 0..r.count()? {
            self.globals.push(Global::read(r)?);
        }
        Some(())
    }

    fn read_exports(&mut self, r: &mut Reader) -> Option<()> {
        for _ in 0..r.count()? {
            self.exports.push(Export::read(r)?);
        }
        Some(())
    }

    fn read_elements(&mut self, r: &mut Reader) -> Option<()> {
        for _ in 0..r.count()? {
            self.elements.push(Element::read(r)?);
        }
        Some(())
    }

    fn read_code(&mut self, r: &mut Reader) -> Option<()> {
        let n = r.count()?;
        let mut index = self.imported_funcs as usize;
        for _ in 0..n {
            let b = Body::read(r)?;
            match self.funcs.get_mut(index) {
                Some(f) if f.import.is_none() => f.body = Some(b),
                _ => {
                    // More bodies than declared functions: kept, with no type.
                    self.funcs.push(Func {
                        type_index: u32::MAX,
                        import: None,
                        body: Some(b),
                    });
                    self.problems.push(format!(
                        "the code section has a body at {:#x} for no declared function",
                        b.entry
                    ));
                }
            }
            index += 1;
        }
        Some(())
    }

    fn read_data(&mut self, r: &mut Reader) -> Option<()> {
        for _ in 0..r.count()? {
            self.data.push(Segment::read(r)?);
        }
        Some(())
    }

    fn read_tags(&mut self, r: &mut Reader) -> Option<()> {
        for _ in 0..r.count()? {
            let start = r.pos();
            r.u8()?;
            let type_index = r.u32()?;
            self.tags.push(Tag {
                type_index,
                import: None,
                entry: start..r.pos(),
            });
        }
        Some(())
    }

    fn read_custom(&mut self, s: &Sec, r: &mut Reader) -> Option<()> {
        match s.name.as_str() {
            "name" => self.read_names(r),
            "producers" => {
                for _ in 0..r.count()? {
                    let field = r.string()?;
                    let mut values = Vec::new();
                    for _ in 0..r.count()? {
                        values.push((r.string()?, r.string()?));
                    }
                    self.producers.push((field, values));
                }
                Some(())
            }
            "target_features" => {
                for _ in 0..r.count()? {
                    let prefix = r.u8()? as char;
                    self.features.push((prefix, r.string()?));
                }
                Some(())
            }
            "sourceMappingURL" => r.string().map(|u| self.source_map_url = Some(u)),
            "external_debug_info" => r.string().map(|u| self.external_debug_info = Some(u)),
            "build_id" => r.name().map(|b| self.build_id = Some(b.to_vec())),
            "linking" => self.read_linking(r),
            _ => Some(()),
        }
    }

    fn read_names(&mut self, r: &mut Reader) -> Option<()> {
        while !r.is_empty() {
            let id = r.u8()?;
            let size = r.u32()?;
            let mut sub = Reader::new(r.data, r.pos(), r.pos() + u64::from(size));
            r.skip(u64::from(size))?;
            let map = |sub: &mut Reader| -> Option<HashMap<u32, String>> {
                let mut out = HashMap::new();
                for _ in 0..sub.count()? {
                    let index = sub.u32()?;
                    out.insert(index, sub.string()?);
                }
                Some(out)
            };
            let n = &mut self.names;
            match id {
                0 => n.module = Some(sub.string()?),
                1 => n.functions = map(&mut sub)?,
                2 => {
                    for _ in 0..sub.count()? {
                        let f = sub.u32()?;
                        let mut locals = Vec::new();
                        for _ in 0..sub.count()? {
                            locals.push((sub.u32()?, sub.string()?));
                        }
                        n.locals.insert(f, locals);
                    }
                }
                4 => n.types = map(&mut sub)?,
                5 => n.tables = map(&mut sub)?,
                6 => n.memories = map(&mut sub)?,
                7 => n.globals = map(&mut sub)?,
                8 => n.elements = map(&mut sub)?,
                9 => n.data = map(&mut sub)?,
                11 => n.tags = map(&mut sub)?,
                _ => {}
            }
        }
        Some(())
    }

    /// An object file's `linking` section: its symbol table and its data segments' names.
    fn read_linking(&mut self, r: &mut Reader) -> Option<()> {
        r.u32()?;
        while !r.is_empty() {
            let kind = r.u8()?;
            let size = r.u32()?;
            let mut sub = Reader::new(r.data, r.pos(), r.pos() + u64::from(size));
            r.skip(u64::from(size))?;
            match kind {
                // WASM_SEGMENT_INFO: name, alignment, flags per segment.
                5 => {
                    for i in 0..sub.count()? {
                        let name = sub.string()?;
                        sub.u32()?;
                        sub.u32()?;
                        self.segment_names.insert(i, name);
                    }
                }
                // WASM_SYMBOL_TABLE
                8 => {
                    for _ in 0..sub.count()? {
                        let kind = sub.u8()?;
                        let flags = sub.u32()?;
                        let mut s = LinkSymbol {
                            kind,
                            index: 0,
                            name: None,
                            data: None,
                        };
                        match kind {
                            // Function, global, tag, table: an index, and a name
                            // when defined or explicitly named.
                            0 | 2 | 4 | 5 => {
                                s.index = sub.u32()?;
                                if flags & SYM_UNDEFINED == 0 || flags & SYM_EXPLICIT_NAME != 0 {
                                    s.name = Some(sub.string()?);
                                }
                            }
                            1 => {
                                s.name = Some(sub.string()?);
                                if flags & SYM_UNDEFINED == 0 {
                                    s.data = Some((sub.u32()?, sub.u64()?, sub.u64()?));
                                }
                            }
                            3 => s.index = sub.u32()?,
                            _ => return None,
                        }
                        self.symbols.push(s);
                    }
                }
                _ => {}
            }
        }
        Some(())
    }

    /// The first section with this id.
    pub fn section(&self, id: u8) -> Option<&Sec> {
        self.sections.iter().find(|s| s.id == id)
    }

    /// Where DWARF's code addresses count from: the code section's contents.
    pub fn code_base(&self) -> Option<u64> {
        self.section(CODE).map(|s| s.payload)
    }

    /// Whether the module is a relocatable object (it has a `linking` section).
    pub fn is_object(&self) -> bool {
        self.sections.iter().any(|s| s.id == CUSTOM && s.name == "linking")
    }

    pub fn func_type(&self, index: u32) -> Option<&FuncType> {
        let t = self.funcs.get(index as usize)?.type_index;
        self.types.get(t as usize)?.as_ref()
    }

    /// The defined function whose body holds `offset` (its size before it excluded).
    pub fn function_at(&self, offset: u64) -> Option<u32> {
        let defined = &self.funcs[(self.imported_funcs as usize).min(self.funcs.len())..];
        let i = defined
            .partition_point(|f| f.body.is_some_and(|b| b.start <= offset))
            .checked_sub(1)?;
        let b = defined[i].body?;
        (offset < b.end).then_some(self.imported_funcs + i as u32)
    }

    /// The defined functions whose entries in the code section (their sizes
    /// included) overlap `lo..hi`, in order.
    pub fn functions_in(&self, lo: u64, hi: u64) -> impl Iterator<Item = (u32, Body)> + '_ {
        let first = self.imported_funcs as usize;
        let defined = &self.funcs[first.min(self.funcs.len())..];
        let from = defined.partition_point(|f| f.body.is_some_and(|b| b.end <= lo));
        defined[from..]
            .iter()
            .enumerate()
            .filter_map(move |(i, f)| Some(((first + from + i) as u32, f.body?)))
            .take_while(move |(_, b)| b.entry < hi)
    }
}

/// An entry of the type section: a type, or a recursive group of them (the
/// GC proposal), each of which takes a type index.
pub(crate) fn read_type_entry(r: &mut Reader) -> Option<Vec<Option<FuncType>>> {
    match r.u8()? {
        0x4E => {
            let mut out = Vec::new();
            for _ in 0..r.count()? {
                let first = r.u8()?;
                out.push(read_subtype(first, r)?);
            }
            Some(out)
        }
        first => Some(vec![read_subtype(first, r)?]),
    }
}

/// A type definition after its first byte: a function, struct or array
/// type, possibly declared as a subtype. Only function types are kept.
fn read_subtype(first: u8, r: &mut Reader) -> Option<Option<FuncType>> {
    let first = match first {
        // sub, sub final: the supertypes, then the type.
        0x50 | 0x4F => {
            for _ in 0..r.count()? {
                r.u32()?;
            }
            r.u8()?
        }
        b => b,
    };
    let storage = |r: &mut Reader| -> Option<()> {
        match r.u8()? {
            // Packed i8 and i16.
            0x78 | 0x77 => {}
            b => {
                ValType::read_after(b, r)?;
            }
        }
        r.u8().map(|_| ())
    };
    match first {
        0x60 => {
            let mut ty = FuncType::default();
            for _ in 0..r.count()? {
                ty.params.push(ValType::read(r)?);
            }
            for _ in 0..r.count()? {
                ty.results.push(ValType::read(r)?);
            }
            Some(Some(ty))
        }
        0x5F => {
            for _ in 0..r.count()? {
                storage(r)?;
            }
            Some(None)
        }
        0x5E => {
            storage(r)?;
            Some(None)
        }
        _ => None,
    }
}

/// Skips a body's locals declarations.
pub(crate) fn skip_locals(r: &mut Reader) -> Option<()> {
    for _ in 0..r.count()? {
        r.u32()?;
        ValType::read(r)?;
    }
    Some(())
}

/// A body's locals declarations: (how many, of which type).
pub(crate) fn read_locals(r: &mut Reader) -> Option<Vec<(u32, ValType)>> {
    let mut out = Vec::new();
    for _ in 0..r.count()? {
        out.push((r.u32()?, ValType::read(r)?));
    }
    Some(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn leb128() {
        assert_eq!(uleb(&[0x00], 32), Some((0, 1)));
        assert_eq!(uleb(&[0xe5, 0x8e, 0x26], 32), Some((624_485, 3)));
        // Padded to five bytes, as a linker leaves a relocated index.
        assert_eq!(uleb(&[0x82, 0x80, 0x80, 0x80, 0x00], 32), Some((2, 5)));
        // Too long for 32 bits, or cut short.
        assert_eq!(uleb(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x00], 32), None);
        assert_eq!(uleb(&[0x80, 0x80], 32), None);
        assert_eq!(
            uleb(&[0xff, 0xff, 0xff, 0xff, 0x0f], 32),
            Some((u64::from(u32::MAX), 5))
        );
        assert_eq!(
            uleb(&[0xff; 9].iter().copied().chain([0x01]).collect::<Vec<_>>(), 64),
            Some((u64::MAX, 10))
        );
        assert_eq!(sleb(&[0x7f], 32), Some((-1, 1)));
        assert_eq!(sleb(&[0xc0, 0xbb, 0x78], 32), Some((-123_456, 3)));
        assert_eq!(sleb(&[0x3f], 32), Some((63, 1)));
        assert_eq!(sleb(&[0x40], 32), Some((-64, 1)));
        // A heap type: 0x70 is funcref's code, -0x10 as a signed 33-bit number.
        assert_eq!(sleb(&[0x70], 33), Some((-16, 1)));
        assert_eq!(
            sleb(&[0x80, 0x80, 0x80, 0x80, 0x78], 32),
            Some((i64::from(i32::MIN), 5))
        );
    }

    /// A section: its id, its size, then its payload.
    fn section(id: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![id, payload.len() as u8];
        out.extend_from_slice(payload);
        out
    }

    /// A small module: one imported and two defined functions, a table
    /// holding one of them, a memory with a data segment, a global, exports
    /// and a name section.
    pub(crate) fn sample() -> Vec<u8> {
        let mut m = b"\0asm\x01\0\0\0".to_vec();
        // Types: (i32) -> (), (i32, i32) -> i32.
        m.extend(section(TYPE, &[2, 0x60, 1, 0x7f, 0, 0x60, 2, 0x7f, 0x7f, 1, 0x7f]));
        m.extend(section(IMPORT, &[1, 3, b'e', b'n', b'v', 3, b'l', b'o', b'g', 0, 0]));
        m.extend(section(FUNCTION, &[2, 1, 1]));
        m.extend(section(TABLE, &[1, 0x70, 0, 1]));
        m.extend(section(MEMORY, &[1, 0, 1]));
        // A mutable i32 global starting at 1024.
        m.extend(section(GLOBAL, &[1, 0x7f, 1, 0x41, 0x80, 0x08, 0x0b]));
        m.extend(section(
            EXPORT,
            &[2, 3, b'a', b'd', b'd', 0, 1, 3, b'm', b'e', b'm', 2, 0],
        ));
        // Table 0 from offset 1: function 2.
        m.extend(section(ELEMENT, &[1, 0, 0x41, 1, 0x0b, 1, 2]));
        // Function 1 adds its arguments; function 2 calls the import and returns 0.
        let add = [0, 0x20, 0, 0x20, 1, 0x6a, 0x0b];
        let other = [1, 1, 0x7f, 0x20, 0, 0x10, 0, 0x41, 0, 0x0b];
        let mut code = vec![2, add.len() as u8];
        code.extend(add);
        code.push(other.len() as u8);
        code.extend(other);
        m.extend(section(CODE, &code));
        m.extend(section(DATA, &[1, 0, 0x41, 0x80, 0x08, 0x0b, 3, b'h', b'i', 0]));
        let mut names = b"\x04name".to_vec();
        // Function names: 0 "log", 1 "add"; module name "demo".
        names.extend([0, 5, 4, b'd', b'e', b'm', b'o']);
        names.extend([1, 11, 2, 0, 3, b'l', b'o', b'g', 1, 3, b'a', b'd', b'd']);
        m.extend(section(CUSTOM, &names));
        m
    }

    #[test]
    fn passive_segments_are_placed_where_the_start_function_copies_them() {
        let mut m = b"\0asm\x01\0\0\0".to_vec();
        m.extend(section(TYPE, &[1, 0x60, 0, 0]));
        m.extend(section(FUNCTION, &[1, 0]));
        m.extend(section(MEMORY, &[1, 0, 1]));
        m.extend(section(START, &[0]));
        m.extend(section(DATA_COUNT, &[2]));
        // Segment 0 copied whole to 1024; segment 1 only in part, so nowhere.
        let body = [
            0, 0x41, 0x80, 0x08, 0x41, 0, 0x41, 3, 0xfc, 8, 0, 0, 0x41, 0x90, 0x08, 0x41, 0, 0x41, 1, 0xfc, 8, 1, 0,
            0x0b,
        ];
        let mut code = vec![1, body.len() as u8];
        code.extend(body);
        m.extend(section(CODE, &code));
        m.extend(section(DATA, &[2, 1, 3, b'h', b'i', 0, 1, 2, b'y', b'o']));
        let module = Module::parse(&m).unwrap();
        assert!(module.problems.is_empty(), "{:?}", module.problems);
        assert_eq!(module.copied, HashMap::from([(0, 1024)]));
        assert_eq!(module.segment_address(0), Some(crate::wasm::MEMORY_BASE + 1024));
        assert_eq!(module.segment_address(1), None);
    }

    #[test]
    fn sections_and_what_they_declare() {
        let data = sample();
        let m = Module::parse(&data).unwrap();
        assert!(m.problems.is_empty(), "{:?}", m.problems);
        let ids: Vec<u8> = m.sections.iter().map(|s| s.id).collect();
        assert_eq!(ids, [1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 0]);
        assert_eq!(m.sections.last().unwrap().name, "name");
        assert_eq!(m.types.len(), 2);
        assert_eq!(m.types[1].as_ref().unwrap().to_string(), "(i32, i32) -> i32");
        assert_eq!(m.types[0].as_ref().unwrap().to_string(), "(i32) -> ()");
        assert_eq!((m.imported_funcs, m.funcs.len()), (1, 3));
        assert_eq!(
            (m.imports[0].module.as_str(), m.imports[0].field.as_str()),
            ("env", "log")
        );
        // The bodies: where each starts, and its first instruction after the locals.
        let add = m.funcs[1].body.unwrap();
        assert_eq!(data[add.start as usize], 0, "no locals");
        assert_eq!(add.code, add.start + 1);
        assert_eq!(data[add.code as usize], 0x20);
        let other = m.funcs[2].body.unwrap();
        assert_eq!(other.code, other.start + 3, "one run of locals");
        assert_eq!(m.function_at(add.code + 2), Some(1));
        assert_eq!(m.function_at(other.start), Some(2));
        assert_eq!(m.function_at(add.entry), None, "a body's size is not in it");
        assert_eq!(m.functions_in(add.entry, other.entry + 1).count(), 2);
        assert_eq!(m.globals[0].init, Some(Const::I32(1024)));
        assert!(m.globals[0].mutable);
        assert_eq!(m.exports.len(), 2);
        assert_eq!((m.exports[1].kind, m.exports[1].index), (Kind::Memory, 0));
        assert_eq!(m.elements[0].items[0].0, Some(2));
        assert_eq!(
            m.elements[0].mode,
            Mode::Active {
                index: 0,
                offset: Const::I32(1)
            }
        );
        let seg = &m.data[0];
        assert_eq!(&data[seg.bytes.start as usize..seg.bytes.end as usize], b"hi\0");
        assert_eq!(m.names.module.as_deref(), Some("demo"));
        assert_eq!(m.names.functions.get(&1).map(String::as_str), Some("add"));
        assert_eq!(m.memories[0].limits.min, 1);
    }

    #[test]
    fn broken_modules() {
        assert!(Module::parse(b"\0asm\x0d\0\x01\0").is_err(), "a component");
        assert!(Module::parse(b"\x7fELF").is_err());
        // A section running past the end: noted, and the rest still read.
        let mut data = sample();
        data.truncate(data.len() - 4);
        let m = Module::parse(&data).unwrap();
        assert!(
            m.problems.iter().any(|p| p.contains("past the end")),
            "{:?}",
            m.problems
        );
        assert_eq!(m.funcs.len(), 3);
        // A count far larger than the section: an error, not an allocation.
        let bad = [b"\0asm\x01\0\0\0".as_slice(), &[TYPE, 5, 0xff, 0xff, 0xff, 0xff, 0x0f]].concat();
        let m = Module::parse(&bad).unwrap();
        assert!(m.types.is_empty());
        assert_eq!(m.problems.len(), 1, "{:?}", m.problems);
    }
}
