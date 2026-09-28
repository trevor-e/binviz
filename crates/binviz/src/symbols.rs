//! Symbol table with address lookup.

use std::collections::HashMap;

use serde::Serialize;

use crate::model::{Section, Symbol, SymbolKind, SymbolRef, SymbolSource};

pub struct SymbolTable {
    symbols: Vec<Symbol>,
    /// Indices of symbols usable for address lookup, sorted by address, one per address.
    by_addr: Vec<u32>,
    by_name: HashMap<String, u32>,
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

fn lookup_rank(s: &Symbol) -> (u8, u8, u8, u8, u8) {
    // The user's own names always win at their address.
    let user = if s.source == SymbolSource::User { 0 } else { 1 };
    let kind = match s.kind {
        SymbolKind::Function => 0,
        SymbolKind::Data => 1,
        SymbolKind::Tls => 2,
        SymbolKind::Unknown => 3,
        SymbolKind::Label => 4,
        _ => 9,
    };
    let binding = match s.binding.as_str() {
        "global" => 0,
        "weak" => 1,
        _ => 2,
    };
    let sized = if s.size > 0 { 0 } else { 1 };
    let source = match s.source {
        SymbolSource::User => 0,
        SymbolSource::Symtab => 1,
        SymbolSource::Dynsym => 2,
        SymbolSource::Export => 3,
        SymbolSource::Dwarf => 4,
        SymbolSource::Discovered => 5,
    };
    (user, kind, sized, binding, source)
}

fn is_addressable(s: &Symbol) -> bool {
    s.defined
        && !matches!(
            s.kind,
            SymbolKind::Section | SymbolKind::File | SymbolKind::Debug | SymbolKind::Tls
        )
        && (s.address != 0 || s.section.is_some())
        && !s.name.is_empty()
}

impl SymbolTable {
    /// Builds the table, inferring sizes for symbols that don't record one.
    pub fn new(mut symbols: Vec<Symbol>, sections: &[Section]) -> SymbolTable {
        for (i, s) in symbols.iter_mut().enumerate() {
            s.index = i as u32;
        }
        let mut candidates: Vec<u32> = symbols.iter().filter(|s| is_addressable(s)).map(|s| s.index).collect();
        candidates.sort_by(|&a, &b| {
            let (sa, sb) = (&symbols[a as usize], &symbols[b as usize]);
            sa.address
                .cmp(&sb.address)
                .then_with(|| lookup_rank(sa).cmp(&lookup_rank(sb)))
        });
        let mut by_addr: Vec<u32> = Vec::with_capacity(candidates.len());
        for idx in candidates {
            match by_addr.last() {
                Some(&last) if symbols[last as usize].address == symbols[idx as usize].address => {}
                _ => by_addr.push(idx),
            }
        }

        // Infer missing sizes from the next symbol, bounded by the section end.
        for i in 0..by_addr.len() {
            let idx = by_addr[i] as usize;
            if symbols[idx].size != 0 {
                continue;
            }
            let addr = symbols[idx].address;
            let section_end = symbols[idx]
                .section
                .and_then(|s| sections.get(s as usize))
                .filter(|s| s.address <= addr && addr < s.address + s.size)
                .map(|s| s.address + s.size);
            // Local labels inside a function don't end it.
            let label = symbols[idx].kind == SymbolKind::Label;
            let next = by_addr[i + 1..]
                .iter()
                .take(64)
                .map(|&n| &symbols[n as usize])
                .find(|s| label || s.kind != SymbolKind::Label)
                .or_else(|| by_addr.get(i + 1).map(|&n| &symbols[n as usize]))
                .map(|s| s.address);
            let end = match (next, section_end) {
                (Some(n), Some(e)) => Some(n.min(e)),
                (Some(n), None) => Some(n),
                (None, e) => e,
            };
            if let Some(end) = end
                && end > addr
            {
                symbols[idx].size = end - addr;
                symbols[idx].size_inferred = true;
            }
        }

        let mut by_name = HashMap::with_capacity(symbols.len());
        for s in &symbols {
            if s.defined && !s.name.is_empty() {
                by_name.entry(s.name.clone()).or_insert(s.index);
                if let Some(d) = &s.demangled {
                    by_name.entry(d.clone()).or_insert(s.index);
                }
            }
        }
        SymbolTable {
            symbols,
            by_addr,
            by_name,
        }
    }

    /// The closest lookup symbol at or before `address`, whatever its size.
    pub fn before(&self, address: u64) -> Option<&Symbol> {
        let pos = self
            .by_addr
            .partition_point(|&i| self.symbols[i as usize].address <= address);
        self.by_addr[..pos].last().map(|&i| &self.symbols[i as usize])
    }

    /// The best symbol at exactly `address` (the one lookups report).
    pub fn at(&self, address: u64) -> Option<&Symbol> {
        let i = self
            .by_addr
            .partition_point(|&i| self.symbols[i as usize].address < address);
        self.by_addr
            .get(i)
            .map(|&i| &self.symbols[i as usize])
            .filter(|s| s.address == address)
    }

    pub fn all(&self) -> &[Symbol] {
        &self.symbols
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    pub fn get(&self, index: u32) -> Option<&Symbol> {
        self.symbols.get(index as usize)
    }

    /// Finds a symbol by raw or demangled name; falls back to a unique
    /// `…::name` suffix match so plain function names work too.
    pub fn by_name(&self, name: &str) -> Option<&Symbol> {
        if let Some(&i) = self.by_name.get(name) {
            return self.get(i);
        }
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
        let mut found = self.symbols.iter().filter(|s| {
            s.defined
                && (s
                    .demangled
                    .as_deref()
                    .map(base)
                    .is_some_and(|d| d == name || d.ends_with(&suffix))
                    || s.name.strip_prefix('_') == Some(name))
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
            .partition_point(|&i| self.symbols[i as usize].address <= address);
        let s = &self.symbols[*self.by_addr[..pos].last()? as usize];
        let offset = address - s.address;
        if offset < s.size.max(1) {
            Some(SymbolRef {
                index: s.index,
                name: s.name.clone(),
                demangled: s.demangled.clone(),
                address: s.address,
                size: s.size,
                offset,
            })
        } else {
            None
        }
    }

    /// Lookup symbols (one per address) overlapping `lo..hi`, in address order.
    pub fn in_range(&self, lo: u64, hi: u64) -> impl Iterator<Item = &Symbol> + '_ {
        let mut start = self.by_addr.partition_point(|&i| self.symbols[i as usize].address < lo);
        if start > 0 {
            let prev = &self.symbols[self.by_addr[start - 1] as usize];
            if prev.address + prev.size > lo {
                start -= 1;
            }
        }
        self.by_addr[start..]
            .iter()
            .map(|&i| &self.symbols[i as usize])
            .take_while(move |s| s.address < hi)
    }

    /// Functions in address order (for disassembly navigation).
    pub fn functions(&self) -> impl Iterator<Item = &Symbol> + '_ {
        self.by_addr
            .iter()
            .map(|&i| &self.symbols[i as usize])
            .filter(|s| s.kind == SymbolKind::Function)
    }

    pub fn query(&self, q: &SymbolQuery) -> SymbolPage {
        let needle = q.filter.to_lowercase();
        let mut matches: Vec<&Symbol> = self
            .symbols
            .iter()
            .filter(|s| !q.defined_only || s.defined)
            .filter(|s| q.kind.is_empty() || kind_name(s.kind) == q.kind || (q.kind == "undefined" && !s.defined))
            .filter(|s| {
                needle.is_empty()
                    || s.name.to_lowercase().contains(&needle)
                    || s.demangled.as_ref().is_some_and(|d| d.to_lowercase().contains(&needle))
            })
            .collect();
        match q.sort.as_str() {
            "name" => matches.sort_by(|a, b| a.display_name().cmp(b.display_name())),
            "size" => matches.sort_by_key(|s| s.size),
            _ => matches.sort_by_key(|s| (!s.defined, s.address)),
        }
        if q.descending {
            matches.reverse();
        }
        let total = matches.len() as u32;
        let limit = if q.limit == 0 { 200 } else { q.limit };
        let symbols = matches
            .into_iter()
            .skip(q.offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        SymbolPage {
            total,
            offset: q.offset,
            symbols,
        }
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
