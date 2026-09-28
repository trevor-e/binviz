//! Mach-O file layout.

use object::macho;

use super::decode::{Decoder, Entry, LinkedTable, Table, TableKind};
use super::elf::flag_string;
use super::fields::{self, F, FieldValue, Fmt};
use super::{Builder, Ctx, Machine};
use crate::model::RegionKind;
use crate::util::{self, hex};

fn cputype(ctx: &Ctx) -> macho::CpuType {
    match ctx.machine {
        Machine::MachO(c) => macho::CpuType(c),
        _ => macho::CpuType(0),
    }
}

fn magic(v: u64) -> Option<&'static str> {
    match v as u32 {
        macho::MH_MAGIC => Some("MH_MAGIC"),
        macho::MH_MAGIC_64 => Some("MH_MAGIC_64"),
        macho::MH_CIGAM => Some("MH_CIGAM"),
        macho::MH_CIGAM_64 => Some("MH_CIGAM_64"),
        _ => None,
    }
}
fn cpu(v: u64) -> Option<&'static str> {
    macho::CpuType(v as u32).name()
}
fn cpusub(ctx: &Ctx, v: u64) -> String {
    let names = macho::machine_names(cputype(ctx));
    format!(
        "{} ({})",
        flag_string(names.cpusubtype, macho::CpuSubtype(v as u32)),
        hex(v)
    )
}
fn filetype(v: u64) -> Option<&'static str> {
    macho::FileType(v as u32).name()
}
fn mh_flags(v: u64) -> String {
    flag_string(macho::FileFlags::NAMES, macho::FileFlags(v as u32))
}
fn lc(v: u64) -> Option<&'static str> {
    macho::LoadCommandType(v as u32).name()
}
fn vmprot(v: u64) -> String {
    format!("{} ({})", prot(v as u32), hex(v))
}
fn sg_flags(v: u64) -> String {
    flag_string(macho::SegmentFlags::NAMES, macho::SegmentFlags(v as u32))
}
fn s_flags(v: u64) -> String {
    format!(
        "{} ({})",
        flag_string(macho::SectionFlags::NAMES, macho::SectionFlags(v as u32)),
        hex(v)
    )
}
fn platform(v: u64) -> Option<&'static str> {
    macho::Platform(v as u32).name()
}
fn tool(v: u64) -> Option<&'static str> {
    macho::Tool(v as u32).name()
}
fn dice(v: u64) -> Option<&'static str> {
    macho::DiceKind(v as u16).name()
}
fn source_version(_: &Ctx, v: u64) -> String {
    let parts = [
        v >> 40,
        (v >> 30) & 0x3ff,
        (v >> 20) & 0x3ff,
        (v >> 10) & 0x3ff,
        v & 0x3ff,
    ];
    let mut end = parts.len();
    while end > 2 && parts[end - 1] == 0 {
        end -= 1;
    }
    parts[..end].iter().map(u64::to_string).collect::<Vec<_>>().join(".")
}
fn n_type(v: u64) -> String {
    format!(
        "{} ({})",
        flag_string(macho::SymbolFlags::NAMES, macho::SymbolFlags(v as u8)),
        hex(v)
    )
}
fn n_sect(ctx: &Ctx, v: u64) -> String {
    if v == 0 {
        return "0 (NO_SECT)".into();
    }
    match ctx.sections.get(v as usize - 1) {
        Some(s) => format!("{v} ({},{})", s.segment_name.as_deref().unwrap_or(""), s.name),
        None => v.to_string(),
    }
}

pub(crate) fn prot(p: u32) -> String {
    format!(
        "{}{}{}",
        if p & macho::VM_PROT_READ.0 != 0 { 'r' } else { '-' },
        if p & macho::VM_PROT_WRITE.0 != 0 { 'w' } else { '-' },
        if p & macho::VM_PROT_EXECUTE.0 != 0 { 'x' } else { '-' }
    )
}

const MH64: &[F] = &[
    F::u32("magic", Fmt::Name(magic)),
    F::u32("cputype", Fmt::Name(cpu)),
    F::u32("cpusubtype", Fmt::Custom(cpusub)),
    F::u32("filetype", Fmt::Name(filetype)),
    F::u32("ncmds", Fmt::Dec),
    F::u32("sizeofcmds", Fmt::Size),
    F::u32("flags", Fmt::Flags(mh_flags)),
    F::u32("reserved", Fmt::Hex),
];
const MH32: &[F] = &[
    F::u32("magic", Fmt::Name(magic)),
    F::u32("cputype", Fmt::Name(cpu)),
    F::u32("cpusubtype", Fmt::Custom(cpusub)),
    F::u32("filetype", Fmt::Name(filetype)),
    F::u32("ncmds", Fmt::Dec),
    F::u32("sizeofcmds", Fmt::Size),
    F::u32("flags", Fmt::Flags(mh_flags)),
];

const CMD: [F; 2] = [F::u32("cmd", Fmt::Name(lc)), F::u32("cmdsize", Fmt::Size)];

const SEGMENT64: &[F] = &[
    CMD[0],
    CMD[1],
    F::str("segname", 16),
    F::u64("vmaddr", Fmt::Addr),
    F::u64("vmsize", Fmt::Size),
    F::u64("fileoff", Fmt::Off),
    F::u64("filesize", Fmt::Size),
    F::u32("maxprot", Fmt::Flags(vmprot)),
    F::u32("initprot", Fmt::Flags(vmprot)),
    F::u32("nsects", Fmt::Dec),
    F::u32("flags", Fmt::Flags(sg_flags)),
];
const SEGMENT32: &[F] = &[
    CMD[0],
    CMD[1],
    F::str("segname", 16),
    F::u32("vmaddr", Fmt::Addr),
    F::u32("vmsize", Fmt::Size),
    F::u32("fileoff", Fmt::Off),
    F::u32("filesize", Fmt::Size),
    F::u32("maxprot", Fmt::Flags(vmprot)),
    F::u32("initprot", Fmt::Flags(vmprot)),
    F::u32("nsects", Fmt::Dec),
    F::u32("flags", Fmt::Flags(sg_flags)),
];
const SECTION64: &[F] = &[
    F::str("sectname", 16),
    F::str("segname", 16),
    F::u64("addr", Fmt::Addr),
    F::u64("size", Fmt::Size),
    F::u32("offset", Fmt::Off),
    F::u32("align", Fmt::Custom(align_pow)),
    F::u32("reloff", Fmt::Off),
    F::u32("nreloc", Fmt::Dec),
    F::u32("flags", Fmt::Custom(|_, v| s_flags(v))),
    F::u32("reserved1", Fmt::Dec),
    F::u32("reserved2", Fmt::Dec),
    F::u32("reserved3", Fmt::Dec),
];
const SECTION32: &[F] = &[
    F::str("sectname", 16),
    F::str("segname", 16),
    F::u32("addr", Fmt::Addr),
    F::u32("size", Fmt::Size),
    F::u32("offset", Fmt::Off),
    F::u32("align", Fmt::Custom(align_pow)),
    F::u32("reloff", Fmt::Off),
    F::u32("nreloc", Fmt::Dec),
    F::u32("flags", Fmt::Custom(|_, v| s_flags(v))),
    F::u32("reserved1", Fmt::Dec),
    F::u32("reserved2", Fmt::Dec),
];
fn align_pow(_: &Ctx, v: u64) -> String {
    if v < 64 {
        format!("2^{v} = {}", 1u64 << v)
    } else {
        v.to_string()
    }
}

const SYMTAB: &[F] = &[
    CMD[0],
    CMD[1],
    F::u32("symoff", Fmt::Off),
    F::u32("nsyms", Fmt::Dec),
    F::u32("stroff", Fmt::Off),
    F::u32("strsize", Fmt::Size),
];
const DYSYMTAB: &[F] = &[
    CMD[0],
    CMD[1],
    F::u32("ilocalsym", Fmt::Dec),
    F::u32("nlocalsym", Fmt::Dec),
    F::u32("iextdefsym", Fmt::Dec),
    F::u32("nextdefsym", Fmt::Dec),
    F::u32("iundefsym", Fmt::Dec),
    F::u32("nundefsym", Fmt::Dec),
    F::u32("tocoff", Fmt::Off),
    F::u32("ntoc", Fmt::Dec),
    F::u32("modtaboff", Fmt::Off),
    F::u32("nmodtab", Fmt::Dec),
    F::u32("extrefsymoff", Fmt::Off),
    F::u32("nextrefsyms", Fmt::Dec),
    F::u32("indirectsymoff", Fmt::Off),
    F::u32("nindirectsyms", Fmt::Dec),
    F::u32("extreloff", Fmt::Off),
    F::u32("nextrel", Fmt::Dec),
    F::u32("locreloff", Fmt::Off),
    F::u32("nlocrel", Fmt::Dec),
];
const DYLIB: &[F] = &[
    CMD[0],
    CMD[1],
    F::u32("name.offset", Fmt::Dec),
    F::u32("timestamp", Fmt::Time),
    F::u32("current_version", Fmt::MachVersion),
    F::u32("compatibility_version", Fmt::MachVersion),
];
const STR_CMD: &[F] = &[CMD[0], CMD[1], F::u32("name.offset", Fmt::Dec)];
const UUID: &[F] = &[CMD[0], CMD[1], F::bytes("uuid", 16)];
const LINKEDIT_DATA: &[F] = &[
    CMD[0],
    CMD[1],
    F::u32("dataoff", Fmt::Off),
    F::u32("datasize", Fmt::Size),
];
const DYLD_INFO: &[F] = &[
    CMD[0],
    CMD[1],
    F::u32("rebase_off", Fmt::Off),
    F::u32("rebase_size", Fmt::Size),
    F::u32("bind_off", Fmt::Off),
    F::u32("bind_size", Fmt::Size),
    F::u32("weak_bind_off", Fmt::Off),
    F::u32("weak_bind_size", Fmt::Size),
    F::u32("lazy_bind_off", Fmt::Off),
    F::u32("lazy_bind_size", Fmt::Size),
    F::u32("export_off", Fmt::Off),
    F::u32("export_size", Fmt::Size),
];
const MAIN: &[F] = &[
    CMD[0],
    CMD[1],
    F::u64("entryoff", Fmt::Off),
    F::u64("stacksize", Fmt::Size),
];
const SOURCE_VERSION: &[F] = &[CMD[0], CMD[1], F::u64("version", Fmt::Custom(source_version))];
const BUILD_VERSION: &[F] = &[
    CMD[0],
    CMD[1],
    F::u32("platform", Fmt::Name(platform)),
    F::u32("minos", Fmt::MachVersion),
    F::u32("sdk", Fmt::MachVersion),
    F::u32("ntools", Fmt::Dec),
];
const BUILD_TOOL: &[F] = &[F::u32("tool", Fmt::Name(tool)), F::u32("version", Fmt::MachVersion)];
const VERSION_MIN: &[F] = &[
    CMD[0],
    CMD[1],
    F::u32("version", Fmt::MachVersion),
    F::u32("sdk", Fmt::MachVersion),
];
const ENCRYPTION32: &[F] = &[
    CMD[0],
    CMD[1],
    F::u32("cryptoff", Fmt::Off),
    F::u32("cryptsize", Fmt::Size),
    F::u32("cryptid", Fmt::Dec),
];
const ENCRYPTION64: &[F] = &[
    CMD[0],
    CMD[1],
    F::u32("cryptoff", Fmt::Off),
    F::u32("cryptsize", Fmt::Size),
    F::u32("cryptid", Fmt::Dec),
    F::u32("pad", Fmt::Hex),
];
const LINKER_OPTION: &[F] = &[CMD[0], CMD[1], F::u32("count", Fmt::Dec)];
const NOTE: &[F] = &[
    CMD[0],
    CMD[1],
    F::str("data_owner", 16),
    F::u64("offset", Fmt::Off),
    F::u64("size", Fmt::Size),
];
const THREAD: &[F] = &[CMD[0], CMD[1], F::u32("flavor", Fmt::Dec), F::u32("count", Fmt::Dec)];
const FILESET_ENTRY: &[F] = &[
    CMD[0],
    CMD[1],
    F::u64("vmaddr", Fmt::Addr),
    F::u64("fileoff", Fmt::Off),
    F::u32("entry_id.offset", Fmt::Dec),
    F::u32("reserved", Fmt::Hex),
];

const NLIST64: &[F] = &[
    F::u32("n_strx", Fmt::StrIndex),
    F::u8("n_type", Fmt::Flags(n_type)),
    F::u8("n_sect", Fmt::Custom(n_sect)),
    F::u16("n_desc", Fmt::Hex),
    F::u64("n_value", Fmt::Addr),
];
const NLIST32: &[F] = &[
    F::u32("n_strx", Fmt::StrIndex),
    F::u8("n_type", Fmt::Flags(n_type)),
    F::u8("n_sect", Fmt::Custom(n_sect)),
    F::u16("n_desc", Fmt::Hex),
    F::u32("n_value", Fmt::Addr),
];
const RELOC: &[F] = &[F::i32("r_address"), F::u32("r_info", Fmt::Hex)];
const DICE: &[F] = &[
    F::u32("offset", Fmt::Off),
    F::u16("length", Fmt::Size),
    F::u16("kind", Fmt::Name(dice)),
];

/// Segment kind by conventional name.
pub(crate) fn segment_kind(name: &str) -> RegionKind {
    match name {
        "__TEXT" | "__TEXT_EXEC" => RegionKind::Code,
        "__DATA_CONST" | "__AUTH_CONST" => RegionKind::Rodata,
        "__LINKEDIT" => RegionKind::Linking,
        "__DWARF" => RegionKind::Debug,
        "__PAGEZERO" => RegionKind::Bss,
        "" => RegionKind::Metadata,
        _ => RegionKind::Data,
    }
}

/// Section kind from name, segment and flags.
pub(crate) fn classify(segname: &str, sectname: &str, flags: u32) -> RegionKind {
    let typ = macho::SectionFlags(flags).typ();
    if let Some(k) = super::elf::classify_by_name(sectname) {
        return k;
    }
    if segname == "__DWARF" || flags & macho::S_ATTR_DEBUG.0 != 0 {
        return RegionKind::Debug;
    }
    match typ {
        macho::S_ZEROFILL | macho::S_GB_ZEROFILL => return RegionKind::Bss,
        macho::S_THREAD_LOCAL_ZEROFILL
        | macho::S_THREAD_LOCAL_REGULAR
        | macho::S_THREAD_LOCAL_VARIABLES
        | macho::S_THREAD_LOCAL_VARIABLE_POINTERS
        | macho::S_THREAD_LOCAL_INIT_FUNCTION_POINTERS => return RegionKind::Tls,
        macho::S_NON_LAZY_SYMBOL_POINTERS | macho::S_LAZY_SYMBOL_POINTERS | macho::S_LAZY_DYLIB_SYMBOL_POINTERS => {
            return RegionKind::Linking;
        }
        macho::S_SYMBOL_STUBS => return RegionKind::Code,
        _ => {}
    }
    if flags & (macho::S_ATTR_PURE_INSTRUCTIONS.0 | macho::S_ATTR_SOME_INSTRUCTIONS.0) != 0 {
        return RegionKind::Code;
    }
    match (segname, sectname) {
        (_, "__got" | "__auth_got" | "__la_symbol_ptr" | "__nl_symbol_ptr") => RegionKind::Linking,
        ("__LD", "__compact_unwind") => RegionKind::Unwind,
        ("__TEXT" | "__DATA_CONST" | "__AUTH_CONST", _) => RegionKind::Rodata,
        _ => RegionKind::Data,
    }
}

struct Cmd {
    offset: u64,
    cmd: u32,
    size: u64,
}

pub(crate) fn build(b: &mut Builder) {
    let ctx = b.ctx;
    let is64 = ctx.is64;
    let (hdr, hv) = b.struct_region(0, RegionKind::Header, "Mach-O header", if is64 { MH64 } else { MH32 });
    b.set_note(
        hdr,
        "Identifies the CPU and file type and counts the load commands that follow",
    );
    let hdr_size: u64 = if is64 { 32 } else { 28 };
    let ncmds = fields::get(&hv, "ncmds");
    let sizeofcmds = fields::get(&hv, "sizeofcmds");
    let filetype = fields::get(&hv, "filetype") as u32;

    let area = b.region(hdr_size, sizeofcmds, RegionKind::Metadata, "Load commands");
    b.set_value(area, format!("{ncmds} commands, {sizeofcmds} bytes"));
    b.set_note(
        area,
        "Tell the kernel and dyld how to map the file and what to link against",
    );

    let mut cmds = Vec::new();
    let mut off = hdr_size;
    for _ in 0..ncmds.min(65536) {
        let (Some(cmd), Some(size)) = (ctx.bytes.u32(off), ctx.bytes.u32(off + 4)) else {
            break;
        };
        if size < 8 || off + size as u64 > hdr_size + sizeofcmds {
            break;
        }
        cmds.push(Cmd {
            offset: off,
            cmd,
            size: size as u64,
        });
        off += size as u64;
    }

    let mut symtab: Option<LinkedTable> = None;
    let mut section_index = 0u32;
    let nlist_size = if is64 { 16 } else { 12 };
    let mut sections_with_relocs = Vec::new();

    for (i, c) in cmds.iter().enumerate() {
        let name = lc(c.cmd as u64).map_or_else(|| format!("Load command {:#x}", c.cmd), str::to_string);
        let Some(area) = area else { break };
        let Some(node) = b.child(area, c.offset, c.size, RegionKind::Metadata, format!("[{i}] {name}")) else {
            continue;
        };
        let cmd = macho::LoadCommandType(c.cmd);
        let specs: &[F] = match cmd {
            macho::LC_SEGMENT_64 => SEGMENT64,
            macho::LC_SEGMENT => SEGMENT32,
            macho::LC_SYMTAB => SYMTAB,
            macho::LC_DYSYMTAB => DYSYMTAB,
            macho::LC_LOAD_DYLIB
            | macho::LC_ID_DYLIB
            | macho::LC_LOAD_WEAK_DYLIB
            | macho::LC_REEXPORT_DYLIB
            | macho::LC_LAZY_LOAD_DYLIB
            | macho::LC_LOAD_UPWARD_DYLIB => DYLIB,
            macho::LC_LOAD_DYLINKER | macho::LC_ID_DYLINKER | macho::LC_DYLD_ENVIRONMENT | macho::LC_RPATH => STR_CMD,
            macho::LC_UUID => UUID,
            macho::LC_CODE_SIGNATURE
            | macho::LC_SEGMENT_SPLIT_INFO
            | macho::LC_FUNCTION_STARTS
            | macho::LC_DATA_IN_CODE
            | macho::LC_DYLIB_CODE_SIGN_DRS
            | macho::LC_LINKER_OPTIMIZATION_HINT
            | macho::LC_DYLD_EXPORTS_TRIE
            | macho::LC_DYLD_CHAINED_FIXUPS
            | macho::LC_ATOM_INFO => LINKEDIT_DATA,
            macho::LC_DYLD_INFO | macho::LC_DYLD_INFO_ONLY => DYLD_INFO,
            macho::LC_MAIN => MAIN,
            macho::LC_SOURCE_VERSION => SOURCE_VERSION,
            macho::LC_BUILD_VERSION => BUILD_VERSION,
            macho::LC_VERSION_MIN_MACOSX
            | macho::LC_VERSION_MIN_IPHONEOS
            | macho::LC_VERSION_MIN_TVOS
            | macho::LC_VERSION_MIN_WATCHOS => VERSION_MIN,
            macho::LC_ENCRYPTION_INFO => ENCRYPTION32,
            macho::LC_ENCRYPTION_INFO_64 => ENCRYPTION64,
            macho::LC_LINKER_OPTION => LINKER_OPTION,
            macho::LC_NOTE => NOTE,
            macho::LC_THREAD | macho::LC_UNIXTHREAD => THREAD,
            macho::LC_FILESET_ENTRY => FILESET_ENTRY,
            _ => &CMD,
        };
        let v = b.fields(node, c.offset, specs);
        let consumed = fields::struct_size(specs).min(c.size);
        let get = |n: &str| fields::get(&v, n);

        match cmd {
            macho::LC_SEGMENT_64 | macho::LC_SEGMENT => {
                let segname = util::fixed_str(ctx.bytes.slice(c.offset + 8, 16).unwrap_or(&[]));
                let (fileoff, filesize) = (get("fileoff"), get("filesize"));
                let (vmaddr, vmsize) = (get("vmaddr"), get("vmsize"));
                b.set_value(
                    Some(node),
                    format!(
                        "{} {} {}..{}",
                        if segname.is_empty() { "(unnamed)" } else { &segname },
                        prot(get("initprot") as u32),
                        hex(vmaddr),
                        hex(vmaddr + vmsize)
                    ),
                );
                let nsects = get("nsects");
                let sect_specs = if is64 { SECTION64 } else { SECTION32 };
                let sect_size = fields::struct_size(sect_specs);
                let mut sect_off = c.offset + consumed;
                for _ in 0..nsects.min(4096) {
                    if sect_off + sect_size > c.offset + c.size {
                        break;
                    }
                    let sectname = util::fixed_str(ctx.bytes.slice(sect_off, 16).unwrap_or(&[]));
                    // Object files put every section in one unnamed segment, but
                    // each section header still names its conventional segment.
                    let segname = util::fixed_str(ctx.bytes.slice(sect_off + 16, 16).unwrap_or(&[]));
                    let (_, sv) = b.struct_child(
                        node,
                        sect_off,
                        RegionKind::Metadata,
                        format!("Section header {segname},{sectname}"),
                        sect_specs,
                    );
                    let sget = |n: &str| fields::get(&sv, n);
                    let flags = sget("flags") as u32;
                    let typ = macho::SectionFlags(flags).typ();
                    let zerofill = matches!(
                        typ,
                        macho::S_ZEROFILL | macho::S_GB_ZEROFILL | macho::S_THREAD_LOCAL_ZEROFILL
                    );
                    let (addr, size, offset) = (sget("addr"), sget("size"), sget("offset"));
                    if !zerofill && offset != 0 && size != 0 {
                        let kind = classify(&segname, &sectname, flags);
                        let id = b.region(offset, size, kind, format!("Section {segname},{sectname}"));
                        b.set_section(id, section_index);
                        b.set_value(id, macho::SectionFlags(flags).typ().name().unwrap_or("").to_string());
                        let decoder = match typ {
                            macho::S_CSTRING_LITERALS => Some(Decoder::Strings { skip: 0 }),
                            macho::S_NON_LAZY_SYMBOL_POINTERS
                            | macho::S_LAZY_SYMBOL_POINTERS
                            | macho::S_MOD_INIT_FUNC_POINTERS
                            | macho::S_MOD_TERM_FUNC_POINTERS => {
                                let mut t = Table::new(TableKind::Pointers, ctx.addr_size() as u64);
                                t.param = addr;
                                Some(Decoder::Table(t))
                            }
                            _ if segname == "__DWARF" => Decoder::for_dwarf_section(&sectname),
                            _ => Some(Decoder::Symbols { address: addr }),
                        };
                        if let Some(d) = decoder {
                            b.set_decoder(id, d);
                        }
                    }
                    let (reloff, nreloc) = (sget("reloff"), sget("nreloc"));
                    if reloff != 0 && nreloc != 0 {
                        sections_with_relocs.push((segname.clone(), sectname.clone(), reloff, nreloc));
                    }
                    section_index += 1;
                    sect_off += sect_size;
                }
                if filesize != 0 {
                    let seg = b.region(
                        fileoff,
                        filesize,
                        segment_kind(&segname),
                        format!(
                            "Segment {}",
                            if segname.is_empty() && filetype == macho::MH_OBJECT.0 {
                                "(object)"
                            } else {
                                &segname
                            }
                        ),
                    );
                    b.set_value(
                        seg,
                        format!(
                            "{} {}..{}",
                            prot(get("initprot") as u32),
                            hex(vmaddr),
                            hex(vmaddr + vmsize)
                        ),
                    );
                    b.set_fill_gaps(seg);
                }
            }
            macho::LC_SYMTAB => {
                let (symoff, nsyms, stroff, strsize) = (get("symoff"), get("nsyms"), get("stroff"), get("strsize"));
                b.set_value(Some(node), format!("{nsyms} symbols"));
                let strtab = Some((stroff, strsize));
                symtab = Some(LinkedTable {
                    offset: symoff,
                    entry_size: nlist_size,
                    count: nsyms,
                    strtab,
                });
                let st = b.region(symoff, nsyms * nlist_size, RegionKind::Symbols, "Symbol table");
                b.set_value(st, format!("{nsyms} nlist entries"));
                let mut t = Table::new(TableKind::MachSymbols, nlist_size);
                t.strtab = strtab;
                b.set_decoder(st, Decoder::Table(t));
                let str_id = b.region(stroff, strsize, RegionKind::Strings, "String table");
                b.set_decoder(str_id, Decoder::Strings { skip: 0 });
            }
            macho::LC_DYSYMTAB => {
                let (off, n) = (get("indirectsymoff"), get("nindirectsyms"));
                if off != 0 && n != 0 {
                    let id = b.region(off, n * 4, RegionKind::Linking, "Indirect symbol table");
                    b.set_value(id, format!("{n} entries"));
                    b.set_note(id, "Maps stub and pointer slots to symbol table entries");
                    let mut t = Table::new(TableKind::MachIndirectSymbols, 4);
                    t.link = symtab;
                    b.set_decoder(id, Decoder::Table(t));
                }
                for (name, off_f, n_f) in [
                    ("External relocations", "extreloff", "nextrel"),
                    ("Local relocations", "locreloff", "nlocrel"),
                ] {
                    let (off, n) = (get(off_f), get(n_f));
                    if off != 0 && n != 0 {
                        let id = b.region(off, n * 8, RegionKind::Relocations, name);
                        let mut t = Table::new(TableKind::MachRelocs, 8);
                        t.link = symtab;
                        b.set_decoder(id, Decoder::Table(t));
                    }
                }
            }
            macho::LC_LOAD_DYLIB
            | macho::LC_ID_DYLIB
            | macho::LC_LOAD_WEAK_DYLIB
            | macho::LC_REEXPORT_DYLIB
            | macho::LC_LAZY_LOAD_DYLIB
            | macho::LC_LOAD_UPWARD_DYLIB
            | macho::LC_LOAD_DYLINKER
            | macho::LC_ID_DYLINKER
            | macho::LC_DYLD_ENVIRONMENT
            | macho::LC_RPATH => {
                let name_off = get("name.offset");
                if name_off >= consumed && name_off < c.size {
                    let s = ctx.bytes.cstr(c.offset + name_off, c.size - name_off).unwrap_or(&[]);
                    if let Some(id) = b.child(
                        node,
                        c.offset + name_off,
                        c.size - name_off,
                        RegionKind::Metadata,
                        "name",
                    ) {
                        b.set_value(Some(id), util::quote(s));
                        b.node_mut(id).is_field = true;
                    }
                    let summary = util::lossy(s);
                    if cmd == macho::LC_RPATH
                        || cmd == macho::LC_LOAD_DYLINKER
                        || cmd == macho::LC_DYLD_ENVIRONMENT
                        || cmd == macho::LC_ID_DYLINKER
                    {
                        b.set_value(Some(node), summary);
                    } else {
                        b.set_value(
                            Some(node),
                            format!("{summary} ({})", util::macho_version(get("current_version") as u32)),
                        );
                    }
                }
            }
            macho::LC_UUID => {
                let bytes = ctx.bytes.slice(c.offset + 8, 16).unwrap_or(&[]);
                b.set_value(Some(node), util::uuid(bytes));
            }
            macho::LC_CODE_SIGNATURE
            | macho::LC_SEGMENT_SPLIT_INFO
            | macho::LC_FUNCTION_STARTS
            | macho::LC_DATA_IN_CODE
            | macho::LC_DYLIB_CODE_SIGN_DRS
            | macho::LC_LINKER_OPTIMIZATION_HINT
            | macho::LC_DYLD_EXPORTS_TRIE
            | macho::LC_DYLD_CHAINED_FIXUPS
            | macho::LC_ATOM_INFO => {
                let (dataoff, datasize) = (get("dataoff"), get("datasize"));
                b.set_value(Some(node), format!("{} bytes at {}", datasize, hex(dataoff)));
                let (kind, label, note) = match cmd {
                    macho::LC_CODE_SIGNATURE => (
                        RegionKind::Signature,
                        "Code signature",
                        "Superblob with code directory hashes, requirements and entitlements",
                    ),
                    macho::LC_DYLIB_CODE_SIGN_DRS => (RegionKind::Signature, "Dylib code signing DRs", ""),
                    macho::LC_FUNCTION_STARTS => (
                        RegionKind::Metadata,
                        "Function starts",
                        "ULEB128 deltas between function start addresses",
                    ),
                    macho::LC_DATA_IN_CODE => (
                        RegionKind::Metadata,
                        "Data in code",
                        "Ranges of data embedded in code sections",
                    ),
                    macho::LC_DYLD_EXPORTS_TRIE => (
                        RegionKind::Linking,
                        "Exports trie",
                        "Prefix tree of exported symbol names",
                    ),
                    macho::LC_DYLD_CHAINED_FIXUPS => (
                        RegionKind::Linking,
                        "Chained fixups",
                        "Rebase and bind information for dyld",
                    ),
                    macho::LC_SEGMENT_SPLIT_INFO => (RegionKind::Metadata, "Segment split info", ""),
                    macho::LC_LINKER_OPTIMIZATION_HINT => (RegionKind::Metadata, "Linker optimization hints", ""),
                    _ => (RegionKind::Metadata, "Atom info", ""),
                };
                let id = b.region(dataoff, datasize, kind, label);
                if !note.is_empty() {
                    b.set_note(id, note);
                }
                if cmd == macho::LC_DATA_IN_CODE {
                    b.set_decoder(id, Decoder::Table(Table::new(TableKind::MachDataInCode, 8)));
                }
            }
            macho::LC_DYLD_INFO | macho::LC_DYLD_INFO_ONLY => {
                for (label, off_f, size_f) in [
                    ("Rebase opcodes", "rebase_off", "rebase_size"),
                    ("Bind opcodes", "bind_off", "bind_size"),
                    ("Weak bind opcodes", "weak_bind_off", "weak_bind_size"),
                    ("Lazy bind opcodes", "lazy_bind_off", "lazy_bind_size"),
                    ("Export trie", "export_off", "export_size"),
                ] {
                    b.region(get(off_f), get(size_f), RegionKind::Linking, label);
                }
            }
            macho::LC_MAIN => {
                b.set_value(Some(node), format!("entry at file offset {}", hex(get("entryoff"))));
            }
            macho::LC_BUILD_VERSION => {
                let ntools = get("ntools");
                let mut tool_off = c.offset + consumed;
                for t in 0..ntools.min(64) {
                    if tool_off + 8 > c.offset + c.size {
                        break;
                    }
                    let (id, tv) =
                        b.struct_child(node, tool_off, RegionKind::Metadata, format!("Tool {t}"), BUILD_TOOL);
                    b.set_value(id, tv.iter().map(|f| f.value.clone()).collect::<Vec<_>>().join(" "));
                    tool_off += 8;
                }
                b.set_value(
                    Some(node),
                    format!(
                        "{} {}",
                        platform(get("platform")).unwrap_or("?"),
                        util::macho_version(get("minos") as u32)
                    ),
                );
            }
            macho::LC_SOURCE_VERSION => {
                b.set_value(Some(node), source_version(&ctx, get("version")));
            }
            macho::LC_NOTE => {
                let owner = util::fixed_str(ctx.bytes.slice(c.offset + 8, 16).unwrap_or(&[]));
                b.region(get("offset"), get("size"), RegionKind::Notes, format!("Note {owner}"));
            }
            _ => {}
        }
        // Bytes of the command past its decoded fields (thread state, option strings...).
        let covered = b
            .node(node)
            .children
            .iter()
            .map(|&ch| b.node(ch).end)
            .max()
            .unwrap_or(c.offset);
        let end = c.offset + c.size;
        if covered < end {
            let tail = ctx.bytes.slice(covered, end - covered).unwrap_or(&[]);
            let (label, value) = if tail.iter().all(|&x| x == 0) {
                ("Padding", None)
            } else if cmd == macho::LC_LINKER_OPTION {
                (
                    "Options",
                    Some(
                        tail.split(|&x| x == 0)
                            .filter(|s| !s.is_empty())
                            .map(util::quote)
                            .collect::<Vec<_>>()
                            .join(" "),
                    ),
                )
            } else {
                ("Command data", Some(util::hex_bytes(&tail[..tail.len().min(24)])))
            };
            if let Some(id) = b.child(node, covered, end - covered, RegionKind::Metadata, label) {
                b.node_mut(id).is_field = true;
                if let Some(v) = value {
                    b.set_value(Some(id), v);
                }
            }
        }
    }

    for (segname, sectname, reloff, nreloc) in sections_with_relocs {
        let id = b.region(
            reloff,
            nreloc * 8,
            RegionKind::Relocations,
            format!("Relocations for {segname},{sectname}"),
        );
        b.set_value(id, format!("{nreloc} entries"));
        let mut t = Table::new(TableKind::MachRelocs, 8);
        t.link = symtab;
        b.set_decoder(id, Decoder::Table(t));
    }
}

pub(crate) fn table_entry(ctx: &Ctx, t: &Table, index: u64, offset: u64) -> Entry {
    match t.kind {
        TableKind::MachSymbols => {
            let c = ctx.with_strtab(t.strtab);
            let f = fields::decode(&c, offset, if ctx.is64 { NLIST64 } else { NLIST32 });
            let raw = c
                .string_at(fields::get(&f, "n_strx"))
                .map(util::lossy)
                .unwrap_or_default();
            let display = util::demangle(&raw).unwrap_or_else(|| raw.clone());
            let ntype = macho::SymbolFlags(fields::get(&f, "n_type") as u8);
            let what = if ntype.is_stab() {
                macho::SymbolStab(ntype.0).name().unwrap_or("stab").to_string()
            } else {
                ntype.typ().name().unwrap_or("?").to_string()
            };
            Entry {
                name: format!(
                    "Symbol {index}: {}",
                    if display.is_empty() { "(unnamed)" } else { &display }
                ),
                value: Some(format!("{what} {}", hex(fields::get(&f, "n_value")))),
                note: (display != raw).then(|| format!("mangled: {raw}")),
                fields: f,
                ..Entry::default()
            }
        }
        TableKind::MachRelocs => {
            let mut f = fields::decode(ctx, offset, RELOC);
            let info = fields::get(&f, "r_info") as u32;
            // Little-endian bitfield layout: symbolnum:24 pcrel:1 length:2 extern:1 type:4.
            let symbolnum = info & 0x00ff_ffff;
            let pcrel = (info >> 24) & 1;
            let length = 1u32 << ((info >> 25) & 3);
            let is_extern = (info >> 27) & 1;
            let typ = (info >> 28) as u8;
            let type_name = macho::machine_names(cputype(ctx))
                .reloc
                .name(macho::RelocationType(typ))
                .map_or_else(|| format!("type {typ}"), str::to_string);
            let target = if is_extern != 0 {
                t.link
                    .and_then(|l| nlist_name(ctx, &l, symbolnum as u64))
                    .unwrap_or_else(|| format!("symbol {symbolnum}"))
            } else {
                match ctx.sections.get((symbolnum as usize).wrapping_sub(1)) {
                    Some(s) => format!("section {},{}", s.segment_name.as_deref().unwrap_or(""), s.name),
                    None => format!("section {symbolnum}"),
                }
            };
            if let Some(field) = f.iter_mut().find(|f| f.name == "r_info") {
                field.value = format!(
                    "{} ({type_name}, {length} bytes{}, {target})",
                    hex(info as u64),
                    if pcrel != 0 { ", pc-relative" } else { "" }
                );
            }
            Entry {
                name: format!("Relocation {index}: {type_name}"),
                value: Some(format!("{} → {target}", hex(fields::get(&f, "r_address")))),
                fields: f,
                ..Entry::default()
            }
        }
        TableKind::MachIndirectSymbols => {
            let v = ctx.bytes.u32(offset).unwrap_or(0);
            let (local, abs) = (macho::INDIRECT_SYMBOL_LOCAL.0, macho::INDIRECT_SYMBOL_ABS.0);
            let desc = if v & local != 0 && v & abs != 0 {
                "INDIRECT_SYMBOL_LOCAL | INDIRECT_SYMBOL_ABS".to_string()
            } else if v & local != 0 {
                "INDIRECT_SYMBOL_LOCAL".to_string()
            } else if v & abs != 0 {
                "INDIRECT_SYMBOL_ABS".to_string()
            } else {
                t.link
                    .and_then(|l| nlist_name(ctx, &l, v as u64))
                    .map_or_else(|| format!("symbol {v}"), |n| format!("symbol {v}: {n}"))
            };
            Entry {
                name: format!("Indirect symbol {index}"),
                value: Some(desc.clone()),
                fields: vec![FieldValue {
                    start: offset,
                    end: offset + 4,
                    name: "index",
                    raw: v as u64,
                    value: desc,
                }],
                ..Entry::default()
            }
        }
        TableKind::MachDataInCode => {
            let f = fields::decode(ctx, offset, DICE);
            let kind = dice(fields::get(&f, "kind")).unwrap_or("?");
            Entry {
                name: format!("Data in code {index}: {kind}"),
                value: Some(format!(
                    "{} bytes at {}",
                    fields::get(&f, "length"),
                    hex(fields::get(&f, "offset"))
                )),
                fields: f,
                ..Entry::default()
            }
        }
        _ => Entry::default(),
    }
}

fn nlist_name(ctx: &Ctx, l: &LinkedTable, index: u64) -> Option<String> {
    if index >= l.count {
        return None;
    }
    let strx = ctx.bytes.u32(l.offset + index * l.entry_size)? as u64;
    let raw = util::lossy(ctx.with_strtab(l.strtab).string_at(strx)?);
    Some(util::demangle(&raw).unwrap_or(raw))
}
