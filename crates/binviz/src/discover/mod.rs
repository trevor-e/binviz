//! Function boundaries recovered without a symbol table, the way a
//! disassembler bootstraps a stripped binary: the PE exception directory
//! (`.pdata`), `.eh_frame` FDEs (ELF, Mach-O), Mach-O `LC_FUNCTION_STARTS`,
//! and for 32-bit PE images, which have none of these, the code itself,
//! followed from the entry point (see [`x86`]).

pub(crate) mod x86;

use gimli::UnwindSection;
use object::{Object, ObjectSection};

use crate::model::{Format, Import, RegionKind, Section, Segment};
use crate::util::{Bytes, Endian};

/// What the file itself says about its code, for following it.
pub(crate) struct Known<'a> {
    pub entry: Option<u64>,
    /// Functions its own tables name: symbols, exports, import thunks.
    pub functions: Vec<u64>,
    pub imports: &'a [Import],
}

/// Functions found without a symbol table, and the tables their jumps read.
#[derive(Default)]
pub(crate) struct Discovery {
    /// (start address, size), sorted; size 0 means "to the next function".
    pub functions: Vec<(u64, u64)>,
    /// Jump tables (and the index tables that pick their entries), sorted.
    pub tables: Vec<x86::Table>,
    /// Pieces of functions away from their entry: (start, end, the function's start), sorted.
    pub parts: Vec<(u64, u64, u64)>,
    /// How the functions were found, when that took following the code.
    pub note: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn discover(
    file: &object::File<'_>,
    format: Format,
    b: &Bytes,
    sections: &[Section],
    segments: &[Segment],
    image_base: u64,
    is64: bool,
    known: &Known,
) -> Discovery {
    // x86-64 PE lists its functions in .pdata, but not the leaf functions, which
    // need no unwind data: following the code from those finds them.
    if format == Format::Pe && file.architecture() == object::Architecture::X86_64 {
        let listed = pdata(b, sections, image_base, is64, file.architecture());
        let mut known = Known {
            entry: known.entry,
            functions: known.functions.clone(),
            imports: known.imports,
        };
        known.functions.extend(listed.iter().map(|f| f.0));
        let image = pe_image(b, sections, image_base, &known, 64);
        let found = x86::follow(&image);
        let mut covered: Vec<(u64, u64)> = listed.iter().map(|&(s, n)| (s, s + n.max(1))).collect();
        covered.sort_unstable();
        let mut functions = listed.clone();
        functions.extend(
            found
                .functions
                .iter()
                .copied()
                .filter(|&(s, _)| covered.binary_search_by(|c| c.0.cmp(&s)).is_err() && !within_ranges(&covered, s)),
        );
        functions.sort_unstable();
        functions.dedup_by_key(|f| f.0);
        let note = format!(
            "{} functions from the exception directory (.pdata), and {} more (leaf functions, which have no \
             unwind data) following the code from those",
            listed.len(),
            functions.len() - listed.len()
        );
        return Discovery {
            functions,
            tables: found.tables,
            parts: found.parts.into_iter().filter(|p| !within_ranges(&covered, p.0)).collect(),
            note: Some(note),
        };
    }
    if format == Format::Pe && file.architecture() == object::Architecture::I386 {
        let image = pe_image(b, sections, image_base, known, 32);
        let found = x86::follow(&image);
        let switches = found.tables.iter().filter(|t| t.entry == 4).count();
        let note = format!(
            "{} functions{} and {switches} jump tables, following the code from the entry point, exports, TLS \
             callbacks and exception handlers, then from the code addresses the image holds ({})",
            found.functions.len(),
            match found.parts.len() {
                0 => String::new(),
                1 => " (one with a piece away from its entry)".into(),
                n => format!(" ({n} pieces of them away from their entries)"),
            },
            if image.relocations.is_some() {
                "its base relocations say which words are addresses"
            } else {
                "it has no base relocations: words that land in the code may be addresses"
            }
        );
        return Discovery {
            functions: found.functions,
            tables: found.tables,
            parts: found.parts,
            note: Some(note),
        };
    }
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
    out.shrink_to_fit();
    Discovery {
        functions: out,
        tables: Vec::new(),
        parts: Vec::new(),
        note: None,
    }
}

/// A 32-bit PE image as the code follower sees it: its sections' bytes; the
/// entry point, exports, import thunks, TLS callbacks and safe exception
/// handlers as starts; its base relocations; and its import slots.
fn pe_image<'a>(b: &Bytes<'a>, sections: &[Section], image_base: u64, known: &Known, bits: u32) -> x86::Image<'a> {
    let mut regions: Vec<x86::Region<'a>> = sections
        .iter()
        .filter(|s| s.loaded && !s.compressed)
        .filter_map(|s| {
            Some(x86::Region {
                address: s.address,
                bytes: b.slice(s.file_offset?, s.file_size.min(s.size))?,
                code: s.kind == RegionKind::Code,
            })
        })
        .collect();
    regions.sort_by_key(|r| r.address);
    let mut starts = known.functions.clone();
    starts.extend(known.entry);
    starts.extend(tls_callbacks(b, sections, image_base));
    starts.extend(safe_seh_handlers(b, sections, image_base));
    x86::Image {
        bits,
        regions,
        starts,
        relocations: base_relocations(b, sections, image_base),
        slots: known
            .imports
            .iter()
            .filter_map(|i| Some((i.address?, never_returns(&i.name))))
            .collect(),
    }
}

/// Whether a PE image's optional header is PE32+ (a 64-bit image).
fn pe32_plus(b: &Bytes) -> bool {
    b.u32(60).and_then(|lfanew| b.u16(lfanew as u64 + 24)) == Some(0x20B)
}

/// The (RVA, size) of a PE data directory, if it is there.
fn pe32_directory(b: &Bytes, index: u64) -> Option<(u64, u64)> {
    let lfanew = b.u32(60)? as u64;
    // The directories follow the optional header's fields, 16 bytes further in PE32+.
    let wide = if pe32_plus(b) { 16 } else { 0 };
    let at = lfanew + 24 + 96 + wide + index * 8;
    let (rva, size) = (b.u32(at)? as u64, b.u32(at + 4)? as u64);
    // NumberOfRvaAndSizes says how many directories the header has.
    (index < b.u32(lfanew + 24 + 92 + wide)? as u64 && rva != 0 && size != 0).then_some((rva, size))
}

/// Whether `a` is in one of the sorted, non-overlapping ranges.
fn within_ranges(ranges: &[(u64, u64)], a: u64) -> bool {
    let i = ranges.partition_point(|r| r.0 <= a);
    i > 0 && a < ranges[i - 1].1
}

/// The functions the TLS directory lists, which the loader calls as threads start and end.
fn tls_callbacks(b: &Bytes, sections: &[Section], image_base: u64) -> Vec<u64> {
    let va_to_offset = |va: u64| rva_to_offset(sections, image_base, va.checked_sub(image_base)?);
    // AddressOfCallBacks, and the words of the list: 32 or 64 bits.
    let wide = pe32_plus(b);
    let word = |at: u64| if wide { b.u64(at) } else { b.u32(at).map(u64::from) };
    let Some(list) = pe32_directory(b, 9)
        .and_then(|(rva, _)| rva_to_offset(sections, image_base, rva))
        .and_then(|dir| word(dir + if wide { 24 } else { 12 }))
        .and_then(va_to_offset)
    else {
        return Vec::new();
    };
    let size = if wide { 8 } else { 4 };
    (0..256).map_while(|i| word(list + size * i).filter(|&va| va != 0)).collect()
}

/// The exception handlers the load configuration lists as safe (`/SAFESEH`).
fn safe_seh_handlers(b: &Bytes, sections: &[Section], image_base: u64) -> Vec<u64> {
    // 64-bit code unwinds by table, with no handler list.
    if pe32_plus(b) {
        return Vec::new();
    }
    let Some(config) = pe32_directory(b, 10).and_then(|(rva, _)| rva_to_offset(sections, image_base, rva)) else {
        return Vec::new();
    };
    // SEHandlerTable and SEHandlerCount, at 0x40 and 0x44 of IMAGE_LOAD_CONFIG_DIRECTORY32.
    let (Some(size), Some(table), Some(count)) = (b.u32(config), b.u32(config + 0x40), b.u32(config + 0x44)) else {
        return Vec::new();
    };
    let Some(table) = (size >= 0x48 && table != 0)
        .then(|| rva_to_offset(sections, image_base, (table as u64).checked_sub(image_base)?))
        .flatten()
    else {
        return Vec::new();
    };
    (0..count.min(1 << 16) as u64)
        .map_while(|i| b.u32(table + 4 * i))
        .map(|rva| image_base + rva as u64)
        .collect()
}

/// Every address the base relocations patch (HIGHLOW and DIR64 entries),
/// sorted; `None` for an image without them (linked `/FIXED`).
fn base_relocations(b: &Bytes, sections: &[Section], image_base: u64) -> Option<Vec<u64>> {
    let (rva, size) = pe32_directory(b, 5)?;
    let start = rva_to_offset(sections, image_base, rva)?;
    let end = start + size;
    let mut out = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        let (Some(page), Some(block)) = (b.u32(pos), b.u32(pos + 4)) else {
            break;
        };
        if block < 8 {
            break;
        }
        for i in 0..(block as u64 - 8) / 2 {
            let Some(v) = b.u16(pos + 8 + 2 * i) else { break };
            if matches!(v >> 12, 3 | 10) {
                out.push(image_base + page as u64 + (v & 0xFFF) as u64);
            }
        }
        pos += block as u64;
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}

/// Imported functions that never return: the code after a call to one is
/// something else (the next function, often). Decorated stdcall names
/// (`_ExitProcess@4`) count as their plain ones.
fn never_returns(name: &str) -> bool {
    const NAMES: &[&str] = &[
        "ExitProcess",
        "ExitThread",
        "FreeLibraryAndExitThread",
        "FatalExit",
        "FatalAppExitA",
        "FatalAppExitW",
        "RaiseFailFastException",
        "RtlExitUserProcess",
        "RtlExitUserThread",
        "exit",
        "_exit",
        "_Exit",
        "quick_exit",
        "abort",
        "_amsg_exit",
        "_invoke_watson",
        "_invalid_parameter_noinfo_noreturn",
        "_CxxThrowException",
        "__std_terminate",
        "?terminate@@YAXXZ",
        "terminate",
        "_purecall",
        "longjmp",
        "_longjmp",
        "__longjmp_chk",
        "__stack_chk_fail",
        "__assert_fail",
        "__cxa_throw",
        "__cxa_rethrow",
        "_Unwind_Resume",
        "pthread_exit",
    ];
    let plain = match name.rsplit_once('@') {
        Some((n, size)) if !size.is_empty() && size.bytes().all(|b| b.is_ascii_digit()) => {
            n.strip_prefix('_').unwrap_or(n)
        }
        _ => name,
    };
    NAMES.contains(&name) || NAMES.contains(&plain)
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
