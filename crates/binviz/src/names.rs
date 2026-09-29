//! Names from another build of the same game: a port with its source (a
//! remaster's C#, say), a symbolized build, a related title. Given each of
//! its functions with the string literals it uses and the functions it
//! calls, the functions here that use the same strings are proposed as the
//! same functions, then their neighbours through the calls: a function
//! whose only unmatched callee stands where the candidate's only unmatched
//! callee does is proposed too, with less confidence.
//!
//! The candidates come as JSON: `[{"name": "InitField", "strings":
//! ["field.bin", "%s: %d"], "calls": ["LoadFile"]}, …]`.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::binary::Binary;
use crate::error::{Error, Result};
use crate::model::Annotation;

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
}

impl NameProposals {
    /// The proposals at or above `min_confidence` as notes.
    pub fn annotations(&self, min_confidence: f32) -> Vec<Annotation> {
        self.proposals
            .iter()
            .filter(|p| p.confidence >= min_confidence)
            .map(|p| Annotation {
                address: p.address,
                size: 0,
                name: p.proposed.clone(),
                comment: format!("named from another build: {} ({:.0}%)", p.evidence, p.confidence * 100.0),
                reviewed: false,
                decomp: None,
            })
            .collect()
    }

    pub fn to_text(&self) -> String {
        let mut out = format!(
            "{} of {} candidates placed; {} had strings not found here.\n",
            self.proposals.len(),
            self.candidates,
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
