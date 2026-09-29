//! WebAssembly instructions: the bytecode of function bodies and constant
//! expressions, decoded one instruction at a time into the text format's
//! names and immediates. Covered: the MVP, sign extension, the `0xFC` group
//! (saturating truncation, bulk memory, table operations), reference
//! types, tail calls, exception handling (both encodings), SIMD and relaxed
//! SIMD (`0xFD`), and threads' atomics (`0xFE`). An opcode outside these
//! (the GC proposal's `0xFB` group, say) can't be stepped over, since its
//! immediates' length is unknown: decoding stops there.
//!
//! Branches are structured: `br 1` leaves the second enclosing block. A
//! body is decoded whole so that each branch gets the address it goes to:
//! the `end` of the block it leaves, or the `loop` it repeats.

use super::read::{Body, HeapType, Module, Reader, ValType, read_locals};

/// An opcode: its byte, or for a prefixed one, `prefix << 16 | number`
/// (`memory.copy` is `0xFC 10`: `0xFC000A`).
pub(crate) type Opcode = u32;

pub(crate) const UNREACHABLE: Opcode = 0x00;
pub(crate) const BLOCK: Opcode = 0x02;
pub(crate) const LOOP: Opcode = 0x03;
pub(crate) const IF: Opcode = 0x04;
pub(crate) const ELSE: Opcode = 0x05;
pub(crate) const TRY: Opcode = 0x06;
pub(crate) const THROW: Opcode = 0x08;
pub(crate) const RETHROW: Opcode = 0x09;
pub(crate) const THROW_REF: Opcode = 0x0A;
pub(crate) const END: Opcode = 0x0B;
pub(crate) const BR: Opcode = 0x0C;
pub(crate) const BR_IF: Opcode = 0x0D;
pub(crate) const BR_TABLE: Opcode = 0x0E;
pub(crate) const RETURN: Opcode = 0x0F;
pub(crate) const CALL: Opcode = 0x10;
pub(crate) const CALL_INDIRECT: Opcode = 0x11;
pub(crate) const RETURN_CALL: Opcode = 0x12;
pub(crate) const RETURN_CALL_INDIRECT: Opcode = 0x13;
pub(crate) const CALL_REF: Opcode = 0x14;
pub(crate) const RETURN_CALL_REF: Opcode = 0x15;
pub(crate) const DELEGATE: Opcode = 0x18;
pub(crate) const DROP: Opcode = 0x1A;
pub(crate) const SELECT: Opcode = 0x1B;
pub(crate) const SELECT_T: Opcode = 0x1C;
pub(crate) const TRY_TABLE: Opcode = 0x1F;
pub(crate) const LOCAL_GET: Opcode = 0x20;
pub(crate) const LOCAL_SET: Opcode = 0x21;
pub(crate) const LOCAL_TEE: Opcode = 0x22;
pub(crate) const GLOBAL_GET: Opcode = 0x23;
pub(crate) const GLOBAL_SET: Opcode = 0x24;
pub(crate) const MEMORY_GROW: Opcode = 0x40;
pub(crate) const I32_CONST: Opcode = 0x41;
pub(crate) const I64_CONST: Opcode = 0x42;
pub(crate) const F32_CONST: Opcode = 0x43;
pub(crate) const F64_CONST: Opcode = 0x44;
pub(crate) const I32_ADD: Opcode = 0x6A;
pub(crate) const REF_NULL: Opcode = 0xD0;
pub(crate) const REF_FUNC: Opcode = 0xD2;
pub(crate) const BR_ON_NULL: Opcode = 0xD5;
pub(crate) const BR_ON_NON_NULL: Opcode = 0xD6;
pub(crate) const V128_CONST: Opcode = 0xFD_000C;

/// What follows an opcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum K {
    None,
    Block,
    Label,
    BrTable,
    Func,
    Indirect,
    Local,
    Global,
    Table,
    Mem,
    Memory,
    I32,
    I64,
    F32,
    F64,
    SelectT,
    Heap,
    TryTable,
    Tag,
    Type,
    /// Two indices: `memory.init` (data, memory), `memory.copy`, `table.init`, `table.copy`.
    Two,
    /// One index: `data.drop`, `memory.fill`, `elem.drop`.
    One,
    V128,
    Shuffle,
    Lane,
    MemLane,
    /// `atomic.fence`: a reserved zero byte.
    Byte,
}

/// A block's type: what it takes and leaves on the stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockType {
    Empty,
    Value(ValType),
    Type(u32),
}

/// A memory access's immediates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MemArg {
    /// log2 of the alignment the code promises.
    pub align: u32,
    pub offset: u64,
    pub memory: u32,
}

/// An instruction's immediates.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Imm {
    None,
    Block(BlockType),
    Label(u32),
    /// `br_table`: the targets, then the default.
    Labels(Vec<u32>),
    Func(u32),
    Indirect {
        ty: u32,
        table: u32,
    },
    Local(u32),
    Global(u32),
    Table(u32),
    Mem(MemArg),
    Memory(u32),
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
    Types(Vec<ValType>),
    Heap(HeapType),
    /// `try_table`: its type and catch clauses (kind, tag, label).
    TryTable(BlockType, Vec<(u8, Option<u32>, u32)>),
    Tag(u32),
    Type(u32),
    Two(u32, u32),
    Index(u32),
    V128([u8; 16]),
    Lanes([u8; 16]),
    Lane(u8),
    MemLane(MemArg, u8),
}

/// A decoded instruction.
#[derive(Clone, Debug)]
pub(crate) struct Insn {
    pub op: Opcode,
    pub name: &'static str,
    pub len: u32,
    pub imm: Imm,
    /// False for an opcode binviz doesn't know: `len` covers only the
    /// opcode, and what follows can't be decoded.
    pub known: bool,
}

/// Memory accesses 0x28..=0x3E: their names and access widths in bytes.
const MEMORY: [(&str, u8); 23] = [
    ("i32.load", 4),
    ("i64.load", 8),
    ("f32.load", 4),
    ("f64.load", 8),
    ("i32.load8_s", 1),
    ("i32.load8_u", 1),
    ("i32.load16_s", 2),
    ("i32.load16_u", 2),
    ("i64.load8_s", 1),
    ("i64.load8_u", 1),
    ("i64.load16_s", 2),
    ("i64.load16_u", 2),
    ("i64.load32_s", 4),
    ("i64.load32_u", 4),
    ("i32.store", 4),
    ("i64.store", 8),
    ("f32.store", 4),
    ("f64.store", 8),
    ("i32.store8", 1),
    ("i32.store16", 2),
    ("i64.store8", 1),
    ("i64.store16", 2),
    ("i64.store32", 4),
];

/// Numeric instructions 0x45..=0xC4, which take no immediates.
#[rustfmt::skip]
const NUMERIC: [&str; 128] = [
    "i32.eqz", "i32.eq", "i32.ne", "i32.lt_s", "i32.lt_u", "i32.gt_s", "i32.gt_u", "i32.le_s", "i32.le_u", "i32.ge_s", "i32.ge_u",
    "i64.eqz", "i64.eq", "i64.ne", "i64.lt_s", "i64.lt_u", "i64.gt_s", "i64.gt_u", "i64.le_s", "i64.le_u", "i64.ge_s", "i64.ge_u",
    "f32.eq", "f32.ne", "f32.lt", "f32.gt", "f32.le", "f32.ge",
    "f64.eq", "f64.ne", "f64.lt", "f64.gt", "f64.le", "f64.ge",
    "i32.clz", "i32.ctz", "i32.popcnt", "i32.add", "i32.sub", "i32.mul", "i32.div_s", "i32.div_u", "i32.rem_s", "i32.rem_u",
    "i32.and", "i32.or", "i32.xor", "i32.shl", "i32.shr_s", "i32.shr_u", "i32.rotl", "i32.rotr",
    "i64.clz", "i64.ctz", "i64.popcnt", "i64.add", "i64.sub", "i64.mul", "i64.div_s", "i64.div_u", "i64.rem_s", "i64.rem_u",
    "i64.and", "i64.or", "i64.xor", "i64.shl", "i64.shr_s", "i64.shr_u", "i64.rotl", "i64.rotr",
    "f32.abs", "f32.neg", "f32.ceil", "f32.floor", "f32.trunc", "f32.nearest", "f32.sqrt",
    "f32.add", "f32.sub", "f32.mul", "f32.div", "f32.min", "f32.max", "f32.copysign",
    "f64.abs", "f64.neg", "f64.ceil", "f64.floor", "f64.trunc", "f64.nearest", "f64.sqrt",
    "f64.add", "f64.sub", "f64.mul", "f64.div", "f64.min", "f64.max", "f64.copysign",
    "i32.wrap_i64", "i32.trunc_f32_s", "i32.trunc_f32_u", "i32.trunc_f64_s", "i32.trunc_f64_u",
    "i64.extend_i32_s", "i64.extend_i32_u", "i64.trunc_f32_s", "i64.trunc_f32_u", "i64.trunc_f64_s", "i64.trunc_f64_u",
    "f32.convert_i32_s", "f32.convert_i32_u", "f32.convert_i64_s", "f32.convert_i64_u", "f32.demote_f64",
    "f64.convert_i32_s", "f64.convert_i32_u", "f64.convert_i64_s", "f64.convert_i64_u", "f64.promote_f32",
    "i32.reinterpret_f32", "i64.reinterpret_f64", "f32.reinterpret_i32", "f64.reinterpret_i64",
    "i32.extend8_s", "i32.extend16_s", "i64.extend8_s", "i64.extend16_s", "i64.extend32_s",
];

/// The `0xFD` group, by number (empty: reserved).
#[rustfmt::skip]
const SIMD: [&str; 0x114] = [
    // 0x00
    "v128.load", "v128.load8x8_s", "v128.load8x8_u", "v128.load16x4_s", "v128.load16x4_u", "v128.load32x2_s",
    "v128.load32x2_u", "v128.load8_splat", "v128.load16_splat", "v128.load32_splat", "v128.load64_splat", "v128.store",
    "v128.const", "i8x16.shuffle", "i8x16.swizzle", "i8x16.splat",
    // 0x10
    "i16x8.splat", "i32x4.splat", "i64x2.splat", "f32x4.splat", "f64x2.splat", "i8x16.extract_lane_s",
    "i8x16.extract_lane_u", "i8x16.replace_lane", "i16x8.extract_lane_s", "i16x8.extract_lane_u", "i16x8.replace_lane",
    "i32x4.extract_lane", "i32x4.replace_lane", "i64x2.extract_lane", "i64x2.replace_lane", "f32x4.extract_lane",
    // 0x20
    "f32x4.replace_lane", "f64x2.extract_lane", "f64x2.replace_lane", "i8x16.eq", "i8x16.ne", "i8x16.lt_s", "i8x16.lt_u",
    "i8x16.gt_s", "i8x16.gt_u", "i8x16.le_s", "i8x16.le_u", "i8x16.ge_s", "i8x16.ge_u", "i16x8.eq", "i16x8.ne",
    "i16x8.lt_s",
    // 0x30
    "i16x8.lt_u", "i16x8.gt_s", "i16x8.gt_u", "i16x8.le_s", "i16x8.le_u", "i16x8.ge_s", "i16x8.ge_u", "i32x4.eq",
    "i32x4.ne", "i32x4.lt_s", "i32x4.lt_u", "i32x4.gt_s", "i32x4.gt_u", "i32x4.le_s", "i32x4.le_u", "i32x4.ge_s",
    // 0x40
    "i32x4.ge_u", "f32x4.eq", "f32x4.ne", "f32x4.lt", "f32x4.gt", "f32x4.le", "f32x4.ge", "f64x2.eq", "f64x2.ne",
    "f64x2.lt", "f64x2.gt", "f64x2.le", "f64x2.ge", "v128.not", "v128.and", "v128.andnot",
    // 0x50
    "v128.or", "v128.xor", "v128.bitselect", "v128.any_true", "v128.load8_lane", "v128.load16_lane", "v128.load32_lane",
    "v128.load64_lane", "v128.store8_lane", "v128.store16_lane", "v128.store32_lane", "v128.store64_lane",
    "v128.load32_zero", "v128.load64_zero", "f32x4.demote_f64x2_zero", "f64x2.promote_low_f32x4",
    // 0x60
    "i8x16.abs", "i8x16.neg", "i8x16.popcnt", "i8x16.all_true", "i8x16.bitmask", "i8x16.narrow_i16x8_s",
    "i8x16.narrow_i16x8_u", "f32x4.ceil", "f32x4.floor", "f32x4.trunc", "f32x4.nearest", "i8x16.shl", "i8x16.shr_s",
    "i8x16.shr_u", "i8x16.add", "i8x16.add_sat_s",
    // 0x70
    "i8x16.add_sat_u", "i8x16.sub", "i8x16.sub_sat_s", "i8x16.sub_sat_u", "f64x2.ceil", "f64x2.floor", "i8x16.min_s",
    "i8x16.min_u", "i8x16.max_s", "i8x16.max_u", "f64x2.trunc", "i8x16.avgr_u", "i16x8.extadd_pairwise_i8x16_s",
    "i16x8.extadd_pairwise_i8x16_u", "i32x4.extadd_pairwise_i16x8_s", "i32x4.extadd_pairwise_i16x8_u",
    // 0x80
    "i16x8.abs", "i16x8.neg", "i16x8.q15mulr_sat_s", "i16x8.all_true", "i16x8.bitmask", "i16x8.narrow_i32x4_s",
    "i16x8.narrow_i32x4_u", "i16x8.extend_low_i8x16_s", "i16x8.extend_high_i8x16_s", "i16x8.extend_low_i8x16_u",
    "i16x8.extend_high_i8x16_u", "i16x8.shl", "i16x8.shr_s", "i16x8.shr_u", "i16x8.add", "i16x8.add_sat_s",
    // 0x90
    "i16x8.add_sat_u", "i16x8.sub", "i16x8.sub_sat_s", "i16x8.sub_sat_u", "f64x2.nearest", "i16x8.mul", "i16x8.min_s",
    "i16x8.min_u", "i16x8.max_s", "i16x8.max_u", "", "i16x8.avgr_u", "i16x8.extmul_low_i8x16_s",
    "i16x8.extmul_high_i8x16_s", "i16x8.extmul_low_i8x16_u", "i16x8.extmul_high_i8x16_u",
    // 0xA0
    "i32x4.abs", "i32x4.neg", "", "i32x4.all_true", "i32x4.bitmask", "", "", "i32x4.extend_low_i16x8_s",
    "i32x4.extend_high_i16x8_s", "i32x4.extend_low_i16x8_u", "i32x4.extend_high_i16x8_u", "i32x4.shl", "i32x4.shr_s",
    "i32x4.shr_u", "i32x4.add", "",
    // 0xB0
    "", "i32x4.sub", "", "", "", "i32x4.mul", "i32x4.min_s", "i32x4.min_u", "i32x4.max_s", "i32x4.max_u",
    "i32x4.dot_i16x8_s", "", "i32x4.extmul_low_i16x8_s", "i32x4.extmul_high_i16x8_s", "i32x4.extmul_low_i16x8_u",
    "i32x4.extmul_high_i16x8_u",
    // 0xC0
    "i64x2.abs", "i64x2.neg", "", "i64x2.all_true", "i64x2.bitmask", "", "", "i64x2.extend_low_i32x4_s",
    "i64x2.extend_high_i32x4_s", "i64x2.extend_low_i32x4_u", "i64x2.extend_high_i32x4_u", "i64x2.shl", "i64x2.shr_s",
    "i64x2.shr_u", "i64x2.add", "",
    // 0xD0
    "", "i64x2.sub", "", "", "", "i64x2.mul", "i64x2.eq", "i64x2.ne", "i64x2.lt_s", "i64x2.gt_s", "i64x2.le_s",
    "i64x2.ge_s", "i64x2.extmul_low_i32x4_s", "i64x2.extmul_high_i32x4_s", "i64x2.extmul_low_i32x4_u",
    "i64x2.extmul_high_i32x4_u",
    // 0xE0
    "f32x4.abs", "f32x4.neg", "", "f32x4.sqrt", "f32x4.add", "f32x4.sub", "f32x4.mul", "f32x4.div", "f32x4.min",
    "f32x4.max", "f32x4.pmin", "f32x4.pmax", "f64x2.abs", "f64x2.neg", "", "f64x2.sqrt",
    // 0xF0
    "f64x2.add", "f64x2.sub", "f64x2.mul", "f64x2.div", "f64x2.min", "f64x2.max", "f64x2.pmin", "f64x2.pmax",
    "i32x4.trunc_sat_f32x4_s", "i32x4.trunc_sat_f32x4_u", "f32x4.convert_i32x4_s", "f32x4.convert_i32x4_u",
    "i32x4.trunc_sat_f64x2_s_zero", "i32x4.trunc_sat_f64x2_u_zero", "f64x2.convert_low_i32x4_s",
    "f64x2.convert_low_i32x4_u",
    // 0x100: relaxed SIMD
    "i8x16.relaxed_swizzle", "i32x4.relaxed_trunc_f32x4_s", "i32x4.relaxed_trunc_f32x4_u",
    "i32x4.relaxed_trunc_f64x2_s_zero", "i32x4.relaxed_trunc_f64x2_u_zero", "f32x4.relaxed_madd", "f32x4.relaxed_nmadd",
    "f64x2.relaxed_madd", "f64x2.relaxed_nmadd", "i8x16.relaxed_laneselect", "i16x8.relaxed_laneselect",
    "i32x4.relaxed_laneselect", "i64x2.relaxed_laneselect", "f32x4.relaxed_min", "f32x4.relaxed_max",
    "f64x2.relaxed_min",
    // 0x110
    "f64x2.relaxed_max", "i16x8.relaxed_q15mulr_s", "i16x8.relaxed_dot_i8x16_i7x16_s",
    "i32x4.relaxed_dot_i8x16_i7x16_add_s",
];

/// The atomic read-modify-write instructions, `0xFE 0x1E..=0x4E`: seven
/// operations, each in seven widths.
#[rustfmt::skip]
const RMW: [&str; 49] = [
    "i32.atomic.rmw.add", "i64.atomic.rmw.add", "i32.atomic.rmw8.add_u", "i32.atomic.rmw16.add_u",
    "i64.atomic.rmw8.add_u", "i64.atomic.rmw16.add_u", "i64.atomic.rmw32.add_u",
    "i32.atomic.rmw.sub", "i64.atomic.rmw.sub", "i32.atomic.rmw8.sub_u", "i32.atomic.rmw16.sub_u",
    "i64.atomic.rmw8.sub_u", "i64.atomic.rmw16.sub_u", "i64.atomic.rmw32.sub_u",
    "i32.atomic.rmw.and", "i64.atomic.rmw.and", "i32.atomic.rmw8.and_u", "i32.atomic.rmw16.and_u",
    "i64.atomic.rmw8.and_u", "i64.atomic.rmw16.and_u", "i64.atomic.rmw32.and_u",
    "i32.atomic.rmw.or", "i64.atomic.rmw.or", "i32.atomic.rmw8.or_u", "i32.atomic.rmw16.or_u",
    "i64.atomic.rmw8.or_u", "i64.atomic.rmw16.or_u", "i64.atomic.rmw32.or_u",
    "i32.atomic.rmw.xor", "i64.atomic.rmw.xor", "i32.atomic.rmw8.xor_u", "i32.atomic.rmw16.xor_u",
    "i64.atomic.rmw8.xor_u", "i64.atomic.rmw16.xor_u", "i64.atomic.rmw32.xor_u",
    "i32.atomic.rmw.xchg", "i64.atomic.rmw.xchg", "i32.atomic.rmw8.xchg_u", "i32.atomic.rmw16.xchg_u",
    "i64.atomic.rmw8.xchg_u", "i64.atomic.rmw16.xchg_u", "i64.atomic.rmw32.xchg_u",
    "i32.atomic.rmw.cmpxchg", "i64.atomic.rmw.cmpxchg", "i32.atomic.rmw8.cmpxchg_u", "i32.atomic.rmw16.cmpxchg_u",
    "i64.atomic.rmw8.cmpxchg_u", "i64.atomic.rmw16.cmpxchg_u", "i64.atomic.rmw32.cmpxchg_u",
];

/// Atomic loads and stores, `0xFE 0x10..=0x1D`.
#[rustfmt::skip]
const ATOMIC: [&str; 14] = [
    "i32.atomic.load", "i64.atomic.load", "i32.atomic.load8_u", "i32.atomic.load16_u", "i64.atomic.load8_u",
    "i64.atomic.load16_u", "i64.atomic.load32_u", "i32.atomic.store", "i64.atomic.store", "i32.atomic.store8",
    "i32.atomic.store16", "i64.atomic.store8", "i64.atomic.store16", "i64.atomic.store32",
];

/// Access widths in bytes of the atomic loads and stores, and of the read-modify-write widths.
const ATOMIC_WIDTH: [u8; 14] = [4, 8, 1, 2, 1, 2, 4, 4, 8, 1, 2, 1, 2, 4];
const RMW_WIDTH: [u8; 7] = [4, 8, 1, 2, 1, 2, 4];

/// A one-byte opcode's name and immediates.
fn single(b: u8) -> Option<(&'static str, K)> {
    Some(match b {
        0x00 => ("unreachable", K::None),
        0x01 => ("nop", K::None),
        0x02 => ("block", K::Block),
        0x03 => ("loop", K::Block),
        0x04 => ("if", K::Block),
        0x05 => ("else", K::None),
        0x06 => ("try", K::Block),
        0x07 => ("catch", K::Tag),
        0x08 => ("throw", K::Tag),
        0x09 => ("rethrow", K::Label),
        0x0A => ("throw_ref", K::None),
        0x0B => ("end", K::None),
        0x0C => ("br", K::Label),
        0x0D => ("br_if", K::Label),
        0x0E => ("br_table", K::BrTable),
        0x0F => ("return", K::None),
        0x10 => ("call", K::Func),
        0x11 => ("call_indirect", K::Indirect),
        0x12 => ("return_call", K::Func),
        0x13 => ("return_call_indirect", K::Indirect),
        0x14 => ("call_ref", K::Type),
        0x15 => ("return_call_ref", K::Type),
        0x18 => ("delegate", K::Label),
        0x19 => ("catch_all", K::None),
        0x1A => ("drop", K::None),
        0x1B => ("select", K::None),
        0x1C => ("select", K::SelectT),
        0x1F => ("try_table", K::TryTable),
        0x20 => ("local.get", K::Local),
        0x21 => ("local.set", K::Local),
        0x22 => ("local.tee", K::Local),
        0x23 => ("global.get", K::Global),
        0x24 => ("global.set", K::Global),
        0x25 => ("table.get", K::Table),
        0x26 => ("table.set", K::Table),
        0x28..=0x3E => (MEMORY[(b - 0x28) as usize].0, K::Mem),
        0x3F => ("memory.size", K::Memory),
        0x40 => ("memory.grow", K::Memory),
        0x41 => ("i32.const", K::I32),
        0x42 => ("i64.const", K::I64),
        0x43 => ("f32.const", K::F32),
        0x44 => ("f64.const", K::F64),
        0x45..=0xC4 => (NUMERIC[(b - 0x45) as usize], K::None),
        0xD0 => ("ref.null", K::Heap),
        0xD1 => ("ref.is_null", K::None),
        0xD2 => ("ref.func", K::Func),
        0xD3 => ("ref.eq", K::None),
        0xD4 => ("ref.as_non_null", K::None),
        0xD5 => ("br_on_null", K::Label),
        0xD6 => ("br_on_non_null", K::Label),
        _ => return None,
    })
}

/// A prefixed opcode's name and immediates.
fn prefixed(prefix: u8, n: u32) -> Option<(&'static str, K)> {
    let n = n as usize;
    Some(match prefix {
        0xFC => match n {
            0 => ("i32.trunc_sat_f32_s", K::None),
            1 => ("i32.trunc_sat_f32_u", K::None),
            2 => ("i32.trunc_sat_f64_s", K::None),
            3 => ("i32.trunc_sat_f64_u", K::None),
            4 => ("i64.trunc_sat_f32_s", K::None),
            5 => ("i64.trunc_sat_f32_u", K::None),
            6 => ("i64.trunc_sat_f64_s", K::None),
            7 => ("i64.trunc_sat_f64_u", K::None),
            8 => ("memory.init", K::Two),
            9 => ("data.drop", K::One),
            10 => ("memory.copy", K::Two),
            11 => ("memory.fill", K::One),
            12 => ("table.init", K::Two),
            13 => ("elem.drop", K::One),
            14 => ("table.copy", K::Two),
            15 => ("table.grow", K::Table),
            16 => ("table.size", K::Table),
            17 => ("table.fill", K::Table),
            _ => return None,
        },
        0xFD => {
            let name = *SIMD.get(n).filter(|s| !s.is_empty())?;
            let kind = match n {
                0x00..=0x0B | 0x5C | 0x5D => K::Mem,
                0x0C => K::V128,
                0x0D => K::Shuffle,
                0x15..=0x22 => K::Lane,
                0x54..=0x5B => K::MemLane,
                _ => K::None,
            };
            (name, kind)
        }
        0xFE => match n {
            0x00 => ("memory.atomic.notify", K::Mem),
            0x01 => ("memory.atomic.wait32", K::Mem),
            0x02 => ("memory.atomic.wait64", K::Mem),
            0x03 => ("atomic.fence", K::Byte),
            0x10..=0x1D => (ATOMIC[n - 0x10], K::Mem),
            0x1E..=0x4E => (RMW[n - 0x1E], K::Mem),
            _ => return None,
        },
        _ => return None,
    })
}

fn block_type(r: &mut Reader) -> Option<BlockType> {
    let b = r.peek()?;
    if b == 0x40 {
        r.u8()?;
        Some(BlockType::Empty)
    } else if ValType::starts(b) {
        ValType::read(r).map(BlockType::Value)
    } else {
        let v = r.sleb(33)?;
        u32::try_from(v).ok().map(BlockType::Type)
    }
}

fn memarg(r: &mut Reader) -> Option<MemArg> {
    let flags = r.u32()?;
    // Bit 6: a memory index follows (the multi-memory proposal).
    let memory = if flags & 0x40 != 0 { r.u32()? } else { 0 };
    Some(MemArg {
        align: flags & !0x40,
        offset: r.u64()?,
        memory,
    })
}

fn immediates(k: K, r: &mut Reader) -> Option<Imm> {
    Some(match k {
        K::None => Imm::None,
        K::Block => Imm::Block(block_type(r)?),
        K::Label => Imm::Label(r.u32()?),
        K::BrTable => {
            let n = r.count()?;
            let mut labels = Vec::with_capacity(n as usize + 1);
            for _ in 0..=n {
                labels.push(r.u32()?);
            }
            Imm::Labels(labels)
        }
        K::Func => Imm::Func(r.u32()?),
        K::Indirect => Imm::Indirect {
            ty: r.u32()?,
            table: r.u32()?,
        },
        K::Local => Imm::Local(r.u32()?),
        K::Global => Imm::Global(r.u32()?),
        K::Table => Imm::Table(r.u32()?),
        K::Mem => Imm::Mem(memarg(r)?),
        K::Memory => Imm::Memory(r.u32()?),
        K::I32 => Imm::I32(r.sleb(32)? as i32),
        K::I64 => Imm::I64(r.sleb(64)?),
        K::F32 => Imm::F32(u32::from_le_bytes(r.fixed()?)),
        K::F64 => Imm::F64(u64::from_le_bytes(r.fixed()?)),
        K::SelectT => {
            let mut types = Vec::new();
            for _ in 0..r.count()? {
                types.push(ValType::read(r)?);
            }
            Imm::Types(types)
        }
        K::Heap => Imm::Heap(HeapType::read(r)?),
        K::TryTable => {
            let ty = block_type(r)?;
            let mut catches = Vec::new();
            for _ in 0..r.count()? {
                let kind = r.u8()?;
                let tag = if kind < 2 { Some(r.u32()?) } else { None };
                catches.push((kind, tag, r.u32()?));
            }
            Imm::TryTable(ty, catches)
        }
        K::Tag => Imm::Tag(r.u32()?),
        K::Type => Imm::Type(r.u32()?),
        K::Two => Imm::Two(r.u32()?, r.u32()?),
        K::One => Imm::Index(r.u32()?),
        K::V128 => Imm::V128(r.fixed()?),
        K::Shuffle => Imm::Lanes(r.fixed()?),
        K::Lane => Imm::Lane(r.u8()?),
        K::MemLane => {
            let m = memarg(r)?;
            Imm::MemLane(m, r.u8()?)
        }
        K::Byte => {
            r.u8()?;
            Imm::None
        }
    })
}

/// Decodes the instruction at the reader's position. `None` when the bytes
/// end inside it; an instruction with `known` false for an opcode binviz
/// doesn't know.
pub(crate) fn decode(r: &mut Reader) -> Option<Insn> {
    let start = r.pos();
    let b = r.u8()?;
    let (op, found) = match b {
        0xFB..=0xFE => {
            let n = r.u32()?;
            (u32::from(b) << 16 | n, prefixed(b, n))
        }
        _ => (u32::from(b), single(b)),
    };
    let Some((name, kind)) = found else {
        return Some(Insn {
            op,
            name: "",
            len: (r.pos() - start) as u32,
            imm: Imm::None,
            known: false,
        });
    };
    let imm = immediates(kind, r)?;
    Some(Insn {
        op,
        name,
        len: (r.pos() - start) as u32,
        imm,
        known: true,
    })
}

/// A float as the text format writes it: `2.5`, `-inf`, `nan:0x400000`.
fn float(v: f64, nan_bits: u64, canonical: u64) -> String {
    if v.is_nan() {
        let sign = if v.is_sign_negative() { "-" } else { "" };
        return if nan_bits == canonical {
            format!("{sign}nan")
        } else {
            format!("{sign}nan:{nan_bits:#x}")
        };
    }
    if v.is_infinite() {
        return if v < 0.0 { "-inf".into() } else { "inf".into() };
    }
    format!("{v:?}")
}

impl Insn {
    /// Whether the instruction opens a block (and a label to branch to).
    pub fn opens(&self) -> bool {
        matches!(self.op, BLOCK | LOOP | IF | TRY | TRY_TABLE)
    }

    /// The width in bytes of the memory it accesses, for a load or store.
    fn access_width(&self) -> Option<u8> {
        match self.op {
            0x28..=0x3E => Some(MEMORY[(self.op - 0x28) as usize].1),
            0xFE_0010..=0xFE_001D => Some(ATOMIC_WIDTH[(self.op - 0xFE_0010) as usize]),
            0xFE_001E..=0xFE_004E => Some(RMW_WIDTH[((self.op - 0xFE_001E) % 7) as usize]),
            0xFE_0000 | 0xFE_0001 => Some(4),
            0xFE_0002 => Some(8),
            0xFD_0000..=0xFD_000B => Some(match self.op & 0xFFFF {
                7 => 1,
                8 => 2,
                9 => 4,
                1..=6 | 10 => 8,
                _ => 16,
            }),
            0xFD_0054 | 0xFD_0058 => Some(1),
            0xFD_0055 | 0xFD_0059 => Some(2),
            0xFD_0056 | 0xFD_005A | 0xFD_005C => Some(4),
            0xFD_0057 | 0xFD_005B | 0xFD_005D => Some(8),
            _ => None,
        }
    }

    /// Whether it reads or writes memory: `Some(true)` for a store.
    pub fn stores(&self) -> Option<bool> {
        match self.op {
            0x28..=0x35 | 0xFD_0000..=0xFD_000A | 0xFD_0054..=0xFD_0057 | 0xFD_005C | 0xFD_005D => Some(false),
            0xFE_0010..=0xFE_0016 => Some(false),
            0x36..=0x3E | 0xFD_000B | 0xFD_0058..=0xFD_005B | 0xFE_0017..=0xFE_004E => Some(true),
            _ => None,
        }
    }

    /// The memory access's immediates, for a load, a store or an atomic.
    pub fn memarg(&self) -> Option<MemArg> {
        match self.imm {
            Imm::Mem(m) | Imm::MemLane(m, _) => Some(m),
            _ => None,
        }
    }

    /// The immediates as the text format writes them (indices as numbers).
    pub fn operands(&self) -> String {
        match &self.imm {
            Imm::None => String::new(),
            Imm::Block(t) => block_text(t),
            Imm::Label(n)
            | Imm::Func(n)
            | Imm::Local(n)
            | Imm::Global(n)
            | Imm::Tag(n)
            | Imm::Type(n)
            | Imm::Index(n) => n.to_string(),
            Imm::Table(n) => n.to_string(),
            Imm::Memory(n) => {
                if *n == 0 {
                    String::new()
                } else {
                    n.to_string()
                }
            }
            Imm::Labels(l) => {
                const SHOWN: usize = 32;
                let (targets, default) = l.split_at(l.len().saturating_sub(1));
                let mut s: Vec<String> = targets.iter().take(SHOWN).map(u32::to_string).collect();
                if targets.len() > SHOWN {
                    s.push(format!("… ({} more)", targets.len() - SHOWN));
                }
                s.extend(default.iter().map(u32::to_string));
                s.join(" ")
            }
            Imm::Indirect { ty, table } => {
                if *table == 0 {
                    format!("(type {ty})")
                } else {
                    format!("{table} (type {ty})")
                }
            }
            Imm::Mem(m) => self.memarg_text(m),
            Imm::MemLane(m, lane) => {
                let t = self.memarg_text(m);
                if t.is_empty() {
                    lane.to_string()
                } else {
                    format!("{t} {lane}")
                }
            }
            Imm::I32(v) => v.to_string(),
            Imm::I64(v) => v.to_string(),
            Imm::F32(bits) => float(
                f64::from(f32::from_bits(*bits)),
                u64::from(*bits & 0x7F_FFFF),
                0x40_0000,
            ),
            Imm::F64(bits) => float(f64::from_bits(*bits), bits & 0xF_FFFF_FFFF_FFFF, 1 << 51),
            Imm::Types(t) => format!(
                "(result {})",
                t.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")
            ),
            Imm::Heap(h) => h.to_string(),
            Imm::TryTable(t, catches) => {
                let mut s = block_text(t);
                for (kind, tag, label) in catches {
                    let clause = match (kind, tag) {
                        (0, Some(t)) => format!("(catch {t} {label})"),
                        (1, Some(t)) => format!("(catch_ref {t} {label})"),
                        (2, _) => format!("(catch_all {label})"),
                        _ => format!("(catch_all_ref {label})"),
                    };
                    if !s.is_empty() {
                        s.push(' ');
                    }
                    s.push_str(&clause);
                }
                s
            }
            Imm::Two(a, b) => format!("{a} {b}"),
            Imm::V128(b) => {
                let words: Vec<String> = b
                    .chunks(4)
                    .map(|w| format!("{:#010x}", u32::from_le_bytes([w[0], w[1], w[2], w[3]])))
                    .collect();
                format!("i32x4 {}", words.join(" "))
            }
            Imm::Lanes(b) => b.iter().map(u8::to_string).collect::<Vec<_>>().join(" "),
            Imm::Lane(l) => l.to_string(),
        }
    }

    /// `offset=1072`, with `align=` when it isn't the access's natural
    /// alignment and `memory=` when it isn't memory 0.
    fn memarg_text(&self, m: &MemArg) -> String {
        let mut parts = Vec::new();
        if m.memory != 0 {
            parts.push(format!("memory={}", m.memory));
        }
        if m.offset != 0 {
            parts.push(format!("offset={}", m.offset));
        }
        let natural = self.access_width().map(|w| w.trailing_zeros());
        if natural != Some(m.align) {
            parts.push(format!("align={}", 1u64 << m.align.min(63)));
        }
        parts.join(" ")
    }
}

fn block_text(t: &BlockType) -> String {
    match t {
        BlockType::Empty => String::new(),
        BlockType::Value(v) => format!("(result {v})"),
        BlockType::Type(i) => format!("(type {i})"),
    }
}

/// A function body decoded whole.
pub(crate) struct Decoded {
    /// The locals declarations: (how many, of which type); `None` when they can't be read.
    pub locals: Option<Vec<(u32, ValType)>>,
    /// Each instruction, at its file offset.
    pub insns: Vec<(u64, Insn)>,
    /// Per instruction, where it branches to: the `end` of the block a
    /// branch leaves, or the `loop` it repeats; for `if`, its `else` branch
    /// (or its `end`); for `else`, the `end`.
    pub targets: Vec<Option<u64>>,
    /// Where decoding stopped before the body's end: an opcode binviz
    /// doesn't know, or an instruction cut short.
    pub stopped: Option<u64>,
}

impl Decoded {
    /// Whether instruction `i` is the `end` that closes the function.
    pub fn is_last_end(&self, i: usize) -> bool {
        self.insns[i].1.op == END && i + 1 == self.insns.len() && self.stopped.is_none()
    }
}

/// Decodes a function body: its locals, its instructions and where each branch goes.
pub(crate) fn decode_body(data: &[u8], body: Body) -> Decoded {
    let mut r = Reader::new(data, body.start, body.end);
    let locals = read_locals(&mut r);
    let mut r = Reader::new(data, body.code, body.end);
    let mut insns = Vec::new();
    let mut stopped = None;
    while !r.is_empty() {
        let at = r.pos();
        match decode(&mut r) {
            Some(i) if i.known => insns.push((at, i)),
            _ => {
                stopped = Some(at);
                break;
            }
        }
    }
    // Where each block ends (and each `if` has its `else`), then where each branch goes.
    let n = insns.len();
    let mut end_of = vec![usize::MAX; n];
    let mut else_of = vec![usize::MAX; n];
    let mut open: Vec<usize> = Vec::new();
    for (i, (_, insn)) in insns.iter().enumerate() {
        match insn.op {
            _ if insn.opens() => open.push(i),
            ELSE => {
                if let Some(&b) = open.last() {
                    else_of[b] = i;
                }
            }
            END | DELEGATE => {
                if let Some(b) = open.pop() {
                    end_of[b] = i;
                }
            }
            _ => {}
        }
    }
    let address = |i: usize| insns.get(i).map(|(a, _)| *a);
    let function_end = insns.iter().rev().find(|(_, i)| i.op == END).map(|(a, _)| *a);
    let mut targets = vec![None; n];
    let mut open: Vec<usize> = Vec::new();
    for (i, (_, insn)) in insns.iter().enumerate() {
        let label = |depth: u32, open: &[usize]| -> Option<u64> {
            let depth = depth as usize;
            match depth.cmp(&open.len()) {
                std::cmp::Ordering::Less => {
                    let b = open[open.len() - 1 - depth];
                    if insns[b].1.op == LOOP {
                        address(b)
                    } else {
                        address(end_of[b])
                    }
                }
                // The function's own block: a branch to it returns.
                std::cmp::Ordering::Equal => function_end,
                std::cmp::Ordering::Greater => None,
            }
        };
        targets[i] = match (&insn.imm, insn.op) {
            (Imm::Label(d), BR | BR_IF | BR_ON_NULL | BR_ON_NON_NULL | DELEGATE) => label(*d, &open),
            (Imm::Labels(l), BR_TABLE) => l.last().and_then(|d| label(*d, &open)),
            (_, IF) => match else_of[i] {
                usize::MAX => address(end_of[i]),
                e => address(e).map(|a| a + u64::from(insns[e].1.len)),
            },
            (_, ELSE) => open.last().and_then(|&b| address(end_of[b])),
            _ => None,
        };
        match insn.op {
            _ if insn.opens() => open.push(i),
            END | DELEGATE => {
                open.pop();
            }
            _ => {}
        }
    }
    Decoded {
        locals,
        insns,
        targets,
        stopped,
    }
}

/// Which values on the operand stack are known constants, as far as
/// straight-line code shows. That is enough to see the address a load or
/// store uses when the code gives it outright: `i32.const 0` then
/// `i32.load offset=1072`, the usual way to reach a global variable. Any
/// branch, block boundary or instruction whose effect isn't modelled here
/// forgets everything.
#[derive(Default)]
pub(crate) struct Stack {
    values: Vec<Option<u64>>,
}

impl Stack {
    fn pop(&mut self) -> Option<u64> {
        self.values.pop().flatten()
    }

    fn pops(&mut self, n: usize) {
        let keep = self.values.len().saturating_sub(n);
        self.values.truncate(keep);
    }

    fn push(&mut self, v: Option<u64>) {
        self.values.push(v);
    }

    fn pushes(&mut self, n: usize) {
        self.values.extend(std::iter::repeat_n(None, n));
    }

    /// The value `depth` below the top of the stack (0: the top), if known.
    pub fn peek(&self, depth: usize) -> Option<u64> {
        let i = self.values.len().checked_sub(depth + 1)?;
        self.values[i]
    }

    /// Steps over an instruction. For a load or store whose address is
    /// known, returns the linear memory address it accesses.
    pub fn step(&mut self, m: &Module, insn: &Insn) -> Option<u64> {
        let op = insn.op;
        let unary = |op: u32| {
            matches!(op, 0x45 | 0x50 | 0x67..=0x69 | 0x79..=0x7B | 0x8B..=0x91 | 0x99..=0x9F | 0xA7..=0xC4)
                || (0xFC_0000..=0xFC_0007).contains(&op)
        };
        let mut access = None;
        match op {
            I32_CONST => {
                let Imm::I32(v) = insn.imm else { return None };
                self.push(Some(u64::from(v as u32)));
            }
            I64_CONST => {
                let Imm::I64(v) = insn.imm else { return None };
                self.push(Some(v as u64));
            }
            F32_CONST | F64_CONST | V128_CONST | LOCAL_GET | GLOBAL_GET | REF_NULL | REF_FUNC | 0x3F => {
                self.push(None);
            }
            LOCAL_SET | GLOBAL_SET | DROP => self.pops(1),
            LOCAL_TEE | MEMORY_GROW | 0xD1 | 0xD4 => {
                self.pops(1);
                self.push(None);
            }
            SELECT | SELECT_T => {
                self.pops(3);
                self.push(None);
            }
            I32_ADD => {
                let (b, a) = (self.pop(), self.pop());
                self.push(a.zip(b).map(|(a, b)| u64::from((a as u32).wrapping_add(b as u32))));
            }
            _ if unary(op) => {
                self.pops(1);
                self.push(None);
            }
            0x46..=0xA6 => {
                self.pops(2);
                self.push(None);
            }
            CALL | RETURN_CALL | CALL_INDIRECT | RETURN_CALL_INDIRECT => {
                let ty = match insn.imm {
                    Imm::Func(f) => m.func_type(f),
                    Imm::Indirect { ty, .. } => m.types.get(ty as usize).and_then(Option::as_ref),
                    _ => None,
                };
                match ty {
                    Some(t) => {
                        self.pops(t.params.len() + usize::from(matches!(insn.imm, Imm::Indirect { .. })));
                        self.pushes(t.results.len());
                    }
                    None => self.values.clear(),
                }
            }
            // memory.init, memory.copy, memory.fill.
            0xFC_0008 | 0xFC_000A | 0xFC_000B => self.pops(3),
            0xFC_0009 => {}
            _ => match insn.stores() {
                Some(store) => {
                    let lanes = matches!(insn.imm, Imm::MemLane(..));
                    let rmw = (0xFE_001E..=0xFE_004E).contains(&op);
                    let cmpxchg = (0xFE_0048..=0xFE_004E).contains(&op);
                    // The operands above the address: a store's value, an rmw's operand(s), a lane's vector.
                    let above = match (store, rmw, cmpxchg, lanes) {
                        (_, _, true, _) => 2,
                        (true, _, _, _) | (_, _, _, true) => 1,
                        _ => 0,
                    };
                    self.pops(above);
                    let base = self.pop();
                    access = base.zip(insn.memarg()).map(|(b, a)| b.wrapping_add(a.offset));
                    if !store || rmw {
                        self.push(None);
                    }
                }
                None if matches!(op, 0xFE_0000..=0xFE_0002) => {
                    // notify: address and count; wait: address, expected value and timeout.
                    self.pops(if op == 0xFE_0000 { 1 } else { 2 });
                    let base = self.pop();
                    access = base.zip(insn.memarg()).map(|(b, a)| b.wrapping_add(a.offset));
                    self.push(None);
                }
                None => self.values.clear(),
            },
        }
        access
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(bytes: &[u8]) -> Insn {
        let mut r = Reader::new(bytes, 0, bytes.len() as u64);
        let i = decode(&mut r).expect("decodes");
        assert_eq!(i.len as usize, bytes.len(), "{} takes all of {bytes:02x?}", i.name);
        i
    }

    #[test]
    fn instructions() {
        let text = |b: &[u8]| {
            let i = one(b);
            format!("{} {}", i.name, i.operands()).trim_end().to_string()
        };
        assert_eq!(text(&[0x41, 0x7f]), "i32.const -1");
        assert_eq!(text(&[0x42, 0x80, 0x80, 0x80, 0x80, 0x10]), "i64.const 4294967296");
        assert_eq!(text(&[0x43, 0, 0, 0x20, 0x40]), "f32.const 2.5");
        assert_eq!(text(&[0x44, 0, 0, 0, 0, 0, 0, 0xf0, 0xff]), "f64.const -inf");
        assert_eq!(text(&[0x43, 0, 0, 0xc0, 0x7f]), "f32.const nan");
        // Natural alignment is left out, an offset shown; a padded LEB128 read whole.
        assert_eq!(
            text(&[0x28, 0x02, 0xb0, 0x88, 0x80, 0x80, 0x00]),
            "i32.load offset=1072"
        );
        assert_eq!(text(&[0x36, 0x00, 0x04]), "i32.store offset=4 align=1");
        assert_eq!(text(&[0x10, 0x80, 0x80, 0x80, 0x80, 0x00]), "call 0");
        assert_eq!(text(&[0x11, 0x03, 0x00]), "call_indirect (type 3)");
        assert_eq!(text(&[0x0e, 0x02, 0x00, 0x01, 0x02]), "br_table 0 1 2");
        assert_eq!(text(&[0x02, 0x7f]), "block (result i32)");
        assert_eq!(text(&[0x02, 0x40]), "block");
        assert_eq!(text(&[0x04, 0x05]), "if (type 5)");
        assert_eq!(text(&[0xc0]), "i32.extend8_s");
        assert_eq!(text(&[0xfc, 0x0a, 0x00, 0x00]), "memory.copy 0 0");
        assert_eq!(text(&[0xfc, 0x0b, 0x00]), "memory.fill 0");
        assert_eq!(text(&[0xfc, 0x00]), "i32.trunc_sat_f32_s");
        assert_eq!(
            text(&[0xfd, 0x0c, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0]),
            "v128.const i32x4 0x00000001 0x00000002 0x00000003 0x00000004"
        );
        assert_eq!(text(&[0xfd, 0x15, 0x03]), "i8x16.extract_lane_s 3");
        assert_eq!(text(&[0xfd, 0xae, 0x01]), "i32x4.add");
        assert_eq!(text(&[0xfd, 0x54, 0x00, 0x00, 0x07]), "v128.load8_lane 7");
        assert_eq!(text(&[0xfe, 0x1e, 0x02, 0x08]), "i32.atomic.rmw.add offset=8");
        assert_eq!(text(&[0xfe, 0x03, 0x00]), "atomic.fence");
        assert_eq!(text(&[0xd0, 0x70]), "ref.null func");
        assert_eq!(text(&[0x1c, 0x01, 0x7f]), "select (result i32)");
        assert_eq!(text(&[0x3f, 0x00]), "memory.size");
        // Unknown: the GC group, a reserved SIMD opcode, a free byte.
        let mut r = Reader::new(&[0xfb, 0x00, 0x01][..], 0, 3);
        assert!(!decode(&mut r).unwrap().known);
        let mut r = Reader::new(&[0xfd, 0x9a, 0x01][..], 0, 3);
        assert!(!decode(&mut r).unwrap().known);
        let mut r = Reader::new(&[0x27][..], 0, 1);
        assert!(!decode(&mut r).unwrap().known);
        // Cut short.
        let mut r = Reader::new(&[0x41][..], 0, 1);
        assert!(decode(&mut r).is_none());
    }

    #[test]
    fn branch_targets() {
        // block; local.get 0; br_if 0; loop; br 1; br 0; end; end; end
        let code = [
            0x00, 0x02, 0x40, 0x20, 0x00, 0x0d, 0x00, 0x03, 0x40, 0x0c, 0x01, 0x0c, 0x00, 0x0b, 0x0b, 0x0b,
        ];
        let body = Body {
            entry: 0,
            start: 0,
            code: 1,
            end: code.len() as u64,
        };
        let d = decode_body(&code, body);
        assert!(d.stopped.is_none());
        let at = |i: usize| d.insns[i].0;
        let names: Vec<&str> = d.insns.iter().map(|(_, i)| i.name).collect();
        assert_eq!(
            names,
            ["block", "local.get", "br_if", "loop", "br", "br", "end", "end", "end"]
        );
        // br_if 0 leaves the block: to its end.
        assert_eq!(d.targets[2], Some(at(7)));
        // br 1 inside the loop leaves the block too; br 0 repeats the loop.
        assert_eq!(d.targets[4], Some(at(7)));
        assert_eq!(d.targets[5], Some(at(3)));
        assert!(d.is_last_end(8));
    }
}
