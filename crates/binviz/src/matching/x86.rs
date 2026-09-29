//! Matching x86 and x86-64 code: a function of the compiler's object (COFF
//! from MSVC or clang-cl, or ELF) against the original's.
//!
//! Instructions are lined up by their shape: the operation, its prefixes,
//! the kinds of its operands, the registers in them and the size of what it
//! reads, but not the numbers (immediates, displacements, branch targets),
//! which are compared once the two sides are lined up. The bytes a
//! relocation covers in the rebuild, and the same bytes of the original's
//! instruction, are what the linker fills in: rather than compared, each
//! relocation's symbol is looked up in the binary and the original checked
//! to point there. A jump inside the function is compared by where it
//! lands (the instruction lined up with its target), not by its
//! displacement, so that code of another length before it isn't counted
//! twice.
//!
//! MSVC keeps a switch's jump table, and the byte table that picks an entry
//! of it, in the code right after the function. The original's tables are
//! those that following its code found, or that its code reads in its own
//! extent; the rebuild's start where a relocation from its code points into
//! the function. Their entries are compared by the case they lead to.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use iced_x86::{
    ConditionCode, ConstantOffsets, Decoder, DecoderOptions, FlowControl, Formatter, Instruction, IntelFormatter,
    MemorySize, Mnemonic, OpKind, Register, SymbolResolver, SymbolResult,
};

use super::{Lookups, MatchLine, MatchResult, ObjectFunction, Reloc, RelocForm, category};
use crate::binary::Binary;
use crate::fndiff::{Edit, LineKind, edit_script};

/// Where a function's code ends: after its last instruction that isn't
/// alignment padding (`nop`, `int3`, zeros).
pub(super) fn code_len(code: &[u8], bits: u32) -> usize {
    let mut decoder = Decoder::with_ip(bits, code, 0, DecoderOptions::NONE);
    let mut ins = Instruction::default();
    let mut end = 0;
    while decoder.can_decode() {
        let pos = decoder.position();
        decoder.decode_out(&mut ins);
        if !is_padding(&ins, &code[pos..pos + ins.len()]) {
            end = pos + ins.len();
        }
    }
    end
}

/// Whether `bytes` (what follows a function's code in its extent) are only padding.
pub(crate) fn padding_only(bytes: &[u8], bits: u32) -> bool {
    code_len(bytes, bits) == 0
}

fn is_padding(ins: &Instruction, bytes: &[u8]) -> bool {
    matches!(ins.mnemonic(), Mnemonic::Nop | Mnemonic::Int3) || bytes.iter().all(|&b| b == 0)
}

/// An instruction of one side, or a row of a jump table in its code.
struct Item {
    /// Its offset in the function.
    at: usize,
    len: usize,
    ins: Instruction,
    /// A table's row: 4 for an entry (a case's address), 1 for index bytes; 0 for an instruction.
    row: u8,
    offsets: ConstantOffsets,
    shape: u32,
}

impl Item {
    fn instruction(at: usize, ins: Instruction, offsets: ConstantOffsets) -> Item {
        Item {
            at,
            len: ins.len(),
            shape: shape(&ins),
            ins,
            row: 0,
            offsets,
        }
    }

    fn row(at: usize, len: usize, entry: u8) -> Item {
        Item {
            at,
            len,
            ins: Instruction::default(),
            row: entry,
            offsets: ConstantOffsets::default(),
            shape: hash32(&("table row", entry)),
        }
    }

    fn end(&self) -> usize {
        self.at + self.len
    }

    /// Where the instruction's immediate (or a branch's displacement) sits in it.
    fn immediate_field(&self) -> Option<usize> {
        self.offsets.has_immediate().then(|| self.offsets.immediate_offset())
    }

    /// Where its memory operand's displacement sits in it.
    fn displacement_field(&self) -> Option<usize> {
        self.offsets
            .has_displacement()
            .then(|| self.offsets.displacement_offset())
    }

    fn is_branch(&self) -> bool {
        self.row == 0 && (0..self.ins.op_count()).any(|i| is_branch_kind(self.ins.op_kind(i)))
    }
}

fn hash32(x: &impl Hash) -> u32 {
    let mut h = DefaultHasher::new();
    x.hash(&mut h);
    h.finish() as u32
}

fn is_immediate(k: OpKind) -> bool {
    matches!(
        k,
        OpKind::Immediate8
            | OpKind::Immediate8_2nd
            | OpKind::Immediate16
            | OpKind::Immediate32
            | OpKind::Immediate64
            | OpKind::Immediate8to16
            | OpKind::Immediate8to32
            | OpKind::Immediate8to64
            | OpKind::Immediate32to64
    )
}

fn is_branch_kind(k: OpKind) -> bool {
    matches!(k, OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64)
}

/// What lines an instruction up with another: its operation and prefixes,
/// the kinds of its operands and the registers in them, and the size of
/// what it reads or writes; not the numbers where things were placed
/// decides (immediates, displacements, branch targets).
fn shape(ins: &Instruction) -> u32 {
    let mut h = DefaultHasher::new();
    (ins.mnemonic() as u32).hash(&mut h);
    (ins.has_rep_prefix(), ins.has_repne_prefix(), ins.has_lock_prefix()).hash(&mut h);
    (ins.segment_prefix() as u32).hash(&mut h);
    for i in 0..ins.op_count() {
        match ins.op_kind(i) {
            OpKind::Register => (0u8, ins.op_register(i) as u32).hash(&mut h),
            OpKind::Memory => (
                1u8,
                ins.memory_base() as u32,
                ins.memory_index() as u32,
                ins.memory_index_scale(),
                ins.memory_size() as u32,
            )
                .hash(&mut h),
            k if is_immediate(k) => 2u8.hash(&mut h),
            k if is_branch_kind(k) => 3u8.hash(&mut h),
            k => (4u8, k as u32).hash(&mut h),
        }
    }
    h.finish() as u32
}

/// What a relocated field refers to, relative to its symbol: the addend the
/// linker adds (with the one the field holds, for formats that keep it
/// there) and, for a displacement, the distance from the field to the end
/// of the instruction, which the processor adds it to.
fn reference_offset(code: &[u8], r: &Reloc, field: usize, ins_end: usize) -> i64 {
    let mut offset = r.addend;
    if r.implicit {
        offset = offset.wrapping_add(read_field(code, field, usize::from(r.bits).div_ceil(8)) as i64);
    }
    if r.form == RelocForm::Relative {
        offset += ins_end as i64 - field as i64;
    }
    offset
}

/// A little-endian field, sign-extended.
fn read_field(code: &[u8], at: usize, width: usize) -> u64 {
    let Some(b) = code.get(at..at + width) else {
        return 0;
    };
    match width {
        1 => b[0] as i8 as i64 as u64,
        2 => i16::from_le_bytes([b[0], b[1]]) as i64 as u64,
        4 => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64 as u64,
        8 => u64::from_le_bytes(b.try_into().unwrap_or_default()),
        _ => 0,
    }
}

/// Names for the operands of the instruction being formatted.
#[derive(Default)]
struct Names {
    memory: Option<String>,
    immediate: Option<String>,
    branch: Option<String>,
}

struct Resolver(Rc<RefCell<Names>>);

impl SymbolResolver for Resolver {
    fn symbol(
        &mut self,
        ins: &Instruction,
        _operand: u32,
        op: Option<u32>,
        address: u64,
        _size: u32,
    ) -> Option<SymbolResult<'_>> {
        let names = self.0.borrow();
        let name = match ins.op_kind(op?) {
            OpKind::Memory => names.memory.clone(),
            k if is_branch_kind(k) => names.branch.clone(),
            k if is_immediate(k) => names.immediate.clone(),
            _ => None,
        }?;
        Some(SymbolResult::with_string(address, name))
    }
}

/// Instructions as text, with the names given for their operands.
struct Text {
    formatter: IntelFormatter,
    names: Rc<RefCell<Names>>,
}

impl Text {
    fn new() -> Text {
        let names = Rc::new(RefCell::new(Names::default()));
        let mut formatter = IntelFormatter::with_options(Some(Box::new(Resolver(names.clone()))), None);
        let o = formatter.options_mut();
        o.set_hex_prefix("0x");
        o.set_hex_suffix("");
        o.set_uppercase_hex(false);
        o.set_space_after_operand_separator(true);
        o.set_branch_leading_zeros(false);
        Text { formatter, names }
    }

    fn format(&mut self, ins: &Instruction, names: Names) -> String {
        *self.names.borrow_mut() = names;
        let mut out = String::new();
        self.formatter.format(ins, &mut out);
        out
    }
}

/// One step of the two sides lined up.
enum Step {
    /// The same shape.
    Keep(usize, usize),
    /// Different instructions in the same place.
    Pair(usize, usize),
    /// Only in the original; `true` when the same instruction is elsewhere in the rebuild.
    Removed(usize, bool),
    /// Only in the rebuild; `true` when the same instruction is elsewhere in the original.
    Added(usize, bool),
}

/// The two sides of a function being matched.
struct Match<'a> {
    bin: &'a Binary,
    func: &'a ObjectFunction,
    lookups: &'a Lookups,
    bits: u32,
    /// The original's address; the rebuild is read as if it were there too.
    start: u64,
    name: String,
    /// The original's code, its instructions, and the rebuild's.
    code: &'a [u8],
    a: Vec<Item>,
    b: Vec<Item>,
    /// The original's item lined up with each of the rebuild's.
    b_to_a: Vec<Option<usize>>,
}

impl Binary {
    /// The rebuilt x86 `func` against the original's function at `address`.
    pub(super) fn match_x86(
        &self,
        address: u64,
        func: &ObjectFunction,
        bits: u32,
        lookups: &Lookups,
    ) -> Option<MatchResult> {
        let f = self.symbols().function_containing(address)?;
        let start = f.address;
        let bytes = self.code_bytes(start)?;
        // A function of unknown size is taken to be as long as the rebuild.
        let size = if f.size > 1 { f.size as usize } else { func.code.len() };
        let code = &bytes[..size.min(bytes.len())];
        let a = self.original_items(start, code, bits);
        let b = rebuilt_items(func, start, bits);
        let mut m = Match {
            bin: self,
            func,
            lookups,
            bits,
            start,
            name: f.display_name().into_owned(),
            code,
            b_to_a: vec![None; b.len()],
            a,
            b,
        };
        Some(m.run())
    }

    /// The original's function at `start` as items: its instructions, and
    /// the rows of its jump tables (those following its code found, and
    /// what its code reads from its own extent after the code); the padding
    /// after its code left out.
    fn original_items(&self, start: u64, code: &[u8], bits: u32) -> Vec<Item> {
        let mut items = Vec::new();
        let mut data: Option<usize> = None;
        let mut decoder = Decoder::with_ip(bits, code, start, DecoderOptions::NONE);
        let mut ins = Instruction::default();
        while decoder.can_decode() {
            let pos = decoder.position();
            if data.is_some_and(|d| pos >= d) {
                break;
            }
            if let Some(t) = self.code_table_at(start + pos as u64) {
                let end = ((t.end() - start) as usize).min(code.len());
                let first = (t.address.max(start) - start) as usize;
                let entry = |at: usize| t.entry == 4 && (at - first).is_multiple_of(4) && at + 4 <= end;
                push_rows(&mut items, pos, end, entry);
                if decoder.set_position(end).is_err() {
                    break;
                }
                decoder.set_ip(start + end as u64);
                continue;
            }
            decoder.decode_out(&mut ins);
            let offsets = decoder.get_constant_offsets(&ins);
            if let Some(at) = own_data(&ins, start, code.len(), bits)
                && at > pos
            {
                data = Some(data.map_or(at, |d| d.min(at)));
            }
            items.push(Item::instruction(pos, ins, offsets));
        }
        if data.is_some() {
            // An entry is the address of a case in the function.
            let from = items.last().map_or(0, Item::end);
            let inside = |at: usize| {
                code.get(at..at + 4)
                    .map(|b| u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
                    .is_some_and(|v| v >= start && v < start + code.len() as u64)
            };
            push_rows(&mut items, from, code.len(), inside);
        }
        while items.len() > 1
            && items
                .last()
                .is_some_and(|i| i.row == 0 && is_padding(&i.ins, &code[i.at..i.end()]))
        {
            items.pop();
        }
        items
    }

    /// The name of what is at `address`, with the offset into it.
    fn label(&self, address: u64) -> Option<String> {
        let s = self.symbols().lookup(address)?;
        let name = s.demangled.as_deref().unwrap_or(&s.name);
        Some(if s.offset == 0 {
            name.to_string()
        } else {
            format!("{name}+{:#x}", s.offset)
        })
    }
}

/// Where in its own function (`len` bytes at `start`) an instruction of the
/// original reads data: a jump table, or the byte table picking its entry.
fn own_data(ins: &Instruction, start: u64, len: usize, bits: u32) -> Option<usize> {
    if ins.mnemonic() == Mnemonic::Lea || !(0..ins.op_count()).any(|i| ins.op_kind(i) == OpKind::Memory) {
        return None;
    }
    let address = if ins.is_ip_rel_memory_operand() {
        ins.ip_rel_memory_address()
    } else if bits == 32 {
        u64::from(ins.memory_displacement32())
    } else {
        return None;
    };
    let at = address.checked_sub(start)?;
    (at < len as u64).then_some(at as usize)
}

/// Rows of a table from `from` to `end`: an entry of 4 bytes where `entry`
/// says one starts, else up to 8 index bytes.
fn push_rows(items: &mut Vec<Item>, from: usize, end: usize, entry: impl Fn(usize) -> bool) {
    let mut at = from;
    while at < end {
        if entry(at) {
            items.push(Item::row(at, 4, 4));
            at += 4;
            continue;
        }
        let mut n = 1;
        while at + n < end && n < 8 && !entry(at + n) {
            n += 1;
        }
        items.push(Item::row(at, n, 1));
        at += n;
    }
}

/// The rebuild's function as items, read as if it were at `start`: its
/// instructions, then its own data (an MSVC switch's tables) from where a
/// relocation from its code points into it.
fn rebuilt_items(func: &ObjectFunction, start: u64, bits: u32) -> Vec<Item> {
    let code = &func.code[..];
    let mut items = Vec::new();
    let mut data: Option<usize> = None;
    let mut decoder = Decoder::with_ip(bits, code, start, DecoderOptions::NONE);
    let mut ins = Instruction::default();
    while decoder.can_decode() {
        let pos = decoder.position();
        if data.is_some_and(|d| pos >= d) {
            break;
        }
        decoder.decode_out(&mut ins);
        let offsets = decoder.get_constant_offsets(&ins);
        if offsets.has_displacement() {
            let field = pos + offsets.displacement_offset();
            if let Some(r) = func.relocs.get(&(field as u64))
                && let Some(target) = target_in_function(func, r, field, pos + ins.len())
                && target > pos
            {
                data = Some(data.map_or(target, |d| d.min(target)));
            }
        }
        items.push(Item::instruction(pos, ins, offsets));
    }
    if data.is_some() {
        let from = items.last().map_or(0, Item::end);
        push_rows(&mut items, from, code.len(), |at| {
            func.relocs.contains_key(&(at as u64)) && at + 4 <= code.len()
        });
    }
    items
}

/// Where in the function a relocation of its own code refers to, when it
/// refers into the function itself (a jump table after its code, a case).
fn target_in_function(func: &ObjectFunction, r: &Reloc, field: usize, ins_end: usize) -> Option<usize> {
    let (section, offset) = r.defined?;
    if section != func.section {
        return None;
    }
    let at = offset as i64 + reference_offset(&func.code, r, field, ins_end) - func.offset as i64;
    (0..func.code.len() as i64).contains(&at).then_some(at as usize)
}

impl Match<'_> {
    fn run(&mut self) -> MatchResult {
        let ta: Vec<u32> = self.a.iter().map(|i| i.shape).collect();
        let tb: Vec<u32> = self.b.iter().map(|i| i.shape).collect();
        let script = edit_script(&ta, &tb, 4000).unwrap_or_else(|| {
            let mut s = vec![Edit::Delete; ta.len()];
            s.extend(std::iter::repeat_n(Edit::Insert, tb.len()));
            s
        });
        let steps = self.line_up(&script);
        let mut text_a = Text::new();
        let mut text_b = Text::new();
        let mut lines = Vec::new();
        let mut counts: BTreeMap<String, u32> = BTreeMap::new();
        let mut matched = 0u32;
        for step in &steps {
            let (i, j, kind, note) = match *step {
                Step::Keep(i, j) => {
                    let note = self.compare(i, j);
                    let kind = if note.is_some() {
                        LineKind::Changed
                    } else {
                        LineKind::Same
                    };
                    (Some(i), Some(j), kind, note)
                }
                Step::Pair(i, j) => (Some(i), Some(j), LineKind::Changed, Some(self.explain_pair(i, j))),
                Step::Removed(i, moved) => (Some(i), None, LineKind::Removed, Some(self.alone(i, moved, true))),
                Step::Added(j, moved) => (None, Some(j), LineKind::Added, Some(self.alone(j, moved, false))),
            };
            match &note {
                None => matched += 1,
                Some(n) => *counts.entry(category(n)).or_default() += 1,
            }
            lines.push(MatchLine {
                kind,
                address: i.map(|i| self.start + self.a[i].at as u64),
                original: i.map(|i| self.original_text(&mut text_a, i)),
                rebuilt: j.map(|j| self.rebuilt_text(&mut text_b, j)),
                note,
            });
        }
        let total = self.a.len().max(self.b.len()) as u32;
        MatchResult {
            name: self.name.clone(),
            address: self.start,
            original_instructions: self.a.len() as u32,
            rebuilt_instructions: self.b.len() as u32,
            matched_instructions: matched,
            original_bytes: self.a.last().map_or(0, Item::end) as u64,
            rebuilt_bytes: self.func.code.len() as u64,
            percent: if total == 0 {
                100.0
            } else {
                matched as f32 * 100.0 / total as f32
            },
            lines,
            differences: counts.into_iter().collect(),
        }
    }

    /// The steps of the edit script, with the instructions it removes in
    /// one place and adds in another paired up as reordered, and those it
    /// replaces paired up in order; fills `b_to_a`.
    fn line_up(&mut self, script: &[Edit]) -> Vec<Step> {
        enum Seg {
            Keep(usize, usize),
            Run(Vec<usize>, Vec<usize>),
        }
        let mut segs = Vec::new();
        let (mut i, mut j, mut k) = (0, 0, 0);
        while k < script.len() {
            if script[k] == Edit::Keep {
                segs.push(Seg::Keep(i, j));
                i += 1;
                j += 1;
                k += 1;
                continue;
            }
            let (mut dels, mut ins) = (Vec::new(), Vec::new());
            while k < script.len() && script[k] != Edit::Keep {
                if script[k] == Edit::Delete {
                    dels.push(i);
                    i += 1;
                } else {
                    ins.push(j);
                    j += 1;
                }
                k += 1;
            }
            segs.push(Seg::Run(dels, ins));
        }
        // The same instruction removed in one place and added in another: reordered.
        let mut added: HashMap<u64, Vec<usize>> = HashMap::new();
        for s in &segs {
            if let Seg::Run(_, ins) = s {
                for &j in ins {
                    if let Some(key) = self.key(j, false) {
                        added.entry(key).or_default().push(j);
                    }
                }
            }
        }
        for list in added.values_mut() {
            list.reverse();
        }
        let mut moved_a = vec![false; self.a.len()];
        let mut moved_b = vec![false; self.b.len()];
        for s in &segs {
            if let Seg::Run(dels, _) = s {
                for &i in dels {
                    if let Some(key) = self.key(i, true)
                        && let Some(j) = added.get_mut(&key).and_then(Vec::pop)
                    {
                        moved_a[i] = true;
                        moved_b[j] = true;
                        self.b_to_a[j] = Some(i);
                    }
                }
            }
        }
        let mut steps = Vec::new();
        for s in segs {
            match s {
                Seg::Keep(i, j) => {
                    self.b_to_a[j] = Some(i);
                    steps.push(Step::Keep(i, j));
                }
                Seg::Run(dels, ins) => {
                    let rest_a: Vec<usize> = dels.iter().copied().filter(|&i| !moved_a[i]).collect();
                    let rest_b: Vec<usize> = ins.iter().copied().filter(|&j| !moved_b[j]).collect();
                    for (&i, &j) in rest_a.iter().zip(&rest_b) {
                        self.b_to_a[j] = Some(i);
                        steps.push(Step::Pair(i, j));
                    }
                    let n = rest_a.len().min(rest_b.len());
                    steps.extend(rest_a[n..].iter().map(|&i| Step::Removed(i, false)));
                    steps.extend(rest_b[n..].iter().map(|&j| Step::Added(j, false)));
                    steps.extend(dels.iter().filter(|&&i| moved_a[i]).map(|&i| Step::Removed(i, true)));
                    steps.extend(ins.iter().filter(|&&j| moved_b[j]).map(|&j| Step::Added(j, true)));
                }
            }
        }
        steps
    }

    /// What must be the same for an instruction elsewhere to be the same
    /// instruction: its shape and its small numbers (stack offsets, field
    /// offsets, constants), not addresses or what the linker fills in.
    fn key(&self, index: usize, original: bool) -> Option<u64> {
        let item = if original { &self.a[index] } else { &self.b[index] };
        if item.row != 0 || item.is_branch() {
            return None;
        }
        let relocated = |field: Option<usize>| !original && field.is_some_and(|f| self.reloc_at(item, f).is_some());
        let small = |v: i64| (-0x10000..0x10000).contains(&v);
        let ins = &item.ins;
        let mut h = DefaultHasher::new();
        item.shape.hash(&mut h);
        for op in 0..ins.op_count() {
            match ins.op_kind(op) {
                k if is_immediate(k) => {
                    let v = immediate(ins, op);
                    if small(v) && !relocated(item.immediate_field()) {
                        v.hash(&mut h);
                    }
                }
                OpKind::Memory => {
                    let v = displacement(ins, self.bits);
                    let absolute = ins.memory_base() == Register::None || ins.is_ip_rel_memory_operand();
                    if !absolute && small(v) && !relocated(item.displacement_field()) {
                        v.hash(&mut h);
                    }
                }
                _ => {}
            }
        }
        Some(h.finish())
    }

    /// The rebuild's relocation of the field at `field` in `item`.
    fn reloc_at(&self, item: &Item, field: usize) -> Option<&Reloc> {
        self.func.relocs.get(&((item.at + field) as u64))
    }

    /// The rebuild's relocations in `item`: (offset in the instruction, relocation).
    fn relocs_in(&self, item: &Item) -> impl Iterator<Item = (usize, &Reloc)> {
        self.func
            .relocs
            .range(item.at as u64..item.end() as u64)
            .map(move |(at, r)| (*at as usize - item.at, r))
    }

    /// Where a branch or a `[rip+N]` of the rebuild's that no relocation
    /// covers lands, as an offset into the function when it lands in it,
    /// else (Err) as an offset into its section.
    fn unrelocated_target(&self, j: usize) -> Option<Result<usize, i64>> {
        let item = &self.b[j];
        let target = if item.is_branch() && item.immediate_field().is_some_and(|f| self.reloc_at(item, f).is_none()) {
            item.ins.near_branch_target()
        } else if item.ins.is_ip_rel_memory_operand()
            && item
                .displacement_field()
                .is_some_and(|f| self.reloc_at(item, f).is_none())
        {
            item.ins.ip_rel_memory_address()
        } else {
            return None;
        };
        let at = target.wrapping_sub(self.start) as i64;
        Some(if (0..self.func.code.len() as i64).contains(&at) {
            Ok(at as usize)
        } else {
            Err(self.func.offset as i64 + at)
        })
    }

    /// The original's address lined up with the rebuild's item at `at` (or,
    /// when that one has no counterpart, the next that has).
    fn lined_up(&self, at: usize) -> Option<u64> {
        let k = self.b.binary_search_by_key(&at, |i| i.at).ok()?;
        (k..self.b.len())
            .find_map(|k| self.b_to_a[k])
            .map(|i| self.start + self.a[i].at as u64)
    }

    /// The name, and offset, of what a branch of the rebuild's that the
    /// assembler resolved lands on outside the function: another function
    /// of its section.
    fn neighbour(&self, section_offset: i64) -> Option<(&str, i64)> {
        let (at, name) = self
            .func
            .neighbours
            .iter()
            .rev()
            .find(|(at, _)| *at as i64 <= section_offset)?;
        Some((name.as_str(), section_offset - *at as i64))
    }

    /// Why the original's item `i` and the rebuild's `j`, lined up by their
    /// shape, differ (None: they match).
    fn compare(&self, i: usize, j: usize) -> Option<String> {
        let (a, b) = (&self.a[i], &self.b[j]);
        if a.row != 0 {
            return self.compare_rows(i, j);
        }
        // Compared elsewhere: the linker's fields, and where a branch lands.
        let mut skip = vec![false; b.len];
        for (at, r) in self.relocs_in(b) {
            for s in skip.iter_mut().skip(at).take(usize::from(r.bits).div_ceil(8)) {
                *s = true;
            }
        }
        let resolved = self.unrelocated_target(j);
        if resolved.is_some() {
            let field = if b.is_branch() {
                (b.offsets.immediate_offset(), b.offsets.immediate_size())
            } else {
                (b.offsets.displacement_offset(), b.offsets.displacement_size())
            };
            for s in skip.iter_mut().skip(field.0).take(field.1) {
                *s = true;
            }
        }
        let (ab, bb) = (&self.code[a.at..a.end()], &self.func.code[b.at..b.end()]);
        if ab.len() != bb.len() || ab.iter().zip(bb).zip(&skip).any(|((x, y), s)| !s && x != y) {
            return Some(self.explain_keep(i, j));
        }
        if let Some(target) = resolved
            && let Some(note) = self.check_resolved(i, j, target)
        {
            return Some(note);
        }
        self.relocs_in(b).find_map(|(at, r)| self.check_reloc(i, j, at, r))
    }

    /// A branch or reference the assembler resolved: whether the original's
    /// lands on the same place.
    fn check_resolved(&self, i: usize, j: usize, target: Result<usize, i64>) -> Option<String> {
        let (a, b) = (&self.a[i].ins, &self.b[j]);
        let original = if b.is_branch() {
            a.near_branch_target()
        } else {
            a.ip_rel_memory_address()
        };
        match target {
            Ok(at) => {
                let expected = self.lined_up(at)?;
                (original != expected).then(|| {
                    format!(
                        "branch target differs: to {} in the original, to what is lined up with {} in the rebuild",
                        self.name_of(original),
                        self.name_of(expected)
                    )
                })
            }
            Err(section_offset) => {
                let (name, offset) = self.neighbour(section_offset)?;
                let expected = self
                    .bin
                    .cached_symbol_address(self.lookups, name)?
                    .wrapping_add(offset as u64);
                (original != expected).then(|| self.target_note(&b.ins, &with_offset(name, offset), original))
            }
        }
    }

    /// A relocation of the rebuild's (at `at` in its instruction `j`):
    /// whether the original points where its symbol is in the binary.
    fn check_reloc(&self, i: usize, j: usize, at: usize, r: &Reloc) -> Option<String> {
        if r.form == RelocForm::Other && !r.image_offset {
            return None;
        }
        let (a, b) = (&self.a[i], &self.b[j]);
        let width = usize::from(r.bits).div_ceil(8);
        let field = read_field(self.code, a.at + at, width);
        let original = match r.form {
            RelocForm::Relative => (self.start + a.end() as u64).wrapping_add(field),
            // An offset from the image base (MSVC x64): where it leads.
            _ if r.image_offset => self.bin.image_base.wrapping_add(field & 0xFFFF_FFFF),
            _ => field,
        };
        let offset = reference_offset(&self.func.code, r, b.at + at, b.end());
        let (expected, rebuilt) = match target_in_function(self.func, r, b.at + at, b.end()) {
            Some(t) => (self.lined_up(t)?, format!("{}+{t:#x}", self.name)),
            None if r.section_symbol => return None,
            None => (
                self.bin
                    .cached_symbol_address(self.lookups, &r.symbol)?
                    .wrapping_add(offset as u64),
                with_offset(&r.symbol, offset),
            ),
        };
        let mask = if width >= 8 {
            u64::MAX
        } else {
            (1u64 << (8 * width)) - 1
        };
        let same = match r.form {
            RelocForm::Relative => original == expected,
            _ if r.image_offset => original == expected,
            _ => (original ^ expected) & mask == 0,
        };
        if same {
            return None;
        }
        let original = if r.form == RelocForm::Relative || r.image_offset {
            original
        } else {
            original & mask
        };
        Some(if a.row != 0 {
            format!(
                "jump table differs: an entry leads to {} in the original, to what is lined up with {} in the rebuild",
                self.name_of(original),
                self.name_of(expected)
            )
        } else if matches!(
            b.ins.flow_control(),
            FlowControl::Call
                | FlowControl::IndirectCall
                | FlowControl::UnconditionalBranch
                | FlowControl::IndirectBranch
        ) {
            self.target_note(&b.ins, &rebuilt, original)
        } else {
            format!(
                "global differs: {rebuilt} in the rebuild; the original refers to {}",
                self.describe(original)
            )
        })
    }

    /// An address of the original's, and its name when it has one.
    fn describe(&self, address: u64) -> String {
        match self.bin.label(address) {
            Some(name) => format!("{address:#x} ({name})"),
            None => format!("{address:#x}"),
        }
    }

    /// An address of the original's by its name, else as a number.
    fn name_of(&self, address: u64) -> String {
        self.bin.label(address).unwrap_or_else(|| format!("{address:#x}"))
    }

    /// A call or a jump (a tail call) to another function than the original's.
    fn target_note(&self, ins: &Instruction, rebuilt: &str, original: u64) -> String {
        let original = self.name_of(original);
        let (verb, through) = match ins.flow_control() {
            FlowControl::Call => ("calls", ""),
            FlowControl::IndirectCall => ("calls", "through "),
            FlowControl::IndirectBranch => ("jumps", "through "),
            _ => ("jumps to", ""),
        };
        format!(
            "call target differs: {verb} {through}{rebuilt} in the rebuild; the original {verb} {through}{original}"
        )
    }

    /// Two table rows lined up.
    fn compare_rows(&self, i: usize, j: usize) -> Option<String> {
        let (a, b) = (&self.a[i], &self.b[j]);
        if a.row == 4 {
            let r = self.reloc_at(b, 0)?;
            return self.check_reloc(i, j, 0, r);
        }
        (self.code[a.at..a.end()] != self.func.code[b.at..b.end()])
            .then(|| "jump table differs: the index bytes differ (cases grouped differently)".to_string())
    }

    /// Two instructions of the same shape whose bytes differ: which of their numbers do.
    fn explain_keep(&self, i: usize, j: usize) -> String {
        let (x, y) = (&self.a[i], &self.b[j]);
        let (a, b) = (&x.ins, &y.ins);
        if x.is_branch() && a.len() != b.len() {
            let size = |i: &Instruction| {
                if i.is_jcc_short() || i.is_jmp_short() || i.len() == 2 {
                    "short"
                } else {
                    "near"
                }
            };
            return format!(
                "short vs near jump: {} in the original, {} in the rebuild (the code it jumps over is a different length)",
                size(a),
                size(b)
            );
        }
        for op in 0..a.op_count() {
            match a.op_kind(op) {
                k if is_immediate(k) => {
                    let relocated = y.immediate_field().is_some_and(|f| self.reloc_at(y, f).is_some());
                    if !relocated && immediate(a, op) != immediate(b, op) {
                        return self.immediate_note(i, j, op);
                    }
                }
                OpKind::Memory => {
                    let relocated = y.displacement_field().is_some_and(|f| self.reloc_at(y, f).is_some());
                    let (da, db) = (displacement(a, self.bits), displacement(b, self.bits));
                    if !relocated && !a.is_ip_rel_memory_operand() && da != db {
                        let (ma, mb) = (memory_text(a, self.bits), memory_text(b, self.bits));
                        let stack = matches!(
                            a.memory_base(),
                            Register::ESP | Register::RSP | Register::EBP | Register::RBP
                        ) && a.memory_index() == Register::None;
                        return if stack {
                            format!(
                                "stack slot offset differs: {ma} in the original, {mb} in the rebuild (another local or argument)"
                            )
                        } else {
                            format!(
                                "offset differs: {ma} in the original, {mb} in the rebuild (another structure field or array element?)"
                            )
                        };
                    }
                }
                _ => {}
            }
        }
        "encoding differs: the same instruction in other bytes (another assembler, or compiler version?)".into()
    }

    /// Two instructions that differ in one immediate.
    fn immediate_note(&self, i: usize, j: usize, op: u32) -> String {
        let (a, b) = (&self.a[i].ins, &self.b[j].ins);
        let (va, vb) = (number(immediate(a, op)), number(immediate(b, op)));
        let sp = (a.op_count() == 2 && a.op_kind(0) == OpKind::Register)
            .then(|| a.op_register(0))
            .filter(|r| matches!(r, Register::ESP | Register::RSP));
        let sp_name = sp.map(register).unwrap_or_default();
        // After a call, `add esp, N` pops its arguments, and the frame with them in an epilogue.
        let frame_delta = self.frame(true).zip(self.frame(false)).map(|(x, y)| y - x);
        let after_call = i
            .checked_sub(1)
            .is_some_and(|p| self.a[p].row == 0 && self.a[p].ins.flow_control() == FlowControl::Call)
            && frame_delta != Some(immediate(b, op) - immediate(a, op));
        match a.mnemonic() {
            Mnemonic::Ret | Mnemonic::Retf => format!(
                "arguments popped differ: ret {va} in the original, ret {vb} in the rebuild (a stdcall function with other parameters?)"
            ),
            Mnemonic::Sub if sp.is_some() => format!(
                "stack frame size differs: sub {sp_name}, {va} in the original, {vb} in the rebuild (other locals, or locals of other sizes)"
            ),
            Mnemonic::Add if sp.is_some() && after_call => format!(
                "arguments popped differ: add {sp_name}, {va} after the call in the original, {vb} in the rebuild (the call passes other arguments)"
            ),
            Mnemonic::Add if sp.is_some() => format!(
                "stack frame size differs: add {sp_name}, {va} in the original, {vb} in the rebuild (other locals, or locals of other sizes)"
            ),
            Mnemonic::Enter => format!(
                "stack frame size differs: enter {va} in the original, {vb} in the rebuild (other locals, or locals of other sizes)"
            ),
            Mnemonic::And if sp.is_some() => {
                format!("stack alignment differs: and {sp_name}, {va} in the original, {vb} in the rebuild")
            }
            _ => format!("immediate differs: {va} in the original, {vb} in the rebuild"),
        }
    }

    /// The size of one side's stack frame: what its prologue subtracts from the stack pointer.
    fn frame(&self, original: bool) -> Option<i64> {
        let items = if original { &self.a } else { &self.b };
        items.iter().take(16).find_map(|i| {
            let ins = &i.ins;
            (i.row == 0
                && ins.mnemonic() == Mnemonic::Sub
                && ins.op_count() == 2
                && ins.op_kind(0) == OpKind::Register
                && matches!(ins.op_register(0), Register::ESP | Register::RSP)
                && is_immediate(ins.op_kind(1)))
            .then(|| immediate(ins, 1))
        })
    }

    /// Two instructions of different shapes in the same place: how they differ.
    fn explain_pair(&self, i: usize, j: usize) -> String {
        let (x, y) = (&self.a[i], &self.b[j]);
        if x.row != 0 || y.row != 0 {
            return "instruction differs".into();
        }
        let (a, b) = (&x.ins, &y.ins);
        if a.mnemonic() == b.mnemonic() && a.op_count() == b.op_count() {
            let mut registers = None;
            for op in 0..a.op_count() {
                let (ka, kb) = (a.op_kind(op), b.op_kind(op));
                let (ra, rb) = (ka == OpKind::Register, kb == OpKind::Register);
                let (ia, ib) = (is_immediate(ka), is_immediate(kb));
                let (ma, mb) = (ka == OpKind::Memory, kb == OpKind::Memory);
                if ra && rb && a.op_register(op) != b.op_register(op) {
                    registers.get_or_insert((register(a.op_register(op)), register(b.op_register(op))));
                } else if ra && ib {
                    return "uses a register in the original, a constant in the rebuild (a variable became a constant?)"
                        .into();
                } else if ia && rb {
                    return "uses a constant in the original, a register in the rebuild (a constant became a variable?)"
                        .into();
                } else if ma && ib {
                    return "reads memory in the original, a constant in the rebuild (a variable became a constant?)"
                        .into();
                } else if ia && mb {
                    return "a constant in the original, reads memory in the rebuild (a constant became a variable?)"
                        .into();
                } else if ma && rb {
                    return format!(
                        "memory vs register: {} in the original, {} in the rebuild (a variable kept in a register rather than in memory?)",
                        memory_text(a, self.bits),
                        register(b.op_register(op))
                    );
                } else if ra && mb {
                    return format!(
                        "memory vs register: {} in the original, {} in the rebuild (a variable kept in memory rather than in a register?)",
                        register(a.op_register(op)),
                        memory_text(b, self.bits)
                    );
                } else if ma && mb {
                    if a.memory_size() != b.memory_size() {
                        return format!(
                            "operand size differs: {} in the original, {} in the rebuild (a variable of another type?)",
                            size_name(a.memory_size()),
                            size_name(b.memory_size())
                        );
                    }
                    if (a.memory_base(), a.memory_index(), a.memory_index_scale())
                        != (b.memory_base(), b.memory_index(), b.memory_index_scale())
                    {
                        registers.get_or_insert((memory_text(a, self.bits), memory_text(b, self.bits)));
                    }
                }
            }
            if let Some((ra, rb)) = registers {
                return format!(
                    "registers differ: {ra} in the original, {rb} in the rebuild (the compiler allocated them differently)"
                );
            }
        }
        let (ma, mb) = (mnemonic(a), mnemonic(b));
        let signed = |m: Mnemonic| match m {
            Mnemonic::Movsx | Mnemonic::Sar | Mnemonic::Imul | Mnemonic::Idiv => Some(true),
            Mnemonic::Movzx | Mnemonic::Shr | Mnemonic::Mul | Mnemonic::Div => Some(false),
            _ => None,
        };
        let family = |m: Mnemonic| match m {
            Mnemonic::Movsx | Mnemonic::Movzx => 1,
            Mnemonic::Sar | Mnemonic::Shr => 2,
            Mnemonic::Imul | Mnemonic::Mul => 3,
            Mnemonic::Idiv | Mnemonic::Div => 4,
            _ => 0,
        };
        let (fa, fb) = (family(a.mnemonic()), family(b.mnemonic()));
        if fa != 0 && fa == fb && signed(a.mnemonic()) != signed(b.mnemonic()) {
            return format!(
                "signedness differs: {ma} in the original, {mb} in the rebuild (a signed type where the other has an unsigned one?)"
            );
        }
        let (ca, cb) = (a.condition_code(), b.condition_code());
        if ca != ConditionCode::None
            && cb != ConditionCode::None
            && condition_family(&ma, ca) == condition_family(&mb, cb)
        {
            use ConditionCode as C;
            let signedness = matches!(
                (ca, cb),
                (C::l, C::b)
                    | (C::b, C::l)
                    | (C::ge, C::ae)
                    | (C::ae, C::ge)
                    | (C::le, C::be)
                    | (C::be, C::le)
                    | (C::g, C::a)
                    | (C::a, C::g)
            );
            let inverted = matches!(
                (ca, cb),
                (C::o, C::no)
                    | (C::no, C::o)
                    | (C::b, C::ae)
                    | (C::ae, C::b)
                    | (C::e, C::ne)
                    | (C::ne, C::e)
                    | (C::be, C::a)
                    | (C::a, C::be)
                    | (C::s, C::ns)
                    | (C::ns, C::s)
                    | (C::p, C::np)
                    | (C::np, C::p)
                    | (C::l, C::ge)
                    | (C::ge, C::l)
                    | (C::le, C::g)
                    | (C::g, C::le)
            );
            return if signedness {
                format!(
                    "signedness differs: {ma} in the original, {mb} in the rebuild (a signed comparison where the other is unsigned?)"
                )
            } else if inverted {
                format!(
                    "condition inverted: {ma} in the original, {mb} in the rebuild (the branches of an if swapped, or the test negated?)"
                )
            } else {
                format!("condition differs: {ma} in the original, {mb} in the rebuild (another comparison: < for <=?)")
            };
        }
        "instruction differs".into()
    }

    /// An item only one side has.
    fn alone(&self, index: usize, moved: bool, original: bool) -> String {
        let (side, other) = if original {
            ("original", "rebuild")
        } else {
            ("rebuild", "original")
        };
        if moved {
            return format!("reordered: this instruction is elsewhere in the {other}");
        }
        let item = if original { &self.a[index] } else { &self.b[index] };
        if item.row == 0 && item.ins.mnemonic() == Mnemonic::Nop {
            return format!(
                "alignment padding differs: a nop in the {side} only (the code before it is a different length)"
            );
        }
        if original {
            "missing in the rebuild".into()
        } else {
            "extra in the rebuild".into()
        }
    }

    fn original_text(&self, text: &mut Text, i: usize) -> String {
        let item = &self.a[i];
        let bytes = &self.code[item.at..item.end()];
        if item.row != 0 {
            return row_text(item, bytes, |v| self.bin.label(v));
        }
        let ins = &item.ins;
        let mut names = Names::default();
        for op in 0..ins.op_count() {
            match ins.op_kind(op) {
                k if is_branch_kind(k) => names.branch = self.bin.label(ins.near_branch_target()),
                OpKind::Memory if ins.is_ip_rel_memory_operand() => {
                    names.memory = self.bin.label(ins.ip_rel_memory_address())
                }
                // A global, or a table of them: an address, not a field's offset.
                OpKind::Memory => {
                    let v = displacement(ins, self.bits);
                    names.memory = (v >= 0x10000).then(|| self.bin.label(v as u64)).flatten();
                }
                // A constant that is an address: a string's, a function's.
                k if is_immediate(k) => {
                    let v = immediate(ins, op);
                    names.immediate = (v >= 0x10000).then(|| self.bin.label(v as u64)).flatten();
                }
                _ => {}
            }
        }
        text.format(ins, names)
    }

    fn rebuilt_text(&self, text: &mut Text, j: usize) -> String {
        let item = &self.b[j];
        let code = &self.func.code;
        let name = |at: usize, r: &Reloc| match target_in_function(self.func, r, item.at + at, item.end()) {
            Some(t) => format!("{}+{t:#x}", self.name),
            None => with_offset(&r.symbol, reference_offset(code, r, item.at + at, item.end())),
        };
        if item.row != 0 {
            let bytes = &code[item.at..item.end()];
            return match self.reloc_at(item, 0) {
                Some(r) if item.row == 4 => format!("dd {}", name(0, r)),
                _ => row_text(item, bytes, |_| None),
            };
        }
        let mut names = Names::default();
        for (at, r) in self.relocs_in(item) {
            let n = name(at, r);
            if Some(at) == item.displacement_field() {
                names.memory = Some(n);
            } else if Some(at) == item.immediate_field() {
                if item.is_branch() {
                    names.branch = Some(n);
                } else {
                    names.immediate = Some(n);
                }
            }
        }
        match self.unrelocated_target(j) {
            Some(Ok(t)) if item.is_branch() => names.branch = Some(format!("{}+{t:#x}", self.name)),
            Some(Ok(t)) => names.memory = Some(format!("{}+{t:#x}", self.name)),
            Some(Err(section_offset)) => {
                let n = self.neighbour(section_offset).map(|(n, o)| with_offset(n, o));
                if item.is_branch() {
                    names.branch = n;
                } else {
                    names.memory = n;
                }
            }
            None => {}
        }
        text.format(&item.ins, names)
    }
}

/// `name`, or `name+0x4`.
fn with_offset(name: &str, offset: i64) -> String {
    match offset {
        0 => name.to_string(),
        o if o < 0 => format!("{name}-{:#x}", -o),
        o => format!("{name}+{o:#x}"),
    }
}

/// A table row as text: `dd 0x401234` (named when `label` knows it) or `db 0, 1, 1, 2`.
fn row_text(item: &Item, bytes: &[u8], label: impl Fn(u64) -> Option<String>) -> String {
    if item.row == 4 && bytes.len() == 4 {
        let v = u64::from(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        return format!("dd {}", label(v).unwrap_or_else(|| format!("{v:#x}")));
    }
    let list: Vec<String> = bytes.iter().map(u8::to_string).collect();
    format!("db {}", list.join(", "))
}

/// The value of an immediate operand, sign-extended.
fn immediate(ins: &Instruction, op: u32) -> i64 {
    let v = ins.immediate(op);
    match ins.op_kind(op) {
        OpKind::Immediate8 | OpKind::Immediate8_2nd => v as u8 as i8 as i64,
        OpKind::Immediate16 | OpKind::Immediate8to16 => v as u16 as i16 as i64,
        OpKind::Immediate32 | OpKind::Immediate8to32 => v as u32 as i32 as i64,
        _ => v as i64,
    }
}

/// A memory operand's displacement, sign-extended.
fn displacement(ins: &Instruction, bits: u32) -> i64 {
    if bits == 64 {
        ins.memory_displacement64() as i64
    } else {
        ins.memory_displacement32() as i32 as i64
    }
}

/// `0x10`, `-0x8`.
fn number(v: i64) -> String {
    if v < 0 {
        format!("-{:#x}", -(v as i128))
    } else {
        format!("{v:#x}")
    }
}

fn register(r: Register) -> String {
    format!("{r:?}").to_lowercase()
}

fn mnemonic(ins: &Instruction) -> String {
    format!("{:?}", ins.mnemonic()).to_lowercase()
}

/// `[esp+0x8]`, `[eax+ecx*4-0x10]`.
fn memory_text(ins: &Instruction, bits: u32) -> String {
    let mut out = String::from("[");
    if ins.memory_base() != Register::None {
        out.push_str(&register(ins.memory_base()));
    }
    if ins.memory_index() != Register::None {
        if out.len() > 1 {
            out.push('+');
        }
        out.push_str(&register(ins.memory_index()));
        if ins.memory_index_scale() > 1 {
            out.push_str(&format!("*{}", ins.memory_index_scale()));
        }
    }
    let d = displacement(ins, bits);
    if d != 0 || out.len() == 1 {
        if out.len() > 1 && d >= 0 {
            out.push('+');
        }
        out.push_str(&number(d));
    }
    out.push(']');
    out
}

/// What a memory operand's size says about the variable: `byte (char)`.
fn size_name(size: MemorySize) -> String {
    match size {
        MemorySize::Float32 => "float".into(),
        MemorySize::Float64 => "double".into(),
        MemorySize::Float80 => "long double".into(),
        s => match s.size() {
            1 => "byte (char)".into(),
            2 => "word (short)".into(),
            4 => "dword (int, or a 32-bit pointer)".into(),
            8 => "qword (long long, or a 64-bit pointer)".into(),
            n => format!("{n} bytes"),
        },
    }
}

/// `j`, `set` or `cmov`: which instructions with a condition these are.
fn condition_family(mnemonic: &str, cc: ConditionCode) -> String {
    let cc = format!("{cc:?}");
    mnemonic.strip_suffix(cc.as_str()).unwrap_or(mnemonic).to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::matching::{ObjectIsa, object_functions, source_names, undecorated};

    /// A section: name, type, flags, address, bytes, link, info, entry size.
    type Section<'a> = (&'a str, u32, u32, u32, Vec<u8>, u32, u32, u32);

    /// An ELF32 file for i386 (`kind` 2: an executable, 1: an object) with
    /// the given sections. An executable loads its first 0x1000 bytes at
    /// 0x401000, where a section with an address is placed.
    fn elf32(kind: u16, sections: &[Section]) -> Vec<u8> {
        let header = |fields: [u32; 10]| {
            let mut h = [0u8; 40];
            for (i, v) in fields.iter().enumerate() {
                h[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
            }
            h
        };
        let mut out = vec![0u8; 0x100];
        let mut shstr = b"\0".to_vec();
        let mut headers = vec![[0u8; 40]];
        for (name, typ, flags, addr, bytes, link, info, entsize) in sections {
            let at = if *addr != 0 {
                (addr - 0x401000) as usize
            } else {
                out.len().next_multiple_of(4)
            };
            if out.len() < at + bytes.len() {
                out.resize(at + bytes.len(), 0);
            }
            out[at..at + bytes.len()].copy_from_slice(bytes);
            let size = bytes.len() as u32;
            headers.push(header([
                shstr.len() as u32,
                *typ,
                *flags,
                *addr,
                at as u32,
                size,
                *link,
                *info,
                4,
                *entsize,
            ]));
            shstr.extend(name.as_bytes());
            shstr.push(0);
        }
        // The section names' own section, last.
        let shstrndx = headers.len();
        let name = shstr.len() as u32;
        shstr.extend(b".shstrtab\0");
        let at = out.len().next_multiple_of(4);
        out.resize(at, 0);
        out.extend(&shstr);
        headers.push(header([name, 3, 0, 0, at as u32, shstr.len() as u32, 0, 0, 1, 0]));
        let shoff = out.len().next_multiple_of(4);
        out.resize(shoff, 0);
        for h in &headers {
            out.extend(h);
        }
        out[..8].copy_from_slice(&[0x7F, b'E', b'L', b'F', 1, 1, 1, 0]);
        out[16..18].copy_from_slice(&kind.to_le_bytes());
        out[18..20].copy_from_slice(&3u16.to_le_bytes());
        out[20..24].copy_from_slice(&1u32.to_le_bytes());
        out[32..36].copy_from_slice(&(shoff as u32).to_le_bytes());
        out[40..42].copy_from_slice(&52u16.to_le_bytes());
        out[46..48].copy_from_slice(&40u16.to_le_bytes());
        out[48..50].copy_from_slice(&(headers.len() as u16).to_le_bytes());
        out[50..52].copy_from_slice(&(shstrndx as u16).to_le_bytes());
        if kind == 2 {
            out[24..28].copy_from_slice(&0x401100u32.to_le_bytes());
            out[28..32].copy_from_slice(&52u32.to_le_bytes());
            out[42..44].copy_from_slice(&32u16.to_le_bytes());
            out[44..46].copy_from_slice(&1u16.to_le_bytes());
            for (i, v) in [1u32, 0, 0x401000, 0x401000, 0x1000, 0x1000, 7, 0x1000]
                .iter()
                .enumerate()
            {
                out[52 + 4 * i..56 + 4 * i].copy_from_slice(&v.to_le_bytes());
            }
            // The segment's bytes are all in the file.
            out.resize(out.len().max(0x1000), 0);
        }
        out
    }

    /// A symbol table and its strings: (name, value, size, info, section).
    fn symtab(symbols: &[(&str, u32, u32, u8, u16)]) -> (Vec<u8>, Vec<u8>) {
        let (mut table, mut strings) = (vec![0u8; 16], b"\0".to_vec());
        for (name, value, size, info, section) in symbols {
            let mut s = [0u8; 16];
            s[0..4].copy_from_slice(&(strings.len() as u32).to_le_bytes());
            s[4..8].copy_from_slice(&value.to_le_bytes());
            s[8..12].copy_from_slice(&size.to_le_bytes());
            s[12] = *info;
            s[14..16].copy_from_slice(&section.to_le_bytes());
            table.extend(s);
            strings.extend(name.as_bytes());
            strings.push(0);
        }
        (table, strings)
    }

    /// An i386 executable: `code` at 0x401100 in `.text` with its functions
    /// (name, address, size), and two words of `.data` at 0x401800, `g` and `g2`.
    pub(crate) fn image(code: &[u8], functions: &[(&str, u32, u32)]) -> Binary {
        let mut symbols: Vec<(&str, u32, u32, u8, u16)> =
            functions.iter().map(|&(n, a, s)| (n, a, s, 0x12, 1)).collect();
        symbols.extend([("g", 0x401800, 4, 0x11, 2), ("g2", 0x401804, 4, 0x11, 2)]);
        let (table, strings) = symtab(&symbols);
        let file = elf32(
            2,
            &[
                (".text", 1, 6, 0x401100, code.to_vec(), 0, 0, 0),
                (".data", 1, 3, 0x401800, vec![0; 8], 0, 0, 0),
                (".symtab", 2, 0, 0, table, 4, 1, 16),
                (".strtab", 3, 0, 0, strings, 0, 0, 0),
            ],
        );
        Binary::parse(file).unwrap()
    }

    /// An i386 object: `.text` holding `code` as the function `name`, with
    /// relocations (offset, `R_386_32` 1 or `R_386_PC32` 2, symbol; `.text`
    /// for the section's own symbol).
    pub(crate) fn object(name: &str, code: &[u8], relocs: &[(u32, u32, &str)]) -> Vec<u8> {
        let mut symbols = vec![(".text", 0, 0, 0x03, 1), (name, 0, code.len() as u32, 0x12, 1)];
        for (_, _, s) in relocs {
            if !symbols.iter().any(|x| x.0 == *s) {
                symbols.push((s, 0, 0, 0x10, 0));
            }
        }
        let (mut table, strings) = symtab(&symbols);
        // A section symbol has no name of its own.
        table[16..20].copy_from_slice(&0u32.to_le_bytes());
        let mut rel = Vec::new();
        for (at, kind, s) in relocs {
            let sym = symbols.iter().position(|x| x.0 == *s).unwrap() as u32 + 1;
            rel.extend(at.to_le_bytes());
            rel.extend(((sym << 8) | kind).to_le_bytes());
        }
        elf32(
            1,
            &[
                (".text", 1, 6, 0, code.to_vec(), 0, 0, 0),
                (".rel.text", 9, 0, 0, rel, 3, 1, 8),
                (".symtab", 2, 0, 0, table, 4, 2, 16),
                (".strtab", 3, 0, 0, strings, 0, 0, 0),
            ],
        )
    }

    /// `f` at 0x401100: a frame, a byte field read, a comparison, a call to
    /// `helper` skipped by a branch, a global updated.
    const F: [u8; 37] = [
        0x55, // push ebp
        0x89, 0xE5, // mov ebp, esp
        0x83, 0xEC, 0x10, // sub esp, 0x10
        0x8B, 0x45, 0x08, // mov eax, [ebp+8]
        0x0F, 0xB6, 0x48, 0x0A, // movzx ecx, byte ptr [eax+0xa]
        0x3B, 0x4D, 0x0C, // cmp ecx, [ebp+0xc]
        0x7C, 0x09, // jl 0x40111b
        0x51, // push ecx
        0xE8, 0x68, 0x00, 0x00, 0x00, // call 0x401180 (helper)
        0x83, 0xC4, 0x04, // add esp, 4
        0x01, 0x05, 0x00, 0x18, 0x40, 0x00, // add [0x401800], eax (g)
        0x89, 0xEC, // mov esp, ebp
        0x5D, // pop ebp
        0xC3, // ret
    ];

    /// The program: `f`, then `helper` at 0x401180 and `helper2` at 0x401190.
    fn program() -> Binary {
        let mut code = F.to_vec();
        code.resize(0x80, 0xCC);
        code.extend([0xB8, 1, 0, 0, 0, 0xC3]);
        code.resize(0x90, 0xCC);
        code.extend([0xB8, 2, 0, 0, 0, 0xC3]);
        image(
            &code,
            &[("f", 0x401100, 37), ("helper", 0x401180, 6), ("helper2", 0x401190, 6)],
        )
    }

    /// `F` as the compiler's object has it: the call and the global relocated.
    fn rebuilt(code: &[u8], call: &str, global: &str) -> ObjectFunction {
        let call_at = code.iter().position(|&b| b == 0xE8).unwrap() as u32 + 1;
        let global_at = code.windows(2).position(|w| w == [0x01, 0x05]).unwrap() as u32 + 2;
        let mut code = code.to_vec();
        code[call_at as usize..call_at as usize + 4].copy_from_slice(&(-4i32).to_le_bytes());
        code[global_at as usize..global_at as usize + 4].copy_from_slice(&[0; 4]);
        let obj = object("_f", &code, &[(call_at, 2, call), (global_at, 1, global)]);
        let mut funcs = object_functions(&obj).unwrap();
        assert_eq!(funcs.len(), 1);
        funcs.remove(0)
    }

    fn notes(m: &MatchResult) -> Vec<&str> {
        m.lines.iter().filter_map(|l| l.note.as_deref()).collect()
    }

    #[test]
    fn the_same_code_matches_whatever_the_linker_filled_in() {
        let bin = program();
        let f = rebuilt(&F, "_helper", "_g");
        assert_eq!(
            (f.name.as_str(), f.isa, f.relocs.len()),
            ("_f", ObjectIsa::X86 { bits: 32 }, 2)
        );
        let m = bin.match_function(0x401100, &f).unwrap();
        assert_eq!(
            (m.percent, m.matched_instructions, m.original_bytes),
            (100.0, 14, 37),
            "{}",
            m.to_text()
        );
        // Found by its C name, the object's decorated one undecorated.
        assert_eq!(bin.object_symbol_address("_f"), Some(0x401100));
        assert_eq!(bin.match_object(&object("_f", &F, &[])).unwrap().len(), 1);
    }

    #[test]
    fn each_difference_is_explained() {
        let bin = program();
        let mut code = F.to_vec();
        code[5] = 0x18; // sub esp, 0x18
        code[8] = 0x0C; // mov eax, [ebp+0xc]
        code[10] = 0xBE; // movsx
        code[16] = 0x7D; // jge
        let f = rebuilt(&code, "_helper2", "_g2");
        let m = bin.match_function(0x401100, &f).unwrap();
        let notes = notes(&m);
        let has = |what: &str| notes.iter().any(|n| n.starts_with(what));
        assert!(
            has("stack frame size differs: sub esp, 0x10 in the original, 0x18"),
            "{notes:?}"
        );
        assert!(
            has("stack slot offset differs: [ebp+0x8] in the original, [ebp+0xc] in the rebuild"),
            "{notes:?}"
        );
        assert!(has("signedness differs: movzx in the original, movsx"), "{notes:?}");
        assert!(has("condition inverted: jl in the original, jge"), "{notes:?}");
        assert!(
            has("call target differs: calls _helper2 in the rebuild; the original calls helper"),
            "{notes:?}"
        );
        assert!(
            has("global differs: _g2 in the rebuild; the original refers to 0x401800 (g)"),
            "{notes:?}"
        );
        assert_eq!(m.matched_instructions, 8, "{}", m.to_text());
        let kinds: Vec<&str> = m.differences.iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            kinds.contains(&"call target differs") && kinds.contains(&"signedness differs"),
            "{kinds:?}"
        );
    }

    #[test]
    fn a_branch_is_compared_by_where_it_lands() {
        let bin = program();
        // One more instruction in the code the branch skips: its displacement
        // is one more, and it still lands on the instruction lined up with
        // the original's target.
        let mut code = F.to_vec();
        code[17] = 0x0A;
        code.insert(24, 0x41); // inc ecx, after the call
        let f = rebuilt(&code, "_helper", "_g");
        let m = bin.match_function(0x401100, &f).unwrap();
        assert_eq!(notes(&m), ["extra in the rebuild"], "{}", m.to_text());
        assert_eq!(m.matched_instructions, 14);
        // Landing elsewhere is a difference; so is a near jump for a short one.
        let mut code = F.to_vec();
        code[17] = 0x06; // jl to the add esp
        let m = bin.match_function(0x401100, &rebuilt(&code, "_helper", "_g")).unwrap();
        assert!(
            notes(&m)[0].starts_with("branch target differs: to f+0x1b in the original"),
            "{:?}",
            notes(&m)
        );
        let mut code = F.to_vec();
        code.splice(16..18, [0x0F, 0x8C, 0x09, 0, 0, 0]);
        let m = bin.match_function(0x401100, &rebuilt(&code, "_helper", "_g")).unwrap();
        assert!(
            notes(&m)[0].starts_with("short vs near jump: short in the original, near"),
            "{:?}",
            notes(&m)
        );
    }

    #[test]
    fn instructions_in_another_order_are_reordered() {
        // mov eax, [ebp+8]; mov ecx, [ebp+0xc]; add eax, ecx; ret, and the two loads swapped.
        let code = [0x8B, 0x45, 0x08, 0x8B, 0x4D, 0x0C, 0x01, 0xC8, 0xC3];
        let bin = image(&code, &[("add2", 0x401100, 9)]);
        let swapped = [0x8B, 0x4D, 0x0C, 0x8B, 0x45, 0x08, 0x01, 0xC8, 0xC3];
        let f = object_functions(&object("add2", &swapped, &[])).unwrap().remove(0);
        let m = bin.match_function(0x401100, &f).unwrap();
        let notes = notes(&m);
        assert_eq!(notes.len(), 2, "{}", m.to_text());
        assert!(notes.iter().all(|n| n.starts_with("reordered")), "{notes:?}");
        assert_eq!(m.differences, [("reordered".to_string(), 2)]);
    }

    #[test]
    fn a_jump_table_in_the_code_is_compared_by_its_cases() {
        // switch (x) { case 0: 10; case 1: 20; case 2: 30; default: 0 }, its table after the code.
        let mut code = vec![
            0x8B, 0x44, 0x24, 0x04, // mov eax, [esp+4]
            0x83, 0xF8, 0x02, // cmp eax, 2
            0x77, 0x19, // ja 0x401122
            0xFF, 0x24, 0x85, 0x28, 0x11, 0x40, 0x00, // jmp [eax*4+0x401128]
            0xB8, 0x0A, 0, 0, 0, 0xC3, // 0x401110: mov eax, 10; ret
            0xB8, 0x14, 0, 0, 0, 0xC3, // 0x401116
            0xB8, 0x1E, 0, 0, 0, 0xC3, // 0x40111c
            0x31, 0xC0, 0xC3, // 0x401122: xor eax, eax; ret
            0x90, 0x90, 0x90, // padding up to the table
        ];
        for case in [0x401110u32, 0x401116, 0x40111C] {
            code.extend(case.to_le_bytes());
        }
        let bin = image(&code, &[("sw", 0x401100, code.len() as u32)]);
        // The object's table and the jump into it are relocated against its section.
        let rebuild = |cases: [u32; 3]| {
            let mut obj = code.clone();
            obj[12..16].copy_from_slice(&0x28u32.to_le_bytes());
            for (i, c) in cases.iter().enumerate() {
                obj[0x28 + 4 * i..0x2C + 4 * i].copy_from_slice(&c.to_le_bytes());
            }
            let relocs = [
                (12, 1, ".text"),
                (0x28, 1, ".text"),
                (0x2C, 1, ".text"),
                (0x30, 1, ".text"),
            ];
            object_functions(&object("sw", &obj, &relocs)).unwrap().remove(0)
        };
        let m = bin.match_function(0x401100, &rebuild([0x10, 0x16, 0x1C])).unwrap();
        assert_eq!(m.percent, 100.0, "{}", m.to_text());
        assert!(
            m.lines.iter().any(|l| l.original.as_deref() == Some("dd sw+0x10")),
            "{}",
            m.to_text()
        );
        // Two cases swapped.
        let m = bin.match_function(0x401100, &rebuild([0x16, 0x10, 0x1C])).unwrap();
        let notes = notes(&m);
        assert_eq!(notes.len(), 2, "{}", m.to_text());
        assert!(
            notes[0].starts_with(
                "jump table differs: an entry leads to sw+0x10 in the original, to what is lined up with sw+0x16"
            ),
            "{notes:?}"
        );
    }

    #[test]
    fn decorated_names_as_source_writes_them() {
        assert_eq!(undecorated("_foo"), "foo");
        assert_eq!(undecorated("_foo@8"), "foo");
        assert_eq!(undecorated("@foo@8"), "foo");
        assert_eq!(undecorated("foo@@16"), "foo");
        assert_eq!(undecorated("@feat.00"), "@feat.00");
        assert_eq!(source_names("__imp__Sleep@4"), ["__imp__Sleep@4", "__imp_Sleep"]);
        assert_eq!(
            source_names("?scaled@Shape@@QBEHH@Z"),
            ["?scaled@Shape@@QBEHH@Z", "Shape::scaled"]
        );
        assert_eq!(
            source_names("_ZNK5Shape6scaledEi"),
            ["_ZNK5Shape6scaledEi", "Shape::scaled"]
        );
        // String literals all demangle alike: never looked up by that.
        assert_eq!(source_names("??_C@_05ABCDEFGH@hello?$AA@").len(), 1);
    }

    #[test]
    fn padding_is_not_code() {
        // mov eax, 1; ret; then int3 and nop padding.
        let code = [0xB8, 1, 0, 0, 0, 0xC3, 0xCC, 0xCC, 0x90, 0x0F, 0x1F, 0x00];
        assert_eq!(code_len(&code, 32), 6);
        assert!(padding_only(&code[6..], 32));
        assert!(!padding_only(&code[5..], 32));
        let _ = ObjectIsa::X86 { bits: 32 };
    }
}
