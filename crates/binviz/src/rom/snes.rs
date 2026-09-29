//! The SNES: no magic number, so the internal header is found where the
//! cartridge's mapping puts it (`$7FC0` in the file for LoROM, `$FFC0` for
//! HiROM, 512 bytes further with a copier header) and judged by what it
//! says: a checksum and its complement, the mapping, a reset vector into
//! ROM, a printable title. Addresses are the 65816's 24-bit ones, each
//! mirror folded onto one (ROM in banks `$00`-`$7D` for LoROM, `$C0`-`$FF`
//! for HiROM; WRAM at `$7E0000`; registers in bank `$00`).

use super::{Area, Map, Platform, RomParts, header_text, prop};
use crate::cpu::{Cpu, State};
use crate::layout::Builder;
use crate::layout::fields::{F, Fmt};
use crate::model::RegionKind;
use crate::util::Endian;

/// The PPU's, APU ports' and CPU's registers, as they are usually named.
#[rustfmt::skip]
const REGISTERS: [(&str, u64); 104] = [
    ("INIDISP", 0x2100), ("OBSEL", 0x2101), ("OAMADDL", 0x2102), ("OAMADDH", 0x2103), ("OAMDATA", 0x2104),
    ("BGMODE", 0x2105), ("MOSAIC", 0x2106), ("BG1SC", 0x2107), ("BG2SC", 0x2108), ("BG3SC", 0x2109),
    ("BG4SC", 0x210A), ("BG12NBA", 0x210B), ("BG34NBA", 0x210C), ("BG1HOFS", 0x210D), ("BG1VOFS", 0x210E),
    ("BG2HOFS", 0x210F), ("BG2VOFS", 0x2110), ("BG3HOFS", 0x2111), ("BG3VOFS", 0x2112), ("BG4HOFS", 0x2113),
    ("BG4VOFS", 0x2114), ("VMAIN", 0x2115), ("VMADDL", 0x2116), ("VMADDH", 0x2117), ("VMDATAL", 0x2118),
    ("VMDATAH", 0x2119), ("M7SEL", 0x211A), ("M7A", 0x211B), ("M7B", 0x211C), ("M7C", 0x211D),
    ("M7D", 0x211E), ("M7X", 0x211F), ("M7Y", 0x2120), ("CGADD", 0x2121), ("CGDATA", 0x2122),
    ("W12SEL", 0x2123), ("W34SEL", 0x2124), ("WOBJSEL", 0x2125), ("WH0", 0x2126), ("WH1", 0x2127),
    ("WH2", 0x2128), ("WH3", 0x2129), ("WBGLOG", 0x212A), ("WOBJLOG", 0x212B), ("TM", 0x212C),
    ("TS", 0x212D), ("TMW", 0x212E), ("TSW", 0x212F), ("CGWSEL", 0x2130), ("CGADSUB", 0x2131),
    ("COLDATA", 0x2132), ("SETINI", 0x2133), ("MPYL", 0x2134), ("MPYM", 0x2135), ("MPYH", 0x2136),
    ("SLHV", 0x2137), ("RDOAM", 0x2138), ("RDVRAML", 0x2139), ("RDVRAMH", 0x213A), ("RDCGRAM", 0x213B),
    ("OPHCT", 0x213C), ("OPVCT", 0x213D), ("STAT77", 0x213E), ("STAT78", 0x213F), ("APUIO0", 0x2140),
    ("APUIO1", 0x2141), ("APUIO2", 0x2142), ("APUIO3", 0x2143), ("WMDATA", 0x2180), ("WMADDL", 0x2181),
    ("WMADDM", 0x2182), ("WMADDH", 0x2183), ("JOYSER0", 0x4016), ("JOYSER1", 0x4017), ("NMITIMEN", 0x4200),
    ("WRIO", 0x4201), ("WRMPYA", 0x4202), ("WRMPYB", 0x4203), ("WRDIVL", 0x4204), ("WRDIVH", 0x4205),
    ("WRDIVB", 0x4206), ("HTIMEL", 0x4207), ("HTIMEH", 0x4208), ("VTIMEL", 0x4209), ("VTIMEH", 0x420A),
    ("MDMAEN", 0x420B), ("HDMAEN", 0x420C), ("MEMSEL", 0x420D), ("RDNMI", 0x4210), ("TIMEUP", 0x4211),
    ("HVBJOY", 0x4212), ("RDIO", 0x4213), ("RDDIVL", 0x4214), ("RDDIVH", 0x4215), ("RDMPYL", 0x4216),
    ("RDMPYH", 0x4217), ("JOY1L", 0x4218), ("JOY1H", 0x4219), ("JOY2L", 0x421A), ("JOY2H", 0x421B),
    ("JOY3L", 0x421C), ("JOY3H", 0x421D), ("JOY4L", 0x421E), ("JOY4H", 0x421F),
];

/// The registers of each of the eight DMA channels (`$43n0`...).
const DMA: [&str; 11] = [
    "DMAP", "BBAD", "A1TL", "A1TH", "A1B", "DASL", "DASH", "DASB", "A2AL", "A2AH", "NLTR",
];

/// Folds a 24-bit address onto the one we give that memory.
pub(super) fn resolve(t: u64, hirom: bool) -> Option<u64> {
    let (bank, a) = (t >> 16, t & 0xFFFF);
    if bank == 0x7E || bank == 0x7F {
        return Some(t);
    }
    let b = bank & 0x7F;
    if b < 0x40 {
        return Some(match a {
            // The first 8 KiB of WRAM, mirrored in every system bank.
            0..0x2000 => 0x7E_0000 | a,
            // Registers, and whatever else is below the ROM.
            0x2000..0x8000 => a,
            _ if hirom => (0xC0 | b) << 16 | a,
            _ => b << 16 | a,
        });
    }
    if hirom {
        return Some((0xC0 | (b & 0x3F)) << 16 | a);
    }
    match (b, a) {
        (_, 0x8000..) => Some(b << 16 | a),
        // Cartridge RAM.
        (0x70..=0x7D, _) => Some(0x70 << 16 | a),
        _ => None,
    }
}

/// Where the internal header is, and how sure: (file offset, HiROM?, score).
fn find_header(data: &[u8]) -> Option<(u64, bool)> {
    let copier = if data.len() % 1024 == 512 { 512 } else { 0 };
    let mut best: Option<(u64, bool, u32)> = None;
    for (at, hirom) in [(0x7FC0u64, false), (0xFFC0, true)] {
        let at = copier + at;
        let Some(h) = data.get(at as usize..at as usize + 0x40) else {
            continue;
        };
        let word = |i: usize| u16::from_le_bytes([h[i], h[i + 1]]);
        let mode = h[0x15];
        let mut score = 0;
        if word(0x1C) ^ word(0x1E) == 0xFFFF {
            score += 4;
        }
        if mode & 0xE0 == 0x20 && (mode & 1 == 1) == hirom {
            score += 3;
        }
        if word(0x3C) >= 0x8000 {
            score += 2;
        }
        if h[..21].iter().all(|&c| (0x20..0x7F).contains(&c)) {
            score += 2;
        }
        if (0x08..=0x0D).contains(&h[0x17]) {
            score += 1;
        }
        if score >= 7 && best.is_none_or(|b| score > b.2) {
            best = Some((at, hirom, score));
        }
    }
    best.map(|b| (b.0, b.1))
}

fn map_mode(v: u64) -> String {
    let speed = if v & 0x10 != 0 { "FastROM" } else { "SlowROM" };
    let map = match v & 0x0F {
        0 => "LoROM",
        1 => "HiROM",
        2 => "LoROM + S-DD1",
        3 => "LoROM + SA-1",
        5 => "ExHiROM",
        _ => "?",
    };
    format!("{map}, {speed}")
}

fn chipset(v: u64) -> String {
    let base = match v & 0x0F {
        0 => "ROM",
        1 => "ROM + RAM",
        2 => "ROM + RAM + battery",
        3 => "ROM + coprocessor",
        4 => "ROM + coprocessor + RAM",
        5 => "ROM + coprocessor + RAM + battery",
        6 => "ROM + coprocessor + battery",
        _ => "?",
    };
    let coprocessor = match v >> 4 {
        0 if v & 0x0F >= 3 => " (DSP)",
        1 => " (Super FX)",
        2 => " (OBC1)",
        3 => " (SA-1)",
        4 => " (S-DD1)",
        5 => " (S-RTC)",
        0xE => " (other)",
        0xF => " (custom)",
        _ => "",
    };
    format!("{base}{coprocessor}")
}

fn size_kib(v: u64) -> String {
    match v {
        0 => "none".into(),
        1..=16 => format!("{} KiB", 1u64 << v),
        _ => format!("{v:#04x}"),
    }
}

fn country(v: u64) -> Option<&'static str> {
    Some(match v {
        0 => "Japan",
        1 => "North America",
        2 => "Europe",
        3 => "Sweden",
        4 => "Finland",
        5 => "Denmark",
        6 => "France",
        7 => "Netherlands",
        8 => "Spain",
        9 => "Germany",
        10 => "Italy",
        11 => "China",
        13 => "Korea",
        15 => "Canada",
        16 => "Brazil",
        17 => "Australia",
        _ => return None,
    })
}

pub(super) fn detect(data: &[u8]) -> Option<RomParts> {
    if data.len() < 0x8000 || data.len() > 12 << 20 {
        return None;
    }
    let (at, hirom) = find_header(data)?;
    let copier = if data.len() % 1024 == 512 { 512u64 } else { 0 };
    let rom = &data[copier as usize..];
    let h = &data[at as usize..at as usize + 0x40];
    let word = |i: usize| u16::from_le_bytes([h[i], h[i + 1]]) as u64;
    let chunk = if hirom { 0x10000u64 } else { 0x8000 };
    let chunks = (rom.len() as u64).div_ceil(chunk).min(if hirom { 0x40 } else { 0x7E });
    let mut areas = Vec::new();
    for n in 0..chunks {
        let (base, name) = if hirom {
            ((0xC0 + n) << 16, format!("ROM bank ${:02X}", 0xC0 + n))
        } else {
            (n << 16 | 0x8000, format!("ROM bank ${n:02X}"))
        };
        let size = chunk.min(rom.len() as u64 - n * chunk);
        areas.push(Area::rom(name, base, size, copier + n * chunk));
    }
    areas.extend([
        Area::ram("WRAM", 0x7E_0000, 0x2_0000),
        Area::io("PPU and APU registers", 0x2100, 0x84),
        Area::io("Joypad ports", 0x4016, 2),
        Area::io("CPU registers", 0x4200, 0x20),
        Area::io("DMA registers", 0x4300, 0x80),
    ]);
    let map = Map::Snes { hirom };
    // Vectors are 16-bit addresses in bank $00: emulation mode's (the CPU starts
    // in it) and native mode's.
    let mut vectors = Vec::new();
    for (name, i) in [
        ("reset", 0x3C),
        ("nmi", 0x2A),
        ("irq", 0x2E),
        ("brk", 0x26),
        ("cop", 0x24),
        ("emulation_nmi", 0x3A),
        ("emulation_irq", 0x3E),
    ] {
        let v = word(i);
        if v >= 0x8000
            && let Some(t) = map.resolve(0, v)
        {
            vectors.push((name, t));
        }
    }
    let mut labels: Vec<(String, u64, u64)> = REGISTERS.iter().map(|&(n, a)| (n.to_string(), a, 1)).collect();
    for c in 0..8u64 {
        for (i, n) in DMA.iter().enumerate() {
            labels.push((format!("{n}{c}"), 0x4300 | c << 4 | i as u64, 1));
        }
    }
    let header_at = if hirom { 0xC0_FFC0 } else { 0x00_FFC0 };
    labels.push(("header".into(), header_at, 0x40));
    let sum = rom.iter().fold(0u16, |s, &b| s.wrapping_add(b as u16)) as u64;
    let properties = vec![
        prop("Mapping", map_mode(h[0x15] as u64)),
        prop("Chipset", chipset(h[0x16] as u64)),
        prop("ROM size", size_kib(h[0x17] as u64)),
        prop("RAM size", size_kib(h[0x18] as u64)),
        prop("Country", country(h[0x19] as u64).unwrap_or("?")),
        prop("Version", format!("1.{}", h[0x1B])),
        prop(
            "Checksum",
            if word(0x1E) == sum && word(0x1C) ^ word(0x1E) == 0xFFFF {
                "matches"
            } else {
                "doesn't match"
            },
        ),
    ];
    let title = header_text(&h[..21]);
    Some(RomParts {
        platform: Platform::Snes,
        cpu: Cpu::W65816,
        format_name: format!(
            "{}{}",
            if hirom { "HiROM" } else { "LoROM" },
            if copier > 0 { " with a copier header" } else { "" }
        ),
        title: (!title.is_empty()).then_some(title),
        endian: Endian::Little,
        bits: 16,
        areas,
        map,
        vectors,
        labels,
        properties,
        data: None,
        entries: Vec::new(),
        state: State::RESET,
        layout,
    })
}

const HEADER: [F; 22] = [
    F::str("Title", 21),
    F::u8("Map mode", Fmt::Flags(map_mode)),
    F::u8("Chipset", Fmt::Flags(chipset)),
    F::u8("ROM size", Fmt::Flags(size_kib)),
    F::u8("RAM size", Fmt::Flags(size_kib)),
    F::u8("Country", Fmt::Name(country)),
    F::u8("Developer ID", Fmt::Hex),
    F::u8("Version", Fmt::Dec),
    F::u16("Checksum complement", Fmt::Hex),
    F::u16("Checksum", Fmt::Hex),
    F::bytes("Unused", 4),
    F::u16("Native COP vector", Fmt::Hex),
    F::u16("Native BRK vector", Fmt::Hex),
    F::u16("Native ABORT vector", Fmt::Hex),
    F::u16("Native NMI vector", Fmt::Hex),
    F::bytes("Unused", 2),
    F::u16("Native IRQ vector", Fmt::Hex),
    F::bytes("Unused", 6),
    F::u16("Emulation ABORT vector", Fmt::Hex),
    F::u16("Emulation NMI vector", Fmt::Hex),
    F::u16("Emulation RESET vector", Fmt::Hex),
    F::u16("Emulation IRQ/BRK vector", Fmt::Hex),
];

fn layout(b: &mut Builder<'_>) {
    let data = b.ctx.bytes.data;
    let Some((at, hirom)) = find_header(data) else { return };
    let copier = if data.len() % 1024 == 512 { 512u64 } else { 0 };
    if copier > 0 {
        b.region(0, 512, RegionKind::Header, "Copier header");
    }
    let chunk = if hirom { 0x10000u64 } else { 0x8000 };
    let len = data.len() as u64 - copier;
    for n in 0..len.div_ceil(chunk) {
        let bank = if hirom { 0xC0 + n } else { n };
        b.region(
            copier + n * chunk,
            chunk.min(len - n * chunk),
            RegionKind::Code,
            format!("ROM bank ${bank:02X}"),
        );
    }
    b.struct_region(at, RegionKind::Header, "Internal header and vectors", &HEADER);
}
