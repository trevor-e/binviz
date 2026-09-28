//! "What is at this location?" — combining layout, symbols, DWARF and code.

use std::sync::Arc;

use object::{Object, ObjectSection};
use serde::Deserialize;

use crate::binary::Binary;
use crate::dwarf::{self, DebugInfo};
use crate::error::{Result, bail};
use crate::layout::Machine;
use std::collections::HashSet;

use crate::model::{Annotation, Inspection, RegionKind, Symbol, SymbolKind, SymbolSource};
use crate::symbols::SymbolTable;

/// A location in a binary, either in the file or in the loaded image.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum Target {
    Offset(u64),
    Address(u64),
}

impl Binary {
    /// Everything known about a file offset or virtual address.
    pub fn inspect(&self, target: Target) -> Inspection {
        let (offset, address) = match target {
            Target::Offset(o) => (
                Some(o).filter(|&o| o < self.data.len() as u64),
                self.offset_to_address(o),
            ),
            Target::Address(a) => (self.address_to_offset(a), Some(a)),
        };
        let path = offset.map(|o| self.describe_offset(o)).unwrap_or_default();
        let byte = offset.and_then(|o| self.data.get(o as usize).copied());
        let segment = address
            .and_then(|a| self.segment_at(a))
            .or_else(|| {
                let o = offset?;
                self.segments
                    .iter()
                    .find(|s| s.mapped && o >= s.file_offset && o - s.file_offset < s.file_size)
            })
            .map(|s| s.index);
        let section = address
            .and_then(|a| self.section_at(a))
            .or_else(|| offset.and_then(|o| self.section_at_offset(o)))
            .map(|s| s.index);
        let symbol = address.and_then(|a| self.symbols.lookup(a));
        let (source, frames, unit) = match (address, &self.debug) {
            (Some(a), Some(d)) => (d.location(a), d.frames(a), d.unit_at(a)),
            _ => (None, Vec::new(), None),
        };
        let instruction = address.and_then(|a| self.instruction_at(a));
        let annotation = address.and_then(|a| self.annotation_at(a)).cloned();
        Inspection {
            offset,
            address,
            byte,
            path,
            segment,
            section,
            symbol,
            source,
            frames,
            instruction,
            unit,
            annotation,
        }
    }

    pub fn debug_info(&self) -> Option<&DebugInfo> {
        self.debug.as_ref()
    }

    pub(crate) fn load_embedded_debug_info(&mut self) {
        let data = self.data.clone();
        let Ok(file) = object::File::parse(&*data) else { return };
        if !dwarf::has_dwarf(&file) {
            return;
        }
        if let Ok(Some(debug)) = DebugInfo::load(&file, &data, "embedded", &self.sections) {
            self.summary.has_dwarf = true;
            self.debug = Some(debug);
            self.rebuild_symbols();
        }
    }

    /// Loads DWARF from a separate debug file: a dSYM's DWARF file, an ELF
    /// `.debug` file (from `objcopy --only-keep-debug`), or an unstripped copy.
    pub fn attach_debug_file(&mut self, name: &str, data: impl Into<Arc<[u8]>>) -> Result<()> {
        let mut data: Arc<[u8]> = data.into();
        // dSYMs of universal binaries are universal too: pick our architecture.
        if let Ok(object::FileKind::MachOFat32 | object::FileKind::MachOFat64) = object::FileKind::parse(&*data) {
            let Machine::MachO(cputype) = self.machine else {
                bail!("{name} is a universal Mach-O file, but this binary is not Mach-O")
            };
            let container = crate::Container::parse(data.clone())?;
            let member = container
                .members()
                .iter()
                .find(|m| m.cputype == Some(cputype))
                .ok_or_else(|| crate::Error::new(format!("{name} has no slice for this architecture")))?;
            data = container.member_data(member.index)?;
        }
        let file = object::File::parse(&*data)?;
        let ours = object::File::parse(&*self.data)?;
        if file.architecture() != ours.architecture() {
            bail!(
                "{name} is for {:?}, but this binary is {:?}",
                file.architecture(),
                ours.architecture()
            );
        }
        let id_of = |f: &object::File<'_>| -> Option<Vec<u8>> {
            f.build_id()
                .ok()
                .flatten()
                .map(<[u8]>::to_vec)
                .or_else(|| f.mach_uuid().ok().flatten().map(|u| u.to_vec()))
        };
        if let (Some(a), Some(b)) = (id_of(&ours), id_of(&file))
            && a != b
        {
            bail!("{name} does not match this binary (build ID/UUID differs)");
        }
        if !dwarf::has_dwarf(&file) {
            bail!("{name} contains no DWARF sections");
        }
        let Some(debug) = DebugInfo::load(&file, &data, name, &self.sections)? else {
            bail!("{name} contains no DWARF units");
        };
        self.summary.has_dwarf = true;
        self.debug = Some(debug);
        self.rebuild_symbols();
        Ok(())
    }

    /// Rebuilds the symbol table from its sources: the file's own symbols,
    /// DWARF subprograms (when the file has no function symbols), recovered
    /// function boundaries (`sub_<address>`) and the user's annotations.
    pub(crate) fn rebuild_symbols(&mut self) {
        let mut symbols: Vec<Symbol> = self.base_symbols.clone();
        let mut named: HashSet<u64> = symbols
            .iter()
            .filter(|s| {
                s.defined
                    && matches!(
                        s.kind,
                        SymbolKind::Function | SymbolKind::Data | SymbolKind::Label | SymbolKind::Unknown
                    )
            })
            .map(|s| s.address)
            .collect();
        let section_of = |a: u64| self.section_at(a).map(|s| s.index);
        let has_functions = symbols.iter().any(|s| s.defined && s.kind == SymbolKind::Function);
        if !has_functions && let Some(debug) = &self.debug {
            for (name, address, size) in debug.subprograms() {
                let Some(section) = section_of(address) else { continue };
                if !named.insert(address) {
                    continue;
                }
                symbols.push(Symbol {
                    index: 0,
                    demangled: crate::util::demangle(&name),
                    name,
                    address,
                    size,
                    size_inferred: false,
                    kind: SymbolKind::Function,
                    binding: "global".into(),
                    section: Some(section),
                    source: SymbolSource::Dwarf,
                    defined: true,
                });
            }
        }
        for &(address, size) in &self.discovered {
            if !named.insert(address) {
                continue;
            }
            symbols.push(Symbol {
                index: 0,
                name: format!("sub_{address:x}"),
                demangled: None,
                address,
                size,
                size_inferred: false,
                kind: SymbolKind::Function,
                binding: "local".into(),
                section: section_of(address),
                source: SymbolSource::Discovered,
                defined: true,
            });
        }
        for a in &self.annotations {
            if a.name.is_empty() {
                continue;
            }
            let section = self.section_at(a.address);
            let code = section.is_some_and(|s| s.kind == RegionKind::Code);
            // Renaming a known function keeps its exact extent.
            let size = if a.size > 0 {
                a.size
            } else {
                let known = self
                    .discovered
                    .binary_search_by_key(&a.address, |&(start, _)| start)
                    .ok()
                    .map(|i| self.discovered[i].1);
                known
                    .filter(|&n| n > 0)
                    .or_else(|| {
                        self.base_symbols
                            .iter()
                            .find(|s| s.defined && s.address == a.address && s.size > 0 && !s.size_inferred)
                            .map(|s| s.size)
                    })
                    .unwrap_or(0)
            };
            symbols.push(Symbol {
                index: 0,
                demangled: crate::util::demangle(&a.name),
                name: a.name.clone(),
                address: a.address,
                size,
                size_inferred: false,
                kind: if code { SymbolKind::Function } else { SymbolKind::Data },
                binding: "global".into(),
                section: section.map(|s| s.index),
                source: SymbolSource::User,
                defined: true,
            });
        }
        self.symbols = SymbolTable::new(symbols, &self.sections);
        self.summary.symbol_count = self.symbols.len() as u32;
        self.summary.has_symbols = !self.symbols.is_empty();
        self.coverage = std::sync::OnceLock::new();
    }

    /// Replaces the user's annotations; named ones become symbols.
    pub fn set_annotations(&mut self, mut annotations: Vec<Annotation>) {
        annotations.sort_by_key(|a| (a.address, a.size));
        annotations.dedup_by(|a, b| a.address == b.address && a.size == b.size);
        self.annotations = annotations;
        self.rebuild_symbols();
    }

    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    /// Number of functions recovered from unwind tables / function-start lists.
    pub fn discovered_function_count(&self) -> usize {
        self.discovered.len()
    }

    /// Extent of an annotation: its size, else the symbol or instruction at its address.
    pub(crate) fn annotation_extent(&self, a: &Annotation) -> (u64, u64) {
        if a.size > 0 {
            return (a.address, a.address + a.size);
        }
        if let Some(sym) = self.symbols.at(a.address)
            && sym.size > 0
        {
            return (a.address, a.address + sym.size);
        }
        let len = self.instruction_at(a.address).map_or(1, |i| i.len as u64);
        (a.address, a.address + len)
    }

    /// The smallest annotation covering `address`.
    pub fn annotation_at(&self, address: u64) -> Option<&Annotation> {
        self.annotations
            .iter()
            .filter(|a| {
                let (lo, hi) = self.annotation_extent(a);
                address >= lo && address < hi
            })
            .min_by_key(|a| {
                let (lo, hi) = self.annotation_extent(a);
                hi - lo
            })
    }

    /// Names of the sections that hold DWARF in this binary (for display).
    pub fn dwarf_section_names(&self) -> Vec<String> {
        let Ok(file) = object::File::parse(&*self.data) else {
            return Vec::new();
        };
        file.sections()
            .filter_map(|s| s.name().ok().map(str::to_string))
            .filter(|n| n.starts_with(".debug") || n.starts_with("__debug") || n.starts_with(".zdebug"))
            .collect()
    }
}
