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
        // A game reaches most of its code through tables of function pointers and
        // callbacks, so following calls from the entry finds a fraction of it; the
        // prologues fill in the rest once the calls have been followed.
        late_entries: prologues(&data[0x800..0x800 + size as usize], text),
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
    let word = |at: usize| {
        data.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
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
    let word = |at: usize| {
        data.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let mut out = Vec::new();
    let mut at = 0;
    while at + 4 <= data.len() {
        if let Some(w) = word(at)
            && w & 0xFFFF_0000 == 0x27BD_0000
            && ((w & 0xFFFF) as u16 as i16) < 0
            && w & 3 == 0
            && (1..=24).any(|k| word(at + 4 * k).is_some_and(|s| s & 0xFFFF_0000 == 0xAFBF_0000))
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
    // Named functions (another image's) are followed first; the prologues
    // after the calls, so that a call to a function's true start (the
    // instructions the compiler hoisted above its frame setup) wins over the
    // prologue found inside it.
    let late_entries = prologues(&data[0x1_0000..], 0x8001_0000);
    let scanned = late_entries.len();
    let entries = extra;
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
        late_entries,
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
    let ram_end = if load + size > 0x8020_0000 {
        0x8080_0000
    } else {
        0x8020_0000
    };
    let mut areas = vec![Area::rom("Overlay", load, size, 0)];
    if load > 0x8000_0000 {
        areas.push(Area::ram("RAM (below the overlay)", 0x8000_0000, load - 0x8000_0000));
    }
    if load + size < ram_end {
        areas.push(Area::ram(
            "RAM (above the overlay)",
            load + size,
            ram_end - (load + size),
        ));
    }
    areas.extend(hardware_areas());
    let late_entries = prologues(data, load);
    let scanned = late_entries.len();
    let entries = extra;
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
        late_entries,
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
        let same = blob[..n]
            .iter()
            .zip(&ram[start..start + n])
            .filter(|(a, b)| a == b)
            .count();
        if same * 10 >= n * 9 {
            return Some(0x8000_0000 + start as u64);
        }
    }
    None
}

/// A stretch of a file found in a memory image: where it is in the file,
/// where it is loaded, and how much of it is still the same (data the
/// program changed after loading it differs).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedPiece {
    /// Offset in the file.
    pub offset: u64,
    pub size: u64,
    pub address: u64,
    /// The share of its bytes still the same, 0 to 1.
    pub same: f32,
}

/// The stretches of `file` (a disc's archive, whatever its format, when it
/// stores what it holds uncompressed) that are loaded in `ram` (a
/// PlayStation's 2 MiB): the overlays and data its loader copied in, found
/// by their bytes, each at least 1 KiB and loaded 4-byte aligned.
pub fn loaded_pieces(file: &[u8], ram: &[u8]) -> Vec<LoadedPiece> {
    const WINDOW: usize = 32;
    const STEP: usize = 64;
    fn hash(w: &[u8]) -> u64 {
        w.chunks(8).fold(0xcbf2_9ce4_8422_2325u64, |h, c| {
            let mut word = [0u8; 8];
            word[..c.len()].copy_from_slice(c);
            (h ^ u64::from_le_bytes(word)).wrapping_mul(0x0100_0000_01b3)
        })
    }
    let telling = |w: &[u8]| {
        let mut seen = [false; 256];
        w.iter()
            .filter(|&&x| !std::mem::replace(&mut seen[x as usize], true))
            .count()
            >= 12
    };
    // Every 4-byte aligned window of memory with bytes enough to tell it apart.
    let mut index: std::collections::HashMap<u64, Vec<u32>> = std::collections::HashMap::new();
    for o in (0..ram.len().saturating_sub(WINDOW)).step_by(4) {
        let w = &ram[o..o + WINDOW];
        if telling(w) {
            index.entry(hash(w)).or_default().push(o as u32);
        }
    }
    // The file's windows found there, by how far memory is from the file (one per loaded stretch).
    let mut hits: std::collections::BTreeMap<i64, Vec<usize>> = std::collections::BTreeMap::new();
    for a in (0..file.len().saturating_sub(WINDOW)).step_by(STEP) {
        let w = &file[a..a + WINDOW];
        let Some(at) = index.get(&hash(w)) else { continue };
        // A pattern repeated all over tells nothing.
        if at.len() > 8 {
            continue;
        }
        for &o in at {
            if &ram[o as usize..o as usize + WINDOW] == w {
                hits.entry(o as i64 - a as i64).or_default().push(a);
            }
        }
    }
    let mut out = Vec::new();
    for (delta, offsets) in hits {
        // Runs of hits close together; data changed since loading leaves gaps.
        let mut runs: Vec<(usize, usize, u32)> = Vec::new();
        for a in offsets {
            match runs.last_mut() {
                Some(r) if a <= r.1 + 4096 => {
                    r.1 = a + WINDOW;
                    r.2 += 1;
                }
                _ => runs.push((a, a + WINDOW, 1)),
            }
        }
        for (mut start, mut end, n) in runs {
            if n < 4 {
                continue;
            }
            let ram_at = |a: usize| (a as i64 + delta) as usize;
            // Out to where the bytes stop being the same.
            while start >= 4 && ram_at(start) >= 4 && file[start - 4..start] == ram[ram_at(start) - 4..ram_at(start)] {
                start -= 4;
            }
            while end + 4 <= file.len()
                && ram_at(end) + 4 <= ram.len()
                && file[end..end + 4] == ram[ram_at(end)..ram_at(end) + 4]
            {
                end += 4;
            }
            if end - start < 1024 {
                continue;
            }
            let same = file[start..end]
                .iter()
                .zip(&ram[ram_at(start)..ram_at(end)])
                .filter(|(x, y)| x == y)
                .count();
            out.push(LoadedPiece {
                offset: start as u64,
                size: (end - start) as u64,
                address: 0x8000_0000 + ram_at(start) as u64,
                same: same as f32 / (end - start) as f32,
            });
        }
    }
    out.sort_by_key(|p| (p.offset, p.address));
    out
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

    /// The stretches of `file` (a disc's archive, of whatever format) loaded
    /// in this memory image: see [`loaded_pieces`].
    pub fn psx_loaded_pieces(&self, file: &[u8]) -> Vec<LoadedPiece> {
        if self.is_psx_memory() {
            loaded_pieces(file, &self.data)
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::Binary;

    #[test]
    fn an_archives_files_are_found_where_they_are_loaded() {
        // Bytes that don't repeat, as code and data don't.
        let mut seed = 0x2545_f491u32;
        let mut noise = |n: usize| -> Vec<u8> {
            (0..n)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    seed as u8
                })
                .collect()
        };
        // An archive of three files, the first two loaded; the program changed a word of the first.
        let (a, b, c) = (noise(4096), noise(8192), noise(2048));
        let mut archive = noise(100);
        let (at_a, at_b) = (archive.len() + 12, archive.len() + 12 + 4096 + 20);
        archive.extend(noise(12));
        archive.extend(&a);
        archive.extend(noise(20));
        archive.extend(&b);
        archive.extend(noise(36));
        archive.extend(&c);
        let mut ram = vec![0u8; 0x20_0000];
        ram[0x4_0000..0x4_1000].copy_from_slice(&a);
        ram[0x4_0800..0x4_0804].copy_from_slice(&[1, 2, 3, 4]);
        ram[0x10_0000..0x10_2000].copy_from_slice(&b);
        let found = loaded_pieces(&archive, &ram);
        let placed: Vec<(u64, u64, u64)> = found.iter().map(|p| (p.offset, p.size, p.address)).collect();
        assert_eq!(
            placed,
            [(at_a as u64, 4096, 0x8004_0000), (at_b as u64, 8192, 0x8010_0000)]
        );
        assert!(
            found[0].same < 1.0 && found[0].same > 0.99 && found[1].same == 1.0,
            "{found:?}"
        );
    }

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// A function with a frame that calls `target`, then returns; 9 words.
    fn caller(target: u32) -> Vec<u32> {
        vec![
            0x27BD_FFE8,                               // addiu $sp, $sp, -0x18
            0xAFBF_0014,                               // sw $ra, 0x14($sp)
            0x0C00_0000 | (target >> 2) & 0x03FF_FFFF, // jal target
            0x0000_0000,                               // nop
            0x8FBF_0014,                               // lw $ra, 0x14($sp)
            0x0000_0000,                               // nop
            0x03E0_0008,                               // jr $ra
            0x27BD_0018,                               // addiu $sp, $sp, 0x18
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
    fn an_executable_finds_functions_nothing_calls() {
        // The entry returns at once; a function with a frame follows that nothing calls
        // (a game's callbacks are reached through tables of pointers).
        let mut words = vec![0x03E0_0008, 0x0000_0000, 0x0000_0000, 0x0000_0000];
        words.extend(caller(0x8001_0000));
        let exe = exe_with(&words);
        let f = exe
            .symbols()
            .function_containing(0x8001_0010)
            .expect("the uncalled function is found");
        assert_eq!(f.name(), "sub_80010010");
    }

    #[test]
    fn fields_are_followed_through_a_global_pointer() {
        let words = [
            0x3C01_8002, // lui $at, 0x8002
            0x8C23_0010, // lw $v1, 0x10($at)     the pointer, from the global at 0x80020010
            0x8C64_0014, // lw $a0, 0x14($v1)
            0xAC60_0030, // sw $zero, 0x30($v1)
            0x0060_8025, // move $s0, $v1
            0x0C00_400B, // jal 0x8001002c
            0x2464_0040, // addiu $a0, $v1, 0x40  (delay slot: &p->0x40)
            0x8E05_0008, // lw $a1, 8($s0)        $s0 survives the call
            0x8C66_0014, // lw $a2, 0x14($v1)     $v1 doesn't
            0x03E0_0008, // jr $ra
            0,
            0x03E0_0008, // the callee
            0,
        ];
        let exe = exe_with(&words);
        exe.prepare_xrefs();
        let found: Vec<(i64, u8, bool, u64)> = exe
            .field_accesses(0x8002_0010)
            .iter()
            .map(|a| (a.offset, a.width, a.store, a.site))
            .collect();
        assert_eq!(
            found,
            [
                (0x8, 4, false, 0x8001_001C),
                (0x14, 4, false, 0x8001_0008),
                (0x30, 4, true, 0x8001_000C),
                (0x40, 0, false, 0x8001_0018),
            ]
        );
        let text = exe.field_refs_text(0x8002_0010, Some(0x14));
        assert!(
            text.contains("1 uses of offset 0x14") && text.contains("at 0x80010008"),
            "{text}"
        );
    }

    #[test]
    fn an_unexplored_stretch_is_named_for_each_thing_in_it() {
        // An entry that returns; then 4 KiB that returns a lot but nothing reaches, 8 KiB of
        // zeroes, and 4 KiB of words that are neither code nor pointers. One gap, three kinds.
        let mut words = vec![0x03E0_0008, 0];
        words.extend((0..512).flat_map(|_| [0x03E0_0008, 0, 0, 0]).take(1024));
        words.extend(vec![0u32; 2048]);
        words.extend(vec![0x1234_5678u32; 1024]);
        let exe = exe_with(&words);
        let c = exe.coverage(10);
        let bytes = |kind: &str| c.unexplored_by_kind.iter().find(|(k, _)| k == kind).map(|k| k.1);
        assert_eq!(bytes("code"), Some(4096), "{:?}", c.unexplored_by_kind);
        assert_eq!(bytes("mostly zeros"), Some(8192), "{:?}", c.unexplored_by_kind);
        assert_eq!(bytes("data"), Some(4096), "{:?}", c.unexplored_by_kind);
        let hints: Vec<&str> = c.gaps.iter().map(|g| g.hint.as_str()).collect();
        assert!(hints.contains(&"code") && hints.contains(&"mostly zeros"), "{hints:?}");
    }

    /// An overlay at 0x80100000 of `words`, on its own.
    fn overlay_of(words: &[u32]) -> Binary {
        Binary::parse_psx_overlay(le(words), 0x8010_0000, None).unwrap()
    }

    fn extent(bin: &Binary, inside: u64) -> (u64, u64) {
        let f = bin.symbols().function_containing(inside).expect("in a function");
        (f.address, f.size)
    }

    #[test]
    fn a_head_hoisted_above_the_frame_setup_is_the_functions_start() {
        // GCC schedules the first `lui`/`lw` above `addiu $sp`: the prologue is found
        // 8 bytes into the function. Called (jal to its true start), or not.
        let head = [0x3C02_8012, 0x8C42_0010]; // lui $v0, 0x8012 / lw $v0, 0x10($v0)
        let body = [
            0x27BD_FFE8, // addiu $sp, $sp, -0x18
            0xAFBF_0014, // sw $ra, 0x14($sp)
            0x0C04_000C, // jal 0x80100030 (a leaf)
            0x0000_0000,
            0x8FBF_0014, // lw $ra, 0x14($sp)
            0x0000_0000,
            0x03E0_0008, // jr $ra
            0x27BD_0018, // addiu $sp, $sp, 0x18
        ];
        // The leaf at 0x80100030 calls back to the head's true start, 0x80100000.
        let leaf = [0x2402_0001, 0x03E0_0008, 0x0000_0000];
        let mut words: Vec<u32> = head.iter().chain(&body).copied().collect();
        words.extend([0x0000_0000, 0x0000_0000]); // padding
        words.extend(leaf);
        let called = overlay_of(&words);
        assert_eq!(extent(&called, 0x8010_0008), (0x8010_0000, 40));
        assert_eq!(extent(&called, 0x8010_0030), (0x8010_0030, 12));

        // Nothing calls the function: its head is a gap before the prologue, and
        // falls through into it.
        let mut words: Vec<u32> = vec![0x03E0_0008, 0x0000_0000]; // a function that returns
        words.extend(head);
        let mut body = body;
        body[2] = 0x0C04_0000; // jal 0x80100000
        words.extend(body);
        let uncalled = overlay_of(&words);
        assert_eq!(extent(&uncalled, 0x8010_0010), (0x8010_0008, 40));
        assert_eq!(extent(&uncalled, 0x8010_0000), (0x8010_0000, 8));
    }

    #[test]
    fn a_piece_falling_into_a_called_function_shares_its_tail() {
        // A's code runs into B's, and B is called too: GCC's layout for a function
        // sharing its tail with another, read as one function from A's start.
        let words = [
            0x0C04_0005, // 0x80100000 (A): jal 0x80100014 (B)
            0x0000_0000,
            0x0C04_0009, // jal 0x80100024 (never returns)
            0x0000_0000,
            0x2402_0001, // li $v0, 1: falls into B
            0x2402_0002, // 0x80100014 (B): li $v0, 2
            0x03E0_0008, // jr $ra
            0x0000_0000,
            0x0000_0000,
            0x0C04_0000, // 0x80100024: jal 0x80100000 (A)
            0x0000_0000,
            0x1000_FFFF, // b . (never returns)
            0x0000_0000,
        ];
        let bin = overlay_of(&words);
        assert_eq!(extent(&bin, 0x8010_0010), (0x8010_0000, 32));
        assert_eq!(extent(&bin, 0x8010_0014), (0x8010_0000, 32));
        assert_eq!(extent(&bin, 0x8010_0024), (0x8010_0024, 16));
    }

    #[test]
    fn a_switch_whose_table_is_outside_the_image_keeps_its_cases() {
        // The table is at 0x80200000, past the overlay: the `jr` can't be followed,
        // so the cases are reached by nothing; they are still the function's.
        let words = [
            0x27BD_FFE8, // addiu $sp, $sp, -0x18
            0xAFBF_0014, // sw $ra, 0x14($sp)
            0x2C82_0003, // sltiu $v0, $a0, 3
            0x1040_000B, // beqz $v0, end (0x80100038)
            0x0000_0000,
            0x0004_1080, // sll $v0, $a0, 2
            0x3C01_8020, // lui $at, 0x8020
            0x0022_0821, // addu $at, $at, $v0
            0x8C22_0000, // lw $v0, 0($at)
            0x0000_0000,
            0x0040_0008, // jr $v0
            0x0000_0000,
            0x2402_0001, // case 0 (0x80100030): li $v0, 1
            0x1000_0001, // b end
            0x0000_0000,
            0x8FBF_0014, // end (0x8010003c): lw $ra, 0x14($sp)
            0x0000_0000,
            0x03E0_0008, // jr $ra
            0x27BD_0018, // addiu $sp, $sp, 0x18
            0x27BD_FFE8, // the next function (0x8010004c)
            0xAFBF_0014,
            0x8FBF_0014,
            0x03E0_0008,
            0x27BD_0018,
        ];
        let bin = overlay_of(&words);
        assert_eq!(extent(&bin, 0x8010_0034), (0x8010_0000, 0x4C));
        assert_eq!(extent(&bin, 0x8010_004C), (0x8010_004C, 20));
        // With no default path, the epilogue is reached by nothing either: the
        // whole tail after the `jr` is the function's, as it branches back into it.
        let mut words = words.to_vec();
        words[3] = 0x0000_0000;
        let bin = overlay_of(&words);
        assert_eq!(extent(&bin, 0x8010_0040), (0x8010_0000, 0x4C));
    }

    #[test]
    fn a_note_with_a_size_and_no_name_sizes_the_function() {
        let mut words = caller(0x8010_0000);
        words.extend(caller(0x8010_0000));
        let mut bin = overlay_of(&words);
        assert_eq!(extent(&bin, 0x8010_0000), (0x8010_0000, 32));
        bin.set_annotations(vec![crate::model::Annotation {
            address: 0x8010_0000,
            size: 0x48,
            ..Default::default()
        }]);
        let f = bin.symbols().at(0x8010_0000).unwrap();
        assert_eq!((f.name(), f.size), ("sub_80100000", 0x48));
        // A named one keeps its name.
        bin.set_annotations(vec![
            crate::model::Annotation {
                address: 0x8010_0000,
                name: "step".into(),
                ..Default::default()
            },
            crate::model::Annotation {
                address: 0x8010_0000,
                size: 0x40,
                ..Default::default()
            },
        ]);
        let f = bin.symbols().at(0x8010_0000).unwrap();
        assert_eq!((f.name(), f.size), ("step", 0x40));
    }

    #[test]
    fn a_frameless_function_nothing_calls_is_found_between_functions() {
        let mut words = caller(0x8010_0000); // a function calling itself; 9 words
        words.extend([0x2402_0005, 0x03E0_0008, 0x0000_0000]); // li $v0, 5 / jr $ra / nop
        words.extend(caller(0x8010_0000));
        let bin = overlay_of(&words);
        assert_eq!(extent(&bin, 0x8010_0024), (0x8010_0024, 12));
        assert_eq!(extent(&bin, 0x8010_0030), (0x8010_0030, 32));
        // A lone return between functions is padding, not a function.
        let mut words = caller(0x8010_0000);
        words.extend([0x03E0_0008, 0x0000_0000]);
        words.extend(caller(0x8010_0000));
        let bin = overlay_of(&words);
        assert!(bin.symbols().function_containing(0x8010_0024).is_none());
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
        // After the prologues: a trace's seeds fill in what those didn't reach.
        parts.late_entries.extend(seeds);
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
        for (at, v) in [
            (0x10, 0x8001_0000u32),
            (0x14, 0x8001_8000),
            (0x18, 0x8001_0000),
            (0x1C, 0x20),
        ] {
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        let words = [0x03E0_0008u32, 0, 0, 0, 0x2402_0001, 0x03E0_0008, 0, 0];
        data.extend(words.iter().flat_map(|w| w.to_le_bytes()));
        let bin = Binary::parse(data).unwrap();
        // Following alone doesn't reach it; the code between the functions is read
        // as one of its own (it does something, then returns).
        let f = bin.symbols().function_containing(0x8001_0010).unwrap();
        assert_eq!((f.address, f.size), (0x8001_0010, 12));
        let (traced, s) = bin
            .with_psx_trace(
                "80010000: 03e00008 jr ra\n80010004: 00000000 nop\n0x80010010\n80010014 jr $ra\nnot a line\n",
            )
            .unwrap();
        assert_eq!((s.lines, s.addresses, s.placed, s.new_runs), (4, 4, 4, 0));
        let f = traced.symbols().function_containing(0x8001_0010).unwrap();
        assert_eq!((f.address, f.size), (0x8001_0010, 12));
        assert_eq!(parse_trace("00010010\n").1, [0x8001_0010]);
    }
}
