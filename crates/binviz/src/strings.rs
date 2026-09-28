//! Printable strings in the loaded data sections, like `strings(1)`: ASCII
//! runs and UTF-16LE runs (common in Windows binaries and resources).
//!
//! The index is compact — 16 bytes per string, pointing into the file — so the
//! millions of strings of a large app cost tens of megabytes; text is read from
//! the file when shown, and searches scan the section bytes directly.

use std::sync::Mutex;

use serde::Serialize;

use crate::binary::Binary;
use crate::model::{Format, RegionKind};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundString {
    pub offset: u64,
    pub address: Option<u64>,
    /// Bytes the string occupies in the file (UTF-16 counts two per character).
    pub size: u32,
    /// UTF-16LE rather than ASCII.
    pub wide: bool,
    pub text: String,
    pub section: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StringPage {
    pub total: u32,
    pub offset: u32,
    pub strings: Vec<FoundString>,
}

/// A string in the file: where it is, how long, and whether it is UTF-16.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StrRec {
    pub offset: u64,
    /// Length in bytes; the top bit marks UTF-16LE.
    len_wide: u32,
    pub section: u32,
}

impl StrRec {
    pub fn len(&self) -> u32 {
        self.len_wide & 0x7fff_ffff
    }

    pub fn wide(&self) -> bool {
        self.len_wide >> 31 != 0
    }

    pub fn end(&self) -> u64 {
        self.offset + self.len() as u64
    }
}

pub(crate) struct StringIndex {
    /// Sorted by offset.
    pub recs: Vec<StrRec>,
    /// File ranges that were scanned (the data sections).
    ranges: Vec<(u64, u64)>,
    /// The last filter's matches, in file order, so paging doesn't rescan.
    filtered: Mutex<Option<(String, Vec<u32>)>>,
}

const MIN_CHARS: usize = 4;
const MAX_STRINGS: usize = 50_000_000;
const MAX_TEXT: usize = 400;

fn printable(b: u8) -> bool {
    (0x20..0x7f).contains(&b) || b == b'\t'
}

impl Binary {
    /// The string index (built once).
    pub(crate) fn string_index(&self) -> &StringIndex {
        self.strings.get_or_init(|| self.scan_strings())
    }

    /// Text of a string, read from the file (at most `MAX_TEXT` characters).
    pub(crate) fn string_text(&self, r: &StrRec) -> String {
        let bytes = &self.data[r.offset as usize..r.end() as usize];
        if r.wide() {
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .take(MAX_TEXT)
                .map(|c| c[0] as char)
                .collect()
        } else {
            // Printable ASCII: always valid UTF-8.
            String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_TEXT)]).into_owned()
        }
    }

    /// The text of a string starting at `address` (possibly inside a longer
    /// one), read straight from the file: at least 2 printable characters ending
    /// in NUL, or 4 without one (not every language NUL-terminates). ASCII or
    /// UTF-16LE; at most `max` characters, and `None` for anything else.
    pub(crate) fn string_preview(&self, address: u64, max: usize) -> Option<String> {
        let sec = self.section_at(address)?;
        if !matches!(sec.kind, RegionKind::Rodata | RegionKind::Data | RegionKind::Tls) {
            return None;
        }
        let off = self.address_to_offset(address)? as usize;
        let end = (sec.file_offset? + sec.file_size.min(sec.size)) as usize;
        let bytes = self.data.get(off..end.min(off + 2 * max + 2).min(self.data.len()))?;
        let text_byte = |b: u8| printable(b) || b == b'\n' || b == b'\r';
        let (text, terminated): (String, bool) = if bytes.len() >= 4 && bytes[1] == 0 && text_byte(bytes[0]) {
            let units = bytes.as_chunks::<2>().0;
            let n = units.iter().take_while(|u| u[1] == 0 && text_byte(u[0])).count();
            let terminated = units.get(n).is_some_and(|u| *u == [0, 0]);
            (units[..n.min(max)].iter().map(|u| u[0] as char).collect(), terminated)
        } else {
            let n = bytes.iter().take_while(|&&b| text_byte(b)).count();
            let terminated = bytes.get(n) == Some(&0);
            (String::from_utf8_lossy(&bytes[..n.min(max)]).into_owned(), terminated)
        };
        let len = text.chars().count();
        if len < 4 && !(len >= 2 && terminated) {
            return None;
        }
        // A pointer's low bytes can look like a short string (0x100003f20 is " ?").
        if !crate::pointers::stringy(&sec.name) && self.holds_pointer(address) {
            return None;
        }
        Some(text)
    }

    pub(crate) fn string_address(&self, r: &StrRec) -> Option<u64> {
        let s = self.sections.get(r.section as usize)?;
        let off = s.file_offset?;
        s.loaded.then(|| s.address + (r.offset - off))
    }

    pub(crate) fn found_string(&self, r: &StrRec) -> FoundString {
        FoundString {
            offset: r.offset,
            address: self.string_address(r),
            size: r.len(),
            wide: r.wide(),
            text: self.string_text(r),
            section: Some(r.section),
        }
    }

    /// A page of strings containing `filter` (ASCII case-insensitive), in file order.
    pub fn strings(&self, filter: &str, offset: u32, limit: u32) -> StringPage {
        let index = self.string_index();
        let limit = if limit == 0 { 200 } else { limit } as usize;
        let page = |ids: &mut dyn Iterator<Item = u32>| -> Vec<FoundString> {
            ids.skip(offset as usize)
                .take(limit)
                .map(|i| self.found_string(&index.recs[i as usize]))
                .collect()
        };
        let needle = filter.to_ascii_lowercase();
        if needle.is_empty() {
            return StringPage {
                total: index.recs.len() as u32,
                offset,
                strings: page(&mut (0..index.recs.len() as u32)),
            };
        }
        let mut cache = index.filtered.lock().unwrap();
        if cache.as_ref().is_none_or(|(k, _)| *k != needle) {
            let mut ids: Vec<u32> = self.string_matches(&needle).into_iter().map(|(i, _)| i).collect();
            ids.sort_unstable();
            *cache = Some((needle, ids));
        }
        let ids = &cache.as_ref().expect("just filled").1;
        StringPage {
            total: ids.len() as u32,
            offset,
            strings: page(&mut ids.iter().copied()),
        }
    }

    /// Every string containing `needle` (lowercase), with a match score like
    /// symbol names get: exact > prefix > at a word boundary > anywhere.
    pub(crate) fn string_matches(&self, needle: &str) -> Vec<(u32, i32)> {
        let index = self.string_index();
        let n = needle.as_bytes();
        let mut out: Vec<(u32, i32)> = Vec::new();
        if n.is_empty() {
            return out;
        }
        let recs = &index.recs;
        // ASCII strings: scan the section bytes.
        for &(lo, hi) in &index.ranges {
            let data = &self.data[lo as usize..hi as usize];
            let mut pos = 0;
            let mut cur: Option<usize> = None;
            while let Some(p) = crate::search::find_ci(data, n, pos) {
                let at = lo + p as u64;
                let owner = cur.filter(|&i| recs[i].offset <= at && at < recs[i].end()).or_else(|| {
                    let i = recs.partition_point(|r| r.offset <= at).checked_sub(1)?;
                    (at < recs[i].end() && !recs[i].wide()).then_some(i)
                });
                let Some(i) = owner else {
                    pos = p + 1;
                    continue;
                };
                cur = Some(i);
                let r = recs[i];
                if at + n.len() as u64 > r.end() {
                    pos = p + 1;
                    continue;
                }
                let start = (r.offset - lo) as usize;
                let class = match_class(data, start, start + r.len() as usize, p, n.len());
                let score = class - ((r.len() as usize - n.len()) / 2).min(99) as i32;
                match out.last_mut() {
                    Some(last) if last.0 == i as u32 => last.1 = last.1.max(score),
                    _ => out.push((i as u32, score)),
                }
                pos = if class >= 700 { start + r.len() as usize } else { p + 1 };
            }
        }
        // UTF-16 strings: check each (they are few).
        for (i, r) in recs.iter().enumerate().filter(|(_, r)| r.wide()) {
            let text: Vec<u8> = self.data[r.offset as usize..r.end() as usize]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| c[0])
                .collect();
            if let Some(score) = crate::search::score_bytes(&text, n) {
                out.push((i as u32, score));
            }
        }
        out
    }

    fn scan_strings(&self) -> StringIndex {
        let mut recs = Vec::new();
        let mut ranges = Vec::new();
        for s in &self.sections {
            // Code sections produce mostly noise; tables and debug info are browsed elsewhere.
            let wanted = matches!(
                s.kind,
                RegionKind::Rodata | RegionKind::Data | RegionKind::Tls | RegionKind::Resources | RegionKind::Metadata
            ) || (s.kind == RegionKind::Notes && s.loaded);
            let Some(off) = s.file_offset else { continue };
            if !wanted || s.compressed || s.file_size == 0 {
                continue;
            }
            let end = (off + s.file_size).min(self.data.len() as u64);
            ranges.push((off, end));
            let bytes = &self.data[off as usize..end as usize];
            let mut push = |start: usize, len: usize, wide: bool| {
                if recs.len() < MAX_STRINGS {
                    recs.push(StrRec {
                        offset: off + start as u64,
                        len_wide: (len.min(0x7fff_ffff) as u32) | if wide { 1 << 31 } else { 0 },
                        section: s.index,
                    });
                }
            };
            // ASCII runs.
            let mut i = 0;
            while i < bytes.len() {
                if !printable(bytes[i]) {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < bytes.len() && printable(bytes[i]) {
                    i += 1;
                }
                if i - start >= MIN_CHARS {
                    push(start, i - start, false);
                }
            }
            // UTF-16LE runs: printable low byte, zero high byte. They live in PE
            // resources and data, and in Mach-O `__ustring`; elsewhere the pass
            // would double the scan for nothing.
            let wide = matches!(self.summary.format, Format::Pe | Format::Coff) || s.name == "__ustring";
            for parity in 0..if wide { 2 } else { 0 } {
                let mut i = parity;
                while i + 1 < bytes.len() {
                    if !(printable(bytes[i]) && bytes[i + 1] == 0) {
                        i += 2;
                        continue;
                    }
                    let start = i;
                    while i + 1 < bytes.len() && printable(bytes[i]) && bytes[i + 1] == 0 {
                        i += 2;
                    }
                    if (i - start) / 2 >= MIN_CHARS {
                        push(start, i - start, true);
                    }
                }
            }
        }
        recs.sort_unstable_by_key(|r| r.offset);
        recs.shrink_to_fit();
        ranges.sort_unstable();
        StringIndex {
            recs,
            ranges,
            filtered: Mutex::new(None),
        }
    }
}

/// How good a match at `p` inside `hay[start..end]` is: exact > prefix > after
/// `::` `/` `.` > word start > anywhere.
pub(crate) fn match_class(hay: &[u8], start: usize, end: usize, p: usize, n: usize) -> i32 {
    if end - start == n {
        1000
    } else if p == start {
        800
    } else {
        match hay[p - 1] {
            b':' | b'/' | b'\\' | b'.' => 700,
            c if !c.is_ascii_alphanumeric() => 600,
            c if c.is_ascii_lowercase() && hay[p].is_ascii_uppercase() => 600,
            _ => 400,
        }
    }
}
