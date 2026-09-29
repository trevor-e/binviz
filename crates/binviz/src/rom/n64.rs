//! The Nintendo 64: a 64-byte header (where the game is loaded, its name
//! and ID, two checksums), the IPL3 boot code the console runs from the
//! RSP's memory, then the game. The boot code copies the first megabyte
//! after it to RDRAM at the header's boot address; the rest stays on the
//! cartridge, read by DMA (from `0xB0000000`). Files come in three byte
//! orders — `.z64` (big-endian, as the console reads them), `.v64` (bytes
//! swapped in pairs), `.n64` (words little-endian) — and are read as `.z64`.

use super::{Area, Map, Platform, RomParts, header_text, prop};
use crate::cpu::{Cpu, State};
use crate::layout::Builder;
use crate::layout::fields::{F, Fmt};
use crate::model::RegionKind;
use crate::util::Endian;

/// The RCP's registers, as the N64 documentation names them.
#[rustfmt::skip]
const REGISTERS: [(&str, u64); 60] = [
    ("SP_MEM_ADDR", 0xA404_0000), ("SP_DRAM_ADDR", 0xA404_0004), ("SP_RD_LEN", 0xA404_0008), ("SP_WR_LEN", 0xA404_000C),
    ("SP_STATUS", 0xA404_0010), ("SP_DMA_FULL", 0xA404_0014), ("SP_DMA_BUSY", 0xA404_0018), ("SP_SEMAPHORE", 0xA404_001C),
    ("SP_PC", 0xA408_0000),
    ("DPC_START", 0xA410_0000), ("DPC_END", 0xA410_0004), ("DPC_CURRENT", 0xA410_0008), ("DPC_STATUS", 0xA410_000C),
    ("DPC_CLOCK", 0xA410_0010), ("DPC_BUFBUSY", 0xA410_0014), ("DPC_PIPEBUSY", 0xA410_0018), ("DPC_TMEM", 0xA410_001C),
    ("MI_MODE", 0xA430_0000), ("MI_VERSION", 0xA430_0004), ("MI_INTR", 0xA430_0008), ("MI_INTR_MASK", 0xA430_000C),
    ("VI_STATUS", 0xA440_0000), ("VI_ORIGIN", 0xA440_0004), ("VI_WIDTH", 0xA440_0008), ("VI_INTR", 0xA440_000C),
    ("VI_CURRENT", 0xA440_0010), ("VI_BURST", 0xA440_0014), ("VI_V_SYNC", 0xA440_0018), ("VI_H_SYNC", 0xA440_001C),
    ("VI_LEAP", 0xA440_0020), ("VI_H_START", 0xA440_0024), ("VI_V_START", 0xA440_0028), ("VI_V_BURST", 0xA440_002C),
    ("VI_X_SCALE", 0xA440_0030), ("VI_Y_SCALE", 0xA440_0034),
    ("AI_DRAM_ADDR", 0xA450_0000), ("AI_LEN", 0xA450_0004), ("AI_CONTROL", 0xA450_0008), ("AI_STATUS", 0xA450_000C),
    ("AI_DACRATE", 0xA450_0010), ("AI_BITRATE", 0xA450_0014),
    ("PI_DRAM_ADDR", 0xA460_0000), ("PI_CART_ADDR", 0xA460_0004), ("PI_RD_LEN", 0xA460_0008), ("PI_WR_LEN", 0xA460_000C),
    ("PI_STATUS", 0xA460_0010), ("PI_BSD_DOM1_LAT", 0xA460_0014), ("PI_BSD_DOM1_PWD", 0xA460_0018),
    ("PI_BSD_DOM1_PGS", 0xA460_001C), ("PI_BSD_DOM1_RLS", 0xA460_0020),
    ("RI_MODE", 0xA470_0000), ("RI_CONFIG", 0xA470_0004), ("RI_CURRENT_LOAD", 0xA470_0008), ("RI_SELECT", 0xA470_000C),
    ("RI_REFRESH", 0xA470_0010),
    ("SI_DRAM_ADDR", 0xA480_0000), ("SI_PIF_ADDR_RD64B", 0xA480_0004), ("SI_PIF_ADDR_WR64B", 0xA480_0010),
    ("SI_STATUS", 0xA480_0018), ("PIF_RAM", 0xBFC0_07C0),
];

/// Folds an address onto the one we give that memory: RDRAM in KSEG0
/// (`0x80000000`), registers and the cartridge in KSEG1 (`0xA4000000`,
/// `0xB0000000`), whether named cached, uncached or physical.
pub(super) fn resolve(t: u64) -> Option<u64> {
    let t = t & 0xFFFF_FFFF;
    let physical = match t {
        0x8000_0000..0xA000_0000 => t - 0x8000_0000,
        0xA000_0000..0xC000_0000 => t - 0xA000_0000,
        0..0x2000_0000 => t,
        _ => return Some(t),
    };
    Some(match physical {
        0..0x80_0000 => 0x8000_0000 + physical,
        _ => 0xA000_0000 + physical,
    })
}

/// The file in the console's byte order (`.z64`), and which order it came in.
fn normalized(data: &[u8]) -> Option<(Vec<u8>, &'static str)> {
    let head = data.get(..4)?;
    let order = match head {
        [0x80, 0x37, 0x12, 0x40] => return Some((data.to_vec(), ".z64 (big-endian)")),
        [0x37, 0x80, 0x40, 0x12] => ".v64 (bytes swapped in pairs); shown as .z64",
        [0x40, 0x12, 0x37, 0x80] => ".n64 (little-endian words); shown as .z64",
        _ => return None,
    };
    let mut out = data.to_vec();
    if head[0] == 0x37 {
        for pair in out.as_chunks_mut::<2>().0 {
            pair.swap(0, 1);
        }
    } else {
        for word in out.as_chunks_mut::<4>().0 {
            word.reverse();
        }
    }
    Some((out, order))
}

fn destination(c: u8) -> &'static str {
    match c {
        b'A' => "all regions",
        b'D' => "Germany",
        b'E' => "North America",
        b'F' => "France",
        b'I' => "Italy",
        b'J' => "Japan",
        b'P' | b'X' | b'Y' => "Europe",
        b'S' => "Spain",
        b'U' => "Australia",
        _ => "?",
    }
}

pub(super) fn detect(data: &[u8]) -> Option<RomParts> {
    if data.len() < 0x1000 {
        return None;
    }
    let (rom, order) = normalized(data)?;
    let word = |at: usize| u32::from_be_bytes([rom[at], rom[at + 1], rom[at + 2], rom[at + 3]]) as u64;
    let boot = resolve(word(8))?;
    let code = (rom.len() as u64 - 0x1000).min(0x10_0000);
    let mut areas = vec![
        Area::rom("IPL3 boot code (runs from SP DMEM)", 0xA400_0040, 0xFC0, 0x40),
        Area::rom(
            "Boot segment (the first MiB after the boot code, copied to RDRAM)",
            boot,
            code,
            0x1000,
        ),
        Area::rom("Cartridge (PI domain 1)", 0xB000_0000, rom.len() as u64, 0),
    ];
    // RDRAM around what the boot code copies.
    if boot > 0x8000_0000 {
        areas.push(Area::ram("RDRAM", 0x8000_0000, boot - 0x8000_0000));
    }
    if boot + code < 0x8080_0000 {
        areas.push(Area::ram("RDRAM", boot + code, 0x8080_0000 - (boot + code)));
    }
    areas.extend([
        Area::io("SP registers", 0xA404_0000, 0x20),
        Area::io("SP PC", 0xA408_0000, 8),
        Area::io("DP registers", 0xA410_0000, 0x20),
        Area::io("MI registers", 0xA430_0000, 0x10),
        Area::io("VI registers", 0xA440_0000, 0x38),
        Area::io("AI registers", 0xA450_0000, 0x18),
        Area::io("PI registers", 0xA460_0000, 0x34),
        Area::io("RI registers", 0xA470_0000, 0x20),
        Area::io("SI registers", 0xA480_0000, 0x1C),
        Area::ram("PIF RAM", 0xBFC0_07C0, 0x40),
    ]);
    let title = header_text(&rom[0x20..0x34]);
    let id: String = rom[0x3B..0x3F]
        .iter()
        .map(|&b| if b.is_ascii_graphic() { b as char } else { '?' })
        .collect();
    let properties = vec![
        prop("Game ID", id),
        prop("Destination", destination(rom[0x3E])),
        prop("Version", rom[0x3F].to_string()),
        prop("Boot address", format!("{:#010x}", word(8))),
        prop(
            "CRC",
            format!(
                "{:08X} {:08X} (not checked: it depends on the boot chip)",
                word(0x10),
                word(0x14)
            ),
        ),
        prop("Byte order", order),
    ];
    let labels: Vec<(String, u64, u64)> = REGISTERS.iter().map(|&(n, a)| (n.to_string(), a, 4)).collect();
    Some(RomParts {
        platform: Platform::Nintendo64,
        cpu: Cpu::MipsR4300,
        format_name: order.split(' ').next().unwrap_or(".z64").into(),
        title: (!title.is_empty()).then_some(title),
        endian: Endian::Big,
        bits: 32,
        areas,
        map: Map::Flat(super::Flat::N64),
        vectors: vec![("entry", boot), ("ipl3", 0xA400_0040)],
        labels,
        properties,
        state: State::default(),
        layout,
        data: (order != ".z64 (big-endian)").then_some(rom),
        entries: Vec::new(),
        late_entries: Vec::new(),
    })
}

const HEADER: [F; 13] = [
    F::u32("PI BSD domain 1 configuration", Fmt::Hex),
    F::u32("Clock rate", Fmt::Hex),
    F::u32("Boot address", Fmt::Hex),
    F::u32("libultra version", Fmt::Hex),
    F::u32("CRC 1", Fmt::Hex),
    F::u32("CRC 2", Fmt::Hex),
    F::bytes("Reserved", 8),
    F::str("Title", 20),
    F::bytes("Reserved", 7),
    F::str("Media format", 1),
    F::str("Cartridge ID", 2),
    F::str("Destination", 1),
    F::u8("Version", Fmt::Dec),
];

fn layout(b: &mut Builder<'_>) {
    let len = b.ctx.bytes.data.len() as u64;
    b.struct_region(0, RegionKind::Header, "Header", &HEADER);
    b.region(0x40, 0xFC0, RegionKind::Code, "IPL3 boot code");
    let code = (len - 0x1000).min(0x10_0000);
    b.region(0x1000, code, RegionKind::Code, "Boot segment (copied to RDRAM at boot)");
    if len > 0x1000 + code {
        b.region(
            0x1000 + code,
            len - 0x1000 - code,
            RegionKind::Data,
            "The rest of the cartridge (read by DMA)",
        );
    }
}
