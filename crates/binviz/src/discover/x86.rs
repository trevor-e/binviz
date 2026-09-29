//! x86 code followed from where it is known to start — the entry point,
//! exports, TLS callbacks, exception handlers — the way a disassembler maps
//! a binary that doesn't list its functions (a 32-bit PE has no unwind
//! tables to find them in):
//!
//! 1. Each path from a start is followed: branches both ways, jumps, the
//!    cases of a jump table, and the code after a call when the callee can
//!    return (`ExitProcess`, `exit` and the functions that only end in them
//!    can't). Each call's target starts a function.
//! 2. The code found is cut into functions: each runs from its start
//!    towards the next one, and a jump out of that range is a tail call
//!    (MSVC's `__finally` blocks, called in place and branched over, stay in
//!    their functions).
//! 3. What nothing reached is looked at last: addresses the image holds
//!    (vtables, tables of callbacks, `push offset f`), then the start of each
//!    stretch still unexplored, become functions when the code there reads
//!    as code — unless they turn out to be the cases of a switch found
//!    later — and what those call is followed in turn.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use iced_x86::{Decoder, DecoderOptions, FlowControl, Instruction, MemorySize, Mnemonic, OpKind, Register};

/// Loaded bytes of the image: code to follow, or data to read tables and pointers from.
pub(crate) struct Region<'a> {
    pub address: u64,
    pub bytes: &'a [u8],
    pub code: bool,
}

/// What following the code needs to know about the image.
pub(crate) struct Image<'a> {
    /// 32 or 64.
    pub bits: u32,
    /// Sorted by address, not overlapping.
    pub regions: Vec<Region<'a>>,
    /// Where functions are known to start.
    pub starts: Vec<u64>,
    /// Where the loader relocates an address (PE base relocations), sorted.
    /// With these, a word of the image is an address exactly when it is
    /// relocated; without, any word that falls inside the code may be one.
    pub relocations: Option<Vec<u64>>,
    /// Import address table slots, and whether what each one imports never returns.
    pub slots: HashMap<u64, bool>,
}

/// A table code reads its jump target from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Table {
    /// The indirect jump that reads it.
    pub jump: u64,
    pub address: u64,
    /// Bytes per entry: 4 for addresses, 1 for the indexes MSVC picks an address with.
    pub entry: u8,
    pub count: u32,
}

impl Table {
    pub fn end(&self) -> u64 {
        self.address + self.entry as u64 * self.count as u64
    }
}

/// What following the code found.
#[derive(Debug, Default)]
pub(crate) struct Found {
    /// Functions: (start, size), sorted.
    pub functions: Vec<(u64, u64)>,
    /// Jump tables and their index tables, sorted by address.
    pub tables: Vec<Table>,
}

/// Where control goes after an instruction that doesn't just go on to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    /// A call, to a known or a computed address.
    Call(Option<u64>),
    /// A call through an import address table slot.
    CallSlot(u64),
    /// A jump, to a known or a computed address.
    Jump(Option<u64>),
    /// A jump through an import address table slot (an import thunk, a tail call).
    JumpSlot(u64),
    /// A jump through the table `switches[n]`.
    Switch(u32),
    Branch(u64),
    Return,
    /// `int3`, `ud2`, `hlt`, a far jump: execution doesn't go on.
    Stop,
}

/// Functions as far as their starts go: each one's (start, end), and the
/// jumps that leave one, (from, to).
struct Bodies {
    extents: Vec<(u64, u64)>,
    exits: Vec<(u64, u64)>,
}

/// A jump table and what it holds.
struct Switch {
    table: Table,
    index: Option<Table>,
    targets: Vec<u64>,
}

/// The per-byte state of code: the length of the instruction decoded there
/// (0 where none starts), whether it doesn't just go on to the next one,
/// whether it has been followed, and whether it is part of a table instead.
const LEN: u8 = 0x0F;
const DATA: u8 = 0x20;
const FLOW: u8 = 0x40;
const FOLLOWED: u8 = 0x80;

/// Whether a function can return, as far as has been worked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Returns {
    Yes,
    No,
    /// Being worked out: a recursive call is taken to return.
    Pending,
}

/// At most this many entries per table.
const MAX_TABLE: usize = 1024;
/// How deep "does this function return?" looks through calls before assuming it does.
const MAX_DEPTH: u32 = 48;

struct Code<'a> {
    address: u64,
    bytes: &'a [u8],
    state: Vec<u8>,
    decoder: Decoder<'a>,
}

struct Follower<'a> {
    bits: u32,
    regions: &'a [Region<'a>],
    code: Vec<Code<'a>>,
    flows: HashMap<u64, Flow>,
    switches: Vec<Switch>,
    relocations: Option<&'a [u64]>,
    slots: &'a HashMap<u64, bool>,
    starts: BTreeSet<u64>,
    queue: Vec<u64>,
    returns: HashMap<u64, Returns>,
    depth: u32,
    /// Code addresses instructions name (`push offset f`): functions, perhaps.
    named: Vec<u64>,
    /// Addresses that didn't read as code.
    rejected: HashSet<u64>,
    /// Starts taken because the image names them (not called, not a known start).
    adopted: HashSet<u64>,
}

/// Follows the code of an image and cuts it into functions.
pub(crate) fn follow(image: &Image) -> Found {
    let mut f = Follower::new(image);
    for &s in &image.starts {
        f.add_start(s);
    }
    f.explore();
    let mut extents = f.partition();
    // Addresses the image holds that point into code nothing reached, then
    // the start of each stretch of code nothing reached, until neither finds more.
    for _ in 0..64 {
        let candidates = f.candidates(&extents);
        let mut added = f.adopt(candidates, &extents);
        if added == 0 {
            let gaps = f.gap_starts(&extents);
            added = f.adopt(gaps, &extents);
        }
        if added == 0 {
            break;
        }
        f.explore();
        extents = f.partition();
    }
    let mut tables: Vec<Table> = f
        .switches
        .iter()
        .flat_map(|s| std::iter::once(s.table).chain(s.index))
        .collect();
    tables.sort_by_key(|t| (t.address, t.jump));
    tables.dedup_by_key(|t| t.address);
    Found {
        functions: extents.into_iter().map(|(s, e)| (s, e - s)).collect(),
        tables,
    }
}

impl<'a> Follower<'a> {
    fn new(image: &'a Image<'a>) -> Follower<'a> {
        let code = image
            .regions
            .iter()
            .filter(|r| r.code && !r.bytes.is_empty())
            .map(|r| Code {
                address: r.address,
                bytes: r.bytes,
                state: vec![0; r.bytes.len()],
                decoder: Decoder::with_ip(image.bits, r.bytes, r.address, DecoderOptions::NONE),
            })
            .collect();
        Follower {
            bits: image.bits,
            regions: &image.regions,
            code,
            flows: HashMap::new(),
            switches: Vec::new(),
            relocations: image.relocations.as_deref(),
            slots: &image.slots,
            starts: BTreeSet::new(),
            queue: Vec::new(),
            returns: HashMap::new(),
            depth: 0,
            named: Vec::new(),
            rejected: HashSet::new(),
            adopted: HashSet::new(),
        }
    }

    /// The code region holding `a`, and `a`'s offset in it.
    fn code_at(&self, a: u64) -> Option<(usize, usize)> {
        let i = self.code.partition_point(|c| c.address <= a).checked_sub(1)?;
        let c = &self.code[i];
        let off = a - c.address;
        (off < c.bytes.len() as u64).then_some((i, off as usize))
    }

    fn is_code(&self, a: u64) -> bool {
        self.code_at(a).is_some()
    }

    /// `n` bytes of the image at `a`, from any region.
    fn read(&self, a: u64, n: usize) -> Option<&'a [u8]> {
        let i = self.regions.partition_point(|r| r.address <= a).checked_sub(1)?;
        let r = &self.regions[i];
        let off = usize::try_from(a - r.address).ok()?;
        r.bytes.get(off..off.checked_add(n)?)
    }

    fn read_word(&self, a: u64) -> Option<u64> {
        if self.bits == 64 {
            self.read(a, 8)
                .map(|b| u64::from_le_bytes(b.try_into().expect("8 bytes")))
        } else {
            self.read_u32(a)
        }
    }

    fn read_u32(&self, a: u64) -> Option<u64> {
        self.read(a, 4)
            .map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")) as u64)
    }

    fn relocated(&self, a: u64) -> bool {
        self.relocations.is_some_and(|r| r.binary_search(&a).is_ok())
    }

    /// The length of the instruction decoded at `a`, if one was.
    fn len_at(&self, a: u64) -> Option<u64> {
        let (c, off) = self.code_at(a)?;
        let n = self.code[c].state[off] & LEN;
        (n != 0).then_some(n as u64)
    }

    /// Whether `a` is inside (not at the start of) an instruction decoded before.
    fn inside_instruction(&self, c: usize, off: usize) -> bool {
        let state = &self.code[c].state;
        (1..15.min(off + 1)).any(|k| (state[off - k] & LEN) as usize > k)
    }

    /// Decodes the instruction at `pc` and remembers it, unless `pc` isn't in
    /// code, the bytes there don't decode, or the instruction would overlap
    /// one decoded before at another boundary.
    fn decode(&mut self, pc: u64) -> Option<Instruction> {
        let (c, off) = self.code_at(pc)?;
        let known = self.code[c].state[off] & LEN != 0;
        if !known && (self.code[c].state[off] & DATA != 0 || self.inside_instruction(c, off)) {
            return None;
        }
        let code = &mut self.code[c];
        code.decoder.set_position(off).ok()?;
        code.decoder.set_ip(pc);
        let mut ins = Instruction::default();
        code.decoder.decode_out(&mut ins);
        if ins.is_invalid() {
            return None;
        }
        if !known {
            let n = ins.len();
            if (1..n).any(|k| code.state.get(off + k).is_some_and(|&s| s & (LEN | DATA) != 0)) {
                return None;
            }
            code.state[off] = n as u8;
            if let Some(flow) = self.flow_of(&ins) {
                self.flows.insert(pc, flow);
                self.code[c].state[off] |= FLOW;
            }
        }
        Some(ins)
    }

    /// The length of the instruction at `pc` and where it goes, decoding it if need be.
    fn step(&mut self, pc: u64) -> Option<(u64, Option<Flow>)> {
        let (c, off) = self.code_at(pc)?;
        let s = self.code[c].state[off];
        let len = if s & LEN != 0 {
            (s & LEN) as u64
        } else {
            self.decode(pc)?.len() as u64
        };
        let flow = if self.code[c].state[off] & FLOW != 0 {
            self.flows.get(&pc).copied()
        } else {
            None
        };
        Some((len, flow))
    }

    fn flow_of(&mut self, ins: &Instruction) -> Option<Flow> {
        let target = || match ins.op0_kind() {
            OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64 => Some(ins.near_branch_target()),
            _ => None,
        };
        Some(match ins.flow_control() {
            FlowControl::Next | FlowControl::XbeginXabortXend => {
                if ins.mnemonic() == Mnemonic::Hlt {
                    Flow::Stop
                } else {
                    return None;
                }
            }
            FlowControl::Return => Flow::Return,
            FlowControl::Exception => Flow::Stop,
            // int3 is padding or a breakpoint; int 29h is __fastfail, int 2Ch an assertion.
            FlowControl::Interrupt => match ins.mnemonic() {
                Mnemonic::Int3 => Flow::Stop,
                Mnemonic::Int if matches!(ins.immediate8(), 0x29 | 0x2c) => Flow::Stop,
                _ => return None,
            },
            FlowControl::ConditionalBranch => Flow::Branch(target()?),
            FlowControl::UnconditionalBranch => target().map_or(Flow::Stop, |t| Flow::Jump(Some(t))),
            FlowControl::Call => match target() {
                // call $+5 / pop: code getting its own address.
                Some(t) if t == ins.next_ip() => return None,
                Some(t) => Flow::Call(Some(t)),
                None => Flow::Stop,
            },
            FlowControl::IndirectCall => match self.slot(ins) {
                Some(s) => Flow::CallSlot(s),
                None => Flow::Call(None),
            },
            FlowControl::IndirectBranch => match self.slot(ins) {
                Some(s) => Flow::JumpSlot(s),
                None => match self.switch(ins) {
                    Some(n) => Flow::Switch(n),
                    None => Flow::Jump(None),
                },
            },
        })
    }

    /// The import address table slot an indirect call or jump goes through.
    fn slot(&self, ins: &Instruction) -> Option<u64> {
        if ins.op0_kind() != OpKind::Memory || ins.memory_index() != Register::None {
            return None;
        }
        let slot = if ins.is_ip_rel_memory_operand() {
            ins.ip_rel_memory_address()
        } else if ins.memory_base() == Register::None {
            ins.memory_displacement64()
        } else {
            return None;
        };
        self.slots.contains_key(&slot).then_some(slot)
    }

    /// Whether what an import address table slot imports can return.
    fn slot_returns(&self, slot: u64) -> bool {
        !self.slots.get(&slot).copied().unwrap_or(false)
    }

    fn add_start(&mut self, a: u64) {
        if self.is_code(a) && self.starts.insert(a) {
            self.queue.push(a);
        }
    }

    /// Follows every path from the starts not followed yet.
    fn explore(&mut self) {
        while let Some(start) = self.queue.pop() {
            let mut paths = vec![start];
            while let Some(mut pc) = paths.pop() {
                while let Some((c, off)) = self.code_at(pc) {
                    if self.code[c].state[off] & FOLLOWED != 0 {
                        break;
                    }
                    let Some(ins) = self.decode(pc) else { break };
                    self.code[c].state[off] |= FOLLOWED;
                    self.note_addresses(&ins);
                    let next = ins.next_ip();
                    let flow = if self.code[c].state[off] & FLOW != 0 {
                        self.flows.get(&pc).copied()
                    } else {
                        None
                    };
                    match flow {
                        None => pc = next,
                        Some(Flow::Call(t)) => {
                            if let Some(t) = t {
                                self.add_start(t);
                            }
                            if t.is_some_and(|t| !self.returns(t)) {
                                break;
                            }
                            pc = next;
                        }
                        Some(Flow::CallSlot(s)) => {
                            if !self.slot_returns(s) {
                                break;
                            }
                            pc = next;
                        }
                        Some(Flow::Jump(Some(t))) => {
                            paths.push(t);
                            break;
                        }
                        Some(Flow::Switch(n)) => {
                            paths.extend_from_slice(&self.switches[n as usize].targets);
                            break;
                        }
                        Some(Flow::Branch(t)) => {
                            paths.push(t);
                            pc = next;
                        }
                        Some(Flow::Jump(None) | Flow::JumpSlot(_) | Flow::Return | Flow::Stop) => break,
                    }
                }
            }
        }
    }

    /// Remembers the code addresses an instruction names: when the image is
    /// relocated, those in the words the loader relocates; otherwise its
    /// immediates (and in 64-bit code, what `lea` takes the address of).
    fn note_addresses(&mut self, ins: &Instruction) {
        let (pc, end) = (ins.ip(), ins.next_ip());
        if self.bits == 64 && ins.is_ip_rel_memory_operand() && ins.mnemonic() == Mnemonic::Lea {
            let t = ins.ip_rel_memory_address();
            if self.is_code(t) {
                self.named.push(t);
            }
        }
        if let Some(relocations) = self.relocations {
            let first = relocations.partition_point(|&r| r < pc);
            for &r in relocations[first..].iter().take_while(|&&r| r < end) {
                if let Some(t) = self.read_word(r).filter(|&t| self.is_code(t)) {
                    self.named.push(t);
                }
            }
            return;
        }
        for i in 0..ins.op_count() {
            if matches!(ins.op_kind(i), OpKind::Immediate32 | OpKind::Immediate64) {
                let t = ins.immediate(i);
                if self.is_code(t) {
                    self.named.push(t);
                }
            }
        }
    }

    /// Whether the function at `f` can return: whether any path from it
    /// reaches a `ret`, or leaves through a jump that can't be followed.
    fn returns(&mut self, f: u64) -> bool {
        match self.returns.get(&f) {
            Some(r) => return *r != Returns::No,
            None if self.depth >= MAX_DEPTH => return true,
            None => {}
        }
        self.returns.insert(f, Returns::Pending);
        self.depth += 1;
        let r = self.reaches_return(f);
        self.depth -= 1;
        self.returns.insert(f, if r { Returns::Yes } else { Returns::No });
        r
    }

    fn reaches_return(&mut self, f: u64) -> bool {
        let mut seen = HashSet::new();
        let mut paths = vec![f];
        let mut budget = 100_000u32;
        while let Some(mut pc) = paths.pop() {
            loop {
                if !seen.insert(pc) {
                    break;
                }
                budget = budget.saturating_sub(1);
                // Code that doesn't decode says nothing: take it to return.
                let (Some((len, flow)), true) = (self.step(pc), budget > 0) else {
                    if pc == f || budget == 0 {
                        return true;
                    }
                    break;
                };
                match flow {
                    None => pc += len,
                    Some(Flow::Return | Flow::Jump(None)) => return true,
                    Some(Flow::Call(t)) => {
                        if t.is_some_and(|t| !self.returns(t)) {
                            break;
                        }
                        pc += len;
                    }
                    Some(Flow::CallSlot(s)) => {
                        if !self.slot_returns(s) {
                            break;
                        }
                        pc += len;
                    }
                    Some(Flow::JumpSlot(s)) => {
                        if self.slot_returns(s) {
                            return true;
                        }
                        break;
                    }
                    // A tail call to a function already worked out is as good as its answer.
                    Some(Flow::Jump(Some(t))) => {
                        match self.returns.get(&t) {
                            Some(Returns::Yes | Returns::Pending) => return true,
                            Some(Returns::No) => {}
                            None => paths.push(t),
                        }
                        break;
                    }
                    Some(Flow::Switch(n)) => {
                        paths.extend_from_slice(&self.switches[n as usize].targets);
                        break;
                    }
                    Some(Flow::Branch(t)) => {
                        paths.push(t);
                        pc += len;
                    }
                    Some(Flow::Stop) => break,
                }
            }
        }
        false
    }

    /// Cuts the code followed into functions: each runs from its start
    /// towards the next, over what it reaches without leaving that range, and
    /// the tables it jumps through there. A jump out of the range to code no
    /// function holds starts a function of its own (tail-called), except
    /// where MSVC calls a `__finally` block in place: the function calls the
    /// block, which sits in its middle, and branches over it to go on. A
    /// start called only from the function before it, which that function
    /// branches over to the code right after it, is part of that function.
    /// Returns (start, end) of each function, sorted.
    fn partition(&mut self) -> Vec<(u64, u64)> {
        // A start taken only because the image holds its address, which turns
        // out to be a case of a switch or where code branches forward to, is
        // part of that code: the address was the jump table's, read before
        // the code using it was found.
        let mut inner: Vec<u64> = self.switches.iter().flat_map(|s| s.targets.iter().copied()).collect();
        inner.extend(self.flows.iter().filter_map(|(&pc, f)| match *f {
            Flow::Branch(t) if t > pc => Some(t),
            _ => None,
        }));
        for t in inner {
            if self.adopted.remove(&t) {
                self.starts.remove(&t);
                self.rejected.insert(t);
            }
        }
        let callers = self.call_sites();
        let mut starts = self.starts.clone();
        let mut extents = Vec::new();
        for _ in 0..64 {
            let Bodies {
                extents: found,
                exits: left,
            } = self.bodies(&starts);
            extents = found;
            // Starts with no code (a call into the middle of an instruction, say) are no functions.
            let empty: Vec<u64> = starts
                .iter()
                .copied()
                .filter(|s| extents.binary_search_by_key(s, |e| e.0).is_err())
                .collect();
            for s in &empty {
                starts.remove(s);
                self.starts.remove(s);
                self.rejected.insert(*s);
            }
            let mut folded = false;
            for &(from, t) in &left {
                let Some(f) = extents.partition_point(|e| e.0 <= from).checked_sub(1) else {
                    continue;
                };
                let (Some(&(n, n_end)), next) = (extents.get(f + 1), extents.get(f + 2).map(|e| e.0)) else {
                    continue;
                };
                let local = callers
                    .get(&n)
                    .is_some_and(|sites| sites.iter().all(|&site| site >= extents[f].0 && site < n));
                if self.flows.get(&from).is_some_and(|fl| matches!(fl, Flow::Branch(_)))
                    && t >= n_end
                    && next.is_none_or(|next| t < next)
                    && local
                    && starts.remove(&n)
                {
                    folded = true;
                }
            }
            if folded {
                continue;
            }
            let mut added = false;
            for &(_, t) in &left {
                if !starts.contains(&t) && !within(&extents, t) && self.len_at(t).is_some() {
                    starts.insert(t);
                    added = true;
                }
            }
            if !added {
                break;
            }
        }
        extents
    }

    /// Each function's extent, from its start over the code it reaches
    /// without leaving its range (see [`Self::partition`]), and the jumps
    /// that leave it.
    fn bodies(&mut self, starts: &BTreeSet<u64>) -> Bodies {
        let list: Vec<u64> = starts.iter().copied().collect();
        let mut extents = Vec::new();
        let mut left = Vec::new();
        let mut seen = HashSet::new();
        for (i, &f) in list.iter().enumerate() {
            let Some((c, _)) = self.code_at(f) else { continue };
            let region_end = self.code[c].address + self.code[c].bytes.len() as u64;
            let limit = list.get(i + 1).map_or(region_end, |&n| n.min(region_end));
            let mut end = f;
            seen.clear();
            let mut paths = vec![f];
            let mut jump = |from: u64, t: u64, paths: &mut Vec<u64>| {
                if t >= f && t < limit {
                    paths.push(t);
                } else {
                    left.push((from, t));
                }
            };
            while let Some(mut pc) = paths.pop() {
                while pc < limit && seen.insert(pc) {
                    let Some((len, flow)) = self.step(pc) else { break };
                    end = end.max(pc + len);
                    match flow {
                        None => pc += len,
                        Some(Flow::Call(t)) => {
                            if t.is_some_and(|t| !self.returns(t)) {
                                break;
                            }
                            pc += len;
                        }
                        Some(Flow::CallSlot(s)) => {
                            if !self.slot_returns(s) {
                                break;
                            }
                            pc += len;
                        }
                        Some(Flow::Jump(Some(t))) => {
                            jump(pc, t, &mut paths);
                            break;
                        }
                        Some(Flow::Switch(n)) => {
                            let s = &self.switches[n as usize];
                            for &t in &s.targets {
                                jump(pc, t, &mut paths);
                            }
                            // MSVC keeps its tables right after the function's code.
                            for t in std::iter::once(s.table).chain(s.index) {
                                if t.address >= f && t.end() <= limit {
                                    end = end.max(t.end());
                                }
                            }
                            break;
                        }
                        Some(Flow::Branch(t)) => {
                            jump(pc, t, &mut paths);
                            pc += len;
                        }
                        Some(Flow::Jump(None) | Flow::JumpSlot(_) | Flow::Return | Flow::Stop) => break,
                    }
                }
            }
            if end > f {
                extents.push((f, end));
            }
        }
        left.sort_unstable();
        left.dedup();
        Bodies { extents, exits: left }
    }

    /// Where each address is called from.
    fn call_sites(&self) -> HashMap<u64, Vec<u64>> {
        let mut out: HashMap<u64, Vec<u64>> = HashMap::new();
        for (&site, flow) in &self.flows {
            if let Flow::Call(Some(t)) = *flow {
                out.entry(t).or_default().push(site);
            }
        }
        out
    }

    /// The code addresses the image names that fall where no function is:
    /// in the words the loader relocates, or (without relocations) aligned
    /// words of the data; and those instructions name.
    fn candidates(&mut self, extents: &[(u64, u64)]) -> Vec<u64> {
        let mut out = std::mem::take(&mut self.named);
        let word = if self.bits == 64 { 8 } else { 4 };
        match self.relocations {
            Some(relocations) => {
                for &r in relocations {
                    if !self.is_code(r)
                        && let Some(t) = self.read_word(r).filter(|&t| self.is_code(t))
                    {
                        out.push(t);
                    }
                }
            }
            None => {
                for r in self.regions.iter().filter(|r| !r.code) {
                    let skip = ((word - r.address % word) % word) as usize;
                    for w in r.bytes.get(skip..).unwrap_or(&[]).chunks_exact(word as usize) {
                        let t = if word == 8 {
                            u64::from_le_bytes(w.try_into().expect("8 bytes"))
                        } else {
                            u32::from_le_bytes(w.try_into().expect("4 bytes")) as u64
                        };
                        if t != 0 && self.is_code(t) {
                            out.push(t);
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        let tables = self.table_extents();
        out.retain(|&t| {
            !self.starts.contains(&t) && !self.rejected.contains(&t) && !within(extents, t) && !within(&tables, t)
        });
        out
    }

    /// Where each unexplored stretch of code starts, past its alignment
    /// filler. Straight after a call that doesn't return is often code the
    /// compiler left there (it didn't know): that only counts when it starts
    /// the way functions do.
    fn gap_starts(&mut self, extents: &[(u64, u64)]) -> Vec<u64> {
        let mut covered: Vec<(u64, u64)> = extents.iter().copied().chain(self.table_extents()).collect();
        covered.sort_unstable();
        let covered = merge(covered);
        let mut out: Vec<(u64, bool, &[u8])> = Vec::new();
        for c in &self.code {
            let end = c.address + c.bytes.len() as u64;
            let mut at = c.address;
            let first = covered.partition_point(|e| e.1 <= at);
            let mut gaps = Vec::new();
            for &(s, e) in &covered[first..] {
                if s >= end {
                    break;
                }
                if s > at {
                    gaps.push((at, s));
                }
                at = at.max(e);
            }
            if at < end {
                gaps.push((at, end));
            }
            for (s, e) in gaps {
                let bytes = &c.bytes[(s - c.address) as usize..(e - c.address) as usize];
                let pad = lead_padding(bytes, s, self.bits);
                if pad < bytes.len() && !self.rejected.contains(&(s + pad as u64)) {
                    out.push((s + pad as u64, pad == 0, &bytes[pad..]));
                }
            }
        }
        out.into_iter()
            .filter(|&(a, after, bytes)| {
                let after_call = after
                    && self
                        .step_back(a)
                        .is_some_and(|f| matches!(f, Some(Flow::Call(_) | Flow::CallSlot(_))));
                !after_call || looks_like_prologue(bytes)
            })
            .map(|(a, ..)| a)
            .collect()
    }

    /// Where the instruction decoded right before `pc` goes, if one ends there.
    fn step_back(&self, pc: u64) -> Option<Option<Flow>> {
        let (c, off) = self.code_at(pc)?;
        let state = &self.code[c].state;
        let k = (1..=15.min(off)).find(|&k| (state[off - k] & LEN) as usize == k)?;
        Some(if state[off - k] & FLOW != 0 {
            self.flows.get(&(pc - k as u64)).copied()
        } else {
            None
        })
    }

    fn table_extents(&self) -> Vec<(u64, u64)> {
        let mut v: Vec<(u64, u64)> = self
            .switches
            .iter()
            .flat_map(|s| std::iter::once(s.table).chain(s.index))
            .map(|t| (t.address, t.end()))
            .collect();
        v.sort_unstable();
        merge(v)
    }

    /// Takes the candidates that read as code as functions, in address
    /// order; one inside the code another was checked with isn't taken this
    /// time. Returns how many were.
    fn adopt(&mut self, candidates: Vec<u64>, extents: &[(u64, u64)]) -> usize {
        let mut claimed: BTreeMap<u64, u64> = BTreeMap::new();
        let mut added = 0;
        for a in candidates {
            let inside = claimed.range(..=a).next_back().is_some_and(|(&p, &l)| a < p + l);
            if inside || within(extents, a) {
                continue;
            }
            match self.reads_as_code(a) {
                Some(code) => {
                    claimed.extend(code);
                    self.add_start(a);
                    self.adopted.insert(a);
                    added += 1;
                }
                None => {
                    self.rejected.insert(a);
                }
            }
        }
        added
    }

    /// Whether the code at `a`, which nothing is known to reach, reads as a
    /// function: every path from it decodes, into nothing decoded before at
    /// another boundary, without instructions user code doesn't use, and
    /// calls and jumps land in code (or 2000 instructions do). Returns the
    /// instructions it holds (address, length), up to where it joins code
    /// followed before.
    fn reads_as_code(&mut self, a: u64) -> Option<BTreeMap<u64, u64>> {
        let mut mine: BTreeMap<u64, u64> = BTreeMap::new();
        let mut paths = vec![a];
        let mut budget = 2_000u32;
        while let Some(mut pc) = paths.pop() {
            loop {
                if mine.contains_key(&pc) {
                    break;
                }
                // Into code already followed, at one of its boundaries: fine.
                if pc != a && self.len_at(pc).is_some() {
                    break;
                }
                if budget == 0 {
                    return Some(mine);
                }
                budget -= 1;
                let (c, off) = self.code_at(pc)?;
                if self.code[c].state[off] & DATA != 0 || self.inside_instruction(c, off) {
                    return None;
                }
                let code = &mut self.code[c];
                code.decoder.set_position(off).ok()?;
                code.decoder.set_ip(pc);
                let mut ins = Instruction::default();
                code.decoder.decode_out(&mut ins);
                if ins.is_invalid() || unlikely(&ins, self.bits) {
                    return None;
                }
                let len = ins.len() as u64;
                // Overlapping code decoded before, or what this decoded itself?
                let overlaps = (1..len).any(|k| {
                    self.code_at(pc + k)
                        .is_some_and(|(c, off)| self.code[c].state[off] & (LEN | DATA) != 0)
                }) || mine.range(pc + 1..pc + len).next().is_some()
                    || mine.range(..pc).next_back().is_some_and(|(&p, &l)| pc < p + l);
                if overlaps {
                    return None;
                }
                mine.insert(pc, len);
                let next = pc + len;
                let flow = if self.len_at(pc).is_some() {
                    self.step(pc).and_then(|s| s.1)
                } else {
                    self.flow_of_unrecorded(&ins)
                };
                let lands = |f: &Follower, t: u64| {
                    f.code_at(t).is_some_and(|(c, off)| {
                        let s = f.code[c].state[off];
                        s & DATA == 0 && (s & LEN != 0 || !f.inside_instruction(c, off))
                    })
                };
                match flow {
                    // A function doesn't run on into the next one.
                    None if self.starts.contains(&next) => return None,
                    None => pc = next,
                    Some(Flow::Call(t)) => {
                        if let Some(t) = t {
                            if !lands(self, t) {
                                return None;
                            }
                            if self.returns.get(&t) == Some(&Returns::No) {
                                break;
                            }
                        }
                        pc = next;
                    }
                    Some(Flow::CallSlot(s)) => {
                        if !self.slot_returns(s) {
                            break;
                        }
                        pc = next;
                    }
                    Some(Flow::Jump(Some(t))) => {
                        if !lands(self, t) {
                            return None;
                        }
                        if !self.starts.contains(&t) {
                            paths.push(t);
                        }
                        break;
                    }
                    Some(Flow::Branch(t)) => {
                        if !lands(self, t) {
                            return None;
                        }
                        paths.push(t);
                        pc = next;
                    }
                    Some(Flow::Switch(_) | Flow::Jump(None) | Flow::JumpSlot(_) | Flow::Return | Flow::Stop) => break,
                }
            }
        }
        Some(mine)
    }

    /// Where an instruction goes, without recording a jump table it may read.
    fn flow_of_unrecorded(&mut self, ins: &Instruction) -> Option<Flow> {
        if ins.flow_control() == FlowControl::IndirectBranch && self.slot(ins).is_none() {
            return Some(Flow::Jump(None));
        }
        self.flow_of(ins)
    }

    /// The decoded instruction that ends at `pc`, if one does.
    fn previous(&mut self, pc: u64) -> Option<Instruction> {
        let (c, off) = self.code_at(pc)?;
        let state = &self.code[c].state;
        let k = (1..=15.min(off)).find(|&k| (state[off - k] & LEN) as usize == k)?;
        let code = &mut self.code[c];
        code.decoder.set_position(off - k).ok()?;
        code.decoder.set_ip(pc - k as u64);
        let mut ins = Instruction::default();
        code.decoder.decode_out(&mut ins);
        (!ins.is_invalid()).then_some(ins)
    }

    /// The jump table an indirect jump reads: `jmp [index*4 + table]`, or
    /// `jmp reg` just after loading `reg` from one, with the number of
    /// entries from the bound checked before (`cmp index, n` / `ja`) and,
    /// the way MSVC writes a sparse switch, a table of bytes picking the entry.
    fn switch(&mut self, ins: &Instruction) -> Option<u32> {
        if self.bits != 32 {
            return None;
        }
        let jump = ins.ip();
        let (table, index) = match ins.op0_kind() {
            OpKind::Memory => (absolute_table(ins)?, ins.memory_index()),
            OpKind::Register => {
                let reg = ins.op0_register().full_register32();
                let load = self.previous(jump)?;
                if load.mnemonic() != Mnemonic::Mov
                    || load.op0_kind() != OpKind::Register
                    || load.op0_register().full_register32() != reg
                    || load.op1_kind() != OpKind::Memory
                {
                    return None;
                }
                (absolute_table(&load)?, load.memory_index())
            }
            _ => return None,
        };
        let (bound, index_table) = self.bound(jump, index.full_register32());
        let mut index = None;
        let count = match (index_table, bound) {
            (Some(at), Some(n)) => {
                let bytes = self.read(at, n as usize)?;
                index = Some(Table {
                    jump,
                    address: at,
                    entry: 1,
                    count: n,
                });
                Some(*bytes.iter().max()? as usize + 1)
            }
            (_, n) => n.map(|n| n as usize),
        };
        let mut targets = Vec::new();
        for i in 0..count.unwrap_or(MAX_TABLE).min(MAX_TABLE) {
            let at = table + 4 * i as u64;
            // Without a bound, the table ends where its entries stop being addresses of code.
            if count.is_none()
                && i > 0
                && (self.relocations.is_some_and(|_| !self.relocated(at))
                    || self.len_at(at).is_some()
                    || index.is_some_and(|x| x.address == at))
            {
                break;
            }
            match self.read_u32(at).filter(|&t| self.is_code(t)) {
                Some(t) => targets.push(t),
                None => break,
            }
        }
        if targets.is_empty() {
            return None;
        }
        let table_end = table + 4 * targets.len() as u64;
        for t in [Some((table, table_end)), index.map(|x| (x.address, x.end()))]
            .into_iter()
            .flatten()
        {
            for a in t.0..t.1 {
                if let Some((c, off)) = self.code_at(a) {
                    self.code[c].state[off] |= DATA;
                }
            }
        }
        self.switches.push(Switch {
            table: Table {
                jump,
                address: table,
                entry: 4,
                count: targets.len() as u32,
            },
            index,
            targets,
        });
        Some(self.switches.len() as u32 - 1)
    }

    /// Walking back from a jump through a table indexed by `reg`: how many
    /// entries the code checks the index against (`cmp reg, n` and `ja`, or
    /// `and reg, n`), and the table of bytes the index was read from, if it was.
    fn bound(&mut self, jump: u64, mut reg: Register) -> (Option<u32>, Option<u64>) {
        let mut index_table = None;
        let mut pc = jump;
        // The instruction after the one looked at: the branch after a `cmp`.
        let mut after = None;
        for _ in 0..8 {
            let Some(ins) = self.previous(pc) else { break };
            if !matches!(ins.flow_control(), FlowControl::Next | FlowControl::ConditionalBranch) {
                break;
            }
            let on_reg =
                ins.op_count() > 0 && ins.op0_kind() == OpKind::Register && ins.op0_register().full_register32() == reg;
            if on_reg {
                let n = is_immediate(ins.op1_kind()).then(|| ins.immediate(1) as u32);
                match (ins.mnemonic(), n) {
                    (Mnemonic::Cmp, Some(n)) => {
                        let bound = match after {
                            Some(Mnemonic::Ja | Mnemonic::Jbe) => n.checked_add(1),
                            Some(Mnemonic::Jae | Mnemonic::Jb) => Some(n),
                            _ => None,
                        };
                        return (bound.filter(|&b| b > 0 && b as usize <= MAX_TABLE), index_table);
                    }
                    (Mnemonic::And, Some(n)) if (n as usize) < MAX_TABLE => return (Some(n + 1), index_table),
                    // movzx reg, byte [other + indexes] (or xor reg, reg / mov reg8, byte [...]).
                    (Mnemonic::Movzx | Mnemonic::Mov, _)
                        if index_table.is_none()
                            && ins.op1_kind() == OpKind::Memory
                            && ins.memory_size() == MemorySize::UInt8 =>
                    {
                        let other = match (ins.memory_base(), ins.memory_index()) {
                            (b, Register::None) if b != Register::None => b,
                            (Register::None, i) if ins.memory_index_scale() == 1 => i,
                            _ => break,
                        };
                        index_table = Some(ins.memory_displacement32() as u64);
                        reg = other.full_register32();
                    }
                    (Mnemonic::Mov, _) if ins.op1_kind() == OpKind::Register => {
                        reg = ins.op1_register().full_register32();
                    }
                    // Moving the range to start at 0 comes before the check; tests don't write.
                    (Mnemonic::Add | Mnemonic::Sub | Mnemonic::Dec | Mnemonic::Inc | Mnemonic::Test, _) => {}
                    _ => break,
                }
            }
            after = Some(ins.mnemonic());
            pc = ins.ip();
        }
        (None, index_table)
    }
}

/// The address of the table `[index*4 + table]` names (32-bit, no base register).
fn absolute_table(ins: &Instruction) -> Option<u64> {
    (ins.memory_base() == Register::None && ins.memory_index() != Register::None && ins.memory_index_scale() == 4)
        .then(|| ins.memory_displacement32() as u64)
}

fn is_immediate(k: OpKind) -> bool {
    matches!(
        k,
        OpKind::Immediate8 | OpKind::Immediate8to32 | OpKind::Immediate16 | OpKind::Immediate32
    )
}

/// Sorted ranges with the overlapping or touching ones merged.
fn merge(sorted: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    let mut out: Vec<(u64, u64)> = Vec::with_capacity(sorted.len());
    for (s, e) in sorted {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

/// Whether `a` is in one of the sorted, non-overlapping ranges.
fn within(ranges: &[(u64, u64)], a: u64) -> bool {
    let i = ranges.partition_point(|r| r.0 <= a);
    i > 0 && a < ranges[i - 1].1
}

/// Instructions a compiler doesn't write in user-mode code, which bytes
/// that aren't code decode to often enough.
fn unlikely(ins: &Instruction, bits: u32) -> bool {
    use Mnemonic::*;
    if matches!(ins.op0_kind(), OpKind::FarBranch16 | OpKind::FarBranch32) {
        return true;
    }
    let privileged = matches!(
        ins.mnemonic(),
        In | Out
            | Insb
            | Insw
            | Insd
            | Outsb
            | Outsw
            | Outsd
            | Hlt
            | Cli
            | Sti
            | Iret
            | Iretd
            | Iretq
            | Retf
            | Lds
            | Les
            | Lss
            | Arpl
            | Bound
            | Into
            | Aaa
            | Aas
            | Daa
            | Das
            | Aam
            | Aad
            | Salc
            | Int1
            | Lgdt
            | Lidt
            | Lldt
            | Ltr
            | Clts
            | Invd
            | Wbinvd
            | Rdmsr
            | Wrmsr
            | Sysexit
            | Sysret
            | Rsm
            | Enter
    );
    if privileged {
        return true;
    }
    // Segment registers other than fs (gs in 64-bit code) are the loader's
    // business; cs and ds prefixes are branch hints and part of long nops.
    let segment = ins.segment_prefix();
    if matches!(segment, Register::ES | Register::SS) || (bits == 32 && segment == Register::GS) {
        return true;
    }
    (0..ins.op_count()).any(|i| ins.op_kind(i) == OpKind::Register && ins.op_register(i).is_segment_register())
}

/// Whether `bytes` start the way compilers start functions: setting up a
/// frame (`push ebp; mov ebp, esp`, the hot-patchable `mov edi, edi`), making
/// room for locals (`sub esp, n`), or pushing an exception handler frame
/// (`push -1; push offset handler`, `push n; push offset scopes` for MSVC's
/// `__SEH_prolog`); in 64-bit code, saving registers and `sub rsp, n`.
fn looks_like_prologue(bytes: &[u8]) -> bool {
    matches!(
        bytes,
        [0x55, 0x8B, 0xEC, ..]
            | [0x55, 0x89, 0xE5, ..]
            | [0x8B, 0xFF, ..]
            | [0x83 | 0x81, 0xEC, ..]
            | [0x6A, _, 0x68, ..]
            | [0x64, 0xA1, 0, 0, 0, 0, ..]
            | [0x48, 0x83 | 0x81, 0xEC, ..]
            | [0x48, 0x89, 0x5C | 0x4C | 0x54 | 0x74 | 0x7C, 0x24, ..]
            | [0x40, 0x53 | 0x55 | 0x56 | 0x57, ..]
            | [0x55, 0x48, 0x89, 0xE5, ..]
    )
}

/// Length of the alignment filler at the start of `bytes`: int3, zeros, and
/// instructions that do nothing (`nop`, the long `nop word cs:[...]` forms,
/// `lea esi, [esi+0]`, `mov esi, esi`). `mov edi, edi` isn't filler: it
/// starts hot-patchable functions.
fn lead_padding(bytes: &[u8], address: u64, bits: u32) -> usize {
    let mut decoder = Decoder::with_ip(bits, bytes, address, DecoderOptions::NONE);
    let mut ins = Instruction::default();
    let mut at = 0;
    while at < bytes.len() {
        if matches!(bytes[at], 0xCC | 0x00) {
            at += 1;
            continue;
        }
        if decoder.set_position(at).is_err() {
            break;
        }
        decoder.set_ip(address + at as u64);
        decoder.decode_out(&mut ins);
        if ins.is_invalid() || !does_nothing(&ins) {
            break;
        }
        at += ins.len();
    }
    at.min(bytes.len())
}

/// `nop` in its forms, and moves of a register onto itself.
fn does_nothing(ins: &Instruction) -> bool {
    match ins.mnemonic() {
        Mnemonic::Nop => true,
        Mnemonic::Xchg | Mnemonic::Mov => {
            ins.op_count() == 2
                && ins.op0_kind() == OpKind::Register
                && ins.op1_kind() == OpKind::Register
                && ins.op0_register() == ins.op1_register()
                && ins.op0_register() != Register::EDI
        }
        Mnemonic::Lea => {
            ins.memory_base() == ins.op0_register()
                && ins.memory_index() == Register::None
                && ins.memory_displacement64() == 0
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Follows 32-bit `code` loaded at 0x401000 from `starts`, with `data` at
    /// 0x402000 (not relocated) and its first word an import slot for a
    /// function that never returns.
    fn follow_code(code: &[u8], data: &[u8], starts: &[u64]) -> Vec<(u64, u64)> {
        let image = Image {
            bits: 32,
            regions: vec![
                Region {
                    address: 0x401000,
                    bytes: code,
                    code: true,
                },
                Region {
                    address: 0x402000,
                    bytes: data,
                    code: false,
                },
            ],
            starts: starts.to_vec(),
            relocations: None,
            slots: HashMap::from([(0x402000, true)]),
        };
        follow(&image).functions
    }

    #[test]
    fn filler_is_skipped_but_not_a_hot_patch_prologue() {
        let filler = [
            0xCC, 0x90, // int3, nop
            0x66, 0x66, 0x2E, 0x0F, 0x1F, 0x84, 0, 0, 0, 0, 0, // clang: nop word cs:[eax+eax]
            0x8D, 0xA4, 0x24, 0, 0, 0, 0, // MSVC: lea esp, [esp]
            0x8D, 0x49, 0x00, // MSVC: lea ecx, [ecx]
        ];
        let mut bytes = filler.to_vec();
        bytes.extend([0x8B, 0xFF, 0x55, 0x8B, 0xEC]); // mov edi, edi; push ebp; mov ebp, esp
        assert_eq!(lead_padding(&bytes, 0x401000, 32), filler.len());
    }

    #[test]
    fn a_call_that_never_returns_ends_the_function() {
        let code = [
            0xE8, 0x0B, 0, 0, 0, // 401000: call 401010
            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, // filler
            0x6A, 0x00, // 401010: push 0
            0xFF, 0x15, 0x00, 0x20, 0x40, 0x00, // call [402000] (ExitProcess)
            0xB8, 0x01, 0, 0, 0,    // 401018: mov eax, 1 (only a pointer in data refers to it)
            0xC3, // ret
        ];
        let data = [0, 0, 0, 0, 0x18, 0x10, 0x40, 0x00];
        assert_eq!(
            follow_code(&code, &data, &[0x401000]),
            [(0x401000, 5), (0x401010, 8), (0x401018, 6)]
        );
    }

    #[test]
    fn code_left_after_a_call_that_never_returns_is_no_function() {
        let code = [
            0xE8, 0x0B, 0, 0, 0, // 401000: call 401010, which ends in ExitProcess
            0x83, 0xC4, 0x04, // add esp, 4 (the compiler didn't know)
            0xC3, // ret
            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, // filler
            0x6A, 0x00, // 401010: push 0
            0xFF, 0x15, 0x00, 0x20, 0x40, 0x00, // call [402000] (ExitProcess)
        ];
        assert_eq!(follow_code(&code, &[0; 4], &[0x401000]), [(0x401000, 5), (0x401010, 8)]);
    }

    #[test]
    fn cases_taken_from_a_table_in_data_go_back_to_their_switch() {
        // Only a pointer in data reaches the switch, and its jump table is in
        // data too: its cases read as functions until the switch is followed.
        let mut code = vec![0xC3]; // 401000: ret
        code.resize(0x10, 0xCC);
        code.extend([
            0x8B, 0x44, 0x24, 0x04, // 401010: mov eax, [esp+4]
            0x83, 0xF8, 0x01, // cmp eax, 1
            0x77, 0x0C, // ja 401025
            0xFF, 0x24, 0x85, 0x10, 0x20, 0x40, 0x00, // jmp [eax*4+402010]
            0x31, 0xC0, 0xC3, // 401020: xor eax, eax; ret
            0x40, 0xC3, // 401023: inc eax; ret
            0x48, 0xC3, // 401025: dec eax; ret
        ]);
        let mut data = vec![0, 0, 0, 0, 0x10, 0x10, 0x40, 0x00];
        data.resize(0x10, 0);
        data.extend([0x20, 0x10, 0x40, 0x00, 0x23, 0x10, 0x40, 0x00]);
        assert_eq!(
            follow_code(&code, &data, &[0x401000]),
            [(0x401000, 1), (0x401010, 0x17)]
        );
    }

    #[test]
    fn a_finally_block_called_in_place_is_part_of_its_function() {
        let code = [
            0xE8, 0x03, 0, 0, 0, // 401000: call 401008 (the __finally block)
            0x74, 0x03, // je 40100a, over it
            0xC3, // ret
            0x41, // 401008: inc ecx
            0xC3, // ret
            0x31, 0xC0, // 40100a: xor eax, eax
            0xC3, // ret
        ];
        assert_eq!(follow_code(&code, &[], &[0x401000]), [(0x401000, 13)]);
    }

    #[test]
    fn a_conditional_tail_call_leaves_the_function() {
        let code = [
            0xE8, 0x0B, 0, 0, 0, // 401000: call 401010
            0xE8, 0x16, 0, 0, 0,    // call 401020
            0xC3, // ret
            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, //
            0x85, 0xC0, // 401010: test eax, eax
            0x74, 0x1C, // je 401030: a tail call past the next function
            0xC3, // ret
            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, //
            0x31, 0xC0, // 401020: xor eax, eax
            0xC3, // ret
            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, //
            0x40, // 401030: inc eax
            0xC3, // ret
        ];
        assert_eq!(
            follow_code(&code, &[], &[0x401000]),
            [(0x401000, 11), (0x401010, 5), (0x401020, 3), (0x401030, 2)]
        );
    }

    #[test]
    fn a_jump_table_is_bounded_by_its_check_and_picked_through_index_bytes() {
        let code = [
            0x8B, 0x44, 0x24, 0x04, // 401000: mov eax, [esp+4]
            0x83, 0xF8, 0x03, // cmp eax, 3
            0x77, 0x14, // ja 40101d
            0x0F, 0xB6, 0x80, 0x28, 0x10, 0x40, 0x00, // movzx eax, byte [eax+401028]
            0xFF, 0x24, 0x85, 0x20, 0x10, 0x40, 0x00, // jmp [eax*4+401020]
            0x31, 0xC0, 0xC3, // 401017: xor eax, eax; ret
            0x40, 0xC3, // 40101a: inc eax; ret
            0x90, // 40101c
            0x48, 0xC3, // 40101d: dec eax; ret
            0x90, // 40101f
            0x17, 0x10, 0x40, 0x00, 0x1A, 0x10, 0x40, 0x00, // 401020: two cases
            0x01, 0x00, 0x00, 0x01, // 401028: the case for each index 0..=3
        ];
        let image_code: &[u8] = &code;
        let image = Image {
            bits: 32,
            regions: vec![Region {
                address: 0x401000,
                bytes: image_code,
                code: true,
            }],
            starts: vec![0x401000],
            relocations: None,
            slots: HashMap::new(),
        };
        let found = follow(&image);
        assert_eq!(found.functions, [(0x401000, 0x2C)]);
        let tables: Vec<(u64, u8, u32)> = found.tables.iter().map(|t| (t.address, t.entry, t.count)).collect();
        assert_eq!(tables, [(0x401020, 4, 2), (0x401028, 1, 4)]);
    }
}
