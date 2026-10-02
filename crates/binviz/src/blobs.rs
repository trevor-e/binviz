//! Code inside a file binviz has no reader for: a PlayStation game's archive
//! of overlays, say. Sectors that hold MIPS code stand out (functions that
//! save `$ra` and return, calls to other functions), and a run of them is a
//! blob to open as an overlay. Where it loads is not written in the blob,
//! but its own calls and pointers give it away: `jal` targets and pointers
//! into itself land on function prologues at exactly one base address, so
//! each one votes for the bases that would make it so.

use std::collections::HashMap;

use serde::Serialize;

use crate::rom::prologues;

/// PlayStation RAM as a program sees it.
const RAM: std::ops::Range<u64> = 0x8001_0000..0x8020_0000;
const SECTOR: usize = 0x800;

/// A guess at where a blob loads.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseGuess {
    pub address: u64,
    /// Distinct call and pointer targets that land on a function start there.
    pub votes: u32,
}

/// A run of sectors that hold code.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeBlob {
    pub offset: u64,
    pub size: u64,
    /// Function prologues in it.
    pub functions: u32,
    /// Distinct calls and pointers into RAM outside the boot executable, the voters.
    pub targets: u32,
    /// The best guesses at the load address, best first.
    pub bases: Vec<BaseGuess>,
    /// Bytes after the code that belong with it (its jump tables, strings and
    /// other data, found because the code reaches them or they point into it):
    /// `size` includes them.
    pub data: u64,
    /// When one run of code held several overlays: the run's offset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_from: Option<u64>,
}

impl CodeBlob {
    /// True when the first guess has enough votes, and well over the second's, to believe.
    /// (Calls into other overlays never vote for any base, so a share of the targets
    /// as small as a seventh can be a clear win.)
    pub fn confident(&self) -> bool {
        match self.bases.as_slice() {
            [best, rest @ ..] => {
                best.votes >= 8
                    && best.votes * 7 >= self.targets
                    && rest.first().is_none_or(|b| best.votes >= 2 * b.votes)
            }
            [] => false,
        }
    }
}

/// The addresses a PS-X EXE's program occupies (its header), for [`find_code_blobs`] to leave
/// out: calls into it say nothing about where an overlay loads. The header's size can include
/// zeroes standing for memory the game fills later (FF9's is 1.9 MB, of which 350 KB is the
/// program), and a few stray bytes at the end (FF9 has a stub at `0x801edf00`) don't count: the
/// program is as far as windows of 4 KiB are an eighth used.
pub fn exe_range(exe: &[u8]) -> Option<std::ops::Range<u64>> {
    if !exe.starts_with(b"PS-X EXE") || exe.len() < 0x800 {
        return None;
    }
    let (load, size) = (word(exe, 0x18) as u64, word(exe, 0x1C) as u64);
    let used = exe[0x800..]
        .chunks(4096)
        .rposition(|w| w.iter().filter(|&&b| b != 0).count() * 8 >= w.len())
        .map_or(0, |i| (i as u64 + 1) * 4096);
    Some(load..load + size.min(used))
}

fn word(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

/// Whether a sector looks like code: it returns, saves `$ra`, and calls.
fn code_sector(s: &[u8]) -> bool {
    let (mut returns, mut saves, mut calls) = (0, 0, 0);
    for at in (0..SECTOR).step_by(4) {
        let w = word(s, at);
        returns += (w == 0x03E0_0008) as u32;
        saves += (w >> 16 == 0xAFBF) as u32;
        calls += (w >> 26 == 3) as u32;
    }
    returns >= 1 && saves >= 1 && calls >= 2
}

/// The code blobs in `data` (MIPS, little-endian), in file order. Calls into
/// `skip` (the boot executable's range) say nothing about where a blob
/// loads, so they don't vote.
pub fn find_code_blobs(data: &[u8], skip: Option<std::ops::Range<u64>>) -> Vec<CodeBlob> {
    let sectors = data.len() / SECTOR;
    let code: Vec<bool> = (0..sectors)
        .map(|i| code_sector(&data[i * SECTOR..(i + 1) * SECTOR]))
        .collect();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < sectors {
        if !code[i] {
            i += 1;
            continue;
        }
        // Up to two data sectors inside a blob (a table between functions).
        let mut end = i + 1;
        let mut j = end;
        while j < sectors && j <= end + 2 {
            if code[j] {
                end = j + 1;
            }
            j += 1;
        }
        if end - i >= 3 {
            runs.push((i, end));
        }
        i = end;
    }
    let mut out = Vec::new();
    for (k, &(a, b)) in runs.iter().enumerate() {
        // Never into the next run of code.
        let limit = runs.get(k + 1).map_or(sectors, |r| r.0);
        let pieces = split_run(data, a, b, skip.as_ref());
        for (i, &(pa, pb)) in pieces.iter().enumerate() {
            let mut blob = blob_at(&data[pa * SECTOR..pb * SECTOR], (pa * SECTOR) as u64, skip.as_ref());
            if pieces.len() > 1 {
                blob.split_from = Some((a * SECTOR) as u64);
            }
            let end = pieces.get(i + 1).map_or(limit, |p| p.0);
            if blob.confident() {
                extend(&mut blob, data, pb, end);
            }
            out.push(blob);
        }
    }
    out
}

/// The `jal` targets in `words`, and the pointers into RAM.
fn targets_in(blob: &[u8], skip: Option<&std::ops::Range<u64>>) -> Vec<u64> {
    let mut targets = Vec::new();
    for at in (0..blob.len() / 4 * 4).step_by(4) {
        let w = word(blob, at);
        let t = if w >> 26 == 3 {
            ((w as u64 & 0x03FF_FFFF) << 2) | 0x8000_0000
        } else if (w as u64) & 3 == 0 && RAM.contains(&(w as u64)) {
            w as u64
        } else {
            continue;
        };
        if RAM.contains(&t) && !skip.is_some_and(|s| s.contains(&t)) {
            targets.push(t);
        }
    }
    targets
}

/// A run of code sectors as the overlays in it: one piece, or several when
/// the run is overlays stored one after another (the calls in its first
/// sectors land on function starts at one base, those in its later sectors
/// at another). Sector ranges.
fn split_run(data: &[u8], a: usize, b: usize, skip: Option<&std::ops::Range<u64>>) -> Vec<(usize, usize)> {
    let whole = blob_at(&data[a * SECTOR..b * SECTOR], (a * SECTOR) as u64, skip);
    if whole.bases.len() < 2 || b - a < 4 {
        return vec![(a, b)];
    }
    let starts: std::collections::HashSet<u64> = prologues(&data[a * SECTOR..b * SECTOR], 0).into_iter().collect();
    // Each sector's base: the candidate most of its calls land on a function start at.
    let winner: Vec<Option<(u64, u32)>> = (a..b)
        .map(|i| {
            let sector = &data[i * SECTOR..(i + 1) * SECTOR];
            let calls: Vec<u64> = targets_in(sector, skip);
            whole
                .bases
                .iter()
                .map(|g| {
                    let hits = calls
                        .iter()
                        .filter(|&&t| t.checked_sub(g.address).is_some_and(|o| starts.contains(&o)))
                        .count() as u32;
                    (g.address, hits)
                })
                .filter(|&(_, hits)| hits > 0)
                .max_by_key(|&(_, hits)| hits)
        })
        .collect();
    // Segments of sectors agreeing on a base; a sector with no calls goes with the one before.
    let mut segments: Vec<(usize, usize, u64, u32, u32)> = Vec::new(); // (from, to, base, hits, sectors with hits)
    for (k, w) in winner.iter().enumerate() {
        let i = a + k;
        match (segments.last_mut(), w) {
            (Some(s), Some((base, hits))) if s.2 == *base => {
                s.1 = i + 1;
                s.3 += hits;
                s.4 += 1;
            }
            (Some(s), None) => s.1 = i + 1,
            (_, Some((base, hits))) => segments.push((i, i + 1, *base, *hits, 1)),
            (None, None) => segments.push((i, i + 1, 0, 0, 0)),
        }
    }
    // A segment too weak to be an overlay of its own (a few calls landing on a
    // base by chance) goes with its neighbour.
    let mut k = 0;
    while segments.len() > 1 && k < segments.len() {
        if segments[k].3 < 4 || segments[k].4 < 2 {
            let weak = segments.remove(k);
            let into = if k == 0 { 0 } else { k - 1 };
            segments[into].0 = segments[into].0.min(weak.0);
            segments[into].1 = segments[into].1.max(weak.1);
        } else {
            k += 1;
        }
    }
    if segments.len() < 2 {
        return vec![(a, b)];
    }
    segments.into_iter().map(|s| (s.0, s.1)).collect()
}

/// The addresses a blob's code builds with `lui` and a following `addiu`,
/// `ori`, load or store off that register (its strings, tables, globals), at
/// the base its first guess gives.
fn addresses_built(blob: &[u8]) -> Vec<u64> {
    let mut out = Vec::new();
    let n = blob.len() / 4;
    for i in 0..n {
        let w = word(blob, 4 * i);
        if w >> 26 != 15 {
            continue;
        }
        let (rt, hi) = ((w >> 16) & 31, (w & 0xFFFF) as u64);
        for k in 1..=8 {
            if i + k >= n {
                break;
            }
            let x = word(blob, 4 * (i + k));
            let (op, rs) = (x >> 26, (x >> 21) & 31);
            let lo = (x & 0xFFFF) as u16 as i16 as i64;
            if rs == rt && (op == 9 || op == 13 || (32..=46).contains(&op)) {
                let v = if op == 13 {
                    (hi << 16) | (x & 0xFFFF) as u64
                } else {
                    ((hi << 16) as i64 + lo) as u64
                };
                if RAM.contains(&v) {
                    out.push(v);
                }
                break;
            }
            // The register written again before use: not this pair.
            if crate::cpu::mips::MipsWord(x).writes() == Some(rt) {
                break;
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Takes the sectors after a blob's code that belong with it (up to `end`,
/// a sector index): a sector the code reaches (an address it builds falls in
/// it), one holding pointers into the blob (a jump table, a table of
/// callbacks), or one of code calling into it (a function without a saved
/// `$ra` the sector test missed). Up to two sectors with none of that are
/// crossed when one with some follows.
fn extend(blob: &mut CodeBlob, data: &[u8], from: usize, end: usize) {
    const MAX_SECTORS: usize = 512;
    let base = blob.bases[0].address;
    let code = &data[blob.offset as usize..(blob.offset + blob.size) as usize];
    let built = addresses_built(code);
    let starts: Vec<u64> = prologues(code, base);
    let mut taken = from;
    let mut i = from;
    while i < end && i - from < MAX_SECTORS && i - taken <= 2 {
        let sector = &data[i * SECTOR..(i + 1) * SECTOR];
        let lo = base + (i * SECTOR) as u64 - blob.offset;
        let hi = lo + SECTOR as u64;
        let reached = built.iter().any(|&a| a >= lo && a < hi);
        let extent = base..lo + SECTOR as u64;
        let pointers = (0..SECTOR)
            .step_by(4)
            .filter(|&at| {
                let v = u64::from(word(sector, at));
                v & 3 == 0 && extent.contains(&v)
            })
            .count();
        let calls_in = (0..SECTOR)
            .step_by(4)
            .filter(|&at| {
                let w = word(sector, at);
                w >> 26 == 3
                    && starts
                        .binary_search(&(((w as u64 & 0x03FF_FFFF) << 2) | 0x8000_0000))
                        .is_ok()
            })
            .count();
        if reached || pointers >= 4 || calls_in >= 2 {
            taken = i + 1;
        }
        i += 1;
    }
    if taken > from {
        let added = ((taken - from) * SECTOR) as u64;
        blob.size += added;
        blob.data = added;
    }
}

/// A blob's shape and load address guesses.
pub fn blob_at(blob: &[u8], offset: u64, skip: Option<&std::ops::Range<u64>>) -> CodeBlob {
    let starts: Vec<u64> = prologues(blob, 0);
    let mut targets = targets_in(blob, skip);
    targets.sort_unstable();
    targets.dedup();
    let mut votes: HashMap<u64, u32> = HashMap::new();
    for &t in &targets {
        for &p in &starts {
            if let Some(base) = t.checked_sub(p)
                && RAM.contains(&base)
                && base + blob.len() as u64 <= RAM.end
            {
                *votes.entry(base).or_default() += 1;
            }
        }
    }
    let mut bases: Vec<BaseGuess> = votes
        .into_iter()
        .map(|(address, votes)| BaseGuess { address, votes })
        .collect();
    bases.sort_by_key(|b| (std::cmp::Reverse(b.votes), b.address));
    // A base a few instructions off the best one is the same guess seen through a
    // neighbouring prologue, not another place to load it.
    let mut distinct: Vec<BaseGuess> = Vec::new();
    for b in bases {
        if distinct.iter().all(|d| d.address.abs_diff(b.address) > 0x40) {
            distinct.push(b);
        }
        if distinct.len() == 3 {
            break;
        }
    }
    let bases = distinct;
    CodeBlob {
        offset,
        size: blob.len() as u64,
        functions: starts.len() as u32,
        targets: targets.len() as u32,
        bases,
        data: 0,
        split_from: None,
    }
}

/// The blobs, one to a line.
pub fn blobs_text(blobs: &[CodeBlob]) -> String {
    let mut out = format!("{} blobs of code:\n", blobs.len());
    for b in blobs {
        let guess = match b.bases.first() {
            Some(g) => format!(
                "loads at {:#x} ({} of {} targets{}{})",
                g.address,
                g.votes,
                b.targets,
                if b.confident() { "" } else { ", unsure" },
                b.bases
                    .get(1)
                    .map_or(String::new(), |o| format!("; next {:#x}: {}", o.address, o.votes)),
            ),
            None if b.targets == 0 => "makes no calls outside the boot executable, so nothing says where".to_string(),
            None => format!("{} calls and pointers, none consistent with one base", b.targets),
        };
        out.push_str(&format!(
            "  @{:#x}..{:#x} ({} bytes, {} functions{}) {guess}{}\n",
            b.offset,
            b.offset + b.size,
            b.size,
            b.functions,
            if b.data > 0 {
                format!(", then {} bytes of its data", b.data)
            } else {
                String::new()
            },
            match b.split_from {
                Some(run) => format!("; one of several overlays stored together from @{run:#x}"),
                None => String::new(),
            },
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// 170 functions of different lengths, each calling the one before it, loaded at `base`.
    /// (Identical functions would fit any base a whole function away.)
    pub(crate) fn functions_at(base: u32) -> Vec<u32> {
        let mut words: Vec<u32> = Vec::new();
        let mut previous = base;
        for i in 0..170u32 {
            let this = base + 4 * words.len() as u32;
            let target = if i == 0 { this } else { previous };
            let call = 0x0C00_0000 | (target >> 2) & 0x03FF_FFFF;
            words.extend([0x27BD_FFE8, 0xAFBF_0014, call, 0, call, 0]);
            words.extend(std::iter::repeat_n(
                0x2402_0001,
                (i.wrapping_mul(2_654_435_761) >> 13) as usize % 13,
            ));
            words.extend([0x8FBF_0014, 0, 0x03E0_0008, 0x27BD_0018]);
            previous = this;
        }
        words
    }

    #[test]
    fn code_in_a_file_is_found_with_its_base() {
        // Functions that call the one before, loaded at 0x80123000, after noise and before more noise.
        let base = 0x8012_3000u32;
        let words = functions_at(base);
        let mut file = vec![0x55u8; SECTOR * 3];
        let at = file.len();
        file.extend(le(&words));
        file.resize((file.len() / SECTOR + 1) * SECTOR, 0);
        file.extend(vec![0xAAu8; SECTOR * 2]);
        let blobs = find_code_blobs(&file, None);
        assert_eq!(blobs.len(), 1, "{blobs:?}");
        assert_eq!(blobs[0].offset, at as u64);
        assert_eq!(blobs[0].bases[0].address, base as u64, "{blobs:?}");
        assert!(blobs[0].confident(), "{blobs:?}");
        assert_eq!(blobs[0].data, 0);
    }

    #[test]
    fn a_blobs_data_after_its_code_comes_with_it() {
        // The code, then a sector of a jump table pointing into it, a sector of
        // numbers nothing reaches, a sector of strings the code builds the address
        // of, then noise. The table and the strings are the blob's; the numbers
        // between them are crossed; the noise is not.
        let base = 0x8012_3000u32;
        let mut words = functions_at(base);
        let code_len = (words.len() * 4).div_ceil(SECTOR) * SECTOR;
        let strings_at = base + (code_len + 2 * SECTOR) as u32;
        // lui $a0, %hi(strings) / addiu $a0, $a0, %lo(strings), in the first function.
        let hi = (strings_at.wrapping_add(0x8000)) >> 16;
        let lo = strings_at & 0xFFFF;
        words[2] = 0x3C04_0000 | hi;
        words[3] = 0x2484_0000 | lo;
        let mut file = vec![0x55u8; SECTOR * 2];
        let at = file.len();
        file.extend(le(&words));
        file.resize(at + code_len, 0);
        let table: Vec<u32> = (0..64).map(|i| base + 16 * i).collect();
        file.extend(le(&table));
        file.resize(file.len() + SECTOR - 4 * table.len(), 0);
        file.extend((0..SECTOR / 4).flat_map(|i| (i as u32 * 7919).to_le_bytes()));
        file.extend(b"Hello, world\0".iter().cycle().take(SECTOR));
        file.extend(vec![0xAAu8; SECTOR * 2]);
        let blobs = find_code_blobs(&file, None);
        assert_eq!(blobs.len(), 1, "{blobs:?}");
        assert_eq!(blobs[0].bases[0].address, base as u64, "{blobs:?}");
        assert_eq!(blobs[0].data, 3 * SECTOR as u64, "{blobs:?}");
        assert_eq!(blobs[0].size, (code_len + 3 * SECTOR) as u64);
        assert!(blobs_text(&blobs).contains("then 6144 bytes of its data"));
    }

    #[test]
    fn overlays_stored_one_after_another_are_told_apart() {
        let (first, second) = (0x8010_0000u32, 0x8014_0000u32);
        let a = le(&functions_at(first));
        let b = le(&functions_at(second));
        let mut file = vec![0x11u8; SECTOR];
        let at = file.len();
        file.extend(&a);
        file.resize(at + a.len().div_ceil(SECTOR) * SECTOR, 0);
        let at_b = file.len();
        file.extend(&b);
        file.resize(at_b + b.len().div_ceil(SECTOR) * SECTOR, 0);
        file.extend(vec![0x22u8; SECTOR]);
        let blobs = find_code_blobs(&file, None);
        assert_eq!(blobs.len(), 2, "{}", blobs_text(&blobs));
        assert_eq!(
            (blobs[0].offset, blobs[0].bases[0].address),
            (at as u64, first as u64),
            "{}",
            blobs_text(&blobs)
        );
        assert_eq!(
            (blobs[1].offset, blobs[1].bases[0].address),
            (at_b as u64, second as u64),
            "{}",
            blobs_text(&blobs)
        );
        assert!(blobs.iter().all(|b| b.confident() && b.split_from == Some(at as u64)));
        assert!(blobs_text(&blobs).contains("stored together"));
    }
}
