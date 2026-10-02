//! Function-level diffs, the way BinDiff does them: which functions of one
//! build (or ROM revision) are which in another, and what changed in each.
//!
//! Functions are matched in passes, each over those still unmatched, the
//! surest first:
//! 1. by name (from the symbol table, debug info, Objective-C metadata or
//!    the user's notes; not the `sub_…` names binviz makes up);
//! 2. by identical bytes;
//! 3. by the same instructions (code that moved: only addresses differ);
//! 4. by the strings they use, and the distinctive numbers in their code,
//!    when no other function on either side has the same;
//! 5. through the call graph: the callees and callers of matched functions,
//!    candidates from where their calls stand among calls already paired
//!    (the call sequence), a lone unmatched neighbour on each side, and
//!    look-alikes among the neighbours; each scored by how alike the two
//!    are and whether their other matched neighbours agree, and paired best
//!    first only when clearly better than the runner-up on both sides (so
//!    variants of one template aren't guessed between). 4 and 5 repeat,
//!    each giving the other more to go on;
//! 6. for game ROMs, at the same address with a similar size.
//!
//! A matched pair is identical (same bytes), relocated (the same
//! instructions, addresses aside) or changed, with how similar their
//! instructions are; [`Binary::diff_function_code`] lines two functions'
//! instructions up.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use serde::Serialize;

use crate::binary::Binary;
use crate::model::{Format, Instruction, SymbolSource};

/// At most this many instructions of a function are compared.
/// Instructions read per function: enough for a 64 KB script interpreter.
const MAX_INSNS: usize = 20_000;

/// A function on one side.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FnInfo {
    pub address: u64,
    pub name: String,
    pub size: u64,
    /// Instructions, when they were decoded (functions matched as identical aren't).
    pub instructions: Option<u32>,
}

/// How a pair was matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchKind {
    Name,
    Bytes,
    Instructions,
    /// The same strings, used by no other function on either side.
    Strings,
    /// The same distinctive numbers in their code, in no other function.
    Constants,
    Calls,
    Address,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PairStatus {
    /// The same bytes.
    Identical,
    /// The same instructions; addresses in them differ.
    Relocated,
    Changed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pair {
    pub old: FnInfo,
    pub new: FnInfo,
    pub how: MatchKind,
    pub status: PairStatus,
    /// 1 for the same instructions, down to 0 for nothing in common.
    pub similarity: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDiff {
    /// Matched functions: the changed first, least similar first.
    pub pairs: Vec<Pair>,
    /// Only in the newer binary.
    pub added: Vec<FnInfo>,
    /// Only in the older one.
    pub removed: Vec<FnInfo>,
    pub identical: u32,
    pub relocated: u32,
    pub changed: u32,
}

/// One line of two functions' instructions lined up.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: LineKind,
    pub old: Option<Instruction>,
    pub new: Option<Instruction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineKind {
    /// The same instruction (its operands may name addresses that moved).
    Same,
    /// An instruction in place of another.
    Changed,
    Removed,
    Added,
}

struct Func {
    address: u64,
    size: u64,
    name: String,
    /// Named by the file, its debug info or the user.
    named: bool,
    /// An import stub or slot (named after what it imports, not a function of the binary).
    import: bool,
    /// Its bytes (padding at the end left out) and instructions.
    bytes: u64,
    tokens: Vec<u32>,
}

struct Side<'a> {
    bin: &'a Binary,
    funcs: Vec<Func>,
    at: HashMap<u64, usize>,
    matched: Vec<Option<usize>>,
    callees: HashMap<usize, Vec<usize>>,
    callers: HashMap<usize, Vec<usize>>,
}

fn hash_of(x: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    x.hash(&mut h);
    h.finish()
}

impl<'a> Side<'a> {
    fn new(bin: &'a Binary) -> Side<'a> {
        let mut funcs = Vec::new();
        let mut at = HashMap::new();
        for s in bin.symbols().functions() {
            if !s.defined || s.size == 0 || at.contains_key(&s.address) {
                continue;
            }
            let best = bin.symbols().at(s.address).unwrap_or(s);
            // A size inferred from the next symbol takes the padding after the function in too.
            let (tokens, len) = bin.instruction_tokens(s.address, s.address + s.size, MAX_INSNS);
            let bytes = bin
                .address_to_offset(s.address)
                .and_then(|o| bin.data().get(o as usize..(o + len.min(s.size)) as usize))
                .map_or(0, hash_of);
            at.insert(s.address, funcs.len());
            funcs.push(Func {
                address: s.address,
                size: s.size,
                name: best.display_name().into_owned(),
                named: best.source != SymbolSource::Discovered,
                import: best.source == SymbolSource::Import,
                bytes,
                tokens,
            });
        }
        let n = funcs.len();
        Side {
            bin,
            funcs,
            at,
            matched: vec![None; n],
            callees: HashMap::new(),
            callers: HashMap::new(),
        }
    }

    fn tokens(&mut self, i: usize) -> &[u32] {
        &self.funcs[i].tokens
    }

    fn info(&self, i: usize) -> FnInfo {
        let f = &self.funcs[i];
        FnInfo {
            address: f.address,
            name: f.name.clone(),
            size: f.size,
            instructions: Some(f.tokens.len() as u32),
        }
    }

    /// The functions a function calls, in the order of their first calls.
    fn callees_of(&mut self, i: usize) -> Vec<usize> {
        if let Some(c) = self.callees.get(&i) {
            return c.clone();
        }
        let mut edges = self.bin.callees(self.funcs[i].address);
        edges.sort_by_key(|e| e.site);
        let list: Vec<usize> = edges.iter().filter_map(|e| self.at.get(&e.address).copied()).collect();
        self.callees.insert(i, list.clone());
        list
    }

    /// The strings a function refers to, sorted, as a key; `None` for none.
    fn strings_key(&self, i: usize) -> Option<u64> {
        let f = &self.funcs[i];
        let mut texts: Vec<String> = self
            .bin
            .scan_function(f.address, f.size)
            .into_iter()
            .filter(|(_, _, kind)| !kind.is_call())
            .filter_map(|(_, target, _)| self.bin.string_at_address(target))
            .filter(|t| t.len() >= 3)
            .collect();
        texts.sort_unstable();
        texts.dedup();
        (!texts.is_empty()).then(|| hash_of(&texts))
    }

    /// The distinctive numbers in a function's instructions (hash
    /// multipliers, magic numbers, sizes: not small ones, masks, powers of
    /// two, or addresses), sorted, as a key; `None` for none.
    fn constants_key(&self, i: usize) -> Option<u64> {
        let f = &self.funcs[i];
        let d = self.bin.disassemble_function(f.address, MAX_INSNS);
        let mut values: Vec<u64> = Vec::new();
        for ins in &d.instructions {
            if ins.target.is_some() {
                continue;
            }
            // Numbers outside memory operands' brackets.
            let mut depth = 0;
            let text = &ins.operands;
            let bytes = text.as_bytes();
            let mut k = 0;
            while k < bytes.len() {
                match bytes[k] {
                    b'[' | b'(' => depth += 1,
                    b']' | b')' => depth -= 1,
                    b'0' if depth == 0 && bytes.get(k + 1) == Some(&b'x') => {
                        let end = text[k + 2..]
                            .find(|c: char| !c.is_ascii_hexdigit())
                            .map_or(text.len(), |e| k + 2 + e);
                        if let Ok(v) = u64::from_str_radix(&text[k + 2..end], 16) {
                            values.push(v);
                        }
                        k = end;
                        continue;
                    }
                    _ => {}
                }
                k += 1;
            }
        }
        let distinctive = |v: u64| {
            let v32 = v as u32 as u64;
            v >= 0x400
                && !v.is_power_of_two()
                && !v.wrapping_add(1).is_power_of_two()
                && v != u64::MAX
                && !(v32 + 1).is_power_of_two()
                && !(v32.wrapping_neg() & 0xFFFF_FFFF).is_power_of_two()
                && self.bin.section_at(v).is_none()
                && self.bin.section_at(v << 16).is_none()
        };
        values.retain(|&v| distinctive(v));
        values.sort_unstable();
        values.dedup();
        (!values.is_empty()).then(|| hash_of(&values))
    }

    /// The functions a function calls, a call site at a time in address
    /// order (a callee called twice is in it twice).
    fn call_sequence(&self, i: usize) -> Vec<usize> {
        let f = &self.funcs[i];
        let mut calls: Vec<(u64, usize)> = self
            .bin
            .scan_function(f.address, f.size)
            .into_iter()
            .filter(|(_, _, kind)| kind.is_call())
            .filter_map(|(site, target, _)| {
                let callee = self
                    .bin
                    .symbols()
                    .function_containing(target)
                    .map_or(target, |g| g.address);
                Some((site, *self.at.get(&callee)?))
            })
            .collect();
        calls.sort_unstable();
        calls.into_iter().map(|c| c.1).take(512).collect()
    }

    fn callers_of(&mut self, i: usize) -> Vec<usize> {
        if let Some(c) = self.callers.get(&i) {
            return c.clone();
        }
        let mut edges = self.bin.callers(self.funcs[i].address);
        edges.sort_by_key(|e| e.address);
        let list: Vec<usize> = edges.iter().filter_map(|e| self.at.get(&e.address).copied()).collect();
        self.callers.insert(i, list.clone());
        list
    }
}

struct Matcher<'a> {
    a: Side<'a>,
    b: Side<'a>,
    pairs: Vec<(usize, usize, MatchKind)>,
}

impl Matcher<'_> {
    fn pair(&mut self, i: usize, j: usize, how: MatchKind) {
        if self.a.matched[i].is_none() && self.b.matched[j].is_none() {
            self.a.matched[i] = Some(j);
            self.b.matched[j] = Some(i);
            self.pairs.push((i, j, how));
        }
    }

    /// Every function's key on each side (`None` for no key).
    fn all_keys(&mut self, key: &dyn Fn(&mut Side, usize) -> Option<u64>) -> (Vec<Option<u64>>, Vec<Option<u64>>) {
        let a = (0..self.a.funcs.len()).map(|i| key(&mut self.a, i)).collect();
        let b = (0..self.b.funcs.len()).map(|j| key(&mut self.b, j)).collect();
        (a, b)
    }

    /// Pairs the unmatched functions whose key no other function on either
    /// side has, matched or not (a key left unique only because its other
    /// holders were paired says little), when their sizes are within
    /// `min_size_ratio` and they share that much of their instructions.
    fn by_key_unique_overall(&mut self, how: MatchKind, keys: &(Vec<Option<u64>>, Vec<Option<u64>>), min_alike: f32) {
        let count = |ks: &[Option<u64>]| {
            let mut c: HashMap<u64, (u32, usize)> = HashMap::new();
            for (i, k) in ks.iter().enumerate() {
                if let Some(k) = k {
                    let e = c.entry(*k).or_insert((0, i));
                    e.0 += 1;
                }
            }
            c
        };
        let (ca, cb) = (count(&keys.0), count(&keys.1));
        let mut found: Vec<(usize, usize)> = ca
            .iter()
            .filter(|(_, v)| v.0 == 1)
            .filter_map(|(k, &(_, i))| cb.get(k).filter(|v| v.0 == 1).map(|&(_, j)| (i, j)))
            .collect();
        found.sort_unstable();
        for (i, j) in found {
            if self.a.matched[i].is_some() || self.b.matched[j].is_some() {
                continue;
            }
            if min_alike > 0.0 {
                let (ta, tb) = (&self.a.funcs[i].tokens, &self.b.funcs[j].tokens);
                let (la, lb) = (ta.len() as f32, tb.len() as f32);
                if la.min(lb) / la.max(lb).max(1.0) < min_alike || overlap(ta, tb) < min_alike {
                    continue;
                }
            }
            self.pair(i, j, how);
        }
    }

    /// Pairs the functions whose key is unique among the unmatched on both
    /// sides. Copies of the same code (the same bytes or instructions, as
    /// many on each side) are as good as one another: each pairs with the
    /// one at its address, the rest in address order.
    fn by_unique_key(&mut self, how: MatchKind, key: &mut dyn FnMut(&mut Side, usize) -> Option<u64>) {
        let mut keys_a: HashMap<u64, Vec<usize>> = HashMap::new();
        for i in 0..self.a.funcs.len() {
            if self.a.matched[i].is_none()
                && let Some(k) = key(&mut self.a, i)
            {
                keys_a.entry(k).or_default().push(i);
            }
        }
        let mut keys_b: HashMap<u64, Vec<usize>> = HashMap::new();
        for j in 0..self.b.funcs.len() {
            if self.b.matched[j].is_none()
                && let Some(k) = key(&mut self.b, j)
            {
                keys_b.entry(k).or_default().push(j);
            }
        }
        let mut found: Vec<(usize, usize)> = Vec::new();
        for (k, is) in &keys_a {
            let Some(js) = keys_b.get(k) else { continue };
            match (is.as_slice(), js.as_slice()) {
                ([i], [j]) => found.push((*i, *j)),
                (is, js) if how != MatchKind::Name && is.len() == js.len() && is.len() <= 32 => {
                    let mut rest_b = js.to_vec();
                    let mut rest_a = Vec::new();
                    for &i in is {
                        let address = self.a.funcs[i].address;
                        match rest_b.iter().position(|&j| self.b.funcs[j].address == address) {
                            Some(p) => found.push((i, rest_b.remove(p))),
                            None => rest_a.push(i),
                        }
                    }
                    found.extend(rest_a.into_iter().zip(rest_b));
                }
                _ => {}
            }
        }
        found.sort_unstable();
        for (i, j) in found {
            self.pair(i, j, how);
        }
    }

    /// How far `x`'s and `y`'s matched callers and callees correspond: of
    /// the matched neighbours of either, the share whose partner is a
    /// neighbour of the other in the same way. `None` when neither has any.
    fn agreement(&mut self, x: usize, y: usize) -> Option<f32> {
        let (mut total, mut agree) = (0u32, 0u32);
        for callers in [false, true] {
            let (na, nb) = if callers {
                (self.a.callers_of(x), self.b.callers_of(y))
            } else {
                (self.a.callees_of(x), self.b.callees_of(y))
            };
            let (sa, sb): (HashSet<usize>, HashSet<usize>) =
                (na.iter().copied().collect(), nb.iter().copied().collect());
            for &p in &sa {
                if let Some(q) = self.a.matched[p] {
                    total += 1;
                    agree += sb.contains(&q) as u32;
                }
            }
            for &q in &sb {
                if let Some(p) = self.b.matched[q] {
                    total += 1;
                    agree += sa.contains(&p) as u32;
                }
            }
        }
        (total > 0).then(|| agree as f32 / total as f32)
    }

    /// Candidates from where calls stand in two matched functions: their
    /// calls to functions already paired line up (a longest common
    /// subsequence), and between two such, a single unmatched callee on each
    /// side is likely the same function (BinDiff's call sequence).
    fn call_order_candidates(&self, i: usize, j: usize, out: &mut Vec<(usize, usize, Evidence)>) {
        let sa = self.a.call_sequence(i);
        let sb = self.b.call_sequence(j);
        if sa.is_empty() || sb.is_empty() {
            return;
        }
        // Positions of calls to matched functions, as the partner on b's side.
        let ma: Vec<(usize, usize)> = sa
            .iter()
            .enumerate()
            .filter_map(|(p, &f)| Some((p, self.a.matched[f]?)))
            .collect();
        let mb: Vec<(usize, usize)> = sb
            .iter()
            .enumerate()
            .filter(|(_, f)| self.b.matched[**f].is_some())
            .map(|(p, &f)| (p, f))
            .collect();
        let (n, m) = (ma.len(), mb.len());
        let mut t = vec![0u16; (n + 1) * (m + 1)];
        for x in (0..n).rev() {
            for y in (0..m).rev() {
                t[x * (m + 1) + y] = if ma[x].1 == mb[y].1 {
                    t[(x + 1) * (m + 1) + y + 1] + 1
                } else {
                    t[(x + 1) * (m + 1) + y].max(t[x * (m + 1) + y + 1])
                };
            }
        }
        let mut bounds: Vec<(Option<usize>, Option<usize>)> = vec![(None, None)];
        let (mut x, mut y) = (0, 0);
        while x < n && y < m {
            if ma[x].1 == mb[y].1 {
                bounds.push((Some(ma[x].0), Some(mb[y].0)));
                x += 1;
                y += 1;
            } else if t[(x + 1) * (m + 1) + y] >= t[x * (m + 1) + y + 1] {
                x += 1;
            } else {
                y += 1;
            }
        }
        bounds.push((None, None));
        for w in bounds.windows(2) {
            let (a_lo, b_lo) = (w[0].0.map_or(0, |p| p + 1), w[0].1.map_or(0, |q| q + 1));
            let (a_hi, b_hi) = (w[1].0.unwrap_or(sa.len()), w[1].1.unwrap_or(sb.len()));
            if a_lo >= a_hi || b_lo >= b_hi {
                continue;
            }
            let ua = dedup(sa[a_lo..a_hi].iter().copied().filter(|&f| self.a.matched[f].is_none()));
            let ub = dedup(sb[b_lo..b_hi].iter().copied().filter(|&f| self.b.matched[f].is_none()));
            if let ([x], [y]) = (ua.as_slice(), ub.as_slice()) {
                out.push((*x, *y, Evidence::Order));
            }
        }
    }

    /// Pairs the unmatched neighbours of matched functions: candidates from
    /// the order of calls, a lone unmatched caller or callee on each side,
    /// and look-alikes among them; each scored by how alike the two are, how
    /// strong the evidence, and whether their other matched neighbours
    /// agree; then taken best first, each only as the other's best.
    fn through_calls(&mut self) {
        // How alike two functions are, kept between rounds.
        let mut alike_cache: HashMap<(usize, usize), f32> = HashMap::new();
        for _round in 0..16 {
            let mut found: Vec<(usize, usize, Evidence)> = Vec::new();
            for k in 0..self.pairs.len() {
                let (i, j, _) = self.pairs[k];
                self.call_order_candidates(i, j, &mut found);
                for callers in [false, true] {
                    let (na, nb) = if callers {
                        (self.a.callers_of(i), self.b.callers_of(j))
                    } else {
                        (self.a.callees_of(i), self.b.callees_of(j))
                    };
                    let na: Vec<usize> = dedup(na.into_iter().filter(|&x| self.a.matched[x].is_none()));
                    let nb: Vec<usize> = dedup(nb.into_iter().filter(|&y| self.b.matched[y].is_none()));
                    if let ([x], [y]) = (na.as_slice(), nb.as_slice()) {
                        found.push((*x, *y, Evidence::Lone));
                        continue;
                    }
                    if na.len() * nb.len() > 4096 {
                        continue;
                    }
                    for &x in &na {
                        for &y in &nb {
                            found.push((x, y, Evidence::Near));
                        }
                    }
                }
            }
            // Each candidate once, with its strongest evidence.
            found.sort_by_key(|&(x, y, e)| (x, y, e));
            found.dedup_by_key(|c| (c.0, c.1));
            let mut scored: Vec<(f32, usize, usize)> = Vec::new();
            for (x, y, e) in found {
                let (lx, ly) = (self.a.funcs[x].tokens.len() as f32, self.b.funcs[y].tokens.len() as f32);
                let ratio = if lx.max(ly) == 0.0 {
                    1.0
                } else {
                    lx.min(ly) / lx.max(ly)
                };
                // Look-alikes need to look alike; a lone neighbour or a call in
                // the same place, only to be about the same size.
                let (min_ratio, bonus, floor) = match e {
                    Evidence::Order => (0.25, 0.3, 0.0),
                    Evidence::Lone => (0.4, 0.15, 0.0),
                    Evidence::Near => (0.6, 0.0, 0.5),
                };
                if ratio < min_ratio {
                    continue;
                }
                // What the two share, whatever the order: another compiler or
                // other flags reorder more than they change.
                let alike = *alike_cache
                    .entry((x, y))
                    .or_insert_with(|| overlap(&self.a.funcs[x].tokens, &self.b.funcs[y].tokens));
                if alike < floor {
                    continue;
                }
                let agree = self.agreement(x, y);
                if agree.is_some_and(|a| a < 0.5) {
                    continue;
                }
                scored.push((alike + bonus + 0.3 * agree.unwrap_or(0.5), x, y));
            }
            // Best first, each only as the other's best candidate, and clearly
            // better than the next: variants of one template called from the
            // same place look alike, and guessing among them spreads mistakes.
            let mut top_a: HashMap<usize, (f32, f32)> = HashMap::new();
            let mut top_b: HashMap<usize, (f32, f32)> = HashMap::new();
            for &(sc, x, y) in &scored {
                for (top, k) in [(&mut top_a, x), (&mut top_b, y)] {
                    let t = top.entry(k).or_insert((f32::MIN, f32::MIN));
                    if sc > t.0 {
                        *t = (sc, t.0);
                    } else if sc > t.1 {
                        t.1 = sc;
                    }
                }
            }
            scored.sort_by(|p, q| q.0.total_cmp(&p.0).then((p.1, p.2).cmp(&(q.1, q.2))));
            let before = self.pairs.len();
            for (sc, x, y) in scored {
                let (ta, tb) = (top_a[&x], top_b[&y]);
                let clear = |t: (f32, f32)| sc >= t.0 && sc - t.1 >= MARGIN;
                if clear(ta) && clear(tb) && self.a.matched[x].is_none() && self.b.matched[y].is_none() {
                    self.pair(x, y, MatchKind::Calls);
                }
            }
            if self.pairs.len() == before {
                break;
            }
        }
    }
}

/// How much better than the runner-up a candidate through the call graph must score.
const MARGIN: f32 = 0.05;

/// Why two functions are candidates through the call graph, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Evidence {
    /// Called from the same place in a matched pair.
    Order,
    /// The only unmatched caller or callee of a matched pair, on each side.
    Lone,
    /// Among a matched pair's unmatched neighbours, and alike.
    Near,
}

fn dedup(it: impl Iterator<Item = usize>) -> Vec<usize> {
    let mut seen = HashSet::new();
    it.filter(|x| seen.insert(*x)).collect()
}

/// How much two token sequences have in common, as multisets (0 to 1).
fn overlap(a: &[u32], b: &[u32]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let mut counts: HashMap<u32, i32> = HashMap::new();
    for t in a {
        *counts.entry(*t).or_default() += 1;
    }
    let mut common = 0;
    for t in b {
        if let Some(c) = counts.get_mut(t)
            && *c > 0
        {
            *c -= 1;
            common += 1;
        }
    }
    2.0 * common as f32 / (a.len() + b.len()) as f32
}

/// How similar two token sequences are, in order: 1 − edits / length.
pub(crate) fn similarity(a: &[u32], b: &[u32]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let max = (a.len() + b.len()).clamp(64, 3000);
    match distance(a, b, max) {
        Some(d) => 1.0 - d as f32 / (a.len() + b.len()) as f32,
        // Too different to line up quickly: what they share at all, discounted.
        None => overlap(a, b) * 0.5,
    }
}

/// The number of insertions and deletions turning `a` into `b` (Myers), or None past `max`.
fn distance(a: &[u32], b: &[u32], max: usize) -> Option<usize> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let off = max as isize + 1;
    let mut v = vec![0isize; 2 * off as usize + 1];
    for d in 0..=max as isize {
        let mut k = -d;
        while k <= d {
            let i = (k + off) as usize;
            let mut x = if k == -d || (k != d && v[i - 1] < v[i + 1]) {
                v[i + 1]
            } else {
                v[i - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[i] = x;
            if x >= n && y >= m {
                return Some(d as usize);
            }
            k += 2;
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edit {
    Keep,
    Delete,
    Insert,
}

/// `a` and `b` lined up: the shortest edit script (Myers) when it takes at
/// most `max` edits; past that, the two are anchored on the tokens each
/// has exactly once, in the order both have them (patience diff), and the
/// stretches between anchors are lined up the same way in turn. Only a
/// stretch too different to line up at all is given as all of one side then
/// all of the other, so a huge function (a script interpreter's dispatcher)
/// still scores by what it shares with its rebuild.
pub(crate) fn line_up(a: &[u32], b: &[u32], max: usize) -> Vec<Edit> {
    if a.len() + b.len() <= SMALL
        && let Some(s) = edit_script(a, b, max)
    {
        return s;
    }
    anchored(a, b, max, 0)
}

/// Sequences up to this long are diffed whole first.
const SMALL: usize = 3000;
/// Myers is given at most this many edits per stretch between anchors.
const STRETCH_EDITS: usize = 1500;

fn positional(a: &[u32], b: &[u32]) -> Vec<Edit> {
    let mut s = vec![Edit::Delete; a.len()];
    s.extend(std::iter::repeat_n(Edit::Insert, b.len()));
    s
}

fn anchored(a: &[u32], b: &[u32], max: usize, depth: u32) -> Vec<Edit> {
    if a.is_empty() || b.is_empty() {
        return positional(a, b);
    }
    if (depth > 0 || a.len() + b.len() <= SMALL)
        && let Some(s) = edit_script(a, b, max.min(STRETCH_EDITS))
    {
        return s;
    }
    let anchors = unique_anchors(a, b);
    if anchors.is_empty() {
        return if depth > 0 {
            positional(a, b)
        } else {
            edit_script(a, b, max.min(STRETCH_EDITS)).unwrap_or_else(|| positional(a, b))
        };
    }
    let mut out = Vec::with_capacity(a.len().max(b.len()));
    let (mut i, mut j) = (0, 0);
    for (ai, bj) in anchors {
        out.extend(anchored(&a[i..ai], &b[j..bj], max, depth + 1));
        out.push(Edit::Keep);
        i = ai + 1;
        j = bj + 1;
    }
    out.extend(anchored(&a[i..], &b[j..], max, depth + 1));
    out
}

/// The tokens `a` and `b` each hold exactly once, paired, in the longest
/// run that keeps the same order on both sides: (index in a, index in b).
fn unique_anchors(a: &[u32], b: &[u32]) -> Vec<(usize, usize)> {
    fn once(seq: &[u32]) -> HashMap<u32, usize> {
        let mut count: HashMap<u32, (usize, usize)> = HashMap::new();
        for (i, &t) in seq.iter().enumerate() {
            let e = count.entry(t).or_insert((0, i));
            e.0 += 1;
        }
        count
            .into_iter()
            .filter(|(_, (n, _))| *n == 1)
            .map(|(t, (_, i))| (t, i))
            .collect()
    }
    let in_b = once(b);
    let pairs: Vec<(usize, usize)> = once(a)
        .into_iter()
        .filter_map(|(t, i)| in_b.get(&t).map(|&j| (i, j)))
        .collect::<std::collections::BTreeMap<usize, usize>>()
        .into_iter()
        .collect();
    // The longest increasing subsequence of the b indices.
    let mut tails: Vec<usize> = Vec::new(); // index into pairs of the best tail per length
    let mut prev: Vec<Option<usize>> = vec![None; pairs.len()];
    for (k, &(_, j)) in pairs.iter().enumerate() {
        let pos = tails.partition_point(|&t| pairs[t].1 < j);
        if pos > 0 {
            prev[k] = Some(tails[pos - 1]);
        }
        if pos == tails.len() {
            tails.push(k);
        } else {
            tails[pos] = k;
        }
    }
    let mut out = Vec::new();
    let mut at = tails.last().copied();
    while let Some(k) = at {
        out.push(pairs[k]);
        at = prev[k];
    }
    out.reverse();
    out
}

/// The shortest edit script turning `a` into `b` (Myers), or None past `max` edits.
pub(crate) fn edit_script(a: &[u32], b: &[u32], max: usize) -> Option<Vec<Edit>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let off = max as isize + 1;
    let idx = |k: isize| (k + off) as usize;
    let mut v = vec![0isize; 2 * off as usize + 1];
    // The furthest points before each round, to walk back through.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut found = None;
    'rounds: for d in 0..=max as isize {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let mut x = if k == -d || (k != d && v[idx(k - 1)] < v[idx(k + 1)]) {
                v[idx(k + 1)]
            } else {
                v[idx(k - 1)] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[idx(k)] = x;
            if x >= n && y >= m {
                found = Some(d);
                break 'rounds;
            }
            k += 2;
        }
    }
    let last = found?;
    let mut script = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (0..=last).rev() {
        let v = &trace[d as usize];
        let k = x - y;
        let prev_k = if k == -d || (k != d && v[idx(k - 1)] < v[idx(k + 1)]) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = v[idx(prev_k)];
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            script.push(Edit::Keep);
            x -= 1;
            y -= 1;
        }
        if d > 0 {
            script.push(if x == prev_x { Edit::Insert } else { Edit::Delete });
        }
        x = prev_x;
        y = prev_y;
    }
    script.reverse();
    Some(script)
}

fn line_token(i: &Instruction) -> u32 {
    hash_of((&i.mnemonic, crate::disasm::operand_shape(&i.operands))) as u32
}

impl Binary {
    /// Which of this binary's functions are which in `newer` (another build,
    /// a ROM's revision or patched copy), and what changed in each.
    pub fn diff_functions(&self, newer: &Binary) -> FunctionDiff {
        let rom = self.summary().format == Format::Rom && newer.summary().format == Format::Rom;
        let mut m = Matcher {
            a: Side::new(self),
            b: Side::new(newer),
            pairs: Vec::new(),
        };
        // 1. Names.
        m.by_unique_key(MatchKind::Name, &mut |s, i| {
            let f = &s.funcs[i];
            // An import's stub shares its name with the import, not with a function of that name.
            f.named.then(|| hash_of((&f.name, f.import)))
        });
        // 2. Identical bytes (a few bytes say little).
        m.by_unique_key(MatchKind::Bytes, &mut |s, i| {
            (s.funcs[i].tokens.len() >= 2).then_some(s.funcs[i].bytes)
        });
        // 3. The same instructions.
        m.by_unique_key(MatchKind::Instructions, &mut |s, i| {
            let t = s.tokens(i);
            (t.len() >= 4).then(|| hash_of(t))
        });
        // 4. The call graph around what is matched, the strings each function
        // uses and the distinctive numbers in its code, over again: each
        // pairs some, which gives the others more to go on.
        let xrefs = self.xrefs_supported() && newer.xrefs_supported();
        let strings = |s: &mut Side, i: usize| s.strings_key(i);
        let constants = |s: &mut Side, i: usize| (s.funcs[i].tokens.len() >= 8).then(|| s.constants_key(i)).flatten();
        let keys_strings = if xrefs {
            m.all_keys(&strings)
        } else {
            Default::default()
        };
        let keys_constants = m.all_keys(&constants);
        for _ in 0..4 {
            let before = m.pairs.len();
            m.by_key_unique_overall(MatchKind::Strings, &keys_strings, 0.0);
            m.by_key_unique_overall(MatchKind::Constants, &keys_constants, 0.5);
            if xrefs {
                m.through_calls();
            }
            if m.pairs.len() == before {
                break;
            }
        }
        // 6. A ROM's code mostly stays put.
        if rom {
            for i in 0..m.a.funcs.len() {
                if m.a.matched[i].is_some() {
                    continue;
                }
                if let Some(&j) = m.b.at.get(&m.a.funcs[i].address) {
                    let (sa, sb) = (m.a.funcs[i].size as f64, m.b.funcs[j].size as f64);
                    if sa.min(sb) / sa.max(sb) >= 0.75 {
                        m.pair(i, j, MatchKind::Address);
                    }
                }
            }
        }
        let mut out = FunctionDiff {
            pairs: Vec::new(),
            added: Vec::new(),
            removed: Vec::new(),
            identical: 0,
            relocated: 0,
            changed: 0,
        };
        let pairs = std::mem::take(&mut m.pairs);
        // Where each matched function went, to tell moved calls from changed ones.
        let moved: HashMap<u64, u64> = pairs
            .iter()
            .map(|&(i, j, _)| (m.a.funcs[i].address, m.b.funcs[j].address))
            .collect();
        for (i, j, how) in pairs {
            let (fa, fb) = (&m.a.funcs[i], &m.b.funcs[j]);
            let (status, similarity) = if fa.bytes == fb.bytes {
                (PairStatus::Identical, 1.0)
            } else {
                let (a_start, b_start) = (fa.address, fb.address);
                let ta = m.a.tokens(i).to_vec();
                let tb = m.b.tokens(j);
                if ta == tb && self.only_moved(a_start, newer, b_start, &moved, rom) {
                    (PairStatus::Relocated, 1.0)
                } else if ta == tb {
                    // The same instructions, other constants.
                    (PairStatus::Changed, 0.99)
                } else {
                    (PairStatus::Changed, similarity(&ta, tb))
                }
            };
            match status {
                PairStatus::Identical => out.identical += 1,
                PairStatus::Relocated => out.relocated += 1,
                PairStatus::Changed => out.changed += 1,
            }
            out.pairs.push(Pair {
                old: m.a.info(i),
                new: m.b.info(j),
                how,
                status,
                similarity,
            });
        }
        for i in 0..m.a.funcs.len() {
            if m.a.matched[i].is_none() {
                m.a.tokens(i);
                out.removed.push(m.a.info(i));
            }
        }
        for j in 0..m.b.funcs.len() {
            if m.b.matched[j].is_none() {
                m.b.tokens(j);
                out.added.push(m.b.info(j));
            }
        }
        let rank = |p: &Pair| match p.status {
            PairStatus::Changed => 0,
            PairStatus::Relocated => 1,
            PairStatus::Identical => 2,
        };
        out.pairs.sort_by(|p, q| {
            rank(p)
                .cmp(&rank(q))
                .then(p.similarity.total_cmp(&q.similarity))
                .then(p.old.address.cmp(&q.old.address))
        });
        out
    }

    /// Whether two functions with the same instructions differ only in
    /// addresses that went where the code did: calls to functions matched to
    /// each other, branches within them, the same symbols (in a rebuild,
    /// unnamed data may have moved; in a ROM, it stays where it is).
    fn only_moved(
        &self,
        address: u64,
        newer: &Binary,
        newer_address: u64,
        moved: &HashMap<u64, u64>,
        rom: bool,
    ) -> bool {
        let a = self.disassemble_function(address, MAX_INSNS).instructions;
        let b = newer.disassemble_function(newer_address, MAX_INSNS).instructions;
        a.len() == b.len()
            && a.iter().zip(&b).all(|(x, y)| {
                if x.mnemonic == y.mnemonic && x.operands == y.operands {
                    return true;
                }
                let (Some(tx), Some(ty)) = (x.target, y.target) else {
                    return false;
                };
                moved.get(&tx) == Some(&ty)
                    || (tx
                        .checked_sub(address)
                        .is_some_and(|o| Some(o) == ty.checked_sub(newer_address))
                        && tx < address + 0x10_0000)
                    || (x.target_symbol.is_some() && x.target_symbol == y.target_symbol)
                    || (!rom && x.target_symbol.is_none() && y.target_symbol.is_none())
            })
    }

    /// Two functions' instructions lined up: this binary's at `address`,
    /// `newer`'s at `newer_address`.
    pub fn diff_function_code(&self, address: u64, newer: &Binary, newer_address: u64) -> Vec<DiffLine> {
        let a = self.disassemble_function(address, MAX_INSNS).instructions;
        let b = newer.disassemble_function(newer_address, MAX_INSNS).instructions;
        let ta: Vec<u32> = a.iter().map(line_token).collect();
        let tb: Vec<u32> = b.iter().map(line_token).collect();
        let script = line_up(&ta, &tb, 4000);
        let mut out = Vec::new();
        let (mut i, mut j) = (0, 0);
        let mut k = 0;
        while k < script.len() {
            match script[k] {
                Edit::Keep => {
                    // The same instruction; other operands are a change unless they are addresses.
                    let (x, y) = (&a[i], &b[j]);
                    let moved = x.target.is_some() && y.target.is_some();
                    let kind = if x.operands == y.operands || moved {
                        LineKind::Same
                    } else {
                        LineKind::Changed
                    };
                    out.push(DiffLine {
                        kind,
                        old: Some(x.clone()),
                        new: Some(y.clone()),
                    });
                    i += 1;
                    j += 1;
                    k += 1;
                }
                _ => {
                    // A run of deletions and insertions: paired up as changes, the rest removed or added.
                    let mut dels = Vec::new();
                    let mut ins = Vec::new();
                    while k < script.len() && script[k] != Edit::Keep {
                        if script[k] == Edit::Delete {
                            dels.push(a[i].clone());
                            i += 1;
                        } else {
                            ins.push(b[j].clone());
                            j += 1;
                        }
                        k += 1;
                    }
                    let n = dels.len().max(ins.len());
                    let mut dels = dels.into_iter();
                    let mut ins = ins.into_iter();
                    for _ in 0..n {
                        let (old, new) = (dels.next(), ins.next());
                        let kind = match (&old, &new) {
                            (Some(_), Some(_)) => LineKind::Changed,
                            (Some(_), None) => LineKind::Removed,
                            _ => LineKind::Added,
                        };
                        out.push(DiffLine { kind, old, new });
                    }
                }
            }
        }
        out
    }
}

impl FunctionDiff {
    /// The comparison as text: the counts, then the changed functions (least
    /// similar first), the added and the removed, `top` of each.
    pub fn to_text(&self, top: usize) -> String {
        let mut out = format!(
            "Functions: {} identical, {} relocated (only addresses differ), {} changed; {} added, {} removed.\n",
            self.identical,
            self.relocated,
            self.changed,
            self.added.len(),
            self.removed.len()
        );
        let changed: Vec<&Pair> = self.pairs.iter().filter(|p| p.status == PairStatus::Changed).collect();
        if !changed.is_empty() {
            out.push_str("\nChanged (least similar first):\n");
            for p in changed.iter().take(top) {
                let name = if p.old.name == p.new.name {
                    p.old.name.clone()
                } else {
                    format!("{} → {}", p.old.name, p.new.name)
                };
                out.push_str(&format!(
                    "  {:#x} → {:#x}  {:>3.0}%  {:+} bytes  {name}  (matched by {})\n",
                    p.old.address,
                    p.new.address,
                    p.similarity * 100.0,
                    p.new.size as i64 - p.old.size as i64,
                    how(p.how)
                ));
            }
            if changed.len() > top {
                out.push_str(&format!("  … {} more\n", changed.len() - top));
            }
        }
        for (label, list) in [("Added", &self.added), ("Removed", &self.removed)] {
            if list.is_empty() {
                continue;
            }
            out.push_str(&format!("\n{label}:\n"));
            for f in list.iter().take(top) {
                out.push_str(&format!("  {:#x}  {:>6} bytes  {}\n", f.address, f.size, f.name));
            }
            if list.len() > top {
                out.push_str(&format!("  … {} more\n", list.len() - top));
            }
        }
        out
    }
}

fn how(k: MatchKind) -> &'static str {
    match k {
        MatchKind::Name => "name",
        MatchKind::Bytes => "bytes",
        MatchKind::Instructions => "instructions",
        MatchKind::Strings => "the strings they use",
        MatchKind::Constants => "the numbers in their code",
        MatchKind::Calls => "the call graph",
        MatchKind::Address => "address",
    }
}

/// Lined-up instructions as text, old on the left: `=` the same, `~` changed, `-` removed, `+` added.
pub fn code_text(lines: &[DiffLine]) -> String {
    let show = |i: &Option<Instruction>| {
        i.as_ref()
            .map(|i| {
                format!("{:#x}  {} {}", i.address, i.mnemonic, i.operands)
                    .trim_end()
                    .to_string()
            })
            .unwrap_or_default()
    };
    let width = lines
        .iter()
        .map(|l| show(&l.old).chars().count())
        .max()
        .unwrap_or(0)
        .max(20);
    let mut out = String::new();
    for l in lines {
        let mark = match l.kind {
            LineKind::Same => '=',
            LineKind::Changed => '~',
            LineKind::Removed => '-',
            LineKind::Added => '+',
        };
        let old = show(&l.old);
        let pad = width.saturating_sub(old.chars().count());
        out.push_str(format!("{mark} {old}{}   {}\n", " ".repeat(pad), show(&l.new)).trim_end());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_scripts_line_sequences_up() {
        let a = [1, 2, 3, 4, 5, 6];
        let b = [1, 2, 9, 4, 5, 6, 7];
        let s = edit_script(&a, &b, 100).unwrap();
        let keeps = s.iter().filter(|e| **e == Edit::Keep).count();
        let dels = s.iter().filter(|e| **e == Edit::Delete).count();
        let ins = s.iter().filter(|e| **e == Edit::Insert).count();
        assert_eq!((keeps, dels, ins), (5, 1, 2));
        assert_eq!(distance(&a, &b, 100), Some(3));
        assert_eq!(edit_script(&[], &[1, 2], 10).unwrap().len(), 2);
        assert_eq!(edit_script(&[1, 2], &[], 10).unwrap().len(), 2);
        assert!(similarity(&a, &a) == 1.0 && similarity(&a, &b) > 0.7);
        // Replaying the script gives b.
        let (mut i, mut j) = (0, 0);
        let mut got = Vec::new();
        for e in &s {
            match e {
                Edit::Keep => {
                    got.push(a[i]);
                    i += 1;
                    j += 1;
                }
                Edit::Delete => i += 1,
                Edit::Insert => {
                    got.push(b[j]);
                    j += 1;
                }
            }
        }
        assert_eq!(got, b);
    }

    #[test]
    fn huge_sequences_line_up_on_anchors() {
        // 12,000 tokens of a repeating body with a unique token every 50, and a copy
        // with a token changed in each block: far past what Myers is allowed, but
        // the anchors line every block up and only the changed tokens differ.
        let a: Vec<u32> = (0..12_000u32)
            .map(|i| if i % 50 == 0 { 100_000 + i } else { i % 7 })
            .collect();
        let mut b = a.clone();
        for k in (25..b.len()).step_by(50) {
            b[k] = 99;
        }
        let s = line_up(&a, &b, 4000);
        let keeps = s.iter().filter(|e| **e == Edit::Keep).count();
        assert!(keeps >= 12_000 - 240, "{keeps} kept");
        assert_eq!(
            s.iter().filter(|e| **e == Edit::Delete).count(),
            s.iter().filter(|e| **e == Edit::Insert).count()
        );
        // Whole blocks moved: what is shared still counts.
        let mut c = a[6000..].to_vec();
        c.extend(&a[..6000]);
        let s = line_up(&a, &c, 4000);
        let keeps = s.iter().filter(|e| **e == Edit::Keep).count();
        assert!(keeps >= 5900, "{keeps} kept");
        // Small ones are Myers's shortest script.
        assert_eq!(line_up(&[1, 2, 3], &[1, 3], 10), [Edit::Keep, Edit::Delete, Edit::Keep]);
    }
}
