//! Finding a ROM's code: nothing marks where code ends and data (graphics,
//! tables, text) begins, so the code is followed from the entry points
//! instead — each branch taken both ways, each call target a new function —
//! until every path ends in a return, a jump through a register or a table,
//! or bytes that don't decode. What the instructions on the way read, write
//! and call are the ROM's cross-references.

use std::collections::{BTreeMap, HashMap, VecDeque};

use super::Rom;
use crate::cpu::{self, Flow, State};
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

pub(crate) fn analyze(data: &[u8], sections: &[Section], rom: &Rom) -> Analysis {
    let bytes_at = |a: u64| code_bytes(data, sections, a);
    let mut starts: BTreeMap<u64, State> = BTreeMap::new();
    let mut queue: VecDeque<u64> = VecDeque::new();
    for &(_, address) in &rom.vectors {
        if bytes_at(address).is_some() && starts.insert(address, rom.state).is_none() {
            queue.push_back(address);
        }
    }
    // Which function each decoded instruction belongs to, and its length.
    let mut owner: HashMap<u64, (u64, u32)> = HashMap::new();
    let mut refs = Vec::new();
    // Jumps and branches within a function: (instruction, target).
    let mut jumps = Vec::new();
    while let Some(function) = queue.pop_front() {
        let mut stack = vec![(function, starts[&function])];
        while let Some((pc, mut state)) = stack.pop() {
            if owner.contains_key(&pc) || owner.len() >= LIMIT {
                continue;
            }
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
