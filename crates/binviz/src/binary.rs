//! Parsing a binary into the owned model.

use std::collections::HashMap;
use std::sync::Arc;

use object::{
    Architecture, BinaryFormat, Object, ObjectKind, ObjectSection, ObjectSegment, ObjectSymbol, SectionFlags,
    SectionIndex, SectionKind, SegmentFlags, SymbolScope,
};

use crate::error::{Result, bail};
use crate::layout::{self, Builder, Ctx, Layout, Machine};
use crate::model::*;
use crate::symbols::{Binding, Builder as SymbolBuilder, NewSym, SymbolTable};
use crate::util::{self, Bytes, Endian, hex};

/// A parsed binary together with its bytes. Everything is extracted up front
/// except the entries of large tables, which are decoded on demand.
pub struct Binary {
    pub(crate) data: Arc<[u8]>,
    pub(crate) summary: Summary,
    pub(crate) sections: Vec<Section>,
    pub(crate) segments: Vec<Segment>,
    pub(crate) symbols: SymbolTable,
    pub(crate) imports: Vec<Import>,
    pub(crate) exports: Vec<Export>,
    pub(crate) layout: Layout,
    pub(crate) machine: Machine,
    pub(crate) arch: Architecture,
    pub(crate) is64: bool,
    pub(crate) endian: Endian,
    pub(crate) image_base: u64,
    pub(crate) debug: Option<crate::dwarf::DebugInfo>,
    /// Function boundaries recovered from unwind tables, function starts, or the code itself.
    pub(crate) discovered: Vec<(u64, u64)>,
    /// Jump tables found following the code, sorted by address: data, even inside code.
    pub(crate) code_tables: Vec<crate::discover::x86::Table>,
    /// The switches reading them: each indirect jump through a table, sorted by the jump.
    pub(crate) code_switches: Vec<crate::discover::x86::Switch>,
    /// Pieces of functions away from their entries, found following the code:
    /// (start, end, the function's start), sorted.
    pub(crate) code_parts: Vec<(u64, u64, u64)>,
    /// The symbol table of an attached debug file (a dSYM's), for stripped binaries.
    pub(crate) debug_symbols: crate::inspect::DebugSymbols,
    pub(crate) annotations: Vec<Annotation>,
    /// Printable strings found in the loaded sections, built on first use.
    pub(crate) strings: std::sync::OnceLock<crate::strings::StringIndex>,
    /// Coverage runs, rebuilt when symbols or annotations change.
    pub(crate) coverage: std::sync::OnceLock<crate::coverage::CoverageRuns>,
    /// Cross-references, built on first use.
    pub(crate) xrefs: std::sync::OnceLock<crate::xrefs::XrefIndex>,
    /// How pointers in data are stored (plain, chained fixups, relocations).
    pub(crate) pointers: std::sync::OnceLock<crate::pointers::Scheme>,
    /// Objective-C metadata, read on first use.
    pub(crate) objc: std::sync::OnceLock<crate::objc::ObjcInfo>,
    /// The functions summed up to find look-alikes, on first use.
    pub(crate) similar: std::sync::OnceLock<crate::similar::SimilarIndex>,
    /// For a game ROM: its console, memory map and the code found in it.
    pub(crate) rom: Option<crate::rom::Rom>,
}

fn arch_name(a: Architecture) -> String {
    match a {
        Architecture::X86_64 => "x86-64".into(),
        Architecture::X86_64_X32 => "x86-64 (x32)".into(),
        Architecture::I386 => "x86".into(),
        Architecture::Aarch64 => "AArch64".into(),
        Architecture::Aarch64_Ilp32 => "AArch64 (ILP32)".into(),
        Architecture::Arm => "ARM".into(),
        Architecture::Riscv32 => "RISC-V 32".into(),
        Architecture::Riscv64 => "RISC-V 64".into(),
        Architecture::PowerPc => "PowerPC".into(),
        Architecture::PowerPc64 => "PowerPC 64".into(),
        Architecture::Mips => "MIPS".into(),
        Architecture::Mips64 => "MIPS64".into(),
        Architecture::LoongArch64 => "LoongArch64".into(),
        Architecture::S390x => "s390x".into(),
        Architecture::Wasm32 => "WebAssembly".into(),
        other => format!("{other:?}"),
    }
}

fn perm_string(r: bool, w: bool, x: bool) -> String {
    format!(
        "{}{}{}",
        if r { 'r' } else { '-' },
        if w { 'w' } else { '-' },
        if x { 'x' } else { '-' }
    )
}

impl Binary {
    /// Parses ELF, Mach-O (thin), PE or COFF data.
    ///
    /// Fat Mach-O files and archives contain several binaries; open those with
    /// [`crate::Container`] first.
    pub fn parse(data: impl Into<Arc<[u8]>>) -> Result<Binary> {
        let data: Arc<[u8]> = data.into();
        let bytes: &[u8] = &data;
        match object::FileKind::parse(bytes) {
            Ok(object::FileKind::MachOFat32 | object::FileKind::MachOFat64) => {
                bail!("this is a universal (fat) Mach-O binary; pick an architecture first")
            }
            Ok(object::FileKind::Archive) => bail!("this is an archive; pick a member first"),
            Ok(object::FileKind::DyldCache) => bail!("dyld shared caches are not supported"),
            Err(_) if crate::xbe::is_xbe(bytes) => return Binary::from_xbe(data.clone()),
            Err(_) if crate::dwarf::pdb::is_pdb(bytes) => {
                bail!(
                    "this is a PDB, the debug info of a Windows binary: open the .exe or .dll it belongs to, then attach this file to it as its debug file"
                )
            }
            Err(_) => {
                return match crate::rom::detect(bytes) {
                    Some(rom) => Binary::from_rom(data.clone(), rom),
                    None => Ok(Binary::raw(data.clone())),
                };
            }
            _ => {}
        }
        let file = object::File::parse(bytes)?;
        let format = match file.format() {
            BinaryFormat::Elf => Format::Elf,
            BinaryFormat::MachO => Format::MachO,
            BinaryFormat::Pe => Format::Pe,
            BinaryFormat::Coff => Format::Coff,
            BinaryFormat::Xcoff => Format::Xcoff,
            BinaryFormat::Wasm => Format::Wasm,
            _ => Format::Unknown,
        };
        let endian = if file.is_little_endian() {
            Endian::Little
        } else {
            Endian::Big
        };
        // Plain COFF headers don't record the word size; the machine does.
        let is64 = match file.architecture().address_size() {
            Some(size) => size.bytes() == 8,
            None => file.is_64(),
        };
        let b = Bytes::new(bytes, endian);
        let machine = match format {
            Format::Elf => Machine::Elf(b.u16(18).unwrap_or(0)),
            Format::MachO => Machine::MachO(b.u32(4).unwrap_or(0)),
            Format::Pe => {
                let lfanew = b.u32(60).unwrap_or(0) as u64;
                Machine::Pe(b.u16(lfanew + 4).unwrap_or(0))
            }
            Format::Coff => Machine::Pe(b.u16(0).unwrap_or(0)),
            _ => Machine::Other,
        };
        let image_base = if format == Format::Pe {
            file.relative_address_base()
        } else {
            0
        };
        let relocatable = file.kind() == ObjectKind::Relocatable && format != Format::MachO;

        // Sections.
        let mut sections = Vec::new();
        let mut native_index: HashMap<usize, u32> = HashMap::new();
        let mut synthetic_base = 0u64;
        for s in file.sections() {
            let name = s.name().unwrap_or("").to_string();
            let segment_name = s.segment_name().ok().flatten().map(str::to_string);
            let (kind, flags, perms, loaded) = section_facts(format, &s, &name, segment_name.as_deref());
            let mut address = s.address();
            if relocatable && loaded {
                let align = s.align().max(1);
                synthetic_base = util::align_up(synthetic_base, align);
                address = synthetic_base;
                synthetic_base += s.size();
            }
            let compressed = s
                .compressed_file_range()
                .map(|r| r.format != object::CompressionFormat::None)
                .unwrap_or(false);
            let (file_offset, file_size) = match s.compressed_file_range() {
                Ok(r) if compressed => (Some(r.offset), r.compressed_size),
                _ => match s.file_range() {
                    Some((o, sz)) => (Some(o), sz),
                    None => (None, 0),
                },
            };
            native_index.insert(s.index().0, sections.len() as u32);
            sections.push(Section {
                index: sections.len() as u32,
                name,
                segment_name,
                kind,
                address,
                size: s.size(),
                file_offset,
                file_size,
                align: s.align(),
                flags,
                perms,
                compressed,
                segment: None,
                loaded,
            });
        }
        let section_of = |idx: Option<SectionIndex>| idx.and_then(|i| native_index.get(&i.0).copied());

        // Segments.
        let mut segments = match format {
            Format::Elf => elf_segments(&b, is64),
            Format::Pe => pe_segments(&sections, image_base, &b),
            // Objects aren't loaded; their sections are placed individually.
            Format::Coff => Vec::new(),
            _ => file
                .segments()
                .enumerate()
                .map(|(i, seg)| {
                    let (fo, fs) = seg.file_range();
                    let perms = match seg.flags() {
                        SegmentFlags::MachO { initprot, .. } => layout::macho::prot(initprot.0),
                        _ => {
                            let p = seg.permissions();
                            perm_string(p.readable(), p.writable(), p.executable())
                        }
                    };
                    Segment {
                        index: i as u32,
                        name: seg.name().ok().flatten().unwrap_or("").to_string(),
                        kind: if format == Format::MachO {
                            if is64 { "LC_SEGMENT_64" } else { "LC_SEGMENT" }.to_string()
                        } else {
                            "Segment".into()
                        },
                        address: seg.address(),
                        mem_size: seg.size(),
                        file_offset: fo,
                        file_size: fs,
                        align: seg.align(),
                        perms,
                        mapped: true,
                    }
                })
                .collect(),
        };
        for s in sections.iter_mut() {
            if !s.loaded {
                continue;
            }
            s.segment = segments
                .iter()
                .find(|g| g.mapped && s.address >= g.address && s.address < g.address + g.mem_size.max(1))
                .map(|g| g.index);
        }
        if format == Format::MachO {
            // Name unnamed object-file segments.
            for g in segments.iter_mut() {
                if g.name.is_empty() {
                    g.name = "(object)".into();
                }
            }
        }

        // Symbols.
        let arm32 = file.architecture() == Architecture::Arm;
        let mut symbols = SymbolBuilder::default();
        let mut push_symbols = |iter: &mut dyn Iterator<Item = object::Symbol<'_, '_>>, source: SymbolSource| {
            for s in iter {
                let name = s.name().unwrap_or("");
                let kind = match s.kind() {
                    object::SymbolKind::Text => SymbolKind::Function,
                    object::SymbolKind::Data => SymbolKind::Data,
                    object::SymbolKind::Section => SymbolKind::Section,
                    object::SymbolKind::File => SymbolKind::File,
                    object::SymbolKind::Label => SymbolKind::Label,
                    object::SymbolKind::Tls => SymbolKind::Tls,
                    _ => SymbolKind::Unknown,
                };
                if kind == SymbolKind::File && name.is_empty() {
                    continue;
                }
                let section = section_of(s.section_index());
                // Section symbols are unnamed in ELF; name them after their section.
                let name = match (kind, section) {
                    (SymbolKind::Section, Some(i)) if name.is_empty() => sections[i as usize].name.as_str(),
                    _ => name,
                };
                let mut address = s.address();
                if arm32 && kind == SymbolKind::Function {
                    address &= !1;
                }
                if relocatable && let Some(sec) = section.and_then(|i| sections.get(i as usize)) {
                    address = address.wrapping_add(sec.address);
                }
                let binding = if s.is_undefined() {
                    Binding::Undefined
                } else if s.is_weak() {
                    Binding::Weak
                } else if s.is_global() || s.scope() == SymbolScope::Dynamic {
                    Binding::Global
                } else {
                    Binding::Local
                };
                symbols.push(NewSym {
                    name,
                    address,
                    size: s.size(),
                    kind,
                    binding,
                    section,
                    source,
                    defined: s.is_definition() || (!s.is_undefined() && section.is_some()),
                    plain: false,
                });
            }
        };
        push_symbols(&mut file.symbols(), SymbolSource::Symtab);
        push_symbols(&mut file.dynamic_symbols(), SymbolSource::Dynsym);

        // Imports and exports.
        let mut imports = Vec::new();
        if let Ok(list) = file.imports() {
            for imp in list.flatten() {
                let (name, ordinal) = match imp.name() {
                    object::NameOrOrdinal::Name(n) => (util::lossy(n), None),
                    object::NameOrOrdinal::Ordinal(o) => (format!("#{o}"), Some(o as u32)),
                };
                imports.push(Import {
                    library: util::lossy(imp.library()),
                    demangled: util::demangle(&name),
                    name,
                    ordinal,
                    address: None,
                });
            }
        }
        if format == Format::Pe {
            fill_iat_addresses(&b, &sections, image_base, is64, &mut imports);
            for imp in &imports {
                if let Some(addr) = imp.address {
                    symbols.push(NewSym {
                        name: &format!("__imp_{}", imp.name),
                        address: addr,
                        size: if is64 { 8 } else { 4 },
                        kind: SymbolKind::Data,
                        binding: Binding::Global,
                        section: sections
                            .iter()
                            .position(|s| addr >= s.address && addr < s.address + s.size)
                            .map(|i| i as u32),
                        source: SymbolSource::Import,
                        defined: true,
                        plain: false,
                    });
                }
            }
        }
        let mut exports = Vec::new();
        // Without an export trie, `object` lists every external symbol as an export:
        // those are all in the symbol table already.
        let has_exports = format != Format::MachO || macho_has_export_trie(&b, is64);
        if has_exports && let Ok(list) = file.exports() {
            for exp in list.flatten() {
                let (name, ordinal) = match exp.name() {
                    object::NameOrOrdinal::Name(n) => (util::lossy(n), None),
                    object::NameOrOrdinal::Ordinal(o) => (format!("#{o}"), Some(o as u32)),
                };
                let (address, forwarder) = match exp.target() {
                    object::ExportTarget::Address { address } => (address, None),
                    object::ExportTarget::TlvDescriptor { address } => (address, None),
                    object::ExportTarget::Resolver { stub, resolver } => (stub.unwrap_or(resolver), None),
                    object::ExportTarget::Reexport { library, name } => {
                        let target = match name {
                            object::NameOrOrdinal::Name(n) => util::lossy(n),
                            object::NameOrOrdinal::Ordinal(o) => format!("#{o}"),
                        };
                        (
                            0,
                            Some(if library.is_empty() {
                                target
                            } else {
                                format!("{}.{target}", util::lossy(library))
                            }),
                        )
                    }
                    _ => (0, None),
                };
                exports.push(Export {
                    demangled: util::demangle(&name),
                    name,
                    address,
                    ordinal,
                    forwarder,
                });
            }
        }
        // PE images without COFF symbols: exported functions are the best names we have.
        if format == Format::Pe {
            for e in &exports {
                if e.address == 0 || e.forwarder.is_some() {
                    continue;
                }
                let section = sections
                    .iter()
                    .position(|s| e.address >= s.address && e.address < s.address + s.size)
                    .map(|i| i as u32);
                let is_code = section
                    .and_then(|i| sections.get(i as usize))
                    .is_some_and(|s| s.kind == RegionKind::Code);
                symbols.push(NewSym {
                    name: &e.name,
                    address: e.address,
                    size: 0,
                    kind: if is_code {
                        SymbolKind::Function
                    } else {
                        SymbolKind::Data
                    },
                    binding: Binding::Global,
                    section,
                    source: SymbolSource::Export,
                    defined: true,
                    plain: false,
                });
            }
        }
        let stubs = crate::stubs::import_stubs(&file, format, &b, &sections, &imports, is64);
        // Import thunks are functions too, for following the code.
        let mut known = crate::discover::Known {
            entry: Some(file.entry()).filter(|&e| e != 0),
            functions: symbols.function_addresses(),
            imports: &imports,
        };
        known
            .functions
            .extend(stubs.iter().filter(|s| s.code).map(|s| s.address));
        let discovery = crate::discover::discover(&file, format, &b, &sections, &segments, image_base, is64, &known);
        // Import stubs and slots, where the file's own symbols name nothing.
        let named = symbols.defined_addresses();
        for stub in stubs {
            if named.binary_search(&stub.address).is_ok() {
                continue;
            }
            symbols.push(NewSym {
                name: &stub.name,
                address: stub.address,
                size: stub.size,
                kind: if stub.code {
                    SymbolKind::Function
                } else {
                    SymbolKind::Data
                },
                binding: Binding::Global,
                section: sections
                    .iter()
                    .position(|s| s.loaded && stub.address >= s.address && stub.address < s.address + s.size)
                    .map(|i| i as u32),
                source: SymbolSource::Import,
                defined: true,
                plain: false,
            });
        }
        let symbols = symbols.finish_unindexed();

        // Summary.
        let kind = kind_name(&file, format, &b);
        let arch = file.architecture();
        let mut summary = Summary {
            format,
            format_name: format_name(format, is64, &b),
            kind,
            arch: arch_name(arch),
            bits: if is64 { 64 } else { 32 },
            little_endian: endian == Endian::Little,
            file_size: bytes.len() as u64,
            entry: None,
            image_base: (format == Format::Pe).then_some(image_base),
            build_id: None,
            debug_link: None,
            has_dwarf: false,
            has_symbols: !symbols.is_empty(),
            synthetic_addresses: relocatable && sections.iter().any(|s| s.loaded),
            section_count: sections.len() as u32,
            segment_count: segments.len() as u32,
            symbol_count: symbols.len() as u32,
            properties: Vec::new(),
            fingerprint: String::new(),
        };
        let mut entry = file.entry();
        if format == Format::MachO && entry != 0 {
            // LC_MAIN records a file offset; the entry address is where that byte is mapped.
            if let Some(seg) = segments
                .iter()
                .find(|s| s.file_size > 0 && entry >= s.file_offset && entry - s.file_offset < s.file_size)
            {
                entry = seg.address + (entry - seg.file_offset);
            }
        }
        if entry != 0 || file.kind() == ObjectKind::Executable {
            summary.entry = Some(entry);
        }
        if let Ok(Some(id)) = file.build_id() {
            summary.build_id = Some(util::hex_compact(id));
        }
        if let Ok(Some(uuid)) = file.mach_uuid() {
            summary.build_id = Some(util::uuid(&uuid));
        }
        if let Ok(Some((link, crc))) = file.gnu_debuglink() {
            summary.debug_link = Some(format!("{} (crc {crc:#010x})", util::lossy(link)));
        }
        if let Ok(Some(pdb)) = file.pdb_info() {
            summary.build_id = Some(format!("{}{:X}", util::guid(&pdb.guid()).replace('-', ""), pdb.age()));
            if !pdb.path().is_empty() {
                summary.debug_link = Some(util::lossy(pdb.path()));
            }
        }
        summary.properties = properties(&file, format, &b, &segments, &imports);
        if let Some(note) = discovery.note {
            summary.properties.push(Property {
                key: "Code found".into(),
                value: note,
            });
        }
        summary.fingerprint = fingerprint(bytes, summary.build_id.as_deref());

        let mut binary = Binary {
            data: data.clone(),
            summary,
            sections,
            segments,
            symbols,
            imports,
            exports,
            layout: Layout::empty(),
            machine,
            arch,
            is64,
            endian,
            image_base,
            debug: None,
            discovered: discovery.functions,
            code_tables: discovery.tables,
            code_switches: discovery.switches,
            code_parts: discovery.parts,
            debug_symbols: Default::default(),
            annotations: Vec::new(),
            strings: std::sync::OnceLock::new(),
            coverage: std::sync::OnceLock::new(),
            xrefs: std::sync::OnceLock::new(),
            pointers: std::sync::OnceLock::new(),
            objc: std::sync::OnceLock::new(),
            similar: std::sync::OnceLock::new(),
            rom: None,
        };
        drop(file);
        // DWARF first: it may add functions, and the symbol index is built once.
        binary.load_embedded_debug_info();
        binary.rebuild_static_symbols();
        binary.layout = binary.build_layout(format);
        Ok(binary)
    }

    /// Bytes in no format binviz recognizes: one region, for looking at them
    /// byte by byte, as tiles or as text.
    pub fn raw(data: impl Into<Arc<[u8]>>) -> Binary {
        let data: Arc<[u8]> = data.into();
        let len = data.len() as u64;
        let sections = vec![Section {
            index: 0,
            name: "Data".into(),
            segment_name: None,
            kind: RegionKind::Unknown,
            address: 0,
            size: len,
            file_offset: Some(0),
            file_size: len,
            align: 1,
            flags: String::new(),
            perms: String::new(),
            compressed: false,
            segment: None,
            loaded: false,
        }];
        let summary = Summary {
            format: Format::Unknown,
            format_name: "raw data".into(),
            kind: "Unrecognized file".into(),
            arch: "unknown".into(),
            bits: 0,
            little_endian: true,
            file_size: len,
            entry: None,
            image_base: None,
            build_id: None,
            debug_link: None,
            has_dwarf: false,
            has_symbols: false,
            synthetic_addresses: false,
            section_count: 1,
            segment_count: 0,
            symbol_count: 0,
            properties: vec![Property {
                key: "Format".into(),
                value: "not one binviz recognizes: shown as raw bytes (the hex view, tiles and text still work)".into(),
            }],
            fingerprint: fingerprint(&data, None),
        };
        let mut binary = Binary {
            data: data.clone(),
            summary,
            sections,
            segments: Vec::new(),
            symbols: SymbolBuilder::default().finish(&[]),
            imports: Vec::new(),
            exports: Vec::new(),
            layout: Layout::empty(),
            machine: Machine::Other,
            arch: Architecture::Unknown,
            is64: false,
            endian: Endian::Little,
            image_base: 0,
            debug: None,
            discovered: Vec::new(),
            code_tables: Vec::new(),
            code_switches: Vec::new(),
            code_parts: Vec::new(),
            debug_symbols: Default::default(),
            annotations: Vec::new(),
            strings: std::sync::OnceLock::new(),
            coverage: std::sync::OnceLock::new(),
            xrefs: std::sync::OnceLock::new(),
            pointers: std::sync::OnceLock::new(),
            objc: std::sync::OnceLock::new(),
            similar: std::sync::OnceLock::new(),
            rom: None,
        };
        binary.layout = binary.build_layout(Format::Unknown);
        binary
    }

    pub(crate) fn build_layout(&self, format: Format) -> Layout {
        let mut b = Builder::new(self.ctx());
        if let Some(rom) = &self.rom {
            rom.build_layout(&mut b);
            return b.finish();
        }
        match format {
            Format::Elf => layout::elf::build(&mut b),
            Format::MachO => layout::macho::build(&mut b),
            Format::Pe => layout::pe::build(&mut b, true),
            Format::Coff => layout::pe::build(&mut b, false),
            Format::Xbe => layout::xbe::build(&mut b),
            _ => {}
        }
        b.finish()
    }

    pub(crate) fn ctx(&self) -> Ctx<'_> {
        Ctx {
            bytes: Bytes::new(&self.data, self.endian),
            machine: self.machine,
            is64: self.is64,
            strtab: None,
            image_base: self.image_base,
            sections: &self.sections,
            symbols: Some(&self.symbols),
            dwarf: self.debug.as_ref(),
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn summary(&self) -> &Summary {
        &self.summary
    }

    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn symbols(&self) -> &SymbolTable {
        &self.symbols
    }

    pub fn imports(&self) -> &[Import] {
        &self.imports
    }

    pub fn exports(&self) -> &[Export] {
        &self.exports
    }

    pub fn architecture(&self) -> Architecture {
        self.arch
    }

    /// Regions directly below `parent` (or the top-level regions).
    pub fn regions(&self, parent: Option<u32>) -> Vec<RegionInfo> {
        let ctx = self.ctx();
        self.layout
            .children(parent)
            .iter()
            .filter_map(|&id| self.layout.info(&ctx, id))
            .collect()
    }

    pub fn region(&self, id: u32) -> Option<RegionInfo> {
        self.layout.info(&self.ctx(), id)
    }

    /// Entries of a table-like region, decoded on demand.
    pub fn region_entries(&self, id: u32, first: u32, count: u32) -> Vec<PathEntry> {
        self.layout.entries(&self.ctx(), id, first, count)
    }

    /// What the byte at `offset` is, from the outermost region inwards.
    pub fn describe_offset(&self, offset: u64) -> Vec<PathEntry> {
        self.layout.describe(&self.ctx(), offset)
    }

    /// Coloured spans for a range of file offsets.
    pub fn spans(&self, start: u64, end: u64) -> Vec<Span> {
        let end = end.min(self.data.len() as u64);
        if start >= end {
            return Vec::new();
        }
        self.layout.spans(&self.ctx(), start..end)
    }

    /// Dominant region kind for each of `buckets` equal slices of the file.
    pub fn file_map(&self, buckets: u32) -> Vec<RegionKind> {
        self.layout.kind_map(self.data.len() as u64, buckets)
    }

    /// Bytes per region kind over the whole file, largest first.
    pub fn composition(&self) -> Vec<(RegionKind, u64)> {
        self.layout.composition()
    }

    /// Shannon entropy (0..=1, normalised from bits per byte) of each slice of the file.
    /// Large slices are sampled (8 evenly spaced 2 KiB blocks), so the cost
    /// doesn't grow with the file.
    pub fn entropy_map(&self, buckets: u32) -> Vec<f32> {
        const BLOCK: usize = 2048;
        const BLOCKS: usize = 8;
        let len = self.data.len();
        let buckets = (buckets as usize).clamp(1, len.max(1));
        (0..buckets)
            .map(|i| {
                let s = i * len / buckets;
                let e = ((i + 1) * len / buckets).max(s + 1).min(len);
                let span = &self.data[s..e];
                if span.len() <= BLOCK * BLOCKS {
                    return entropy(span);
                }
                let mut counts = [0u32; 256];
                for k in 0..BLOCKS {
                    let start = k * (span.len() - BLOCK) / (BLOCKS - 1);
                    for &b in &span[start..start + BLOCK] {
                        counts[b as usize] += 1;
                    }
                }
                entropy_of(&counts, (BLOCK * BLOCKS) as f32)
            })
            .collect()
    }

    /// The mapped segment (or, for objects, section) containing a virtual address.
    pub fn segment_at(&self, address: u64) -> Option<&Segment> {
        self.segments
            .iter()
            .find(|s| s.mapped && address >= s.address && address - s.address < s.mem_size)
    }

    pub fn section_at(&self, address: u64) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| s.loaded && address >= s.address && address - s.address < s.size.max(1))
    }

    /// Section whose file bytes contain `offset`.
    pub fn section_at_offset(&self, offset: u64) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| s.file_offset.is_some_and(|o| offset >= o && offset - o < s.file_size))
    }

    /// File offset backing a virtual address, if any (bss has none).
    pub fn address_to_offset(&self, address: u64) -> Option<u64> {
        if let Some(seg) = self.segment_at(address) {
            let delta = address - seg.address;
            return (delta < seg.file_size).then(|| seg.file_offset + delta);
        }
        let sec = self.section_at(address)?;
        let delta = address - sec.address;
        (delta < sec.file_size && !sec.compressed)
            .then(|| sec.file_offset.map(|o| o + delta))
            .flatten()
    }

    /// Virtual address a file offset is loaded at, if it is mapped.
    pub fn offset_to_address(&self, offset: u64) -> Option<u64> {
        let mapped: Vec<&Segment> = self.segments.iter().filter(|s| s.mapped).collect();
        if !mapped.is_empty() {
            return mapped
                .iter()
                .find(|s| {
                    offset >= s.file_offset && offset - s.file_offset < s.file_size.min(s.mem_size.max(s.file_size))
                })
                .map(|s| s.address + (offset - s.file_offset));
        }
        let sec = self.section_at_offset(offset)?;
        (sec.loaded && !sec.compressed).then(|| sec.address + (offset - sec.file_offset.unwrap_or(0)))
    }
}

impl Layout {
    pub(crate) fn empty() -> Layout {
        Builder::new(Ctx {
            bytes: Bytes::new(&[], Endian::Little),
            machine: Machine::Other,
            is64: false,
            strtab: None,
            image_base: 0,
            sections: &[],
            symbols: None,
            dwarf: None,
        })
        .finish()
    }
}

/// FNV-1a over the length, the build ID, the first and last 64 KiB, and 64
/// evenly spaced 1 KiB blocks: identifies a file without reading all of it.
pub(crate) fn fingerprint(data: &[u8], build_id: Option<&str>) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    eat(&(data.len() as u64).to_le_bytes());
    eat(build_id.unwrap_or("").as_bytes());
    let edge = data.len().min(64 * 1024);
    eat(&data[..edge]);
    eat(&data[data.len() - edge..]);
    const BLOCK: usize = 1024;
    if data.len() > BLOCK {
        for k in 0..64 {
            let start = k * (data.len() - BLOCK) / 63;
            eat(&data[start..start + BLOCK]);
        }
    }
    format!("{h:016x}-{:x}", data.len())
}

pub(crate) fn entropy(data: &[u8]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0u32; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    entropy_of(&counts, data.len() as f32)
}

/// Shannon entropy of a byte histogram over `n` bytes, scaled to 0..1.
fn entropy_of(counts: &[u32; 256], n: f32) -> f32 {
    let h: f32 = counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f32 / n;
            -p * p.log2()
        })
        .sum();
    (h / 8.0).clamp(0.0, 1.0)
}

fn section_facts(
    format: Format,
    s: &object::Section<'_, '_>,
    name: &str,
    segment: Option<&str>,
) -> (RegionKind, String, String, bool) {
    match s.flags() {
        SectionFlags::Elf { sh_type, sh_flags } => {
            let kind = layout::elf::classify(name, sh_type.0, sh_flags.0);
            let alloc = sh_flags.0 & object::elf::SHF_ALLOC.0 != 0;
            let perms = if alloc {
                perm_string(
                    true,
                    sh_flags.0 & object::elf::SHF_WRITE.0 != 0,
                    sh_flags.0 & object::elf::SHF_EXECINSTR.0 != 0,
                )
            } else {
                "---".into()
            };
            (
                kind,
                layout::elf::flag_string(object::elf::SectionFlags::NAMES, sh_flags),
                perms,
                alloc,
            )
        }
        SectionFlags::MachO { flags, .. } => {
            let kind = layout::macho::classify(segment.unwrap_or(""), name, flags.0);
            let debug = kind == RegionKind::Debug;
            let exec =
                flags.0 & (object::macho::S_ATTR_PURE_INSTRUCTIONS.0 | object::macho::S_ATTR_SOME_INSTRUCTIONS.0) != 0;
            let write = matches!(segment, Some("__DATA" | "__DATA_DIRTY" | "__AUTH" | "__OBJC"));
            let perms = if debug {
                "---".into()
            } else {
                perm_string(true, write, exec)
            };
            (
                kind,
                layout::elf::flag_string(object::macho::SectionFlags::NAMES, flags),
                perms,
                !debug,
            )
        }
        SectionFlags::Coff { characteristics } => {
            let c = characteristics.0;
            let has_raw = s.file_range().is_some();
            let kind = layout::pe::classify(name, c, has_raw);
            let perms = perm_string(
                c & object::pe::IMAGE_SCN_MEM_READ.0 != 0,
                c & object::pe::IMAGE_SCN_MEM_WRITE.0 != 0,
                c & object::pe::IMAGE_SCN_MEM_EXECUTE.0 != 0,
            );
            let loaded = if format == Format::Pe {
                true
            } else {
                c & object::pe::IMAGE_SCN_LNK_REMOVE.0 == 0
                    && kind != RegionKind::Debug
                    && c & object::pe::IMAGE_SCN_LNK_INFO.0 == 0
            };
            (
                kind,
                layout::elf::flag_string(object::pe::SectionFlags::NAMES, characteristics),
                perms,
                loaded,
            )
        }
        _ => {
            let kind = match s.kind() {
                SectionKind::Text => RegionKind::Code,
                SectionKind::Data => RegionKind::Data,
                SectionKind::ReadOnlyData | SectionKind::ReadOnlyString => RegionKind::Rodata,
                SectionKind::UninitializedData => RegionKind::Bss,
                SectionKind::Debug | SectionKind::DebugString => RegionKind::Debug,
                _ => RegionKind::Metadata,
            };
            (kind, String::new(), "r--".into(), true)
        }
    }
}

fn elf_segments(b: &Bytes, is64: bool) -> Vec<Segment> {
    let (phoff, phentsize, phnum) = if is64 {
        (
            b.u64(32).unwrap_or(0),
            b.u16(54).unwrap_or(0) as u64,
            b.u16(56).unwrap_or(0) as u64,
        )
    } else {
        (
            b.u32(28).unwrap_or(0) as u64,
            b.u16(42).unwrap_or(0) as u64,
            b.u16(44).unwrap_or(0) as u64,
        )
    };
    let machine = object::elf::Machine(b.u16(18).unwrap_or(0));
    let names = object::elf::machine_names(machine);
    let mut out = Vec::new();
    for i in 0..phnum.min(65535) {
        let o = phoff + i * phentsize;
        let rec = if is64 {
            (|| {
                Some((
                    b.u32(o)?,
                    b.u32(o + 4)?,
                    b.u64(o + 8)?,
                    b.u64(o + 16)?,
                    b.u64(o + 32)?,
                    b.u64(o + 40)?,
                    b.u64(o + 48)?,
                ))
            })()
        } else {
            (|| {
                Some((
                    b.u32(o)?,
                    b.u32(o + 24)?,
                    b.u32(o + 4)? as u64,
                    b.u32(o + 8)? as u64,
                    b.u32(o + 16)? as u64,
                    b.u32(o + 20)? as u64,
                    b.u32(o + 28)? as u64,
                ))
            })()
        };
        let Some((p_type, p_flags, offset, vaddr, filesz, memsz, align)) = rec else {
            break;
        };
        let type_name = names
            .pt
            .name(object::elf::ProgramType(p_type))
            .map_or_else(|| hex(p_type as u64), str::to_string);
        let load = p_type == object::elf::PT_LOAD.0;
        out.push(Segment {
            index: i as u32,
            name: if load {
                format!("LOAD #{i}")
            } else {
                type_name.trim_start_matches("PT_").to_string()
            },
            kind: type_name,
            address: vaddr,
            mem_size: memsz,
            file_offset: offset,
            file_size: filesz,
            align,
            perms: layout::elf::perms(p_flags),
            mapped: load,
        });
    }
    out
}

fn pe_segments(sections: &[Section], image_base: u64, b: &Bytes) -> Vec<Segment> {
    let lfanew = b.u32(60).unwrap_or(0) as u64;
    // SizeOfHeaders sits at the same offset in PE32 and PE32+ optional headers.
    let size_of_headers = b.u32(lfanew + 24 + 60).unwrap_or(0) as u64;
    let mut out = vec![Segment {
        index: 0,
        name: "Headers".into(),
        kind: "Headers".into(),
        address: image_base,
        mem_size: size_of_headers,
        file_offset: 0,
        file_size: size_of_headers.min(b.data.len() as u64),
        align: 0,
        perms: "r--".into(),
        mapped: true,
    }];
    for s in sections {
        out.push(Segment {
            index: out.len() as u32,
            name: s.name.clone(),
            kind: "Section".into(),
            address: s.address,
            mem_size: s.size.max(s.file_size),
            file_offset: s.file_offset.unwrap_or(0),
            file_size: if s.file_offset.is_some() { s.file_size } else { 0 },
            align: s.align,
            perms: s.perms.clone(),
            mapped: true,
        });
    }
    out
}

/// PE imports: find the IAT slot address for each import by walking the import descriptors.
fn fill_iat_addresses(b: &Bytes, sections: &[Section], image_base: u64, is64: bool, imports: &mut [Import]) {
    let lfanew = b.u32(60).unwrap_or(0) as u64;
    let opt = lfanew + 24;
    let dir_base = opt + if is64 { 112 } else { 96 };
    let import_rva = b.u32(dir_base + 8).unwrap_or(0) as u64;
    let rva_to_off = |rva: u64| -> Option<u64> {
        let va = image_base + rva;
        sections.iter().find_map(|s| {
            let o = s.file_offset?;
            (va >= s.address && va - s.address < s.file_size).then(|| o + (va - s.address))
        })
    };
    let Some(mut desc) = rva_to_off(import_rva) else { return };
    let ptr = if is64 { 8 } else { 4 };
    let mut by_key: HashMap<(String, String), u64> = HashMap::new();
    for _ in 0..4096 {
        let (Some(ilt), Some(name_rva), Some(iat)) = (b.u32(desc), b.u32(desc + 12), b.u32(desc + 16)) else {
            break;
        };
        if name_rva == 0 && iat == 0 {
            break;
        }
        let dll = rva_to_off(name_rva as u64)
            .and_then(|o| b.cstr(o, 512))
            .map(util::lossy)
            .unwrap_or_default();
        let lookup = if ilt != 0 { ilt } else { iat } as u64;
        if let Some(mut t) = rva_to_off(lookup) {
            let mut i = 0u64;
            loop {
                let v = b.uint(t, ptr as u32).unwrap_or(0);
                if v == 0 || i > 65536 {
                    break;
                }
                let flag = if is64 { 1u64 << 63 } else { 1u64 << 31 };
                let name = if v & flag != 0 {
                    format!("#{}", v & 0xffff)
                } else {
                    rva_to_off(v & 0x7fff_ffff)
                        .and_then(|h| b.cstr(h + 2, 4096))
                        .map(util::lossy)
                        .unwrap_or_default()
                };
                by_key.insert((dll.to_lowercase(), name), image_base + iat as u64 + i * ptr);
                t += ptr;
                i += 1;
            }
        }
        desc += 20;
    }
    for imp in imports {
        imp.address = by_key.get(&(imp.library.to_lowercase(), imp.name.clone())).copied();
    }
}

/// Whether a Mach-O file has an export trie (`LC_DYLD_INFO` or `LC_DYLD_EXPORTS_TRIE`).
fn macho_has_export_trie(b: &Bytes, is64: bool) -> bool {
    let ncmds = b.u32(16).unwrap_or(0);
    let mut off: u64 = if is64 { 32 } else { 28 };
    for _ in 0..ncmds.min(65536) {
        let (Some(cmd), Some(size)) = (b.u32(off), b.u32(off + 4)) else {
            break;
        };
        let exports = match cmd {
            c if c == object::macho::LC_DYLD_INFO.0 || c == object::macho::LC_DYLD_INFO_ONLY.0 => b.u32(off + 44),
            c if c == object::macho::LC_DYLD_EXPORTS_TRIE.0 => b.u32(off + 12),
            _ => None,
        };
        if exports.is_some_and(|n| n > 0) {
            return true;
        }
        if size < 8 {
            break;
        }
        off += size as u64;
    }
    false
}

fn kind_name(file: &object::File<'_>, format: Format, b: &Bytes) -> String {
    match format {
        Format::MachO => {
            let ft = object::macho::FileType(b.u32(12).unwrap_or(0));
            match ft {
                object::macho::MH_OBJECT => "Relocatable object",
                object::macho::MH_EXECUTE => "Executable",
                object::macho::MH_DYLIB => "Dynamic library",
                object::macho::MH_BUNDLE => "Bundle",
                object::macho::MH_DSYM => "Debug symbols (dSYM)",
                object::macho::MH_DYLINKER => "Dynamic linker",
                object::macho::MH_CORE => "Core dump",
                object::macho::MH_KEXT_BUNDLE => "Kernel extension",
                object::macho::MH_FILESET => "File set",
                _ => "Mach-O file",
            }
            .into()
        }
        Format::Pe => {
            let lfanew = b.u32(60).unwrap_or(0) as u64;
            let chars = b.u16(lfanew + 22).unwrap_or(0);
            if chars & object::pe::IMAGE_FILE_DLL.0 != 0 {
                "DLL"
            } else {
                "Executable"
            }
            .into()
        }
        Format::Elf => {
            let segs = elf_segments(b, file.is_64());
            let has = |kind: &str| segs.iter().any(|s| s.kind == kind);
            match file.kind() {
                ObjectKind::Relocatable => "Relocatable object".into(),
                ObjectKind::Executable => "Executable".into(),
                ObjectKind::Dynamic if has("PT_INTERP") => "PIE executable".into(),
                // No interpreter: a shared library, or a static PIE that relocates itself.
                ObjectKind::Dynamic if file.entry() != 0 && file.symbol_by_name("_start").is_some() => {
                    "Static PIE executable".into()
                }
                ObjectKind::Dynamic => "Shared library".into(),
                ObjectKind::Core => "Core dump".into(),
                _ => "ELF file".into(),
            }
        }
        _ => match file.kind() {
            ObjectKind::Relocatable => "Relocatable object".into(),
            ObjectKind::Executable => "Executable".into(),
            ObjectKind::Dynamic => "Shared library".into(),
            ObjectKind::Core => "Core dump".into(),
            _ => "Unknown".into(),
        },
    }
}

fn format_name(format: Format, is64: bool, b: &Bytes) -> String {
    match format {
        Format::Elf => if is64 { "ELF64" } else { "ELF32" }.into(),
        Format::MachO => if is64 { "Mach-O 64-bit" } else { "Mach-O 32-bit" }.into(),
        Format::Pe => {
            let lfanew = b.u32(60).unwrap_or(0) as u64;
            if b.u16(lfanew + 24) == Some(0x20b) {
                "PE32+"
            } else {
                "PE32"
            }
            .into()
        }
        Format::Coff => "COFF object".into(),
        Format::Xcoff => "XCOFF".into(),
        Format::Wasm => "WebAssembly".into(),
        Format::Rom => "ROM".into(),
        Format::Xbe => "XBE".into(),
        Format::Unknown => "Unknown".into(),
    }
}

fn prop(props: &mut Vec<Property>, key: &str, value: impl Into<String>) {
    let value = value.into();
    if !value.is_empty() {
        props.push(Property { key: key.into(), value });
    }
}

fn properties(
    file: &object::File<'_>,
    format: Format,
    b: &Bytes,
    segments: &[Segment],
    imports: &[Import],
) -> Vec<Property> {
    let mut p = Vec::new();
    match format {
        Format::Elf => {
            if let object::FileFlags::Elf {
                os_abi,
                abi_version,
                e_flags,
            } = file.flags()
            {
                prop(
                    &mut p,
                    "OS/ABI",
                    os_abi.name().map_or_else(|| os_abi.0.to_string(), str::to_string),
                );
                if abi_version != 0 {
                    prop(&mut p, "ABI version", abi_version.to_string());
                }
                if e_flags.0 != 0 {
                    let names = object::elf::machine_names(object::elf::Machine(b.u16(18).unwrap_or(0)));
                    prop(&mut p, "Flags", layout::elf::flag_string(names.ef, e_flags));
                }
            }
            if let Some(interp) = segments.iter().find(|s| s.kind == "PT_INTERP")
                && let Some(s) = b.cstr(interp.file_offset, interp.file_size)
            {
                prop(&mut p, "Interpreter", util::lossy(s));
            }
            if let Some(stack) = segments.iter().find(|s| s.kind == "PT_GNU_STACK") {
                prop(
                    &mut p,
                    "Stack",
                    if stack.perms.contains('x') {
                        "executable"
                    } else {
                        "non-executable (NX)"
                    },
                );
            }
            if segments.iter().any(|s| s.kind == "PT_GNU_RELRO") {
                prop(&mut p, "RELRO", "yes");
            }
            if let Ok(libs) = file.import_libraries() {
                let libs: Vec<String> = libs.flatten().map(|l| util::lossy(l.name())).collect();
                prop(&mut p, "Needed libraries", libs.join(", "));
            }
        }
        Format::MachO => {
            if let object::FileFlags::MachO { flags } = file.flags() {
                prop(
                    &mut p,
                    "Flags",
                    layout::elf::flag_string(object::macho::FileFlags::NAMES, flags),
                );
            }
            if let Ok(libs) = file.import_libraries() {
                let libs: Vec<String> = libs.flatten().map(|l| util::lossy(l.name())).collect();
                prop(&mut p, "Linked dylibs", libs.join(", "));
            }
            if let object::File::MachO64(m) = file
                && let Ok(Some((bv, _tools))) = m.build_version()
            {
                prop(
                    &mut p,
                    "Platform",
                    bv.platform.get(m.endian()).name().unwrap_or("?").to_string(),
                );
                prop(&mut p, "Minimum OS", util::macho_version(bv.minos.get(m.endian()).0));
                prop(&mut p, "SDK", util::macho_version(bv.sdk.get(m.endian()).0));
            }
            let map = file.object_map();
            if !map.objects().is_empty() {
                prop(
                    &mut p,
                    "Debug map",
                    format!(
                        "{} object file{} named by stabs (its DWARF is there, unless in a dSYM)",
                        map.objects().len(),
                        if map.objects().len() == 1 { "" } else { "s" }
                    ),
                );
            }
        }
        Format::Pe | Format::Coff => {
            let base = if format == Format::Pe {
                b.u32(60).unwrap_or(0) as u64 + 4
            } else {
                0
            };
            let ts = b.u32(base + 4).unwrap_or(0) as u64;
            prop(&mut p, "Timestamp", util::unix_time(ts));
            if format == Format::Pe {
                let opt = base + 20;
                prop(
                    &mut p,
                    "Linker version",
                    format!("{}.{}", b.u8(opt + 2).unwrap_or(0), b.u8(opt + 3).unwrap_or(0)),
                );
                let sub = b.u16(opt + 68).unwrap_or(0);
                prop(
                    &mut p,
                    "Subsystem",
                    object::pe::Subsystem(sub)
                        .name()
                        .unwrap_or("?")
                        .trim_start_matches("IMAGE_SUBSYSTEM_")
                        .to_string(),
                );
                let dll = b.u16(opt + 70).unwrap_or(0);
                prop(
                    &mut p,
                    "DLL characteristics",
                    layout::elf::flag_string(object::pe::DllFlags::NAMES, object::pe::DllFlags(dll)),
                );
                let os = (b.u16(opt + 40).unwrap_or(0), b.u16(opt + 42).unwrap_or(0));
                prop(&mut p, "OS version", format!("{}.{}", os.0, os.1));
                // The Rich header names the tools that made the objects linked in.
                if let Some(rich) = object::read::pe::RichHeaderInfo::parse(b.data, base - 4) {
                    let tools: Vec<layout::rich::Tool> = rich
                        .unmasked_entries()
                        .map(|e| layout::rich::tool((e.comp_id >> 16) as u16, e.comp_id as u16, e.count))
                        .collect();
                    if let Some(compiler) = layout::rich::summary(&tools) {
                        prop(&mut p, "Compiler", compiler);
                    }
                }
                let mut dlls: Vec<&str> = imports.iter().map(|i| i.library.as_str()).collect();
                dlls.dedup();
                prop(&mut p, "Imported DLLs", dlls.join(", "));
            }
            let chars = b.u16(base + 18).unwrap_or(0);
            prop(
                &mut p,
                "Characteristics",
                layout::elf::flag_string(object::pe::FileFlags::NAMES, object::pe::FileFlags(chars)),
            );
        }
        _ => {}
    }
    p
}
