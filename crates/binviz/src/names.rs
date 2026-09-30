//! Names from another build of the same game: a port with its source (a
//! remaster's C#, say), a symbolized build, a related title. Given each of
//! its functions with the string literals it uses and the functions it
//! calls, the functions here that use the same strings are proposed as the
//! same functions, then their neighbours through the calls: a function
//! whose only unmatched callee stands where the candidate's only unmatched
//! callee does is proposed too, with less confidence.
//!
//! The candidates come as JSON: `[{"name": "InitField", "strings":
//! ["field.bin", "%s: %d"], "calls": ["LoadFile"]}, …]`, or from another
//! binary that has names ([`Binary::port_names`]): there, functions pair up
//! by their code first, and data by the instructions that use it.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::binary::Binary;
use crate::error::{Error, Result};
use crate::fndiff::{LineKind, MatchKind, PairStatus};
use crate::model::{Annotation, SymbolKind, SymbolSource};

/// A function of the other build.
#[derive(Debug, Clone, Deserialize)]
pub struct Candidate {
    pub name: String,
    #[serde(default)]
    pub strings: Vec<String>,
    #[serde(default)]
    pub calls: Vec<String>,
}

/// A name proposed for a function here.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameProposal {
    pub address: u64,
    pub current: String,
    pub proposed: String,
    /// 1 when every string of the candidate is used here and nowhere else.
    pub confidence: f32,
    pub evidence: String,
    /// It names data (of `size` bytes), not a function.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub data: bool,
    #[serde(skip_serializing_if = "is_zero")]
    pub size: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameProposals {
    pub proposals: Vec<NameProposal>,
    pub candidates: u32,
    /// Candidates with strings none of which are here.
    pub unmatched: Vec<String>,
}

pub fn parse_candidates(json: &[u8]) -> Result<Vec<Candidate>> {
    serde_json::from_slice(json).map_err(|e| Error::new(format!("not a candidates list: {e}")))
}

impl Binary {
    /// Proposes names for this binary's functions from `candidates`.
    pub fn propose_names(&self, candidates: &[Candidate]) -> NameProposals {
        // The strings each function here uses, and which functions use each string.
        let mut uses: HashMap<u64, HashSet<String>> = HashMap::new();
        let mut users: HashMap<String, Vec<u64>> = HashMap::new();
        let functions: Vec<(u64, String)> = self
            .symbols()
            .functions()
            .filter(|f| f.size > 0)
            .map(|f| (f.address, f.display_name().into_owned()))
            .collect();
        for (address, _) in &functions {
            let Some(s) = self.function_summary(*address, 200) else { continue };
            for u in s.strings {
                uses.entry(*address).or_default().insert(u.text.clone());
                users.entry(u.text).or_default().push(*address);
            }
        }
        let name_of: HashMap<u64, &str> = functions.iter().map(|(a, n)| (*a, n.as_str())).collect();
        let mut proposals: Vec<NameProposal> = Vec::new();
        let mut taken: HashSet<u64> = HashSet::new();
        let mut matched: HashMap<&str, u64> = HashMap::new();
        let mut unmatched = Vec::new();
        // By strings: weigh each string by how few functions use it.
        let mut by_score: Vec<(f32, usize, u64, String)> = Vec::new();
        for (i, c) in candidates.iter().enumerate() {
            if c.strings.is_empty() {
                continue;
            }
            let mut scores: HashMap<u64, f32> = HashMap::new();
            let mut total = 0.0;
            for s in &c.strings {
                let weight = users.get(s).map_or(0.0, |u| 1.0 / u.len() as f32);
                total += 1.0;
                for &f in users.get(s).map(Vec::as_slice).unwrap_or(&[]) {
                    *scores.entry(f).or_default() += weight;
                }
            }
            let Some((&f, &score)) = scores.iter().max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(a.0))) else {
                unmatched.push(c.name.clone());
                continue;
            };
            let shared = c.strings.iter().filter(|s| uses.get(&f).is_some_and(|u| u.contains(*s))).count();
            let evidence = format!(
                "{shared} of {} strings shared{}",
                c.strings.len(),
                if score < shared as f32 { ", some used by other functions too" } else { "" }
            );
            by_score.push((score / total, i, f, evidence));
        }
        by_score.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        for (confidence, i, f, evidence) in by_score {
            if taken.contains(&f) || matched.contains_key(candidates[i].name.as_str()) {
                continue;
            }
            taken.insert(f);
            matched.insert(&candidates[i].name, f);
            proposals.push(NameProposal {
                address: f,
                current: name_of.get(&f).map_or_else(String::new, |n| n.to_string()),
                proposed: candidates[i].name.clone(),
                confidence,
                evidence,
                data: false,
                size: 0,
            });
        }
        // Through the calls: a matched candidate's only unmatched callee, where the
        // matched function's only unmatched callee is.
        for _round in 0..4 {
            let mut added = Vec::new();
            for c in candidates {
                let Some(&f) = matched.get(c.name.as_str()) else { continue };
                let want: Vec<&str> = c
                    .calls
                    .iter()
                    .map(String::as_str)
                    .filter(|n| !matched.contains_key(n) && candidates.iter().any(|k| k.name == *n))
                    .collect();
                let [want] = want.as_slice() else { continue };
                let here: Vec<u64> = self
                    .callees(f)
                    .into_iter()
                    .map(|e| e.address)
                    .filter(|a| !taken.contains(a) && name_of.contains_key(a))
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                let [here] = here.as_slice() else { continue };
                added.push((*here, want.to_string(), c.name.clone()));
            }
            if added.is_empty() {
                break;
            }
            for (f, name, via) in added {
                if taken.contains(&f) || matched.contains_key(name.as_str()) {
                    continue;
                }
                taken.insert(f);
                let key: &str = candidates.iter().find(|c| c.name == name).map(|c| c.name.as_str()).unwrap_or("");
                matched.insert(key, f);
                proposals.push(NameProposal {
                    address: f,
                    current: name_of.get(&f).map_or_else(String::new, |n| n.to_string()),
                    proposed: name,
                    confidence: 0.4,
                    evidence: format!("the only unmatched function {via} calls"),
                    data: false,
                    size: 0,
                });
            }
        }
        proposals.sort_by(|a, b| b.confidence.total_cmp(&a.confidence).then(a.address.cmp(&b.address)));
        NameProposals {
            proposals,
            candidates: candidates.len() as u32,
            unmatched,
        }
    }

    /// Names for this binary's functions and data from another build that
    /// has them (its symbols, its debug info, its notes): a stripped release
    /// named from a symbolized one, an older build from a newer. Functions
    /// pair up by their code first (the same bytes, the same instructions,
    /// their place in the call graph: [`Binary::diff_functions`]), then by
    /// the strings they use and the functions they call; data takes the name
    /// the other build gives what the same instruction of a function with
    /// the same instructions uses. `unmatched` lists the other build's named
    /// functions that have no counterpart here.
    pub fn port_names(&self, from: &Binary) -> NameProposals {
        let diff = from.diff_functions(self);
        let mut proposals = Vec::new();
        let mut placed: HashSet<u64> = HashSet::new();
        let mut used: HashSet<String> = HashSet::new();
        let mut paired: HashSet<u64> = HashSet::new();
        for p in &diff.pairs {
            paired.insert(p.old.address);
            let (Some(name), None) = (real_name(from, p.old.address), real_name(self, p.new.address)) else {
                continue;
            };
            let alike = p.similarity * 100.0;
            let (confidence, evidence) = match p.how {
                MatchKind::Name => continue,
                MatchKind::Bytes => (1.0, "the same bytes".to_string()),
                MatchKind::Instructions => (0.95, "the same instructions, at other addresses".to_string()),
                MatchKind::Calls => (
                    0.5 + 0.4 * p.similarity,
                    format!("where the calls put it, {alike:.0}% alike"),
                ),
                MatchKind::Address => (0.4 * p.similarity, format!("at the same address, {alike:.0}% alike")),
            };
            if placed.insert(p.new.address) && used.insert(name.clone()) {
                proposals.push(NameProposal {
                    address: p.new.address,
                    current: p.new.name.clone(),
                    proposed: name,
                    confidence,
                    evidence,
                    data: false,
                    size: 0,
                });
            }
        }
        // Data: what the same instructions use.
        let mut data: std::collections::BTreeMap<u64, (String, u64)> = std::collections::BTreeMap::new();
        for p in diff.pairs.iter().filter(|p| p.status != PairStatus::Changed) {
            for line in from.diff_function_code(p.old.address, self, p.new.address) {
                let (LineKind::Same, Some(a), Some(b)) = (line.kind, &line.old, &line.new) else {
                    continue;
                };
                let (Some(ta), Some(tb)) = (a.target, b.target) else { continue };
                let Some(r) = from.symbols().lookup(ta) else { continue };
                let Some(s) = from.symbols().get(r.index) else { continue };
                if s.kind != SymbolKind::Data || s.source == SymbolSource::Discovered || is_made_up(&r.name) {
                    continue;
                }
                let Some(start) = tb.checked_sub(r.offset) else { continue };
                if real_name(self, start).is_some() {
                    continue;
                }
                data.entry(start).or_insert((r.name.clone(), r.size));
            }
        }
        for (address, (name, size)) in data {
            if placed.insert(address) && used.insert(name.clone()) {
                proposals.push(NameProposal {
                    address,
                    current: String::new(),
                    proposed: name,
                    confidence: 0.9,
                    evidence: "used by the same instructions of a function with the same code".into(),
                    data: true,
                    size,
                });
            }
        }
        // The rest by the strings they use and the functions they call.
        let named: Vec<(u64, String)> = from
            .symbols()
            .functions()
            .filter(|f| f.size > 0)
            .filter_map(|f| Some((f.address, real_name(from, f.address)?)))
            .collect();
        let candidates: Vec<Candidate> = named
            .iter()
            .filter(|(a, n)| !paired.contains(a) && !used.contains(n))
            .filter_map(|(a, name)| {
                let s = from.function_summary(*a, 64)?;
                let strings: Vec<String> = s.strings.into_iter().map(|u| u.text).collect();
                let calls = s.callees.iter().filter_map(|e| real_name(from, e.address)).collect();
                (!strings.is_empty()).then(|| Candidate {
                    name: name.clone(),
                    strings,
                    calls,
                })
            })
            .collect();
        for p in self.propose_names(&candidates).proposals {
            if real_name(self, p.address).is_none() && placed.insert(p.address) && used.insert(p.proposed.clone()) {
                proposals.push(p);
            }
        }
        proposals.sort_by(|a, b| b.confidence.total_cmp(&a.confidence).then(a.address.cmp(&b.address)));
        let unmatched = named.iter().filter(|(_, n)| !used.contains(n)).filter(|(a, _)| !paired.contains(a));
        NameProposals {
            unmatched: unmatched.map(|(_, n)| n.clone()).collect(),
            candidates: named.len() as u32,
            proposals,
        }
    }
}

impl NameProposals {
    /// The proposals at or above `min_confidence` as notes.
    pub fn annotations(&self, min_confidence: f32) -> Vec<Annotation> {
        self.proposals
            .iter()
            .filter(|p| p.confidence >= min_confidence)
            .map(|p| Annotation {
                address: p.address,
                size: p.size,
                name: p.proposed.clone(),
                comment: format!("named from another build: {} ({:.0}%)", p.evidence, p.confidence * 100.0),
                reviewed: false,
                kind: Some(if p.data { "data" } else { "function" }.into()),
                decomp: None,
                ctype: None,
                author: String::new(),
            })
            .collect()
    }

    pub fn to_text(&self) -> String {
        let data = self.proposals.iter().filter(|p| p.data).count();
        let mut out = format!(
            "{} of {} functions placed{}; {} not found here.\n",
            self.proposals.len() - data,
            self.candidates,
            if data > 0 { format!(", and {data} data") } else { String::new() },
            self.unmatched.len()
        );
        for p in &self.proposals {
            out.push_str(&format!(
                "{:#x}  {:<24} -> {:<24} {:>3.0}%  {}\n",
                p.address,
                p.current,
                p.proposed,
                p.confidence * 100.0,
                p.evidence
            ));
        }
        if !self.unmatched.is_empty() {
            out.push_str(&format!(
                "Not found: {}\n",
                self.unmatched.iter().take(40).cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        out
    }
}

/// The name `bin` gives what starts at `address`, if it is one of its own
/// (its symbols, debug info or notes), not one binviz made up: as it has it
/// (mangled, which reads demangled wherever it is shown).
fn real_name(bin: &Binary, address: u64) -> Option<String> {
    let s = bin.symbols().at(address)?;
    (s.source != SymbolSource::Discovered && !is_made_up(s.name())).then(|| s.name().to_string())
}

/// A name binviz (or a disassembler) makes up: `sub_401000`, `func[12]`.
pub(crate) fn is_made_up(name: &str) -> bool {
    ["sub_", "func[", "FUN_", "loc_", "unk_", "dword_", "byte_", "word_", "flt_", "dbl_", "off_", "stru_"]
        .iter()
        .any(|p| name.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_by_strings_and_calls() {
        // Three functions: A uses "field.bin" and calls B; B uses "%s: %d"; C (called by A) uses nothing.
        // Code at 0x80010000, strings at 0x80010080.
        let mut words: Vec<u32> = vec![
            0x27BD_FFE8, // A: addiu $sp, -0x18
            0xAFBF_0014, // sw $ra
            0x3C04_8001, // lui $a0, 0x8001
            0x2484_0080, // addiu $a0, "field.bin"
            0x0C00_4010, // jal B (0x80010040)
            0x0000_0000,
            0x0C00_4018, // jal C (0x80010060)
            0x0000_0000,
            0x8FBF_0014, // lw $ra
            0x03E0_0008, // jr $ra
            0x27BD_0018,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x3C04_8001, // B at 0x40: lui $a0
            0x2484_008C, // addiu $a0, "%s: %d"
            0x03E0_0008, // jr $ra
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x2402_0001, // C at 0x60: li $v0, 1
            0x03E0_0008, // jr $ra
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
        ];
        assert_eq!(words.len() * 4, 0x80);
        let mut data = vec![0u8; 0x800];
        data[..8].copy_from_slice(b"PS-X EXE");
        let mut code: Vec<u8> = words.drain(..).flat_map(|w| w.to_le_bytes()).collect();
        code.extend(b"field.bin\0\0\0%s: %d\0\0");
        code.resize(0xA0, 0);
        for (at, v) in [(0x10, 0x8001_0000u32), (0x14, 0x8001_8000), (0x18, 0x8001_0000), (0x1C, code.len() as u32)] {
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        data.extend(code);
        let bin = Binary::parse(data).unwrap();
        let candidates = parse_candidates(
            br#"[{"name":"LoadField","strings":["field.bin"],"calls":["Printf","Tick"]},
                {"name":"Printf","strings":["%s: %d"]},
                {"name":"Tick","strings":[]},
                {"name":"Nowhere","strings":["missing string"]}]"#,
        )
        .unwrap();
        let p = bin.propose_names(&candidates);
        let got: Vec<(u64, &str, f32)> = p.proposals.iter().map(|p| (p.address, p.proposed.as_str(), p.confidence)).collect();
        assert_eq!(got.len(), 3, "{}", p.to_text());
        assert_eq!(got[0], (0x8001_0000, "LoadField", 1.0));
        assert_eq!(got[1], (0x8001_0040, "Printf", 1.0));
        assert_eq!((got[2].0, got[2].1), (0x8001_0060, "Tick"));
        assert_eq!(p.unmatched, ["Nowhere"]);
        assert_eq!(p.annotations(0.5).len(), 2);
    }
}
