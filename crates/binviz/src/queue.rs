//! Working through a matching decompilation: where it stands, and which
//! function to take next.
//!
//! Writing a function's C means calling what it calls with their exact
//! prototypes, so a function is *ready* once everything it calls is done:
//! matched, nonmatching (its C exists) or library code; imports always are.
//! Functions are listed best first:
//! 1. those shaped like a function already done (90 % of the same
//!    instructions or more, and eight instructions or more: tiny functions
//!    all look alike), whose C is then a template;
//! 2. ready functions, cheapest for what they unlock first:
//!    `(1 + 2 × callers left waiting only on it + log2(1 + callers)) /
//!    (1 + instructions / 32)`, so that small functions many others wait on
//!    come first and big ones last;
//! 3. functions still waiting on some of theirs, fewest missing first;
//! 4. functions tried three times without matching, until something they
//!    call is done after the last try.
//!
//! What is done, set aside, or claimed by someone (within `claim_ttl`) isn't listed.

use std::collections::HashMap;

use serde::Serialize;

use crate::binary::Binary;
use crate::model::{Decomp, DecompState};
use crate::similar::Similar;

/// Functions at least this alike, and this long, are a template for each other.
pub const TEMPLATE: f32 = 0.9;
pub const TEMPLATE_INSNS: u32 = 8;
/// Tries after which a function waits for something new before coming back.
const TRIES: u32 = 3;

/// What to list.
#[derive(Debug, Clone)]
pub struct NextQuery {
    /// At most this many.
    pub limit: usize,
    /// Only functions starting in `lo..hi`.
    pub within: Option<(u64, u64)>,
    /// List what others have claimed, and what was set aside, too.
    pub include_claimed: bool,
    pub include_skipped: bool,
    /// The time now, in seconds since 1970, and how long a claim holds.
    pub now: u64,
    pub claim_ttl: u64,
}

impl Default for NextQuery {
    fn default() -> NextQuery {
        NextQuery {
            limit: 10,
            within: None,
            include_claimed: false,
            include_skipped: false,
            now: 0,
            claim_ttl: 3600,
        }
    }
}

/// Why a function is where it is in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Readiness {
    /// Shaped like a function already done.
    LikeDone,
    /// Everything it calls is done.
    Ready,
    /// Some of what it calls isn't done yet.
    Waiting,
    /// Tried without matching, with nothing new to go on since.
    Hard,
}

/// A function to decompile, and what the list made of it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextFunction {
    pub address: u64,
    /// Bytes, up to the alignment filler.
    pub size: u64,
    pub instructions: u32,
    pub readiness: Readiness,
    /// Distinct functions calling it, and that it calls.
    pub callers: u32,
    pub callees: u32,
    /// What it calls that isn't done yet.
    pub waiting_on: Vec<u64>,
    /// Callers that wait only on it.
    pub unblocks: u32,
    /// The function already done that is most like it (60 % alike or more).
    pub like: Option<Similar>,
    pub score: f32,
    pub state: DecompState,
    pub attempts: u32,
    /// Its best match so far.
    pub percent: Option<f32>,
}

/// How far a decompilation has come, over the binary's own functions.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecompProgress {
    pub functions: u32,
    /// Bytes of code in them.
    pub bytes: u64,
    pub matched: u32,
    pub matched_bytes: u64,
    pub nonmatching: u32,
    pub nonmatching_bytes: u64,
    pub library: u32,
    pub library_bytes: u64,
    pub in_progress: u32,
    pub skipped: u32,
}

/// Functions to decompile, best first.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextList {
    pub progress: DecompProgress,
    /// How many of the functions not done (in `within`) are in each group.
    pub like_done: u32,
    pub ready: u32,
    pub waiting: u32,
    pub hard: u32,
    /// Claimed by someone else, so not listed.
    pub claimed: u32,
    pub functions: Vec<NextFunction>,
}

impl Binary {
    /// Where decompiling the function starting at `address` stands, if anyone said.
    pub fn decomp_at(&self, address: u64) -> Option<&Decomp> {
        let first = self.annotations.partition_point(|a| a.address < address);
        self.annotations[first..]
            .iter()
            .take_while(|a| a.address == address)
            .find_map(|a| a.decomp.as_ref())
    }

    fn decomp_state(&self, address: u64) -> DecompState {
        self.decomp_at(address).map_or(DecompState::Todo, |d| d.state)
    }

    /// How far the decompilation has come.
    pub fn decomp_progress(&self) -> DecompProgress {
        let mut p = DecompProgress::default();
        for (address, bytes, _) in self.similar_index().functions() {
            p.functions += 1;
            p.bytes += bytes;
            match self.decomp_state(address) {
                DecompState::Matched => {
                    p.matched += 1;
                    p.matched_bytes += bytes;
                }
                DecompState::Nonmatching => {
                    p.nonmatching += 1;
                    p.nonmatching_bytes += bytes;
                }
                DecompState::Library => {
                    p.library += 1;
                    p.library_bytes += bytes;
                }
                DecompState::InProgress => p.in_progress += 1,
                DecompState::Skipped => p.skipped += 1,
                DecompState::Todo => {}
            }
        }
        p
    }

    /// The calls between the binary's own functions (tail calls included):
    /// each function's callees and callers, sorted, itself left out.
    fn function_calls(&self) -> (HashMap<u64, Vec<u64>>, HashMap<u64, Vec<u64>>) {
        let index = self.similar_index();
        let mut callees: HashMap<u64, Vec<u64>> = HashMap::new();
        let mut callers: HashMap<u64, Vec<u64>> = HashMap::new();
        for (site, target) in self.calls() {
            let Some((caller, _)) = self.symbols.static_function_containing(site) else {
                continue;
            };
            // Imports, and code no function starts at, are nothing to wait for.
            if caller == target || !index.contains(caller) || !index.contains(target) {
                continue;
            }
            callees.entry(caller).or_default().push(target);
            callers.entry(target).or_default().push(caller);
        }
        for list in callees.values_mut().chain(callers.values_mut()) {
            list.sort_unstable();
            list.dedup();
        }
        (callees, callers)
    }

    /// The functions matched or nonmatching whose instructions are most like
    /// those of the function at `address` (half alike or more), most alike
    /// first: their C is the worked example for writing its.
    pub fn worked_examples(&self, address: u64, count: usize) -> Vec<Similar> {
        let index = self.similar_index();
        let written = |a: u64| matches!(self.decomp_state(a), DecompState::Matched | DecompState::Nonmatching);
        index.like(&index.file(&written), address, count, 0.5)
    }

    /// The functions calling the one at `address` that aren't done and now
    /// have everything they call done.
    pub fn callers_now_ready(&self, address: u64) -> Vec<u64> {
        let (callees, callers) = self.function_calls();
        let done = |a: u64| self.decomp_state(a).is_done();
        callers.get(&address).map_or_else(Vec::new, |list| {
            list.iter()
                .copied()
                .filter(|&c| !done(c) && callees.get(&c).is_none_or(|calls| calls.iter().all(|&d| done(d))))
                .collect()
        })
    }

    /// The functions to decompile next, best first (see [`crate::queue`]).
    pub fn next_functions(&self, q: &NextQuery) -> NextList {
        let index = self.similar_index();
        let (callees, callers) = self.function_calls();
        let done = |a: u64| self.decomp_state(a).is_done();
        let functions: Vec<(u64, u64, u32)> = index.functions().collect();
        // The functions done, filed to find which of them each one is like.
        let examples = index.file(&done);
        // What each function not done yet waits on, and whom it would free.
        let waiting: HashMap<u64, Vec<u64>> = functions
            .iter()
            .filter(|f| !done(f.0))
            .map(|f| {
                let calls = callees.get(&f.0).map_or(&[][..], Vec::as_slice);
                (f.0, calls.iter().copied().filter(|&c| !done(c)).collect())
            })
            .collect();
        let mut unblocks: HashMap<u64, u32> = HashMap::new();
        for w in waiting.values() {
            if let [only] = w.as_slice() {
                *unblocks.entry(*only).or_default() += 1;
            }
        }
        let mut list = NextList {
            progress: self.decomp_progress(),
            like_done: 0,
            ready: 0,
            waiting: 0,
            hard: 0,
            claimed: 0,
            functions: Vec::new(),
        };
        for &(address, size, instructions) in &functions {
            let d = self.decomp_at(address);
            let state = d.map_or(DecompState::Todo, |d| d.state);
            if state.is_done() || q.within.is_some_and(|(lo, hi)| address < lo || address >= hi) {
                continue;
            }
            let since = d.map_or(0, |d| d.since);
            if state == DecompState::InProgress && q.now < since.saturating_add(q.claim_ttl) {
                list.claimed += 1;
                if !q.include_claimed {
                    continue;
                }
            }
            if state == DecompState::Skipped && !q.include_skipped {
                continue;
            }
            let calls = callees.get(&address).map_or(&[][..], Vec::as_slice);
            let attempts = d.map_or(0, |d| d.attempts);
            let news = calls.iter().any(|&c| {
                self.decomp_at(c)
                    .is_some_and(|cd| cd.state.is_done() && cd.since > since)
            });
            let like = index.like(&examples, address, 1, 0.6).into_iter().next();
            let waiting_on = waiting.get(&address).cloned().unwrap_or_default();
            let readiness = if attempts >= TRIES && !news {
                Readiness::Hard
            } else if instructions >= TEMPLATE_INSNS && like.is_some_and(|l| l.similarity >= TEMPLATE) {
                Readiness::LikeDone
            } else if waiting_on.is_empty() {
                Readiness::Ready
            } else {
                Readiness::Waiting
            };
            let n_callers = callers.get(&address).map_or(0, Vec::len) as u32;
            let frees = unblocks.get(&address).copied().unwrap_or(0);
            let score =
                (1.0 + 2.0 * frees as f32 + (1.0 + n_callers as f32).log2()) / (1.0 + instructions as f32 / 32.0);
            match readiness {
                Readiness::LikeDone => list.like_done += 1,
                Readiness::Ready => list.ready += 1,
                Readiness::Waiting => list.waiting += 1,
                Readiness::Hard => list.hard += 1,
            }
            list.functions.push(NextFunction {
                address,
                size,
                instructions,
                readiness,
                callers: n_callers,
                callees: calls.len() as u32,
                waiting_on,
                unblocks: frees,
                like,
                score,
                state,
                attempts,
                percent: d.and_then(|d| d.percent),
            });
        }
        list.functions.sort_by(|a, b| {
            let within_group = match a.readiness {
                Readiness::LikeDone => {
                    let (sa, sb) = (
                        a.like.map_or(0.0, |l| l.similarity),
                        b.like.map_or(0.0, |l| l.similarity),
                    );
                    sb.total_cmp(&sa)
                }
                Readiness::Waiting => a.waiting_on.len().cmp(&b.waiting_on.len()),
                Readiness::Ready | Readiness::Hard => std::cmp::Ordering::Equal,
            };
            a.readiness
                .cmp(&b.readiness)
                .then(within_group)
                .then(b.score.total_cmp(&a.score))
                .then(a.address.cmp(&b.address))
        });
        list.functions.truncate(q.limit);
        list
    }
}
