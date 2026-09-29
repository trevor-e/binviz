//! "What is at this location?" — combining layout, symbols, DWARF and code.

use std::sync::Arc;

use object::{Object, ObjectSection, ObjectSymbol};
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
        let global = address.filter(|_| self.xrefs_ready()).and_then(|a| self.global_at(a));
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
            global,
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
        // A PDB: read into DWARF.
        if dwarf::pdb::is_pdb(&data) {
            return self.attach_pdb(name, &data);
        }
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
        let debug = if dwarf::has_dwarf(&file) {
            DebugInfo::load(&file, &data, name, &self.sections)?
        } else {
            None
        };
        // The debug file's symbol table names what stripping removed (a dSYM
        // keeps every symbol the linker wrote, data included).
        let mut syms = DebugSymbols::default();
        for s in file.symbols() {
            let kind = match s.kind() {
                object::SymbolKind::Text => SymbolKind::Function,
                object::SymbolKind::Data => SymbolKind::Data,
                object::SymbolKind::Unknown => SymbolKind::Unknown,
                _ => continue,
            };
            let Ok(n) = s.name() else { continue };
            if n.is_empty() || s.section_index().is_none() || s.is_undefined() || s.address() == 0 {
                continue;
            }
            syms.push(n, s.address(), s.size(), kind);
        }
        if debug.is_none() && syms.recs.is_empty() {
            bail!("{name} contains neither DWARF nor symbols");
        }
        self.debug_symbols = syms;
        if let Some(debug) = debug {
            self.summary.has_dwarf = true;
            self.debug = Some(debug);
        }
        self.rebuild_static_symbols();
        Ok(())
    }

    /// Attaches a PDB (Microsoft's debug info): its modules, functions, globals
    /// and line records read into DWARF, its procedures and public symbols
    /// naming what the binary doesn't.
    fn attach_pdb(&mut self, name: &str, data: &[u8]) -> Result<()> {
        if self.summary.format != crate::model::Format::Pe {
            bail!("{name} is a PDB, the debug info of Windows binaries; this binary isn't one");
        }
        let converted = dwarf::pdb::convert(data, self.image_base)?;
        // The GUID says which build a PDB is for (its age only how often it was written).
        let Some(ours) = &self.summary.build_id else {
            bail!("this binary names no PDB (it has no CodeView record), so {name} can't be matched to it");
        };
        if ours.get(..32) != converted.id.get(..32) {
            bail!(
                "{name} does not match this binary (its GUID differs: this binary wants {ours}, the PDB is {})",
                converted.id
            );
        }
        let debug = if converted.modules > 0 {
            Some(DebugInfo::from_sections(
                converted.sections,
                gimli::RunTimeEndian::Little,
                name,
                &self.sections,
                self.arch,
            )?)
        } else {
            None
        };
        let mut syms = DebugSymbols::default();
        for (n, address, size, kind) in &converted.symbols {
            syms.push(n, *address, *size, *kind);
        }
        if debug.is_none() && syms.recs.is_empty() {
            bail!("{name} contains neither modules nor symbols for this binary");
        }
        self.debug_symbols = syms;
        if let Some(debug) = debug {
            self.summary.has_dwarf = true;
            self.debug = Some(debug);
        }
        self.rebuild_static_symbols();
        Ok(())
    }

    /// Rebuilds the symbols that don't come from the file's own tables: an
    /// attached debug file's symbols, DWARF subprograms (when the file has no
    /// function symbols), names from Objective-C metadata and recovered
    /// function boundaries (`sub_<address>`), each only where nothing before
    /// it names the address; then the user's names on top.
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
        // Each layer only fills addresses the layers before it leave unnamed.
        // (`named` stays sorted: new addresses are merged in per layer, not one by one.)
        let merge = |named: &mut Vec<u64>, more: &mut Vec<u64>| {
            more.sort_unstable();
            more.dedup();
            named.extend_from_slice(more);
            named.sort_unstable();
            named.dedup();
        };
        let ds = &self.debug_symbols;
        let from_debug_file: Vec<(usize, u32)> = (0..ds.recs.len())
            .filter(|&i| named.binary_search(&ds.recs[i].address).is_err())
            .filter_map(|i| Some((i, section_of(ds.recs[i].address)?)))
            .collect();
        merge(
            &mut named,
            &mut from_debug_file.iter().map(|&(i, _)| ds.recs[i].address).collect(),
        );
        let mut dwarf: Vec<(String, u64, u64, u32)> = Vec::new();
        if !has_functions && let Some(debug) = &self.debug {
            for (name, address, size) in debug.subprograms() {
                if named.binary_search(&address).is_err()
                    && let Some(section) = section_of(address)
                {
                    dwarf.push((name, address, size, section));
                }
            }
            // One per address (a function can have several subprogram DIEs).
            dwarf.sort_by_key(|d| d.1);
            dwarf.dedup_by_key(|d| d.1);
        }
        merge(&mut named, &mut dwarf.iter().map(|d| d.1).collect());
        // Objective-C methods, metadata and selector references.
        let objc: Vec<(&str, u64, u64, bool, u32)> = self
            .objc
            .get_or_init(|| crate::objc::parse(self))
            .names
            .iter()
            .filter(|n| named.binary_search(&n.address).is_err())
            .filter_map(|n| {
                // A method runs to the next function start, when the image lists them.
                let size = match n.size {
                    0 if n.code => self
                        .discovered
                        .binary_search_by_key(&n.address, |d| d.0)
                        .map_or(0, |i| self.discovered[i].1),
                    size => size,
                };
                Some((n.name.as_str(), n.address, size, n.code, section_of(n.address)?))
            })
            .collect();
        merge(&mut named, &mut objc.iter().map(|o| o.1).collect());
        // C++ vtables and the descriptors of classes, from an MSVC binary's RTTI.
        let rtti: Vec<(String, u64, u64, u32)> = self
            .read_rtti()
            .symbols
            .into_iter()
            .filter(|s| named.binary_search(&s.1).is_err())
            .filter_map(|(name, address, size)| Some((name, address, size, section_of(address)?)))
            .collect();
        merge(&mut named, &mut rtti.iter().map(|r| r.1).collect());
        // Recovered functions go where nothing is named: `named` is sorted, so check by search.
        let recovered: Vec<(u64, u64, Option<u32>)> = self
            .discovered
            .iter()
            .filter(|&&(address, _)| named.binary_search(&address).is_err())
            .map(|&(address, size)| (address, size, section_of(address)))
            .collect();
        let debug_file = from_debug_file.iter().map(|&(i, section)| {
            let r = &ds.recs[i];
            NewSym {
                name: ds.name(r),
                address: r.address,
                size: r.size,
                kind: r.kind,
                binding: Binding::Global,
                section: Some(section),
                source: SymbolSource::DebugFile,
                defined: true,
                plain: false,
            }
        });
        let extra = debug_file
            .chain(dwarf.iter().map(|(name, address, size, section)| NewSym {
                name,
                address: *address,
                size: *size,
                kind: SymbolKind::Function,
                binding: Binding::Global,
                section: Some(*section),
                source: SymbolSource::Dwarf,
                defined: true,
                plain: true,
            }))
            .chain(objc.iter().map(|&(name, address, size, code, section)| NewSym {
                name,
                address,
                size,
                kind: if code { SymbolKind::Function } else { SymbolKind::Data },
                binding: Binding::Local,
                section: Some(section),
                source: SymbolSource::Objc,
                defined: true,
                plain: true,
            }))
            .chain(rtti.iter().map(|(name, address, size, section)| NewSym {
                name,
                address: *address,
                size: *size,
                kind: SymbolKind::Data,
                binding: Binding::Local,
                section: Some(*section),
                source: SymbolSource::Rtti,
                defined: true,
                plain: false,
            }))
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
        let extra: Vec<NewSym<'_>> = extra.collect();
        let sections = &self.sections;
        let symbols = &mut self.symbols;
        symbols.set_static(extra, sections);
        // Function boundaries shape the references found in code, and what is summed up.
        self.xrefs = std::sync::OnceLock::new();
        self.similar = std::sync::OnceLock::new();
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
                let code = match a.kind.as_deref() {
                    Some("function") => true,
                    Some("data") => false,
                    // A console mixes code and data in one area: four zero bytes start no function.
                    _ if self.rom.is_some() => {
                        section.is_some_and(|s| s.kind == RegionKind::Code)
                            && !self
                                .code_bytes(a.address)
                                .is_some_and(|b| b.len() >= 4 && b[..4].iter().all(|&x| x == 0))
                    }
                    _ => section.is_some_and(|s| s.kind == RegionKind::Code),
                };
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

/// A debug file's symbol table, kept compactly: names in one arena.
#[derive(Default)]
pub(crate) struct DebugSymbols {
    names: String,
    pub(crate) recs: Vec<DebugSymbol>,
}

pub(crate) struct DebugSymbol {
    pub(crate) address: u64,
    pub(crate) size: u64,
    name: u32,
    len: u32,
    pub(crate) kind: SymbolKind,
}

impl DebugSymbols {
    fn push(&mut self, name: &str, address: u64, size: u64, kind: SymbolKind) {
        let at = self.names.len() as u32;
        self.names.push_str(name);
        self.recs.push(DebugSymbol {
            address,
            size,
            name: at,
            len: name.len() as u32,
            kind,
        });
    }

    fn name(&self, r: &DebugSymbol) -> &str {
        &self.names[r.name as usize..(r.name + r.len) as usize]
    }
}
