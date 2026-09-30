//! Reverse-engineering coverage: how much of the code and data has been
//! mapped out, and where the unexplored gaps are.
//!
//! Every byte of the loaded code and data sections gets the strongest status
//! that applies to it:
//!
//! | status       | meaning                                                        |
//! |--------------|----------------------------------------------------------------|
//! | `reviewed`   | inside an annotation marked as reviewed                        |
//! | `annotated`  | inside a user annotation (named range or comment)              |
//! | `named`      | inside a symbol from the file or its debug info                |
//! | `structure`  | inside a format structure binviz decodes (import tables...)    |
//! | `recovered`  | inside a function found from unwind tables / function starts,  |
//! |              | or by following the code (32-bit PE, ROMs); strings; data a    |
//! |              | code/data log saw                                              |
//! | `padding`    | alignment filler (zeros, int3, nops) between the above         |
//! | `unexplored` | anything else                                                  |

use std::collections::HashMap;

use object::Architecture;
use serde::Serialize;

use crate::binary::Binary;
use crate::model::{RegionKind, SymbolKind, SymbolSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MapStatus {
    Unexplored,
    Padding,
    Recovered,
    Structure,
    Named,
    Annotated,
    Reviewed,
    /// A function whose decompiled C compiles to these bytes.
    Matched,
}

const STATUSES: usize = 8;

impl MapStatus {
    const ALL: [MapStatus; STATUSES] = [
        MapStatus::Unexplored,
        MapStatus::Padding,
        MapStatus::Recovered,
        MapStatus::Structure,
        MapStatus::Named,
        MapStatus::Annotated,
        MapStatus::Reviewed,
        MapStatus::Matched,
    ];
}

/// Bytes per status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusBytes {
    pub matched: u64,
    pub reviewed: u64,
    pub annotated: u64,
    pub named: u64,
    pub structure: u64,
    pub recovered: u64,
    pub padding: u64,
    pub unexplored: u64,
}

impl StatusBytes {
    fn add(&mut self, status: MapStatus, n: u64) {
        let slot = match status {
            MapStatus::Matched => &mut self.matched,
            MapStatus::Reviewed => &mut self.reviewed,
            MapStatus::Annotated => &mut self.annotated,
            MapStatus::Named => &mut self.named,
            MapStatus::Structure => &mut self.structure,
            MapStatus::Recovered => &mut self.recovered,
            MapStatus::Padding => &mut self.padding,
            MapStatus::Unexplored => &mut self.unexplored,
        };
        *slot += n;
    }

    fn merge(&mut self, o: &StatusBytes) {
        self.matched += o.matched;
        self.reviewed += o.reviewed;
        self.annotated += o.annotated;
        self.named += o.named;
        self.structure += o.structure;
        self.recovered += o.recovered;
        self.padding += o.padding;
        self.unexplored += o.unexplored;
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionCoverage {
    pub section: u32,
    pub name: String,
    pub kind: RegionKind,
    pub address: u64,
    pub size: u64,
    pub bytes: StatusBytes,
}

/// A run of unexplored bytes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gap {
    pub start: u64,
    pub end: u64,
    pub section: u32,
    /// File offset of the first byte, if the gap has file bytes.
    pub offset: Option<u64>,
    /// The closest symbol before the gap, as `name` or `name+0x10`.
    pub after: Option<String>,
    /// A guess at what the bytes are: "code", "text", "pointers", "zeros"...
    pub hint: String,
    /// The first bytes, in hex.
    pub preview: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCounts {
    /// Functions named by the file or its debug info.
    pub named: u32,
    /// Functions recovered from unwind tables / function starts or by following the code, still unnamed.
    pub recovered: u32,
    /// Functions the user named.
    pub user: u32,
    /// Functions an agent named (its notes carry an author), not yet confirmed.
    pub agents: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    /// Loaded code and data sections.
    pub sections: Vec<SectionCoverage>,
    pub totals: StatusBytes,
    /// Largest unexplored gaps first.
    pub gaps: Vec<Gap>,
    pub gap_count: u32,
    /// What the unexplored bytes are, by [`Gap::hint`], most first.
    pub unexplored_by_kind: Vec<(String, u64)>,
    pub functions: FunctionCounts,
    pub annotations: u32,
    pub reviewed: u32,
    /// Notes an agent wrote (carrying an author).
    pub agent_notes: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Run {
    pub start: u64,
    pub end: u64,
    pub status: MapStatus,
}

/// Per content section, its runs in address order.
pub(crate) struct CoverageRuns {
    sections: Vec<(u32, Vec<Run>)>,
}

fn is_content(kind: RegionKind) -> bool {
    matches!(
        kind,
        RegionKind::Code | RegionKind::Rodata | RegionKind::Data | RegionKind::Bss | RegionKind::Tls
    )
}

fn is_x86(arch: Architecture) -> bool {
    matches!(
        arch,
        Architecture::X86_64 | Architecture::X86_64_X32 | Architecture::I386
    )
}

/// Length of the alignment filler at the end of `bytes`: zeros, int3 and the
/// usual single- and multi-byte nop encodings.
pub(crate) fn tail_padding(bytes: &[u8], code: bool, arch: Architecture) -> usize {
    let mut end = bytes.len();
    if !code {
        while end > 0 && bytes[end - 1] == 0 {
            end -= 1;
        }
        return bytes.len() - end;
    }
    if matches!(arch, Architecture::Aarch64 | Architecture::Aarch64_Ilp32) {
        while end >= 4 {
            let w = u32::from_le_bytes([bytes[end - 4], bytes[end - 3], bytes[end - 2], bytes[end - 1]]);
            if w != 0 && w != 0xd503_201f {
                break;
            }
            end -= 4;
        }
        return bytes.len() - end;
    }
    if !is_x86(arch) || bytes.iter().all(|&b| b == 0) {
        while end > 0 && bytes[end - 1] == 0 {
            end -= 1;
        }
        return bytes.len() - end;
    }
    // `nop dword/word [...]` forms, longest first; each may carry 66/2e prefixes.
    const NOPS: [&[u8]; 5] = [
        &[0x0f, 0x1f, 0x84, 0, 0, 0, 0, 0],
        &[0x0f, 0x1f, 0x80, 0, 0, 0, 0],
        &[0x0f, 0x1f, 0x44, 0, 0],
        &[0x0f, 0x1f, 0x40, 0],
        &[0x0f, 0x1f, 0x00],
    ];
    // MSVC's `npad` filler in 32-bit code: `lea r, [r+0]` in its lengths.
    const LEAS: [&[u8]; 4] = [
        &[0x8d, 0xa4, 0x24, 0, 0, 0, 0],
        &[0x8d, 0x9b, 0, 0, 0, 0],
        &[0x8d, 0x64, 0x24, 0],
        &[0x8d, 0x49, 0],
    ];
    let leas: &[&[u8]] = if arch == Architecture::I386 { &LEAS } else { &[] };
    loop {
        let t = &bytes[..end];
        if let Some(p) = leas.iter().find(|p| t.ends_with(p)) {
            end -= p.len();
            continue;
        }
        let multi = NOPS.iter().find(|p| t.ends_with(p)).map(|p| p.len());
        let strip = match multi {
            Some(n) => n,
            // A lone zero is more likely the end of an immediate than filler.
            None => match t.last() {
                Some(0xcc | 0x90) => 1,
                _ => break,
            },
        };
        end -= strip;
        if multi.is_some() || bytes.get(end) == Some(&0x90) {
            let mut prefixes = 0;
            while end > 0 && prefixes < 13 && matches!(bytes[end - 1], 0x66 | 0x2e) {
                end -= 1;
                prefixes += 1;
            }
        }
    }
    bytes.len() - end
}

/// Merges overlapping or touching intervals of the same status: millions of
/// back-to-back functions or strings become a handful of intervals. Each
/// source adds its intervals in address order, so grouping them by status
/// keeps them sorted and the big lists never need sorting.
fn coalesce(intervals: &mut Vec<(u64, u64, MapStatus)>) {
    let mut by_status: [Vec<(u64, u64)>; STATUSES] = Default::default();
    for &(s, e, st) in intervals.iter() {
        by_status[st as usize].push((s, e));
    }
    let mut out: Vec<(u64, u64, MapStatus)> = Vec::new();
    for (k, list) in by_status.iter_mut().enumerate() {
        if !list.is_sorted() {
            list.sort_unstable();
        }
        let first = out.len();
        for &(s, e) in list.iter() {
            let merging = out.len() > first;
            match out.last_mut() {
                Some(last) if merging && s <= last.1 => last.1 = last.1.max(e),
                _ => out.push((s, e, MapStatus::ALL[k])),
            }
        }
    }
    *intervals = out;
}

/// Paints intervals by priority and returns the covered runs, merged, with
/// `None` for uncovered stretches.
fn sweep(lo: u64, hi: u64, intervals: &[(u64, u64, MapStatus)]) -> Vec<(u64, u64, Option<MapStatus>)> {
    let mut events: Vec<(u64, i32, usize)> = Vec::with_capacity(intervals.len() * 2);
    for &(s, e, st) in intervals {
        let (s, e) = (s.max(lo), e.min(hi));
        if s < e {
            events.push((s, 1, st as usize));
            events.push((e, -1, st as usize));
        }
    }
    events.sort_unstable_by_key(|&(p, d, _)| (p, d));
    let mut counts = [0i32; STATUSES];
    let mut out: Vec<(u64, u64, Option<MapStatus>)> = Vec::new();
    let mut push = |s: u64, e: u64, st: Option<MapStatus>| {
        if s >= e {
            return;
        }
        match out.last_mut() {
            Some(last) if last.2 == st && last.1 == s => last.1 = e,
            _ => out.push((s, e, st)),
        }
    };
    let mut pos = lo;
    let mut i = 0;
    while i < events.len() {
        let p = events[i].0;
        let current = (0..STATUSES).rev().find(|&k| counts[k] > 0).map(|k| MapStatus::ALL[k]);
        push(pos, p, current);
        pos = p;
        while i < events.len() && events[i].0 == p {
            counts[events[i].2] += events[i].1;
            i += 1;
        }
    }
    push(pos, hi, None);
    out
}

impl Binary {
    pub(crate) fn coverage_runs(&self) -> &CoverageRuns {
        self.coverage.get_or_init(|| self.compute_coverage())
    }

    fn compute_coverage(&self) -> CoverageRuns {
        let mut sections = Vec::new();
        for sec in &self.sections {
            if !sec.loaded || sec.size == 0 || !is_content(sec.kind) {
                continue;
            }
            let (lo, hi) = (sec.address, sec.address + sec.size);
            // lld pads the RELRO segment to a page with this NOBITS section.
            if sec.name == ".relro_padding" {
                sections.push((
                    sec.index,
                    vec![Run {
                        start: lo,
                        end: hi,
                        status: MapStatus::Padding,
                    }],
                ));
                continue;
            }
            let code = sec.kind == RegionKind::Code;
            let file_bytes = |a: u64, b: u64| -> Option<&[u8]> {
                let off = sec.file_offset?;
                if sec.compressed || b > sec.address + sec.file_size {
                    return None;
                }
                let start = off + (a - sec.address);
                self.data.get(start as usize..(start + (b - a)) as usize)
            };
            let mut intervals: Vec<(u64, u64, MapStatus)> = Vec::new();
            for s in self.symbols.in_range(lo, hi) {
                if s.size == 0
                    || s.name().starts_with('$')
                    || !matches!(s.kind, SymbolKind::Function | SymbolKind::Data | SymbolKind::Unknown)
                {
                    continue;
                }
                let status = match s.source {
                    SymbolSource::User => MapStatus::Annotated,
                    SymbolSource::Discovered => MapStatus::Recovered,
                    _ => MapStatus::Named,
                };
                let mut end = s.address.saturating_add(s.size).min(hi);
                // An inferred size runs up to the next symbol, over the alignment filler.
                if s.size_inferred
                    && code
                    && s.address < end
                    && let Some(bytes) = file_bytes(s.address, end)
                {
                    end -= tail_padding(bytes, true, self.arch) as u64;
                }
                intervals.push((s.address, end, status));
            }
            // The pieces of functions away from their entries are their functions'.
            for &(ps, pe, owner) in self.code_parts.iter().filter(|p| p.0 < hi && p.1 > lo) {
                let status = match self.symbols.at(owner).map(|s| s.source) {
                    Some(SymbolSource::User) => MapStatus::Annotated,
                    Some(SymbolSource::Discovered) | None => MapStatus::Recovered,
                    Some(_) => MapStatus::Named,
                };
                intervals.push((ps.max(lo), pe.min(hi), status));
            }
            for a in self.annotations.iter().filter(|a| a.address < hi) {
                let matched = a.decomp.as_ref().is_some_and(|d| d.state == crate::model::DecompState::Matched);
                if !a.is_note() && !matched {
                    continue;
                }
                let (s, e) = self.annotation_extent(a);
                if e <= lo {
                    continue;
                }
                let status = if matched {
                    MapStatus::Matched
                } else if a.reviewed {
                    MapStatus::Reviewed
                } else {
                    MapStatus::Annotated
                };
                intervals.push((s, e, status));
            }
            // Strings found by scanning count as recovered data.
            if !code && let Some(off) = sec.file_offset {
                let strings = &self.string_index().recs;
                let first = strings.partition_point(|s| s.offset < off);
                for s in strings[first..].iter().take_while(|s| s.offset < off + sec.file_size) {
                    if s.section == sec.index {
                        let a = sec.address + (s.offset - off);
                        // Count the terminator too, so back-to-back strings make one run.
                        let term = if s.wide() { 2 } else { 1 };
                        let end = s.end() as usize;
                        let nul = self
                            .data
                            .get(end..end + term)
                            .is_some_and(|t| t.iter().all(|&b| b == 0));
                        let len = s.len() as u64 + if nul { term as u64 } else { 0 };
                        intervals.push((a, a + len, MapStatus::Recovered));
                    }
                }
            }
            // Data a code/data log saw the game read counts as recovered too.
            if let Some((log, _)) = self.rom.as_ref().and_then(|r| r.log.as_ref())
                && let Some(off) = sec.file_offset
            {
                let mut run = None;
                for i in 0..=sec.file_size {
                    let data = i < sec.file_size && log.at(off + i) & crate::rom::cdl::flag::DATA != 0;
                    match (data, run) {
                        (true, None) => run = Some(i),
                        (false, Some(s)) => {
                            intervals.push((sec.address + s, sec.address + i, MapStatus::Recovered));
                            run = None;
                        }
                        _ => {}
                    }
                }
            }
            if let Some(off) = sec.file_offset {
                for (s, e) in self.layout.section_structures(sec.index) {
                    let (s, e) = (s.saturating_sub(off), e.saturating_sub(off));
                    intervals.push((sec.address + s, sec.address + e, MapStatus::Structure));
                }
            }
            coalesce(&mut intervals);
            let mut runs: Vec<Run> = Vec::new();
            let mut push = |start: u64, end: u64, status: MapStatus| {
                if start >= end {
                    return;
                }
                match runs.last_mut() {
                    Some(last) if last.status == status && last.end == start => last.end = end,
                    _ => runs.push(Run { start, end, status }),
                }
            };
            for (s, e, st) in sweep(lo, hi, &intervals) {
                if let Some(st) = st {
                    push(s, e, st);
                    continue;
                }
                match file_bytes(s, e) {
                    Some(bytes) => {
                        let pad = tail_padding(bytes, code, self.arch) as u64;
                        // In data, only an all-zero gap counts as filler.
                        let pad = if code || pad == e - s { pad } else { 0 };
                        push(s, e - pad, MapStatus::Unexplored);
                        push(e - pad, e, MapStatus::Padding);
                    }
                    None => push(s, e, MapStatus::Unexplored),
                }
            }
            runs.shrink_to_fit();
            sections.push((sec.index, runs));
        }
        CoverageRuns { sections }
    }

    /// Coverage statistics and the largest unexplored gaps.
    pub fn coverage(&self, max_gaps: u32) -> Coverage {
        let runs = self.coverage_runs();
        let mut sections = Vec::new();
        let mut totals = StatusBytes::default();
        let mut gaps: Vec<(u32, Run)> = Vec::new();
        for (idx, list) in &runs.sections {
            let sec = &self.sections[*idx as usize];
            let mut bytes = StatusBytes::default();
            for r in list {
                bytes.add(r.status, r.end - r.start);
                if r.status == MapStatus::Unexplored {
                    gaps.push((*idx, *r));
                }
            }
            totals.merge(&bytes);
            sections.push(SectionCoverage {
                section: *idx,
                name: sec.name.clone(),
                kind: sec.kind,
                address: sec.address,
                size: sec.size,
                bytes,
            });
        }
        // A gap is named for what it holds, so one that holds several things is several gaps.
        let mut budget = 64u64 << 20;
        let mut gaps: Vec<(u32, Run, &'static str)> = gaps
            .into_iter()
            .flat_map(|(idx, r)| self.split_gap(idx, r, &mut budget))
            .collect();
        let gap_count = gaps.len() as u32;
        let mut kinds: HashMap<&str, u64> = HashMap::new();
        for (_, r, hint) in &gaps {
            *kinds.entry(hint).or_default() += r.end - r.start;
        }
        let mut unexplored_by_kind: Vec<(String, u64)> = kinds.into_iter().map(|(k, n)| (k.to_string(), n)).collect();
        unexplored_by_kind.sort_by_key(|(k, n)| (std::cmp::Reverse(*n), k.clone()));
        gaps.sort_by_key(|(_, r, _)| (std::cmp::Reverse(r.end - r.start), r.start));
        gaps.truncate(max_gaps as usize);
        let gaps = gaps
            .into_iter()
            .map(|(idx, r, hint)| self.describe_gap(idx, r, hint))
            .collect();

        let mut functions = FunctionCounts::default();
        let by_agents: std::collections::HashSet<u64> = self
            .annotations
            .iter()
            .filter(|a| !a.author.is_empty() && !a.name.is_empty())
            .map(|a| a.address)
            .collect();
        for f in self.symbols.functions() {
            match f.source {
                SymbolSource::User if by_agents.contains(&f.address) => functions.agents += 1,
                SymbolSource::User => functions.user += 1,
                SymbolSource::Discovered => functions.recovered += 1,
                _ => functions.named += 1,
            }
        }
        Coverage {
            sections,
            totals,
            gaps,
            gap_count,
            unexplored_by_kind,
            functions,
            annotations: self.annotations.iter().filter(|a| a.is_note()).count() as u32,
            reviewed: self.annotations.iter().filter(|a| a.reviewed).count() as u32,
            agent_notes: self.annotations.iter().filter(|a| !a.author.is_empty()).count() as u32,
        }
    }

    /// The file bytes of an unexplored run (fewer than its length if it runs off the file's end).
    fn gap_bytes(&self, section: u32, start: u64, end: u64) -> &[u8] {
        let sec = &self.sections[section as usize];
        let Some(off) = sec.file_offset.filter(|_| !sec.compressed) else {
            return &[];
        };
        let from = start - sec.address;
        let n = (end - start).min(sec.file_size.saturating_sub(from));
        self.data.get((off + from) as usize..(off + from + n) as usize).unwrap_or(&[])
    }

    /// An unexplored run, cut where what its bytes are changes (zeros, then a table, then code).
    /// `budget` bounds the bytes looked at, so a huge binary stays quick.
    fn split_gap(&self, section: u32, r: Run, budget: &mut u64) -> Vec<(u32, Run, &'static str)> {
        const WINDOW: u64 = 4096;
        let kind = self.sections[section as usize].kind;
        let len = r.end - r.start;
        if len < 2 * WINDOW || len > *budget {
            let hint = self.gap_hint(kind, self.gap_bytes(section, r.start, r.end.min(r.start + WINDOW)));
            return vec![(section, r, hint)];
        }
        *budget -= len;
        let mut out: Vec<(u32, Run, &'static str)> = Vec::new();
        let mut at = r.start;
        while at < r.end {
            let to = (at + WINDOW).min(r.end);
            let hint = self.gap_hint(kind, self.gap_bytes(section, at, to));
            match out.last_mut() {
                Some(last) if last.2 == hint => last.1.end = to,
                _ => out.push((section, Run { start: at, end: to, status: r.status }, hint)),
            }
            at = to;
        }
        out
    }

    fn describe_gap(&self, section: u32, r: Run, hint: &'static str) -> Gap {
        let sec = &self.sections[section as usize];
        let offset = sec
            .file_offset
            .filter(|_| r.start - sec.address < sec.file_size && !sec.compressed)
            .map(|o| o + (r.start - sec.address));
        let after = r
            .start
            .checked_sub(1)
            .and_then(|a| self.symbols.before(a))
            .filter(|s| s.address >= sec.address)
            .map(|s| {
                let d = r.start - s.address;
                if d == 0 {
                    s.display_name().to_string()
                } else {
                    format!("{}+{d:#x}", s.display_name())
                }
            });
        let bytes = offset
            .and_then(|o| {
                let n = (r.end - r.start).min(sec.file_size - (r.start - sec.address)).min(4096);
                self.data.get(o as usize..(o + n) as usize)
            })
            .unwrap_or(&[]);
        let preview = bytes
            .iter()
            .take(16)
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        Gap {
            start: r.start,
            end: r.end,
            section,
            offset,
            after,
            hint: hint.to_string(),
            preview,
        }
    }

    /// A guess at what a run of bytes holds.
    fn gap_hint(&self, kind: RegionKind, bytes: &[u8]) -> &'static str {
        if bytes.is_empty() {
            return "uninitialized";
        }
        let n = bytes.len();
        let zeros = bytes.iter().filter(|&&b| b == 0).count();
        if zeros * 10 >= n * 9 {
            return "mostly zeros";
        }
        let printable = bytes
            .iter()
            .filter(|&&b| (0x20..0x7f).contains(&b) || matches!(b, b'\n' | b'\r' | b'\t' | 0))
            .count();
        if printable * 10 >= n * 9 && zeros * 2 < n {
            return "text";
        }
        let width = if self.is64 { 8 } else { 4 };
        if n >= width * 2 {
            let words = n / width;
            let pointers = (0..words)
                .filter(|i| {
                    let w = &bytes[i * width..(i + 1) * width];
                    let v = match (width, self.endian) {
                        (8, crate::util::Endian::Little) => u64::from_le_bytes(w.try_into().unwrap()),
                        (8, crate::util::Endian::Big) => u64::from_be_bytes(w.try_into().unwrap()),
                        (_, crate::util::Endian::Little) => u32::from_le_bytes(w.try_into().unwrap()) as u64,
                        (_, crate::util::Endian::Big) => u32::from_be_bytes(w.try_into().unwrap()) as u64,
                    };
                    v != 0 && self.segment_at(v).is_some()
                })
                .count();
            if pointers * 2 >= words {
                return "pointers";
            }
        }
        if kind == RegionKind::Code {
            // Machine code returns often; a MIPS run with fewer than one `jr $ra` in 2 KiB is data.
            if let Some(rom) = &self.rom
                && matches!(rom.cpu, crate::cpu::Cpu::MipsR3000 | crate::cpu::Cpu::MipsR4300)
                && n >= 512
            {
                let returns = bytes
                    .chunks_exact(4)
                    .filter(|w| {
                        let w = [w[0], w[1], w[2], w[3]];
                        let v = if self.endian == crate::util::Endian::Little {
                            u32::from_le_bytes(w)
                        } else {
                            u32::from_be_bytes(w)
                        };
                        v == 0x03E0_0008
                    })
                    .count();
                if returns * 2048 < n {
                    return "data";
                }
            }
            return "code";
        }
        if n >= 256 && crate::binary::entropy(bytes) > 7.2 {
            return "high entropy";
        }
        "data"
    }

    /// Dominant status of each of `buckets` equal slices of a section.
    pub fn coverage_strip(&self, section: u32, buckets: u32) -> Vec<MapStatus> {
        let runs = self.coverage_runs();
        let Some((_, list)) = runs.sections.iter().find(|(i, _)| *i == section) else {
            return Vec::new();
        };
        let sec = &self.sections[section as usize];
        let spans: Vec<(u64, u64, MapStatus)> = list
            .iter()
            .map(|r| (r.start - sec.address, r.end - sec.address, r.status))
            .collect();
        bucketize(&spans, sec.size, buckets)
    }

    /// Dominant status of each of `buckets` equal slices of the whole file.
    /// Bytes outside code and data sections are `structure` (headers, tables,
    /// debug info...), `padding`, or `unexplored` when nothing claims them.
    pub fn coverage_map(&self, buckets: u32) -> Vec<MapStatus> {
        let runs = self.coverage_runs();
        let mut content: Vec<(u64, u64)> = Vec::new();
        let mut spans: Vec<(u64, u64, MapStatus)> = Vec::new();
        for (idx, list) in &runs.sections {
            let sec = &self.sections[*idx as usize];
            let Some(off) = sec.file_offset.filter(|_| !sec.compressed) else {
                continue;
            };
            let backed = sec.file_size.min(sec.size);
            content.push((off, off + backed));
            for r in list {
                let (s, e) = (r.start - sec.address, (r.end - sec.address).min(backed));
                if s < e {
                    spans.push((off + s, off + e, r.status));
                }
            }
        }
        content.sort_unstable();
        for (s, e, kind) in self.layout.leaves() {
            let status = match kind {
                RegionKind::Padding => MapStatus::Padding,
                RegionKind::Unknown | RegionKind::Overlay => MapStatus::Unexplored,
                _ => MapStatus::Structure,
            };
            // The part of the leaf outside content sections.
            let mut cursor = s;
            let first = content.partition_point(|&(_, ce)| ce <= s);
            for &(cs, ce) in &content[first..] {
                if cs >= e {
                    break;
                }
                if cs > cursor {
                    spans.push((cursor, cs, status));
                }
                cursor = cursor.max(ce);
            }
            if cursor < e {
                spans.push((cursor, e, status));
            }
        }
        spans.sort_unstable_by_key(|&(s, _, _)| s);
        bucketize(&spans, self.data.len() as u64, buckets)
    }
}

/// Dominant status per bucket of `0..size`; `spans` are sorted by start.
fn bucketize(spans: &[(u64, u64, MapStatus)], size: u64, buckets: u32) -> Vec<MapStatus> {
    // Never more buckets than bytes: an empty bucket has no status.
    let buckets = (buckets.clamp(1, 1 << 16) as u64).min(size.max(1)) as usize;
    let mut weights = vec![[0u64; STATUSES]; buckets];
    let per = (size as f64 / buckets as f64).max(1.0);
    for &(start, end, status) in spans {
        let mut s = start;
        while s < end {
            let b = ((s as f64 / per) as usize).min(buckets - 1);
            let bucket_end = (((b + 1) as f64 * per) as u64).max(s + 1);
            let e = if b == buckets - 1 { end } else { end.min(bucket_end) };
            weights[b][status as usize] += e - s;
            s = e;
        }
    }
    weights
        .into_iter()
        .map(|w| {
            // Ties go to the stronger status.
            let (k, n) = w.iter().enumerate().max_by_key(|&(k, &n)| (n, k)).expect("non-empty");
            if *n == 0 {
                MapStatus::Unexplored
            } else {
                MapStatus::ALL[k]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x86_padding_forms() {
        let arch = Architecture::X86_64;
        assert_eq!(tail_padding(&[0xc3, 0xcc, 0xcc, 0xcc], true, arch), 3);
        assert_eq!(
            tail_padding(&[0xc3, 0x66, 0x2e, 0x0f, 0x1f, 0x84, 0, 0, 0, 0, 0], true, arch),
            10
        );
        assert_eq!(tail_padding(&[0xc3, 0x0f, 0x1f, 0x40, 0x00, 0x66, 0x90], true, arch), 6);
        assert_eq!(tail_padding(&[0xc3], true, arch), 0);
        // `mov dword [rbp-4], 0xff` ends in zeros that aren't filler.
        assert_eq!(tail_padding(&[0xc7, 0x45, 0xfc, 0xff, 0, 0, 0], true, arch), 0);
        assert_eq!(tail_padding(&[0, 0, 0, 0], true, arch), 4);
        // MSVC's lea forms, in 32-bit code only.
        let msvc = [0xc3, 0x8d, 0xa4, 0x24, 0, 0, 0, 0, 0x8d, 0x64, 0x24, 0];
        assert_eq!(tail_padding(&msvc, true, Architecture::I386), 11);
        assert_eq!(tail_padding(&msvc, true, arch), 0);
        assert_eq!(tail_padding(&[0x01, 0x00, 0x00], false, arch), 2);
        let a64 = Architecture::Aarch64;
        assert_eq!(
            tail_padding(&[0xc0, 0x03, 0x5f, 0xd6, 0x1f, 0x20, 0x03, 0xd5], true, a64),
            4
        );
    }

    #[test]
    fn sweep_prefers_stronger_status() {
        let runs = sweep(
            0,
            100,
            &[
                (10, 50, MapStatus::Named),
                (20, 30, MapStatus::Reviewed),
                (40, 60, MapStatus::Recovered),
            ],
        );
        let got: Vec<_> = runs.iter().map(|&(s, e, st)| (s, e, st)).collect();
        assert_eq!(
            got,
            vec![
                (0, 10, None),
                (10, 20, Some(MapStatus::Named)),
                (20, 30, Some(MapStatus::Reviewed)),
                (30, 50, Some(MapStatus::Named)),
                (50, 60, Some(MapStatus::Recovered)),
                (60, 100, None),
            ]
        );
    }
}
