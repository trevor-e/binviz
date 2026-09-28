//! "What is at this location?" — combining layout, symbols, DWARF and code.

use std::sync::Arc;

use object::{Object, ObjectSection};
use serde::Deserialize;

use crate::binary::Binary;
use crate::dwarf::{self, DebugInfo};
use crate::error::{Result, bail};
use crate::layout::Machine;
use crate::model::{Annotation, Inspection, RegionKind, SymbolKind, SymbolSource};
use crate::symbols::{Binding, NewSym};

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
        self.rebuild_static_symbols();
        Ok(())
    }

    /// Rebuilds the symbols that don't come from the file's own tables: DWARF
    /// subprograms (when the file has no function symbols) and recovered
    /// function boundaries (`sub_<address>`), then the user's names on top.
    pub(crate) fn rebuild_static_symbols(&mut self) {
        let file = self.symbols.file_len();
        // Addresses the file already names.
        let mut named: Vec<u64> = self
            .symbols
            .iter()
            .take(file as usize)
            .filter(|s| {
                s.defined
                    && matches!(
                        s.kind,
                        SymbolKind::Function | SymbolKind::Data | SymbolKind::Label | SymbolKind::Unknown
                    )
            })
            .map(|s| s.address)
            .collect();
        named.sort_unstable();
        named.dedup();
        let has_functions = self
            .symbols
            .iter()
            .take(file as usize)
            .any(|s| s.defined && s.kind == SymbolKind::Function);
        let section_of = |a: u64| self.section_at(a).map(|s| s.index);
        let taken = |a: u64, named: &mut Vec<u64>| match named.binary_search(&a) {
            Ok(_) => true,
            Err(pos) => {
                named.insert(pos, a);
                false
            }
        };
        let mut dwarf: Vec<(String, u64, u64, u32)> = Vec::new();
        if !has_functions && let Some(debug) = &self.debug {
            for (name, address, size) in debug.subprograms() {
                let Some(section) = section_of(address) else { continue };
                if !taken(address, &mut named) {
                    dwarf.push((name, address, size, section));
                }
            }
        }
        // Recovered functions go where nothing is named: `named` is sorted, so check by search.
        let recovered: Vec<(u64, u64, Option<u32>)> = self
            .discovered
            .iter()
            .filter(|&&(address, _)| named.binary_search(&address).is_err())
            .map(|&(address, size)| (address, size, section_of(address)))
            .collect();
        let extra = dwarf
            .iter()
            .map(|(name, address, size, section)| NewSym {
                name,
                address: *address,
                size: *size,
                kind: SymbolKind::Function,
                binding: Binding::Global,
                section: Some(*section),
                source: SymbolSource::Dwarf,
                defined: true,
                plain: true,
            })
            .chain(recovered.iter().map(|&(address, size, section)| NewSym {
                // An empty recovered name is written as `sub_<address>`.
                name: "",
                address,
                size,
                kind: SymbolKind::Function,
                binding: Binding::Local,
                section,
                source: SymbolSource::Discovered,
                defined: true,
                plain: true,
            }));
        let sections = std::mem::take(&mut self.sections);
        self.symbols.set_static(extra, &sections);
        self.sections = sections;
        self.rebuild_user_symbols();
    }

    /// Puts the user's named annotations into the symbol table.
    pub(crate) fn rebuild_user_symbols(&mut self) {
        let user: Vec<(&str, u64, u64, Option<u32>, bool)> = self
            .annotations
            .iter()
            .filter(|a| !a.name.is_empty())
            .map(|a| {
                let section = self.section_at(a.address);
                let code = section.is_some_and(|s| s.kind == RegionKind::Code);
                (a.name.as_str(), a.address, a.size, section.map(|s| s.index), code)
            })
            .collect();
        let syms = user.iter().map(|&(name, address, size, section, code)| NewSym {
            name,
            address,
            size,
            kind: if code { SymbolKind::Function } else { SymbolKind::Data },
            binding: Binding::Global,
            section,
            source: SymbolSource::User,
            defined: true,
            plain: false,
        });
        // The table borrows nothing from these; collect first to satisfy the borrow checker.
        let syms: Vec<NewSym<'_>> = syms.collect();
        let sections = &self.sections;
        let table = &mut self.symbols;
        table.set_user(syms, sections);
        self.summary.symbol_count = self.symbols.len() as u32;
        self.summary.has_symbols = !self.symbols.is_empty();
        self.coverage = std::sync::OnceLock::new();
    }

    /// Replaces the user's annotations; named ones become symbols.
    pub fn set_annotations(&mut self, mut annotations: Vec<Annotation>) {
        annotations.sort_by_key(|a| (a.address, a.size));
        annotations.dedup_by(|a, b| a.address == b.address && a.size == b.size);
        self.annotations = annotations;
        self.rebuild_user_symbols();
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
