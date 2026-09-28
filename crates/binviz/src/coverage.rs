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
//! | `recovered`  | inside a function found from unwind tables / function starts   |
//! | `padding`    | alignment filler (zeros, int3, nops) between the above         |
//! | `unexplored` | anything else                                                  |

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
}

const STATUSES: usize = 7;

impl MapStatus {
    const ALL: [MapStatus; STATUSES] = [
        MapStatus::Unexplored,
        MapStatus::Padding,
        MapStatus::Recovered,
        MapStatus::Structure,
        MapStatus::Named,
        MapStatus::Annotated,
        MapStatus::Reviewed,
    ];
}

/// Bytes per status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusBytes {
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
    /// Functions recovered from unwind tables / function starts, still unnamed.
    pub recovered: u32,
    /// Functions the user named.
    pub user: u32,
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
    pub functions: FunctionCounts,
    pub annotations: u32,
    pub reviewed: u32,
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
    loop {
        let t = &bytes[..end];
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
                    || s.name.starts_with('$')
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
            for a in &self.annotations {
                let (s, e) = self.annotation_extent(a);
                let status = if a.reviewed {
                    MapStatus::Reviewed
                } else {
                    MapStatus::Annotated
                };
                intervals.push((s, e, status));
            }
            // Strings found by scanning count as recovered data.
            if !code && let Some(off) = sec.file_offset {
                let strings = self.found_strings();
                let first = strings.partition_point(|s| s.offset < off);
                for s in strings[first..].iter().take_while(|s| s.offset < off + sec.file_size) {
                    if s.section == Some(sec.index)
                        && let Some(a) = s.address
                    {
                        intervals.push((a, a + s.size as u64, MapStatus::Recovered));
                    }
                }
            }
            if let Some(off) = sec.file_offset {
                for (s, e) in self.layout.section_structures(sec.index) {
                    let (s, e) = (s.saturating_sub(off), e.saturating_sub(off));
                    intervals.push((sec.address + s, sec.address + e, MapStatus::Structure));
                }
            }
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
        let gap_count = gaps.len() as u32;
        gaps.sort_by_key(|(_, r)| (std::cmp::Reverse(r.end - r.start), r.start));
        gaps.truncate(max_gaps as usize);
        let gaps = gaps.into_iter().map(|(idx, r)| self.describe_gap(idx, r)).collect();

        let mut functions = FunctionCounts::default();
        for f in self.symbols.functions() {
            match f.source {
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
            functions,
            annotations: self.annotations.len() as u32,
            reviewed: self.annotations.iter().filter(|a| a.reviewed).count() as u32,
        }
    }

    fn describe_gap(&self, section: u32, r: Run) -> Gap {
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
            hint: self.gap_hint(sec.kind, bytes).to_string(),
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
