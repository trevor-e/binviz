//! One search box for everything in a binary: addresses and file offsets,
//! symbols, imports and exports, sections, source files and lines, DWARF
//! names, the user's notes, strings, and byte patterns.
//!
//! Query forms:
//!
//! | query                 | finds                                                  |
//! |-----------------------|--------------------------------------------------------|
//! | `0x401000`, `401000`  | that address and/or file offset (hex), plus text hits  |
//! | `@0x200`              | a file offset only                                     |
//! | `main+0x10`           | an address relative to a symbol                        |
//! | `48 8b ?? 08`         | a byte pattern (`??` matches any byte)                 |
//! | `"text"`              | exact ASCII / UTF-16 text anywhere in the file         |
//! | `file.c:42`           | the code generated for a source line                   |
//! | anything else         | names containing the text, best matches first          |

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::binary::Binary;
use crate::model::{SymbolKind, SymbolSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HitKind {
    Address,
    Offset,
    Symbol,
    Import,
    Export,
    Section,
    Source,
    Dwarf,
    Note,
    String,
    Bytes,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub kind: HitKind,
    pub label: String,
    pub detail: String,
    pub address: Option<u64>,
    pub offset: Option<u64>,
    pub size: Option<u64>,
    pub symbol: Option<u32>,
    pub section: Option<u32>,
    /// Source file id.
    pub file: Option<u32>,
    pub line: Option<u32>,
    pub unit: Option<u32>,
    /// Unit-relative DIE offset.
    pub die: Option<u64>,
    pub score: i32,
}

impl SearchHit {
    fn new(kind: HitKind, label: impl Into<String>, detail: impl Into<String>, score: i32) -> SearchHit {
        SearchHit {
            kind,
            label: label.into(),
            detail: detail.into(),
            address: None,
            offset: None,
            size: None,
            symbol: None,
            section: None,
            file: None,
            line: None,
            unit: None,
            die: None,
            score,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindCount {
    pub kind: HitKind,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub query: String,
    /// Grouped by kind, the group with the best hit first; best hit first
    /// within a group.
    pub hits: Vec<SearchHit>,
    /// Matches per kind (a group returns at most `per_kind` hits).
    pub counts: Vec<KindCount>,
}

/// Byte pattern matches stop counting here.
pub const MAX_BYTE_MATCHES: u32 = 10_000;

/// Parses `0x1f`, `1f` (hex, needs a digit), WinDbg's `0000`1f00` and `1f_00`.
pub fn parse_number(s: &str) -> Option<u64> {
    let s = s.trim();
    let (digits, explicit) = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => (h, true),
        None => (s, false),
    };
    let clean: String = digits.chars().filter(|&c| c != '`' && c != '_').collect();
    if clean.is_empty() || clean.len() > 16 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    if !explicit && !clean.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    u64::from_str_radix(&clean, 16).ok()
}

/// `48 8b ?? 08` or `488b??08`: two hex digits or `??` per byte.
fn parse_pattern(s: &str) -> Option<Vec<Option<u8>>> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    let pairs: Vec<String> = if tokens.len() >= 2 {
        tokens.iter().map(|t| t.to_string()).collect()
    } else if s.contains('?') {
        let compact: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
        if !compact.len().is_multiple_of(2) {
            return None;
        }
        compact.chunks(2).map(|c| c.iter().collect()).collect()
    } else {
        return None;
    };
    let mut out = Vec::with_capacity(pairs.len());
    for p in pairs {
        match p.as_str() {
            "??" | "?" => out.push(None),
            _ if p.len() == 2 => out.push(Some(u8::from_str_radix(&p, 16).ok()?)),
            _ => return None,
        }
    }
    out.iter().any(Option::is_some).then_some(out)
}

/// How well `needle` (lowercase) matches `hay`, ignoring ASCII case: exact >
/// prefix > after `::` `/` `.` > word start > anywhere, shorter names first.
pub(crate) fn score(hay: &str, needle: &str) -> Option<i32> {
    score_bytes(hay.as_bytes(), needle.as_bytes())
}

/// [`score`] on bytes.
pub(crate) fn score_bytes(h: &[u8], n: &[u8]) -> Option<i32> {
    if n.is_empty() || n.len() > h.len() {
        return None;
    }
    let first = n[0];
    let mut best: Option<i32> = None;
    for i in 0..=h.len() - n.len() {
        if h[i].to_ascii_lowercase() != first || !h[i..i + n.len()].eq_ignore_ascii_case(n) {
            continue;
        }
        let class = if h.len() == n.len() {
            1000
        } else if i == 0 {
            800
        } else {
            match h[i - 1] {
                b':' | b'/' | b'\\' | b'.' => 700,
                c if !c.is_ascii_alphanumeric() => 600,
                c if c.is_ascii_lowercase() && h[i].is_ascii_uppercase() => 600,
                _ => 400,
            }
        };
        best = best.max(Some(class));
        if class >= 700 {
            break;
        }
    }
    let penalty = ((h.len() - n.len()) / 2).min(99) as i32;
    best.map(|c| c - penalty)
}

/// Keeps the `n` best of `(score, index)` pairs, best first (ties by index).
fn top(mut v: Vec<(i32, usize)>, n: usize) -> Vec<(i32, usize)> {
    let cmp = |a: &(i32, usize), b: &(i32, usize)| b.0.cmp(&a.0).then(a.1.cmp(&b.1));
    if v.len() > n && n > 0 {
        v.select_nth_unstable_by(n - 1, cmp);
        v.truncate(n);
    }
    v.sort_unstable_by(cmp);
    v
}

fn tag_label(tag: gimli::DwTag) -> String {
    tag.static_string().map_or_else(
        || format!("{:#x}", tag.0),
        |s| s.trim_start_matches("DW_TAG_").replace('_', " "),
    )
}

fn human_size(n: u64) -> String {
    match n {
        0..1024 => format!("{n} B"),
        1024..1_048_576 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}

struct Collector {
    per_kind: usize,
    only: Option<HitKind>,
    hits: Vec<SearchHit>,
    counts: Vec<KindCount>,
}

impl Collector {
    fn wants(&self, kind: HitKind) -> bool {
        self.only.is_none_or(|k| k == kind)
    }

    fn add(&mut self, kind: HitKind, count: usize, hits: Vec<SearchHit>) {
        if count == 0 {
            return;
        }
        match self.counts.iter_mut().find(|k| k.kind == kind) {
            Some(k) => k.count += count as u32,
            None => self.counts.push(KindCount {
                kind,
                count: count as u32,
            }),
        }
        let room = self.per_kind - self.hits.iter().filter(|h| h.kind == kind).count().min(self.per_kind);
        self.hits.extend(hits.into_iter().take(room));
    }
}

impl Binary {
    /// Builds the indexes search uses (strings, DWARF names) ahead of time.
    pub fn prepare_search(&self) {
        self.symbols.prepare_names();
        self.string_index();
        if let Some(d) = &self.debug {
            d.name_index();
        }
    }

    /// Searches everything at once; see the module docs for query forms.
    /// `per_kind` caps the hits returned per kind; `only` restricts the search
    /// to one kind (for "show all" lists).
    pub fn search(&self, query: &str, per_kind: u32, only: Option<HitKind>) -> SearchResults {
        let q = query.trim();
        let mut c = Collector {
            per_kind: per_kind.max(1) as usize,
            only,
            hits: Vec::new(),
            counts: Vec::new(),
        };
        if !q.is_empty() {
            self.search_into(q, &mut c);
        }
        // Order groups by their best hit; hits stay in score order within a group.
        let mut best: Vec<(HitKind, i32)> = Vec::new();
        for h in &c.hits {
            match best.iter_mut().find(|(k, _)| *k == h.kind) {
                Some(b) => b.1 = b.1.max(h.score),
                None => best.push((h.kind, h.score)),
            }
        }
        let rank = |k: HitKind| {
            let s = best.iter().find(|(b, _)| *b == k).map_or(i32::MIN, |b| b.1);
            (std::cmp::Reverse(s), k)
        };
        c.hits.sort_by_key(|h| rank(h.kind));
        c.counts.sort_by_key(|k| rank(k.kind));
        SearchResults {
            query: q.to_string(),
            hits: c.hits,
            counts: c.counts,
        }
    }

    fn search_into(&self, q: &str, c: &mut Collector) {
        if let Some(rest) = q.strip_prefix('@') {
            if let Some(off) = parse_number(rest)
                && c.wants(HitKind::Offset)
                && off < self.data.len() as u64
            {
                c.add(HitKind::Offset, 1, vec![self.offset_hit(off, 1900)]);
            }
            return;
        }
        if let Some(pattern) = parse_pattern(q) {
            if c.wants(HitKind::Bytes) {
                self.search_bytes(&pattern, None, c);
            }
            return;
        }
        if q.len() >= 2
            && let Some(text) = q.strip_prefix('"').map(|t| t.strip_suffix('"').unwrap_or(t))
        {
            if !text.is_empty() {
                if c.wants(HitKind::Bytes) {
                    let ascii: Vec<Option<u8>> = text.bytes().map(Some).collect();
                    self.search_bytes(&ascii, Some(("ASCII", text)), c);
                    let wide: Vec<Option<u8>> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).map(Some).collect();
                    self.search_bytes(&wide, Some(("UTF-16", text)), c);
                }
                if c.wants(HitKind::String) {
                    self.search_strings(&text.to_lowercase(), c);
                }
            }
            return;
        }

        if let Some(n) = parse_number(q) {
            self.search_number(n, c);
        }
        // A ROM's bank:address (`03:C000`).
        if let Some(address) = self.rom_address(q)
            && c.wants(HitKind::Address)
        {
            let mut hit = SearchHit::new(
                HitKind::Address,
                q.to_string(),
                format!("{address:#x} · {}", self.describe_address(address)),
                2500,
            );
            hit.address = Some(address);
            c.add(HitKind::Address, 1, vec![hit]);
        }
        if let Some((name, delta)) = q.rsplit_once('+')
            && let Some(delta) = parse_number(delta)
            && let Some(sym) = self.symbols.by_name(name.trim())
            && c.wants(HitKind::Address)
        {
            let address = sym.address + delta;
            let mut hit = SearchHit::new(
                HitKind::Address,
                format!("{}+{delta:#x}", sym.display_name()),
                format!("{address:#x} · {}", self.describe_address(address)),
                2000,
            );
            hit.address = Some(address);
            hit.symbol = Some(sym.index);
            c.add(HitKind::Address, 1, vec![hit]);
        }
        if let Some((file, line)) = split_file_line(q)
            && c.wants(HitKind::Source)
        {
            self.search_file_line(file, line, c);
            return;
        }

        let needle = q.to_lowercase();
        // One character matches nearly every name and string: not worth scanning them all.
        let long = needle.len() >= 2;
        if long && c.wants(HitKind::Symbol) {
            self.search_symbols(&needle, c);
        }
        if c.wants(HitKind::Import) {
            self.search_imports(&needle, c);
        }
        if c.wants(HitKind::Export) {
            self.search_exports(&needle, c);
        }
        if c.wants(HitKind::Section) {
            self.search_sections(&needle, c);
        }
        if c.wants(HitKind::Source) {
            self.search_sources(&needle, c);
        }
        if long && c.wants(HitKind::Dwarf) {
            self.search_dies(&needle, c);
        }
        if c.wants(HitKind::Note) {
            self.search_notes(&needle, c);
        }
        if long && c.wants(HitKind::String) {
            self.search_strings(&needle, c);
        }
    }

    /// "section · symbol+off · file:line" for an address.
    pub(crate) fn describe_address(&self, address: u64) -> String {
        self.describe_address_for(address, "")
    }

    /// Like `describe_address`, leaving out the symbol when it is `label` itself.
    fn describe_address_for(&self, address: u64, label: &str) -> String {
        let mut parts = Vec::new();
        if let Some(sec) = self.section_at(address) {
            parts.push(sec.name.clone());
        } else if let Some(seg) = self.segment_at(address) {
            parts.push(seg.name.clone());
        }
        if let Some(sym) = self.symbols.lookup(address) {
            let name = sym.demangled.unwrap_or(sym.name);
            if sym.offset != 0 {
                parts.push(format!("{name}+{:#x}", sym.offset));
            } else if name != label {
                parts.push(name);
            }
        }
        if let Some(loc) = self.debug.as_ref().and_then(|d| d.location(address)) {
            let file = loc.path.rsplit(['/', '\\']).next().unwrap_or(&loc.path).to_string();
            parts.push(format!("{file}:{}", loc.line));
        }
        if parts.is_empty() {
            parts.push("mapped".into());
        }
        parts.join(" · ")
    }

    /// The innermost layout regions at an offset, e.g. "Section .text".
    fn describe_offset_short(&self, offset: u64) -> String {
        let path = self.describe_offset(offset);
        let names: Vec<&str> = path.iter().rev().take(2).map(|p| p.name.as_str()).collect();
        names.into_iter().rev().collect::<Vec<_>>().join(" › ")
    }

    fn offset_hit(&self, offset: u64, score: i32) -> SearchHit {
        let mut detail = self.describe_offset_short(offset);
        let address = self.offset_to_address(offset);
        if let Some(a) = address {
            detail = format!("{detail} · loaded at {a:#x}");
        }
        let mut hit = SearchHit::new(HitKind::Offset, format!("file offset {offset:#x}"), detail, score);
        hit.offset = Some(offset);
        hit.address = address;
        hit.section = self.section_at_offset(offset).map(|s| s.index);
        hit
    }

    fn address_hit(&self, label: String, address: u64, score: i32) -> SearchHit {
        let mut hit = SearchHit::new(HitKind::Address, label, self.describe_address(address), score);
        hit.address = Some(address);
        hit.offset = self.address_to_offset(address);
        hit.section = self.section_at(address).map(|s| s.index);
        hit.symbol = self.symbols.lookup(address).map(|s| s.index);
        hit
    }

    fn search_number(&self, n: u64, c: &mut Collector) {
        let mapped = |a: u64| self.segment_at(a).is_some() || self.section_at(a).is_some();
        if c.wants(HitKind::Address) {
            let mut hits = Vec::new();
            if mapped(n) {
                hits.push(self.address_hit(format!("address {n:#x}"), n, 2000));
            }
            // PE tools print RVAs; accept them when the number isn't an address itself.
            let rva = self.image_base.checked_add(n);
            if self.image_base != 0
                && !mapped(n)
                && let Some(a) = rva.filter(|&a| mapped(a))
            {
                hits.push(self.address_hit(format!("RVA {n:#x} = {a:#x}"), a, 1800));
            }
            let count = hits.len();
            c.add(HitKind::Address, count, hits);
        }
        if c.wants(HitKind::Offset) && n < self.data.len() as u64 {
            c.add(HitKind::Offset, 1, vec![self.offset_hit(n, 1500)]);
        }
    }

    fn search_bytes(&self, pattern: &[Option<u8>], text: Option<(&str, &str)>, c: &mut Collector) {
        // Anchor on the longest run of literal bytes; the rest is checked per candidate.
        let mut anchor = (0, 0);
        let mut run_start = 0;
        for (i, p) in pattern.iter().enumerate() {
            if p.is_none() {
                run_start = i + 1;
            } else if i + 1 - run_start > anchor.1 {
                anchor = (run_start, i + 1 - run_start);
            }
        }
        if anchor.1 == 0 {
            return;
        }
        let literal: Vec<u8> = pattern[anchor.0..anchor.0 + anchor.1]
            .iter()
            .map(|p| p.expect("literal"))
            .collect();
        let data = &self.data[..];
        let len = pattern.len();
        let mut found: Vec<u64> = Vec::new();
        let mut count = 0u32;
        for at in memchr::memmem::find_iter(data, &literal) {
            let Some(start) = at.checked_sub(anchor.0) else {
                continue;
            };
            if start + len > data.len() {
                break;
            }
            let window = &data[start..start + len];
            if pattern.iter().zip(window).all(|(p, b)| p.is_none_or(|p| p == *b)) {
                count += 1;
                if found.len() < c.per_kind {
                    found.push(start as u64);
                }
                if count >= MAX_BYTE_MATCHES {
                    break;
                }
            }
        }
        let hits = found
            .into_iter()
            .map(|off| {
                let bytes = &data[off as usize..off as usize + len.min(16)];
                let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
                let label = match text {
                    Some((_, t)) => format!("\"{t}\""),
                    None => hex.join(" ") + if len > 16 { " …" } else { "" },
                };
                let mut detail = self.describe_offset_short(off);
                if let Some((enc, _)) = text {
                    detail = format!("{enc} · {detail}");
                }
                let mut hit = SearchHit::new(HitKind::Bytes, label, detail, 1000);
                hit.offset = Some(off);
                hit.address = self.offset_to_address(off);
                hit.size = Some(len as u64);
                hit.section = self.section_at_offset(off).map(|s| s.index);
                hit
            })
            .collect();
        c.add(HitKind::Bytes, count as usize, hits);
    }

    fn search_symbols(&self, needle: &str, c: &mut Collector) {
        let table = &self.symbols;
        if needle.is_empty() || table.is_empty() {
            return;
        }
        table.prepare_names();
        let n = needle.as_bytes();
        // Scan every raw name, then every demangled name, each as one buffer.
        let mut raw = Vec::new();
        scan_arena(
            table.name_arena().as_bytes(),
            n,
            |p| table.record_at_name_offset(p).map(|r| (r, table.name_span(r))),
            &mut raw,
        );
        let mut dem = Vec::new();
        scan_arena(
            table.demangled_arena().as_bytes(),
            n,
            |p| {
                table
                    .record_at_demangled_offset(p)
                    .and_then(|r| table.demangled_span(r).map(|s| (r, s)))
            },
            &mut dem,
        );
        let section_names: HashSet<&str> = self.sections.iter().map(|s| s.name.as_str()).collect();
        // Both lists are in record order: merge them. A match on the demangled
        // name beats one on the mangled name.
        let mut matches: Vec<(i32, usize)> = Vec::new();
        let (mut i, mut j) = (0, 0);
        while i < raw.len() || j < dem.len() {
            let (rec, score) = match (raw.get(i), dem.get(j)) {
                (Some(&(r, rs)), Some(&(d, ds))) if r == d => {
                    i += 1;
                    j += 1;
                    (r, ds.max(rs - 50))
                }
                (Some(&(r, rs)), Some(&(d, _))) if r < d => {
                    i += 1;
                    (r, if table.demangled_span(r).is_some() { rs - 50 } else { rs })
                }
                (Some(&(r, rs)), None) => {
                    i += 1;
                    (r, if table.demangled_span(r).is_some() { rs - 50 } else { rs })
                }
                (_, Some(&(d, ds))) => {
                    j += 1;
                    (d, ds)
                }
                (None, None) => break,
            };
            let Some(s) = table.get(rec) else { continue };
            // Section, file and debug symbols (and COFF's per-object `.text`...) are noise here.
            if matches!(s.kind, SymbolKind::Section | SymbolKind::File | SymbolKind::Debug)
                || (s.kind != SymbolKind::Function && section_names.contains(s.name()))
            {
                continue;
            }
            let bonus = match s.kind {
                SymbolKind::Function => 40,
                SymbolKind::Data => 20,
                _ => 0,
            } + if s.source == SymbolSource::User { 50 } else { 0 }
                - if s.defined { 0 } else { 30 };
            matches.push((score + bonus, rec as usize));
        }
        let count = matches.len();
        let mut seen = HashSet::new();
        let hits = top(matches, c.per_kind * 2)
            .into_iter()
            .filter_map(|(score, i)| table.get(i as u32).map(|s| (score, s)))
            .filter(|(_, s)| seen.insert((s.address, s.defined, s.display_name().into_owned())))
            .take(c.per_kind)
            .map(|(score, s)| {
                let mut parts = vec![format!("{:?}", s.kind).to_lowercase()];
                if !s.defined {
                    parts.push("undefined".into());
                }
                if let Some(sec) = s.section.and_then(|i| self.sections.get(i as usize)) {
                    parts.push(sec.name.clone());
                }
                if s.size > 0 {
                    let approx = if s.size_inferred { "~" } else { "" };
                    parts.push(format!("{approx}{}", human_size(s.size)));
                }
                match s.source {
                    SymbolSource::Discovered => parts.push("recovered".into()),
                    SymbolSource::User => parts.push("your name".into()),
                    SymbolSource::Dwarf => parts.push("from DWARF".into()),
                    SymbolSource::Import => parts.push("import stub".into()),
                    SymbolSource::DebugFile => parts.push("from the debug file".into()),
                    SymbolSource::Objc => parts.push("from Objective-C metadata".into()),
                    _ => {}
                }
                if s.demangled().is_some() {
                    parts.push(s.name().to_string());
                }
                let mut hit = SearchHit::new(HitKind::Symbol, s.display_name(), parts.join(" · "), score);
                hit.symbol = Some(s.index);
                hit.address = s.defined.then_some(s.address);
                hit.offset = hit.address.and_then(|a| self.address_to_offset(a));
                hit.size = Some(s.size);
                hit.section = s.section;
                hit
            })
            .collect();
        c.add(HitKind::Symbol, count, hits);
    }

    fn search_imports(&self, needle: &str, c: &mut Collector) {
        let mut matches = Vec::new();
        for (i, imp) in self.imports.iter().enumerate() {
            let name = imp.demangled.as_deref().unwrap_or(&imp.name);
            let s = score(name, needle).or_else(|| score(&imp.library, needle).map(|s| s - 300));
            if let Some(s) = s {
                matches.push((s, i));
            }
        }
        let count = matches.len();
        let hits = top(matches, c.per_kind)
            .into_iter()
            .map(|(score, i)| {
                let imp = &self.imports[i];
                let mut detail = if imp.library.is_empty() {
                    "import".to_string()
                } else {
                    format!("import from {}", imp.library)
                };
                if let Some(o) = imp.ordinal {
                    detail.push_str(&format!(" · ordinal {o}"));
                }
                if imp.address.is_some() {
                    detail.push_str(if self.summary.format == crate::model::Format::Xbe {
                        " · kernel thunk slot"
                    } else {
                        " · IAT slot"
                    });
                }
                let mut hit = SearchHit::new(
                    HitKind::Import,
                    imp.demangled.as_deref().unwrap_or(&imp.name),
                    detail,
                    score,
                );
                hit.address = imp.address;
                hit.offset = imp.address.and_then(|a| self.address_to_offset(a));
                hit
            })
            .collect();
        c.add(HitKind::Import, count, hits);
    }

    fn search_exports(&self, needle: &str, c: &mut Collector) {
        let mut matches = Vec::new();
        for (i, e) in self.exports.iter().enumerate() {
            if let Some(s) = score(e.demangled.as_deref().unwrap_or(&e.name), needle) {
                matches.push((s, i));
            }
        }
        let count = matches.len();
        let hits = top(matches, c.per_kind)
            .into_iter()
            .map(|(score, i)| {
                let e = &self.exports[i];
                let mut detail = "export".to_string();
                if let Some(o) = e.ordinal {
                    detail.push_str(&format!(" · ordinal {o}"));
                }
                if let Some(f) = &e.forwarder {
                    detail.push_str(&format!(" · forwarded to {f}"));
                } else {
                    let name = e.demangled.as_deref().unwrap_or(&e.name);
                    detail.push_str(&format!(" · {}", self.describe_address_for(e.address, name)));
                }
                let mut hit = SearchHit::new(
                    HitKind::Export,
                    e.demangled.as_deref().unwrap_or(&e.name),
                    detail,
                    score,
                );
                if e.forwarder.is_none() {
                    hit.address = Some(e.address);
                    hit.offset = self.address_to_offset(e.address);
                }
                hit
            })
            .collect();
        c.add(HitKind::Export, count, hits);
    }

    fn search_sections(&self, needle: &str, c: &mut Collector) {
        let mut hits = Vec::new();
        for s in &self.sections {
            let full = match &s.segment_name {
                Some(seg) if !seg.is_empty() => format!("{seg},{}", s.name),
                _ => s.name.clone(),
            };
            let Some(sc) = score(&s.name, needle).or_else(|| score(&full, needle)) else {
                continue;
            };
            let kind = format!("{:?}", s.kind).to_lowercase();
            let mut detail = format!("{kind} section · {}", human_size(s.size));
            if !s.perms.is_empty() {
                detail.push_str(&format!(" · {}", s.perms));
            }
            let mut hit = SearchHit::new(HitKind::Section, full, detail, sc + 20);
            hit.section = Some(s.index);
            hit.address = s.loaded.then_some(s.address);
            hit.offset = s.file_offset;
            hit.size = Some(s.size);
            hits.push(hit);
        }
        for seg in &self.segments {
            if seg.name.is_empty() || self.sections.iter().any(|s| s.name == seg.name) {
                continue;
            }
            let Some(sc) = score(&seg.name, needle) else { continue };
            let mut hit = SearchHit::new(
                HitKind::Section,
                seg.name.clone(),
                format!("{} segment · {} · {}", seg.kind, human_size(seg.mem_size), seg.perms),
                sc,
            );
            hit.address = seg.mapped.then_some(seg.address);
            hit.offset = Some(seg.file_offset);
            hit.size = Some(seg.mem_size);
            hits.push(hit);
        }
        hits.sort_by_key(|h| std::cmp::Reverse(h.score));
        let count = hits.len();
        c.add(HitKind::Section, count, hits);
    }

    fn search_sources(&self, needle: &str, c: &mut Collector) {
        let Some(debug) = self.debug.as_ref() else { return };
        let files = debug.source_files();
        let mut matches = Vec::new();
        for (i, f) in files.iter().enumerate() {
            if let Some(s) = score(&f.name, needle).or_else(|| score(&f.path, needle).map(|s| s - 100)) {
                matches.push((s, i));
            }
        }
        let count = matches.len();
        let hits = top(matches, c.per_kind)
            .into_iter()
            .map(|(score, i)| {
                let f = &files[i];
                let mut hit = SearchHit::new(HitKind::Source, f.name.clone(), f.dir.clone(), score);
                hit.file = Some(f.id);
                hit
            })
            .collect();
        c.add(HitKind::Source, count, hits);
    }

    fn search_file_line(&self, file: &str, line: u32, c: &mut Collector) {
        let Some(debug) = self.debug.as_ref() else { return };
        let needle = file.replace('\\', "/").to_lowercase();
        let mut hits = Vec::new();
        let mut count = 0;
        for f in debug.source_files() {
            let path = f.path.replace('\\', "/").to_lowercase();
            if !(path == needle || path.ends_with(&format!("/{needle}")) || f.name.eq_ignore_ascii_case(file)) {
                continue;
            }
            let lines = debug.file_lines(f.id);
            let Some(l) = lines
                .iter()
                .filter(|l| l.line >= line)
                .min_by_key(|l| (l.line, !l.is_stmt, l.start))
            else {
                continue;
            };
            count += 1;
            if hits.len() >= c.per_kind {
                continue;
            }
            let detail = if l.line == line {
                self.describe_address(l.start)
            } else {
                format!(
                    "no code for line {line}; next is {} · {}",
                    l.line,
                    self.describe_address(l.start)
                )
            };
            let mut hit = SearchHit::new(HitKind::Source, format!("{}:{}", f.name, l.line), detail, 1900);
            hit.file = Some(f.id);
            hit.line = Some(l.line);
            hit.address = Some(l.start);
            hit.offset = self.address_to_offset(l.start);
            hits.push(hit);
        }
        c.add(HitKind::Source, count, hits);
    }

    fn search_dies(&self, needle: &str, c: &mut Collector) {
        let Some(debug) = self.debug.as_ref() else { return };
        let index = debug.name_index();
        let mut matches = Vec::new();
        for (i, d) in index.iter().enumerate() {
            if let Some(s) = score(&d.name, needle) {
                let bonus = match d.tag {
                    gimli::DW_TAG_subprogram => 30,
                    gimli::DW_TAG_variable => 20,
                    gimli::DW_TAG_member | gimli::DW_TAG_enumerator => 0,
                    _ => 10,
                };
                matches.push((s + bonus, i));
            }
        }
        let count = matches.len();
        let hits = top(matches, c.per_kind)
            .into_iter()
            .map(|(score, i)| {
                let d = &index[i];
                let mut detail = tag_label(d.tag);
                if let Some(a) = d.address {
                    detail = format!("{detail} · {}", self.describe_address_for(a, &d.name));
                } else if let Some(u) = debug.units().get(d.unit as usize).and_then(|u| u.name.as_deref()) {
                    let base = u.rsplit(['/', '\\']).next().unwrap_or(u);
                    detail = format!("{detail} · {base}");
                }
                let mut hit = SearchHit::new(HitKind::Dwarf, d.name.clone(), detail, score);
                hit.unit = Some(d.unit);
                hit.die = Some(d.offset);
                hit.address = d.address;
                hit.offset = d.address.and_then(|a| self.address_to_offset(a));
                hit
            })
            .collect();
        c.add(HitKind::Dwarf, count, hits);
    }

    fn search_notes(&self, needle: &str, c: &mut Collector) {
        let mut matches = Vec::new();
        for (i, a) in self.annotations.iter().enumerate() {
            // Named annotations are symbols already; notes match on their comments.
            if let Some(s) = score(&a.comment, needle) {
                matches.push((s, i));
            }
        }
        let count = matches.len();
        let hits = top(matches, c.per_kind)
            .into_iter()
            .map(|(score, i)| {
                let a = &self.annotations[i];
                let label = if a.name.is_empty() {
                    format!("note at {:#x}", a.address)
                } else {
                    a.name.clone()
                };
                let comment: String = a.comment.chars().take(160).collect();
                let mut hit = SearchHit::new(HitKind::Note, label, comment, score + 30);
                hit.address = Some(a.address);
                hit.offset = self.address_to_offset(a.address);
                hit
            })
            .collect();
        c.add(HitKind::Note, count, hits);
    }

    fn search_strings(&self, needle: &str, c: &mut Collector) {
        let index = self.string_index();
        let matches: Vec<(i32, usize)> = self
            .string_matches(needle)
            .into_iter()
            .map(|(i, score)| (score, i as usize))
            .collect();
        let count = matches.len();
        let hits = top(matches, c.per_kind)
            .into_iter()
            .map(|(score, i)| {
                let s = self.found_string(&index.recs[i]);
                let mut detail = s
                    .section
                    .and_then(|i| self.sections.get(i as usize))
                    .map_or_else(String::new, |sec| sec.name.clone());
                if s.wide {
                    detail.push_str(" · UTF-16");
                }
                let label: String = s.text.chars().take(160).collect();
                let mut hit = SearchHit::new(HitKind::String, label, detail, score - 100);
                hit.offset = Some(s.offset);
                hit.address = s.address;
                hit.size = Some(s.size as u64);
                hit.section = s.section;
                hit
            })
            .collect();
        c.add(HitKind::String, count, hits);
    }
}

/// Scores the names in `arena` that contain `needle` (lowercase), ignoring
/// ASCII case, like [`score`]. `owner` maps an offset to the record whose name
/// holds it and that name's (start, length). Appends (record, best score) in
/// arena order.
fn scan_arena(
    arena: &[u8],
    needle: &[u8],
    owner: impl Fn(usize) -> Option<(u32, (usize, usize))>,
    out: &mut Vec<(u32, i32)>,
) {
    let mut pos = 0;
    // The name the last match was in: (record, start, length).
    let mut cur: Option<(u32, usize, usize)> = None;
    while let Some(p) = find_ci(arena, needle, pos) {
        let known = cur.filter(|&(_, s, l)| s <= p && p < s + l);
        let Some((rec, start, len)) = known.or_else(|| owner(p).map(|(r, (s, l))| (r, s, l))) else {
            pos = p + 1;
            continue;
        };
        cur = Some((rec, start, len));
        let end = start + len;
        if p + needle.len() > end {
            // The match runs into the next name.
            pos = p + 1;
            continue;
        }
        let class = if len == needle.len() {
            1000
        } else if p == start {
            800
        } else {
            match arena[p - 1] {
                b':' | b'/' | b'\\' | b'.' => 700,
                c if !c.is_ascii_alphanumeric() => 600,
                c if c.is_ascii_lowercase() && arena[p].is_ascii_uppercase() => 600,
                _ => 400,
            }
        };
        let score = class - ((len - needle.len()) / 2).min(99) as i32;
        match out.last_mut() {
            Some(last) if last.0 == rec => last.1 = last.1.max(score),
            _ => out.push((rec, score)),
        }
        // Nothing later in this name can beat a match at a segment boundary.
        pos = if class >= 700 { end } else { p + 1 };
    }
}

/// How common a byte is in names and strings: higher is rarer. Scans look for
/// a query's rarest byte first, so they stop at far fewer false candidates.
fn rarity(b: u8) -> u8 {
    const COMMON: &[u8] = b"etaoinsrhldcumfpgwybvkxjqz";
    match b.to_ascii_lowercase() {
        c @ b'a'..=b'z' => 40 + COMMON.iter().position(|&x| x == c).unwrap_or(0) as u8,
        b'0'..=b'9' | b'_' => 10,
        b' ' | b'.' | b'/' | b':' => 20,
        _ => 100,
    }
}

/// The next position at or after `from` where `needle` (lowercase) occurs in
/// `hay`, ignoring ASCII case. Anchors on the needle's rarest byte.
pub(crate) fn find_ci(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    let n = needle.len();
    if n == 0 || hay.len() < n {
        return None;
    }
    let (a, &anchor) = needle
        .iter()
        .enumerate()
        .max_by_key(|&(i, &b)| (rarity(b), usize::MAX - i))?;
    let (lo, up) = (anchor, anchor.to_ascii_uppercase());
    let mut pos = from + a;
    while pos < hay.len() {
        let k = if lo == up {
            memchr::memchr(lo, &hay[pos..])
        } else {
            memchr::memchr2(lo, up, &hay[pos..])
        }?;
        let p = pos + k - a;
        if p + n > hay.len() {
            return None;
        }
        if hay[p..p + n].eq_ignore_ascii_case(needle) {
            return Some(p);
        }
        pos += k + 1;
    }
    None
}

/// `path/file.c:42` or `file.c:42:7` → ("path/file.c", 42).
fn split_file_line(q: &str) -> Option<(&str, u32)> {
    let (rest, last) = q.rsplit_once(':')?;
    let last: u32 = last.trim().parse().ok()?;
    // A trailing column.
    if let Some((file, line)) = rest.rsplit_once(':')
        && let Ok(line) = line.trim().parse::<u32>()
        && !file.is_empty()
    {
        return Some((file.trim(), line));
    }
    let file = rest.trim();
    (!file.is_empty() && file.len() > 1).then_some((file, last))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(parse_number("0x401000"), Some(0x401000));
        assert_eq!(parse_number("401000"), Some(0x401000));
        assert_eq!(parse_number("00007ff6`12345678"), Some(0x7ff6_1234_5678));
        assert_eq!(parse_number("beef"), None);
        assert_eq!(parse_number("0xbeef"), Some(0xbeef));
        assert_eq!(parse_number("main"), None);
    }

    #[test]
    fn patterns() {
        assert_eq!(
            parse_pattern("48 8b ?? 08"),
            Some(vec![Some(0x48), Some(0x8b), None, Some(8)])
        );
        assert_eq!(
            parse_pattern("488b??08"),
            Some(vec![Some(0x48), Some(0x8b), None, Some(8)])
        );
        assert_eq!(parse_pattern("hello world"), None);
        assert_eq!(parse_pattern("?? ??"), None);
    }

    #[test]
    fn ranking() {
        let s = |h| score(h, "print").unwrap_or(i32::MIN);
        assert!(s("print") > s("println"));
        assert!(s("println") > s("std::io::print_to"));
        assert!(s("std::io::print_to") > s("eprint"));
        assert!(s("std::io::_print") > s("eprint"));
        assert!(score("SPRINTF", "print").is_some());
        assert_eq!(score("main", "xyz"), None);
    }

    #[test]
    fn file_lines() {
        assert_eq!(split_file_line("tiny.rs:12"), Some(("tiny.rs", 12)));
        assert_eq!(split_file_line("src/a.c:3:9"), Some(("src/a.c", 3)));
        assert_eq!(split_file_line("C:\\x\\a.c:3"), Some(("C:\\x\\a.c", 3)));
        assert_eq!(split_file_line("std::fmt"), None);
    }
}
