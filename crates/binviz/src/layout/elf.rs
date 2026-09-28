//! ELF file layout.

use object::elf;

use super::decode::{Decoder, Entry, LinkedTable, Table, TableKind};
use super::fields::{self, F, FieldValue, Fmt};
use super::{Builder, Ctx, Machine};
use crate::model::RegionKind;
use crate::util::{self, hex};

pub(crate) fn flag_string<T>(names: &object::FlagNames<T>, value: T) -> String
where
    T: object::Wrap,
    T::Inner: Copy
        + PartialEq
        + Default
        + core::ops::BitAnd<Output = T::Inner>
        + core::ops::Not<Output = T::Inner>
        + core::fmt::LowerHex,
{
    let mut parts: Vec<String> = Vec::new();
    let rest = names.names(value, |_, n| parts.push(n.to_string()));
    if rest != T::Inner::default() {
        parts.push(format!("{rest:#x}"));
    }
    if parts.is_empty() {
        "0".into()
    } else {
        parts.join(" | ")
    }
}

fn machine(ctx: &Ctx) -> elf::Machine {
    match ctx.machine {
        Machine::Elf(m) => elf::Machine(m),
        _ => elf::EM_NONE,
    }
}

fn names(ctx: &Ctx) -> &'static elf::Names {
    elf::machine_names(machine(ctx))
}

fn et(v: u64) -> Option<&'static str> {
    elf::FileType(v as u16).name()
}
fn em(v: u64) -> Option<&'static str> {
    elf::Machine(v as u16).name()
}
fn class(v: u64) -> Option<&'static str> {
    elf::FileClass(v as u8).name()
}
fn data(v: u64) -> Option<&'static str> {
    elf::DataEncoding(v as u8).name()
}
fn osabi(v: u64) -> Option<&'static str> {
    elf::OsAbi(v as u8).name()
}
fn e_flags(ctx: &Ctx, v: u64) -> String {
    if v == 0 {
        return "0".into();
    }
    format!("{} ({})", flag_string(names(ctx).ef, elf::FileFlags(v as u32)), hex(v))
}
pub(crate) fn pt_name(ctx: &Ctx, v: u64) -> Option<String> {
    names(ctx).pt.name(elf::ProgramType(v as u32)).map(str::to_string)
}
fn pf(ctx: &Ctx, v: u64) -> String {
    format!(
        "{} ({})",
        perms(v as u32),
        flag_string(names(ctx).pf, elf::ProgramFlags(v as u32))
    )
}
fn sht(ctx: &Ctx, v: u64) -> Option<String> {
    names(ctx).sht.name(elf::SectionType(v as u32)).map(str::to_string)
}
fn shf(ctx: &Ctx, v: u64) -> String {
    if v == 0 {
        return "0".into();
    }
    format!("{} ({})", flag_string(names(ctx).shf, elf::SectionFlags(v)), hex(v))
}
fn shndx(ctx: &Ctx, v: u64) -> String {
    if let Some(n) = names(ctx).shn.name(elf::SymbolSection(v as u16)) {
        return format!("{n} ({v})");
    }
    // Our section list skips the null section, so ELF index N is entry N-1.
    match ctx.sections.get((v as usize).wrapping_sub(1)) {
        Some(s) if v > 0 => format!("{v} ({})", s.name),
        _ => v.to_string(),
    }
}
fn st_info(ctx: &Ctx, v: u64) -> String {
    let info = elf::SymbolInfo(v as u8);
    let n = names(ctx);
    let bind = n
        .stb
        .name(info.st_bind())
        .map_or_else(|| format!("bind {}", info.st_bind().0), str::to_string);
    let typ = n
        .stt
        .name(info.st_type())
        .map_or_else(|| format!("type {}", info.st_type().0), str::to_string);
    format!("{bind} {typ} ({})", hex(v))
}
fn st_other(ctx: &Ctx, v: u64) -> String {
    let vis = elf::SymbolVisibility(v as u8 & 0x3);
    let mut s = vis.name().unwrap_or("?").to_string();
    let rest = v & !0x3;
    if rest != 0 {
        s.push_str(&format!(
            " | {}",
            flag_string(names(ctx).sto, elf::SymbolOther(rest as u8))
        ));
    }
    format!("{s} ({})", hex(v))
}

pub(crate) fn perms(p_flags: u32) -> String {
    format!(
        "{}{}{}",
        if p_flags & elf::PF_R.0 != 0 { 'r' } else { '-' },
        if p_flags & elf::PF_W.0 != 0 { 'w' } else { '-' },
        if p_flags & elf::PF_X.0 != 0 { 'x' } else { '-' }
    )
}

const EHDR64: &[F] = &[
    F::bytes("ei_magic", 4),
    F::u8("ei_class", Fmt::Name(class)),
    F::u8("ei_data", Fmt::Name(data)),
    F::u8("ei_version", Fmt::Dec),
    F::u8("ei_osabi", Fmt::Name(osabi)),
    F::u8("ei_abiversion", Fmt::Dec),
    F::bytes("ei_pad", 7),
    F::u16("e_type", Fmt::Name(et)),
    F::u16("e_machine", Fmt::Name(em)),
    F::u32("e_version", Fmt::Dec),
    F::u64("e_entry", Fmt::Addr),
    F::u64("e_phoff", Fmt::Off),
    F::u64("e_shoff", Fmt::Off),
    F::u32("e_flags", Fmt::Custom(e_flags)),
    F::u16("e_ehsize", Fmt::Size),
    F::u16("e_phentsize", Fmt::Size),
    F::u16("e_phnum", Fmt::Dec),
    F::u16("e_shentsize", Fmt::Size),
    F::u16("e_shnum", Fmt::Dec),
    F::u16("e_shstrndx", Fmt::Dec),
];

const EHDR32: &[F] = &[
    F::bytes("ei_magic", 4),
    F::u8("ei_class", Fmt::Name(class)),
    F::u8("ei_data", Fmt::Name(data)),
    F::u8("ei_version", Fmt::Dec),
    F::u8("ei_osabi", Fmt::Name(osabi)),
    F::u8("ei_abiversion", Fmt::Dec),
    F::bytes("ei_pad", 7),
    F::u16("e_type", Fmt::Name(et)),
    F::u16("e_machine", Fmt::Name(em)),
    F::u32("e_version", Fmt::Dec),
    F::u32("e_entry", Fmt::Addr),
    F::u32("e_phoff", Fmt::Off),
    F::u32("e_shoff", Fmt::Off),
    F::u32("e_flags", Fmt::Custom(e_flags)),
    F::u16("e_ehsize", Fmt::Size),
    F::u16("e_phentsize", Fmt::Size),
    F::u16("e_phnum", Fmt::Dec),
    F::u16("e_shentsize", Fmt::Size),
    F::u16("e_shnum", Fmt::Dec),
    F::u16("e_shstrndx", Fmt::Dec),
];

const PHDR64: &[F] = &[
    F::u32("p_type", Fmt::CtxName(pt_name)),
    F::u32("p_flags", Fmt::Custom(pf)),
    F::u64("p_offset", Fmt::Off),
    F::u64("p_vaddr", Fmt::Addr),
    F::u64("p_paddr", Fmt::Addr),
    F::u64("p_filesz", Fmt::Size),
    F::u64("p_memsz", Fmt::Size),
    F::u64("p_align", Fmt::Hex),
];

const PHDR32: &[F] = &[
    F::u32("p_type", Fmt::CtxName(pt_name)),
    F::u32("p_offset", Fmt::Off),
    F::u32("p_vaddr", Fmt::Addr),
    F::u32("p_paddr", Fmt::Addr),
    F::u32("p_filesz", Fmt::Size),
    F::u32("p_memsz", Fmt::Size),
    F::u32("p_flags", Fmt::Custom(pf)),
    F::u32("p_align", Fmt::Hex),
];

const SHDR64: &[F] = &[
    F::u32("sh_name", Fmt::StrIndex),
    F::u32("sh_type", Fmt::CtxName(sht)),
    F::u64("sh_flags", Fmt::Custom(shf)),
    F::u64("sh_addr", Fmt::Addr),
    F::u64("sh_offset", Fmt::Off),
    F::u64("sh_size", Fmt::Size),
    F::u32("sh_link", Fmt::Dec),
    F::u32("sh_info", Fmt::Dec),
    F::u64("sh_addralign", Fmt::Hex),
    F::u64("sh_entsize", Fmt::Size),
];

const SHDR32: &[F] = &[
    F::u32("sh_name", Fmt::StrIndex),
    F::u32("sh_type", Fmt::CtxName(sht)),
    F::u32("sh_flags", Fmt::Custom(shf)),
    F::u32("sh_addr", Fmt::Addr),
    F::u32("sh_offset", Fmt::Off),
    F::u32("sh_size", Fmt::Size),
    F::u32("sh_link", Fmt::Dec),
    F::u32("sh_info", Fmt::Dec),
    F::u32("sh_addralign", Fmt::Hex),
    F::u32("sh_entsize", Fmt::Size),
];

const SYM64: &[F] = &[
    F::u32("st_name", Fmt::StrIndex),
    F::u8("st_info", Fmt::Custom(st_info)),
    F::u8("st_other", Fmt::Custom(st_other)),
    F::u16("st_shndx", Fmt::Custom(shndx)),
    F::u64("st_value", Fmt::Addr),
    F::u64("st_size", Fmt::Size),
];

const SYM32: &[F] = &[
    F::u32("st_name", Fmt::StrIndex),
    F::u32("st_value", Fmt::Addr),
    F::u32("st_size", Fmt::Size),
    F::u8("st_info", Fmt::Custom(st_info)),
    F::u8("st_other", Fmt::Custom(st_other)),
    F::u16("st_shndx", Fmt::Custom(shndx)),
];

const RELA64: &[F] = &[
    F::u64("r_offset", Fmt::Addr),
    F::u64("r_info", Fmt::Hex),
    F::i64("r_addend"),
];
const REL64: &[F] = &[F::u64("r_offset", Fmt::Addr), F::u64("r_info", Fmt::Hex)];
const RELA32: &[F] = &[
    F::u32("r_offset", Fmt::Addr),
    F::u32("r_info", Fmt::Hex),
    F::i32("r_addend"),
];
const REL32: &[F] = &[F::u32("r_offset", Fmt::Addr), F::u32("r_info", Fmt::Hex)];

const DYN64: &[F] = &[F::i64("d_tag"), F::u64("d_val", Fmt::Hex)];
const DYN32: &[F] = &[F::i32("d_tag"), F::u32("d_val", Fmt::Hex)];

const CHDR64: &[F] = &[
    F::u32("ch_type", Fmt::Name(ch_type)),
    F::u32("ch_reserved", Fmt::Dec),
    F::u64("ch_size", Fmt::Size),
    F::u64("ch_addralign", Fmt::Hex),
];
const CHDR32: &[F] = &[
    F::u32("ch_type", Fmt::Name(ch_type)),
    F::u32("ch_size", Fmt::Size),
    F::u32("ch_addralign", Fmt::Hex),
];
fn ch_type(v: u64) -> Option<&'static str> {
    elf::CompressionType(v as u32).name()
}

struct Shdr {
    name: String,
    sh_type: u32,
    flags: u64,
    addr: u64,
    offset: u64,
    size: u64,
    link: u32,
    align: u64,
    entsize: u64,
}

fn read_shdrs(ctx: &Ctx, shoff: u64, shnum: u64, shentsize: u64) -> Vec<Shdr> {
    let b = &ctx.bytes;
    let mut out = Vec::new();
    for i in 0..shnum.min(1 << 20) {
        let o = shoff + i * shentsize;
        let s = if ctx.is64 {
            (|| {
                Some(Shdr {
                    name: String::new(),
                    sh_type: b.u32(o + 4)?,
                    flags: b.u64(o + 8)?,
                    addr: b.u64(o + 16)?,
                    offset: b.u64(o + 24)?,
                    size: b.u64(o + 32)?,
                    link: b.u32(o + 40)?,
                    align: b.u64(o + 48)?,
                    entsize: b.u64(o + 56)?,
                })
            })()
        } else {
            (|| {
                Some(Shdr {
                    name: String::new(),
                    sh_type: b.u32(o + 4)?,
                    flags: b.u32(o + 8)? as u64,
                    addr: b.u32(o + 12)? as u64,
                    offset: b.u32(o + 16)? as u64,
                    size: b.u32(o + 20)? as u64,
                    link: b.u32(o + 24)?,
                    align: b.u32(o + 32)? as u64,
                    entsize: b.u32(o + 36)? as u64,
                })
            })()
        };
        match s {
            Some(s) => out.push(s),
            None => break,
        }
    }
    out
}

pub(crate) fn build(b: &mut Builder) {
    let ctx = b.ctx;
    let is64 = ctx.is64;
    let (hdr, hv) = b.struct_region(0, RegionKind::Header, "ELF header", if is64 { EHDR64 } else { EHDR32 });
    b.set_note(
        hdr,
        "Identifies the file as ELF and locates the program and section header tables",
    );
    let phoff = fields::get(&hv, "e_phoff");
    let mut phnum = fields::get(&hv, "e_phnum");
    let phentsize = fields::get(&hv, "e_phentsize");
    let shoff = fields::get(&hv, "e_shoff");
    let mut shnum = fields::get(&hv, "e_shnum");
    let shentsize = fields::get(&hv, "e_shentsize");
    let mut shstrndx = fields::get(&hv, "e_shstrndx");

    // Extended numbering: real counts live in section header 0.
    if shoff != 0 && shentsize != 0 {
        let s0 = read_shdrs(&ctx, shoff, 1, shentsize);
        if let Some(s0) = s0.first() {
            if shnum == 0 {
                shnum = s0.size;
            }
            if shstrndx == elf::SHN_XINDEX.0 as u64 {
                shstrndx = s0.link as u64;
            }
            if phnum == elf::PN_XNUM as u64 {
                let info_off = shoff + if is64 { 44 } else { 28 };
                phnum = ctx.bytes.u32(info_off).unwrap_or(0) as u64;
            }
        }
    }

    // Program headers: few, so decode them eagerly.
    if phoff != 0 && phnum != 0 && phentsize != 0 {
        let table = b.region(phoff, phnum * phentsize, RegionKind::Metadata, "Program header table");
        b.set_value(table, format!("{phnum} entries × {phentsize} bytes"));
        b.set_note(table, "Describes the segments the loader maps into memory");
        if let Some(table) = table {
            for i in 0..phnum.min(4096) {
                let off = phoff + i * phentsize;
                let (id, v) = b.struct_child(
                    table,
                    off,
                    RegionKind::Metadata,
                    format!("Program header {i}"),
                    if is64 { PHDR64 } else { PHDR32 },
                );
                let ptype = fields::get(&v, "p_type");
                let name = pt_name(&ctx, ptype).unwrap_or_else(|| hex(ptype));
                b.set_value(
                    id,
                    format!(
                        "{name} {} {}..{}",
                        perms(fields::get(&v, "p_flags") as u32),
                        hex(fields::get(&v, "p_vaddr")),
                        hex(fields::get(&v, "p_vaddr") + fields::get(&v, "p_memsz"))
                    ),
                );
            }
        }
    }

    let mut shdrs = if shoff != 0 && shentsize != 0 {
        read_shdrs(&ctx, shoff, shnum, shentsize)
    } else {
        Vec::new()
    };
    let shstrtab = shdrs.get(shstrndx as usize).map(|s| (s.offset, s.size));
    let names_ctx = ctx.with_strtab(shstrtab);
    for (i, s) in shdrs.iter_mut().enumerate() {
        let o = shoff + i as u64 * shentsize;
        let idx = ctx.bytes.u32(o).unwrap_or(0) as u64;
        s.name = names_ctx.string_at(idx).map(util::lossy).unwrap_or_default();
    }

    if shoff != 0 && shnum != 0 && shentsize != 0 {
        let table = b.region(shoff, shnum * shentsize, RegionKind::Metadata, "Section header table");
        b.set_value(table, format!("{shnum} entries × {shentsize} bytes"));
        b.set_note(
            table,
            "Describes every section: name, type, flags, address and file range",
        );
        let mut t = Table::new(TableKind::ElfSectionHeaders, shentsize);
        t.strtab = shstrtab;
        b.set_decoder(table, Decoder::Table(t));
    }

    let symtab_info = |link: u32| -> Option<LinkedTable> {
        let s = shdrs.get(link as usize)?;
        let strtab = shdrs.get(s.link as usize).map(|t| (t.offset, t.size));
        let entry_size = if s.entsize != 0 {
            s.entsize
        } else if is64 {
            24
        } else {
            16
        };
        Some(LinkedTable {
            offset: s.offset,
            entry_size,
            count: s.size / entry_size,
            strtab,
        })
    };

    for (i, s) in shdrs.iter().enumerate().skip(1) {
        if s.sh_type == elf::SHT_NOBITS.0 || s.size == 0 {
            continue;
        }
        let kind = classify(&s.name, s.sh_type, s.flags);
        let id = b.region(s.offset, s.size, kind, format!("Section {}", s.name));
        let Some(id) = id else { continue };
        b.set_section(Some(id), (i - 1) as u32);
        let type_name = sht(&ctx, s.sh_type as u64).unwrap_or_else(|| hex(s.sh_type as u64));
        b.set_value(Some(id), type_name);

        if s.flags & elf::SHF_COMPRESSED.0 != 0 {
            let chdr = if is64 { CHDR64 } else { CHDR32 };
            b.struct_child(id, s.offset, kind, "Compression header", chdr);
            let hsize = fields::struct_size(chdr);
            let body = b.child(
                id,
                s.offset + hsize,
                s.size.saturating_sub(hsize),
                kind,
                "Compressed data",
            );
            b.set_note(body, "zlib/zstd stream; decompressed on demand for DWARF parsing");
            continue;
        }

        let linked_strtab = shdrs.get(s.link as usize).map(|t| (t.offset, t.size));
        let decoder = match elf::SectionType(s.sh_type) {
            elf::SHT_SYMTAB | elf::SHT_DYNSYM => {
                let mut t = Table::new(
                    TableKind::ElfSymbols,
                    if s.entsize != 0 {
                        s.entsize
                    } else if is64 {
                        24
                    } else {
                        16
                    },
                );
                t.strtab = linked_strtab;
                Some(Decoder::Table(t))
            }
            elf::SHT_STRTAB => Some(Decoder::Strings { skip: 0 }),
            elf::SHT_RELA | elf::SHT_REL => {
                let rela = s.sh_type == elf::SHT_RELA.0;
                let default = match (rela, is64) {
                    (true, true) => 24,
                    (false, true) => 16,
                    (true, false) => 12,
                    (false, false) => 8,
                };
                let mut t = Table::new(
                    if rela { TableKind::ElfRela } else { TableKind::ElfRel },
                    if s.entsize != 0 { s.entsize } else { default },
                );
                t.link = symtab_info(s.link);
                Some(Decoder::Table(t))
            }
            elf::SHT_DYNAMIC => {
                let mut t = Table::new(
                    TableKind::ElfDynamic,
                    if s.entsize != 0 {
                        s.entsize
                    } else if is64 {
                        16
                    } else {
                        8
                    },
                );
                t.strtab = linked_strtab;
                Some(Decoder::Table(t))
            }
            elf::SHT_NOTE => Some(Decoder::ElfNotes { align: s.align.max(4) }),
            elf::SHT_INIT_ARRAY | elf::SHT_FINI_ARRAY | elf::SHT_PREINIT_ARRAY => {
                let mut t = Table::new(TableKind::Pointers, ctx.addr_size() as u64);
                t.param = s.addr;
                Some(Decoder::Table(t))
            }
            elf::SHT_GNU_VERSYM => Some(Decoder::Table(Table::new(TableKind::ElfVersym, 2))),
            _ if s.name == ".got" || s.name == ".got.plt" => {
                let mut t = Table::new(TableKind::Pointers, ctx.addr_size() as u64);
                t.param = s.addr;
                Some(Decoder::Table(t))
            }
            _ if Decoder::for_dwarf_section(&s.name).is_some() => Decoder::for_dwarf_section(&s.name),
            _ if s.flags & elf::SHF_STRINGS.0 != 0 && s.entsize <= 1 => Some(Decoder::Strings { skip: 0 }),
            _ if s.name == ".comment" => Some(Decoder::Strings { skip: 0 }),
            _ if s.flags & elf::SHF_ALLOC.0 != 0
                && s.addr != 0
                && s.flags & elf::SHF_TLS.0 == 0
                && matches!(kind, RegionKind::Code | RegionKind::Data | RegionKind::Rodata) =>
            {
                Some(Decoder::Symbols { address: s.addr })
            }
            _ => None,
        };
        if let Some(d) = decoder {
            b.set_decoder(Some(id), d);
        }
    }
}

/// Classifies a section by type, flags and well-known names.
pub(crate) fn classify(name: &str, sh_type: u32, flags: u64) -> RegionKind {
    let t = elf::SectionType(sh_type);
    let has = |f: elf::SectionFlags| flags & f.0 != 0;
    match t {
        elf::SHT_NULL => RegionKind::Unknown,
        elf::SHT_SYMTAB | elf::SHT_DYNSYM | elf::SHT_SYMTAB_SHNDX => RegionKind::Symbols,
        elf::SHT_STRTAB => RegionKind::Strings,
        elf::SHT_RELA | elf::SHT_REL | elf::SHT_RELR => RegionKind::Relocations,
        elf::SHT_DYNAMIC
        | elf::SHT_HASH
        | elf::SHT_GNU_HASH
        | elf::SHT_GNU_VERSYM
        | elf::SHT_GNU_VERDEF
        | elf::SHT_GNU_VERNEED => RegionKind::Linking,
        elf::SHT_NOTE => RegionKind::Notes,
        elf::SHT_NOBITS => {
            if has(elf::SHF_TLS) {
                RegionKind::Tls
            } else {
                RegionKind::Bss
            }
        }
        elf::SHT_GROUP => RegionKind::Metadata,
        elf::SHT_INIT_ARRAY | elf::SHT_FINI_ARRAY | elf::SHT_PREINIT_ARRAY => RegionKind::Data,
        _ if sh_type == elf::SHT_X86_64_UNWIND.0 && name.contains("eh_frame") => RegionKind::Unwind,
        _ => classify_by_name(name).unwrap_or(if has(elf::SHF_EXECINSTR) {
            RegionKind::Code
        } else if has(elf::SHF_TLS) {
            RegionKind::Tls
        } else if has(elf::SHF_ALLOC) {
            if has(elf::SHF_WRITE) {
                RegionKind::Data
            } else {
                RegionKind::Rodata
            }
        } else {
            RegionKind::Metadata
        }),
    }
}

/// Well-known section names shared by all formats (Mach-O names are passed
/// without the segment, PE names as-is).
pub(crate) fn classify_by_name(name: &str) -> Option<RegionKind> {
    let n = name.trim_start_matches('.').trim_start_matches("__");
    Some(
        if name.starts_with(".debug")
            || name.starts_with(".zdebug")
            || name.starts_with("__debug")
            || name.starts_with(".stab")
            || name == ".gdb_index"
            || name.starts_with("__apple_")
            || name == ".gnu_debuglink"
            || name == ".gnu_debugaltlink"
        {
            RegionKind::Debug
        } else if matches!(
            n,
            "eh_frame"
                | "eh_frame_hdr"
                | "gcc_except_table"
                | "ARM.exidx"
                | "ARM.extab"
                | "unwind_info"
                | "pdata"
                | "xdata"
                | "compact_unwind"
        ) {
            RegionKind::Unwind
        } else if matches!(
            n,
            "interp"
                | "got"
                | "got.plt"
                | "plt.got"
                | "idata"
                | "edata"
                | "la_symbol_ptr"
                | "nl_symbol_ptr"
                | "dynamic"
        ) {
            RegionKind::Linking
        } else if name == ".comment" || name.starts_with(".note") || name == ".ident" {
            RegionKind::Notes
        } else if matches!(n, "rsrc") {
            RegionKind::Resources
        } else if matches!(n, "reloc") {
            RegionKind::Relocations
        } else {
            return None;
        },
    )
}

pub(crate) fn table_entry(ctx: &Ctx, t: &Table, index: u64, offset: u64) -> Entry {
    let is64 = ctx.is64;
    match t.kind {
        TableKind::ElfSectionHeaders => {
            let c = ctx.with_strtab(t.strtab);
            let f = fields::decode(&c, offset, if is64 { SHDR64 } else { SHDR32 });
            let name = c
                .string_at(fields::get(&f, "sh_name"))
                .map(util::lossy)
                .unwrap_or_default();
            let ty = sht(ctx, fields::get(&f, "sh_type")).unwrap_or_default();
            Entry {
                name: format!(
                    "Section header {index}{}",
                    if name.is_empty() {
                        String::new()
                    } else {
                        format!(": {name}")
                    }
                ),
                value: Some(ty),
                fields: f,
                ..Entry::default()
            }
        }
        TableKind::ElfSymbols => {
            let c = ctx.with_strtab(t.strtab);
            let f = fields::decode(&c, offset, if is64 { SYM64 } else { SYM32 });
            let raw_name = c
                .string_at(fields::get(&f, "st_name"))
                .map(util::lossy)
                .unwrap_or_default();
            let info = elf::SymbolInfo(fields::get(&f, "st_info") as u8);
            let n = names(ctx);
            let typ = n.stt.name(info.st_type()).unwrap_or("?");
            let bind = n.stb.name(info.st_bind()).unwrap_or("?");
            let display = util::demangle(&raw_name).unwrap_or_else(|| raw_name.clone());
            Entry {
                name: format!(
                    "Symbol {index}: {}",
                    if display.is_empty() { "(unnamed)" } else { &display }
                ),
                value: Some(format!("{typ} {bind} {}", hex(fields::get(&f, "st_value")))),
                note: (display != raw_name).then(|| format!("mangled: {raw_name}")),
                fields: f,
                ..Entry::default()
            }
        }
        TableKind::ElfRel | TableKind::ElfRela => {
            let rela = t.kind == TableKind::ElfRela;
            let specs = match (rela, is64) {
                (true, true) => RELA64,
                (false, true) => REL64,
                (true, false) => RELA32,
                (false, false) => REL32,
            };
            let mut f = fields::decode(ctx, offset, specs);
            let info = fields::get(&f, "r_info");
            let (sym, typ) = if is64 {
                (info >> 32, info & 0xffff_ffff)
            } else {
                (info >> 8, info & 0xff)
            };
            let type_name = names(ctx)
                .r
                .name(elf::RelocationType(typ as u32))
                .map_or_else(|| format!("type {typ}"), str::to_string);
            let sym_name = t.link.and_then(|l| linked_symbol_name(ctx, &l, sym));
            if let Some(field) = f.iter_mut().find(|f| f.name == "r_info") {
                field.value = format!(
                    "{} (sym {sym}{}, {type_name})",
                    hex(info),
                    sym_name.as_ref().map(|n| format!(" {n}")).unwrap_or_default()
                );
            }
            let target = fields::get(&f, "r_offset");
            Entry {
                name: format!("Relocation {index}: {type_name}"),
                value: Some(match &sym_name {
                    Some(n) if !n.is_empty() => format!("{} → {n}", hex(target)),
                    _ => hex(target),
                }),
                fields: f,
                ..Entry::default()
            }
        }
        TableKind::ElfDynamic => {
            let c = ctx.with_strtab(t.strtab);
            let mut f = fields::decode(&c, offset, if is64 { DYN64 } else { DYN32 });
            let tag = fields::get(&f, "d_tag") as i64;
            let val = fields::get(&f, "d_val");
            let tag_name = names(ctx)
                .dt
                .name(elf::DynamicTag(tag))
                .map_or_else(|| format!("{tag:#x}"), str::to_string);
            let rendered = match elf::DynamicTag(tag) {
                elf::DT_NEEDED | elf::DT_SONAME | elf::DT_RPATH | elf::DT_RUNPATH => {
                    c.string_at(val).map_or_else(|| hex(val), util::quote)
                }
                elf::DT_FLAGS => flag_string(elf::DynamicFlags::NAMES, elf::DynamicFlags(val)),
                elf::DT_FLAGS_1 => flag_string(elf::DynamicFlags1::NAMES, elf::DynamicFlags1(val)),
                elf::DT_PLTREL => names(ctx)
                    .dt
                    .name(elf::DynamicTag(val as i64))
                    .unwrap_or("?")
                    .to_string(),
                elf::DT_NULL => String::new(),
                _ => hex(val),
            };
            if let Some(field) = f.iter_mut().find(|f| f.name == "d_tag") {
                field.value = format!("{tag_name} ({tag})");
            }
            if let Some(field) = f.iter_mut().find(|f| f.name == "d_val") {
                field.value = rendered.clone();
            }
            Entry {
                name: format!("Dynamic {index}: {tag_name}"),
                value: Some(rendered),
                fields: f,
                ..Entry::default()
            }
        }
        TableKind::ElfVersym => {
            let v = ctx.bytes.u16(offset).unwrap_or(0);
            let idx = v & 0x7fff;
            let desc = match idx {
                0 => "local".to_string(),
                1 => "global".to_string(),
                n => format!("version {n}"),
            };
            let hidden = if v & 0x8000 != 0 { " (hidden)" } else { "" };
            Entry {
                name: format!("Version index {index}"),
                value: Some(format!("{desc}{hidden}")),
                fields: vec![FieldValue {
                    start: offset,
                    end: offset + 2,
                    name: "vs_index",
                    raw: v as u64,
                    value: format!("{v}"),
                }],
                ..Entry::default()
            }
        }
        _ => Entry::default(),
    }
}

fn linked_symbol_name(ctx: &Ctx, l: &LinkedTable, sym: u64) -> Option<String> {
    if sym == 0 || sym >= l.count {
        return None;
    }
    let off = l.offset + sym * l.entry_size;
    let name_idx = ctx.bytes.u32(off)? as u64;
    let c = ctx.with_strtab(l.strtab);
    let raw = util::lossy(c.string_at(name_idx)?);
    if raw.is_empty() {
        // Section symbols have no name; show the section instead.
        let shndx_off = if ctx.is64 { off + 6 } else { off + 14 };
        let shndx = ctx.bytes.u16(shndx_off)? as usize;
        return ctx.sections.get(shndx.wrapping_sub(1)).map(|s| s.name.clone());
    }
    Some(util::demangle(&raw).unwrap_or(raw))
}

pub(crate) fn note_entry(ctx: &Ctx, pos: u64, end: u64, align: u64) -> Option<Entry> {
    let b = &ctx.bytes;
    let namesz = b.u32(pos)? as u64;
    let descsz = b.u32(pos + 4)? as u64;
    let ntype = b.u32(pos + 8)?;
    let name_off = pos + 12;
    let desc_off = name_off + util::align_up(namesz, align.clamp(4, 8));
    let next = desc_off + util::align_up(descsz, align.clamp(4, 8));
    if next > end || desc_off > end {
        return None;
    }
    let name_bytes = b.slice(name_off, namesz)?;
    let name = util::fixed_str(name_bytes);
    let desc = b.slice(desc_off, descsz)?;
    let type_name = elf::NoteType::names(name.as_bytes())
        .name(elf::NoteType(ntype))
        .map_or_else(|| format!("type {ntype}"), str::to_string);
    let rendered = match (name.as_str(), elf::NoteType(ntype)) {
        ("GNU", elf::NT_GNU_BUILD_ID) => util::hex_compact(desc),
        ("GNU", elf::NT_GNU_ABI_TAG) if desc.len() >= 16 => {
            let w = |i: u64| b.u32(desc_off + i).unwrap_or(0);
            let os = match w(0) {
                0 => "Linux",
                1 => "Hurd",
                2 => "Solaris",
                3 => "FreeBSD",
                _ => "?",
            };
            format!("{os} {}.{}.{}", w(4), w(8), w(12))
        }
        _ if desc.iter().all(|&c| c == 0 || (0x20..0x7f).contains(&c)) && !desc.is_empty() => {
            util::quote(&util::fixed_str(desc).into_bytes())
        }
        _ => util::hex_bytes(&desc[..desc.len().min(32)]),
    };
    let fields = vec![
        FieldValue {
            start: pos,
            end: pos + 4,
            name: "n_namesz",
            raw: namesz,
            value: namesz.to_string(),
        },
        FieldValue {
            start: pos + 4,
            end: pos + 8,
            name: "n_descsz",
            raw: descsz,
            value: descsz.to_string(),
        },
        FieldValue {
            start: pos + 8,
            end: pos + 12,
            name: "n_type",
            raw: ntype as u64,
            value: format!("{type_name} ({ntype})"),
        },
        FieldValue {
            start: name_off,
            end: desc_off,
            name: "n_name",
            raw: 0,
            value: util::quote(name.as_bytes()),
        },
        FieldValue {
            start: desc_off,
            end: next,
            name: "n_desc",
            raw: 0,
            value: rendered.clone(),
        },
    ];
    Some(Entry {
        start: pos,
        end: next,
        name: format!("Note {name} {type_name}"),
        value: Some(rendered),
        fields,
        ..Entry::default()
    })
}
