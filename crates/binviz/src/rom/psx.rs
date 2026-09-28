//! PlayStation executables (PS-X EXE, like a disc's `SLUS_000.01`): a
//! 2 KiB header (where to load the code, where to start, `$gp`, the stack)
//! then the code and data, loaded into RAM as they are.

use super::{Area, Map, Platform, RomParts, header_text, prop};
use crate::cpu::{Cpu, State};
use crate::layout::Builder;
use crate::layout::fields::{F, Fmt};
use crate::model::RegionKind;
use crate::util::Endian;

/// The I/O registers, as psx-spx names them.
#[rustfmt::skip]
const REGISTERS: [(&str, u64); 45] = [
    ("EXP1_BASE", 0x1F80_1000), ("EXP2_BASE", 0x1F80_1004), ("EXP1_DELAY", 0x1F80_1008), ("EXP3_DELAY", 0x1F80_100C),
    ("BIOS_DELAY", 0x1F80_1010), ("SPU_DELAY", 0x1F80_1014), ("CDROM_DELAY", 0x1F80_1018), ("EXP2_DELAY", 0x1F80_101C),
    ("COM_DELAY", 0x1F80_1020), ("JOY_DATA", 0x1F80_1040), ("JOY_STAT", 0x1F80_1044), ("JOY_MODE", 0x1F80_1048),
    ("JOY_CTRL", 0x1F80_104A), ("JOY_BAUD", 0x1F80_104E), ("SIO_DATA", 0x1F80_1050), ("SIO_STAT", 0x1F80_1054),
    ("SIO_MODE", 0x1F80_1058), ("SIO_CTRL", 0x1F80_105A), ("SIO_BAUD", 0x1F80_105E), ("RAM_SIZE", 0x1F80_1060),
    ("I_STAT", 0x1F80_1070), ("I_MASK", 0x1F80_1074), ("DPCR", 0x1F80_10F0), ("DICR", 0x1F80_10F4),
    ("CD_INDEX", 0x1F80_1800), ("CD_CMD", 0x1F80_1801), ("CD_PARAM", 0x1F80_1802), ("CD_REQ", 0x1F80_1803),
    ("GP0", 0x1F80_1810), ("GP1", 0x1F80_1814), ("MDEC_CMD", 0x1F80_1820), ("MDEC_CTRL", 0x1F80_1824),
    ("SPU_MAIN_VOL_L", 0x1F80_1D80), ("SPU_MAIN_VOL_R", 0x1F80_1D82), ("SPU_REVERB_VOL_L", 0x1F80_1D84),
    ("SPU_REVERB_VOL_R", 0x1F80_1D86), ("SPU_KEY_ON", 0x1F80_1D88), ("SPU_KEY_OFF", 0x1F80_1D8C),
    ("SPU_TRANSFER_ADDR", 0x1F80_1DA6), ("SPU_DATA", 0x1F80_1DA8), ("SPU_CTRL", 0x1F80_1DAA),
    ("SPU_TRANSFER_CTRL", 0x1F80_1DAC), ("SPU_STAT", 0x1F80_1DAE), ("POST", 0x1F80_2041), ("CACHE_CTRL", 0xFFFE_0130),
];

/// The DMA channels, each with an address, a block count and a control register.
const DMA: [&str; 7] = ["MDEC_IN", "MDEC_OUT", "GPU", "CDROM", "SPU", "PIO", "OTC"];

/// Folds an address onto the one we give that memory: RAM (mirrored four
/// times) in KSEG0, the scratchpad and I/O where the code usually names
/// them, the BIOS in KSEG1.
pub(super) fn resolve(t: u64) -> Option<u64> {
    let t = t & 0xFFFF_FFFF;
    if t >= 0xFFFE_0000 {
        return Some(t);
    }
    let physical = t & 0x1FFF_FFFF;
    Some(match physical {
        0..0x80_0000 => 0x8000_0000 + (physical & 0x1F_FFFF),
        0x1F80_0000..0x1F80_0400 | 0x1F80_1000..0x1F80_3000 => physical,
        0x1FC0_0000..0x1FC8_0000 => 0xA000_0000 + physical,
        _ => t,
    })
}

pub(super) fn detect(data: &[u8]) -> Option<RomParts> {
    if data.len() < 0x800 || &data[..8] != b"PS-X EXE" {
        return None;
    }
    let word = |at: usize| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as u64;
    let (pc, gp, text, size) = (word(0x10), word(0x14), resolve(word(0x18))?, word(0x1C));
    let size = size.min(data.len() as u64 - 0x800);
    let ram_end = 0x8020_0000;
    let mut areas = vec![Area::rom("Code and data (loaded as they are)", text, size, 0x800)];
    if text > 0x8000_0000 {
        areas.push(Area::ram(
            "RAM (kernel and below the executable)",
            0x8000_0000,
            text - 0x8000_0000,
        ));
    }
    if text + size < ram_end {
        areas.push(Area::ram("RAM", text + size, ram_end - (text + size)));
    }
    areas.extend([
        Area::ram("Scratchpad", 0x1F80_0000, 0x400),
        Area::io("I/O registers", 0x1F80_1000, 0x2000),
        Area {
            name: "BIOS".into(),
            address: Some(0xBFC0_0000),
            size: 0x8_0000,
            file_offset: None,
            kind: RegionKind::Code,
            perms: "r-x",
        },
        Area::io("Cache control", 0xFFFE_0130, 4),
    ]);
    let mut labels: Vec<(String, u64, u64)> = REGISTERS.iter().map(|&(n, a)| (n.to_string(), a, 4)).collect();
    for (i, ch) in DMA.iter().enumerate() {
        let base = 0x1F80_1080 + 0x10 * i as u64;
        for (j, what) in ["MADR", "BCR", "CHCR"].iter().enumerate() {
            labels.push((format!("D{i}_{what}_{ch}"), base + 4 * j as u64, 4));
        }
    }
    for n in 0..3u64 {
        for (j, what) in ["COUNT", "MODE", "TARGET"].iter().enumerate() {
            labels.push((format!("TIMER{n}_{what}"), 0x1F80_1100 + 0x10 * n + 4 * j as u64, 4));
        }
    }
    // The kernel's entry points for BIOS calls (the function number in $t1).
    for (name, at) in [
        ("bios_a", 0x8000_00A0),
        ("bios_b", 0x8000_00B0),
        ("bios_c", 0x8000_00C0),
    ] {
        labels.push((name.into(), at, 0x10));
    }
    let region = header_text(&data[0x4C..0x800.min(data.len())]);
    let properties = vec![
        prop("Entry", format!("{pc:#010x}")),
        prop("$gp", format!("{gp:#010x}")),
        prop("Loaded at", format!("{text:#010x} ({size} bytes)")),
        prop("Region", if region.is_empty() { "?".into() } else { region }),
    ];
    Some(RomParts {
        platform: Platform::PlayStation,
        cpu: Cpu::MipsR3000,
        format_name: "PS-X EXE".into(),
        title: None,
        endian: Endian::Little,
        bits: 32,
        areas,
        map: Map::Flat(super::Flat::Psx),
        vectors: vec![("entry", resolve(pc)?)],
        labels,
        properties,
        state: State::with_gp(gp as u32),
        layout,
        data: None,
    })
}

const HEADER: [F; 13] = [
    F::str("Magic", 8),
    F::bytes("Zero", 8),
    F::u32("Initial PC", Fmt::Hex),
    F::u32("Initial $gp", Fmt::Hex),
    F::u32("Load address", Fmt::Hex),
    F::u32("Size", Fmt::Size),
    F::u32("Data address", Fmt::Hex),
    F::u32("Data size", Fmt::Size),
    F::u32("BSS address", Fmt::Hex),
    F::u32("BSS size", Fmt::Size),
    F::u32("Stack base", Fmt::Hex),
    F::u32("Stack size", Fmt::Size),
    F::bytes("Saved registers (the BIOS's)", 20),
];

fn layout(b: &mut Builder<'_>) {
    let len = b.ctx.bytes.data.len() as u64;
    if let Some(h) = b.region(0, 0x800, RegionKind::Header, "PS-X EXE header") {
        b.fields(h, 0, &HEADER);
        b.child(h, 0x4C, 0x800 - 0x4C, RegionKind::Notes, "Region marker");
    }
    b.region(0x800, len - 0x800, RegionKind::Code, "Code and data");
}
