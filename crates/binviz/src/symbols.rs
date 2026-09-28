//! Symbol table with address lookup, built for binaries with millions of
//! symbols (the recovered functions of a large app, a full symbol table):
//!
//! - symbols are 32-byte records whose names live in one shared string arena,
//!   rather than a heap string (or several) per symbol;
//! - names are demangled when displayed, or all at once — only when a search or
//!   a sort by name needs every name;
//! - the user's own names sit on top of the file's symbols, so renaming
//!   something re-merges an index instead of rebuilding the table.

use std::borrow::Cow;
use std::sync::{Mutex, OnceLock};

use serde::Serialize;

use crate::model::{Section, Symbol, SymbolKind, SymbolRef, SymbolSource};
use crate::util;

/// How a symbol is bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Binding {
    Global,
    Weak,
    Local,
    Undefined,
}

impl Binding {
    pub fn as_str(self) -> &'static str {
        match self {
            Binding::Global => "global",
            Binding::Weak => "weak",
            Binding::Local => "local",
            Binding::Undefined => "undefined",
        }
    }
}

const DEFINED: u8 = 1;
const SIZE_INFERRED: u8 = 2;
/// The name is never mangled (recovered `sub_` names, names from DWARF).
const PLAIN: u8 = 4;
/// An ARM mapping symbol (`$x`, `$d.12`): it marks code or data but names
/// nothing, so address lookups skip it.
const MAPPING: u8 = 8;
const NO_SECTION: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
struct Rec {
    address: u64,
    size: u64,
    /// Offset and length of the name in the arena.
    name: u32,
    name_len: u32,
    section: u32,
    kind: SymbolKind,
    binding: Binding,
    source: SymbolSource,
    flags: u8,
}

/// A symbol to add to the table.
pub(crate) struct NewSym<'n> {
    pub name: &'n str,
    pub address: u64,
    pub size: u64,
    pub kind: SymbolKind,
    pub binding: Binding,
    pub section: Option<u32>,
    pub source: SymbolSource,
    pub defined: bool,
    /// Known not to be mangled, so display never tries to demangle it.
    pub plain: bool,
}

/// A symbol in the table: its fields, plus its names on demand.
#[derive(Clone, Copy)]
pub struct Sym<'a> {
    table: &'a SymbolTable,
    pub index: u32,
    pub address: u64,
    pub size: u64,
    /// The size was inferred from the next symbol rather than recorded in the file.
    pub size_inferred: bool,
    pub kind: SymbolKind,
    pub binding: Binding,
    pub section: Option<u32>,
    pub source: SymbolSource,
    pub defined: bool,
}

impl<'a> Sym<'a> {
    pub fn name(&self) -> &'a str {
        self.table.name_of(self.index)
    }

    /// The demangled name, if the name is mangled.
    pub fn demangled(&self) -> Option<Cow<'a, str>> {
        self.table.demangled_of(self.index)
    }

    pub fn display_name(&self) -> Cow<'a, str> {
        self.demangled().unwrap_or(Cow::Borrowed(self.name()))
    }

    /// An owned copy, for handing out (serialization, pages of results).
    pub fn to_symbol(&self) -> Symbol {
        Symbol {
            index: self.index,
            name: self.name().to_string(),
            demangled: self.demangled().map(Cow::into_owned),
            address: self.address,
            size: self.size,
            size_inferred: self.size_inferred,
            kind: self.kind,
            binding: self.binding.as_str().to_string(),
            section: self.section,
            source: self.source,
            defined: self.defined,
        }
    }
}

impl std::fmt::Debug for Sym<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} @ {:#x}+{:#x}", self.name(), self.address, self.size)
    }
}

/// Demangled names of the file's symbols, computed together when needed.
struct Demangled {
    text: String,
    /// Per file record: offset and length in `text`; length 0 means not mangled.
    spans: Vec<(u32, u32)>,
    /// (offset in `text`, record) for each mangled record, in order.
    owners: Vec<(u32, u32)>,
}

pub struct SymbolTable {
    recs: Vec<Rec>,
    names: String,
    /// Records `..file_len` come from the file, `file_len..static_len` from
    /// DWARF and function recovery, the rest are the user's.
    file_len: u32,
    static_len: u32,
    /// Arena length at the end of the file's / the static records' names.
    file_names: usize,
    static_names: usize,
    /// Best file or recovered symbol per address, in address order.
    base_by_addr: Vec<u32>,
    /// `base_by_addr` with the user's symbols merged in.
    by_addr: Vec<u32>,
    demangled: OnceLock<Demangled>,
    /// File and recovered records sorted by raw name, built on first lookup by name.
    by_name: OnceLock<Vec<u32>>,
    /// File and recovered records sorted by display name, built the first time
    /// symbols are sorted by name (the user's few are merged in per query).
    name_order: OnceLock<Vec<u32>>,
    /// The last symbol query's matches, in order, so paging doesn't re-sort.
    query_cache: Mutex<Option<(String, Vec<u32>)>>,
    /// The last function-list filter's matches, in address order.
    function_cache: Mutex<Option<(String, Vec<u32>)>>,
}

/// A page of the function list: `(address, size, name)`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionPage {
    pub total: u32,
    pub offset: u32,
    pub functions: Vec<(u64, u64, String)>,
}

/// Parameters for [`SymbolTable::query`].
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SymbolQuery {
    /// Case-insensitive substring matched against raw and demangled names.
    pub filter: String,
    /// Restrict to one kind ("function", "data", ...); empty for all.
    pub kind: String,
    /// "address" (default), "name", "size".
    pub sort: String,
    pub descending: bool,
    pub defined_only: bool,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolPage {
    pub total: u32,
    pub offset: u32,
    pub symbols: Vec<Symbol>,
}

fn lookup_rank(r: &Rec) -> (u8, u8, u8, u8, u8) {
    // The user's own names always win at their address.
    let user = if r.source == SymbolSource::User { 0 } else { 1 };
    let kind = match r.kind {
        SymbolKind::Function => 0,
        SymbolKind::Data => 1,
        SymbolKind::Tls => 2,
        SymbolKind::Unknown => 3,
        SymbolKind::Label => 4,
        _ => 9,
    };
    let binding = match r.binding {
        Binding::Global => 0,
        Binding::Weak => 1,
        _ => 2,
    };
    let sized = if r.size > 0 { 0 } else { 1 };
    let source = match r.source {
        SymbolSource::User => 0,
        SymbolSource::Symtab => 1,
        SymbolSource::Dynsym => 2,
        SymbolSource::Export => 3,
        SymbolSource::Dwarf => 4,
        SymbolSource::Import => 5,
        SymbolSource::Discovered => 6,
    };
    (user, kind, sized, binding, source)
}

fn is_addressable(r: &Rec) -> bool {
    r.flags & DEFINED != 0
        && !matches!(
            r.kind,
            SymbolKind::Section | SymbolKind::File | SymbolKind::Debug | SymbolKind::Tls
        )
        && (r.address != 0 || r.section != NO_SECTION)
        && r.name_len > 0
        && r.flags & MAPPING == 0
}

/// `lookup_rank` packed into one number with the same order.
fn rank_key(r: &Rec) -> u32 {
    let (a, b, c, d, e) = lookup_rank(r);
    (a as u32) << 16 | (b as u32) << 12 | (c as u32) << 8 | (d as u32) << 4 | e as u32
}

/// Records in `range` usable for address lookup: sorted by address, best first
/// per address, one per address.
fn best_per_address(recs: &[Rec], range: std::ops::Range<usize>) -> Vec<u32> {
    let mut keyed: Vec<(u64, u32, u32)> = range
        .filter(|&i| is_addressable(&recs[i]))
        .map(|i| (recs[i].address, rank_key(&recs[i]), i as u32))
        .collect();
    keyed.sort_unstable();
    keyed.dedup_by_key(|k| k.0);
    keyed.into_iter().map(|k| k.2).collect()
}

/// The end of the section containing `r`, if any.
fn section_end(r: &Rec, sections: &[Section]) -> Option<u64> {
    let s = sections.get(r.section as usize)?;
    (s.address <= r.address && r.address < s.address + s.size).then_some(s.address + s.size)
}

/// Gives sizeless symbols in `order` a size reaching the next symbol (or their
/// section's end). Local labels inside a function don't end it.
fn infer_sizes(recs: &mut [Rec], order: &[u32], sections: &[Section], only: impl Fn(u32) -> bool) {
    for (i, &idx) in order.iter().enumerate() {
        let r = recs[idx as usize];
        if r.size != 0 || !only(idx) {
            continue;
        }
        // A symbol outside the section it names (Mach-O's `__mh_execute_header`
        // sits on the header, not in `__text`) marks a spot; it has no extent.
        if r.section != NO_SECTION && section_end(&r, sections).is_none() {
            continue;
        }
        let label = r.kind == SymbolKind::Label;
        let next = order[i + 1..]
            .iter()
            .take(64)
            .map(|&n| &recs[n as usize])
            .find(|s| label || s.kind != SymbolKind::Label)
            .or_else(|| order.get(i + 1).map(|&n| &recs[n as usize]))
            .map(|s| s.address);
        let end = match (next, section_end(&r, sections)) {
            (Some(n), Some(e)) => Some(n.min(e)),
            (Some(n), None) => Some(n),
            (None, e) => e,
        };
        if let Some(end) = end
            && end > r.address
        {
            let rec = &mut recs[idx as usize];
            rec.size = end - r.address;
            rec.flags |= SIZE_INFERRED;
        }
    }
}

/// Collects symbols into records and arena storage.
#[derive(Default)]
pub(crate) struct Builder {
    recs: Vec<Rec>,
    names: String,
}

impl Builder {
    pub fn push(&mut self, s: NewSym<'_>) {
        push_rec(&mut self.recs, &mut self.names, s);
    }

    /// Addresses of the named, defined symbols so far, sorted.
    pub fn defined_addresses(&self) -> Vec<u64> {
        let mut v: Vec<u64> = self
            .recs
            .iter()
            .filter(|r| is_addressable(r))
            .map(|r| r.address)
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// The file's symbols as a table (recovered and user symbols come later).
    pub fn finish(self, sections: &[Section]) -> SymbolTable {
        let mut table = self.finish_unindexed();
        table.set_static(std::iter::empty(), sections);
        table
    }

    /// Like `finish`, for a caller about to call `set_static` itself: lookups
    /// by address find nothing until it does.
    pub fn finish_unindexed(mut self) -> SymbolTable {
        self.recs.shrink_to_fit();
        self.names.shrink_to_fit();
        let n = self.recs.len() as u32;
        let names = self.names.len();
        SymbolTable {
            recs: self.recs,
            names: self.names,
            file_len: n,
            static_len: n,
            file_names: names,
            static_names: names,
            base_by_addr: Vec::new(),
            by_addr: Vec::new(),
            demangled: OnceLock::new(),
            by_name: OnceLock::new(),
            name_order: OnceLock::new(),
            query_cache: Mutex::new(None),
            function_cache: Mutex::new(None),
        }
    }
}

fn push_rec(recs: &mut Vec<Rec>, names: &mut String, s: NewSym<'_>) {
    let name = names.len() as u32;
    if s.name.is_empty() && s.source == SymbolSource::Discovered {
        use std::fmt::Write as _;
        let _ = write!(names, "sub_{:x}", s.address);
    } else {
        names.push_str(s.name);
    }
    let name_len = names.len() as u32 - name;
    let mut flags = 0;
    if s.defined {
        flags |= DEFINED;
    }
    if s.plain || !util::looks_mangled(s.name) {
        flags |= PLAIN;
    }
    let b = s.name.as_bytes();
    if b.len() >= 2 && b[0] == b'$' && matches!(b[1], b'a' | b'd' | b't' | b'x') && (b.len() == 2 || b[2] == b'.') {
        flags |= MAPPING;
    }
    recs.push(Rec {
        address: s.address,
        size: s.size,
        name,
        name_len,
        section: s.section.unwrap_or(NO_SECTION),
        kind: s.kind,
        binding: s.binding,
        source: s.source,
        flags,
    });
}

impl SymbolTable {
    /// An empty table.
    pub fn empty() -> SymbolTable {
        Builder::default().finish(&[])
    }

    /// Replaces the recovered and DWARF-derived symbols (and drops the user's,
    /// which [`Self::set_user`] adds back).
    pub(crate) fn set_static<'n>(&mut self, extra: impl IntoIterator<Item = NewSym<'n>>, sections: &[Section]) {
        self.recs.truncate(self.file_len as usize);
        self.names.truncate(self.file_names);
        for s in extra {
            push_rec(&mut self.recs, &mut self.names, s);
        }
        self.recs.shrink_to_fit();
        self.names.shrink_to_fit();
        // Room for the user's names: appending them must not double the arrays.
        self.recs.reserve_exact(4096);
        self.names.reserve_exact(256 * 1024);
        self.static_len = self.recs.len() as u32;
        self.static_names = self.names.len();
        // Undo sizes inferred earlier: neighbours may have changed.
        for r in &mut self.recs {
            if r.flags & SIZE_INFERRED != 0 {
                r.size = 0;
                r.flags &= !SIZE_INFERRED;
            }
        }
        let order = best_per_address(&self.recs, 0..self.static_len as usize);
        infer_sizes(&mut self.recs, &order, sections, |_| true);
        self.by_addr = order.clone();
        self.base_by_addr = order;
        self.by_name = OnceLock::new();
        self.name_order = OnceLock::new();
        *self.query_cache.lock().unwrap() = None;
        *self.function_cache.lock().unwrap() = None;
    }

    /// Replaces the user's symbols. A sizeless one takes the size of a symbol
    /// already at its address, else reaches the next symbol.
    pub(crate) fn set_user<'n>(&mut self, user: impl IntoIterator<Item = NewSym<'n>>, sections: &[Section]) {
        self.recs.truncate(self.static_len as usize);
        self.names.truncate(self.static_names);
        for s in user {
            push_rec(&mut self.recs, &mut self.names, s);
        }
        let users = best_per_address(&self.recs, self.static_len as usize..self.recs.len());
        // Merge: at an address both have, the user's symbol wins.
        let mut merged = Vec::with_capacity(self.base_by_addr.len() + users.len());
        let (mut i, mut j) = (0, 0);
        let (base, recs) = (&self.base_by_addr, &self.recs);
        while i < base.len() || j < users.len() {
            let a = base.get(i).map(|&b| recs[b as usize].address);
            let u = users.get(j).map(|&x| recs[x as usize].address);
            match (a, u) {
                (Some(a), Some(u)) if u <= a => {
                    merged.push(users[j]);
                    j += 1;
                    if u == a {
                        i += 1;
                    }
                }
                (Some(_), _) => {
                    merged.push(base[i]);
                    i += 1;
                }
                (None, Some(_)) => {
                    merged.push(users[j]);
                    j += 1;
                }
                (None, None) => break,
            }
        }
        for &u in &users {
            let r = self.recs[u as usize];
            if r.size != 0 {
                continue;
            }
            let pos = self
                .base_by_addr
                .partition_point(|&b| self.recs[b as usize].address < r.address);
            if let Some(&b) = self.base_by_addr.get(pos) {
                let under = self.recs[b as usize];
                if under.address == r.address && under.size > 0 && under.flags & SIZE_INFERRED == 0 {
                    self.recs[u as usize].size = under.size;
                }
            }
        }
        let first_user = self.static_len;
        infer_sizes(&mut self.recs, &merged, sections, |i| i >= first_user);
        self.by_addr = merged;
        *self.query_cache.lock().unwrap() = None;
        *self.function_cache.lock().unwrap() = None;
    }

    fn sym(&self, index: u32) -> Sym<'_> {
        let r = &self.recs[index as usize];
        Sym {
            table: self,
            index,
            address: r.address,
            size: r.size,
            size_inferred: r.flags & SIZE_INFERRED != 0,
            kind: r.kind,
            binding: r.binding,
            section: (r.section != NO_SECTION).then_some(r.section),
            source: r.source,
            defined: r.flags & DEFINED != 0,
        }
    }

    fn name_of(&self, index: u32) -> &str {
        let r = &self.recs[index as usize];
        &self.names[r.name as usize..(r.name + r.name_len) as usize]
    }

    fn demangled_of(&self, index: u32) -> Option<Cow<'_, str>> {
        let r = &self.recs[index as usize];
        if r.flags & PLAIN != 0 {
            return None;
        }
        if index < self.file_len
            && let Some(d) = self.demangled.get()
        {
            let (off, len) = d.spans[index as usize];
            return (len > 0).then(|| Cow::Borrowed(&d.text[off as usize..(off + len) as usize]));
        }
        util::demangle(self.name_of(index)).map(Cow::Owned)
    }

    fn demangled_all(&self) -> &Demangled {
        self.demangled.get_or_init(|| {
            let mut text = String::new();
            let mut spans = Vec::with_capacity(self.file_len as usize);
            let mut owners = Vec::new();
            for i in 0..self.file_len {
                let r = &self.recs[i as usize];
                let d = if r.flags & PLAIN == 0 {
                    util::demangle(self.name_of(i))
                } else {
                    None
                };
                match d {
                    Some(d) => {
                        spans.push((text.len() as u32, d.len() as u32));
                        owners.push((text.len() as u32, i));
                        text.push_str(&d);
                    }
                    None => spans.push((text.len() as u32, 0)),
                }
            }
            text.shrink_to_fit();
            Demangled { text, spans, owners }
        })
    }

    /// Demangles every name now (search and sorting by name need them all).
    pub fn prepare_names(&self) {
        self.demangled_all();
    }

    /// Every raw name, back to back in record order, for scanning all at once.
    pub(crate) fn name_arena(&self) -> &str {
        &self.names
    }

    /// The record whose raw name contains arena offset `offset`.
    pub(crate) fn record_at_name_offset(&self, offset: usize) -> Option<u32> {
        let i = self
            .recs
            .partition_point(|r| r.name as usize <= offset)
            .checked_sub(1)?;
        let r = &self.recs[i];
        (offset < (r.name + r.name_len) as usize).then_some(i as u32)
    }

    /// Every demangled name of the file's symbols, back to back in record order.
    pub(crate) fn demangled_arena(&self) -> &str {
        &self.demangled_all().text
    }

    /// The file record whose demangled name contains offset `offset` of the demangled arena.
    pub(crate) fn record_at_demangled_offset(&self, offset: usize) -> Option<u32> {
        let d = self.demangled_all();
        let k = d
            .owners
            .partition_point(|&(o, _)| o as usize <= offset)
            .checked_sub(1)?;
        let record = d.owners[k].1;
        let (o, l) = d.spans[record as usize];
        (offset < (o + l) as usize).then_some(record)
    }

    /// The raw name span of a record: (offset, length) in the name arena.
    pub(crate) fn name_span(&self, index: u32) -> (usize, usize) {
        let r = &self.recs[index as usize];
        (r.name as usize, r.name_len as usize)
    }

    /// The demangled span of a file record, once all names are demangled.
    pub(crate) fn demangled_span(&self, index: u32) -> Option<(usize, usize)> {
        let (o, l) = *self.demangled_all().spans.get(index as usize)?;
        (l > 0).then_some((o as usize, l as usize))
    }

    /// Number of records that come from the file's own tables.
    pub(crate) fn file_len(&self) -> u32 {
        self.file_len
    }

    /// The best symbol at exactly `address` (the one lookups report).
    pub fn at(&self, address: u64) -> Option<Sym<'_>> {
        let i = self
            .by_addr
            .partition_point(|&i| self.recs[i as usize].address < address);
        self.by_addr
            .get(i)
            .map(|&i| self.sym(i))
            .filter(|s| s.address == address)
    }

    /// The closest lookup symbol at or before `address`, whatever its size.
    pub fn before(&self, address: u64) -> Option<Sym<'_>> {
        let pos = self
            .by_addr
            .partition_point(|&i| self.recs[i as usize].address <= address);
        self.by_addr[..pos].last().map(|&i| self.sym(i))
    }

    pub fn iter(&self) -> impl Iterator<Item = Sym<'_>> + '_ {
        (0..self.recs.len() as u32).map(|i| self.sym(i))
    }

    pub fn len(&self) -> usize {
        self.recs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recs.is_empty()
    }

    pub fn get(&self, index: u32) -> Option<Sym<'_>> {
        ((index as usize) < self.recs.len()).then(|| self.sym(index))
    }

    /// Finds a symbol by raw or demangled name; falls back to a unique
    /// `…::name` suffix match so plain function names work too.
    pub fn by_name(&self, name: &str) -> Option<Sym<'_>> {
        // The user's names first, then the file's, exactly.
        if let Some(i) = (self.static_len..self.recs.len() as u32).find(|&i| self.name_of(i) == name) {
            return Some(self.sym(i));
        }
        let sorted = self.by_name.get_or_init(|| {
            let mut v: Vec<u32> = (0..self.static_len)
                .filter(|&i| self.recs[i as usize].flags & DEFINED != 0 && self.recs[i as usize].name_len > 0)
                .collect();
            v.sort_unstable_by(|&a, &b| self.name_of(a).cmp(self.name_of(b)).then(a.cmp(&b)));
            v
        });
        let pos = sorted.partition_point(|&i| self.name_of(i) < name);
        if let Some(&i) = sorted.get(pos)
            && self.name_of(i) == name
        {
            return Some(self.sym(i));
        }
        self.prepare_names();
        let suffix = format!("::{name}");
        // C++ demangled names carry parameter lists: compare the part before them.
        let base = |d: &str| -> String {
            let mut depth = 0;
            for (i, c) in d.char_indices() {
                match c {
                    '<' => depth += 1,
                    '>' => depth -= 1,
                    '(' if depth == 0 && i > 0 => return d[..i].to_string(),
                    _ => {}
                }
            }
            d.to_string()
        };
        let mut found = self.iter().filter(|s| {
            s.defined
                && (s
                    .demangled()
                    .map(|d| base(&d))
                    .is_some_and(|d| d == name || d.ends_with(&suffix))
                    || s.name().strip_prefix('_') == Some(name))
        });
        let first = found.next()?;
        // Prefer functions if the suffix is ambiguous.
        Some(if first.kind == SymbolKind::Function {
            first
        } else {
            found.find(|s| s.kind == SymbolKind::Function).unwrap_or(first)
        })
    }

    /// The symbol whose extent contains `address`.
    pub fn lookup(&self, address: u64) -> Option<SymbolRef> {
        let pos = self
            .by_addr
            .partition_point(|&i| self.recs[i as usize].address <= address);
        let s = self.sym(*self.by_addr[..pos].last()?);
        let offset = address - s.address;
        (offset < s.size.max(1)).then(|| SymbolRef {
            index: s.index,
            name: s.name().to_string(),
            demangled: s.demangled().map(Cow::into_owned),
            address: s.address,
            size: s.size,
            offset,
        })
    }

    /// Lookup symbols (one per address) overlapping `lo..hi`, in address order.
    pub fn in_range(&self, lo: u64, hi: u64) -> impl Iterator<Item = Sym<'_>> + '_ {
        let mut start = self.by_addr.partition_point(|&i| self.recs[i as usize].address < lo);
        if start > 0 {
            let prev = &self.recs[self.by_addr[start - 1] as usize];
            if prev.address + prev.size > lo {
                start -= 1;
            }
        }
        self.by_addr[start..]
            .iter()
            .map(|&i| self.sym(i))
            .take_while(move |s| s.address < hi)
    }

    /// Functions in address order (for disassembly navigation).
    pub fn functions(&self) -> impl Iterator<Item = Sym<'_>> + '_ {
        self.by_addr
            .iter()
            .filter(|&&i| self.recs[i as usize].kind == SymbolKind::Function)
            .map(|&i| self.sym(i))
    }

    /// The function in `order` whose extent contains `address`, looking back
    /// past labels and data symbols inside it.
    fn containing_in(&self, order: &[u32], address: u64) -> Option<u32> {
        let pos = order.partition_point(|&i| self.recs[i as usize].address <= address);
        for &i in order[..pos].iter().rev().take(64) {
            let r = &self.recs[i as usize];
            if r.kind == SymbolKind::Function {
                return (address - r.address < r.size.max(1)).then_some(i);
            }
        }
        None
    }

    /// The function whose extent contains `address` (the user's included).
    pub fn function_containing(&self, address: u64) -> Option<Sym<'_>> {
        self.containing_in(&self.by_addr, address).map(|i| self.sym(i))
    }

    /// Like [`Self::function_containing`], over the file's and recovered
    /// functions only (analyses built once must not depend on the user's names):
    /// the function's start and end.
    pub(crate) fn static_function_containing(&self, address: u64) -> Option<(u64, u64)> {
        let r = &self.recs[self.containing_in(&self.base_by_addr, address)? as usize];
        Some((r.address, r.address + r.size.max(1)))
    }

    /// Whether a file or recovered function starts at `address`.
    pub(crate) fn is_static_function_start(&self, address: u64) -> bool {
        let pos = self
            .base_by_addr
            .partition_point(|&i| self.recs[i as usize].address < address);
        self.base_by_addr.get(pos).is_some_and(|&i| {
            let r = &self.recs[i as usize];
            r.address == address && r.kind == SymbolKind::Function
        })
    }

    /// File and recovered functions starting in `lo..hi`: (start, end), in order.
    pub(crate) fn static_functions_in(&self, lo: u64, hi: u64) -> impl Iterator<Item = (u64, u64)> + '_ {
        let first = self
            .base_by_addr
            .partition_point(|&i| self.recs[i as usize].address < lo);
        self.base_by_addr[first..]
            .iter()
            .map(|&i| &self.recs[i as usize])
            .take_while(move |r| r.address < hi)
            .filter(|r| r.kind == SymbolKind::Function)
            .map(|r| (r.address, r.address + r.size.max(1)))
    }

    /// Functions whose name (or hex address) contains `filter`, in address
    /// order; the result is kept so paging and position lookups are cheap.
    fn filtered_functions(&self, filter: &str) -> std::sync::MutexGuard<'_, Option<(String, Vec<u32>)>> {
        let needle = filter.trim().to_ascii_lowercase();
        let mut cache = self.function_cache.lock().unwrap();
        if cache.as_ref().is_none_or(|(k, _)| *k != needle) {
            let hex = needle.strip_prefix("0x").unwrap_or(&needle).to_string();
            let is_hex = !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit());
            if !needle.is_empty() {
                self.prepare_names();
            }
            let contains = |hay: &str| {
                let (h, n) = (hay.as_bytes(), needle.as_bytes());
                n.len() <= h.len() && h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
            };
            let list: Vec<u32> = self
                .by_addr
                .iter()
                .copied()
                .filter(|&i| self.recs[i as usize].kind == SymbolKind::Function)
                .filter(|&i| {
                    needle.is_empty()
                        || contains(&self.sym(i).display_name())
                        || (is_hex && format!("{:x}", self.recs[i as usize].address).contains(&hex))
                })
                .collect();
            *cache = Some((needle, list));
        }
        cache
    }

    /// A page of the function list (see [`Self::function_position`]).
    pub fn function_page(&self, filter: &str, offset: u32, limit: u32) -> FunctionPage {
        let cache = self.filtered_functions(filter);
        let list = &cache.as_ref().expect("filled").1;
        FunctionPage {
            total: list.len() as u32,
            offset,
            functions: list
                .iter()
                .skip(offset as usize)
                .take(if limit == 0 { 200 } else { limit } as usize)
                .map(|&i| {
                    let s = self.sym(i);
                    (s.address, s.size, s.display_name().into_owned())
                })
                .collect(),
        }
    }

    /// Position in the filtered function list of the function containing `address`.
    pub fn function_position(&self, filter: &str, address: u64) -> Option<u32> {
        let cache = self.filtered_functions(filter);
        let list = &cache.as_ref().expect("filled").1;
        let pos = list
            .partition_point(|&i| self.recs[i as usize].address <= address)
            .checked_sub(1)?;
        let r = &self.recs[list[pos] as usize];
        (address - r.address < r.size.max(1)).then_some(pos as u32)
    }

    pub fn query(&self, q: &SymbolQuery) -> SymbolPage {
        let key = format!(
            "{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}",
            q.filter, q.kind, q.sort, q.descending, q.defined_only
        );
        let mut cache = self.query_cache.lock().unwrap();
        if cache.as_ref().is_none_or(|(k, _)| *k != key) {
            *cache = Some((key, self.run_query(q)));
        }
        let matches = &cache.as_ref().expect("just filled").1;
        let limit = if q.limit == 0 { 200 } else { q.limit };
        SymbolPage {
            total: matches.len() as u32,
            offset: q.offset,
            symbols: matches
                .iter()
                .skip(q.offset as usize)
                .take(limit as usize)
                .map(|&i| self.sym(i).to_symbol())
                .collect(),
        }
    }

    /// File and recovered records by display name (sorted once).
    fn static_name_order(&self) -> &[u32] {
        self.name_order.get_or_init(|| {
            self.prepare_names();
            let mut keyed: Vec<(Cow<'_, str>, u32)> =
                (0..self.static_len).map(|i| (self.sym(i).display_name(), i)).collect();
            keyed.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            keyed.into_iter().map(|(_, i)| i).collect()
        })
    }

    fn run_query(&self, q: &SymbolQuery) -> Vec<u32> {
        let needle = q.filter.to_lowercase();
        if q.sort == "name" || !needle.is_empty() {
            self.prepare_names();
        }
        let contains = |hay: &str| -> bool {
            let (h, n) = (hay.as_bytes(), needle.as_bytes());
            n.len() <= h.len() && h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
        };
        let keep = |i: u32| -> bool {
            let r = &self.recs[i as usize];
            (!q.defined_only || r.flags & DEFINED != 0)
                && (q.kind.is_empty()
                    || kind_name(r.kind) == q.kind
                    || (q.kind == "undefined" && r.flags & DEFINED == 0))
                && (needle.is_empty()
                    || contains(self.name_of(i))
                    || self.demangled_of(i).is_some_and(|d| contains(&d)))
        };
        let mut matches: Vec<u32> = if q.sort == "name" {
            // The file's symbols come presorted; merge the user's few in.
            let mut user: Vec<u32> = (self.static_len..self.recs.len() as u32).filter(|&i| keep(i)).collect();
            user.sort_by(|&a, &b| self.sym(a).display_name().cmp(&self.sym(b).display_name()));
            let mut user = user.into_iter().peekable();
            let mut out = Vec::new();
            for b in self.static_name_order().iter().copied().filter(|&i| keep(i)) {
                while let Some(&u) = user.peek()
                    && self.sym(u).display_name() <= self.sym(b).display_name()
                {
                    out.push(u);
                    user.next();
                }
                out.push(b);
            }
            out.extend(user);
            out
        } else {
            let mut m: Vec<u32> = (0..self.recs.len() as u32).filter(|&i| keep(i)).collect();
            if q.sort == "size" {
                m.sort_by_key(|&i| self.recs[i as usize].size);
            } else {
                m.sort_by_key(|&i| {
                    let r = &self.recs[i as usize];
                    (r.flags & DEFINED == 0, r.address)
                });
            }
            m
        };
        if q.descending {
            matches.reverse();
        }
        matches
    }
}

fn kind_name(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Data => "data",
        SymbolKind::Section => "section",
        SymbolKind::File => "file",
        SymbolKind::Label => "label",
        SymbolKind::Tls => "tls",
        SymbolKind::Debug => "debug",
        SymbolKind::Unknown => "unknown",
    }
}
