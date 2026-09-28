//! Game ROMs and console executables: NES, SNES, Game Boy (Color), Game
//! Boy Advance, Mega Drive / Genesis, Nintendo 64, PlayStation. Each is read into the same model
//! as other binaries: its header decoded field by field; its banks placed at
//! the addresses the console's CPU sees them; its RAM and hardware
//! registers as regions without file bytes, the registers named, so that
//! what reads and writes them shows; the interrupt vectors as entry points;
//! and the code found by following it from them (see [`analysis`]).
//!
//! Banked consoles see only part of their ROM at a time: each bank gets
//! addresses of its own, `bank << 16 | CPU address` (`03:8000`), so that
//! banks sharing a window don't collide.

pub(crate) mod analysis;
mod gb;
mod gba;
mod megadrive;
mod n64;
mod nes;
mod psx;
mod snes;

use std::sync::Arc;

use serde::Serialize;

use crate::binary::Binary;
use crate::cpu::{Cpu, State};
use crate::error::Result;
use crate::layout::{Builder, Machine};
use crate::model::{Format, Property, RegionKind, Section, Segment, Summary, SymbolKind, SymbolSource};
use crate::symbols::{Binding, Builder as SymbolBuilder, NewSym};
use crate::util::Endian;

/// A console.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    Nes,
    Snes,
    GameBoy,
    GameBoyColor,
    GameBoyAdvance,
    MegaDrive,
    Nintendo64,
    PlayStation,
}

impl Platform {
    pub fn name(self) -> &'static str {
        match self {
            Platform::Nes => "NES",
            Platform::Snes => "SNES",
            Platform::GameBoy => "Game Boy",
            Platform::GameBoyColor => "Game Boy Color",
            Platform::GameBoyAdvance => "Game Boy Advance",
            Platform::MegaDrive => "Mega Drive / Genesis",
            Platform::Nintendo64 => "Nintendo 64",
            Platform::PlayStation => "PlayStation",
        }
    }
}

/// Part of a banked address space: CPU addresses `lo..hi`, repeating every
/// `size` bytes, where one of `banks` is mapped (each given as our address
/// for `lo`).
#[derive(Debug, Clone)]
pub(crate) struct Window {
    pub lo: u64,
    pub hi: u64,
    pub size: u64,
    pub banks: Vec<u64>,
}

impl Window {
    pub fn fixed(lo: u64, hi: u64, base: u64) -> Window {
        Window {
            lo,
            hi,
            size: hi - lo,
            banks: vec![base],
        }
    }
}

/// How a console's CPU sees memory.
#[derive(Debug, Clone)]
pub(crate) enum Map {
    /// A 16-bit address space some of whose windows switch between banks
    /// (NES, Game Boy): our addresses are `bank << 16 | CPU address`.
    Banked(Vec<Window>),
    /// The SNES's 24-bit space, its mirrors folded onto one address each.
    Snes { hirom: bool },
    /// A flat 32-bit space, its mirrors (cached, uncached, physical) folded.
    Flat(Flat),
}

/// Consoles with a flat address space.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Flat {
    Gba,
    MegaDrive,
    N64,
    Psx,
}

impl Map {
    /// The CPU's view of one of our addresses.
    pub fn cpu(&self, address: u64) -> u64 {
        match self {
            Map::Banked(_) => address & 0xFFFF,
            Map::Snes { .. } => address & 0xFF_FFFF,
            Map::Flat(_) => address & 0xFFFF_FFFF,
        }
    }

    /// Our address for the CPU address `t`, named by code at `from`: in a
    /// window that switches banks, the bank the code is in (when it is one of
    /// them); `None` when that can't be known.
    pub fn resolve(&self, from: u64, t: u64) -> Option<u64> {
        match self {
            Map::Banked(windows) => {
                let t = t & 0xFFFF;
                let w = windows.iter().find(|w| t >= w.lo && t < w.hi)?;
                let offset = (t - w.lo) % w.size.max(1);
                if let [base] = w.banks.as_slice() {
                    return Some(base + offset);
                }
                let own = (from & !0xFFFF) | w.lo;
                w.banks.contains(&own).then_some(own + offset)
            }
            Map::Snes { hirom } => snes::resolve(t & 0xFF_FFFF, *hirom),
            Map::Flat(Flat::Gba) => gba::resolve(t),
            Map::Flat(Flat::MegaDrive) => megadrive::resolve(t),
            Map::Flat(Flat::N64) => n64::resolve(t),
            Map::Flat(Flat::Psx) => psx::resolve(t),
        }
    }
}

/// A region of memory: ROM bytes at an address, or RAM and registers (which
/// have no bytes in the file). Areas the CPU doesn't see (the NES's CHR ROM)
/// have no address.
pub(crate) struct Area {
    pub name: String,
    pub address: Option<u64>,
    pub size: u64,
    pub file_offset: Option<u64>,
    pub kind: RegionKind,
    pub perms: &'static str,
}

impl Area {
    pub fn rom(name: impl Into<String>, address: u64, size: u64, file_offset: u64) -> Area {
        Area {
            name: name.into(),
            address: Some(address),
            size,
            file_offset: Some(file_offset),
            kind: RegionKind::Code,
            perms: "r-x",
        }
    }

    pub fn ram(name: impl Into<String>, address: u64, size: u64) -> Area {
        Area {
            name: name.into(),
            address: Some(address),
            size,
            file_offset: None,
            kind: RegionKind::Bss,
            perms: "rw-",
        }
    }

    pub fn io(name: impl Into<String>, address: u64, size: u64) -> Area {
        Area {
            name: name.into(),
            address: Some(address),
            size,
            file_offset: None,
            kind: RegionKind::Data,
            perms: "rw-",
        }
    }
}

/// What reading a ROM gives: what a [`Binary`] is made of.
pub(crate) struct RomParts {
    pub platform: Platform,
    pub cpu: Cpu,
    /// `iNES`, `SNES LoROM`, `Game Boy (MBC1)`...
    pub format_name: String,
    pub title: Option<String>,
    pub endian: Endian,
    /// The CPU's word size.
    pub bits: u32,
    pub areas: Vec<Area>,
    pub map: Map,
    /// Where the CPU starts (first) and where interrupts go: (name, address).
    pub vectors: Vec<(&'static str, u64)>,
    /// Hardware registers and other named places: (name, address, size).
    pub labels: Vec<(String, u64, u64)>,
    pub properties: Vec<Property>,
    /// The CPU's state at the entry points.
    pub state: State,
    /// The header and the rest of the file, as regions.
    pub layout: fn(&mut Builder<'_>),
    /// The file's bytes in the order the console reads them, when the file
    /// has them otherwise (a byte-swapped Nintendo 64 ROM).
    pub data: Option<Vec<u8>>,
}

/// What a [`Binary`] keeps of a ROM: how to read its code, and what was found by following it.
pub(crate) struct Rom {
    pub platform: Platform,
    pub cpu: Cpu,
    pub map: Map,
    pub vectors: Vec<(&'static str, u64)>,
    pub state: State,
    layout: fn(&mut Builder<'_>),
    pub analysis: analysis::Analysis,
}

impl Rom {
    pub fn build_layout(&self, b: &mut Builder<'_>) {
        (self.layout)(b);
    }
}

/// Recognizes a ROM by its header.
pub(crate) fn detect(data: &[u8]) -> Option<RomParts> {
    nes::detect(data)
        .or_else(|| gb::detect(data))
        .or_else(|| gba::detect(data))
        .or_else(|| megadrive::detect(data))
        .or_else(|| n64::detect(data))
        .or_else(|| psx::detect(data))
        .or_else(|| snes::detect(data))
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(bytes);
    crc.sum()
}

/// The text of a fixed-size header field: printable ASCII, trimmed.
pub(crate) fn header_text(bytes: &[u8]) -> String {
    let text: String = bytes
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { ' ' })
        .collect();
    text.trim().to_string()
}

pub(crate) fn prop(key: &str, value: impl Into<String>) -> Property {
    Property {
        key: key.into(),
        value: value.into(),
    }
}

impl Binary {
    /// A ROM read into the model: sections for its banks, RAM and registers,
    /// symbols for its vectors and registers, and the code followed from its
    /// entry points.
    pub(crate) fn from_rom(data: Arc<[u8]>, mut parts: RomParts) -> Result<Binary> {
        let data: Arc<[u8]> = parts.data.take().map_or(data, Arc::from);
        let mut sections = Vec::new();
        let mut segments = Vec::new();
        for area in &parts.areas {
            let index = sections.len() as u32;
            let loaded = area.address.is_some();
            let segment = loaded.then(|| {
                segments.push(Segment {
                    index: segments.len() as u32,
                    name: area.name.clone(),
                    kind: match (area.file_offset, area.kind) {
                        (Some(_), _) => "ROM",
                        (None, RegionKind::Bss) => "RAM",
                        _ => "I/O",
                    }
                    .into(),
                    address: area.address.unwrap_or(0),
                    mem_size: area.size,
                    file_offset: area.file_offset.unwrap_or(0),
                    file_size: if area.file_offset.is_some() { area.size } else { 0 },
                    align: 1,
                    perms: area.perms.into(),
                    mapped: true,
                });
                segments.len() as u32 - 1
            });
            sections.push(Section {
                index,
                name: area.name.clone(),
                segment_name: None,
                kind: area.kind,
                address: area.address.unwrap_or(0),
                size: area.size,
                file_offset: area.file_offset,
                file_size: if area.file_offset.is_some() { area.size } else { 0 },
                align: 1,
                flags: String::new(),
                perms: area.perms.into(),
                compressed: false,
                segment,
                loaded,
            });
        }
        let section_of = |a: u64| {
            sections
                .iter()
                .find(|s| s.loaded && a >= s.address && a < s.address + s.size)
                .map(|s| s.index)
        };
        // The code first: the vectors' handlers get their sizes from it.
        let mut rom = Rom {
            platform: parts.platform,
            cpu: parts.cpu,
            map: parts.map,
            vectors: parts.vectors,
            state: parts.state,
            layout: parts.layout,
            analysis: Default::default(),
        };
        rom.analysis = analysis::analyze(&data, &sections, &rom);
        let size_of = |a: u64| {
            let f = &rom.analysis.functions;
            f.binary_search_by_key(&a, |f| f.0).map_or(0, |i| f[i].1)
        };
        let mut symbols = SymbolBuilder::default();
        for (name, address, size) in &parts.labels {
            symbols.push(NewSym {
                name,
                address: *address,
                size: *size,
                kind: SymbolKind::Data,
                binding: Binding::Global,
                section: section_of(*address),
                source: SymbolSource::Symtab,
                defined: true,
                plain: true,
            });
        }
        // Each handler is named after its vector (the first, if several share one).
        let mut named = std::collections::HashSet::new();
        for (name, address) in &rom.vectors {
            if named.insert(*address) && section_of(*address).is_some() {
                symbols.push(NewSym {
                    name,
                    address: *address,
                    size: size_of(*address),
                    kind: SymbolKind::Function,
                    binding: Binding::Global,
                    section: section_of(*address),
                    source: SymbolSource::Symtab,
                    defined: true,
                    plain: true,
                });
            }
        }
        let symbols = symbols.finish_unindexed();
        let bytes: &[u8] = &data;
        let mut properties = vec![prop("Platform", parts.platform.name())];
        if let Some(title) = &parts.title {
            properties.push(prop("Title", title.clone()));
        }
        properties.extend(parts.properties);
        let id = format!("{:08X}", crc32(bytes));
        properties.push(prop("CRC32", id.clone()));
        properties.push(prop(
            "Code found",
            format!(
                "{} instructions in {} functions, following the code from {} vectors",
                rom.analysis.instructions,
                rom.analysis.functions.len(),
                rom.vectors.len()
            ),
        ));
        let summary = Summary {
            format: Format::Rom,
            format_name: parts.format_name,
            kind: format!("{} ROM", parts.platform.name()),
            arch: parts.cpu.name().into(),
            bits: parts.bits,
            little_endian: parts.endian == Endian::Little,
            file_size: bytes.len() as u64,
            entry: rom.vectors.first().map(|v| v.1),
            image_base: None,
            build_id: Some(id),
            debug_link: None,
            has_dwarf: false,
            has_symbols: !symbols.is_empty(),
            synthetic_addresses: false,
            section_count: sections.len() as u32,
            segment_count: segments.len() as u32,
            symbol_count: symbols.len() as u32,
            properties,
            fingerprint: crate::binary::fingerprint(bytes, None),
        };
        let arch = match parts.cpu {
            Cpu::M68000 => object::Architecture::M68k,
            Cpu::MipsR4300 | Cpu::MipsR3000 => object::Architecture::Mips,
            Cpu::Arm7Tdmi => object::Architecture::Arm,
            _ => object::Architecture::Unknown,
        };
        let mut binary = Binary {
            data: data.clone(),
            summary,
            sections,
            segments,
            symbols,
            imports: Vec::new(),
            exports: Vec::new(),
            layout: crate::layout::Layout::empty(),
            machine: Machine::Other,
            arch,
            is64: false,
            endian: parts.endian,
            image_base: 0,
            debug: None,
            discovered: Vec::new(),
            debug_symbols: Default::default(),
            annotations: Vec::new(),
            strings: std::sync::OnceLock::new(),
            coverage: std::sync::OnceLock::new(),
            xrefs: std::sync::OnceLock::new(),
            pointers: std::sync::OnceLock::new(),
            objc: std::sync::OnceLock::new(),
            rom: None,
        };
        binary.discovered = rom.analysis.functions.clone();
        binary.rom = Some(rom);
        binary.rebuild_static_symbols();
        binary.layout = binary.build_layout(Format::Rom);
        Ok(binary)
    }

    /// The console, for a ROM.
    pub fn platform(&self) -> Option<Platform> {
        self.rom.as_ref().map(|r| r.platform)
    }

    /// The CPU state code at `address` runs in: as found at the start of its
    /// function (the 65816's register widths).
    pub(crate) fn rom_state_at(&self, address: u64) -> State {
        let Some(rom) = &self.rom else {
            return State::default();
        };
        let states = &rom.analysis.states;
        states
            .get(&address)
            .or_else(|| {
                self.symbols
                    .function_containing(address)
                    .and_then(|f| states.get(&f.address))
            })
            .copied()
            .unwrap_or(rom.state)
    }
}
