//! The Game Boy Advance: the cartridge starts with an ARM branch to the
//! game's start, then the header (logo, title, game code, a checksum). The
//! ROM is at `0x08000000` (mirrored at `0x0A000000` and `0x0C000000`, with
//! other wait states); the game runs in ARM and Thumb code.

use super::{Area, Flat, Map, Platform, RomParts, header_text, prop};
use crate::cpu::{Cpu, State};
use crate::layout::Builder;
use crate::layout::fields::{F, Fmt};
use crate::model::RegionKind;
use crate::util::Endian;

/// The I/O registers, as GBATEK names them.
#[rustfmt::skip]
const REGISTERS: [(&str, u64, u64); 90] = [
    ("DISPCNT", 0x000, 2), ("GREENSWAP", 0x002, 2), ("DISPSTAT", 0x004, 2), ("VCOUNT", 0x006, 2),
    ("BG0CNT", 0x008, 2), ("BG1CNT", 0x00A, 2), ("BG2CNT", 0x00C, 2), ("BG3CNT", 0x00E, 2),
    ("BG0HOFS", 0x010, 2), ("BG0VOFS", 0x012, 2), ("BG1HOFS", 0x014, 2), ("BG1VOFS", 0x016, 2),
    ("BG2HOFS", 0x018, 2), ("BG2VOFS", 0x01A, 2), ("BG3HOFS", 0x01C, 2), ("BG3VOFS", 0x01E, 2),
    ("BG2PA", 0x020, 2), ("BG2PB", 0x022, 2), ("BG2PC", 0x024, 2), ("BG2PD", 0x026, 2),
    ("BG2X", 0x028, 4), ("BG2Y", 0x02C, 4), ("BG3PA", 0x030, 2), ("BG3PB", 0x032, 2),
    ("BG3PC", 0x034, 2), ("BG3PD", 0x036, 2), ("BG3X", 0x038, 4), ("BG3Y", 0x03C, 4),
    ("WIN0H", 0x040, 2), ("WIN1H", 0x042, 2), ("WIN0V", 0x044, 2), ("WIN1V", 0x046, 2),
    ("WININ", 0x048, 2), ("WINOUT", 0x04A, 2), ("MOSAIC", 0x04C, 2), ("BLDCNT", 0x050, 2),
    ("BLDALPHA", 0x052, 2), ("BLDY", 0x054, 2), ("SOUND1CNT_L", 0x060, 2), ("SOUND1CNT_H", 0x062, 2),
    ("SOUND1CNT_X", 0x064, 2), ("SOUND2CNT_L", 0x068, 2), ("SOUND2CNT_H", 0x06C, 2), ("SOUND3CNT_L", 0x070, 2),
    ("SOUND3CNT_H", 0x072, 2), ("SOUND3CNT_X", 0x074, 2), ("SOUND4CNT_L", 0x078, 2), ("SOUND4CNT_H", 0x07C, 2),
    ("SOUNDCNT_L", 0x080, 2), ("SOUNDCNT_H", 0x082, 2), ("SOUNDCNT_X", 0x084, 2), ("SOUNDBIAS", 0x088, 2),
    ("WAVE_RAM", 0x090, 16), ("FIFO_A", 0x0A0, 4), ("FIFO_B", 0x0A4, 4), ("DMA0SAD", 0x0B0, 4),
    ("DMA0DAD", 0x0B4, 4), ("DMA0CNT_L", 0x0B8, 2), ("DMA0CNT_H", 0x0BA, 2), ("DMA1SAD", 0x0BC, 4),
    ("DMA1DAD", 0x0C0, 4), ("DMA1CNT_L", 0x0C4, 2), ("DMA1CNT_H", 0x0C6, 2), ("DMA2SAD", 0x0C8, 4),
    ("DMA2DAD", 0x0CC, 4), ("DMA2CNT_L", 0x0D0, 2), ("DMA2CNT_H", 0x0D2, 2), ("DMA3SAD", 0x0D4, 4),
    ("DMA3DAD", 0x0D8, 4), ("DMA3CNT_L", 0x0DC, 2), ("DMA3CNT_H", 0x0DE, 2), ("TM0CNT_L", 0x100, 2),
    ("TM0CNT_H", 0x102, 2), ("TM1CNT_L", 0x104, 2), ("TM1CNT_H", 0x106, 2), ("TM2CNT_L", 0x108, 2),
    ("TM2CNT_H", 0x10A, 2), ("TM3CNT_L", 0x10C, 2), ("TM3CNT_H", 0x10E, 2), ("SIODATA32", 0x120, 4),
    ("SIOCNT", 0x128, 2), ("SIOMLT_SEND", 0x12A, 2), ("KEYINPUT", 0x130, 2), ("KEYCNT", 0x132, 2),
    ("RCNT", 0x134, 2), ("IE", 0x200, 2), ("IF", 0x202, 2), ("WAITCNT", 0x204, 2),
    ("IME", 0x208, 4), ("HALTCNT", 0x301, 1),
];

/// Folds an address onto the one we give that memory (its first mirror).
pub(super) fn resolve(t: u64) -> Option<u64> {
    let t = t & 0xFFFF_FFFF;
    Some(match t >> 24 {
        0x02 => 0x0200_0000 | (t & 0x3_FFFF),
        0x03 => 0x0300_0000 | (t & 0x7FFF),
        0x05 => 0x0500_0000 | (t & 0x3FF),
        0x06 => {
            let o = t & 0x1_FFFF;
            0x0600_0000 | if o >= 0x1_8000 { o - 0x8000 } else { o }
        }
        0x07 => 0x0700_0000 | (t & 0x3FF),
        0x08..=0x0D => 0x0800_0000 | (t & 0x1FF_FFFF),
        0x0E | 0x0F => 0x0E00_0000 | (t & 0xFFFF),
        _ => t,
    })
}

fn complement(data: &[u8]) -> u8 {
    data[0xA0..=0xBC]
        .iter()
        .fold(0u8, |c, &b| c.wrapping_sub(b))
        .wrapping_sub(0x19)
}

pub(super) fn detect(data: &[u8]) -> Option<RomParts> {
    // An ARM branch (always) first, and the header's fixed byte.
    if data.len() < 0xC0 || data[3] != 0xEA || data[0xB2] != 0x96 || data.len() > 32 << 20 {
        return None;
    }
    let areas = vec![
        Area::rom("ROM", 0x0800_0000, data.len() as u64, 0),
        Area {
            name: "BIOS".into(),
            address: Some(0),
            size: 0x4000,
            file_offset: None,
            kind: RegionKind::Code,
            perms: "r-x",
        },
        Area::ram("EWRAM", 0x0200_0000, 0x4_0000),
        Area::ram("IWRAM", 0x0300_0000, 0x8000),
        Area::io("I/O registers", 0x0400_0000, 0x400),
        Area::ram("Palette RAM", 0x0500_0000, 0x400),
        Area::ram("VRAM", 0x0600_0000, 0x1_8000),
        Area::ram("OAM", 0x0700_0000, 0x400),
        Area::ram("Cartridge save RAM", 0x0E00_0000, 0x1_0000),
    ];
    let mut labels: Vec<(String, u64, u64)> = REGISTERS
        .iter()
        .map(|&(n, a, size)| (n.to_string(), 0x0400_0000 + a, size))
        .collect();
    // Where the BIOS looks for the game's interrupt handler.
    labels.push(("IRQ_HANDLER".into(), 0x0300_7FFC, 4));
    labels.push(("header".into(), 0x0800_0004, 0xBC));
    let title = header_text(&data[0xA0..0xAC]);
    let code: String = data[0xAC..0xB0]
        .iter()
        .map(|&b| if b.is_ascii_graphic() { b as char } else { '?' })
        .collect();
    let maker: String = data[0xB0..0xB2]
        .iter()
        .map(|&b| if b.is_ascii_graphic() { b as char } else { '?' })
        .collect();
    let properties = vec![
        prop("Game code", code),
        prop("Maker code", maker),
        prop("Version", data[0xBC].to_string()),
        prop(
            "Header checksum",
            if complement(data) == data[0xBD] {
                "matches"
            } else {
                "doesn't match"
            },
        ),
    ];
    Some(RomParts {
        platform: Platform::GameBoyAdvance,
        cpu: Cpu::Arm7Tdmi,
        format_name: "cartridge image".into(),
        title: (!title.is_empty()).then_some(title),
        endian: Endian::Little,
        bits: 32,
        areas,
        map: Map::Flat(Flat::Gba),
        // The cartridge starts in ARM code.
        vectors: vec![("entry", 0x0800_0000)],
        labels,
        properties,
        state: State::default(),
        layout,
        data: None,
        entries: Vec::new(),
    })
}

const HEADER: [F; 12] = [
    F::bytes("Nintendo logo", 156),
    F::str("Title", 12),
    F::str("Game code", 4),
    F::str("Maker code", 2),
    F::u8("Fixed value (0x96)", Fmt::Hex),
    F::u8("Main unit code", Fmt::Hex),
    F::u8("Device type", Fmt::Hex),
    F::bytes("Reserved", 7),
    F::u8("Version", Fmt::Dec),
    F::u8("Header checksum", Fmt::Hex),
    F::bytes("Reserved", 2),
    F::u32("", Fmt::Hex),
];

fn layout(b: &mut Builder<'_>) {
    let len = b.ctx.bytes.data.len() as u64;
    if let Some(h) = b.region(0, 0xC0, RegionKind::Header, "Cartridge header") {
        b.child(h, 0, 4, RegionKind::Code, "Branch to the start (ARM)");
        b.fields(h, 4, &HEADER[..11]);
    }
    b.region(0xC0, len - 0xC0, RegionKind::Code, "The game");
}
