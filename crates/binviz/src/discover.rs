//! Function boundaries recovered without a symbol table, the way a
//! disassembler bootstraps a stripped binary: the PE exception directory
//! (`.pdata`), `.eh_frame` FDEs (ELF, Mach-O), and Mach-O `LC_FUNCTION_STARTS`.

use gimli::UnwindSection;
use object::{Object, ObjectSection};

use crate::model::{Format, RegionKind, Section, Segment};
use crate::util::{Bytes, Endian};

/// (start address, size) pairs; size 0 means "to the next function".
pub(crate) fn discover(
    file: &object::File<'_>,
    format: Format,
    b: &Bytes,
    sections: &[Section],
    segments: &[Segment],
    image_base: u64,
    is64: bool,
) -> Vec<(u64, u64)> {
    let mut out = match format {
        Format::Pe => pdata(b, sections, image_base, is64, file.architecture()),
        Format::MachO => {
            let mut v = function_starts(b, segments, is64);
            v.extend(eh_frame(file, b.endian, is64));
            v
        }
        Format::Elf => eh_frame(file, b.endian, is64),
        _ => Vec::new(),
    };
    let in_code = |a: u64| {
        sections
            .iter()
            .any(|s| s.kind == RegionKind::Code && s.loaded && a >= s.address && a < s.address + s.size)
    };
    out.retain(|&(a, _)| in_code(a));
    out.sort_unstable();
    out.dedup_by_key(|&mut (a, _)| a);
    out
}

fn rva_to_offset(sections: &[Section], image_base: u64, rva: u64) -> Option<u64> {
    let va = image_base + rva;
    sections.iter().find_map(|s| {
        let o = s.file_offset?;
        (va >= s.address && va - s.address < s.file_size).then(|| o + (va - s.address))
    })
}

/// RUNTIME_FUNCTION entries of the exception directory.
fn pdata(b: &Bytes, sections: &[Section], image_base: u64, is64: bool, arch: object::Architecture) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let lfanew = b.u32(60).unwrap_or(0) as u64;
    let dirs = lfanew + 24 + if is64 { 112 } else { 96 };
    let (Some(rva), Some(size)) = (b.u32(dirs + 3 * 8), b.u32(dirs + 3 * 8 + 4)) else {
        return out;
    };
    if rva == 0 || size == 0 {
        return out;
    }
    let Some(off) = rva_to_offset(sections, image_base, rva as u64) else {
        return out;
    };
    let arm64 = arch == object::Architecture::Aarch64;
    let entry = if arm64 { 8 } else { 12 };
    for i in 0..(size as u64 / entry).min(1 << 20) {
        let e = off + i * entry;
        let Some(begin) = b.u32(e) else { break };
        let len = if arm64 {
            let unwind = b.u32(e + 4).unwrap_or(0);
            if unwind & 3 != 0 {
                // Packed unwind data: FunctionLength in units of 4 bytes.
                ((unwind >> 2) & 0x7ff) as u64 * 4
            } else {
                rva_to_offset(sections, image_base, unwind as u64)
                    .and_then(|x| b.u32(x))
                    .map_or(0, |h| (h & 0x3_ffff) as u64 * 4)
            }
        } else {
            b.u32(e + 4).unwrap_or(begin).saturating_sub(begin) as u64
        };
        if begin != 0 {
            out.push((image_base + begin as u64, len));
        }
    }
    out
}

/// Frame description entries: each covers one function (or a cold fragment).
fn eh_frame(file: &object::File<'_>, endian: Endian, is64: bool) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let Some(section) = file.section_by_name(".eh_frame") else {
        return out;
    };
    let Ok(data) = section.uncompressed_data() else {
        return out;
    };
    let endian = match endian {
        Endian::Little => gimli::RunTimeEndian::Little,
        Endian::Big => gimli::RunTimeEndian::Big,
    };
    let mut eh = gimli::EhFrame::new(&data, endian);
    eh.set_address_size(if is64 { 8 } else { 4 });
    let text = file.section_by_name(".text").map_or(0, |s| s.address());
    let bases = gimli::BaseAddresses::default()
        .set_eh_frame(section.address())
        .set_text(text);
    let mut entries = eh.entries(&bases);
    while let Ok(Some(entry)) = entries.next() {
        if let gimli::CieOrFde::Fde(partial) = entry
            && let Ok(fde) = partial.parse(|s, b, o| s.cie_from_offset(b, o))
            && fde.len() > 0
        {
            out.push((fde.initial_address(), fde.len()));
        }
    }
    out
}

/// LC_FUNCTION_STARTS: ULEB128 deltas between function starts, from __TEXT.
fn function_starts(b: &Bytes, segments: &[Segment], is64: bool) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let Some(text) = segments.iter().find(|s| s.name == "__TEXT") else {
        return out;
    };
    let ncmds = b.u32(16).unwrap_or(0);
    let mut off: u64 = if is64 { 32 } else { 28 };
    for _ in 0..ncmds.min(65536) {
        let (Some(cmd), Some(size)) = (b.u32(off), b.u32(off + 4)) else {
            break;
        };
        if cmd == object::macho::LC_FUNCTION_STARTS.0 {
            let (dataoff, datasize) = (b.u32(off + 8).unwrap_or(0) as u64, b.u32(off + 12).unwrap_or(0) as u64);
            let mut pos = dataoff;
            let mut addr = text.address;
            while pos < dataoff + datasize {
                let mut delta = 0u64;
                let mut shift = 0;
                loop {
                    let Some(byte) = b.u8(pos) else { return out };
                    pos += 1;
                    if shift < 64 {
                        delta |= u64::from(byte & 0x7f) << shift;
                    }
                    shift += 7;
                    if byte & 0x80 == 0 {
                        break;
                    }
                }
                if delta == 0 {
                    break;
                }
                addr += delta;
                out.push((addr, 0));
            }
        }
        if size < 8 {
            break;
        }
        off += size as u64;
    }
    out
}
