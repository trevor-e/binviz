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

use std::collections::{BTreeMap, HashMap, VecDeque};

use super::Rom;
use super::cdl::{CodeDataLog, flag};
use super::jumptable;
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

pub(crate) fn analyze(data: &[u8], sections: &[Section], rom: &Rom) -> Analysis {
    let bytes_at = |a: u64| code_bytes(data, sections, a);
    let log = rom.log.as_ref().map(|(l, _)| l.as_ref());
    let logged = |a: u64| log.and_then(|l| offset_of(sections, a).map(|o| l.at(o))).unwrap_or(0);
    let mut starts: BTreeMap<u64, State> = BTreeMap::new();
    let mut queue: VecDeque<u64> = VecDeque::new();
    let entries = rom.vectors.iter().map(|v| v.1).chain(rom.entries.iter().map(|e| e.1));
    for address in entries {
        if bytes_at(address).is_some()
            && starts
                .insert(address, logged_state(rom.state, logged(address), rom.cpu))
                .is_none()
        {
            queue.push_back(address);
        }
    }
    if let Some(log) = log {
        for (a, f) in logged_code(log, sections, true) {
            if starts.insert(a, logged_state(rom.state, f, rom.cpu)).is_none() {
                queue.push_back(a);
            }
        }
    }
    // Which function each decoded instruction belongs to, and its length.
    let mut owner: HashMap<u64, (u64, u32)> = HashMap::new();
    let mut refs = Vec::new();
    // Jumps and branches within a function: (instruction, target).
    let mut jumps = Vec::new();
    // Code the log saw run that following the code didn't reach, for when the queue is done.
    let mut later = log
        .map(|l| logged_code(l, sections, false))
        .unwrap_or_default()
        .into_iter();
    while let Some(function) = queue.pop_front().or_else(|| {
        let (a, f) = later.find(|(a, _)| !owner.contains_key(a) && !starts.contains_key(a))?;
        starts.insert(a, logged_state(rom.state, f, rom.cpu));
        Some(a)
    }) {
        let mut stack = vec![(function, starts[&function])];
        while let Some((pc, mut state)) = stack.pop() {
            if owner.contains_key(&pc) || owner.len() >= LIMIT {
                continue;
            }
            let f = logged(pc);
            // Only ever read as data: not code, whatever leads here.
            if f & (flag::CODE | flag::DATA) == flag::DATA {
                continue;
            }
            state = logged_state(state, f, rom.cpu);
            let Some(bytes) = bytes_at(pc) else { continue };
            let Some(insn) = cpu::decode(rom.cpu, bytes, rom.map.cpu(pc), &mut state) else {
                continue;
            };
            if insn.flow == Flow::Stop {
                continue;
            }
            owner.insert(pc, (function, insn.len));
            let mut next = pc + insn.len as u64;
            if let Some((t, kind)) = insn.data
                && let Some(a) = rom.map.resolve(pc, t)
            {
                refs.push((pc, a, kind));
            }
            // MIPS: the instruction after a jump or branch (its delay slot) runs first.
            if insn.delay_slot {
                if let Some(bytes) = bytes_at(next)
                    && !owner.contains_key(&next)
                    && let Some(slot) = cpu::decode(rom.cpu, bytes, rom.map.cpu(next), &mut state)
                    && slot.flow != Flow::Stop
                {
                    owner.insert(next, (function, slot.len));
                    if let Some((t, kind)) = slot.data
                        && let Some(a) = rom.map.resolve(next, t)
                    {
                        refs.push((next, a, kind));
                    }
                }
                next += 4;
                if matches!(insn.flow, Flow::Call(_)) {
                    state.known &= !cpu::mips::CALL_CLOBBERS;
                }
            }
            // An indirect jump (or the 6502's return trick) through a table next to it.
            if matches!(insn.flow, Flow::Jump(None) | Flow::Call(None) | Flow::Return) {
                let resolve = |from: u64, t: u64| rom.map.resolve(from, t);
                let may_be_code = |a: u64| logged(a) & (flag::CODE | flag::DATA) != flag::DATA;
                let code = jumptable::Code {
                    bytes_at: &bytes_at,
                    resolve: &resolve,
                    may_be_code: &may_be_code,
                };
                for (t, thumb) in jumptable::targets(rom.cpu, pc, rom.map.cpu(pc), state.thumb, &code) {
                    let mut s = state;
                    if let Some(thumb) = thumb {
                        s.thumb = thumb;
                    }
                    if matches!(insn.flow, Flow::Call(None)) {
                        refs.push((pc, t, RefKind::Call));
                        if let std::collections::btree_map::Entry::Vacant(e) = starts.entry(t) {
                            e.insert(s);
                            queue.push_back(t);
                        }
                    } else {
                        jumps.push((pc, t));
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
                        refs.push((pc, t, RefKind::Call));
                        if bytes_at(t).is_some() && !starts.contains_key(&t) {
                            starts.insert(t, target_state);
                            queue.push_back(t);
                        }
                    }
                    stack.push((next, state));
                }
                Flow::Jump(_) | Flow::Branch(_) => {
                    if let Some(t) = target {
                        // A jump to another function's start is a tail call.
                        if t != function && starts.contains_key(&t) {
                            refs.push((pc, t, RefKind::Jump));
                        } else {
                            jumps.push((pc, t));
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
    let instructions = owner.len();
    // A function is the run of instructions from its start; code it jumps to past a
    // gap (over data, say) is a function of its own, entered by that jump.
    let mut by_function: HashMap<u64, Vec<(u64, u32)>> = HashMap::new();
    for (&pc, &(function, len)) in &owner {
        by_function.entry(function).or_default().push((pc, len));
    }
    let mut functions = Vec::new();
    let mut chunk_of: HashMap<u64, u64> = HashMap::new();
    for (function, mut insns) in by_function {
        insns.sort_unstable();
        let state = starts.get(&function).copied().unwrap_or(rom.state);
        let mut run = (insns[0].0, insns[0].0 + insns[0].1 as u64);
        chunk_of.insert(insns[0].0, run.0);
        for &(pc, len) in &insns[1..] {
            if pc != run.1 {
                functions.push((run.0, run.1 - run.0));
                starts.entry(pc).or_insert(state);
                run = (pc, pc);
            }
            run.1 = pc + len as u64;
            chunk_of.insert(pc, run.0);
        }
        functions.push((run.0, run.1 - run.0));
    }
    functions.sort_unstable();
    let chunk_starts: std::collections::HashSet<u64> = functions.iter().map(|f| f.0).collect();
    for (pc, t) in jumps {
        if chunk_starts.contains(&t) && chunk_of.get(&pc) != Some(&t) {
            refs.push((pc, t, RefKind::Jump));
        }
    }
    refs.sort_unstable();
    refs.dedup();
    Analysis {
        functions,
        refs,
        states: starts.into_iter().collect(),
        instructions,
    }
}
