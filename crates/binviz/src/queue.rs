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

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::binary::Binary;
use crate::model::{Annotation, Decomp, DecompState};
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
    /// Tried without matching (a percent recorded, not matched or nonmatching).
    pub attempted: u32,
    /// Partial credit: the bytes of code the recorded percents add up to
    /// (each function's size times its best percent), the matched ones at
    /// 100; what decomp.dev's fuzzy percent is of.
    pub credited_bytes: f64,
}

/// What built the C that matched: kept with a function's note so the match
/// can be reused in another project of the same build.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildInfo {
    pub compiler: String,
    pub flags: String,
    pub sdk: String,
}

impl BuildInfo {
    /// `compiler=gcc 2.7.2.3,flags=-O2 -G0,sdk=Psy-Q 4.3`: keys and values,
    /// comma-separated (a value with a comma goes in quotes).
    pub fn parse(text: &str) -> Result<BuildInfo, String> {
        let mut b = BuildInfo::default();
        for part in split_meta(text) {
            let (k, v) = part
                .split_once('=')
                .ok_or_else(|| format!("not key=value: {part} (compiler=…, flags=…, sdk=…)"))?;
            let v = v.trim().trim_matches('"').to_string();
            match k.trim() {
                "compiler" => b.compiler = v,
                "flags" => b.flags = v,
                "sdk" => b.sdk = v,
                other => return Err(format!("unknown key {other} (compiler, flags, sdk)")),
            }
        }
        Ok(b)
    }

    /// Sets what was given where nothing is recorded yet.
    pub fn apply_if_missing(&self, d: &mut Decomp) {
        for (mine, theirs) in [
            (&self.compiler, &mut d.compiler),
            (&self.flags, &mut d.flags),
            (&self.sdk, &mut d.sdk),
        ] {
            if theirs.is_empty() {
                theirs.clone_from(mine);
            }
        }
    }
}

fn split_meta(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out.into_iter().filter(|p| !p.trim().is_empty()).collect()
}

/// Records a batch's verdicts in `notes` as objdiff's report would be:
/// each function at 100% becomes matched (its unit the source file, the
/// build kept where none is), one that matched before and no longer does
/// goes back to todo, and the others keep their best percent. Returns how
/// many are newly matched, and the addresses that stopped matching.
/// `now`: seconds since 1970, for when a state changed.
pub fn record_scores(
    notes: &mut Vec<Annotation>,
    functions: &[crate::matching::FunctionProgress],
    build: &BuildInfo,
    now: u64,
) -> (u32, Vec<u64>) {
    let (mut matched, mut lost) = (0, Vec::new());
    for f in functions {
        let i = notes
            .iter()
            .position(|a| a.address == f.address && a.decomp.is_some())
            .or_else(|| notes.iter().position(|a| a.address == f.address && a.size == 0))
            .unwrap_or_else(|| {
                notes.push(Annotation {
                    address: f.address,
                    ..Default::default()
                });
                notes.len() - 1
            });
        let d = notes[i].decomp.get_or_insert_with(Decomp::default);
        let before = d.clone();
        if f.percent >= 100.0 {
            if d.state != DecompState::Matched {
                matched += 1;
            }
            d.state = DecompState::Matched;
            d.by.clear();
            d.percent = Some(100.0);
            build.apply_if_missing(d);
        } else if d.state == DecompState::Matched {
            lost.push(f.address);
            d.state = DecompState::Todo;
            d.percent = Some(f.percent);
        } else {
            d.percent = Some(d.percent.map_or(f.percent, |p| p.max(f.percent)));
        }
        if d.source.is_empty() {
            d.source = f.unit.clone();
        }
        if *d != before {
            d.since = now;
        }
    }
    (matched, lost)
}

impl DecompProgress {
    /// One line: how much is done, with partial credit.
    pub fn summary(&self) -> String {
        let pct = |n: f64| {
            if self.bytes > 0 {
                n * 100.0 / self.bytes as f64
            } else {
                0.0
            }
        };
        format!(
            "{} of {} functions matched ({:.1}% of the code), {} nonmatching, {} library, {} attempted, {} in progress, {} skipped; with partial credit {:.1}% of the code",
            self.matched,
            self.functions,
            pct(self.matched_bytes as f64),
            self.nonmatching,
            self.library,
            self.attempted,
            self.in_progress,
            self.skipped,
            pct(self.credited_bytes)
        )
    }
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
        self.progress_over(&self.functions_as_noted())
    }

    /// [`Binary::decomp_progress`] by area: each loaded part of the image
    /// with code in it (a boot executable's program, an overlay, a memory
    /// image's RAM) and where its functions stand, in address order.
    pub fn decomp_progress_by_area(&self) -> Vec<(String, DecompProgress)> {
        let mut areas: Vec<(String, Vec<(u64, u64)>)> = Vec::new();
        for f in self.functions_as_noted() {
            let name = self
                .section_at(f.0)
                .map_or_else(|| "(outside any section)".to_string(), |s| s.name.clone());
            match areas.last_mut() {
                Some(a) if a.0 == name => a.1.push(f),
                _ => areas.push((name, vec![f])),
            }
        }
        areas
            .into_iter()
            .map(|(name, fs)| (name, self.progress_over(&fs)))
            .collect()
    }

    fn progress_over(&self, functions: &[(u64, u64)]) -> DecompProgress {
        let mut p = DecompProgress::default();
        // The functions as the notes size them: a range merged by a note counts once.
        for &(address, bytes) in functions {
            p.functions += 1;
            p.bytes += bytes;
            let d = self.decomp_at(address);
            let state = d.map_or(DecompState::Todo, |d| d.state);
            match state {
                DecompState::Matched => {
                    p.matched += 1;
                    p.matched_bytes += bytes;
                    p.credited_bytes += bytes as f64;
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
            if state != DecompState::Matched
                && let Some(percent) = d.and_then(|d| d.percent)
            {
                if d.is_some_and(|d| d.attempts > 0) && matches!(state, DecompState::Todo | DecompState::InProgress) {
                    p.attempted += 1;
                }
                p.credited_bytes += bytes as f64 * f64::from(percent.clamp(0.0, 99.99)) / 100.0;
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

    /// The functions of `sibling`, another game's decompilation built with
    /// the same compiler, matched or nonmatching in its notes, whose
    /// instructions are most like those of the function at `address` here
    /// (half alike or more, and four instructions or more), most alike
    /// first: worked examples before this project has its own. None for code
    /// of another instruction set.
    pub fn sibling_examples(&self, sibling: &Binary, address: u64, count: usize) -> Vec<Similar> {
        if self.summary().arch != sibling.summary().arch {
            return Vec::new();
        }
        let theirs = sibling.similar_index();
        let written = |a: u64| matches!(sibling.decomp_state(a), DecompState::Matched | DecompState::Nonmatching);
        // Another project's `jmp` or `ret` alone says nothing about this one.
        let mut like =
            self.similar_index()
                .like_across(theirs, &theirs.file(&written), address, (count * 2).max(16), 0.5);
        like.retain(|s| s.instructions >= 4);
        // Look-alikes as alike as each other (a game's `Cmd_God_f` and `Cmd_Notarget_f`): the one
        // using the same strings is this one's counterpart.
        let strings = |bin: &Binary, a: u64| -> HashSet<String> {
            if bin.xrefs_supported() {
                bin.prepare_xrefs();
            }
            bin.function_summary(a, 64)
                .map(|f| f.strings.into_iter().map(|s| s.text).collect())
                .unwrap_or_default()
        };
        let ours = strings(self, address);
        if !ours.is_empty() {
            let mut keyed: Vec<(usize, Similar)> = like
                .into_iter()
                .map(|s| (strings(sibling, s.address).intersection(&ours).count(), s))
                .collect();
            keyed.sort_by(|a, b| {
                let alike = |s: &Similar| (s.similarity * 100.0).round() as i32;
                alike(&b.1).cmp(&alike(&a.1)).then(b.0.cmp(&a.0))
            });
            like = keyed.into_iter().map(|(_, s)| s).collect();
        }
        like.truncate(count);
        like
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
