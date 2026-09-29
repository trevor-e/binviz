//! PlayStation executables (PS-X EXE, like a disc's `SLUS_000.01`): a
//! 2 KiB header (where to load the code, where to start, `$gp`, the stack)
//! then the code and data, loaded into RAM as they are.

use serde::Serialize;

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
        entries: Vec::new(),
        late_entries: Vec::new(),
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

// ---- Memory images and overlays -------------------------------------------

/// The first 64 KiB of RAM hold the kernel the BIOS copies there: the
/// exception vector at `0x80` and the BIOS-call entry points at `0xA0`,
/// `0xB0` and `0xC0` each start with `lui` (into `$k0` or `$t0`) then jump.
fn kernel_present(data: &[u8]) -> bool {
    let word = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    word(0x80).is_some_and(|w| w & 0xFFFF_0000 == 0x3C1A_0000)
        && [0xA0, 0xB0, 0xC0]
            .iter()
            .all(|&at| word(at).is_some_and(|w| w & 0xFFFF_0000 == 0x3C08_0000))
}

/// RAM as an emulator dumped it (2 MiB, or 8 MiB on a development unit),
/// from `0x80000000`, the kernel in its first 64 KiB: whatever the game had
/// loaded, overlays included, analysable in place.
pub(super) fn detect_memory(data: &[u8]) -> Option<RomParts> {
    memory_parts(data, Vec::new())
}

/// The areas every PlayStation image shares besides its own bytes.
fn hardware_areas() -> Vec<Area> {
    vec![
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
    ]
}

fn hardware_labels() -> Vec<(String, u64, u64)> {
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
    labels
}

/// Where functions start, by their prologues: `addiu $sp, $sp, -N` with
/// `$ra` saved to the frame within the next few instructions (leaf functions
/// without a frame are found by the calls to them instead).
pub(crate) fn prologues(data: &[u8], base: u64) -> Vec<u64> {
    let word = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let mut out = Vec::new();
    let mut at = 0;
    while at + 4 <= data.len() {
        if let Some(w) = word(at)
            && w & 0xFFFF_0000 == 0x27BD_0000
            && ((w & 0xFFFF) as u16 as i16) < 0
            && w & 3 == 0
            && (1..=8).any(|k| word(at + 4 * k).is_some_and(|s| s & 0xFFFF_0000 == 0xAFBF_0000))
        {
            out.push(base + at as u64);
        }
        at += 4;
    }
    out
}

/// A memory image, with `extra` named entry points (functions another image
/// of the game names).
pub(crate) fn memory_parts(data: &[u8], extra: Vec<(String, u64, u64)>) -> Option<RomParts> {
    if !(data.len() == 0x20_0000 || data.len() == 0x80_0000) || !kernel_present(data) {
        return None;
    }
    let size = data.len() as u64;
    let mut areas = vec![
        Area::rom("Kernel (copied by the BIOS)", 0x8000_0000, 0x1_0000, 0),
        Area::rom("RAM (memory image)", 0x8001_0000, size - 0x1_0000, 0x1_0000),
    ];
    areas.extend(hardware_areas());
    let mut entries: Vec<(String, u64, u64)> = prologues(&data[0x1_0000..], 0x8001_0000)
        .into_iter()
        .map(|a| (String::new(), a, 0))
        .collect();
    let scanned = entries.len();
    entries.extend(extra);
    let properties = vec![
        prop(
            "Image",
            format!(
                "{} of RAM from 0x80000000",
                if size == 0x20_0000 { "2 MiB" } else { "8 MiB" }
            ),
        ),
        prop("Function prologues", format!("{scanned} found by scanning")),
    ];
    Some(RomParts {
        platform: Platform::PlayStation,
        cpu: Cpu::MipsR3000,
        format_name: "PlayStation memory image".into(),
        title: None,
        endian: Endian::Little,
        bits: 32,
        areas,
        map: Map::Flat(super::Flat::Psx),
        vectors: vec![
            ("exception", 0x8000_0080),
            ("bios_a", 0x8000_00A0),
            ("bios_b", 0x8000_00B0),
            ("bios_c", 0x8000_00C0),
        ],
        labels: hardware_labels(),
        properties,
        state: State::default(),
        layout: memory_layout,
        data: None,
        entries,
        late_entries: Vec::new(),
    })
}

/// A code overlay (a chunk of a game's code loaded from the disc at run
/// time) at the address it is loaded to, with `extra` named entry points
/// (the boot executable's functions, which the overlay calls).
pub(crate) fn overlay_parts(data: &[u8], load: u64, extra: Vec<(String, u64, u64)>) -> Option<RomParts> {
    let load = resolve(load)?;
    let size = data.len() as u64;
    if !(0x8000_0000..0x8080_0000).contains(&load) || size == 0 || load + size > 0x8080_0000 {
        return None;
    }
    let ram_end = if load + size > 0x8020_0000 { 0x8080_0000 } else { 0x8020_0000 };
    let mut areas = vec![Area::rom("Overlay", load, size, 0)];
    if load > 0x8000_0000 {
        areas.push(Area::ram("RAM (below the overlay)", 0x8000_0000, load - 0x8000_0000));
    }
    if load + size < ram_end {
        areas.push(Area::ram("RAM (above the overlay)", load + size, ram_end - (load + size)));
    }
    areas.extend(hardware_areas());
    let mut entries: Vec<(String, u64, u64)> = prologues(data, load).into_iter().map(|a| (String::new(), a, 0)).collect();
    let scanned = entries.len();
    entries.extend(extra);
    let properties = vec![
        prop("Loaded at", format!("{load:#010x} ({size} bytes)")),
        prop("Function prologues", format!("{scanned} found by scanning")),
    ];
    Some(RomParts {
        platform: Platform::PlayStation,
        cpu: Cpu::MipsR3000,
        format_name: "PlayStation overlay".into(),
        title: None,
        endian: Endian::Little,
        bits: 32,
        areas,
        map: Map::Flat(super::Flat::Psx),
        vectors: Vec::new(),
        labels: hardware_labels(),
        properties,
        state: State::default(),
        layout: overlay_layout,
        data: None,
        entries,
        late_entries: Vec::new(),
    })
}

fn memory_layout(b: &mut Builder<'_>) {
    let len = b.ctx.bytes.data.len() as u64;
    b.region(0, 0x1_0000, RegionKind::Code, "Kernel");
    b.region(0x1_0000, len - 0x1_0000, RegionKind::Code, "RAM image");
}

fn overlay_layout(b: &mut Builder<'_>) {
    let len = b.ctx.bytes.data.len() as u64;
    b.region(0, len, RegionKind::Code, "Overlay");
}

/// Where `blob` (an overlay file from the disc) sits in `ram` (a memory
/// image): its address, found by a distinctive 32 bytes near its start and
/// checked over up to its first 64 KiB, one byte in ten allowed to differ
/// (the game writes to its own data).
pub fn locate(blob: &[u8], ram: &[u8]) -> Option<u64> {
    let distinct = |b: &[u8]| {
        let mut seen = [false; 256];
        b.iter()
            .filter(|&&x| !std::mem::replace(&mut seen[x as usize], true))
            .count()
    };
    let window = (0..blob.len().checked_sub(32)?.min(4096))
        .step_by(4)
        .find(|&o| distinct(&blob[o..o + 32]) >= 12)?;
    let needle = &blob[window..window + 32];
    for pos in memchr::memmem::find_iter(ram, needle) {
        if pos < window || (pos - window) % 4 != 0 {
            continue;
        }
        let start = pos - window;
        let n = blob.len().min(ram.len() - start).min(64 * 1024);
        let same = blob[..n].iter().zip(&ram[start..start + n]).filter(|(a, b)| a == b).count();
        if same * 10 >= n * 9 {
            return Some(0x8000_0000 + start as u64);
        }
    }
    None
}

/// The functions an image knows, for another image of the same game to
/// share: (name, address, size), so that calls into it read as in it.
fn shared_functions(exe: &crate::binary::Binary) -> Vec<(String, u64, u64)> {
    exe.symbols()
        .functions()
        .filter(|f| f.address >= 0x8000_0000 && f.address < 0x8080_0000)
        .map(|f| (f.display_name().into_owned(), f.address, f.size))
        .collect()
}

impl crate::binary::Binary {
    /// A PlayStation memory image (2 MiB of RAM as an emulator dumped it),
    /// with the functions of `exe` (the game's boot executable, or another
    /// image with names) named in it.
    pub fn parse_psx_memory(
        data: impl Into<std::sync::Arc<[u8]>>,
        exe: Option<&crate::binary::Binary>,
    ) -> crate::error::Result<crate::binary::Binary> {
        let data: std::sync::Arc<[u8]> = data.into();
        let extra = exe.map(shared_functions).unwrap_or_default();
        let parts = memory_parts(&data, extra).ok_or_else(|| {
            crate::error::Error::new(
                "not a PlayStation memory image: expected 2 MiB (or 8 MiB) of RAM from 0x80000000 with the kernel in its first 64 KiB",
            )
        })?;
        crate::binary::Binary::from_rom(data, parts)
    }

    /// A PlayStation overlay (code the game loads from the disc at run time)
    /// at `load_address`, with the functions of `exe` named for the calls
    /// into it.
    pub fn parse_psx_overlay(
        data: impl Into<std::sync::Arc<[u8]>>,
        load_address: u64,
        exe: Option<&crate::binary::Binary>,
    ) -> crate::error::Result<crate::binary::Binary> {
        let data: std::sync::Arc<[u8]> = data.into();
        let extra = exe.map(shared_functions).unwrap_or_default();
        let parts = overlay_parts(&data, load_address, extra).ok_or_else(|| {
            crate::error::Error::new(format!(
                "an overlay must load inside RAM (0x80000000 to 0x80200000; got {load_address:#x} for {} bytes)",
                data.len()
            ))
        })?;
        crate::binary::Binary::from_rom(data, parts)
    }

    /// Whether this is a PlayStation memory image.
    pub fn is_psx_memory(&self) -> bool {
        self.summary.format_name == "PlayStation memory image" && kernel_present(&self.data)
    }

    /// Where `blob` (a file from the disc) is loaded in this memory image, if it is.
    pub fn psx_locate(&self, blob: &[u8]) -> Option<u64> {
        self.is_psx_memory().then(|| locate(blob, &self.data)).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::Binary;

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// A function with a frame that calls `target`, then returns; 9 words.
    fn caller(target: u32) -> Vec<u32> {
        vec![
            0x27BD_FFE8,                                // addiu $sp, $sp, -0x18
            0xAFBF_0014,                                // sw $ra, 0x14($sp)
            0x0C00_0000 | (target >> 2) & 0x03FF_FFFF,  // jal target
            0x0000_0000,                                // nop
            0x8FBF_0014,                                // lw $ra, 0x14($sp)
            0x0000_0000,                                // nop
            0x03E0_0008,                                // jr $ra
            0x27BD_0018,                                // addiu $sp, $sp, 0x18
            0x0000_0000,
        ]
    }

    fn exe_with(words: &[u32]) -> Binary {
        let mut data = vec![0u8; 0x800];
        data[..8].copy_from_slice(b"PS-X EXE");
        for (at, v) in [(0x10, 0x8001_0000u32), (0x14, 0x8001_8000), (0x18, 0x8001_0000)] {
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        let code = le(words);
        data[0x1C..0x20].copy_from_slice(&(code.len() as u32).to_le_bytes());
        data.extend(code);
        Binary::parse(data).unwrap()
    }

    #[test]
    fn memory_image_and_overlay() {
        // The exe: entry at 0x80010000 calls a leaf at 0x80010024.
        let mut words = caller(0x8001_0024);
        words.extend([0x03E0_0008, 0x2402_0001]); // jr $ra / li $v0, 1
        let exe = exe_with(&words);
        // The image: kernel signature, the exe's code where it loads, and an overlay at
        // 0x80100000 whose function calls the exe's leaf and reads a global.
        let mut ram = vec![0u8; 0x20_0000];
        for at in [0x80, 0xA0, 0xB0, 0xC0] {
            let w: u32 = if at == 0x80 { 0x3C1A_0000 } else { 0x3C08_0000 };
            ram[at..at + 4].copy_from_slice(&w.to_le_bytes());
            ram[at + 8..at + 12].copy_from_slice(&0x0340_0008u32.to_le_bytes()); // jr $k0
        }
        ram[0x1_0000..0x1_0000 + 4 * words.len()].copy_from_slice(&le(&words));
        let mut overlay = caller(0x8001_0024);
        overlay.extend([0x3C02_8012, 0x8C42_0010]); // lui $v0, 0x8012 / lw $v0, 0x10($v0)
        overlay.extend([0xDEAD_BEEF, 0x1234_5678, 0x0BAD_F00D, 0xFEED_FACE]);
        let blob = le(&overlay);
        ram[0x10_0000..0x10_0000 + blob.len()].copy_from_slice(&blob);
        assert!(prologues(&blob, 0x8010_0000) == [0x8010_0000]);
        assert_eq!(locate(&blob, &ram), Some(0x8010_0000));

        // Plain open recognizes the image; with the exe its functions are named.
        let plain = Binary::parse(ram.clone()).unwrap();
        assert_eq!(plain.summary().format_name, "PlayStation memory image");
        assert!(plain.is_psx_memory());
        assert!(plain.symbols().function_containing(0x8010_0000).is_some());
        let with = Binary::parse_psx_memory(ram.clone(), Some(&exe)).unwrap();
        let f = with.symbols().function_containing(0x8001_0000).unwrap();
        assert_eq!(f.name(), "entry");
        assert_eq!(with.psx_locate(&blob), Some(0x8010_0000));
        assert!(!exe.is_psx_memory());

        // The overlay on its own, at its address: its call into the exe resolves by name.
        let ov = Binary::parse_psx_overlay(blob.clone(), 0x8010_0000, Some(&exe)).unwrap();
        assert_eq!(ov.summary().format_name, "PlayStation overlay");
        let f = ov.symbols().function_containing(0x8010_0000).unwrap();
        assert_eq!((f.address, f.size), (0x8010_0000, 8 * 4));
        let dis = ov.disassemble_function(0x8010_0000, 32);
        let jal = dis.instructions.iter().find(|i| i.mnemonic == "jal").unwrap();
        assert_eq!(jal.target_symbol.as_deref(), Some("sub_80010024"));
        assert!(Binary::parse_psx_overlay(blob, 0x1F00_0000, None).is_err());
    }
}

// ---- Emulator traces ------------------------------------------------------

/// What a trace of the code an emulator ran gave.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceSummary {
    /// Lines with an address on them.
    pub lines: u64,
    /// Distinct addresses seen run.
    pub addresses: u64,
    /// Addresses in this image.
    pub placed: u64,
    /// Runs of code the trace saw that following the code hadn't reached.
    pub new_runs: u64,
}

/// The addresses a trace saw run: the first word of 8 hex digits on each
/// line (PCSX-Redux's and DuckStation's CPU traces, a Lua script's list of
/// PCs, `80010000: 27bdffe8 addiu sp, sp, -0x18`…), each folded onto the
/// address we give that memory; sorted and unique.
pub fn parse_trace(text: &str) -> (u64, Vec<u64>) {
    let mut lines = 0;
    let mut out = Vec::new();
    for line in text.lines() {
        let hex = line
            .split(|c: char| !c.is_ascii_hexdigit() && c != 'x')
            .map(|t| t.trim_start_matches("0x").trim_start_matches("0X"))
            .find(|t| t.len() == 8 && t.chars().all(|c| c.is_ascii_hexdigit()));
        if let Some(h) = hex
            && let Ok(a) = u64::from_str_radix(h, 16)
            && let Some(a) = resolve(a)
        {
            lines += 1;
            out.push(a & !3);
        }
    }
    out.sort_unstable();
    out.dedup();
    (lines, out)
}

impl crate::binary::Binary {
    /// This PlayStation image read again with a trace of the code an
    /// emulator saw run: code reached only through pointers and tables is
    /// followed too (each run of traced code the following hadn't reached
    /// becomes a function, after everything reachable has been followed).
    pub fn with_psx_trace(&self, text: &str) -> crate::error::Result<(crate::binary::Binary, TraceSummary)> {
        let rom = self
            .rom
            .as_ref()
            .filter(|r| r.platform == Platform::PlayStation)
            .ok_or_else(|| crate::error::Error::new("traces are for PlayStation images"))?;
        let (lines, addresses) = parse_trace(text);
        let in_image = |a: u64| {
            self.sections
                .iter()
                .any(|s| s.loaded && s.file_offset.is_some() && a >= s.address && a < s.address + s.size)
        };
        let known = |a: u64| self.symbols.static_function_containing(a).is_some();
        let placed: Vec<u64> = addresses.iter().copied().filter(|&a| in_image(a)).collect();
        // Every traced address not already in a function, in order: following one
        // owns what it reaches, and the next one still unowned starts the next function.
        let seeds: Vec<u64> = placed.iter().copied().filter(|&a| !known(a)).collect();
        let mut runs = 0u64;
        let mut prev = None;
        for &a in &seeds {
            if prev != Some(a - 4) {
                runs += 1;
            }
            prev = Some(a);
        }
        let data = self.data.clone();
        let entries = rom.entries.clone();
        let mut parts = match self.summary.format_name.as_str() {
            "PlayStation memory image" => memory_parts(&data, entries),
            "PlayStation overlay" => {
                let load = self
                    .sections
                    .iter()
                    .find(|s| s.name == "Overlay")
                    .map(|s| s.address)
                    .unwrap_or(0);
                overlay_parts(&data, load, entries)
            }
            _ => detect(&data),
        }
        .ok_or_else(|| crate::error::Error::new("the image no longer reads"))?;
        parts.late_entries = seeds;
        let bin = crate::binary::Binary::from_rom(data, parts)?;
        Ok((
            bin,
            TraceSummary {
                lines,
                addresses: addresses.len() as u64,
                placed: placed.len() as u64,
                new_runs: runs,
            },
        ))
    }
}

#[cfg(test)]
mod trace_tests {
    use super::*;
    use crate::binary::Binary;

    #[test]
    fn trace_finds_code_reached_through_pointers() {
        // entry: jr $ra / nop; then, unreachable by following, a function at +0x10.
        let mut data = vec![0u8; 0x800];
        data[..8].copy_from_slice(b"PS-X EXE");
        for (at, v) in [(0x10, 0x8001_0000u32), (0x14, 0x8001_8000), (0x18, 0x8001_0000), (0x1C, 0x20)] {
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        let words = [0x03E0_0008u32, 0, 0, 0, 0x2402_0001, 0x03E0_0008, 0, 0];
        data.extend(words.iter().flat_map(|w| w.to_le_bytes()));
        let bin = Binary::parse(data).unwrap();
        assert!(bin.symbols().function_containing(0x8001_0010).is_none());
        let (traced, s) = bin
            .with_psx_trace("80010000: 03e00008 jr ra\n80010004: 00000000 nop\n0x80010010\n80010014 jr $ra\nnot a line\n")
            .unwrap();
        assert_eq!((s.lines, s.addresses, s.placed, s.new_runs), (4, 4, 4, 1));
        let f = traced.symbols().function_containing(0x8001_0010).unwrap();
        assert_eq!((f.address, f.size), (0x8001_0010, 12));
        assert_eq!(parse_trace("00010010\n").1, [0x8001_0010]);
    }
}
