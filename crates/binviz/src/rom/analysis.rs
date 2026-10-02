//! Finding a ROM's code: nothing marks where code ends and data (graphics,
//! tables, text) begins, so the code is followed from the entry points
//! instead — each branch taken both ways, each call target a new function —
//! until every path ends in a return, a jump through a register or a table,
//! or bytes that don't decode. What the instructions on the way read, write
//! and call are the ROM's cross-references.
//!
//! A code/data log from an emulator adds what played the game saw: the
//! subroutines and jump-table targets it marked are entry points too, then
//! any code it saw run that following the code didn't reach; bytes it only
//! saw read as data are never decoded; and each instruction runs in the
//! state it ran in then (the 65816's register widths, ARM or Thumb).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use super::Rom;
use super::cdl::{CodeDataLog, flag};
use super::jumptable;
use super::pointers;
use crate::cpu::{self, Cpu, Flow, State};
use crate::model::Section;
use crate::xrefs::RefKind;

/// What following a ROM's code found.
#[derive(Debug, Clone, Default)]
pub(crate) struct Analysis {
    /// Functions: (start, size), sorted.
    pub functions: Vec<(u64, u64)>,
    /// References the code makes: (instruction, target, kind), in our addresses.
    pub refs: Vec<(u64, u64, RefKind)>,
    /// The CPU state at each function's start (the 65816's register widths).
    pub states: HashMap<u64, State>,
    /// Instructions decoded.
    pub instructions: usize,
    /// Tables of pointers the code reads (see [`pointers`]).
    pub tables: Vec<pointers::Table>,
    /// Addresses code builds in a window whose bank can't be told: (instruction, CPU address).
    pub unplaced: Vec<(u64, u64)>,
}

/// At most this many instructions are followed (a 4 MiB ROM holds far fewer).
const LIMIT: usize = 2_000_000;

/// The file bytes at one of our addresses, to the end of their section.
fn code_bytes<'a>(data: &'a [u8], sections: &[Section], address: u64) -> Option<&'a [u8]> {
    let s = sections
        .iter()
        .find(|s| s.loaded && s.file_offset.is_some() && address >= s.address && address < s.address + s.size)?;
    let start = s.file_offset? + (address - s.address);
    let end = s.file_offset? + s.file_size;
    data.get(start as usize..end as usize)
}

impl crate::binary::Binary {
    /// The file bytes at one of our addresses, to the end of their section:
    /// what an instruction may read past its own function (a literal pool).
    pub(crate) fn code_bytes(&self, address: u64) -> Option<&[u8]> {
        code_bytes(&self.data, &self.sections, address)
    }

    /// MIPS: the jump table the `jr` at `pc` reads where it goes from, if it reads one.
    pub(crate) fn mips_jump_table(&self, pc: u64) -> Option<jumptable::MipsTable> {
        let rom = self.rom.as_ref()?;
        let big = match rom.cpu {
            Cpu::MipsR3000 => false,
            Cpu::MipsR4300 => true,
            _ => return None,
        };
        let bytes_at = |a: u64| self.code_bytes(a);
        let resolve = |from: u64, t: u64| rom.map.resolve(from, t);
        let may_be_code = |_: u64| true;
        let code = jumptable::Code {
            bytes_at: &bytes_at,
            resolve: &resolve,
            may_be_code: &may_be_code,
        };
        jumptable::mips_table(pc, &code, big)
    }
}

/// The file offset of one of our addresses.
fn offset_of(sections: &[Section], address: u64) -> Option<u64> {
    let s = sections
        .iter()
        .find(|s| s.loaded && s.file_offset.is_some() && address >= s.address && address < s.address + s.size)?;
    Some(s.file_offset? + (address - s.address))
}

/// The state code the log saw run in: the 65816's register widths, ARM or Thumb.
fn logged_state(mut state: State, f: u16, cpu: Cpu) -> State {
    if f & flag::CODE != 0 {
        match cpu {
            Cpu::W65816 => {
                state.m8 = f & flag::M8 != 0;
                state.x8 = f & flag::X8 != 0;
            }
            Cpu::Arm7Tdmi => state.thumb = f & flag::THUMB != 0,
            _ => {}
        }
    }
    state
}

/// Code a log saw run: (address, flags). `first` gives the entry points it
/// marked (subroutines, jump-table targets); otherwise each run of code and
/// each jump target it marked.
fn logged_code(log: &CodeDataLog, sections: &[Section], first: bool) -> Vec<(u64, u16)> {
    let mut out = Vec::new();
    for s in sections.iter().filter(|s| s.loaded && s.file_size > 0) {
        let Some(base) = s.file_offset else { continue };
        let mut prev = 0;
        for i in 0..s.file_size {
            let f = log.at(base + i);
            let code = f & flag::CODE != 0;
            let entry = f & flag::ENTRY != 0 || f & (flag::JUMP | flag::INDIRECT) == flag::JUMP | flag::INDIRECT;
            let pick = if first {
                entry
            } else {
                f & flag::JUMP != 0 || prev & flag::CODE == 0
            };
            if code && pick {
                out.push((s.address + i, f));
            }
            prev = f;
        }
    }
    out
}

/// Following the code: what has been decoded so far and what it found.
struct Walk<'a> {
    data: &'a [u8],
    sections: &'a [Section],
    rom: &'a Rom,
    log: Option<&'a CodeDataLog>,
    /// Where functions start, with the CPU state there.
    starts: BTreeMap<u64, State>,
    /// Function starts still to follow.
    queue: VecDeque<u64>,
    /// Which function each decoded instruction belongs to, and its length.
    owner: HashMap<u64, (u64, u32)>,
    refs: Vec<(u64, u64, RefKind)>,
    /// Jumps and branches within a function: (instruction, target).
    jumps: Vec<(u64, u64)>,
    /// Addresses the flow never runs on to: after a return, an unconditional
    /// jump (its delay slot included), a trap, or bytes that don't decode.
    ends: HashSet<u64>,
    /// Functions with an indirect jump whose table wasn't found (a switch
    /// whose cases the following couldn't reach).
    unresolved: HashSet<u64>,
}

impl Walk<'_> {
    fn bytes_at(&self, a: u64) -> Option<&[u8]> {
        code_bytes(self.data, self.sections, a)
    }

    fn logged(&self, a: u64) -> u16 {
        self.log
            .and_then(|l| offset_of(self.sections, a).map(|o| l.at(o)))
            .unwrap_or(0)
    }

    /// Nothing starts with four zero bytes (a data symbol taken for a function, zeroed memory).
    fn zeroes(&self, a: u64) -> bool {
        self.bytes_at(a)
            .is_some_and(|b| b.len() >= 4 && b[..4].iter().all(|&x| x == 0))
    }

    /// A function start to follow, unless it is one already.
    fn start(&mut self, address: u64, state: State) {
        if self.starts.insert(address, state).is_none() {
            self.queue.push_back(address);
        }
    }

    /// Follows the code from `pc` as part of `function`: each branch taken
    /// both ways, each call target a new function, until every path ends.
    fn follow(&mut self, function: u64, pc: u64, state: State) {
        let rom = self.rom;
        let mut stack = vec![(pc, state)];
        while let Some((pc, mut state)) = stack.pop() {
            if self.owner.contains_key(&pc) || self.owner.len() >= LIMIT {
                continue;
            }
            // Running into the start of another function: its run ends here, and the
            // two are joined afterwards (a fall-through is one function to the compiler).
            if pc != function && self.starts.contains_key(&pc) {
                continue;
            }
            let f = self.logged(pc);
            // Only ever read as data: not code, whatever leads here.
            if f & (flag::CODE | flag::DATA) == flag::DATA {
                continue;
            }
            state = logged_state(state, f, rom.cpu);
            let Some(bytes) = self.bytes_at(pc) else { continue };
            // A run of zeroes is memory nothing was loaded into, not a sled of nops.
            if bytes.len() >= 64 && bytes[..64].iter().all(|&b| b == 0) {
                self.ends.insert(pc);
                continue;
            }
            let Some(insn) = cpu::decode(rom.cpu, bytes, rom.map.cpu(pc), &mut state) else {
                continue;
            };
            if insn.flow == Flow::Stop {
                self.ends.insert(pc);
                continue;
            }
            self.owner.insert(pc, (function, insn.len));
            let mut next = pc + insn.len as u64;
            if let Some((t, kind)) = insn.data
                && let Some(a) = rom.map.resolve(pc, t)
            {
                self.refs.push((pc, a, kind));
            }
            // MIPS: the instruction after a jump or branch (its delay slot) runs first.
            if insn.delay_slot {
                if let Some(bytes) = self.bytes_at(next)
                    && !self.owner.contains_key(&next)
                    && let Some(slot) = cpu::decode(rom.cpu, bytes, rom.map.cpu(next), &mut state)
                    && slot.flow != Flow::Stop
                {
                    self.owner.insert(next, (function, slot.len));
                    if let Some((t, kind)) = slot.data
                        && let Some(a) = rom.map.resolve(next, t)
                    {
                        self.refs.push((next, a, kind));
                    }
                }
                next += 4;
                if matches!(insn.flow, Flow::Call(_)) {
                    state.known &= !cpu::mips::CALL_CLOBBERS;
                }
            }
            if matches!(insn.flow, Flow::Return | Flow::Jump(_) | Flow::Trap) {
                self.ends.insert(next);
            }
            // An indirect jump (or the 6502's return trick) through a table next to it.
            if matches!(insn.flow, Flow::Jump(None) | Flow::Call(None) | Flow::Return) {
                let bytes_at = |a: u64| self.bytes_at(a);
                let resolve = |from: u64, t: u64| rom.map.resolve(from, t);
                let may_be_code = |a: u64| self.logged(a) & (flag::CODE | flag::DATA) != flag::DATA;
                let code = jumptable::Code {
                    bytes_at: &bytes_at,
                    resolve: &resolve,
                    may_be_code: &may_be_code,
                };
                let targets = jumptable::targets(rom.cpu, pc, rom.map.cpu(pc), state.thumb, &code);
                if targets.is_empty() && insn.flow == Flow::Jump(None) {
                    self.unresolved.insert(function);
                }
                for (t, thumb) in targets {
                    let mut s = state;
                    if let Some(thumb) = thumb {
                        s.thumb = thumb;
                    }
                    if matches!(insn.flow, Flow::Call(None)) {
                        self.refs.push((pc, t, RefKind::Call));
                        if let std::collections::btree_map::Entry::Vacant(e) = self.starts.entry(t) {
                            e.insert(s);
                            self.queue.push_back(t);
                        }
                    } else {
                        self.jumps.push((pc, t));
                        stack.push((t, s));
                    }
                }
            }
            // A target's address, and the state code there runs in (ARM or Thumb).
            let mut target_state = state;
            let target = insn.flow.target().and_then(|t| {
                let (t, thumb) = cpu::code_target(rom.cpu, t);
                if let Some(thumb) = thumb {
                    target_state.thumb = thumb;
                }
                rom.map.resolve(pc, t)
            });
            match insn.flow {
                Flow::Call(_) => {
                    if let Some(t) = target {
                        self.refs.push((pc, t, RefKind::Call));
                        if self.bytes_at(t).is_some() && !self.starts.contains_key(&t) {
                            self.starts.insert(t, target_state);
                            self.queue.push_back(t);
                        }
                    }
                    stack.push((next, state));
                }
                Flow::Jump(_) | Flow::Branch(_) => {
                    if let Some(t) = target {
                        // A jump to another function's start is a tail call.
                        if t != function && self.starts.contains_key(&t) {
                            self.refs.push((pc, t, RefKind::Jump));
                        } else {
                            self.jumps.push((pc, t));
                            stack.push((t, target_state));
                        }
                    }
                    if matches!(insn.flow, Flow::Branch(_)) {
                        stack.push((next, state));
                    }
                }
                Flow::Next | Flow::CondReturn => stack.push((next, state)),
                Flow::Return | Flow::Trap | Flow::Stop => {}
            }
        }
    }

    /// Follows every function queued, then each of `later` (code a log or a
    /// trace saw run) that nothing reached, one at a time: following one
    /// owns what it reaches, and the next one still unowned starts the next.
    fn drain(&mut self, later: &mut impl Iterator<Item = (u64, u16)>) {
        let rom = self.rom;
        loop {
            let function = match self.queue.pop_front() {
                Some(f) => f,
                None => {
                    let Some((a, f)) = later.find(|(a, _)| !self.owner.contains_key(a) && !self.starts.contains_key(a))
                    else {
                        return;
                    };
                    self.starts.insert(a, logged_state(rom.state, f, rom.cpu));
                    a
                }
            };
            let state = self.starts[&function];
            self.follow(function, function, state);
        }
    }

    /// The runs of decoded instructions: (start, end, function), in address order.
    fn runs(&self) -> Vec<(u64, u64, u64)> {
        let mut by_function: HashMap<u64, Vec<(u64, u32)>> = HashMap::new();
        for (&pc, &(function, len)) in &self.owner {
            by_function.entry(function).or_default().push((pc, len));
        }
        let mut runs = Vec::new();
        for (function, mut insns) in by_function {
            insns.sort_unstable();
            let mut run = (insns[0].0, insns[0].0 + insns[0].1 as u64);
            for &(pc, len) in &insns[1..] {
                if pc != run.1 {
                    runs.push((run.0, run.1, function));
                    run = (pc, pc);
                }
                run.1 = pc + len as u64;
            }
            runs.push((run.0, run.1, function));
        }
        runs.sort_unstable();
        runs
    }
}

/// How far a gap between functions is read as code: 16 KiB.
const GAP_LIMIT: u64 = 0x4000;
/// A hoisted head (what the compiler scheduled above a function's frame
/// setup) is at most this many instructions.
const HEAD_LIMIT: usize = 16;

/// What a gap of unreached bytes between two functions held, on the
/// compiler's habits (GCC's MIPS output): (where to follow from, the
/// function it belongs to, or itself when it is one of its own).
fn gap_seeds(w: &Walk<'_>, runs: &[(u64, u64, u64)]) -> Vec<(u64, u64)> {
    let rom = w.rom;
    let big = rom.cpu == Cpu::MipsR4300;
    let mut seeds = Vec::new();
    // An entry point (a function another image names, a note's function) is a
    // boundary: what sits before it in a gap is never made part of it, or the
    // function would start before the address the note gave it.
    let pinned: HashSet<u64> = rom.entries.iter().map(|e| e.1).collect();
    // Where a word's value would point: into the image, or anywhere loaded.
    let pointer_like = |v: u32| {
        let v = u64::from(v);
        w.sections
            .iter()
            .any(|s| s.loaded && v >= s.address && v < s.address + s.size)
    };
    let mut gaps: Vec<(u64, u64, Option<u64>, Option<u64>)> = Vec::new();
    // Gaps between consecutive runs of one section, and before the first run of each.
    for s in w.sections.iter().filter(|s| s.loaded && s.file_offset.is_some()) {
        let (lo, hi) = (s.address, s.address + s.file_size);
        let first = runs.partition_point(|r| r.0 < lo);
        let mut prev: Option<(u64, u64, u64)> = None;
        for r in runs[first..].iter().take_while(|r| r.0 < hi) {
            let (gap_start, before) = match prev {
                Some(p) => (p.1, Some(p.2)),
                None => (lo, None),
            };
            if r.0 > gap_start {
                gaps.push((gap_start, r.0, before, Some(r.2)));
            }
            prev = Some(*r);
        }
        match prev {
            Some(p) if p.1 < hi => gaps.push((p.1, hi, Some(p.2), None)),
            // Nothing followed in the section at all: read it as a trailing gap.
            None if hi > lo => gaps.push((lo, hi, None, None)),
            _ => {}
        }
    }
    for (gap_start, gap_end, before, after) in gaps {
        let len = gap_end - gap_start;
        if len < 4 || len % 4 != 0 {
            continue;
        }
        let trailing = after.is_none();
        let Some(bytes) = w.bytes_at(gap_start) else { continue };
        let words = (len.min(GAP_LIMIT) as usize / 4).min(bytes.len() / 4);
        // Every word decoded, up to the first that can't be code.
        let mut insns: Vec<cpu::Insn> = Vec::new();
        let mut clean = true;
        for k in 0..words {
            let pc = gap_start + 4 * k as u64;
            let b = &bytes[4 * k..4 * k + 4];
            let v = if big {
                u32::from_be_bytes([b[0], b[1], b[2], b[3]])
            } else {
                u32::from_le_bytes([b[0], b[1], b[2], b[3]])
            };
            let mut state = rom.state;
            let insn = cpu::decode(rom.cpu, b, rom.map.cpu(pc), &mut state);
            let data_only = w.logged(pc) & (flag::CODE | flag::DATA) == flag::DATA;
            match insn {
                Some(i) if i.flow != Flow::Stop && !pointer_like(v) && !data_only => insns.push(i),
                _ => {
                    clean = false;
                    break;
                }
            }
        }
        if insns.is_empty() || insns.iter().all(|i| i.mnemonic == "nop") {
            continue;
        }
        // Between two functions, data (a word that isn't code) means the gap is data;
        // after the last function, code is read as far as it goes.
        if !clean && !trailing {
            continue;
        }
        if words < len as usize / 4 && !trailing {
            continue;
        }
        // Where each branch and jump goes, by the index of the instruction.
        let target_of = |k: usize| -> Option<u64> {
            let pc = gap_start + 4 * k as u64;
            insns[k]
                .flow
                .target()
                .and_then(|t| rom.map.resolve(pc, cpu::code_target(rom.cpu, t).0))
        };
        let in_gap = |t: u64| t >= gap_start && t < gap_start + 4 * insns.len() as u64;
        let owned_by = |t: u64| w.owner.get(&t).map(|o| o.0);
        // Terminators: a return or an unconditional jump, each with its delay slot.
        let terminal = |k: usize| matches!(insns[k].flow, Flow::Return | Flow::Jump(_) | Flow::Trap);
        let mut cuts: Vec<usize> = Vec::new();
        let mut k = 0;
        while k < insns.len() {
            if terminal(k) {
                let after = if insns[k].delay_slot { k + 2 } else { k + 1 };
                cuts.push(after.min(insns.len()));
                k = after;
            } else {
                k += 1;
            }
        }
        let body_end = cuts.last().copied().unwrap_or(0);
        // The head: what falls through into the function after the gap (the
        // instructions the compiler hoisted above its frame setup).
        let nop = |k: usize| insns[k].mnemonic == "nop";
        // A piece's real start: past the nops padding the function before it.
        let trimmed = |from: usize, to: usize| (from..to).find(|&k| !nop(k)).unwrap_or(to);
        if let Some(next) = after
            && !trailing
            && let head_start = trimmed(body_end, insns.len())
            && head_start < insns.len()
            && insns.len() - head_start <= HEAD_LIMIT
        {
            let head = head_start..insns.len();
            let goes_before = head
                .clone()
                .any(|k| target_of(k).is_some_and(|t| owned_by(t).is_some() && owned_by(t) != Some(next)));
            if !goes_before {
                seeds.push((gap_start + 4 * head_start as u64, gap_start + 4 * head_start as u64));
            }
        }
        if body_end == 0 {
            continue;
        }
        // The body, which ends with a return or a jump: the function before the gap's
        // tail (its switch cases past a `jr` whose table wasn't found, code that
        // branches back into it), or functions of their own that nothing reaches by
        // a call (frameless leaves reached through a table of pointers).
        let branches_into = |who: u64| (0..body_end).any(|k| target_of(k).is_some_and(|t| owned_by(t) == Some(who)));
        if let Some(prev) = before
            && (w.unresolved.contains(&prev) || branches_into(prev))
        {
            seeds.push((gap_start, prev));
            continue;
        }
        if let Some(next) = after
            && !pinned.contains(&next)
            && branches_into(next)
        {
            seeds.push((gap_start, next));
            continue;
        }
        // Pieces: cut at a terminator no branch crosses. A piece is a function
        // when it does something besides returning (a lone `jr $ra` between
        // functions is padding or a stub nothing uses).
        let mut pieces: Vec<(usize, usize)> = Vec::new();
        let mut piece_start = 0;
        for &cut in &cuts {
            if cut > body_end {
                break;
            }
            let crosses = (piece_start..cut)
                .any(|k| target_of(k).is_some_and(|t| in_gap(t) && t >= gap_start + 4 * cut as u64))
                || (cut..body_end).any(|k| target_of(k).is_some_and(|t| in_gap(t) && t < gap_start + 4 * cut as u64));
            if !crosses {
                pieces.push((piece_start, cut));
                piece_start = cut;
            }
        }
        for (from, to) in pieces {
            let start = trimmed(from, to);
            if (start..to).filter(|&k| !nop(k)).count() >= 2 {
                seeds.push((gap_start + 4 * start as u64, gap_start + 4 * start as u64));
            }
        }
    }
    seeds
}

/// The functions as the runs make them. For MIPS, runs are joined up the way
/// the compiler laid the functions out: a run that falls through into another
/// function's code is part of it (a head hoisted above the frame setup, a
/// piece a trace seeded in the middle of a function, a short entry falling
/// into a function it shares its tail with), and one function's runs either
/// side of a gap of code nothing reached (a switch's cases) span it.
/// Returns the functions (start, size) and, for each run, the start of the
/// function it is in.
fn join_runs(w: &mut Walk<'_>, runs: &[(u64, u64, u64)], mips: bool) -> (Vec<(u64, u64)>, HashMap<u64, u64>) {
    let rom = w.rom;
    // Union-find over functions (by their start).
    let mut parent: HashMap<u64, u64> = HashMap::new();
    fn find(parent: &mut HashMap<u64, u64>, x: u64) -> u64 {
        let p = *parent.get(&x).unwrap_or(&x);
        if p == x {
            return x;
        }
        let root = find(parent, p);
        parent.insert(x, root);
        root
    }
    // A piece that falls through into another is part of it, called or not:
    // GCC lays a function that shares its tail with another out as a short
    // entry falling into the other's start, and a decompilation writes the
    // two as one function (the FF9 project matched twelve such; cutting them
    // at the second entry lost the matches).
    //
    // Except an entry point: a function another image names, or one a note
    // records (`Binary::with_function_boundaries`), is a boundary the joining
    // never removes, whatever falls through into it. The FF9 notes record the
    // fragments of a script interpreter and score them one by one.
    let pinned: HashSet<u64> = rom.entries.iter().map(|e| e.1).collect();
    if mips {
        for &(_, end, function) in runs {
            if w.ends.contains(&end) {
                continue;
            }
            if let Some(&(into, _)) = w.owner.get(&end)
                && into != function
            {
                let (a, b) = (find(&mut parent, function), find(&mut parent, into));
                if a != b {
                    // The group is named by its lowest start: the entry.
                    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                    if pinned.contains(&hi) {
                        continue;
                    }
                    parent.insert(hi, lo);
                }
            }
        }
    }
    let group_of: Vec<u64> = runs.iter().map(|r| find(&mut parent, r.2)).collect();
    // Which runs each group has, in address order.
    let mut groups: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    for (i, &g) in group_of.iter().enumerate() {
        groups.entry(g).or_default().push(i);
    }
    let (data, sections) = (w.data, w.sections);
    let all_zero = |lo: u64, hi: u64| {
        code_bytes(data, sections, lo)
            .is_some_and(|b| b.len() >= (hi - lo) as usize && b[..(hi - lo) as usize].iter().all(|&x| x == 0))
    };
    let mut functions = Vec::new();
    let mut run_function: HashMap<u64, u64> = HashMap::new();
    for (&g, members) in &groups {
        let state = w.starts.get(&g).copied().unwrap_or(rom.state);
        // The group's runs as stretches: consecutive runs joined when only a gap
        // of unreached code (not zeroes, not another function) lies between.
        let mut stretches: Vec<(u64, u64, Vec<usize>)> = Vec::new();
        for &j in members {
            let next = runs[j];
            let spans = match stretches.last() {
                Some(s) => {
                    let i = *s.2.last().unwrap();
                    let gap = next.0 - s.1;
                    mips && j == i + 1
                        && gap <= GAP_LIMIT * 4
                        && (gap == 0 || !all_zero(s.1, next.0))
                        && code_bytes(data, sections, s.1).is_some_and(|b| b.len() as u64 >= gap)
                }
                None => false,
            };
            match stretches.last_mut() {
                Some(s) if spans => {
                    s.1 = next.1;
                    s.2.push(j);
                }
                _ => stretches.push((next.0, next.1, vec![j])),
            }
        }
        for (start, end, members) in stretches {
            functions.push((start, end - start));
            w.starts.entry(start).or_insert(state);
            for i in members {
                run_function.insert(runs[i].0, start);
            }
        }
    }
    functions.sort_unstable();
    functions.dedup();
    (functions, run_function)
}

pub(crate) fn analyze(data: &[u8], sections: &[Section], rom: &Rom) -> Analysis {
    let log = rom.log.as_ref().map(|(l, _)| l.as_ref());
    let mips = matches!(rom.cpu, Cpu::MipsR3000 | Cpu::MipsR4300);
    let mut w = Walk {
        data,
        sections,
        rom,
        log,
        starts: BTreeMap::new(),
        queue: VecDeque::new(),
        owner: HashMap::new(),
        refs: Vec::new(),
        jumps: Vec::new(),
        ends: HashSet::new(),
        unresolved: HashSet::new(),
    };
    let entries: Vec<u64> = rom
        .vectors
        .iter()
        .map(|v| v.1)
        .chain(rom.entries.iter().map(|e| e.1).filter(|&a| !w.zeroes(a)))
        .collect();
    for address in entries {
        if w.bytes_at(address).is_some() {
            let state = logged_state(rom.state, w.logged(address), rom.cpu);
            w.start(address, state);
        }
    }
    if let Some(log) = log {
        for (a, f) in logged_code(log, sections, true) {
            w.start(a, logged_state(rom.state, f, rom.cpu));
        }
    }
    // Code the log saw run that following the code didn't reach, and the late
    // entries (prologues, a trace), for when the queue is done.
    let mut later: Vec<(u64, u16)> = log.map(|l| logged_code(l, sections, false)).unwrap_or_default();
    later.extend(rom.late_entries.iter().filter(|&&a| !w.zeroes(a)).map(|&a| (a, 0)));
    let mut later = later.into_iter();
    w.drain(&mut later);
    if mips {
        // What the gaps between functions hold, followed as part of the function
        // it belongs to; twice, since a function found in a gap can call more.
        for _ in 0..2 {
            let runs = w.runs();
            let seeds = gap_seeds(&w, &runs);
            if seeds.is_empty() {
                break;
            }
            for (pc, function) in seeds {
                if w.owner.contains_key(&pc) {
                    continue;
                }
                let state = w.starts.get(&function).copied().unwrap_or(rom.state);
                if pc == function {
                    w.starts.entry(pc).or_insert(state);
                }
                w.follow(function, pc, state);
            }
            w.drain(&mut std::iter::empty());
        }
    }
    let instructions = w.owner.len();
    let runs = w.runs();
    let (functions, run_function) = join_runs(&mut w, &runs, mips);
    let mut refs = std::mem::take(&mut w.refs);
    // A jump into another function past a gap enters it (the code it jumps to
    // is a function of its own): a reference.
    let function_starts: HashSet<u64> = functions.iter().map(|f| f.0).collect();
    let run_of = |pc: u64| {
        let i = runs.partition_point(|r| r.0 <= pc);
        (i > 0 && pc < runs[i - 1].1).then(|| runs[i - 1].0)
    };
    for &(pc, t) in &w.jumps {
        if function_starts.contains(&t) && run_of(pc).and_then(|r| run_function.get(&r)) != run_function.get(&t) {
            refs.push((pc, t, RefKind::Jump));
        }
    }
    let starts = std::mem::take(&mut w.starts);
    // Pointers to data the code builds, and the tables it builds them from.
    let mut found = pointers::Found::default();
    if matches!(rom.cpu, Cpu::Mos6502 | Cpu::W65816) {
        let bytes_at = |a: u64| code_bytes(data, sections, a);
        let runs: Vec<(u64, u64, State)> = functions
            .iter()
            .map(|&(start, size)| (start, size, starts.get(&start).copied().unwrap_or(rom.state)))
            .collect();
        let is_code = |a: u64| {
            let i = functions.partition_point(|f| f.0 <= a);
            i > 0 && a < functions[i - 1].0 + functions[i - 1].1
        };
        let read: HashSet<u64> = refs.iter().filter(|r| r.2 == RefKind::Read).map(|r| r.1).collect();
        let resolve = |from: u64, t: u64| rom.map.resolve(from, t);
        let cpu_of = |a: u64| rom.map.cpu(a);
        found = pointers::find(&pointers::Code {
            cpu: rom.cpu,
            runs: &runs,
            bytes_at: &bytes_at,
            resolve: &resolve,
            cpu_of: &cpu_of,
            is_code: &is_code,
            read: &read,
        });
        refs.append(&mut found.refs);
    }
    refs.sort_unstable();
    refs.dedup();
    Analysis {
        functions,
        refs,
        states: starts.into_iter().collect(),
        instructions,
        tables: found.tables,
        unplaced: found.unplaced,
    }
}
