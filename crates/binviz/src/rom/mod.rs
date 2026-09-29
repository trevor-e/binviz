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
pub mod cdl;
mod gb;
mod gba;
mod jumptable;
pub mod labels;
mod megadrive;
mod n64;
mod nes;
mod psx;
mod snes;

use std::sync::Arc;

use serde::Serialize;

use crate::binary::Binary;
use crate::cpu::{Cpu, State};
use crate::error::{Error, Result};
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
    /// More places to follow the code from: (name, address, size). A function
    /// found by its prologue has no name (it gets a `sub_…` one); one another
    /// image of the same game names brings its name and size (0: as followed).
    pub entries: Vec<(String, u64, u64)>,
}

/// What a [`Binary`] keeps of a ROM: how to read its code, and what was found by following it.
pub(crate) struct Rom {
    pub platform: Platform,
    pub cpu: Cpu,
    pub map: Map,
    pub vectors: Vec<(&'static str, u64)>,
    /// Entry points besides the vectors (see [`RomParts::entries`]).
    pub entries: Vec<(String, u64, u64)>,
    pub state: State,
    layout: fn(&mut Builder<'_>),
    pub analysis: analysis::Analysis,
    /// A code/data log the code was followed with, and what it covers.
    pub log: Option<(Arc<cdl::CodeDataLog>, cdl::LogSummary)>,
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
        .or_else(|| psx::detect_memory(data))
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
    pub(crate) fn from_rom(data: Arc<[u8]>, parts: RomParts) -> Result<Binary> {
        Binary::from_rom_logged(data, parts, None)
    }

    /// The same ROM read again with a code/data log (FCEUX's or Mesen's):
    /// the code it saw run is followed too (code reached only through jump
    /// tables), what it saw read as data isn't taken for code, the 65816's
    /// register widths and ARM or Thumb are as they were when the code ran,
    /// and an NES game's switched banks are placed where they ran.
    pub fn with_code_log(&self, log: &[u8]) -> Result<(Binary, cdl::LogSummary)> {
        let rom = self
            .rom
            .as_ref()
            .ok_or_else(|| Error::new("code/data logs are for game ROMs"))?;
        let data = &self.data;
        let shape = match rom.platform {
            Platform::Nes => {
                let h = nes::header(data).ok_or_else(|| Error::new("not an iNES file"))?;
                cdl::RomShape {
                    platform: rom.platform,
                    rom_offset: h.prg_offset,
                    rom_size: h.prg_size,
                    chr_size: h.chr_size,
                }
            }
            Platform::Snes => {
                let copier = if data.len() % 1024 == 512 { 512 } else { 0 };
                cdl::RomShape {
                    platform: rom.platform,
                    rom_offset: copier,
                    rom_size: data.len() as u64 - copier,
                    chr_size: 0,
                }
            }
            Platform::GameBoy | Platform::GameBoyColor | Platform::GameBoyAdvance => cdl::RomShape {
                platform: rom.platform,
                rom_offset: 0,
                rom_size: data.len() as u64,
                chr_size: 0,
            },
            p => {
                return Err(Error::new(format!(
                    "binviz reads code/data logs for the NES, SNES, Game Boy and Game Boy Advance, not the {}",
                    p.name()
                )));
            }
        };
        let log = cdl::CodeDataLog::parse(log, &shape)?;
        let (parts, placed) = match rom.platform {
            Platform::Nes => nes::detect_with(data, log.page_windows().as_deref()),
            _ => detect(data).map(|p| (p, 0)),
        }
        .ok_or_else(|| Error::new("the ROM no longer reads"))?;
        let summary = log.summary(crc32(data), placed);
        let bin = Binary::from_rom_logged(data.clone(), parts, Some((Arc::new(log), summary.clone())))?;
        Ok((bin, summary))
    }

    /// What the code/data log the code was followed with covers, if there is one.
    pub fn code_log(&self) -> Option<&cdl::LogSummary> {
        self.rom.as_ref()?.log.as_ref().map(|(_, s)| s)
    }

    /// The code/data log's flags for a file offset ([`cdl::flag`] bits).
    pub fn code_log_at(&self, offset: u64) -> u16 {
        self.rom
            .as_ref()
            .and_then(|r| r.log.as_ref())
            .map_or(0, |(l, _)| l.at(offset))
    }

    fn from_rom_logged(
        data: Arc<[u8]>,
        mut parts: RomParts,
        log: Option<(Arc<cdl::CodeDataLog>, cdl::LogSummary)>,
    ) -> Result<Binary> {
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
            entries: parts.entries,
            state: parts.state,
            layout: parts.layout,
            analysis: Default::default(),
            log,
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
        // The other entry points, where the code was followed (a function
        // found by its prologue; a function another image names): a symbol
        // each, named functions first so that a name wins over `sub_…`.
        for (name, address, size) in &rom.entries {
            if !name.is_empty() && named.insert(*address) && section_of(*address).is_some() {
                symbols.push(NewSym {
                    name,
                    address: *address,
                    size: if *size > 0 { *size } else { size_of(*address) },
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
                "{} instructions in {} functions, following the code from {} vectors{}{}",
                rom.analysis.instructions,
                rom.analysis.functions.len(),
                rom.vectors.len(),
                if rom.entries.is_empty() {
                    String::new()
                } else {
                    format!(" and {} other entry points", rom.entries.len())
                },
                if rom.log.is_some() {
                    " and what the code/data log saw run"
                } else {
                    ""
                }
            ),
        ));
        if let Some((_, s)) = &rom.log {
            let pct = |n: u64| {
                let p = n as f64 * 100.0 / s.bytes.max(1) as f64;
                if n > 0 && p < 0.1 {
                    "<0.1%".to_string()
                } else {
                    format!("{p:.1}%")
                }
            };
            let mut text = format!(
                "{}: {} bytes of code ({}), {} of data ({}), {} never seen",
                s.format.name(),
                s.code,
                pct(s.code),
                s.data,
                pct(s.data),
                pct(s.bytes - (s.code + s.data - s.both))
            );
            if s.pages_placed > 0 {
                text.push_str(&format!("; {} PRG pages placed where they ran", s.pages_placed));
            }
            if s.crc_matches == Some(false) {
                text.push_str("; made for another ROM (its CRC-32 differs)");
            }
            properties.push(prop("Code/data log", text));
        }
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

    /// A ROM address as people write them, `bank:address`: `03:C000` (bank
    /// 3's `$C000`), or for the SNES a 24-bit address (`$80:8000`, mirrors
    /// folded). binviz's address for it, if it is somewhere.
    pub fn rom_address(&self, text: &str) -> Option<u64> {
        let rom = self.rom.as_ref()?;
        let (bank, address) = text.trim().trim_start_matches('$').split_once(':')?;
        let bank = u64::from_str_radix(bank.trim(), 16).ok()?;
        let address = u64::from_str_radix(address.trim().trim_start_matches('$'), 16).ok()?;
        if bank > 0xFF || address > 0xFFFF {
            return None;
        }
        let known = |a: u64| {
            self.sections
                .iter()
                .any(|s| s.loaded && a >= s.address && a < s.address + s.size.max(1))
        };
        match rom.map {
            Map::Snes { .. } => rom.map.resolve(0, bank << 16 | address),
            // A bank of its own at that address, else the bank a window that doesn't switch holds.
            Map::Banked(_) => {
                let a = bank << 16 | address;
                if known(a) { Some(a) } else { rom.map.resolve(a, address) }
            }
            Map::Flat(_) => None,
        }
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
