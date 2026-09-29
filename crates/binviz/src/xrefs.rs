//! Cross-references and the call graph: who calls a function, what it calls,
//! which code reads a string or writes a global, and which data points at what.
//!
//! The index is built once, on first use:
//!
//! - AArch64: one pass over the instruction words — `bl` (calls), `b` to
//!   another function (tail calls), `adrp` + `add` / `ldr` / `str` pairs, `adr`
//!   and literal loads. No full disassembly is needed.
//! - x86 / x86-64: the code is decoded (iced-x86), restarting at every function
//!   start: direct calls and jumps, calls and jumps through pointer slots (the
//!   IAT, the GOT), RIP-relative and absolute memory operands, and immediates
//!   that are addresses.
//! - Data: pointers stored in data (see [`crate::pointers`]).
//!
//! References are packed into 8 bytes each, one sorted list per kind, so "who
//! refers to X" is a binary search. What a function refers to is found by
//! scanning that function again.

use std::collections::{HashMap, HashSet, VecDeque};

use iced_x86::{FlowControl, InstructionInfoFactory, Mnemonic, OpAccess, OpKind, Register};
use object::Architecture;
use serde::Serialize;

use crate::binary::Binary;
use crate::model::{RegionKind, SymbolKind, SymbolSource};
use crate::pointers::section_bytes;
use crate::symbols::SymbolTable;

/// How one address refers to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefKind {
    /// A direct call (`call`, `bl`), or a call through a pointer slot.
    Call,
    /// A tail call: a jump to another function, or through a pointer slot.
    Jump,
    /// Code loading data from the address.
    Read,
    /// Code storing to the address.
    Write,
    /// Code taking the address: `lea`, `adrp` + `add`, `adr`, an immediate.
    Address,
    /// Data holding a pointer to the address.
    Pointer,
}

const KINDS: [RefKind; 6] = [
    RefKind::Call,
    RefKind::Jump,
    RefKind::Read,
    RefKind::Write,
    RefKind::Address,
    RefKind::Pointer,
];

impl RefKind {
    pub fn is_call(self) -> bool {
        matches!(self, RefKind::Call | RefKind::Jump)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RefKind::Call => "call",
            RefKind::Jump => "jump",
            RefKind::Read => "read",
            RefKind::Write => "write",
            RefKind::Address => "address",
            RefKind::Pointer => "pointer",
        }
    }
}

pub(crate) struct XrefIndex {
    /// The lowest loaded address; everything is stored relative to it.
    base: u64,
    /// Per kind: `(target - base) << 32 | (source - base)`, sorted.
    lists: [Vec<u64>; 6],
}

impl XrefIndex {
    /// References of one kind to targets in `lo..hi`.
    fn range(&self, kind: RefKind, lo: u64, hi: u64) -> &[u64] {
        let list = &self.lists[kind as usize];
        let (a, b) = (lo.saturating_sub(self.base), hi.saturating_sub(self.base));
        let a = list.partition_point(|&v| (v >> 32) < a);
        let b = list.partition_point(|&v| (v >> 32) < b);
        &list[a..b.max(a)]
    }

    fn source(&self, v: u64) -> u64 {
        self.base + (v & 0xFFFF_FFFF)
    }

    fn target(&self, v: u64) -> u64 {
        self.base + (v >> 32)
    }

    /// The lowest referenced address above `address`.
    fn next_target(&self, address: u64) -> Option<u64> {
        let a = address.checked_sub(self.base)?;
        self.lists
            .iter()
            .filter_map(|list| {
                let i = list.partition_point(|&v| (v >> 32) <= a);
                list.get(i).map(|&v| self.target(v))
            })
            .min()
    }

    fn contains(&self, kind: RefKind, source: u64, target: u64) -> bool {
        let (Some(s), Some(t)) = (source.checked_sub(self.base), target.checked_sub(self.base)) else {
            return false;
        };
        s <= u32::MAX as u64 && t <= u32::MAX as u64 && self.lists[kind as usize].binary_search(&(t << 32 | s)).is_ok()
    }
}

/// Counts of references by kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RefCounts {
    pub call: u32,
    pub jump: u32,
    pub read: u32,
    pub write: u32,
    pub address: u32,
    pub pointer: u32,
}

impl RefCounts {
    fn add(&mut self, kind: RefKind, n: u32) {
        let slot = match kind {
            RefKind::Call => &mut self.call,
            RefKind::Jump => &mut self.jump,
            RefKind::Read => &mut self.read,
            RefKind::Write => &mut self.write,
            RefKind::Address => &mut self.address,
            RefKind::Pointer => &mut self.pointer,
        };
        *slot += n;
    }

    pub fn total(&self) -> u32 {
        self.call + self.jump + self.read + self.write + self.address + self.pointer
    }
}

/// What kind of thing a call graph node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    Function,
    /// An imported function: a stub, a PLT entry, an IAT or GOT slot.
    Import,
    /// Code not at the start of any known function.
    Code,
    Data,
}

/// One reference, with names for display.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub source: u64,
    pub target: u64,
    pub kind: RefKind,
    /// Start of the function containing the source.
    pub function: Option<u64>,
    /// Where the source is: `main+0x1c`, `vtable for Foo+0x10`, `__data+0x40`.
    pub from: Option<String>,
    /// What the target is: a string, `symbol+offset`, or `-> what a pointer there points to`.
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefPage {
    pub total: u32,
    pub offset: u32,
    pub counts: RefCounts,
    pub refs: Vec<Reference>,
}

/// A caller or callee, with how many call sites connect it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallEdge {
    /// The function's start (or, for code outside any function, the call site).
    pub address: u64,
    pub name: String,
    pub kind: NodeKind,
    pub calls: u32,
    /// The first call site.
    pub site: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub address: u64,
    pub name: String,
    pub kind: NodeKind,
    pub source: Option<SymbolSource>,
    pub size: u64,
    /// 0 for the centre, negative for callers, positive for callees.
    pub depth: i32,
    /// Callers and callees, when this node was expanded (the centre always is).
    pub callers: Option<u32>,
    pub callees: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub from: u64,
    pub to: u64,
    pub calls: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallGraph {
    pub center: u64,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// Neighbours left out to keep the graph readable.
    pub hidden: u32,
}

/// One step of a call path.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathStep {
    pub address: u64,
    pub name: String,
    /// Where the previous step calls this one.
    pub site: Option<u64>,
}

/// A string a function refers to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StringUse {
    pub address: u64,
    pub text: String,
    /// The instruction that refers to it.
    pub site: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSummary {
    pub address: u64,
    pub name: String,
    pub size: u64,
    pub caller_count: u32,
    pub callers: Vec<CallEdge>,
    pub callee_count: u32,
    pub callees: Vec<CallEdge>,
    pub strings: Vec<StringUse>,
    /// Other data it reads, writes or takes the address of (one per target).
    pub data: Vec<Reference>,
    /// References to the function, by kind: calls, tail calls, pointers to it
    /// in data (vtables, callbacks), code taking its address.
    pub referenced_by: RefCounts,
}

/// Sorted loaded ranges: (start, end, code).
struct AddressMap {
    ranges: Vec<(u64, u64, bool)>,
}

impl AddressMap {
    fn new(bin: &Binary) -> AddressMap {
        let mut ranges: Vec<(u64, u64, bool)> = bin
            .sections
            .iter()
            // .tbss overlaps the sections after it and holds no addressable data.
            .filter(|s| s.loaded && s.size > 0 && !(s.kind == RegionKind::Tls && s.file_size == 0))
            .map(|s| (s.address, s.address + s.size, s.kind == RegionKind::Code))
            .collect();
        ranges.sort_unstable();
        AddressMap { ranges }
    }

    /// Whether `a` is loaded, and if so whether it is code.
    fn get(&self, a: u64) -> Option<bool> {
        let i = self.ranges.partition_point(|r| r.0 <= a).checked_sub(1)?;
        let r = self.ranges[i];
        (a < r.1).then_some(r.2)
    }

    fn is_code(&self, a: u64) -> bool {
        self.get(a) == Some(true)
    }
}

/// What the code scanners need.
struct Scan<'a> {
    map: &'a AddressMap,
    symbols: &'a SymbolTable,
    /// The image is not loaded near zero (or is an XBE, loaded at 0x10000
    /// and never moved), so absolute numbers (immediates, displacements)
    /// that fall inside it are addresses rather than constants.
    absolute: bool,
    /// Jump tables in the code, sorted.
    tables: &'a [crate::discover::x86::Table],
}

impl Scan<'_> {
    /// Whether a jump from `pc` to `t` leaves its function for another one's start.
    fn tail_call(&self, pc: u64, t: u64) -> bool {
        self.symbols.is_static_function_start(t)
            && self
                .symbols
                .static_function_containing(pc)
                .is_none_or(|(s, e)| t < s || t >= e)
    }
}

fn sext(value: u32, bits: u32) -> i64 {
    let shift = 64 - bits;
    ((value as i64) << shift) >> shift
}

/// Scale, and whether it stores, of an AArch64 load/store with an unsigned
/// immediate offset.
pub(crate) fn a64_ldst(v: u32) -> Option<(u64, bool)> {
    if v & 0x3B00_0000 != 0x3900_0000 {
        return None;
    }
    let size = v >> 30;
    let opc = (v >> 22) & 3;
    if (v >> 26) & 1 == 1 {
        // SIMD & FP registers: opc 2 and 3 with size 0 are the 128-bit forms.
        let q = size == 0 && opc >= 2;
        Some((if q { 16 } else { 1 << size }, opc & 1 == 0))
    } else if size == 3 && opc == 2 {
        None // prfm
    } else {
        Some((1 << size, opc == 0))
    }
}

/// References in AArch64 code `bytes` loaded at `addr`.
fn scan_a64(bytes: &[u8], addr: u64, cx: &Scan, emit: &mut dyn FnMut(u64, u64, RefKind)) {
    let words = bytes.as_chunks::<4>().0;
    for (i, w) in words.iter().enumerate() {
        let w = u32::from_le_bytes(*w);
        let pc = addr + 4 * i as u64;
        match w >> 26 {
            // bl
            0b100101 => {
                let t = pc.wrapping_add_signed(sext(w & 0x03FF_FFFF, 26) * 4);
                if cx.map.is_code(t) {
                    emit(pc, t, RefKind::Call);
                }
                continue;
            }
            // b: a tail call when it lands on another function's start.
            0b000101 => {
                let t = pc.wrapping_add_signed(sext(w & 0x03FF_FFFF, 26) * 4);
                if cx.tail_call(pc, t) {
                    emit(pc, t, RefKind::Jump);
                }
                continue;
            }
            _ => {}
        }
        if w & 0x9F00_0000 == 0x9000_0000 {
            // adrp, finished by an add or a load/store off the same register.
            let rd = w & 31;
            let imm = sext(((w >> 5) & 0x7FFFF) << 2 | ((w >> 29) & 3), 21) << 12;
            let page = (pc & !0xFFF).wrapping_add_signed(imm);
            for k in 1..=4 {
                let Some(&v) = words.get(i + k) else { break };
                let v = u32::from_le_bytes(v);
                if (v >> 5) & 31 != rd {
                    continue;
                }
                if v & 0xFF80_0000 == 0x9100_0000 {
                    let shift = 12 * ((v >> 22) & 1);
                    emit(pc, page + ((((v >> 10) & 0xFFF) as u64) << shift), RefKind::Address);
                    break;
                }
                if let Some((scale, store)) = a64_ldst(v) {
                    let kind = if store { RefKind::Write } else { RefKind::Read };
                    let slot = page + ((v >> 10) & 0xFFF) as u64 * scale;
                    emit(pc, slot, kind);
                    // ldr x16, [x16, slot]; blr x16: a call through the slot.
                    let rt = v & 31;
                    if v & 0xFFC0_0000 == 0xF940_0000 {
                        for j in 1..=3 {
                            let Some(&u) = words.get(i + k + j) else { break };
                            let u = u32::from_le_bytes(u);
                            // br / blr, with or without pointer authentication
                            if u & 0xFEDF_F000 == 0xD61F_0000 && (u >> 5) & 31 == rt {
                                let kind = if (u >> 21) & 1 == 1 {
                                    RefKind::Call
                                } else {
                                    RefKind::Jump
                                };
                                emit(pc + 4 * (k + j) as u64, slot, kind);
                                break;
                            }
                        }
                    }
                    break;
                }
            }
        } else if w & 0x9F00_0000 == 0x1000_0000 {
            // adr
            let imm = sext(((w >> 5) & 0x7FFFF) << 2 | ((w >> 29) & 3), 21);
            emit(pc, pc.wrapping_add_signed(imm), RefKind::Address);
        } else if w & 0x3B00_0000 == 0x1800_0000 && !(w >> 30 == 3 && (w >> 26) & 1 == 0) {
            // ldr (literal), but not prfm
            emit(
                pc,
                pc.wrapping_add_signed(sext((w >> 5) & 0x7FFFF, 19) * 4),
                RefKind::Read,
            );
        }
    }
}

/// The address a memory operand names outright: RIP-relative, or with no base
/// register (an absolute address, possibly indexed: a table).
fn memory_address(ins: &iced_x86::Instruction, absolute: bool) -> Option<u64> {
    if ins.is_ip_rel_memory_operand() {
        return Some(ins.ip_rel_memory_address());
    }
    let has_memory = (0..ins.op_count()).any(|i| ins.op_kind(i) == OpKind::Memory);
    (absolute && has_memory && ins.memory_base() == Register::None && ins.memory_displacement64() != 0)
        .then(|| ins.memory_displacement64())
}

fn is_near_branch(ins: &iced_x86::Instruction) -> bool {
    matches!(
        ins.op0_kind(),
        OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64
    )
}

/// References in x86 code `bytes` loaded at `addr`. Decoding is linear, but
/// restarts at every function start; outside known functions (padding, jump
/// tables, leaf functions without unwind data) only calls to known functions
/// and through pointer slots are trusted.
fn scan_x86(bits: u32, bytes: &[u8], addr: u64, cx: &Scan, emit: &mut dyn FnMut(u64, u64, RefKind)) {
    let end = addr + bytes.len() as u64;
    let mut functions = cx.symbols.static_functions_in(addr, end).peekable();
    let any = functions.peek().is_some();
    let (mut cur_start, mut cur_end) = (0u64, 0u64);
    let mut decoder = iced_x86::Decoder::with_ip(bits, bytes, addr, iced_x86::DecoderOptions::NONE);
    let mut ins = iced_x86::Instruction::default();
    let mut info = InstructionInfoFactory::new();
    // Registers loaded from memory the instruction names (`mov r14, [rip+puts@got]`),
    // so that a later `call r14` is a call through that slot.
    let mut loaded = [0u64; 16];
    while decoder.can_decode() {
        let ip = decoder.ip();
        // A jump table is data: each entry points at a case.
        let t = cx
            .tables
            .partition_point(|t| t.address <= ip)
            .checked_sub(1)
            .map(|i| cx.tables[i]);
        if let Some(t) = t.filter(|t| ip < t.end()) {
            if t.entry == 4 {
                let first = ip.next_multiple_of(4).max(t.address);
                for at in (first..t.end()).step_by(4) {
                    if let Some(w) = bytes.get((at - addr) as usize..(at - addr) as usize + 4) {
                        emit(
                            at,
                            u32::from_le_bytes(w.try_into().expect("4 bytes")) as u64,
                            RefKind::Pointer,
                        );
                    }
                }
            }
            if decoder.set_position((t.end().min(end) - addr) as usize).is_err() {
                break;
            }
            decoder.set_ip(t.end().min(end));
            continue;
        }
        while let Some(&(s, e)) = functions.peek() {
            if s > ip {
                break;
            }
            (cur_start, cur_end) = (s, e);
            loaded = [0; 16];
            functions.next();
        }
        decoder.decode_out(&mut ins);
        // An instruction running into the next function was decoded out of step.
        if let Some(&(s, _)) = functions.peek()
            && ins.next_ip() > s
        {
            if decoder.set_position((s - addr) as usize).is_err() {
                break;
            }
            decoder.set_ip(s);
            continue;
        }
        if ins.is_invalid() {
            continue;
        }
        let inside = !any || (ip >= cur_start && ip < cur_end);
        let mut via_slot = false;
        let gpr = (ins.op0_kind() == OpKind::Register && ins.op0_register().is_gpr())
            .then(|| ins.op0_register().full_register().number() & 15);
        match ins.flow_control() {
            FlowControl::Call if is_near_branch(&ins) => {
                let t = ins.near_branch_target();
                if cx.map.is_code(t) && (inside || cx.symbols.is_static_function_start(t)) {
                    emit(ip, t, RefKind::Call);
                }
            }
            FlowControl::UnconditionalBranch if is_near_branch(&ins) => {
                let t = ins.near_branch_target();
                if cx.tail_call(ip, t) {
                    emit(ip, t, RefKind::Jump);
                }
            }
            FlowControl::IndirectCall | FlowControl::IndirectBranch => {
                // call [rip+slot] / jmp [slot]: through an import slot or a table.
                if let Some(slot) = memory_address(&ins, cx.absolute) {
                    let kind = if ins.flow_control() == FlowControl::IndirectCall {
                        RefKind::Call
                    } else if ins.memory_index() == Register::None {
                        RefKind::Jump
                    } else {
                        RefKind::Read // jmp [table + index*8]: a switch
                    };
                    if inside || kind != RefKind::Read {
                        emit(ip, slot, kind);
                    }
                    via_slot = true;
                } else if let Some(r) = gpr
                    && loaded[r] != 0
                {
                    let call = ins.flow_control() == FlowControl::IndirectCall;
                    emit(ip, loaded[r], if call { RefKind::Call } else { RefKind::Jump });
                }
            }
            _ => {}
        }
        // Track registers holding a value loaded from a named slot.
        let branch = matches!(
            ins.flow_control(),
            FlowControl::IndirectCall | FlowControl::IndirectBranch
        );
        if let Some(r) = gpr.filter(|_| !branch) {
            let load = ins.mnemonic() == Mnemonic::Mov && ins.op1_kind() == OpKind::Memory;
            loaded[r] = match memory_address(&ins, cx.absolute) {
                Some(slot) if load => slot,
                _ if matches!(
                    ins.mnemonic(),
                    Mnemonic::Cmp | Mnemonic::Test | Mnemonic::Push | Mnemonic::Bt
                ) =>
                {
                    loaded[r]
                }
                _ => 0,
            };
        }
        if matches!(ins.flow_control(), FlowControl::Call | FlowControl::IndirectCall) {
            // Registers a call may change.
            for r in [0, 1, 2, 6, 7, 8, 9, 10, 11] {
                loaded[r] = 0;
            }
        }
        if !inside {
            continue;
        }
        if !via_slot && let Some(a) = memory_address(&ins, cx.absolute) {
            let kind = if ins.mnemonic() == Mnemonic::Lea {
                RefKind::Address
            } else if info.info(&ins).used_memory().iter().any(|m| {
                matches!(
                    m.access(),
                    OpAccess::Write | OpAccess::CondWrite | OpAccess::ReadWrite | OpAccess::ReadCondWrite
                )
            }) {
                RefKind::Write
            } else {
                RefKind::Read
            };
            emit(ip, a, kind);
        }
        // Immediates that are addresses (`push offset string` in 32-bit code).
        if cx.absolute {
            for op in 0..ins.op_count() {
                if matches!(
                    ins.op_kind(op),
                    OpKind::Immediate32 | OpKind::Immediate32to64 | OpKind::Immediate64
                ) {
                    emit(ip, ins.immediate(op), RefKind::Address);
                }
            }
        }
    }
}

fn short_quote(text: &str, max: usize) -> String {
    let cut: String = text.chars().take(max).collect();
    let mut q = crate::util::quote(cut.as_bytes());
    if cut.len() < text.len() {
        q.insert(q.len() - 1, '…');
    }
    q
}

impl Binary {
    pub(crate) fn xref_index(&self) -> &XrefIndex {
        self.xrefs.get_or_init(|| self.build_xrefs())
    }

    /// Builds the reference index now (it is otherwise built on first use).
    pub fn prepare_xrefs(&self) {
        self.xref_index();
    }

    /// Whether the reference index has been built.
    pub fn xrefs_ready(&self) -> bool {
        self.xrefs.get().is_some()
    }

    /// Whether references can be found in this binary's code.
    pub fn xrefs_supported(&self) -> bool {
        self.rom.is_some()
            || matches!(
                self.arch,
                Architecture::Aarch64
                    | Architecture::Aarch64_Ilp32
                    | Architecture::X86_64
                    | Architecture::I386
                    | Architecture::X86_64_X32
            )
    }

    fn scan_code(&self, bytes: &[u8], addr: u64, cx: &Scan, emit: &mut dyn FnMut(u64, u64, RefKind)) {
        if let Some(rom) = &self.rom {
            let mut state = self.rom_state_at(addr);
            let tail = self.code_bytes(addr).unwrap_or(bytes);
            let mut pos = 0;
            while pos < bytes.len() {
                let pc = addr + pos as u64;
                let Some(insn) = crate::cpu::decode(rom.cpu, &tail[pos..], rom.map.cpu(pc), &mut state) else {
                    break;
                };
                if let Some((t, kind)) = insn.data
                    && let Some(a) = rom.map.resolve(pc, t)
                {
                    emit(pc, a, kind);
                }
                if let Some(t) = insn
                    .flow
                    .target()
                    .and_then(|t| rom.map.resolve(pc, crate::cpu::code_target(rom.cpu, t).0))
                {
                    match insn.flow {
                        crate::cpu::Flow::Call(_) => emit(pc, t, RefKind::Call),
                        crate::cpu::Flow::Jump(_) | crate::cpu::Flow::Branch(_) if cx.tail_call(pc, t) => {
                            emit(pc, t, RefKind::Jump)
                        }
                        _ => {}
                    }
                }
                pos += (insn.len as usize).max(1);
            }
            return;
        }
        match self.arch {
            Architecture::Aarch64 | Architecture::Aarch64_Ilp32 => scan_a64(bytes, addr, cx, emit),
            Architecture::X86_64 => scan_x86(64, bytes, addr, cx, emit),
            Architecture::I386 | Architecture::X86_64_X32 => scan_x86(32, bytes, addr, cx, emit),
            _ => {}
        }
    }

    fn build_xrefs(&self) -> XrefIndex {
        let map = AddressMap::new(self);
        let base = map.ranges.first().map_or(0, |r| r.0);
        let cx = Scan {
            map: &map,
            symbols: &self.symbols,
            absolute: base >= 0x10_0000 || self.summary.format == crate::model::Format::Xbe,
            tables: &self.code_tables,
        };
        let top = base + u32::MAX as u64;
        let mut lists: [Vec<u64>; 6] = Default::default();
        let mut emit = |source: u64, target: u64, kind: RefKind| {
            if target >= base && target <= top && source >= base && source <= top && map.get(target).is_some() {
                lists[kind as usize].push((target - base) << 32 | (source - base));
            }
        };
        if let Some(rom) = &self.rom {
            // A ROM mixes code with data: only what following the code found.
            for &(source, target, kind) in &rom.analysis.refs {
                emit(source, target, kind);
            }
        } else {
            for sec in self.sections.iter().filter(|s| s.loaded && s.kind == RegionKind::Code) {
                if let Some(bytes) = section_bytes(&self.data, sec) {
                    self.scan_code(bytes, sec.address, &cx, &mut emit);
                }
            }
            self.scan_pointers(self.scheme(), &mut |at, t| emit(at, t, RefKind::Pointer));
        }
        for list in &mut lists {
            list.sort_unstable();
            list.dedup();
            list.shrink_to_fit();
        }
        XrefIndex { base, lists }
    }

    /// Every call and tail call in the code: (site, target).
    pub(crate) fn calls(&self) -> Vec<(u64, u64)> {
        let index = self.xref_index();
        [RefKind::Call, RefKind::Jump]
            .into_iter()
            .flat_map(|k| {
                index.lists[k as usize]
                    .iter()
                    .map(|&v| (index.source(v), index.target(v)))
            })
            .collect()
    }

    /// Number of references in the index, by kind.
    pub fn xref_counts(&self) -> RefCounts {
        let index = self.xref_index();
        let mut counts = RefCounts::default();
        for k in KINDS {
            counts.add(k, index.lists[k as usize].len() as u32);
        }
        counts
    }

    /// References to addresses in `lo..hi`, by kind (calls first), then by source.
    pub fn references_to(&self, lo: u64, hi: u64, offset: u32, limit: u32) -> RefPage {
        let index = self.xref_index();
        let mut counts = RefCounts::default();
        let slices: Vec<(RefKind, &[u64])> = KINDS
            .iter()
            .map(|&k| {
                let s = index.range(k, lo, hi);
                counts.add(k, s.len() as u32);
                (k, s)
            })
            .collect();
        let refs = slices
            .iter()
            .flat_map(|&(k, s)| s.iter().map(move |&v| (k, v)))
            .skip(offset as usize)
            .take(limit as usize)
            .map(|(k, v)| self.describe_ref(index.source(v), index.target(v), k))
            .collect();
        RefPage {
            total: counts.total(),
            offset,
            counts,
            refs,
        }
    }

    /// References to addresses in `lo..hi`, counted by kind.
    pub fn reference_counts(&self, lo: u64, hi: u64) -> RefCounts {
        let index = self.xref_index();
        let mut counts = RefCounts::default();
        for k in KINDS {
            counts.add(k, index.range(k, lo, hi).len() as u32);
        }
        counts
    }

    /// References to addresses in `lo..hi` from elsewhere, counted by kind.
    fn reference_counts_from_outside(&self, lo: u64, hi: u64) -> RefCounts {
        let index = self.xref_index();
        let mut counts = RefCounts::default();
        for k in KINDS {
            let outside = index
                .range(k, lo, hi)
                .iter()
                .filter(|&&v| !(lo..hi).contains(&index.source(v)))
                .count();
            counts.add(k, outside as u32);
        }
        counts
    }

    /// References made from `lo..hi`: code is scanned again, data is read for pointers.
    fn scan_range(&self, lo: u64, hi: u64) -> Vec<(u64, u64, RefKind)> {
        let mut out = Vec::new();
        let Some(sec) = self.section_at(lo) else { return out };
        let Some(bytes) = section_bytes(&self.data, sec) else {
            return out;
        };
        let from = (lo - sec.address) as usize;
        if from >= bytes.len() {
            return out;
        }
        let to = (hi.min(sec.address + bytes.len() as u64) - sec.address) as usize;
        let bytes = &bytes[from..to.max(from)];
        if sec.kind == RegionKind::Code {
            let map = AddressMap::new(self);
            let cx = Scan {
                map: &map,
                symbols: &self.symbols,
                absolute: map.ranges.first().is_some_and(|r| r.0 >= 0x10_0000)
                    || self.summary.format == crate::model::Format::Xbe,
                tables: &self.code_tables,
            };
            self.scan_code(bytes, lo, &cx, &mut |s, t, k| {
                if map.get(t).is_some() {
                    out.push((s, t, k));
                }
            });
        } else {
            let index = self.xref_index();
            let step = if self.is64 { 8 } else { 4 };
            // A megabyte of data at most: this is for looking at a table, not a whole section.
            let end = lo + (bytes.len() as u64).min(1 << 20);
            let mut at = lo.next_multiple_of(step);
            while at + step <= end && out.len() < 100_000 {
                if let Some(t) = self.decode_pointer(self.scheme(), at)
                    && index.contains(RefKind::Pointer, at, t)
                {
                    out.push((at, t, RefKind::Pointer));
                }
                at += step;
            }
        }
        out
    }

    /// References made by the code or data in `lo..hi`, in address order.
    pub fn references_from(&self, lo: u64, hi: u64) -> Vec<Reference> {
        self.scan_range(lo, hi)
            .into_iter()
            .map(|(s, t, k)| self.describe_ref(s, t, k))
            .collect()
    }

    /// The pointer stored at `address`, if the index found one there.
    pub fn pointer_at(&self, address: u64) -> Option<u64> {
        let index = self.xref_index();
        let t = self.decode_pointer(self.scheme(), address)?;
        index.contains(RefKind::Pointer, address, t).then_some(t)
    }

    fn describe_ref(&self, source: u64, target: u64, kind: RefKind) -> Reference {
        let function = self.symbols.function_containing(source);
        Reference {
            source,
            target,
            kind,
            function: function.map(|f| f.address),
            from: self.place(source),
            to: self.name_for(target).or_else(|| self.place(target)),
        }
    }

    /// Where an address is: `symbol+offset`, or `section+offset`.
    pub(crate) fn place(&self, address: u64) -> Option<String> {
        if let Some(s) = self.symbols.lookup(address) {
            let name = s.demangled.unwrap_or(s.name);
            return Some(if s.offset == 0 {
                name
            } else {
                format!("{name}+{:#x}", s.offset)
            });
        }
        let sec = self.section_at(address)?;
        Some(format!("{}+{:#x}", sec.name, address - sec.address))
    }

    /// A readable name for an address: a symbol starting there, a string's
    /// text, `symbol+offset`, or for a pointer slot, what it points to. Cheap:
    /// it uses the reference index only if it has been built.
    pub fn name_for(&self, address: u64) -> Option<String> {
        let sym = self.symbols.lookup(address);
        if let Some(s) = &sym
            && s.offset == 0
        {
            return Some(s.demangled.clone().unwrap_or_else(|| s.name.clone()));
        }
        if let Some(text) = self.string_at_address(address) {
            return Some(short_quote(&text, 80));
        }
        if let Some(s) = sym {
            return Some(format!("{}+{:#x}", s.demangled.unwrap_or(s.name), s.offset));
        }
        if let Some(index) = self.xrefs.get()
            && let Some(t) = self.decode_pointer(self.scheme(), address)
            && index.contains(RefKind::Pointer, address, t)
        {
            let what = match self.string_at_address(t) {
                Some(text) => short_quote(&text, 80),
                None => self.place(t)?,
            };
            return Some(format!("-> {what}"));
        }
        None
    }

    /// The text of the string at `address` (possibly the tail of a longer one).
    ///
    /// Outside C string sections, strings may not be NUL-terminated (Rust, Go,
    /// Swift literals run into each other), so once references are indexed the
    /// text also ends where the next referenced address begins.
    pub(crate) fn string_at_address(&self, address: u64) -> Option<String> {
        let text = self.string_preview(address, 200)?;
        if let Some(xrefs) = self.xrefs.get()
            && self
                .section_at(address)
                .is_some_and(|s| !crate::pointers::stringy(&s.name))
            && let Some(next) = xrefs.next_target(address)
            && next - address < text.len() as u64
        {
            let cut: String = text.chars().take((next - address) as usize).collect();
            return (cut.chars().count() >= 2).then_some(cut);
        }
        Some(text)
    }

    /// A call graph node for an address: what it is and its name.
    fn node(&self, address: u64) -> (NodeKind, String, Option<SymbolSource>, u64) {
        let code = self.section_at(address).is_some_and(|s| s.kind == RegionKind::Code);
        if let Some(s) = self.symbols.at(address) {
            let kind = if s.source == SymbolSource::Import || s.name().starts_with("__imp_") {
                NodeKind::Import
            } else if s.kind == SymbolKind::Function {
                NodeKind::Function
            } else if code {
                NodeKind::Code
            } else {
                NodeKind::Data
            };
            return (kind, s.display_name().into_owned(), Some(s.source), s.size);
        }
        let kind = if code { NodeKind::Code } else { NodeKind::Data };
        let name = self.name_for(address).unwrap_or_else(|| format!("{address:#x}"));
        (kind, name, None, 0)
    }

    /// The extent of the function containing `address`, or just the address.
    fn extent(&self, address: u64) -> (u64, u64) {
        match self.symbols.function_containing(address) {
            Some(f) => (f.address, f.address + f.size.max(1)),
            None => (address, address + 1),
        }
    }

    fn edges(&self, counts: HashMap<u64, (u32, u64)>) -> Vec<CallEdge> {
        let mut out: Vec<CallEdge> = counts
            .into_iter()
            .map(|(address, (calls, site))| {
                let (kind, name, _, _) = self.node(address);
                CallEdge {
                    address,
                    name,
                    kind,
                    calls,
                    site,
                }
            })
            .collect();
        out.sort_by(|a, b| b.calls.cmp(&a.calls).then_with(|| a.address.cmp(&b.address)));
        out
    }

    /// The functions that call (or tail-call) the function containing
    /// `address`, most call sites first. Call sites outside any known function
    /// are listed on their own.
    pub fn callers(&self, address: u64) -> Vec<CallEdge> {
        let (lo, hi) = self.extent(address);
        self.referrers(&[(lo, hi, RefKind::Call), (lo, hi, RefKind::Jump)], &|_| false)
    }

    /// The functions referring to any of `targets` (`lo..hi`, and how),
    /// most sites first; sites for which `skip` holds are left out.
    /// References from outside any known function are listed on their own.
    pub(crate) fn referrers(&self, targets: &[(u64, u64, RefKind)], skip: &dyn Fn(u64) -> bool) -> Vec<CallEdge> {
        let index = self.xref_index();
        let mut by_function: HashMap<u64, (u32, u64)> = HashMap::new();
        for &(lo, hi, kind) in targets {
            for &v in index.range(kind, lo, hi) {
                let site = index.source(v);
                if skip(site) {
                    continue;
                }
                let function = self.symbols.function_containing(site).map_or(site, |f| f.address);
                let e = by_function.entry(function).or_insert((0, site));
                e.0 += 1;
                e.1 = e.1.min(site);
            }
        }
        self.edges(by_function)
    }

    /// What the function containing `address` calls or tail-calls, most call
    /// sites first: functions, imports (stubs and slots), or code addresses.
    pub fn callees(&self, address: u64) -> Vec<CallEdge> {
        let Some(f) = self.symbols.function_containing(address) else {
            return Vec::new();
        };
        let mut by_callee: HashMap<u64, (u32, u64)> = HashMap::new();
        for (site, target, kind) in self.scan_range(f.address, f.address + f.size.max(1)) {
            if !kind.is_call() {
                continue;
            }
            let callee = self.symbols.function_containing(target).map_or(target, |g| g.address);
            let e = by_callee.entry(callee).or_insert((0, site));
            e.0 += 1;
            e.1 = e.1.min(site);
        }
        self.edges(by_callee)
    }

    /// The call graph around the function containing `center`: callers up to
    /// `up` levels, callees down to `down` levels, at most `fanout` neighbours
    /// per node (those with the most call sites).
    pub fn call_graph(&self, center: u64, up: u32, down: u32, fanout: usize) -> CallGraph {
        let center = self.extent(center).0;
        let mut depth_of: HashMap<u64, i32> = HashMap::from([(center, 0)]);
        let mut edges: HashMap<(u64, u64), u32> = HashMap::new();
        let mut counts: HashMap<u64, (Option<u32>, Option<u32>)> = HashMap::new();
        let mut hidden = 0u32;
        for (dir, levels) in [(-1i32, up), (1, down)] {
            let mut frontier = vec![center];
            for level in 1..=levels as i32 {
                let mut next = Vec::new();
                for &n in &frontier {
                    let list = if dir < 0 { self.callers(n) } else { self.callees(n) };
                    let entry = counts.entry(n).or_default();
                    if dir < 0 {
                        entry.0 = Some(list.len() as u32);
                    } else {
                        entry.1 = Some(list.len() as u32);
                    }
                    hidden += list.len().saturating_sub(fanout) as u32;
                    for e in list.into_iter().take(fanout) {
                        let key = if dir < 0 { (e.address, n) } else { (n, e.address) };
                        *edges.entry(key).or_default() += e.calls;
                        if let std::collections::hash_map::Entry::Vacant(v) = depth_of.entry(e.address) {
                            v.insert(dir * level);
                            // Imports and data are leaves.
                            if matches!(e.kind, NodeKind::Function | NodeKind::Code) {
                                next.push(e.address);
                            }
                        }
                    }
                }
                frontier = next;
            }
        }
        // The centre's counts even when a direction was not expanded.
        if counts.get(&center).is_none_or(|c| c.0.is_none()) {
            let n = self.callers(center).len() as u32;
            counts.entry(center).or_default().0 = Some(n);
        }
        if counts.get(&center).is_none_or(|c| c.1.is_none()) {
            let n = self.callees(center).len() as u32;
            counts.entry(center).or_default().1 = Some(n);
        }
        let mut nodes: Vec<GraphNode> = depth_of
            .into_iter()
            .map(|(address, depth)| {
                let (kind, name, source, size) = self.node(address);
                let (callers, callees) = counts.get(&address).copied().unwrap_or_default();
                GraphNode {
                    address,
                    name,
                    kind,
                    source,
                    size,
                    depth,
                    callers,
                    callees,
                }
            })
            .collect();
        nodes.sort_by_key(|n| (n.depth, n.address));
        let mut edges: Vec<GraphEdge> = edges
            .into_iter()
            .map(|((from, to), calls)| GraphEdge { from, to, calls })
            .collect();
        edges.sort_by_key(|e| (e.from, e.to));
        CallGraph {
            center,
            nodes,
            edges,
            hidden,
        }
    }

    /// A shortest chain of calls from the function containing `from` to the one
    /// containing `to` (both ends included), at most `max_depth` calls long.
    pub fn call_path(&self, from: u64, to: u64, max_depth: u32) -> Option<Vec<PathStep>> {
        let from = self.extent(from).0;
        let to = self.extent(to).0;
        let mut parent: HashMap<u64, (u64, u64)> = HashMap::new();
        let mut seen: HashSet<u64> = HashSet::from([from]);
        let mut queue: VecDeque<(u64, u32)> = VecDeque::from([(from, 0)]);
        while let Some((n, depth)) = queue.pop_front() {
            if n == to {
                let mut steps = Vec::new();
                let mut cur = to;
                loop {
                    let (_, name, _, _) = self.node(cur);
                    let up = parent.get(&cur).copied();
                    steps.push(PathStep {
                        address: cur,
                        name,
                        site: up.map(|p| p.1),
                    });
                    match up {
                        Some((p, _)) => cur = p,
                        None => break,
                    }
                }
                steps.reverse();
                return Some(steps);
            }
            if depth >= max_depth || seen.len() > 100_000 {
                continue;
            }
            for e in self.callees(n) {
                if seen.insert(e.address) {
                    parent.insert(e.address, (n, e.site));
                    queue.push_back((e.address, depth + 1));
                }
            }
        }
        None
    }

    /// Callers, callees, strings and data used by the function containing `address`.
    pub fn function_summary(&self, address: u64, limit: usize) -> Option<FunctionSummary> {
        let f = self.symbols.function_containing(address)?;
        let (lo, hi) = (f.address, f.address + f.size.max(1));
        let callers = self.callers(lo);
        let callees = self.callees(lo);
        let mut strings = Vec::new();
        let mut data = Vec::new();
        let mut seen = HashSet::new();
        for (site, target, kind) in self.scan_range(lo, hi) {
            // Its own code and jump tables aren't data it uses.
            if kind.is_call() || (lo..hi).contains(&target) || !seen.insert(target) {
                continue;
            }
            // A float or double read (its bytes can pass for a short string) is data with a value.
            if let Some(value) = self.float_operand_at(site) {
                if data.len() < limit {
                    let mut r = self.describe_ref(site, target, kind);
                    r.to = Some(value);
                    data.push(r);
                }
                continue;
            }
            match self.string_at_address(target) {
                Some(text) if strings.len() < limit => strings.push(StringUse {
                    address: target,
                    text,
                    site,
                }),
                Some(_) => {}
                None if data.len() < limit => data.push(self.describe_ref(site, target, kind)),
                None => {}
            }
        }
        Some(FunctionSummary {
            address: lo,
            name: f.display_name().into_owned(),
            size: hi - lo,
            caller_count: callers.len() as u32,
            callers: callers.into_iter().take(limit).collect(),
            callee_count: callees.len() as u32,
            callees: callees.into_iter().take(limit).collect(),
            strings,
            data,
            referenced_by: self.reference_counts_from_outside(lo, hi),
        })
    }
}
