//! The NES: iNES and NES 2.0 files. A 16-byte header, a trainer sometimes,
//! then the program (PRG ROM, which the 6502 runs from `$8000`) and the
//! graphics (CHR ROM, which only the PPU sees). Mappers switch PRG banks in
//! and out of the CPU's view; the common arrangement is modelled: 16 KiB
//! banks switched at `$8000`, the last one fixed at `$C000` (with the
//! vectors), or whole 32 KiB banks for the mappers that switch those. A
//! code/data log that saw banks elsewhere (MMC3's 8 KiB pages at `$A000`...)
//! places each 8 KiB page where it ran.

use super::{Area, Map, Platform, RomParts, Window, prop};
use crate::cpu::{Cpu, State};
use crate::layout::Builder;
use crate::layout::fields::{F, Fmt};
use crate::model::RegionKind;
use crate::util::Endian;

/// The PPU's and the APU's registers, as the NESdev wiki names them.
#[rustfmt::skip]
const REGISTERS: [(&str, u64); 30] = [
    ("PPUCTRL", 0x2000), ("PPUMASK", 0x2001), ("PPUSTATUS", 0x2002), ("OAMADDR", 0x2003),
    ("OAMDATA", 0x2004), ("PPUSCROLL", 0x2005), ("PPUADDR", 0x2006), ("PPUDATA", 0x2007),
    ("SQ1_VOL", 0x4000), ("SQ1_SWEEP", 0x4001), ("SQ1_LO", 0x4002), ("SQ1_HI", 0x4003),
    ("SQ2_VOL", 0x4004), ("SQ2_SWEEP", 0x4005), ("SQ2_LO", 0x4006), ("SQ2_HI", 0x4007),
    ("TRI_LINEAR", 0x4008), ("TRI_LO", 0x400A), ("TRI_HI", 0x400B), ("NOISE_VOL", 0x400C),
    ("NOISE_LO", 0x400E), ("NOISE_HI", 0x400F), ("DMC_FREQ", 0x4010), ("DMC_RAW", 0x4011),
    ("DMC_START", 0x4012), ("DMC_LEN", 0x4013), ("OAMDMA", 0x4014), ("SND_CHN", 0x4015),
    ("JOY1", 0x4016), ("JOY2", 0x4017),
];

fn mapper_name(n: u16) -> Option<&'static str> {
    Some(match n {
        0 => "NROM",
        1 => "MMC1",
        2 => "UxROM",
        3 => "CNROM",
        4 => "MMC3",
        5 => "MMC5",
        7 => "AxROM",
        9 => "MMC2",
        10 => "MMC4",
        11 => "Color Dreams",
        19 => "Namco 163",
        21..=23 | 25 => "Konami VRC2/VRC4",
        24 | 26 => "Konami VRC6",
        34 => "BNROM / NINA-001",
        66 => "GxROM",
        69 => "Sunsoft FME-7",
        71 => "Camerica",
        85 => "Konami VRC7",
        206 => "Namco 118",
        _ => return None,
    })
}

/// Mappers that switch the whole 32 KiB at `$8000` at once.
fn switches_32k(mapper: u16) -> bool {
    matches!(mapper, 7 | 11 | 34 | 38 | 66 | 140)
}

pub(super) struct Header {
    nes2: bool,
    mapper: u16,
    pub prg_offset: u64,
    pub prg_size: u64,
    chr_offset: u64,
    pub chr_size: u64,
}

pub(super) fn header(data: &[u8]) -> Option<Header> {
    if data.len() < 16 || &data[..4] != b"NES\x1A" {
        return None;
    }
    let nes2 = data[7] & 0x0C == 0x08;
    let mapper =
        (data[6] >> 4) as u16 | (data[7] & 0xF0) as u16 | if nes2 { ((data[8] & 0x0F) as u16) << 8 } else { 0 };
    let size = |low: u8, high: u8, unit: u64| -> u64 {
        if nes2 && high == 0xF {
            // Exponent-multiplier notation: 2^E * (2M + 1) bytes.
            (1u64 << (low >> 2).min(40)) * ((low & 3) as u64 * 2 + 1)
        } else {
            (low as u64 | if nes2 { (high as u64) << 8 } else { 0 }) * unit
        }
    };
    let prg_offset = 16 + if data[6] & 4 != 0 { 512 } else { 0 };
    let len = data.len() as u64;
    let prg_size = size(data[4], data[9] & 0x0F, 16 * 1024).min(len.saturating_sub(prg_offset));
    let chr_offset = prg_offset + prg_size;
    let chr_size = size(data[5], data[9] >> 4, 8 * 1024).min(len.saturating_sub(chr_offset));
    Some(Header {
        nes2,
        mapper,
        prg_offset,
        prg_size,
        chr_offset,
        chr_size,
    })
}

fn kib(n: u64) -> String {
    if n >= 1024 && n.is_multiple_of(1024) {
        format!("{} KiB", n / 1024)
    } else {
        format!("{n} bytes")
    }
}

pub(super) fn detect(data: &[u8]) -> Option<RomParts> {
    detect_with(data, None).map(|(parts, _)| parts)
}

/// The CPU window each 8 KiB page of PRG ROM is in, as the usual arrangement
/// has it: the last 16 KiB fixed at `$C000`, the others switched in at `$8000`.
fn usual_window(page: u64, pages: u64) -> u64 {
    if page + 2 >= pages {
        0xC000 + (page + 2 - pages) * 0x2000
    } else {
        0x8000 + (page % 2) * 0x2000
    }
}

/// Reads a ROM; with `logged` (the window each 8 KiB page of PRG ROM ran in,
/// from a code/data log), switched banks are placed as 8 KiB pages where the
/// log saw them, when that isn't the usual arrangement. Returns how many
/// pages were placed from the log.
pub(super) fn detect_with(data: &[u8], logged: Option<&[Option<u64>]>) -> Option<(RomParts, u32)> {
    let h = header(data)?;
    if h.prg_size == 0 {
        return None;
    }
    let mut placed = 0;
    let mut areas = vec![
        Area::ram("RAM", 0x0000, 0x800),
        Area::io("PPU registers", 0x2000, 8),
        Area::io("APU and I/O registers", 0x4000, 0x18),
        Area::ram("PRG RAM", 0x6000, 0x2000),
    ];
    let mut windows = vec![
        Window {
            lo: 0,
            hi: 0x2000,
            size: 0x800,
            banks: vec![0],
        },
        Window {
            lo: 0x2000,
            hi: 0x4000,
            size: 8,
            banks: vec![0x2000],
        },
        Window::fixed(0x4000, 0x4020, 0x4000),
        Window::fixed(0x6000, 0x8000, 0x6000),
    ];
    // Pages the log saw somewhere the usual arrangement doesn't put them.
    let pages = h.prg_size / 0x2000;
    let paged = logged.filter(|l| {
        h.prg_size > 0x8000
            && !switches_32k(h.mapper)
            && (0..pages).any(|p| {
                l.get(p as usize)
                    .copied()
                    .flatten()
                    .is_some_and(|w| w != usual_window(p, pages))
            })
    });
    // Where each PRG bank sits, and where the vectors are.
    let vectors_at = if let Some(logged) = paged {
        let mut at: [Vec<u64>; 4] = Default::default();
        for p in 0..pages {
            let seen = logged.get(p as usize).copied().flatten();
            placed += seen.is_some() as u32;
            let w = seen.unwrap_or_else(|| usual_window(p, pages));
            let base = p << 16 | w;
            areas.push(Area::rom(
                format!("PRG page {p} (at ${w:04X})"),
                base,
                0x2000,
                h.prg_offset + p * 0x2000,
            ));
            at[((w - 0x8000) / 0x2000) as usize].push(base);
        }
        for (i, banks) in at.into_iter().enumerate() {
            if !banks.is_empty() {
                let lo = 0x8000 + i as u64 * 0x2000;
                windows.push(Window {
                    lo,
                    hi: lo + 0x2000,
                    size: 0x2000,
                    banks,
                });
            }
        }
        let last = pages - 1;
        let w = logged.get(last as usize).copied().flatten().unwrap_or(0xE000);
        last << 16 | (w + 0x1FFA)
    } else if h.prg_size <= 0x8000 {
        let window = h.prg_size.max(0x4000).next_power_of_two().min(0x8000);
        let base = 0x10000 - window;
        areas.push(Area::rom("PRG ROM", base, h.prg_size, h.prg_offset));
        windows.push(Window {
            lo: 0x8000,
            hi: 0x10000,
            size: window,
            banks: vec![base],
        });
        0xFFFA
    } else if switches_32k(h.mapper) {
        let count = h.prg_size / 0x8000;
        let mut banks = Vec::new();
        for n in 0..count {
            let base = n << 16 | 0x8000;
            areas.push(Area::rom(
                format!("PRG bank {n}"),
                base,
                0x8000,
                h.prg_offset + n * 0x8000,
            ));
            banks.push(base);
        }
        windows.push(Window {
            lo: 0x8000,
            hi: 0x10000,
            size: 0x8000,
            banks,
        });
        // Which bank is there at power-on varies; the last is the usual place for the vectors.
        (count - 1) << 16 | 0xFFFA
    } else {
        let count = h.prg_size / 0x4000;
        let last = count - 1;
        let mut banks = Vec::new();
        for n in 0..last {
            let base = n << 16 | 0x8000;
            areas.push(Area::rom(
                format!("PRG bank {n}"),
                base,
                0x4000,
                h.prg_offset + n * 0x4000,
            ));
            banks.push(base);
        }
        let fixed = last << 16 | 0xC000;
        areas.push(Area::rom(
            format!("PRG bank {last} (fixed)"),
            fixed,
            0x4000,
            h.prg_offset + last * 0x4000,
        ));
        windows.push(Window {
            lo: 0x8000,
            hi: 0xC000,
            size: 0x4000,
            banks,
        });
        windows.push(Window::fixed(0xC000, 0x10000, fixed));
        fixed | 0x3FFA
    };
    if h.chr_size > 0 {
        areas.push(Area {
            name: "CHR ROM".into(),
            address: None,
            size: h.chr_size,
            file_offset: Some(h.chr_offset),
            kind: RegionKind::Rodata,
            perms: "r--",
        });
    }
    let map = Map::Banked(windows);
    // The vectors: NMI, reset and IRQ, the last six bytes of the bank at $FFFA.
    let vector_file = h.prg_offset + h.prg_size - 6;
    let word = |i: u64| {
        let at = (vector_file + 2 * i) as usize;
        data.get(at..at + 2).map(|w| u16::from_le_bytes([w[0], w[1]]) as u64)
    };
    let mut vectors = Vec::new();
    for (name, i) in [("reset", 1), ("nmi", 0), ("irq", 2)] {
        if let Some(t) = word(i).and_then(|t| map.resolve(vectors_at, t)) {
            vectors.push((name, t));
        }
    }
    let mut labels: Vec<(String, u64, u64)> = REGISTERS.iter().map(|&(n, a)| (n.to_string(), a, 1)).collect();
    labels.push(("vectors".into(), vectors_at, 6));
    let mirroring = if data[6] & 8 != 0 {
        "four-screen"
    } else if data[6] & 1 != 0 {
        "vertical"
    } else {
        "horizontal"
    };
    let properties = vec![
        prop(
            "Mapper",
            match mapper_name(h.mapper) {
                Some(n) => format!("{} ({n})", h.mapper),
                None => h.mapper.to_string(),
            },
        ),
        prop("PRG ROM", kib(h.prg_size)),
        prop(
            "CHR ROM",
            if h.chr_size > 0 {
                kib(h.chr_size)
            } else {
                "none (CHR RAM)".into()
            },
        ),
        prop("Mirroring", mirroring),
        prop("Battery", if data[6] & 2 != 0 { "yes" } else { "no" }),
    ];
    let parts = RomParts {
        platform: Platform::Nes,
        cpu: Cpu::Mos6502,
        format_name: if h.nes2 { "NES 2.0" } else { "iNES" }.into(),
        title: None,
        endian: Endian::Little,
        bits: 8,
        areas,
        map,
        vectors,
        labels,
        properties,
        data: None,
        entries: Vec::new(),
        late_entries: Vec::new(),
        state: State::default(),
        layout,
    };
    Some((parts, placed))
}

fn flags6(v: u64) -> String {
    let mut parts = vec![
        if v & 8 != 0 {
            "four-screen"
        } else if v & 1 != 0 {
            "vertical mirroring"
        } else {
            "horizontal mirroring"
        }
        .to_string(),
    ];
    if v & 2 != 0 {
        parts.push("battery".into());
    }
    if v & 4 != 0 {
        parts.push("trainer".into());
    }
    parts.push(format!("mapper low nibble {}", v >> 4));
    parts.join(", ")
}

fn flags7(v: u64) -> String {
    let format = if v & 0x0C == 0x08 { "NES 2.0" } else { "iNES" };
    let console = match v & 3 {
        1 => ", Vs. System",
        2 => ", PlayChoice-10",
        3 => ", extended console type",
        _ => "",
    };
    format!("{format}{console}, mapper high nibble {}", v >> 4)
}

const HEADER: [F; 9] = [
    F::bytes("Magic", 4),
    F::u8("PRG ROM size (16 KiB units)", Fmt::Dec),
    F::u8("CHR ROM size (8 KiB units)", Fmt::Dec),
    F::u8("Flags 6", Fmt::Flags(flags6)),
    F::u8("Flags 7", Fmt::Flags(flags7)),
    F::u8("Flags 8 (PRG RAM size, or NES 2.0 mapper MSB/submapper)", Fmt::Hex),
    F::u8("Flags 9 (TV system, or NES 2.0 ROM size MSBs)", Fmt::Hex),
    F::u8("Flags 10", Fmt::Hex),
    F::bytes("Padding / NES 2.0 fields", 5),
];

const VECTORS: [F; 3] = [
    F::u16("NMI vector", Fmt::Hex),
    F::u16("Reset vector", Fmt::Hex),
    F::u16("IRQ/BRK vector", Fmt::Hex),
];

fn layout(b: &mut Builder<'_>) {
    let Some(h) = header(b.ctx.bytes.data) else { return };
    b.struct_region(0, RegionKind::Header, "iNES header", &HEADER);
    if h.prg_offset > 16 {
        b.region(16, 512, RegionKind::Code, "Trainer (loaded at $7000)");
    }
    if let Some(prg) = b.region(h.prg_offset, h.prg_size, RegionKind::Code, "PRG ROM") {
        let bank = if h.prg_size > 0x8000 && !switches_32k(h.mapper) {
            0x4000
        } else {
            0x8000.min(h.prg_size)
        };
        let count = h.prg_size / bank;
        let mut last = prg;
        if count > 1 {
            for n in 0..count {
                if let Some(id) = b.child(
                    prg,
                    h.prg_offset + n * bank,
                    bank,
                    RegionKind::Code,
                    format!("PRG bank {n}"),
                ) {
                    last = id;
                }
            }
        }
        b.struct_child(
            last,
            h.prg_offset + h.prg_size - 6,
            RegionKind::Metadata,
            "Vectors",
            &VECTORS,
        );
    }
    if h.chr_size > 0
        && let Some(chr) = b.region(
            h.chr_offset,
            h.chr_size,
            RegionKind::Rodata,
            "CHR ROM (tiles, 2 bits per pixel)",
        )
    {
        for n in 0..h.chr_size / 0x2000 {
            b.child(
                chr,
                h.chr_offset + n * 0x2000,
                0x2000,
                RegionKind::Rodata,
                format!("CHR bank {n}"),
            );
        }
    }
    let end = h.chr_offset + h.chr_size;
    let len = b.ctx.bytes.data.len() as u64;
    if end < len {
        b.region(
            end,
            len - end,
            RegionKind::Overlay,
            "After the ROM (PlayChoice data, or extra)",
        );
    }
}
