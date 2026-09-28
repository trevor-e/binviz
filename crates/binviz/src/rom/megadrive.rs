//! The Mega Drive / Genesis: the 68000's vector table (the stack, where to
//! start, where each exception and interrupt goes), then at `$100` the
//! header (console, names, serial, a checksum of everything after `$200`,
//! region). The ROM is at `$000000`; the VDP, the Z80's space and RAM
//! further up the 24-bit bus. `.smd` copier dumps (interleaved in 16 KiB
//! blocks, behind a 512-byte header) are read as plain ROMs.

use super::{Area, Flat, Map, Platform, RomParts, header_text, prop};
use crate::cpu::{Cpu, State};
use crate::layout::Builder;
use crate::layout::fields::{F, Fmt};
use crate::model::RegionKind;
use crate::util::Endian;

#[rustfmt::skip]
const REGISTERS: [(&str, u64, u64); 22] = [
    ("VDP_DATA", 0xC0_0000, 4), ("VDP_CTRL", 0xC0_0004, 4), ("HV_COUNTER", 0xC0_0008, 2), ("PSG", 0xC0_0011, 1),
    ("YM2612_ADDR0", 0xA0_4000, 1), ("YM2612_DATA0", 0xA0_4001, 1), ("YM2612_ADDR1", 0xA0_4002, 1),
    ("YM2612_DATA1", 0xA0_4003, 1), ("IO_VERSION", 0xA1_0001, 1), ("IO_DATA1", 0xA1_0003, 1),
    ("IO_DATA2", 0xA1_0005, 1), ("IO_DATA3", 0xA1_0007, 1), ("IO_CTRL1", 0xA1_0009, 1), ("IO_CTRL2", 0xA1_000B, 1),
    ("IO_CTRL3", 0xA1_000D, 1), ("IO_TXDATA1", 0xA1_000F, 1), ("IO_RXDATA1", 0xA1_0011, 1), ("IO_SCTRL1", 0xA1_0013, 1),
    ("Z80_BUSREQ", 0xA1_1100, 2), ("Z80_RESET", 0xA1_1200, 2), ("TMSS", 0xA1_4000, 4), ("SRAM_CTRL", 0xA1_30F1, 1),
];

/// The vectors, as named in the order the handlers are: the reset first,
/// then the interrupts, then the exceptions and traps.
fn vector_names() -> Vec<(&'static str, usize)> {
    let mut v = vec![
        ("reset", 1),
        ("vblank", 30),
        ("hblank", 28),
        ("external", 26),
        ("bus_error", 2),
        ("address_error", 3),
        ("illegal_instruction", 4),
        ("zero_divide", 5),
        ("chk", 6),
        ("trapv", 7),
        ("privilege_violation", 8),
        ("trace", 9),
        ("line_a", 10),
        ("line_f", 11),
        ("spurious", 24),
    ];
    const TRAPS: [&str; 16] = [
        "trap0", "trap1", "trap2", "trap3", "trap4", "trap5", "trap6", "trap7", "trap8", "trap9", "trap10", "trap11",
        "trap12", "trap13", "trap14", "trap15",
    ];
    v.extend(TRAPS.iter().enumerate().map(|(i, n)| (*n, 32 + i)));
    v
}

/// Folds an address onto the one we give that memory: the 24-bit bus, RAM
/// mirrored from `$E00000`, the VDP's ports from `$C00000`.
pub(super) fn resolve(t: u64) -> Option<u64> {
    let t = t & 0xFF_FFFF;
    Some(match t {
        0xE0_0000.. => 0xFF_0000 | (t & 0xFFFF),
        0xC0_0000..0xE0_0000 => 0xC0_0000 | (t & 0x1F),
        _ => t,
    })
}

/// A `.smd` dump's bytes in ROM order.
fn deinterleaved(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() % 0x4000 != 512 || data.get(8..10) != Some(&[0xAA, 0xBB]) {
        return None;
    }
    let mut out = vec![0u8; data.len() - 512];
    for (b, block) in data[512..].chunks(0x4000).enumerate() {
        for i in 0..0x2000.min(block.len().saturating_sub(0x2000)) {
            out[b * 0x4000 + 2 * i] = block[0x2000 + i];
            out[b * 0x4000 + 2 * i + 1] = block[i];
        }
    }
    Some(out)
}

pub(super) fn detect(data: &[u8]) -> Option<RomParts> {
    let smd = deinterleaved(data);
    let rom: &[u8] = smd.as_deref().unwrap_or(data);
    if rom.len() < 0x200 || rom.len() > 8 << 20 || &rom[0x100..0x104] != b"SEGA" && &rom[0x101..0x105] != b"SEGA" {
        return None;
    }
    let long = |at: usize| u32::from_be_bytes([rom[at], rom[at + 1], rom[at + 2], rom[at + 3]]) as u64;
    let areas = vec![
        Area::rom("ROM", 0, rom.len() as u64, 0),
        Area::ram("Z80 RAM", 0xA0_0000, 0x2000),
        Area::io("YM2612", 0xA0_4000, 4),
        Area::io("I/O registers", 0xA1_0000, 0x20),
        Area::io("Z80 bus request", 0xA1_1100, 2),
        Area::io("Z80 reset", 0xA1_1200, 2),
        Area::io("SRAM control", 0xA1_30F1, 1),
        Area::io("TMSS", 0xA1_4000, 4),
        Area::io("VDP ports", 0xC0_0000, 0x20),
        Area::ram("RAM", 0xFF_0000, 0x1_0000),
    ];
    let mut vectors = Vec::new();
    for (name, i) in vector_names() {
        let t = long(4 * i) & 0xFF_FFFF;
        if t < rom.len() as u64 && t.is_multiple_of(2) {
            vectors.push((name, t));
        }
    }
    let mut labels: Vec<(String, u64, u64)> = REGISTERS.iter().map(|&(n, a, s)| (n.to_string(), a, s)).collect();
    labels.push(("vectors".into(), 0, 0x100));
    labels.push(("header".into(), 0x100, 0x100));
    let stored = u16::from_be_bytes([rom[0x18E], rom[0x18F]]);
    let sum = rom[0x200..].chunks(2).fold(0u16, |s, w| {
        s.wrapping_add(u16::from_be_bytes([w[0], *w.get(1).unwrap_or(&0)]))
    });
    let overseas = header_text(&rom[0x150..0x180]);
    let domestic = header_text(&rom[0x120..0x150]);
    let properties = vec![
        prop("System", header_text(&rom[0x100..0x110])),
        prop("Copyright", header_text(&rom[0x110..0x120])),
        prop("Domestic name", domestic.clone()),
        prop("Serial", header_text(&rom[0x180..0x18E])),
        prop("Region", header_text(&rom[0x1F0..0x1F3])),
        prop("Checksum", if stored == sum { "matches" } else { "doesn't match" }),
        prop("Initial stack", format!("${:06X}", long(0))),
    ];
    let title = if overseas.is_empty() { domestic } else { overseas };
    Some(RomParts {
        platform: Platform::MegaDrive,
        cpu: Cpu::M68000,
        format_name: if smd.is_some() {
            ".smd (interleaved; shown as .bin)"
        } else {
            "cartridge image"
        }
        .into(),
        title: (!title.is_empty()).then_some(title),
        endian: Endian::Big,
        bits: 32,
        areas,
        map: Map::Flat(Flat::MegaDrive),
        vectors,
        labels,
        properties,
        state: State::default(),
        layout,
        data: smd,
    })
}

fn vector_text(v: u64) -> String {
    format!("${v:06X}")
}

const HEADER: [F; 17] = [
    F::str("System", 16),
    F::str("Copyright and date", 16),
    F::str("Domestic name", 48),
    F::str("Overseas name", 48),
    F::str("Serial and version", 14),
    F::u16("Checksum", Fmt::Hex),
    F::str("I/O support", 16),
    F::u32("ROM start", Fmt::Hex),
    F::u32("ROM end", Fmt::Hex),
    F::u32("RAM start", Fmt::Hex),
    F::u32("RAM end", Fmt::Hex),
    F::bytes("SRAM", 12),
    F::bytes("Modem", 12),
    F::str("Notes", 40),
    F::str("Region", 3),
    F::bytes("Reserved", 13),
    F::u8("", Fmt::Hex),
];

fn layout(b: &mut Builder<'_>) {
    let len = b.ctx.bytes.data.len() as u64;
    if let Some(v) = b.region(0, 0x100, RegionKind::Header, "68000 vectors") {
        let names = vector_names();
        for i in 0..64u64 {
            let name = match i {
                0 => "Initial stack pointer".to_string(),
                _ => names
                    .iter()
                    .find(|n| n.1 as u64 == i)
                    .map_or_else(|| format!("Vector {i}"), |n| format!("{} vector", n.0)),
            };
            let id = b.child(v, 4 * i, 4, RegionKind::Header, name);
            let value = vector_text(b.ctx.bytes.u32(4 * i).unwrap_or(0) as u64);
            b.set_value(id, value);
        }
    }
    b.struct_region(0x100, RegionKind::Header, "Header", &HEADER[..16]);
    b.region(0x200, len - 0x200, RegionKind::Code, "The game");
}
