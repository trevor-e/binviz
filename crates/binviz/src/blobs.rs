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
    runs.into_iter()
        .map(|(a, b)| {
            let blob = &data[a * SECTOR..b * SECTOR];
            blob_at(blob, (a * SECTOR) as u64, skip.as_ref())
        })
        .collect()
}

/// A blob's shape and load address guesses.
pub fn blob_at(blob: &[u8], offset: u64, skip: Option<&std::ops::Range<u64>>) -> CodeBlob {
    let starts: Vec<u64> = prologues(blob, 0);
    let mut targets: Vec<u64> = Vec::new();
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
            "  @{:#x}..{:#x} ({} bytes, {} functions) {guess}\n",
            b.offset,
            b.offset + b.size,
            b.size,
            b.functions,
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
            words.extend(std::iter::repeat_n(0x2402_0001, (i.wrapping_mul(2_654_435_761) >> 13) as usize % 13));
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
    }
}
