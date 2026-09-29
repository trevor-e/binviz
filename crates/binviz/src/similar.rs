//! Functions shaped alike: the already-decompiled functions whose C is the
//! best worked example for another, and near-copies (templates, code pasted
//! and edited).
//!
//! A function is read as the tokens function diffs use (each instruction's
//! operation and the kinds of its operands, never the addresses or numbers
//! in it) and summed up by a MinHash of its runs of three tokens: two
//! summaries agree in about the share of runs the functions have in common.
//! Summaries are filed by bands of four hashes (locality-sensitive hashing),
//! so functions that share much of their code meet in some band; only those
//! are then compared instruction by instruction.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::binary::Binary;
use crate::fndiff::similarity;
use crate::model::RegionKind;

/// Hashes in a function's summary, and per band.
const HASHES: usize = 64;
const BAND: usize = 4;
/// At most this many instructions of a function are read.
const MAX_INSNS: usize = 4000;
/// Functions a search takes from each band: when thousands share one (tiny
/// functions all look alike), the rest say nothing more.
const PER_BAND: usize = 32;
/// Candidates compared instruction by instruction, the best by their summaries.
const COMPARED: usize = 8;

/// The binary's own functions summed up for finding their look-alikes.
pub(crate) struct SimilarIndex {
    /// Start address, bytes up to the alignment filler, and instruction tokens.
    functions: Vec<(u64, u64, Vec<u32>)>,
    by_address: HashMap<u64, u32>,
    /// HASHES per function.
    summaries: Vec<u32>,
    /// (band, its hashes combined) → functions.
    bands: HashMap<(u8, u64), Vec<u32>>,
}

/// Some of the functions, filed by band: those a search looks among.
pub(crate) struct Filed {
    bands: HashMap<(u8, u64), Vec<u32>>,
}

/// A function like another, and how alike: 1 for the same instructions,
/// down to 0 for nothing in common.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Similar {
    pub address: u64,
    pub similarity: f32,
    pub instructions: u32,
}

/// A 64-bit mix (splitmix64's finalizer).
fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// The MinHash of a function's runs of three tokens (of its tokens, when it
/// has fewer than three).
fn summary(tokens: &[u32]) -> [u32; HASHES] {
    let mut out = [u32::MAX; HASHES];
    let runs: Box<dyn Iterator<Item = u64>> = if tokens.len() < 3 {
        Box::new(tokens.iter().map(|&t| t as u64))
    } else {
        Box::new(
            tokens
                .windows(3)
                .map(|w| (w[0] as u64) << 42 ^ (w[1] as u64) << 21 ^ w[2] as u64),
        )
    };
    for run in runs {
        let h = mix(run);
        for (j, slot) in out.iter_mut().enumerate() {
            let v = mix(h ^ (j as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)) as u32;
            if v < *slot {
                *slot = v;
            }
        }
    }
    out
}

impl SimilarIndex {
    fn build(bin: &Binary) -> SimilarIndex {
        let mut functions = Vec::new();
        for sec in bin.sections.iter().filter(|s| s.loaded && s.kind == RegionKind::Code) {
            for (start, end) in bin.symbols.own_functions_in(sec.address, sec.address + sec.size) {
                let (tokens, bytes) = bin.instruction_tokens(start, end, MAX_INSNS);
                if !tokens.is_empty() {
                    functions.push((start, bytes, tokens));
                }
            }
        }
        functions.sort_by_key(|f| f.0);
        functions.dedup_by_key(|f| f.0);
        let mut summaries = Vec::with_capacity(functions.len() * HASHES);
        for (_, _, tokens) in &functions {
            summaries.extend_from_slice(&summary(tokens));
        }
        let by_address = functions.iter().enumerate().map(|(i, f)| (f.0, i as u32)).collect();
        let mut index = SimilarIndex {
            functions,
            by_address,
            summaries,
            bands: HashMap::new(),
        };
        index.bands = index.file(&|_| true).bands;
        index
    }

    /// The keys of the bands function `i` is filed under.
    fn band_keys(&self, i: usize) -> impl Iterator<Item = (u8, u64)> + '_ {
        self.summaries[i * HASHES..(i + 1) * HASHES]
            .chunks(BAND)
            .enumerate()
            .map(|(band, hashes)| {
                let key = hashes.iter().fold(band as u64, |acc, &h| mix(acc ^ h as u64));
                (band as u8, key)
            })
    }

    /// The functions for which `keep` holds, filed by band.
    pub(crate) fn file(&self, keep: &dyn Fn(u64) -> bool) -> Filed {
        let mut bands: HashMap<(u8, u64), Vec<u32>> = HashMap::new();
        for (i, f) in self.functions.iter().enumerate() {
            if keep(f.0) {
                for key in self.band_keys(i) {
                    bands.entry(key).or_default().push(i as u32);
                }
            }
        }
        Filed { bands }
    }

    /// How many of two functions' summary hashes agree: about the share of their runs in common.
    fn agreeing(&self, i: usize, j: usize) -> usize {
        let a = &self.summaries[i * HASHES..(i + 1) * HASHES];
        let b = &self.summaries[j * HASHES..(j + 1) * HASHES];
        a.iter().zip(b).filter(|(x, y)| x == y).count()
    }

    /// The functions summed up, in address order: (start, bytes, instructions).
    pub(crate) fn functions(&self) -> impl Iterator<Item = (u64, u64, u32)> + '_ {
        self.functions.iter().map(|f| (f.0, f.1, f.2.len() as u32))
    }

    pub(crate) fn contains(&self, address: u64) -> bool {
        self.by_address.contains_key(&address)
    }

    /// The functions filed in `bands` most like the one at `address`, most
    /// alike first, at least `floor` alike: those sharing a band with it,
    /// the closest by their summaries compared instruction by instruction.
    fn like_in(&self, bands: &HashMap<(u8, u64), Vec<u32>>, address: u64, count: usize, floor: f32) -> Vec<Similar> {
        let Some(&i) = self.by_address.get(&address) else {
            return Vec::new();
        };
        let i = i as usize;
        let mut seen = HashSet::new();
        let mut candidates: Vec<(usize, usize)> = Vec::new();
        for key in self.band_keys(i) {
            let Some(list) = bands.get(&key) else { continue };
            for &j in list.iter().filter(|&&j| j as usize != i).take(PER_BAND) {
                if seen.insert(j) {
                    candidates.push((self.agreeing(i, j as usize), j as usize));
                }
            }
        }
        candidates.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        candidates.truncate(count.max(COMPARED));
        let tokens = &self.functions[i].2;
        let mut out: Vec<Similar> = candidates
            .into_iter()
            .map(|(_, j)| {
                let f = &self.functions[j];
                Similar {
                    address: f.0,
                    similarity: similarity(tokens, &f.2),
                    instructions: f.2.len() as u32,
                }
            })
            .filter(|s| s.similarity >= floor)
            .collect();
        out.sort_by(|a, b| b.similarity.total_cmp(&a.similarity).then(a.address.cmp(&b.address)));
        out.truncate(count);
        out
    }

    /// The functions filed in `filed` most like the one at `address`.
    pub(crate) fn like(&self, filed: &Filed, address: u64, count: usize, floor: f32) -> Vec<Similar> {
        self.like_in(&filed.bands, address, count, floor)
    }
}

impl Binary {
    pub(crate) fn similar_index(&self) -> &SimilarIndex {
        self.similar.get_or_init(|| SimilarIndex::build(self))
    }

    /// The functions whose instructions are most like those of the function
    /// at `address` (its start), most alike first. Only functions that share
    /// a good part of their code are found.
    pub fn similar_functions(&self, address: u64, count: usize) -> Vec<Similar> {
        let index = self.similar_index();
        index.like_in(&index.bands, address, count, 0.0)
    }
}
