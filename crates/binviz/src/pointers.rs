//! Pointers stored in data: places where the image holds the address of
//! another part of itself — vtables, tables of callbacks, Objective-C metadata
//! and selector references, GOT slots of local functions.
//!
//! How to find them depends on how the loader relocates the image:
//!
//! - Mach-O with chained fixups (the default since iOS 15 and macOS 12): each
//!   pointer is a link in a chain that dyld walks, packed in one of several
//!   formats; walking the chains finds every rebased pointer and its target.
//! - Position-independent ELF: the targets are the addends of `RELATIVE` (and
//!   symbolic) relocations, or for `.relr.dyn`, the words the packed
//!   relocations mark.
//! - Everything else stores plain addresses: aligned words in the data sections
//!   whose value is an address of the image.

use object::{Architecture, Object, ObjectKind, ObjectSymbol, ObjectSymbolTable};

use crate::binary::Binary;
use crate::model::{Format, RegionKind, Section};
use crate::util::{Bytes, Endian};

pub(crate) enum Scheme {
    /// Plain addresses in the data sections; for Mach-O with dyld info, also
    /// where the loader binds pointers to symbols: (address, symbol), sorted.
    Plain { binds: Vec<(u64, String)> },
    /// Mach-O chained fixups: the image's base address, per segment with
    /// fixups (start, end, file offset, pointer format), and the symbols
    /// binds refer to by ordinal.
    Chained {
        base: u64,
        segments: Vec<ChainedSegment>,
        imports: Vec<String>,
    },
    /// Relocated ELF: (location, target), sorted by location.
    Relocated(Vec<(u64, u64)>),
}

#[derive(Clone, Copy)]
pub(crate) struct ChainedSegment {
    start: u64,
    end: u64,
    file_offset: u64,
    format: u16,
    page_size: u64,
    /// Where the page starts are in the file, and how many.
    starts: u64,
    pages: u64,
}

/// Section names that hold text rather than pointers.
pub(crate) fn stringy(name: &str) -> bool {
    [
        "cstring",
        "methname",
        "classname",
        "methtype",
        "ustring",
        "reflstr",
        ".str",
        "gcc_except",
        "swift5_",
    ]
    .iter()
    .any(|s| name.contains(s))
}

/// The import a chained pointer binds to (its ordinal), if it is a bind.
fn chained_bind(v: u64, format: u16) -> Option<usize> {
    match format {
        // DYLD_CHAINED_PTR_ARM64E, _ARM64E_USERLAND: a 16-bit ordinal; _USERLAND24: 24 bits.
        1 | 9 => ((v >> 62) & 1 == 1).then_some((v & 0xFFFF) as usize),
        12 => ((v >> 62) & 1 == 1).then_some((v & 0xFF_FFFF) as usize),
        // DYLD_CHAINED_PTR_64, _64_OFFSET
        2 | 6 => (v >> 63 == 1).then_some((v & 0xFF_FFFF) as usize),
        _ => None,
    }
}

/// The names of the symbols binds refer to, by ordinal (`dyld_chained_import`s).
fn chained_imports(b: &Bytes, data: &[u8], dataoff: u64) -> Vec<String> {
    let (Some(imports), Some(symbols), Some(count), Some(format)) = (
        b.u32(dataoff + 8),
        b.u32(dataoff + 12),
        b.u32(dataoff + 16),
        b.u32(dataoff + 20),
    ) else {
        return Vec::new();
    };
    let (imports, symbols) = (dataoff + imports as u64, dataoff + symbols as u64);
    let name = |off: u64| -> String {
        let start = (symbols + off) as usize;
        let bytes = data.get(start..).unwrap_or_default();
        let end = bytes.iter().take(4096).position(|&c| c == 0).unwrap_or(0);
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    };
    let mut out = Vec::new();
    for i in 0..count.min(1 << 20) as u64 {
        let offset = match format {
            1 => b.u32(imports + 4 * i).map(|v| (v >> 9) as u64),
            2 => b.u32(imports + 8 * i).map(|v| (v >> 9) as u64),
            3 => b.u64(imports + 16 * i).map(|v| v >> 32),
            _ => None,
        };
        let Some(offset) = offset else { break };
        out.push(name(offset));
    }
    out
}

/// Where the loader binds pointers to symbols in a Mach-O image with dyld
/// info (`LC_DYLD_INFO`): (address, symbol), sorted by address.
fn dyld_info_binds(b: &Bytes, bin: &Binary) -> Vec<(u64, String)> {
    use object::macho;
    let mut out = Vec::new();
    let Some(ncmds) = b.u32(16) else { return out };
    let mut off: u64 = if bin.is64 { 32 } else { 28 };
    // Binds are placed by segment index: the segments' addresses, in load command order.
    let mut segments = Vec::new();
    let mut info = None;
    for _ in 0..ncmds.min(65536) {
        let (Some(cmd), Some(size)) = (b.u32(off), b.u32(off + 4)) else {
            break;
        };
        if cmd == macho::LC_SEGMENT_64.0 {
            segments.push(b.u64(off + 24).unwrap_or(0));
        } else if cmd == macho::LC_SEGMENT.0 {
            segments.push(b.u32(off + 24).unwrap_or(0) as u64);
        } else if cmd == macho::LC_DYLD_INFO.0 || cmd == macho::LC_DYLD_INFO_ONLY.0 {
            info = Some(off);
        }
        if size < 8 {
            break;
        }
        off += size as u64;
    }
    let Some(info) = info else { return out };
    let Some(command) = b.slice(
        info,
        std::mem::size_of::<macho::DyldInfoCommand<object::Endianness>>() as u64,
    ) else {
        return out;
    };
    let Ok((command, _)) = object::pod::from_bytes::<macho::DyldInfoCommand<object::Endianness>>(command) else {
        return out;
    };
    let endian = match bin.endian {
        Endian::Little => object::Endianness::Little,
        Endian::Big => object::Endianness::Big,
    };
    let pointer = if bin.is64 { 8 } else { 4 };
    let Ok(mut binds) = command.binds(endian, &*bin.data, pointer) else {
        return out;
    };
    while let Ok(Some(bind)) = binds.next() {
        if let Some(&segment) = segments.get(bind.segment_index as usize) {
            out.push((segment + bind.segment_offset, crate::util::lossy(bind.symbol)));
        }
        if out.len() >= 1 << 22 {
            break;
        }
    }
    out.sort_by_key(|b| b.0);
    out.dedup_by_key(|b| b.0);
    out
}

/// Decodes one link of a chain: the target if it is a rebase (binds point
/// outside the image), and the distance to the next link (0 at the end).
fn decode_chained(v: u64, format: u16, base: u64) -> (Option<u64>, u64) {
    match format {
        // DYLD_CHAINED_PTR_ARM64E, _ARM64E_USERLAND, _ARM64E_USERLAND24
        1 | 9 | 12 => {
            let next = ((v >> 51) & 0x7FF) * 8;
            let target = if (v >> 62) & 1 == 1 {
                None
            } else if v >> 63 == 1 {
                // Authenticated rebase: an offset from the base.
                Some(base + (v & 0xFFFF_FFFF))
            } else {
                let t = v & 0x7FF_FFFF_FFFF;
                Some(if format == 1 { t } else { base + t })
            };
            (target, next)
        }
        // DYLD_CHAINED_PTR_64 (target is an address), _64_OFFSET (an offset)
        2 | 6 => {
            let next = ((v >> 51) & 0xFFF) * 4;
            let target = (v >> 63 == 0).then(|| {
                let t = v & 0xF_FFFF_FFFF;
                if format == 2 { t } else { base + t }
            });
            (target, next)
        }
        _ => (None, 0),
    }
}

fn chained_segments(b: &Bytes, bin: &Binary) -> Option<(u64, Vec<ChainedSegment>, Vec<String>)> {
    let ncmds = b.u32(16)?;
    let mut off: u64 = if bin.is64 { 32 } else { 28 };
    let mut data = None;
    for _ in 0..ncmds.min(65536) {
        let (cmd, size) = (b.u32(off)?, b.u32(off + 4)?);
        if cmd == object::macho::LC_DYLD_CHAINED_FIXUPS.0 {
            data = Some((b.u32(off + 8)? as u64, b.u32(off + 12)? as u64));
        }
        if size < 8 {
            break;
        }
        off += size as u64;
    }
    let (dataoff, datasize) = data.filter(|d| d.1 > 0)?;
    // The image's base: where the Mach header is mapped.
    let base = bin
        .segments
        .iter()
        .find(|s| s.file_offset == 0 && s.file_size > 0)?
        .address;
    let image = dataoff + b.u32(dataoff + 4)? as u64;
    let count = b.u32(image)? as u64;
    let mut out = Vec::new();
    for i in 0..count.min(256) {
        let info = b.u32(image + 4 + 4 * i)? as u64;
        if info == 0 {
            continue;
        }
        let seg = image + info;
        if seg >= dataoff + datasize {
            continue;
        }
        let page_size = b.u16(seg + 4)? as u64;
        let format = b.u16(seg + 6)?;
        let start = base + b.u64(seg + 8)?;
        let pages = b.u16(seg + 20)? as u64;
        let Some(mapped) = bin
            .segments
            .iter()
            .find(|s| s.mapped && start >= s.address && start < s.address + s.mem_size.max(1))
        else {
            continue;
        };
        out.push(ChainedSegment {
            start,
            end: start + pages * page_size,
            file_offset: mapped.file_offset + (start - mapped.address),
            format,
            page_size,
            starts: seg + 22,
            pages,
        });
    }
    Some((base, out, chained_imports(b, &bin.data, dataoff)))
}

fn relocated_elf(bin: &Binary, file: &object::File<'_>) -> Vec<(u64, u64)> {
    use object::elf;
    let arch = bin.arch;
    let word = |address: u64| bin.read_word(address);
    let mut out = Vec::new();
    let dynsyms = file.dynamic_symbol_table();
    if let Some(relocs) = file.dynamic_relocations() {
        for (offset, r) in relocs {
            let object::RelocationFlags::Elf { r_type } = r.flags() else {
                continue;
            };
            let relative = matches!(
                (arch, r_type),
                (Architecture::X86_64, elf::R_X86_64_RELATIVE)
                    | (Architecture::Aarch64, elf::R_AARCH64_RELATIVE)
                    | (Architecture::I386, elf::R_386_RELATIVE)
                    | (Architecture::Arm, elf::R_ARM_RELATIVE)
            );
            let symbolic = matches!(
                (arch, r_type),
                (Architecture::X86_64, elf::R_X86_64_64 | elf::R_X86_64_GLOB_DAT)
                    | (Architecture::Aarch64, elf::R_AARCH64_ABS64 | elf::R_AARCH64_GLOB_DAT)
                    | (Architecture::I386, elf::R_386_32 | elf::R_386_GLOB_DAT)
                    | (Architecture::Arm, elf::R_ARM_ABS32 | elf::R_ARM_GLOB_DAT)
            );
            let addend = if r.has_implicit_addend() {
                word(offset).unwrap_or(0)
            } else {
                r.addend() as u64
            };
            let target = if relative {
                Some(addend)
            } else if symbolic && let object::RelocationTarget::Symbol(index) = r.target() {
                dynsyms
                    .as_ref()
                    .and_then(|t| t.symbol_by_index(index).ok())
                    .filter(|s| s.is_definition())
                    .map(|s| s.address().wrapping_add(addend))
            } else {
                None
            };
            if let Some(t) = target {
                out.push((offset, t));
            }
        }
    }
    // Packed relative relocations: the words they mark hold their targets.
    let step = if bin.is64 { 8u64 } else { 4 };
    for sec in bin.sections.iter().filter(|s| s.name == ".relr.dyn") {
        let Some(bytes) = sec
            .file_offset
            .and_then(|o| bin.data.get(o as usize..(o + sec.file_size) as usize))
        else {
            continue;
        };
        let mut next = 0u64;
        for entry in bytes.chunks_exact(step as usize) {
            let v = read(entry, bin.endian);
            if v & 1 == 0 {
                if let Some(t) = word(v) {
                    out.push((v, t));
                }
                next = v + step;
            } else {
                let mut bits = v >> 1;
                let mut at = next;
                while bits != 0 {
                    if bits & 1 == 1
                        && let Some(t) = word(at)
                    {
                        out.push((at, t));
                    }
                    bits >>= 1;
                    at += step;
                }
                next += (step * 8 - 1) * step;
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out.shrink_to_fit();
    out
}

fn read(w: &[u8], endian: Endian) -> u64 {
    match (w.len(), endian) {
        (8, Endian::Little) => u64::from_le_bytes(w.try_into().expect("8 bytes")),
        (8, Endian::Big) => u64::from_be_bytes(w.try_into().expect("8 bytes")),
        (_, Endian::Little) => u32::from_le_bytes(w.try_into().expect("4 bytes")) as u64,
        (_, Endian::Big) => u32::from_be_bytes(w.try_into().expect("4 bytes")) as u64,
    }
}

impl Binary {
    /// How this image stores pointers (worked out once).
    pub(crate) fn scheme(&self) -> &Scheme {
        self.pointers.get_or_init(|| {
            let file = object::File::parse(&*self.data).ok();
            self.pointer_scheme(file.as_ref())
        })
    }

    /// Whether the pointer-sized word at `address` holds an address of the
    /// image, as the loader would read it.
    pub(crate) fn holds_pointer(&self, address: u64) -> bool {
        let size = if self.is64 { 8 } else { 4 };
        address.is_multiple_of(size)
            && self
                .decode_pointer(self.scheme(), address)
                .is_some_and(|t| t != 0 && self.section_at(t).is_some())
    }

    fn pointer_scheme(&self, file: Option<&object::File<'_>>) -> Scheme {
        let b = Bytes::new(&self.data, self.endian);
        match self.summary.format {
            Format::MachO => match chained_segments(&b, self) {
                Some((base, segments, imports)) => Scheme::Chained {
                    base,
                    segments,
                    imports,
                },
                None => Scheme::Plain {
                    binds: dyld_info_binds(&b, self),
                },
            },
            Format::Elf if file.is_some_and(|f| f.kind() == ObjectKind::Dynamic) => {
                Scheme::Relocated(relocated_elf(self, file.expect("checked")))
            }
            _ => Scheme::Plain { binds: Vec::new() },
        }
    }

    /// The pointer-sized word at a virtual address, as stored in the file.
    pub(crate) fn read_word(&self, address: u64) -> Option<u64> {
        let off = self.address_to_offset(address)? as usize;
        let n = if self.is64 { 8 } else { 4 };
        Some(read(self.data.get(off..off + n)?, self.endian))
    }

    /// Every pointer stored in data: `emit(location, target)`. Targets are not
    /// checked; the caller keeps those inside the image.
    pub(crate) fn scan_pointers(&self, scheme: &Scheme, emit: &mut dyn FnMut(u64, u64)) {
        match scheme {
            Scheme::Chained { base, segments, .. } => {
                let b = Bytes::new(&self.data, self.endian);
                for seg in segments {
                    for p in 0..seg.pages {
                        let Some(first) = b.u16(seg.starts + 2 * p) else { break };
                        // DYLD_CHAINED_PTR_START_NONE, or a 32-bit format's multi-start list.
                        if first == 0xFFFF || first & 0x8000 != 0 {
                            continue;
                        }
                        let page = seg.start + p * seg.page_size;
                        let mut at = page + first as u64;
                        while at + 8 <= page + seg.page_size && at + 8 <= seg.end {
                            let Some(v) = b.u64(seg.file_offset + (at - seg.start)) else {
                                break;
                            };
                            let (target, next) = decode_chained(v, seg.format, *base);
                            if let Some(t) = target {
                                emit(at, t);
                            }
                            if next == 0 {
                                break;
                            }
                            at += next;
                        }
                    }
                }
            }
            Scheme::Relocated(pairs) => {
                for &(at, t) in pairs {
                    emit(at, t);
                }
            }
            Scheme::Plain { .. } => {
                let step = if self.is64 { 8u64 } else { 4 };
                for sec in &self.sections {
                    if !sec.loaded
                        || !matches!(sec.kind, RegionKind::Data | RegionKind::Rodata | RegionKind::Tls)
                        || stringy(&sec.name)
                    {
                        continue;
                    }
                    let Some(bytes) = section_bytes(&self.data, sec) else {
                        continue;
                    };
                    let skip = ((step - sec.address % step) % step) as usize;
                    let Some(bytes) = bytes.get(skip..) else { continue };
                    for (i, w) in bytes.chunks_exact(step as usize).enumerate() {
                        let v = read(w, self.endian);
                        if v != 0 {
                            emit(sec.address + skip as u64 + i as u64 * step, v);
                        }
                    }
                }
            }
        }
    }

    /// The symbol the pointer at `address` is bound to by a Mach-O image's
    /// loader (`_OBJC_CLASS_$_NSObject`, say). Only for places known to hold
    /// pointers.
    pub(crate) fn bound_symbol(&self, address: u64) -> Option<&str> {
        match self.scheme() {
            Scheme::Chained { segments, imports, .. } => {
                let seg = segments.iter().find(|s| address >= s.start && address < s.end)?;
                let v = Bytes::new(&self.data, self.endian).u64(seg.file_offset + (address - seg.start))?;
                imports.get(chained_bind(v, seg.format)?).map(String::as_str)
            }
            Scheme::Plain { binds } => {
                let i = binds.binary_search_by_key(&address, |b| b.0).ok()?;
                Some(&binds[i].1)
            }
            Scheme::Relocated(_) => None,
        }
    }

    /// Decodes the pointer stored at `address` under `scheme`, if it holds one.
    pub(crate) fn decode_pointer(&self, scheme: &Scheme, address: u64) -> Option<u64> {
        match scheme {
            Scheme::Plain { .. } => self.read_word(address),
            Scheme::Chained { base, segments, .. } => {
                let seg = segments.iter().find(|s| address >= s.start && address < s.end)?;
                let v = Bytes::new(&self.data, self.endian).u64(seg.file_offset + (address - seg.start))?;
                decode_chained(v, seg.format, *base).0
            }
            Scheme::Relocated(pairs) => {
                let i = pairs.binary_search_by_key(&address, |p| p.0).ok()?;
                Some(pairs[i].1)
            }
        }
    }
}

/// File bytes of a section, when it has them all.
pub(crate) fn section_bytes<'a>(data: &'a [u8], sec: &Section) -> Option<&'a [u8]> {
    let off = sec.file_offset?;
    if sec.compressed {
        return None;
    }
    let len = sec.file_size.min(sec.size);
    data.get(off as usize..(off + len) as usize)
}
