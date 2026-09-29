//! The Game Boy and Game Boy Color: the cartridge header at `$0100` (entry
//! point, logo, title, which memory bank controller, sizes, checksums),
//! bank 0 always at `$0000`, and the other 16 KiB banks switched in at
//! `$4000` by the controller.

use super::{Area, Map, Platform, RomParts, Window, header_text, prop};
use crate::cpu::{Cpu, State};
use crate::layout::Builder;
use crate::layout::fields::{F, Fmt};
use crate::model::RegionKind;
use crate::util::Endian;

/// The logo every cartridge carries (the boot ROM compares it).
#[rustfmt::skip]
const LOGO: [u8; 48] = [
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
    0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99,
    0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
];

/// The I/O registers, as Pan Docs names them.
#[rustfmt::skip]
const REGISTERS: [(&str, u64); 57] = [
    ("P1", 0xFF00), ("SB", 0xFF01), ("SC", 0xFF02), ("DIV", 0xFF04), ("TIMA", 0xFF05), ("TMA", 0xFF06),
    ("TAC", 0xFF07), ("IF", 0xFF0F), ("NR10", 0xFF10), ("NR11", 0xFF11), ("NR12", 0xFF12), ("NR13", 0xFF13),
    ("NR14", 0xFF14), ("NR21", 0xFF16), ("NR22", 0xFF17), ("NR23", 0xFF18), ("NR24", 0xFF19), ("NR30", 0xFF1A),
    ("NR31", 0xFF1B), ("NR32", 0xFF1C), ("NR33", 0xFF1D), ("NR34", 0xFF1E), ("NR41", 0xFF20), ("NR42", 0xFF21),
    ("NR43", 0xFF22), ("NR44", 0xFF23), ("NR50", 0xFF24), ("NR51", 0xFF25), ("NR52", 0xFF26), ("LCDC", 0xFF40),
    ("STAT", 0xFF41), ("SCY", 0xFF42), ("SCX", 0xFF43), ("LY", 0xFF44), ("LYC", 0xFF45), ("DMA", 0xFF46),
    ("BGP", 0xFF47), ("OBP0", 0xFF48), ("OBP1", 0xFF49), ("WY", 0xFF4A), ("WX", 0xFF4B), ("KEY1", 0xFF4D),
    ("VBK", 0xFF4F), ("HDMA1", 0xFF51), ("HDMA2", 0xFF52), ("HDMA3", 0xFF53), ("HDMA4", 0xFF54), ("HDMA5", 0xFF55),
    ("RP", 0xFF56), ("BCPS", 0xFF68), ("BCPD", 0xFF69), ("OCPS", 0xFF6A), ("OCPD", 0xFF6B), ("OPRI", 0xFF6C),
    ("SVBK", 0xFF70), ("PCM12", 0xFF76), ("IE", 0xFFFF),
];

fn cartridge(t: u64) -> Option<&'static str> {
    Some(match t {
        0x00 => "ROM only",
        0x01 => "MBC1",
        0x02 => "MBC1 + RAM",
        0x03 => "MBC1 + RAM + battery",
        0x05 => "MBC2",
        0x06 => "MBC2 + battery",
        0x08 => "ROM + RAM",
        0x09 => "ROM + RAM + battery",
        0x0B => "MMM01",
        0x0C => "MMM01 + RAM",
        0x0D => "MMM01 + RAM + battery",
        0x0F => "MBC3 + timer + battery",
        0x10 => "MBC3 + timer + RAM + battery",
        0x11 => "MBC3",
        0x12 => "MBC3 + RAM",
        0x13 => "MBC3 + RAM + battery",
        0x19 => "MBC5",
        0x1A => "MBC5 + RAM",
        0x1B => "MBC5 + RAM + battery",
        0x1C => "MBC5 + rumble",
        0x1D => "MBC5 + rumble + RAM",
        0x1E => "MBC5 + rumble + RAM + battery",
        0x20 => "MBC6",
        0x22 => "MBC7 + sensor + rumble + RAM + battery",
        0xFC => "Pocket Camera",
        0xFD => "Bandai TAMA5",
        0xFE => "HuC3",
        0xFF => "HuC1 + RAM + battery",
        _ => return None,
    })
}

fn rom_size(v: u64) -> String {
    if v <= 8 {
        format!("{} KiB ({} banks)", 32 << v, 2 << v)
    } else {
        format!("{v:#04x}")
    }
}

fn ram_size(v: u64) -> String {
    match v {
        0 => "none".into(),
        1 => "2 KiB".into(),
        2 => "8 KiB".into(),
        3 => "32 KiB (4 banks)".into(),
        4 => "128 KiB (16 banks)".into(),
        5 => "64 KiB (8 banks)".into(),
        _ => format!("{v:#04x}"),
    }
}

fn cgb(v: u64) -> Option<&'static str> {
    match v {
        0x80 => Some("works on the Game Boy Color too"),
        0xC0 => Some("Game Boy Color only"),
        _ => None,
    }
}

fn header_checksum(data: &[u8]) -> u8 {
    data[0x134..=0x14C]
        .iter()
        .fold(0u8, |x, &b| x.wrapping_sub(b).wrapping_sub(1))
}

fn global_checksum(data: &[u8]) -> u16 {
    data.iter()
        .enumerate()
        .filter(|&(i, _)| i != 0x14E && i != 0x14F)
        .fold(0u16, |x, (_, &b)| x.wrapping_add(b as u16))
}

pub(super) fn detect(data: &[u8]) -> Option<RomParts> {
    if data.len() < 0x8000 || data.len() > 8 << 20 {
        return None;
    }
    let logo = data[0x104..0x134] == LOGO;
    let checked = header_checksum(data) == data[0x14D];
    if !logo && !(checked && cartridge(data[0x147] as u64).is_some()) {
        return None;
    }
    let banks = (data.len() as u64 / 0x4000).max(2);
    let mut areas = vec![Area::rom("ROM bank 0", 0, 0x4000, 0)];
    let mut switched = Vec::new();
    for n in 1..banks {
        let base = n << 16 | 0x4000;
        areas.push(Area::rom(format!("ROM bank {n}"), base, 0x4000, n * 0x4000));
        switched.push(base);
    }
    areas.extend([
        Area::ram("VRAM", 0x8000, 0x2000),
        Area::ram("Cartridge RAM", 0xA000, 0x2000),
        Area::ram("WRAM", 0xC000, 0x2000),
        Area::ram("OAM", 0xFE00, 0xA0),
        Area::io("I/O registers", 0xFF00, 0x80),
        Area::ram("HRAM", 0xFF80, 0x7F),
        Area::io("Interrupt enable", 0xFFFF, 1),
    ]);
    let map = Map::Banked(vec![
        Window::fixed(0, 0x4000, 0),
        Window {
            lo: 0x4000,
            hi: 0x8000,
            size: 0x4000,
            banks: switched,
        },
        Window::fixed(0x8000, 0xA000, 0x8000),
        Window::fixed(0xA000, 0xC000, 0xA000),
        Window::fixed(0xC000, 0xE000, 0xC000),
        // Echo RAM: WRAM again.
        Window {
            lo: 0xE000,
            hi: 0xFE00,
            size: 0x2000,
            banks: vec![0xC000],
        },
        Window::fixed(0xFE00, 0xFEA0, 0xFE00),
        Window::fixed(0xFF00, 0xFF80, 0xFF00),
        Window::fixed(0xFF80, 0xFFFF, 0xFF80),
        Window::fixed(0xFFFF, 0x10000, 0xFFFF),
    ]);
    let color = data[0x143] & 0x80 != 0;
    // The title is 16 bytes, or 11 when the Game Boy Color fields follow it.
    let title = header_text(&data[0x134..if color { 0x13F } else { 0x144 }]);
    let stored = u16::from_be_bytes([data[0x14E], data[0x14F]]);
    let properties = vec![
        prop(
            "Cartridge",
            cartridge(data[0x147] as u64).map_or_else(|| format!("{:#04x}", data[0x147]), str::to_string),
        ),
        prop("ROM size", rom_size(data[0x148] as u64)),
        prop("RAM size", ram_size(data[0x149] as u64)),
        prop("Game Boy Color", cgb(data[0x143] as u64).unwrap_or("no")),
        prop("Super Game Boy", if data[0x146] == 3 { "yes" } else { "no" }),
        prop("Destination", if data[0x14A] == 0 { "Japan" } else { "overseas" }),
        prop("Header checksum", if checked { "matches" } else { "doesn't match" }),
        prop(
            "Global checksum",
            if global_checksum(data) == stored {
                "matches"
            } else {
                "doesn't match (the hardware doesn't check it)"
            },
        ),
    ];
    let vectors = vec![
        ("entry", 0x0100),
        ("vblank", 0x0040),
        ("lcd_stat", 0x0048),
        ("timer", 0x0050),
        ("serial", 0x0058),
        ("joypad", 0x0060),
    ];
    let mut labels: Vec<(String, u64, u64)> = REGISTERS.iter().map(|&(n, a)| (n.to_string(), a, 1)).collect();
    labels.push(("WAVE_RAM".into(), 0xFF30, 16));
    labels.push(("header".into(), 0x0104, 0x4C));
    Some(RomParts {
        platform: if color {
            Platform::GameBoyColor
        } else {
            Platform::GameBoy
        },
        cpu: Cpu::Sm83,
        format_name: "cartridge image".into(),
        title: (!title.is_empty()).then_some(title),
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
    })
}

fn destination(v: u64) -> Option<&'static str> {
    match v {
        0 => Some("Japan"),
        1 => Some("overseas"),
        _ => None,
    }
}

const HEADER: [F; 14] = [
    F::bytes("Entry point", 4),
    F::bytes("Nintendo logo", 48),
    F::str("Title", 15),
    F::u8("Game Boy Color flag", Fmt::Name(cgb)),
    F::str("New licensee code", 2),
    F::u8("Super Game Boy flag", Fmt::Hex),
    F::u8("Cartridge type", Fmt::Name(cartridge)),
    F::u8("ROM size", Fmt::Flags(rom_size)),
    F::u8("RAM size", Fmt::Flags(ram_size)),
    F::u8("Destination", Fmt::Name(destination)),
    F::u8("Old licensee code", Fmt::Hex),
    F::u8("Version", Fmt::Dec),
    F::u8("Header checksum", Fmt::Hex),
    F::bytes("Global checksum (big-endian)", 2),
];

fn layout(b: &mut Builder<'_>) {
    let len = b.ctx.bytes.data.len() as u64;
    for n in 0..len / 0x4000 {
        b.region(n * 0x4000, 0x4000, RegionKind::Code, format!("ROM bank {n}"));
    }
    b.region(0, 0x100, RegionKind::Code, "RST and interrupt vectors");
    b.struct_region(0x100, RegionKind::Header, "Cartridge header", &HEADER);
}
