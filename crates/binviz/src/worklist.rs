//! What to look at next while mapping out a binary: the functions nothing
//! names yet, those whose callees all have names first (what they call says
//! what they do), then those called the most, then the smallest.
//!
//! Several agents can split the work: each takes one shard of the functions.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::binary::Binary;
use crate::model::{SymbolKind, SymbolSource};
use crate::xrefs::RefKind;

/// A function still to name.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub address: u64,
    /// What it is called now (`sub_4010a0`).
    pub name: String,
    pub size: u64,
    /// Functions calling it.
    pub callers: u32,
    /// Functions it calls, and how many of those have no name yet.
    pub callees: u32,
    pub unnamed_callees: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worklist {
    /// Functions in the binary, and how many have names (from the file, its
    /// debug info, or notes).
    pub functions: u32,
    pub named: u32,
    /// Functions still to name (in the shard asked for).
    pub remaining: u32,
    pub items: Vec<WorkItem>,
}

/// Which of `n` shards an address is in: spread evenly, whatever the layout.
fn shard_of(address: u64, n: u32) -> u32 {
    ((address.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32) % u64::from(n.max(1))) as u32
}

impl Binary {
    /// The `limit` unnamed functions to look at next. `shard` = (k, n) keeps
    /// the k-th of n parts (from 0), so that n agents can split the work;
    /// `skip` leaves out functions given up on.
    pub fn worklist(&self, limit: usize, shard: Option<(u32, u32)>, skip: &[u64]) -> Worklist {
        let mut named: HashSet<u64> = HashSet::new();
        let mut functions: Vec<(u64, u64)> = Vec::new();
        for s in self.symbols.iter().filter(|s| s.defined) {
            if s.kind == SymbolKind::Function {
                functions.push((s.address, s.size));
            }
            if s.source != SymbolSource::Discovered {
                named.insert(s.address);
            }
        }
        functions.sort_unstable();
        functions.dedup_by_key(|f| f.0);
        let starts: HashSet<u64> = functions.iter().map(|f| f.0).collect();
        // Who calls whom: calls and tail calls, between function starts.
        let index = self.xref_index();
        let mut callers: HashMap<u64, HashSet<u64>> = HashMap::new();
        let mut callees: HashMap<u64, HashSet<u64>> = HashMap::new();
        for kind in [RefKind::Call, RefKind::Jump] {
            for (source, target) in index.pairs(kind) {
                if !starts.contains(&target) {
                    continue;
                }
                let Some(from) = self.symbols.function_containing(source) else {
                    continue;
                };
                callers.entry(target).or_default().insert(from.address);
                callees.entry(from.address).or_default().insert(target);
            }
        }
        let skip: HashSet<u64> = skip.iter().copied().collect();
        let mut todo: Vec<WorkItem> = functions
            .iter()
            .filter(|f| !named.contains(&f.0) && !skip.contains(&f.0))
            .filter(|f| shard.is_none_or(|(k, n)| shard_of(f.0, n) == k))
            .map(|&(address, size)| {
                let out = callees.get(&address);
                WorkItem {
                    address,
                    name: self
                        .symbols
                        .at(address)
                        .map_or_else(|| format!("sub_{address:x}"), |s| s.display_name().into_owned()),
                    size,
                    callers: callers.get(&address).map_or(0, |c| c.len() as u32),
                    callees: out.map_or(0, |c| c.len() as u32),
                    unnamed_callees: out.map_or(0, |c| {
                        c.iter().filter(|&&t| t != address && !named.contains(&t)).count() as u32
                    }),
                }
            })
            .collect();
        let remaining = todo.len() as u32;
        todo.sort_by_key(|w| (w.unnamed_callees, Reverse(w.callers), w.size, w.address));
        todo.truncate(limit);
        Worklist {
            functions: functions.len() as u32,
            named: functions.iter().filter(|f| named.contains(&f.0)).count() as u32,
            remaining,
            items: todo,
        }
    }
}

/// A function both name: the name notes give it, and its real one.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamePair {
    pub address: u64,
    pub real: String,
    pub noted: String,
    /// The same name (ignoring case and leading underscores).
    pub same: bool,
    /// Not the same, but sharing a word (`print_string` and `print`).
    pub close: bool,
}

/// How the names notes give compare with the real ones (from debug info, or
/// an unstripped build): an answer key for mapping a binary blind.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameComparison {
    /// Functions the real names cover.
    pub functions: u32,
    /// Those the notes name too, with both names.
    pub pairs: Vec<NamePair>,
    /// Those the notes don't name: (address, real name).
    pub missed: Vec<(u64, String)>,
    /// Names in notes where no real function starts (data, labels, or a wrong start).
    pub extra: Vec<(u64, String)>,
}

/// The words of a name: `printString2`, `print_string` → print, string.
fn words(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if !c.is_ascii_alphanumeric() || (c.is_ascii_uppercase() && prev_lower) {
            if cur.len() >= 3 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
        }
        if c.is_ascii_alphabetic() {
            cur.push(c.to_ascii_lowercase());
        }
        prev_lower = c.is_ascii_lowercase();
    }
    if cur.len() >= 3 {
        out.push(cur);
    }
    out
}

impl Binary {
    /// The names this binary's notes give, against the real ones `real` has
    /// (the same binary with its debug file, or an unstripped build of it).
    pub fn compare_names(&self, real: &Binary) -> NameComparison {
        let mut truth: Vec<(u64, String)> = real
            .symbols
            .functions()
            .filter(|s| !matches!(s.source, SymbolSource::Discovered | SymbolSource::User))
            .map(|s| (s.address, s.display_name().into_owned()))
            .collect();
        truth.sort_unstable();
        truth.dedup_by_key(|t| t.0);
        let noted: HashMap<u64, &str> = self
            .annotations
            .iter()
            .filter(|a| !a.name.is_empty())
            .map(|a| (a.address, a.name.as_str()))
            .collect();
        let plain = |s: &str| s.trim_start_matches('_').to_ascii_lowercase();
        let mut pairs = Vec::new();
        let mut missed = Vec::new();
        for (address, real) in &truth {
            match noted.get(address) {
                Some(&n) => {
                    let same = plain(n) == plain(real);
                    let theirs = words(real);
                    pairs.push(NamePair {
                        address: *address,
                        real: real.clone(),
                        noted: n.to_string(),
                        same,
                        close: !same && words(n).iter().any(|w| theirs.contains(w)),
                    });
                }
                None => missed.push((*address, real.clone())),
            }
        }
        let starts: HashSet<u64> = truth.iter().map(|t| t.0).collect();
        let mut extra: Vec<(u64, String)> = noted
            .iter()
            .filter(|(a, _)| !starts.contains(a))
            .map(|(a, n)| (*a, n.to_string()))
            .collect();
        extra.sort_unstable();
        NameComparison {
            functions: truth.len() as u32,
            pairs,
            missed,
            extra,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{shard_of, words};

    #[test]
    fn names_split_into_words() {
        assert_eq!(words("printString2"), ["print", "string"]);
        assert_eq!(words("_update_player_x"), ["update", "player"]);
        assert_eq!(words("NMI"), ["nmi"]);
    }

    #[test]
    fn shards_split_evenly() {
        let mut counts = [0u32; 4];
        for a in (0x1000..0x9000u64).step_by(0x10) {
            counts[shard_of(a, 4) as usize] += 1;
        }
        // 2048 functions: about 512 in each.
        assert!(counts.iter().all(|&c| (400..=620).contains(&c)), "{counts:?}");
    }
}
