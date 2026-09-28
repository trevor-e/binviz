//! Label files: the names emulators' debuggers show, read into binviz's
//! notes and written from them.
//!
//! - Mesen's `.mlb`: `Type:Address[-End]:Label[:Comment]`, the type saying
//!   what the address is in (PRG ROM offsets, RAM offsets, CPU addresses).
//! - FCEUX's `.nl` name lists: `$ADDR[/SIZE]#name#comment`, CPU addresses,
//!   one file per 16 KiB bank (`game.nes.3.nl`) and one for RAM
//!   (`game.nes.ram.nl`).
//! - `.sym` files as assemblers write them for debuggers: RGBDS's (Game Boy)
//!   and WLA DX's (`[labels]`; SNES, Game Boy), `bank:address name`; and
//!   no$gba's, `AAAAAAAA name`.

use serde::Serialize;

use super::{Platform, nes};
use crate::binary::Binary;
use crate::error::{Error, Result, bail};
use crate::model::Annotation;

/// A label file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LabelFormat {
    /// Mesen's `.mlb`.
    Mlb,
    /// FCEUX's `.nl` name lists.
    Nl,
    /// `bank:address name` (RGBDS; WLA DX with `[labels]`).
    Sym,
    /// no$gba's `.sym`: `AAAAAAAA name`.
    NoCash,
}

impl LabelFormat {
    pub fn name(self) -> &'static str {
        match self {
            LabelFormat::Mlb => "Mesen (.mlb)",
            LabelFormat::Nl => "FCEUX (.nl)",
            LabelFormat::Sym => "RGBDS / WLA DX (.sym)",
            LabelFormat::NoCash => "no$gba (.sym)",
        }
    }

    pub fn from_name(s: &str) -> Option<LabelFormat> {
        Some(match s.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "mlb" | "mesen" => LabelFormat::Mlb,
            "nl" | "fceux" => LabelFormat::Nl,
            "sym" | "rgbds" | "wla" => LabelFormat::Sym,
            "nocash" | "no-cash" | "no$gba" | "no$" => LabelFormat::NoCash,
            _ => return None,
        })
    }
}

/// What a label file gave.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelImport {
    pub format: LabelFormat,
    /// The names and comments, as notes at binviz's addresses.
    pub labels: Vec<Annotation>,
    /// Lines naming places binviz can't place: a switched window's CPU
    /// address without its bank, memory it doesn't model.
    pub skipped: u32,
    /// Directives that aren't names (no$gba's `.arm`, `.thumb`, `.byt`…).
    pub directives: u32,
}

/// A file of labels to write.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelFile {
    /// What goes after the ROM's name: `mlb`, `3.nl`, `ram.nl`, `sym`.
    pub suffix: String,
    pub text: String,
}

/// How a console's addresses read in label files.
struct Places<'a> {
    bin: &'a Binary,
    platform: Platform,
    /// Where offsets into the ROM start in the file (the NES's PRG ROM, an SNES ROM past a copier header).
    rom_base: u64,
    rom_size: u64,
    hirom: bool,
}

impl<'a> Places<'a> {
    fn new(bin: &'a Binary) -> Result<Places<'a>> {
        let rom = bin
            .rom
            .as_ref()
            .ok_or_else(|| Error::new("label files are for game ROMs"))?;
        let data = bin.data();
        let (rom_base, rom_size) = match rom.platform {
            Platform::Nes => {
                let h = nes::header(data).ok_or_else(|| Error::new("not an iNES file"))?;
                (h.prg_offset, h.prg_size)
            }
            Platform::Snes => {
                let copier = if data.len() % 1024 == 512 { 512 } else { 0 };
                (copier, data.len() as u64 - copier)
            }
            _ => (0, data.len() as u64),
        };
        Ok(Places {
            bin,
            platform: rom.platform,
            rom_base,
            rom_size,
            hirom: matches!(rom.map, super::Map::Snes { hirom: true }),
        })
    }

    /// Ours for an offset into the ROM.
    fn rom(&self, offset: u64) -> Option<u64> {
        (offset < self.rom_size)
            .then(|| self.bin.offset_to_address(self.rom_base + offset))
            .flatten()
    }

    /// Ours for a CPU address outside switched windows.
    fn cpu(&self, address: u64) -> Option<u64> {
        let rom = self.bin.rom.as_ref()?;
        let a = rom.map.resolve(0, address)?;
        self.known(a)
    }

    /// Ours for a CPU address in a bank: a Game Boy ROM bank, a 24-bit SNES address.
    fn banked(&self, bank: u64, address: u64) -> Option<u64> {
        match self.platform {
            Platform::GameBoy | Platform::GameBoyColor => match address {
                0..0x4000 => self.known(address),
                0x4000..0x8000 => self.known(bank.max(1) << 16 | address),
                _ => self.cpu(address),
            },
            Platform::Snes => self.cpu(bank << 16 | address),
            // FCEUX's banks are 16 KiB pages of PRG ROM.
            Platform::Nes if address >= 0x8000 => self.rom(bank * 0x4000 + (address & 0x3FFF)),
            _ => self.cpu(address),
        }
    }

    /// An address of ours that is somewhere: in a section.
    fn known(&self, a: u64) -> Option<u64> {
        self.bin
            .sections()
            .iter()
            .any(|s| s.loaded && a >= s.address && a < s.address + s.size.max(1))
            .then_some(a)
    }

    /// The offset into the ROM of one of our addresses.
    fn rom_offset(&self, a: u64) -> Option<u64> {
        let o = self.bin.address_to_offset(a)?.checked_sub(self.rom_base)?;
        (o < self.rom_size).then_some(o)
    }

    /// The CPU's view of one of our addresses.
    fn cpu_of(&self, a: u64) -> u64 {
        self.bin.rom.as_ref().map_or(a, |r| r.map.cpu(a))
    }
}

impl Binary {
    /// Reads a label file (named `file_name`) into notes at binviz's addresses.
    pub fn read_labels(&self, file_name: &str, text: &str) -> Result<LabelImport> {
        let places = Places::new(self)?;
        let lower = file_name.to_ascii_lowercase();
        let text = text.trim_start_matches('\u{feff}');
        let mut out = LabelImport {
            format: LabelFormat::Mlb,
            labels: Vec::new(),
            skipped: 0,
            directives: 0,
        };
        if lower.ends_with(".nl") {
            out.format = LabelFormat::Nl;
            read_nl(&places, &lower, text, &mut out);
        } else if lower.ends_with(".mlb") || (!lower.ends_with(".sym") && text.lines().any(is_mlb_line)) {
            read_mlb(&places, text, &mut out);
        } else if text.lines().any(|l| l.trim().eq_ignore_ascii_case("[labels]")) {
            out.format = LabelFormat::Sym;
            read_wla(&places, text, &mut out);
        } else if text.lines().any(|l| nocash_line(l).is_some()) {
            out.format = LabelFormat::NoCash;
            read_nocash(&places, text, &mut out);
        } else {
            out.format = LabelFormat::Sym;
            read_sym(&places, text, &mut out);
        }
        if out.labels.is_empty() && out.skipped == 0 {
            bail!(
                "no labels in {file_name}: binviz reads Mesen's .mlb, FCEUX's .nl, and .sym files (RGBDS, WLA DX, no$gba)"
            );
        }
        out.labels.sort_by_key(|a| (a.address, a.size));
        Ok(out)
    }

    /// The label formats the emulators for this ROM's console read.
    pub fn label_formats(&self) -> Vec<LabelFormat> {
        match self.platform() {
            Some(Platform::Nes) => vec![LabelFormat::Mlb, LabelFormat::Nl],
            Some(Platform::Snes) => vec![LabelFormat::Mlb, LabelFormat::Sym],
            Some(Platform::GameBoy | Platform::GameBoyColor) => vec![LabelFormat::Sym, LabelFormat::Mlb],
            Some(Platform::GameBoyAdvance) => vec![LabelFormat::NoCash, LabelFormat::Mlb],
            _ => Vec::new(),
        }
    }

    /// Writes the notes (their names and comments) as a label file for an
    /// emulator; FCEUX's take one file per bank.
    pub fn write_labels(&self, format: LabelFormat) -> Result<Vec<LabelFile>> {
        let places = Places::new(self)?;
        if !self.label_formats().contains(&format) {
            bail!("{} files aren't for the {}", format.name(), places.platform.name());
        }
        let notes: Vec<&Annotation> = self
            .annotations()
            .iter()
            .filter(|a| !a.name.is_empty() || !a.comment.is_empty())
            .collect();
        Ok(match format {
            LabelFormat::Mlb => vec![LabelFile {
                suffix: "mlb".into(),
                text: write_mlb(&places, &notes),
            }],
            LabelFormat::Nl => write_nl(&places, &notes),
            LabelFormat::Sym => vec![LabelFile {
                suffix: "sym".into(),
                text: write_sym(&places, &notes),
            }],
            LabelFormat::NoCash => vec![LabelFile {
                suffix: "sym".into(),
                text: write_nocash(&places, &notes),
            }],
        })
    }
}

fn hex(s: &str) -> Option<u64> {
    let s = s.trim().trim_start_matches('$');
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    (!s.is_empty()).then(|| u64::from_str_radix(s, 16).ok()).flatten()
}

fn note(address: u64, size: u64, name: &str, comment: &str) -> Annotation {
    Annotation {
        address,
        size: if size > 1 { size } else { 0 },
        name: name.trim().to_string(),
        comment: comment.trim().to_string(),
        reviewed: false,
    }
}

// --- Mesen ------------------------------------------------------------------------

fn is_mlb_line(line: &str) -> bool {
    let mut parts = line.splitn(3, ':');
    let (Some(ty), Some(addr)) = (parts.next(), parts.next()) else {
        return false;
    };
    !ty.is_empty()
        && ty.chars().all(|c| c.is_ascii_alphanumeric())
        && hex(addr.split('-').next().unwrap_or("")).is_some()
}

fn read_mlb(places: &Places, text: &str, out: &mut LabelImport) {
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || !is_mlb_line(line) {
            continue;
        }
        let mut parts = line.splitn(4, ':');
        let ty = parts.next().unwrap_or("");
        let range = parts.next().unwrap_or("");
        let label = parts.next().unwrap_or("");
        let comment = parts.next().unwrap_or("").replace("\\n", "\n");
        let mut ends = range.splitn(2, '-');
        let Some(start) = ends.next().and_then(hex) else {
            continue;
        };
        let end = ends.next().and_then(hex).filter(|&e| e >= start).unwrap_or(start);
        let at = match ty {
            "P" | "NesPrgRom" | "PRG" | "SnesPrgRom" | "GbPrgRom" | "GBPRG" | "GbaPrgRom" | "PcePrgRom" => {
                places.rom(start)
            }
            "R" | "NesInternalRam" => places.cpu(start & 0x7FF),
            "W" | "S" | "NesWorkRam" | "NesSaveRam" => places.cpu(0x6000 + start),
            "G" | "NesMemory" | "SnesMemory" | "SnesRegister" | "REG" | "GameboyMemory" | "GbaMemory" => {
                places.cpu(start)
            }
            "SnesWorkRam" | "WORK" => places.cpu(0x7E_0000 + start),
            "SnesSaveRam" | "SAVE" if !places.hirom => places.cpu(0x70_0000 + start),
            "GbWorkRam" if start < 0x2000 => places.cpu(0xC000 + start),
            "GbCartRam" if start < 0x2000 => places.cpu(0xA000 + start),
            "GbHighRam" => places.cpu(0xFF80 + start),
            "GbaIntWorkRam" => places.cpu(0x0300_0000 + start),
            "GbaExtWorkRam" => places.cpu(0x0200_0000 + start),
            "GbaSaveRam" => places.cpu(0x0E00_0000 + start),
            _ => None,
        };
        match at {
            Some(a) => out.labels.push(note(a, end - start + 1, label, &comment)),
            None => out.skipped += 1,
        }
    }
}

/// A name Mesen takes: letters, digits, `_` and `@`, not starting with a digit.
fn mesen_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '@' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

fn write_mlb(places: &Places, notes: &[&Annotation]) -> String {
    let mut out = String::new();
    for a in notes {
        let last = a.address + a.size.max(1) - 1;
        let (ty, start) = match places.platform {
            Platform::Nes => match places.rom_offset(a.address) {
                Some(o) => ("P", o),
                None => match places.cpu_of(a.address) {
                    x @ 0..0x2000 => ("R", x & 0x7FF),
                    x @ 0x6000..0x8000 => ("W", x - 0x6000),
                    x => ("G", x),
                },
            },
            Platform::Snes => match places.rom_offset(a.address) {
                Some(o) => ("SnesPrgRom", o),
                None if (0x7E_0000..0x80_0000).contains(&a.address) => ("SnesWorkRam", a.address - 0x7E_0000),
                None => ("SnesMemory", places.cpu_of(a.address)),
            },
            Platform::GameBoy | Platform::GameBoyColor => match places.rom_offset(a.address) {
                Some(o) => ("GbPrgRom", o),
                None => match places.cpu_of(a.address) {
                    x @ 0xC000..0xE000 => ("GbWorkRam", x - 0xC000),
                    x @ 0xA000..0xC000 => ("GbCartRam", x - 0xA000),
                    x @ 0xFF80..0xFFFF => ("GbHighRam", x - 0xFF80),
                    x => ("GameboyMemory", x),
                },
            },
            _ => match places.rom_offset(a.address) {
                Some(o) => ("GbaPrgRom", o),
                None if (0x0300_0000..0x0300_8000).contains(&a.address) => ("GbaIntWorkRam", a.address - 0x0300_0000),
                None => ("GbaMemory", places.cpu_of(a.address)),
            },
        };
        let range = if a.size > 1 {
            format!("{start:04X}-{:04X}", start + (last - a.address))
        } else {
            format!("{start:04X}")
        };
        out.push_str(&format!("{ty}:{range}:{}", mesen_name(&a.name)));
        if !a.comment.is_empty() {
            out.push_str(&format!(":{}", a.comment.replace('\n', "\\n")));
        }
        out.push('\n');
    }
    out
}

// --- FCEUX ------------------------------------------------------------------------

/// The bank a name list is for, from its file name: `game.nes.3.nl` (none for `game.nes.ram.nl`).
fn nl_bank(file_name: &str) -> Option<u64> {
    let stem = file_name.strip_suffix(".nl")?;
    let part = stem.rsplit('.').next()?;
    if part == "ram" {
        None
    } else {
        u64::from_str_radix(part, 16).ok()
    }
}

fn read_nl(places: &Places, file_name: &str, text: &str, out: &mut LabelImport) {
    let bank = nl_bank(file_name);
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(more) = line.strip_prefix('\\') {
            if let Some(last) = out.labels.last_mut() {
                last.comment.push('\n');
                last.comment.push_str(more.trim());
            }
            continue;
        }
        let Some(rest) = line.trim().strip_prefix('$') else {
            continue;
        };
        let mut parts = rest.splitn(3, '#');
        let place = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("");
        let comment = parts.next().unwrap_or("");
        let mut place = place.splitn(2, '/');
        let Some(address) = place.next().and_then(hex) else {
            continue;
        };
        let size = place.next().and_then(hex).unwrap_or(1).max(1);
        let at = match bank {
            Some(b) if address >= 0x8000 => places.banked(b, address),
            _ => places.cpu(address),
        };
        match at {
            Some(a) => out.labels.push(note(a, size, name, comment)),
            None => out.skipped += 1,
        }
    }
}

fn write_nl(places: &Places, notes: &[&Annotation]) -> Vec<LabelFile> {
    let mut files: std::collections::BTreeMap<Option<u64>, String> = std::collections::BTreeMap::new();
    for a in notes {
        let cpu = places.cpu_of(a.address);
        let bank = places.rom_offset(a.address).map(|o| o >> 14);
        let file = files.entry(if cpu >= 0x8000 { bank } else { None }).or_default();
        let mut comment = a.comment.lines();
        let size = if a.size > 1 {
            format!("/{:X}", a.size)
        } else {
            String::new()
        };
        file.push_str(&format!(
            "${cpu:04X}{size}#{}#{}\n",
            a.name.replace('#', "_"),
            comment.next().unwrap_or("")
        ));
        for more in comment {
            file.push_str(&format!("\\{more}\n"));
        }
    }
    files
        .into_iter()
        .map(|(bank, text)| LabelFile {
            suffix: match bank {
                Some(b) => format!("{b:X}.nl"),
                None => "ram.nl".into(),
            },
            text,
        })
        .collect()
}

// --- .sym -------------------------------------------------------------------------

/// `BB:AAAA name`: a bank and an address.
fn bank_line(line: &str) -> Option<(u64, u64, &str)> {
    let line = line.split(';').next()?.trim();
    let mut tokens = line.split_whitespace();
    let (place, name) = (tokens.next()?, tokens.next()?);
    let (bank, address) = place.split_once(':')?;
    Some((
        u64::from_str_radix(bank, 16).ok()?,
        u64::from_str_radix(address, 16).ok()?,
        name,
    ))
}

fn read_sym(places: &Places, text: &str, out: &mut LabelImport) {
    for line in text.lines() {
        // Bare `VALUE name` lines are RGBDS's exported constants, not places.
        let Some((bank, address, name)) = bank_line(line) else {
            continue;
        };
        match places.banked(bank, address) {
            Some(a) => out.labels.push(note(a, 1, name, "")),
            None => out.skipped += 1,
        }
    }
}

fn read_wla(places: &Places, text: &str, out: &mut LabelImport) {
    let mut in_labels = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_labels = t.eq_ignore_ascii_case("[labels]");
            continue;
        }
        if !in_labels {
            continue;
        }
        let Some((bank, address, name)) = bank_line(t) else {
            continue;
        };
        match places.banked(bank, address) {
            Some(a) => out.labels.push(note(a, 1, name, "")),
            None => out.skipped += 1,
        }
    }
}

/// `AAAAAAAA name`: eight hex digits and a name.
fn nocash_line(line: &str) -> Option<(u64, &str)> {
    let line = line.split(';').next()?.trim();
    let (address, name) = line.split_once(|c: char| c.is_whitespace())?;
    (address.len() == 8 && address.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| Some((u64::from_str_radix(address, 16).ok()?, name.trim())))
        .flatten()
}

fn read_nocash(places: &Places, text: &str, out: &mut LabelImport) {
    for line in text.lines() {
        let Some((address, name)) = nocash_line(line) else {
            continue;
        };
        if name.starts_with('.') {
            out.directives += 1;
            continue;
        }
        match places.cpu(address) {
            Some(a) => out.labels.push(note(a, 1, name, "")),
            None => out.skipped += 1,
        }
    }
}

/// A name assemblers' symbol files take: letters, digits, `_`, `.`, `@`.
fn sym_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '@') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        s.insert(0, '_');
    }
    s
}

fn write_sym(places: &Places, notes: &[&Annotation]) -> String {
    let snes = places.platform == Platform::Snes;
    let mut out = String::from(if snes {
        "; written by binviz\n[labels]\n"
    } else {
        "; written by binviz\n"
    });
    for a in notes.iter().filter(|a| !a.name.is_empty()) {
        let cpu = places.cpu_of(a.address);
        let (bank, address) = if snes {
            (cpu >> 16, cpu & 0xFFFF)
        } else {
            // A Game Boy ROM bank past the first is where our address puts it.
            ((a.address >> 16) & 0xFF, cpu & 0xFFFF)
        };
        out.push_str(&format!("{bank:02x}:{address:04x} {}\n", sym_name(&a.name)));
    }
    out
}

fn write_nocash(places: &Places, notes: &[&Annotation]) -> String {
    let mut out = String::new();
    for a in notes.iter().filter(|a| !a.name.is_empty()) {
        let name: String = a
            .name
            .chars()
            .map(|c| if c.is_whitespace() { '_' } else { c })
            .collect();
        out.push_str(&format!("{:08X} {name}\n", places.cpu_of(a.address)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_made_safe() {
        assert_eq!(mesen_name("Shape::area"), "Shape__area");
        assert_eq!(mesen_name("3d_draw"), "_3d_draw");
        assert_eq!(sym_name("main.loop"), "main.loop");
        assert_eq!(nl_bank("game.nes.1f.nl"), Some(0x1F));
        assert_eq!(nl_bank("game.nes.ram.nl"), None);
        assert_eq!(bank_line("01:4000 BankedFn ; a comment"), Some((1, 0x4000, "BankedFn")));
        assert_eq!(nocash_line("080001EC init_video"), Some((0x0800_01EC, "init_video")));
        assert!(is_mlb_line("P:0100:MyLabel:This is a comment"));
        assert!(is_mlb_line("R:0200-02FF:ShadowOam"));
        assert!(!is_mlb_line("01:4000 BankedFn"));
    }
}
