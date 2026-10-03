//! Bounded PS1 incoming-GPR liveness over an authoritative exact code extent.
//!
//! This is a first-read audit, not an emulator or ABI argument inference.
//! Conditional successors are followed; register branch targets are captured
//! before their slot. Delayed loads retain the old GPR word for the following
//! instruction; LWL/LWR pending-load merge forwarding is handled separately.
//! Unknown calls have no assumed ABI clobbers. Returns and reviewed summaries
//! are explicit policy, whose evidence identifiers are retained in witnesses.
//!
//! The caller supplies valid R3000A words and a verified exact extent. File
//! membership/hashes, branch feasibility, memory safety, exceptions/IRQs and
//! GTE arithmetic are outside this audit. C prototypes are not callee proofs.
//! Dead includes discarded returns as well as kills: inspect the endpoints
//! before turning an audit into another function's kill summary.
use crate::cpu::mips::{MipsWord, ps1_gpr_effects};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};

/// Exact contiguous original instruction extent. Words are decoded by the
/// caller using the chosen byte order (little endian for PS1).
#[derive(Debug, Clone, Copy)]
pub struct ExactExtent<'a> {
    pub start: u32,
    pub words: &'a [u32],
}
impl ExactExtent<'_> {
    fn word(self, pc: u32) -> Option<u32> {
        let offset = pc.checked_sub(self.start)?;
        if offset & 3 != 0 {
            return None;
        }
        self.words.get((offset / 4) as usize).copied()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ReturnUse {
    #[default]
    Unresolved,
    /// Reviewed caller discards this register at return; not an actual kill.
    Discarded,
    /// Reviewed return exposes this register's surviving word.
    Consumed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CalleeEffect {
    Consumed,
    Killed,
    /// No read/kill; all paths return the register unchanged.
    Preserved,
    Unresolved,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionPoint {
    pub pc: u32,
    pub word: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalleeSummary {
    pub register: u8,
    pub effect: CalleeEffect,
    /// Required reviewed native evidence identifier, not an ABI signature.
    pub evidence: String,
    #[serde(default)]
    pub instruction_path: Vec<InstructionPoint>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewedTargets {
    /// All possible destinations, not just an observed dynamic target.
    pub targets: Vec<u32>,
    pub evidence: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditPolicy {
    #[serde(default)]
    pub return_use: ReturnUse,
    #[serde(default)]
    pub indirect_targets: BTreeMap<u32, ReviewedTargets>,
    #[serde(default)]
    pub callees: BTreeMap<u32, CalleeSummary>,
    #[serde(default = "default_state_budget")]
    pub max_states: usize,
}
fn default_state_budget() -> usize {
    16384
}
impl Default for AuditPolicy {
    fn default() -> Self {
        Self {
            return_use: ReturnUse::Unresolved,
            indirect_targets: BTreeMap::new(),
            callees: BTreeMap::new(),
            max_states: 16384,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Consumed,
    Dead,
    Unresolved,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WitnessKind {
    Read,
    End,
    Frontier,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceStep {
    pub pc: u32,
    /// None for outside-extent or exhausted-budget successors.
    pub word: Option<u32>,
    pub delay_slot: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Witness {
    pub kind: WitnessKind,
    pub reason: String,
    pub path: Vec<TraceStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewed_instruction_path: Vec<InstructionPoint>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReport {
    pub register: u8,
    pub entry: u32,
    pub extent_start: u32,
    pub extent_words: usize,
    /// Supplied assumptions, including preserved callees on continuation paths.
    pub policy: AuditPolicy,
    pub outcome: Outcome,
    pub states: usize,
    pub reads: Vec<Witness>,
    pub endpoints: Vec<Witness>,
    pub frontiers: Vec<Witness>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditError(pub String);
impl std::fmt::Display for AuditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for AuditError {}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Transfer {
    Flow(Vec<u32>),
    Jumps(Vec<u32>),
    Return,
    Calls { targets: Vec<u32>, resume: Option<u32> },
    Unknown(&'static str),
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct State {
    pc: u32,
    pending_kill: bool,
    post: Option<Transfer>,
}
struct Node {
    state: State,
    parent: Option<usize>,
    edges: Vec<usize>,
}
struct Auditor<'a> {
    extent: ExactExtent<'a>,
    policy: &'a AuditPolicy,
    register: u8,
    report: AuditReport,
    nodes: Vec<Node>,
    seen: HashMap<State, usize>,
    queue: VecDeque<usize>,
}
impl Auditor<'_> {
    fn step(&self, index: usize) -> TraceStep {
        let s = &self.nodes[index].state;
        TraceStep {
            pc: s.pc,
            word: self.extent.word(s.pc),
            delay_slot: s.post.is_some(),
        }
    }
    fn path(&self, mut index: usize) -> Vec<TraceStep> {
        let mut path = vec![];
        loop {
            path.push(self.step(index));
            if let Some(parent) = self.nodes[index].parent {
                index = parent;
            } else {
                break;
            }
        }
        path.reverse();
        path
    }
    fn witness(
        &mut self,
        index: usize,
        kind: WitnessKind,
        reason: &str,
        target: Option<u32>,
        summary: Option<&CalleeSummary>,
    ) {
        let w = Witness {
            kind,
            reason: reason.into(),
            path: self.path(index),
            target,
            evidence: summary.map(|s| s.evidence.clone()),
            reviewed_instruction_path: summary.map(|s| s.instruction_path.clone()).unwrap_or_default(),
        };
        match kind {
            WitnessKind::Read => self.report.reads.push(w),
            WitnessKind::End => self.report.endpoints.push(w),
            WitnessKind::Frontier => self.report.frontiers.push(w),
        }
    }
    fn enqueue(&mut self, parent: usize, state: State) {
        if let Some(&index) = self.seen.get(&state) {
            self.nodes[parent].edges.push(index);
            return;
        }
        if self.nodes.len() >= self.policy.max_states {
            self.witness(
                parent,
                WitnessKind::Frontier,
                "state budget exhausted before successor",
                Some(state.pc),
                None,
            );
            return;
        }
        let index = self.nodes.len();
        self.seen.insert(state.clone(), index);
        self.nodes.push(Node {
            state,
            parent: Some(parent),
            edges: vec![],
        });
        self.nodes[parent].edges.push(index);
        self.queue.push_back(index);
    }
    fn returned(&mut self, index: usize) {
        match self.policy.return_use {
            ReturnUse::Discarded => self.witness(
                index,
                WitnessKind::End,
                "reviewed discarded return after delay slot; not a kill",
                None,
                None,
            ),
            ReturnUse::Consumed => self.witness(
                index,
                WitnessKind::Read,
                "reviewed return exposes surviving register after delay slot",
                None,
                None,
            ),
            ReturnUse::Unresolved => self.witness(
                index,
                WitnessKind::Frontier,
                "surviving register at return; return use unreviewed",
                None,
                None,
            ),
        }
    }
    fn calls(&mut self, index: usize, targets: &[u32], resume: Option<u32>, pending: bool) {
        for &target in targets {
            if target & 3 != 0 {
                self.witness(
                    index,
                    WitnessKind::Frontier,
                    "unaligned reviewed transfer target",
                    Some(target),
                    None,
                );
                continue;
            }
            let Some(s) = self.policy.callees.get(&target) else {
                self.witness(
                    index,
                    WitnessKind::Frontier,
                    "unreviewed callee; no ABI clobber assumption",
                    Some(target),
                    None,
                );
                continue;
            };
            if s.evidence.trim().is_empty() || s.register != self.register {
                self.witness(
                    index,
                    WitnessKind::Frontier,
                    "callee summary lacks matching reviewed register/evidence",
                    Some(target),
                    Some(s),
                );
                continue;
            }
            if pending {
                self.witness(
                    index,
                    WitnessKind::Frontier,
                    "delayed load enters callee; first-instruction contract required",
                    Some(target),
                    Some(s),
                );
                continue;
            }
            match s.effect {
                CalleeEffect::Consumed => self.witness(
                    index,
                    WitnessKind::Read,
                    "reviewed native callee consumes incoming word",
                    Some(target),
                    Some(s),
                ),
                CalleeEffect::Killed => self.witness(
                    index,
                    WitnessKind::End,
                    "reviewed native callee kills word before reading",
                    Some(target),
                    Some(s),
                ),
                CalleeEffect::Unresolved => self.witness(
                    index,
                    WitnessKind::Frontier,
                    "reviewed callee remains unresolved",
                    Some(target),
                    Some(s),
                ),
                CalleeEffect::Preserved => {
                    if let Some(pc) = resume {
                        self.enqueue(
                            index,
                            State {
                                pc,
                                pending_kill: false,
                                post: None,
                            },
                        );
                    } else {
                        self.returned(index);
                    }
                }
            }
        }
    }
    fn transfer(&self, pc: u32, w: MipsWord) -> Option<Transfer> {
        let branch = || {
            pc.wrapping_add(4)
                .wrapping_add((w.simm() as i32).wrapping_mul(4) as u32)
        };
        let jump = || (pc.wrapping_add(4) & 0xf0000000) | ((w.0 & 0x03ffffff) << 2);
        let indirect = || {
            self.policy
                .indirect_targets
                .get(&pc)
                .filter(|r| !r.evidence.trim().is_empty() && !r.targets.is_empty())
                .map(|r| r.targets.clone())
        };
        match (w.op(), w.funct()) {
            (1, _) | (4..=7, _) => Some(Transfer::Flow(vec![pc.wrapping_add(8), branch()])),
            (2, _) => Some(Transfer::Jumps(vec![jump()])),
            (3, _) => Some(Transfer::Calls {
                targets: vec![jump()],
                resume: Some(pc.wrapping_add(8)),
            }),
            (0, 8) if w.rs() == 31 => Some(Transfer::Return),
            (0, 8) => Some(
                indirect()
                    .map(Transfer::Jumps)
                    .unwrap_or(Transfer::Unknown("unreviewed indirect jump")),
            ),
            (0, 9) if w.rd() == 0 => Some(
                indirect()
                    .map(Transfer::Jumps)
                    .unwrap_or(Transfer::Unknown("unreviewed indirect tail jump")),
            ),
            (0, 9) if w.rd() == 31 => Some(
                indirect()
                    .map(|targets| Transfer::Calls {
                        targets,
                        resume: Some(pc.wrapping_add(8)),
                    })
                    .unwrap_or(Transfer::Unknown("unreviewed indirect call")),
            ),
            (0, 9) => Some(Transfer::Unknown(
                "non-RA indirect link needs a reviewed continuation contract",
            )),
            _ => None,
        }
    }
    fn execute(&mut self, index: usize) {
        let state = self.nodes[index].state.clone();
        let Some(word) = self.extent.word(state.pc) else {
            self.witness(
                index,
                WitnessKind::Frontier,
                "outside authoritative exact extent",
                None,
                None,
            );
            return;
        };
        let w = MipsWord(word);
        let Some(e) = ps1_gpr_effects(word) else {
            self.witness(
                index,
                WitnessKind::Frontier,
                "unknown or unsupported PS1 instruction",
                None,
                None,
            );
            return;
        };
        let transfer = self.transfer(state.pc, w);
        if state.post.is_some() && transfer.is_some() {
            self.witness(
                index,
                WitnessKind::Frontier,
                "control instruction in delay slot is unpredictable",
                None,
                None,
            );
            return;
        }
        if (w.op() == 0 && matches!(w.funct(), 12 | 13)) || (w.op() == 16 && w.rs() >= 16) {
            self.witness(
                index,
                WitnessKind::Frontier,
                "explicit exception or privileged control boundary",
                None,
                None,
            );
            return;
        }
        let bit = 1u32 << self.register;
        if e.reads & bit != 0 || (!state.pending_kill && e.merge_reads & bit != 0) {
            self.witness(
                index,
                WitnessKind::Read,
                "instruction reads incoming word before write/load commit",
                None,
                None,
            );
            return;
        }
        if state.pending_kill || (e.write == Some(self.register) && !e.delayed_write) {
            self.witness(
                index,
                WitnessKind::End,
                if state.pending_kill {
                    "delayed load commits after old-value read opportunity"
                } else {
                    "immediate register overwrite before any read"
                },
                None,
                None,
            );
            return;
        }
        let pending = e.write == Some(self.register) && e.delayed_write;
        if let Some(post) = state.post {
            match post {
                Transfer::Flow(targets) => {
                    for pc in targets {
                        self.enqueue(
                            index,
                            State {
                                pc,
                                pending_kill: pending,
                                post: None,
                            },
                        );
                    }
                }
                Transfer::Jumps(targets) => {
                    for pc in targets {
                        if self.extent.word(pc).is_some() {
                            self.enqueue(
                                index,
                                State {
                                    pc,
                                    pending_kill: pending,
                                    post: None,
                                },
                            );
                        } else {
                            self.calls(index, &[pc], None, pending);
                        }
                    }
                }
                Transfer::Return => {
                    if pending {
                        self.witness(
                            index,
                            WitnessKind::Frontier,
                            "delayed load leaves return slot; caller first instruction required",
                            None,
                            None,
                        );
                    } else {
                        self.returned(index);
                    }
                }
                Transfer::Calls { targets, resume } => self.calls(index, &targets, resume, pending),
                Transfer::Unknown(reason) => self.witness(index, WitnessKind::Frontier, reason, None, None),
            }
        } else {
            self.enqueue(
                index,
                State {
                    pc: state.pc.wrapping_add(4),
                    pending_kill: pending,
                    post: transfer,
                },
            );
        }
    }
    /// Graph cycles are distinct from convergent diamonds.
    fn cycles(&mut self) {
        let mut color = vec![0u8; self.nodes.len()];
        for root in 0..self.nodes.len() {
            if color[root] != 0 {
                continue;
            }
            color[root] = 1;
            let mut stack = vec![(root, 0usize)];
            while let Some(&(node, edge)) = stack.last() {
                if edge == self.nodes[node].edges.len() {
                    color[node] = 2;
                    stack.pop();
                    continue;
                }
                stack.last_mut().unwrap().1 += 1;
                let next = self.nodes[node].edges[edge];
                if color[next] == 0 {
                    color[next] = 1;
                    stack.push((next, 0));
                } else if color[next] == 1 {
                    let mut path = self.path(root);
                    path.extend(stack.iter().skip(1).map(|&(n, _)| self.step(n)));
                    path.push(self.step(next));
                    self.report.frontiers.push(Witness {
                        kind: WitnessKind::Frontier,
                        reason: "live CFG cycle; termination unproved".into(),
                        path,
                        target: Some(self.nodes[next].state.pc),
                        evidence: None,
                        reviewed_instruction_path: vec![],
                    });
                }
            }
        }
    }
}
/// Audit an incoming architectural GPR word with no initially pending load.
/// A post-JAL result audit starts after the call slot. Pending loads originating
/// outside the extent require a separate entry contract.
pub fn audit(
    extent: ExactExtent<'_>,
    entry: u32,
    register: u8,
    policy: &AuditPolicy,
) -> Result<AuditReport, AuditError> {
    if extent.start & 3 != 0
        || extent.words.is_empty()
        || extent.start as u64 + extent.words.len() as u64 * 4 > 0x1_0000_0000
    {
        return Err(AuditError(
            "extent must be nonempty, aligned, and fit 32-bit address space".into(),
        ));
    }
    if !(1..32).contains(&register) || extent.word(entry).is_none() || policy.max_states == 0 {
        return Err(AuditError("entry/register/state budget is invalid".into()));
    }
    let state = State {
        pc: entry,
        pending_kill: false,
        post: None,
    };
    let mut a = Auditor {
        extent,
        policy,
        register,
        report: AuditReport {
            register,
            entry,
            extent_start: extent.start,
            extent_words: extent.words.len(),
            policy: policy.clone(),
            outcome: Outcome::Unresolved,
            states: 0,
            reads: vec![],
            endpoints: vec![],
            frontiers: vec![],
        },
        nodes: vec![Node {
            state: state.clone(),
            parent: None,
            edges: vec![],
        }],
        seen: HashMap::from([(state, 0)]),
        queue: VecDeque::from([0]),
    };
    while let Some(index) = a.queue.pop_front() {
        a.execute(index);
    }
    a.cycles();
    a.report.states = a.nodes.len();
    a.report.outcome = if !a.report.reads.is_empty() {
        Outcome::Consumed
    } else if !a.report.frontiers.is_empty() {
        Outcome::Unresolved
    } else if !a.report.endpoints.is_empty() {
        Outcome::Dead
    } else {
        Outcome::Unresolved
    };
    Ok(a.report)
}
impl crate::Binary {
    /// Audit an exact caller-supplied loaded extent with the explicit PS1
    /// profile. No discovered function boundary extends or clips the size.
    pub fn audit_ps1_register(
        &self,
        address: u64,
        size: usize,
        entry: u64,
        register: u8,
        policy: &AuditPolicy,
    ) -> Result<AuditReport, AuditError> {
        if self.mips_endian() != Some(crate::util::Endian::Little) {
            return Err(AuditError(
                "PS1 audit requires supported little-endian MIPS input".into(),
            ));
        }
        let end = address
            .checked_add(size as u64)
            .ok_or_else(|| AuditError("exact extent overflows".into()))?;
        if address > u32::MAX as u64 || end > 0x1_0000_0000 || address & 3 != 0 || size == 0 || size & 3 != 0 {
            return Err(AuditError(
                "exact extent must be aligned, nonempty and fit PS1 32-bit addresses".into(),
            ));
        }
        if entry < address || entry >= end || entry & 3 != 0 || entry > u32::MAX as u64 {
            return Err(AuditError("entry is outside the aligned exact extent".into()));
        }
        let bytes = self
            .code_bytes(address)
            .and_then(|b| b.get(..size))
            .ok_or_else(|| AuditError("complete exact extent is not backed by loaded file bytes".into()))?;
        let words: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        audit(
            ExactExtent {
                start: address as u32,
                words: &words,
            },
            entry as u32,
            register,
            policy,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const BASE: u32 = 0x8001_0000;
    fn i(op: u32, rs: u32, rt: u32, imm: i32) -> u32 {
        (op << 26) | (rs << 21) | (rt << 16) | (imm as u32 & 65535)
    }
    fn r(f: u32, rs: u32, rt: u32, rd: u32) -> u32 {
        (rs << 21) | (rt << 16) | (rd << 11) | f
    }
    fn j(op: u32, target: u32) -> u32 {
        (op << 26) | ((target >> 2) & 0x3ff_ffff)
    }
    fn run(words: &[u32], reg: u8, policy: &AuditPolicy) -> AuditReport {
        audit(ExactExtent { start: BASE, words }, BASE, reg, policy).unwrap()
    }
    fn void_policy() -> AuditPolicy {
        AuditPolicy {
            return_use: ReturnUse::Discarded,
            ..Default::default()
        }
    }

    #[test]
    fn twelve_existing_boundary_cases() {
        let cases = [
            ("stack-store", vec![i(43, 29, 2, 16)], Outcome::Consumed),
            ("branch-test", vec![i(4, 2, 0, 1), 0, i(15, 0, 2, 1)], Outcome::Consumed),
            (
                "branch-delay-store",
                vec![i(4, 0, 0, 1), i(43, 29, 2, 16), i(15, 0, 2, 1)],
                Outcome::Consumed,
            ),
            (
                "load-old-value-use",
                vec![i(35, 29, 2, 0), r(33, 2, 0, 3)],
                Outcome::Consumed,
            ),
            ("load-nop-kill", vec![i(35, 29, 2, 0), 0], Outcome::Dead),
            (
                "read-before-same-register-write",
                vec![i(9, 2, 2, 1)],
                Outcome::Consumed,
            ),
            ("overwrite", vec![i(15, 0, 2, 1)], Outcome::Dead),
            ("void-return-delay-nop", vec![r(8, 31, 0, 0), 0], Outcome::Dead),
            (
                "void-return-delay-store",
                vec![r(8, 31, 0, 0), i(43, 29, 2, 16)],
                Outcome::Consumed,
            ),
            (
                "unknown-callee-not-ABI-clobbered",
                vec![j(3, 0x8002_0000), 0],
                Outcome::Unresolved,
            ),
            (
                "branch-load-delay-store",
                vec![i(35, 29, 2, 0), i(4, 0, 0, 1), i(43, 29, 2, 16), i(15, 0, 2, 1)],
                Outcome::Dead,
            ),
            (
                "jump-delay-load-kill",
                vec![j(2, BASE + 8), i(35, 29, 2, 0), 0],
                Outcome::Dead,
            ),
        ];
        for (name, words, expected) in cases {
            let out = run(&words, 2, &void_policy());
            assert_eq!(out.outcome, expected, "{name}: {out:?}");
            assert!(
                out.reads
                    .iter()
                    .chain(&out.endpoints)
                    .chain(&out.frontiers)
                    .all(|w| !w.path.is_empty())
            );
        }
    }
    #[test]
    fn a2_a3_read_before_write_hazards() {
        for reg in [6, 7] {
            assert_eq!(
                run(&[i(9, reg as u32, reg as u32, 1)], reg, &void_policy()).outcome,
                Outcome::Consumed
            );
            assert_eq!(
                run(
                    &[i(35, 29, reg as u32, 0), i(43, 29, reg as u32, 16)],
                    reg,
                    &void_policy()
                )
                .outcome,
                Outcome::Consumed
            );
            assert_eq!(
                run(
                    &[i(35, 29, reg as u32, 0), 0, i(43, 29, reg as u32, 16)],
                    reg,
                    &void_policy()
                )
                .outcome,
                Outcome::Dead
            );
        }
    }
    #[test]
    fn load_in_jump_slot_reads_old_word_at_target() {
        let out = run(&[j(2, BASE + 8), i(35, 29, 2, 0), i(43, 29, 2, 16)], 2, &void_policy());
        assert_eq!(out.outcome, Outcome::Consumed);
        assert!(out.reads[0].path[1].delay_slot);
        assert!(!out.reads[0].path[2].delay_slot);
    }
    #[test]
    fn merge_load_forwarding_is_not_an_old_register_read() {
        for op in [34, 38] {
            assert_eq!(run(&[i(op, 29, 2, 0)], 2, &void_policy()).outcome, Outcome::Consumed);
            assert_eq!(
                run(&[i(35, 29, 2, 0), i(op, 29, 2, 0)], 2, &void_policy()).outcome,
                Outcome::Dead
            );
            // Forwarding the merge source cannot bypass the architectural base.
            assert_eq!(
                run(&[i(35, 29, 2, 0), i(op, 2, 2, 0)], 2, &void_policy()).outcome,
                Outcome::Consumed
            );
        }
    }
    #[test]
    fn coprocessor_delayed_reads_and_memory_bases() {
        for op in [16, 18] {
            let load = i(op, 0, 2, 0);
            assert_eq!(
                run(&[load, i(43, 29, 2, 16)], 2, &void_policy()).outcome,
                Outcome::Consumed
            );
            assert_eq!(run(&[load, 0], 2, &void_policy()).outcome, Outcome::Dead);
        }
        assert_eq!(run(&[i(18, 4, 2, 0)], 2, &void_policy()).outcome, Outcome::Consumed);
        assert_eq!(run(&[i(18, 6, 2, 0)], 2, &void_policy()).outcome, Outcome::Consumed);
        assert_eq!(run(&[i(50, 2, 0, 0)], 2, &void_policy()).outcome, Outcome::Consumed);
        assert_eq!(run(&[i(58, 2, 0, 0)], 2, &void_policy()).outcome, Outcome::Consumed);
    }
    #[test]
    fn all_branch_successors_and_extent_frontier_retained() {
        // One path kills, the other reads.
        let out = run(
            &[i(4, 4, 0, 3), 0, i(15, 0, 2, 1), 0, i(43, 29, 2, 16)],
            2,
            &void_policy(),
        );
        assert_eq!(out.outcome, Outcome::Consumed);
        assert_eq!(out.reads.len(), 1);
        assert_eq!(out.endpoints.len(), 1);
        let out = run(&[i(4, 4, 0, 3), 0, i(15, 0, 2, 1)], 2, &void_policy());
        assert_eq!(out.outcome, Outcome::Unresolved);
        assert_eq!(out.frontiers[0].path.last().unwrap().pc, BASE + 16);
        assert_eq!(out.frontiers[0].path.last().unwrap().word, None);
    }
    #[test]
    fn branch_register_read_precedes_slot_overwrite() {
        assert_eq!(
            run(&[i(4, 2, 0, 1), i(15, 0, 2, 1), 0], 2, &void_policy()).outcome,
            Outcome::Consumed
        );
        assert_eq!(
            run(&[r(8, 2, 0, 0), i(15, 0, 2, 1)], 2, &void_policy()).outcome,
            Outcome::Consumed
        );
    }
    #[test]
    fn illegal_delay_control_and_unknown_instructions_are_frontiers() {
        for word in [j(2, BASE), 0xffff_ffff, 0x4200_0010] {
            assert_eq!(run(&[j(2, BASE), word], 2, &void_policy()).outcome, Outcome::Unresolved);
        }
        assert_eq!(run(&[0x0000_000d], 2, &void_policy()).outcome, Outcome::Unresolved);
        // N64 branch-likely and conditional link are not guessed as PS1 flow.
        assert_eq!(run(&[i(20, 4, 0, 1)], 2, &void_policy()).outcome, Outcome::Unresolved);
        assert_eq!(run(&[i(1, 4, 16, 1)], 2, &void_policy()).outcome, Outcome::Unresolved);
    }
    #[test]
    fn return_contract_is_explicit_and_slot_load_is_unresolved() {
        let words = [r(8, 31, 0, 0), 0];
        assert_eq!(run(&words, 2, &AuditPolicy::default()).outcome, Outcome::Unresolved);
        assert_eq!(
            run(
                &words,
                2,
                &AuditPolicy {
                    return_use: ReturnUse::Consumed,
                    ..Default::default()
                }
            )
            .outcome,
            Outcome::Consumed
        );
        assert_eq!(run(&words, 2, &void_policy()).outcome, Outcome::Dead);
        assert_eq!(
            run(&[r(8, 31, 0, 0), i(35, 29, 2, 0)], 2, &void_policy()).outcome,
            Outcome::Unresolved
        );
    }
    fn summary(effect: CalleeEffect) -> CalleeSummary {
        CalleeSummary {
            register: 2,
            effect,
            evidence: "reviewed-synthetic-native".into(),
            instruction_path: vec![InstructionPoint {
                pc: 0x8002_0000,
                word: i(15, 0, 2, 1),
            }],
        }
    }
    #[test]
    fn reviewed_call_effects_preserve_their_evidence() {
        let words = [j(3, 0x8002_0000), 0, i(43, 29, 2, 16)];
        for (effect, expected) in [
            (CalleeEffect::Consumed, Outcome::Consumed),
            (CalleeEffect::Killed, Outcome::Dead),
            (CalleeEffect::Preserved, Outcome::Consumed),
            (CalleeEffect::Unresolved, Outcome::Unresolved),
        ] {
            let mut p = void_policy();
            p.callees.insert(0x8002_0000, summary(effect));
            let out = run(&words, 2, &p);
            assert_eq!(out.outcome, expected);
            if effect == CalleeEffect::Killed {
                assert_eq!(out.endpoints[0].reviewed_instruction_path.len(), 1);
                assert_eq!(out.endpoints[0].evidence.as_deref(), Some("reviewed-synthetic-native"));
            }
        }
        let mut p = void_policy();
        let mut s = summary(CalleeEffect::Killed);
        s.register = 7;
        p.callees.insert(0x8002_0000, s);
        assert_eq!(run(&words, 2, &p).outcome, Outcome::Unresolved);
        p.callees.insert(0x8002_0000, summary(CalleeEffect::Killed));
        assert_eq!(
            run(&[j(3, 0x8002_0000), i(35, 29, 2, 0)], 2, &p).outcome,
            Outcome::Unresolved
        );
    }
    #[test]
    fn reviewed_indirect_targets_are_complete_and_captured_before_slot() {
        let words = [r(8, 8, 0, 0), i(15, 0, 8, 0), i(15, 0, 2, 0), i(43, 29, 2, 0)];
        let mut p = void_policy();
        p.indirect_targets.insert(
            BASE,
            ReviewedTargets {
                targets: vec![BASE + 8, BASE + 12],
                evidence: "reviewed full table".into(),
            },
        );
        let out = run(&words, 2, &p);
        assert_eq!(out.outcome, Outcome::Consumed);
        assert_eq!(out.endpoints.len(), 1);
        p.indirect_targets.get_mut(&BASE).unwrap().targets.clear();
        assert_eq!(run(&words, 2, &p).outcome, Outcome::Unresolved);
    }
    #[test]
    fn indirect_tail_calls_do_not_invent_a_return_to_pc_plus_eight() {
        let words = [r(9, 8, 0, 0), 0, i(43, 29, 2, 0)];
        let mut p = void_policy();
        p.indirect_targets.insert(
            BASE,
            ReviewedTargets {
                targets: vec![0x8002_0000],
                evidence: "reviewed constant tail target".into(),
            },
        );
        p.callees.insert(0x8002_0000, summary(CalleeEffect::Preserved));
        let out = run(&words, 2, &p);
        assert_eq!(out.outcome, Outcome::Dead);
        assert!(out.endpoints[0].reason.contains("not a kill"));
        assert_eq!(out.policy.callees.len(), 1);
        assert_eq!(out.endpoints[0].path.len(), 2);
        let words = [r(9, 8, 0, 5), 0, i(43, 29, 2, 0)];
        assert_eq!(run(&words, 2, &p).outcome, Outcome::Unresolved);
    }
    #[test]
    fn mixed_local_and_reviewed_external_jump_targets() {
        let words = [r(8, 8, 0, 0), 0, i(15, 0, 2, 0)];
        let mut p = void_policy();
        p.indirect_targets.insert(
            BASE,
            ReviewedTargets {
                targets: vec![BASE + 8, 0x8002_0000],
                evidence: "reviewed complete jump table".into(),
            },
        );
        assert_eq!(run(&words, 2, &p).outcome, Outcome::Unresolved);
        p.callees.insert(0x8002_0000, summary(CalleeEffect::Killed));
        assert_eq!(run(&words, 2, &p).outcome, Outcome::Dead);
        p.indirect_targets.get_mut(&BASE).unwrap().targets = vec![0x8002_0001];
        p.callees.insert(0x8002_0001, summary(CalleeEffect::Killed));
        let out = run(&words, 2, &p);
        assert_eq!(out.outcome, Outcome::Unresolved);
        assert!(out.frontiers[0].reason.contains("unaligned"));
    }
    #[test]
    fn cycles_are_frontiers_but_diamond_joins_are_not() {
        let words = [i(4, 4, 0, 1), 0, j(2, BASE), 0];
        let out = run(&words, 2, &void_policy());
        assert_eq!(out.outcome, Outcome::Unresolved);
        assert!(out.frontiers.iter().any(|f| f.reason.contains("cycle")));
        let words = [i(4, 4, 0, 3), 0, j(2, BASE + 24), 0, j(2, BASE + 24), 0, i(15, 0, 2, 0)];
        let out = run(&words, 2, &void_policy());
        assert_eq!(out.outcome, Outcome::Dead);
        assert!(out.frontiers.is_empty());
    }
    #[test]
    fn architectural_link_kill_does_not_assume_other_call_clobbers() {
        assert_eq!(run(&[j(3, 0x8002_0000)], 31, &void_policy()).outcome, Outcome::Dead);
        for reg in [2, 3, 4, 5, 6, 7, 8, 24] {
            assert_eq!(
                run(&[j(3, 0x8002_0000), 0], reg, &void_policy()).outcome,
                Outcome::Unresolved
            );
        }
        assert_eq!(run(&[r(9, 2, 0, 2), 0], 2, &void_policy()).outcome, Outcome::Consumed);
    }
    #[test]
    fn invalid_inputs_and_budgets_do_not_prove_dead() {
        let p = AuditPolicy {
            max_states: 1,
            ..void_policy()
        };
        assert_eq!(run(&[0, i(15, 0, 2, 0)], 2, &p).outcome, Outcome::Unresolved);
        assert!(
            audit(
                ExactExtent {
                    start: BASE,
                    words: &[0]
                },
                BASE,
                0,
                &p
            )
            .is_err()
        );
        assert!(
            audit(
                ExactExtent {
                    start: BASE + 1,
                    words: &[0]
                },
                BASE + 1,
                2,
                &p
            )
            .is_err()
        );
        assert!(
            audit(
                ExactExtent {
                    start: BASE,
                    words: &[]
                },
                BASE,
                2,
                &p
            )
            .is_err()
        );
    }

    #[test]
    fn binary_wrapper_uses_exact_loaded_extent_and_checks_bounds() {
        let words = [i(15, 0, 2, 1), r(8, 31, 0, 0), 0, i(43, 29, 2, 0)];
        let bytes = words.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<_>>();
        let binary = crate::Binary::parse_psx_overlay(bytes, BASE as u64, None).unwrap();
        let out = binary
            .audit_ps1_register(BASE as u64, 16, BASE as u64, 2, &void_policy())
            .unwrap();
        assert_eq!(out.extent_words, 4);
        assert_eq!(out.outcome, Outcome::Dead);
        let out = binary
            .audit_ps1_register(BASE as u64, 16, (BASE + 12) as u64, 2, &void_policy())
            .unwrap();
        assert_eq!(out.outcome, Outcome::Consumed);
        for (address, size, entry) in [
            (BASE as u64, 20, BASE as u64),
            (BASE as u64 + 1, 12, BASE as u64 + 1),
            (BASE as u64, 15, BASE as u64),
            (BASE as u64, 16, BASE as u64 + 16),
            (0x1_8001_0000, 16, 0x1_8001_0000),
            (u64::MAX - 3, 8, u64::MAX - 3),
        ] {
            assert!(
                binary
                    .audit_ps1_register(address, size, entry, 2, &void_policy())
                    .is_err()
            );
        }
        let raw = crate::Binary::raw(vec![0u8; 16]);
        assert!(raw.audit_ps1_register(0, 16, 0, 2, &void_policy()).is_err());
    }

    /// Optional user-assets validation; no original arrays live in this source.
    /// Set BINVIZ_MIPSAUDIT_PSX_EXE then run this ignored test explicitly.
    #[test]
    #[ignore = "requires the user's original executable"]
    fn local_ff9_native_tail_contracts() {
        let path = std::env::var("BINVIZ_MIPSAUDIT_PSX_EXE").expect("set original EXE path");
        let exe = std::fs::read(path).unwrap();
        assert_eq!(&exe[..8], b"PS-X EXE");
        let binary = crate::Binary::parse(exe.clone()).unwrap();
        let base = u32::from_le_bytes(exe[0x18..0x1c].try_into().unwrap());
        let mut reports = vec![];
        for (address, size, register, expected) in [
            (0x80031718u32, 1108usize, 7u8, Outcome::Dead),
            (0x800548e8, 312, 6, Outcome::Dead),
            (0x800548e8, 312, 7, Outcome::Dead),
            (0x800548e8, 312, 4, Outcome::Consumed),
            (0x800548e8, 312, 5, Outcome::Consumed),
        ] {
            let offset = 0x800 + (address - base) as usize;
            let words = exe[offset..offset + size]
                .chunks_exact(4)
                .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
                .collect::<Vec<_>>();
            let report = audit(
                ExactExtent {
                    start: address,
                    words: &words,
                },
                address,
                register,
                &void_policy(),
            )
            .unwrap();
            assert_eq!(report.outcome, expected);
            let loaded = binary
                .audit_ps1_register(address as u64, size, address as u64, register, &void_policy())
                .unwrap();
            assert_eq!(
                serde_json::to_value(&report).unwrap(),
                serde_json::to_value(&loaded).unwrap()
            );
            if address == 0x80031718 {
                assert_eq!(report.endpoints[0].path.last().unwrap().pc, 0x80031738);
            }
            reports.push(report);
        }
        if let Ok(path) = std::env::var("BINVIZ_MIPSAUDIT_REPORT") {
            std::fs::write(path, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
        }
    }
}
