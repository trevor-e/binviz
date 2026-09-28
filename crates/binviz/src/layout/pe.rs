//! PE (and plain COFF object) file layout.

use object::pe;

use super::decode::{Decoder, Entry, Table, TableKind};
use super::elf::flag_string;
use super::fields::{self, F, FieldValue, Fmt};
use super::{Builder, Ctx, Machine, Node};
use crate::model::RegionKind;
use crate::util::{self, hex};

fn machine(ctx: &Ctx) -> pe::Machine {
    match ctx.machine {
        Machine::Pe(m) => pe::Machine(m),
        _ => pe::Machine(0),
    }
}

fn mz(v: u64) -> Option<&'static str> {
    (v == 0x5a4d).then_some("MZ")
}
fn file_machine(v: u64) -> Option<&'static str> {
    pe::Machine(v as u16).name()
}
fn characteristics(v: u64) -> String {
    flag_string(pe::FileFlags::NAMES, pe::FileFlags(v as u16))
}
fn opt_magic(v: u64) -> Option<&'static str> {
    match v as u16 {
        0x10b => Some("PE32"),
        0x20b => Some("PE32+"),
        0x107 => Some("ROM"),
        _ => None,
    }
}
fn subsystem(v: u64) -> Option<&'static str> {
    pe::Subsystem(v as u16).name()
}
fn dll_flags(v: u64) -> String {
    flag_string(pe::DllFlags::NAMES, pe::DllFlags(v as u16))
}
fn scn_flags(v: u64) -> String {
    flag_string(pe::SectionFlags::NAMES, pe::SectionFlags(v as u32))
}
fn debug_type(v: u64) -> Option<&'static str> {
    pe::DebugType(v as u32).name()
}
fn guard_flags(v: u64) -> String {
    flag_string(pe::GuardFlags::NAMES, pe::GuardFlags(v as u32))
}
fn cor_flags(v: u64) -> String {
    flag_string(pe::CorFlags::NAMES, pe::CorFlags(v as u32))
}
fn sym_class(v: u64) -> Option<&'static str> {
    pe::SymbolClass(v as u8).name()
}
fn sym_section(ctx: &Ctx, v: u64) -> String {
    let n = v as i16;
    match n {
        0 => "0 (IMAGE_SYM_UNDEFINED)".into(),
        -1 => "-1 (IMAGE_SYM_ABSOLUTE)".into(),
        -2 => "-2 (IMAGE_SYM_DEBUG)".into(),
        n if n > 0 => match ctx.sections.get(n as usize - 1) {
            Some(s) => format!("{n} ({})", s.name),
            None => n.to_string(),
        },
        n => n.to_string(),
    }
}
fn cert_type(v: u64) -> Option<&'static str> {
    match v {
        1 => Some("WIN_CERT_TYPE_X509"),
        2 => Some("WIN_CERT_TYPE_PKCS_SIGNED_DATA"),
        3 => Some("WIN_CERT_TYPE_RESERVED_1"),
        4 => Some("WIN_CERT_TYPE_TS_STACK_SIGNED"),
        _ => None,
    }
}
fn rel_type(ctx: &Ctx, v: u64) -> Option<String> {
    Some(flag_string(
        pe::machine_names(machine(ctx)).rel,
        pe::RelocationType(v as u16),
    ))
}

const DIRECTORY_NAMES: [&str; 16] = [
    "Export table",
    "Import table",
    "Resource table",
    "Exception table",
    "Certificate table",
    "Base relocation table",
    "Debug directory",
    "Architecture",
    "Global pointer",
    "TLS table",
    "Load config table",
    "Bound import table",
    "Import address table",
    "Delay import descriptors",
    "CLR runtime header",
    "Reserved",
];

const DOS_HEADER: &[F] = &[
    F::u16("e_magic", Fmt::Name(mz)),
    F::u16("e_cblp", Fmt::Dec),
    F::u16("e_cp", Fmt::Dec),
    F::u16("e_crlc", Fmt::Dec),
    F::u16("e_cparhdr", Fmt::Dec),
    F::u16("e_minalloc", Fmt::Dec),
    F::u16("e_maxalloc", Fmt::Hex),
    F::u16("e_ss", Fmt::Hex),
    F::u16("e_sp", Fmt::Hex),
    F::u16("e_csum", Fmt::Hex),
    F::u16("e_ip", Fmt::Hex),
    F::u16("e_cs", Fmt::Hex),
    F::u16("e_lfarlc", Fmt::Hex),
    F::u16("e_ovno", Fmt::Dec),
    F::bytes("e_res", 8),
    F::u16("e_oemid", Fmt::Hex),
    F::u16("e_oeminfo", Fmt::Hex),
    F::bytes("e_res2", 20),
    F::u32("e_lfanew", Fmt::Off),
];

const FILE_HEADER: &[F] = &[
    F::u16("Machine", Fmt::Name(file_machine)),
    F::u16("NumberOfSections", Fmt::Dec),
    F::u32("TimeDateStamp", Fmt::Time),
    F::u32("PointerToSymbolTable", Fmt::Off),
    F::u32("NumberOfSymbols", Fmt::Dec),
    F::u16("SizeOfOptionalHeader", Fmt::Size),
    F::u16("Characteristics", Fmt::Flags(characteristics)),
];

const OPT32: &[F] = &[
    F::u16("Magic", Fmt::Name(opt_magic)),
    F::u8("MajorLinkerVersion", Fmt::Dec),
    F::u8("MinorLinkerVersion", Fmt::Dec),
    F::u32("SizeOfCode", Fmt::Size),
    F::u32("SizeOfInitializedData", Fmt::Size),
    F::u32("SizeOfUninitializedData", Fmt::Size),
    F::u32("AddressOfEntryPoint", Fmt::Rva),
    F::u32("BaseOfCode", Fmt::Rva),
    F::u32("BaseOfData", Fmt::Rva),
    F::u32("ImageBase", Fmt::Addr),
    F::u32("SectionAlignment", Fmt::Hex),
    F::u32("FileAlignment", Fmt::Hex),
    F::u16("MajorOperatingSystemVersion", Fmt::Dec),
    F::u16("MinorOperatingSystemVersion", Fmt::Dec),
    F::u16("MajorImageVersion", Fmt::Dec),
    F::u16("MinorImageVersion", Fmt::Dec),
    F::u16("MajorSubsystemVersion", Fmt::Dec),
    F::u16("MinorSubsystemVersion", Fmt::Dec),
    F::u32("Win32VersionValue", Fmt::Hex),
    F::u32("SizeOfImage", Fmt::Size),
    F::u32("SizeOfHeaders", Fmt::Size),
    F::u32("CheckSum", Fmt::Hex),
    F::u16("Subsystem", Fmt::Name(subsystem)),
    F::u16("DllCharacteristics", Fmt::Flags(dll_flags)),
    F::u32("SizeOfStackReserve", Fmt::Size),
    F::u32("SizeOfStackCommit", Fmt::Size),
    F::u32("SizeOfHeapReserve", Fmt::Size),
    F::u32("SizeOfHeapCommit", Fmt::Size),
    F::u32("LoaderFlags", Fmt::Hex),
    F::u32("NumberOfRvaAndSizes", Fmt::Dec),
];

const OPT64: &[F] = &[
    F::u16("Magic", Fmt::Name(opt_magic)),
    F::u8("MajorLinkerVersion", Fmt::Dec),
    F::u8("MinorLinkerVersion", Fmt::Dec),
    F::u32("SizeOfCode", Fmt::Size),
    F::u32("SizeOfInitializedData", Fmt::Size),
    F::u32("SizeOfUninitializedData", Fmt::Size),
    F::u32("AddressOfEntryPoint", Fmt::Rva),
    F::u32("BaseOfCode", Fmt::Rva),
    F::u64("ImageBase", Fmt::Addr),
    F::u32("SectionAlignment", Fmt::Hex),
    F::u32("FileAlignment", Fmt::Hex),
    F::u16("MajorOperatingSystemVersion", Fmt::Dec),
    F::u16("MinorOperatingSystemVersion", Fmt::Dec),
    F::u16("MajorImageVersion", Fmt::Dec),
    F::u16("MinorImageVersion", Fmt::Dec),
    F::u16("MajorSubsystemVersion", Fmt::Dec),
    F::u16("MinorSubsystemVersion", Fmt::Dec),
    F::u32("Win32VersionValue", Fmt::Hex),
    F::u32("SizeOfImage", Fmt::Size),
    F::u32("SizeOfHeaders", Fmt::Size),
    F::u32("CheckSum", Fmt::Hex),
    F::u16("Subsystem", Fmt::Name(subsystem)),
    F::u16("DllCharacteristics", Fmt::Flags(dll_flags)),
    F::u64("SizeOfStackReserve", Fmt::Size),
    F::u64("SizeOfStackCommit", Fmt::Size),
    F::u64("SizeOfHeapReserve", Fmt::Size),
    F::u64("SizeOfHeapCommit", Fmt::Size),
    F::u32("LoaderFlags", Fmt::Hex),
    F::u32("NumberOfRvaAndSizes", Fmt::Dec),
];

const DATA_DIR: &[F] = &[F::u32("VirtualAddress", Fmt::Rva), F::u32("Size", Fmt::Size)];
const DATA_DIR_FILE: &[F] = &[F::u32("FileOffset", Fmt::Off), F::u32("Size", Fmt::Size)];

const SECTION_HEADER: &[F] = &[
    F::str("Name", 8),
    F::u32("VirtualSize", Fmt::Size),
    F::u32("VirtualAddress", Fmt::Rva),
    F::u32("SizeOfRawData", Fmt::Size),
    F::u32("PointerToRawData", Fmt::Off),
    F::u32("PointerToRelocations", Fmt::Off),
    F::u32("PointerToLinenumbers", Fmt::Off),
    F::u16("NumberOfRelocations", Fmt::Dec),
    F::u16("NumberOfLinenumbers", Fmt::Dec),
    F::u32("Characteristics", Fmt::Flags(scn_flags)),
];

const IMPORT_DESCRIPTOR: &[F] = &[
    F::u32("OriginalFirstThunk", Fmt::Rva),
    F::u32("TimeDateStamp", Fmt::Hex),
    F::u32("ForwarderChain", Fmt::Hex),
    F::u32("Name", Fmt::Rva),
    F::u32("FirstThunk", Fmt::Rva),
];
const DELAY_DESCRIPTOR: &[F] = &[
    F::u32("Attributes", Fmt::Hex),
    F::u32("DllNameRVA", Fmt::Rva),
    F::u32("ModuleHandleRVA", Fmt::Rva),
    F::u32("ImportAddressTableRVA", Fmt::Rva),
    F::u32("ImportNameTableRVA", Fmt::Rva),
    F::u32("BoundImportAddressTableRVA", Fmt::Rva),
    F::u32("UnloadInformationTableRVA", Fmt::Rva),
    F::u32("TimeDateStamp", Fmt::Time),
];
const EXPORT_DIRECTORY: &[F] = &[
    F::u32("Characteristics", Fmt::Hex),
    F::u32("TimeDateStamp", Fmt::Time),
    F::u16("MajorVersion", Fmt::Dec),
    F::u16("MinorVersion", Fmt::Dec),
    F::u32("Name", Fmt::Rva),
    F::u32("Base", Fmt::Dec),
    F::u32("NumberOfFunctions", Fmt::Dec),
    F::u32("NumberOfNames", Fmt::Dec),
    F::u32("AddressOfFunctions", Fmt::Rva),
    F::u32("AddressOfNames", Fmt::Rva),
    F::u32("AddressOfNameOrdinals", Fmt::Rva),
];
const DEBUG_DIRECTORY: &[F] = &[
    F::u32("Characteristics", Fmt::Hex),
    F::u32("TimeDateStamp", Fmt::Time),
    F::u16("MajorVersion", Fmt::Dec),
    F::u16("MinorVersion", Fmt::Dec),
    F::u32("Type", Fmt::Name(debug_type)),
    F::u32("SizeOfData", Fmt::Size),
    F::u32("AddressOfRawData", Fmt::Rva),
    F::u32("PointerToRawData", Fmt::Off),
];
const RUNTIME_FUNCTION_X64: &[F] = &[
    F::u32("BeginAddress", Fmt::Rva),
    F::u32("EndAddress", Fmt::Rva),
    F::u32("UnwindInfoAddress", Fmt::Rva),
];
const RUNTIME_FUNCTION_ARM64: &[F] = &[F::u32("BeginAddress", Fmt::Rva), F::u32("UnwindData", Fmt::Hex)];
const TLS64: &[F] = &[
    F::u64("StartAddressOfRawData", Fmt::Addr),
    F::u64("EndAddressOfRawData", Fmt::Addr),
    F::u64("AddressOfIndex", Fmt::Addr),
    F::u64("AddressOfCallBacks", Fmt::Addr),
    F::u32("SizeOfZeroFill", Fmt::Size),
    F::u32("Characteristics", Fmt::Hex),
];
const TLS32: &[F] = &[
    F::u32("StartAddressOfRawData", Fmt::Addr),
    F::u32("EndAddressOfRawData", Fmt::Addr),
    F::u32("AddressOfIndex", Fmt::Addr),
    F::u32("AddressOfCallBacks", Fmt::Addr),
    F::u32("SizeOfZeroFill", Fmt::Size),
    F::u32("Characteristics", Fmt::Hex),
];
const LOAD_CONFIG64: &[F] = &[
    F::u32("Size", Fmt::Size),
    F::u32("TimeDateStamp", Fmt::Time),
    F::u16("MajorVersion", Fmt::Dec),
    F::u16("MinorVersion", Fmt::Dec),
    F::u32("GlobalFlagsClear", Fmt::Hex),
    F::u32("GlobalFlagsSet", Fmt::Hex),
    F::u32("CriticalSectionDefaultTimeout", Fmt::Dec),
    F::u64("DeCommitFreeBlockThreshold", Fmt::Size),
    F::u64("DeCommitTotalFreeThreshold", Fmt::Size),
    F::u64("LockPrefixTable", Fmt::Addr),
    F::u64("MaximumAllocationSize", Fmt::Size),
    F::u64("VirtualMemoryThreshold", Fmt::Size),
    F::u64("ProcessAffinityMask", Fmt::Hex),
    F::u32("ProcessHeapFlags", Fmt::Hex),
    F::u16("CSDVersion", Fmt::Dec),
    F::u16("DependentLoadFlags", Fmt::Hex),
    F::u64("EditList", Fmt::Addr),
    F::u64("SecurityCookie", Fmt::Addr),
    F::u64("SEHandlerTable", Fmt::Addr),
    F::u64("SEHandlerCount", Fmt::Dec),
    F::u64("GuardCFCheckFunctionPointer", Fmt::Addr),
    F::u64("GuardCFDispatchFunctionPointer", Fmt::Addr),
    F::u64("GuardCFFunctionTable", Fmt::Addr),
    F::u64("GuardCFFunctionCount", Fmt::Dec),
    F::u32("GuardFlags", Fmt::Flags(guard_flags)),
];
const LOAD_CONFIG32: &[F] = &[
    F::u32("Size", Fmt::Size),
    F::u32("TimeDateStamp", Fmt::Time),
    F::u16("MajorVersion", Fmt::Dec),
    F::u16("MinorVersion", Fmt::Dec),
    F::u32("GlobalFlagsClear", Fmt::Hex),
    F::u32("GlobalFlagsSet", Fmt::Hex),
    F::u32("CriticalSectionDefaultTimeout", Fmt::Dec),
    F::u32("DeCommitFreeBlockThreshold", Fmt::Size),
    F::u32("DeCommitTotalFreeThreshold", Fmt::Size),
    F::u32("LockPrefixTable", Fmt::Addr),
    F::u32("MaximumAllocationSize", Fmt::Size),
    F::u32("VirtualMemoryThreshold", Fmt::Size),
    F::u32("ProcessHeapFlags", Fmt::Hex),
    F::u32("ProcessAffinityMask", Fmt::Hex),
    F::u16("CSDVersion", Fmt::Dec),
    F::u16("DependentLoadFlags", Fmt::Hex),
    F::u32("EditList", Fmt::Addr),
    F::u32("SecurityCookie", Fmt::Addr),
    F::u32("SEHandlerTable", Fmt::Addr),
    F::u32("SEHandlerCount", Fmt::Dec),
    F::u32("GuardCFCheckFunctionPointer", Fmt::Addr),
    F::u32("GuardCFDispatchFunctionPointer", Fmt::Addr),
    F::u32("GuardCFFunctionTable", Fmt::Addr),
    F::u32("GuardCFFunctionCount", Fmt::Dec),
    F::u32("GuardFlags", Fmt::Flags(guard_flags)),
];
const CLR_HEADER: &[F] = &[
    F::u32("cb", Fmt::Size),
    F::u16("MajorRuntimeVersion", Fmt::Dec),
    F::u16("MinorRuntimeVersion", Fmt::Dec),
    F::u32("MetaData.VirtualAddress", Fmt::Rva),
    F::u32("MetaData.Size", Fmt::Size),
    F::u32("Flags", Fmt::Flags(cor_flags)),
    F::u32("EntryPointToken", Fmt::Hex),
    F::u32("Resources.VirtualAddress", Fmt::Rva),
    F::u32("Resources.Size", Fmt::Size),
    F::u32("StrongNameSignature.VirtualAddress", Fmt::Rva),
    F::u32("StrongNameSignature.Size", Fmt::Size),
    F::u32("CodeManagerTable.VirtualAddress", Fmt::Rva),
    F::u32("CodeManagerTable.Size", Fmt::Size),
    F::u32("VTableFixups.VirtualAddress", Fmt::Rva),
    F::u32("VTableFixups.Size", Fmt::Size),
    F::u32("ExportAddressTableJumps.VirtualAddress", Fmt::Rva),
    F::u32("ExportAddressTableJumps.Size", Fmt::Size),
    F::u32("ManagedNativeHeader.VirtualAddress", Fmt::Rva),
    F::u32("ManagedNativeHeader.Size", Fmt::Size),
];
const CODEVIEW_RSDS: &[F] = &[F::bytes("Signature", 4), F::bytes("Guid", 16), F::u32("Age", Fmt::Dec)];
const COFF_SYMBOL: &[F] = &[
    F::bytes("Name", 8),
    F::u32("Value", Fmt::Hex),
    F::i16("SectionNumber"),
    F::u16("Type", Fmt::Hex),
    F::u8("StorageClass", Fmt::Name(sym_class)),
    F::u8("NumberOfAuxSymbols", Fmt::Dec),
];
const COFF_RELOC: &[F] = &[
    F::u32("VirtualAddress", Fmt::Hex),
    F::u32("SymbolTableIndex", Fmt::Dec),
    F::u16("Type", Fmt::CtxName(rel_type)),
];
const WIN_CERTIFICATE: &[F] = &[
    F::u32("dwLength", Fmt::Size),
    F::u16("wRevision", Fmt::Hex),
    F::u16("wCertificateType", Fmt::Name(cert_type)),
];
const RESOURCE_DIRECTORY: &[F] = &[
    F::u32("Characteristics", Fmt::Hex),
    F::u32("TimeDateStamp", Fmt::Time),
    F::u16("MajorVersion", Fmt::Dec),
    F::u16("MinorVersion", Fmt::Dec),
    F::u16("NumberOfNamedEntries", Fmt::Dec),
    F::u16("NumberOfIdEntries", Fmt::Dec),
];
const RESOURCE_ENTRY: &[F] = &[F::u32("Name", Fmt::Hex), F::u32("OffsetToData", Fmt::Hex)];
const RESOURCE_DATA: &[F] = &[
    F::u32("OffsetToData", Fmt::Rva),
    F::u32("Size", Fmt::Size),
    F::u32("CodePage", Fmt::Dec),
    F::u32("Reserved", Fmt::Hex),
];

pub(crate) fn classify(name: &str, chars: u32, has_raw: bool) -> RegionKind {
    let has = |f: u32| chars & f != 0;
    if let Some(k) = super::elf::classify_by_name(name) {
        return k;
    }
    if name == ".tls" || name.starts_with(".tls$") {
        return RegionKind::Tls;
    }
    if has(pe::IMAGE_SCN_CNT_CODE.0) || has(pe::IMAGE_SCN_MEM_EXECUTE.0) {
        RegionKind::Code
    } else if has(pe::IMAGE_SCN_CNT_UNINITIALIZED_DATA.0) && !has_raw {
        RegionKind::Bss
    } else if has(pe::IMAGE_SCN_MEM_WRITE.0) {
        RegionKind::Data
    } else if has(pe::IMAGE_SCN_MEM_READ.0) && !has(pe::IMAGE_SCN_MEM_DISCARDABLE.0) {
        RegionKind::Rodata
    } else {
        RegionKind::Metadata
    }
}

struct Sect {
    name: String,
    vsize: u64,
    va: u64,
    raw_size: u64,
    raw_ptr: u64,
    reloc_ptr: u64,
    nrelocs: u64,
    chars: u32,
}

/// Resolves a section name, following `/NNN` references into the COFF string table.
fn section_name(ctx: &Ctx, raw: &[u8], strtab: Option<(u64, u64)>) -> String {
    let short = util::fixed_str(raw);
    if let Some(rest) = short.strip_prefix('/')
        && let Ok(off) = rest.parse::<u64>()
        && let Some(s) = ctx.with_strtab(strtab).string_at(off)
    {
        return util::lossy(s);
    }
    short
}

pub(crate) fn build(b: &mut Builder, image: bool) {
    let ctx = b.ctx;
    let (coff_off, opt_size) = if image {
        let (dos, dv) = b.struct_region(0, RegionKind::Header, "DOS header", DOS_HEADER);
        b.set_note(dos, "Legacy MS-DOS header; e_lfanew points at the PE headers");
        let lfanew = fields::get(&dv, "e_lfanew");
        if lfanew > 64 {
            let stub = b.region(64, lfanew - 64, RegionKind::Header, "DOS stub");
            if let Some(stub) = stub {
                dos_stub(b, stub, lfanew);
            }
        }
        let opt_size = ctx.bytes.u16(lfanew + 20).unwrap_or(0) as u64;
        let nt = b.region(lfanew, 24 + opt_size, RegionKind::Header, "NT headers");
        if let Some(nt) = nt {
            if let Some(sig) = b.child(nt, lfanew, 4, RegionKind::Header, "Signature") {
                b.node_mut(sig).is_field = true;
                b.set_value(Some(sig), util::quote(ctx.bytes.slice(lfanew, 4).unwrap_or(&[])));
            }
            let (fh, _) = b.struct_child(nt, lfanew + 4, RegionKind::Header, "COFF file header", FILE_HEADER);
            b.set_note(fh, "Target machine, section count, timestamp and characteristics");
        }
        (lfanew + 4, opt_size)
    } else {
        let (fh, fv) = b.struct_region(0, RegionKind::Header, "COFF file header", FILE_HEADER);
        b.set_note(fh, "Header of a COFF relocatable object");
        (0, fields::get(&fv, "SizeOfOptionalHeader"))
    };

    let b8 = ctx.bytes;
    let nsections = b8.u16(coff_off + 2).unwrap_or(0) as u64;
    let symtab_off = b8.u32(coff_off + 8).unwrap_or(0) as u64;
    let nsyms = b8.u32(coff_off + 12).unwrap_or(0) as u64;
    let strtab = if symtab_off != 0 {
        let st = symtab_off + nsyms * 18;
        b8.u32(st).map(|size| (st, size as u64))
    } else {
        None
    };

    let opt_off = coff_off + 20;
    let mut dirs: Vec<(u64, u64)> = Vec::new();
    let mut size_of_headers = 0;
    if image && opt_size > 0 {
        let magic = b8.u16(opt_off).unwrap_or(0);
        let is64 = magic == pe::IMAGE_NT_OPTIONAL_HDR64_MAGIC;
        let specs = if is64 { OPT64 } else { OPT32 };
        let nt_parent = b.node_path_of(opt_off);
        if let Some(parent) = nt_parent {
            let (opt, ov) = b.struct_child(parent, opt_off, RegionKind::Header, "Optional header", specs);
            b.set_note(
                opt,
                "Loader parameters: entry point, image base, alignment, subsystem, data directories",
            );
            size_of_headers = fields::get(&ov, "SizeOfHeaders");
            let ndirs = fields::get(&ov, "NumberOfRvaAndSizes").min(16);
            let dir_off = opt_off + fields::struct_size(specs);
            // The optional header node covers only the fixed part; the
            // directories follow it within the NT headers.
            if opt.is_some() && ndirs > 0 {
                let dd = b.child(parent, dir_off, ndirs * 8, RegionKind::Header, "Data directories");
                if let Some(dd) = dd {
                    for i in 0..ndirs {
                        let spec = if i == 4 { DATA_DIR_FILE } else { DATA_DIR };
                        let (id, v) = b.struct_child(
                            dd,
                            dir_off + i * 8,
                            RegionKind::Header,
                            DIRECTORY_NAMES[i as usize],
                            spec,
                        );
                        let (rva, size) = (v.first().map_or(0, |f| f.raw), v.get(1).map_or(0, |f| f.raw));
                        if rva != 0 || size != 0 {
                            b.set_value(
                                id,
                                format!(
                                    "{} bytes at {}",
                                    size,
                                    if i == 4 {
                                        format!("offset {}", hex(rva))
                                    } else {
                                        format!("RVA {}", hex(rva))
                                    }
                                ),
                            );
                        } else {
                            b.set_value(id, "(empty)");
                        }
                        dirs.push((rva, size));
                    }
                }
            }
        }
    }

    // Section table.
    let sect_off = opt_off + opt_size;
    let mut sects = Vec::new();
    if nsections > 0 {
        let table = b.region(sect_off, nsections * 40, RegionKind::Metadata, "Section table");
        b.set_value(table, format!("{nsections} sections"));
        for i in 0..nsections.min(65536) {
            let off = sect_off + i * 40;
            let name = section_name(&ctx, b8.slice(off, 8).unwrap_or(&[]), strtab);
            let (_, v) = match table {
                Some(t) => b.struct_child(
                    t,
                    off,
                    RegionKind::Metadata,
                    format!("Section header {name}"),
                    SECTION_HEADER,
                ),
                None => (None, Vec::new()),
            };
            if v.len() < SECTION_HEADER.len() {
                break;
            }
            sects.push(Sect {
                name,
                vsize: fields::get(&v, "VirtualSize"),
                va: fields::get(&v, "VirtualAddress"),
                raw_size: fields::get(&v, "SizeOfRawData"),
                raw_ptr: fields::get(&v, "PointerToRawData"),
                reloc_ptr: fields::get(&v, "PointerToRelocations"),
                nrelocs: fields::get(&v, "NumberOfRelocations"),
                chars: fields::get(&v, "Characteristics") as u32,
            });
        }
    }

    // Headers padding up to SizeOfHeaders.
    let headers_end = sect_off + nsections * 40;
    if image && size_of_headers > headers_end {
        let id = b.region(
            headers_end,
            size_of_headers - headers_end,
            RegionKind::Padding,
            "Header padding",
        );
        b.set_note(id, "Pads the headers to FileAlignment (SizeOfHeaders)");
    }

    // Section contents.
    let mut image_end = headers_end.max(size_of_headers);
    for (i, s) in sects.iter().enumerate() {
        if s.raw_ptr == 0 || s.raw_size == 0 {
            continue;
        }
        let data_size = if image && s.vsize != 0 {
            s.raw_size.min(s.vsize)
        } else {
            s.raw_size
        };
        let kind = classify(&s.name, s.chars, true);
        let id = b.region(s.raw_ptr, data_size, kind, format!("Section {}", s.name));
        b.set_section(id, i as u32);
        if image {
            b.set_value(id, format!("RVA {}..{}", hex(s.va), hex(s.va + s.vsize.max(data_size))));
        } else {
            b.set_value(id, format!("{data_size} bytes of raw data"));
        }
        if matches!(
            kind,
            RegionKind::Code | RegionKind::Data | RegionKind::Rodata | RegionKind::Tls
        ) && image
        {
            b.set_decoder(
                id,
                Decoder::Symbols {
                    address: ctx.image_base + s.va,
                },
            );
        } else if let Some(d) = Decoder::for_dwarf_section(&s.name) {
            b.set_decoder(id, d);
        }
        if s.raw_size > data_size {
            let pad = b.region(
                s.raw_ptr + data_size,
                s.raw_size - data_size,
                RegionKind::Padding,
                format!("File alignment padding ({})", s.name),
            );
            b.set_note(
                pad,
                "Raw data beyond VirtualSize, rounding the section up to FileAlignment",
            );
        }
        image_end = image_end.max(s.raw_ptr + s.raw_size);
        if s.reloc_ptr != 0 && s.nrelocs != 0 {
            let rid = b.region(
                s.reloc_ptr,
                s.nrelocs * 10,
                RegionKind::Relocations,
                format!("Relocations for {}", s.name),
            );
            b.set_value(rid, format!("{} entries", s.nrelocs));
            let mut t = Table::new(TableKind::PeSectionRelocs, 10);
            t.strtab = strtab;
            t.param = symtab_off;
            b.set_decoder(rid, Decoder::Table(t));
            image_end = image_end.max(s.reloc_ptr + s.nrelocs * 10);
        }
    }

    if image {
        directories(b, &dirs);
    }

    // COFF symbol table and string table.
    if symtab_off != 0 && nsyms != 0 {
        let id = b.region(symtab_off, nsyms * 18, RegionKind::Symbols, "COFF symbol table");
        b.set_value(id, format!("{nsyms} records"));
        b.set_note(id, "Present in objects and in images linked with debug info by MinGW");
        b.set_decoder(id, Decoder::CoffSymbols { strtab });
        image_end = image_end.max(symtab_off + nsyms * 18);
        if let Some((st, size)) = strtab {
            let sid = b.region(st, size.max(4), RegionKind::Strings, "COFF string table");
            b.set_decoder(sid, Decoder::Strings { skip: 4 });
            image_end = image_end.max(st + size.max(4));
        }
    }

    // Certificate table (a file offset, not an RVA) and overlay.
    let file_size = b8.data.len() as u64;
    let mut cert = None;
    if let Some(&(off, size)) = dirs.get(4)
        && off != 0
        && size != 0
    {
        let id = b.region(off, size, RegionKind::Signature, "Certificate table");
        b.set_note(id, "Authenticode signature (WIN_CERTIFICATE records)");
        if let Some(id) = id {
            let mut pos = off;
            let mut n = 0;
            while pos + 8 <= off + size && n < 16 {
                let len = b8.u32(pos).unwrap_or(0) as u64;
                if len < 8 {
                    break;
                }
                let (cid, _) = b.struct_child(
                    id,
                    pos,
                    RegionKind::Signature,
                    format!("Certificate {n}"),
                    WIN_CERTIFICATE,
                );
                if let Some(cid) = cid {
                    b.child(cid, pos + 8, len - 8, RegionKind::Signature, "bCertificate");
                    b.node_mut(cid).end = (pos + len).min(file_size);
                }
                pos += util::align_up(len, 8);
                n += 1;
            }
        }
        cert = Some((off, off + size));
    }
    if image && image_end < file_size {
        let mut pieces = vec![(image_end, file_size)];
        if let Some((cs, ce)) = cert {
            pieces = pieces
                .into_iter()
                .flat_map(|(s, e)| {
                    let mut v = Vec::new();
                    if cs > s {
                        v.push((s, cs.min(e)));
                    }
                    if ce < e {
                        v.push((ce.max(s), e));
                    }
                    v
                })
                .collect();
        }
        for (s, e) in pieces {
            if e <= s {
                continue;
            }
            // Linkers pad the file to FileAlignment; that isn't a real overlay.
            let zero = b8.slice(s, e - s).is_some_and(|d| d.iter().all(|&x| x == 0));
            if zero && e - s < 0x1000 {
                let id = b.region(s, e - s, RegionKind::Padding, "File alignment padding");
                b.set_note(id, "Zero bytes rounding the file up to FileAlignment");
            } else {
                let id = b.region(s, e - s, RegionKind::Overlay, "Overlay");
                b.set_note(id, "Data appended after the last section; not mapped by the loader (installers, signatures, archives...)");
            }
        }
    }
}

impl Builder<'_> {
    /// The NT headers node, looked up by offset (helper for the optional header).
    fn node_path_of(&self, offset: u64) -> Option<u32> {
        (0..self.nodes.len() as u32).rev().find(|&i| {
            let n: &Node = &self.nodes[i as usize];
            n.name == "NT headers" && n.start <= offset && offset < n.end
        })
    }
}

fn dos_stub(b: &mut Builder, stub: u32, lfanew: u64) {
    let ctx = b.ctx;
    let rich = object::read::pe::RichHeaderInfo::parse(ctx.bytes.data, lfanew);
    let (rich_start, rich_end) = match &rich {
        Some(r) => (r.offset as u64, (r.offset + r.length) as u64),
        None => (lfanew, lfanew),
    };
    if rich_start > 64 {
        let prog = b.child(stub, 64, rich_start - 64, RegionKind::Header, "DOS program");
        let text = ctx.bytes.slice(64, rich_start - 64).unwrap_or(&[]);
        if let Some(pos) = text.windows(4).position(|w| w == b"This") {
            let msg: Vec<u8> = text[pos..].iter().copied().take_while(|&c| c != b'$').collect();
            b.set_value(prog, util::quote(msg.trim_ascii()));
        }
        b.set_note(prog, "16-bit program that prints a message when run under DOS");
    }
    if let Some(r) = rich {
        let id = b.child(
            stub,
            rich_start,
            rich_end - rich_start,
            RegionKind::Metadata,
            "Rich header",
        );
        b.set_note(
            id,
            "Undocumented MSVC toolchain fingerprint, XOR-masked with a checksum key",
        );
        b.set_value(id, format!("key {:#010x}", r.xor_key));
        if let Some(id) = id {
            if let Some(f) = b.child(id, rich_start, 16, RegionKind::Metadata, "\"DanS\" marker + padding") {
                b.node_mut(f).is_field = true;
            }
            for (i, e) in r.unmasked_entries().enumerate() {
                let off = rich_start + 16 + i as u64 * 8;
                if let Some(f) = b.child(id, off, 8, RegionKind::Metadata, format!("Entry {i}")) {
                    b.node_mut(f).is_field = true;
                    b.set_value(
                        Some(f),
                        format!(
                            "product {:#06x}, build {}, used {} times",
                            e.comp_id >> 16,
                            e.comp_id & 0xffff,
                            e.count
                        ),
                    );
                }
            }
            if let Some(f) = b.child(id, rich_end - 8, 8, RegionKind::Metadata, "\"Rich\" marker + key") {
                b.node_mut(f).is_field = true;
            }
        }
    }
    if rich_end < lfanew && rich.is_some() {
        b.child(stub, rich_end, lfanew - rich_end, RegionKind::Padding, "Padding");
    }
}

fn directories(b: &mut Builder, dirs: &[(u64, u64)]) {
    let ctx = b.ctx;
    let ptr = ctx.addr_size() as u64;
    let dir = |i: usize| dirs.get(i).copied().filter(|&(rva, size)| rva != 0 && size != 0);
    let off_of = |rva: u64| ctx.rva_to_offset(rva);

    // Import directory and its per-DLL tables.
    if let Some((rva, size)) = dir(1)
        && let Some(off) = off_of(rva)
    {
        let id = b.region(off, size, RegionKind::Linking, "Import directory");
        b.set_note(id, "One descriptor per imported DLL, terminated by a zero descriptor");
        b.set_decoder(id, Decoder::Table(Table::new(TableKind::PeImportDescriptors, 20)));
        let mut pos = off;
        let mut count = 0;
        while count < 4096 {
            let v = fields::decode(&ctx, pos, IMPORT_DESCRIPTOR);
            if v.len() < 5 || v.iter().all(|f| f.raw == 0) {
                break;
            }
            let dll = ctx
                .cstr_at_rva(fields::get(&v, "Name"))
                .map(util::lossy)
                .unwrap_or_default();
            if let Some(noff) = off_of(fields::get(&v, "Name")) {
                let nid = b.region(
                    noff,
                    dll.len() as u64 + 1,
                    RegionKind::Linking,
                    format!("DLL name {dll}"),
                );
                b.set_value(nid, util::quote(dll.as_bytes()));
            }
            for (field, label) in [
                ("OriginalFirstThunk", "Import lookup table"),
                ("FirstThunk", "Import address table"),
            ] {
                let trva = fields::get(&v, field);
                let Some(toff) = off_of(trva) else { continue };
                let mut n = 0;
                while n < 65536 {
                    let entry = ctx.bytes.uint(toff + n * ptr, ptr as u32).unwrap_or(0);
                    if entry == 0 {
                        break;
                    }
                    // Hint/name entries referenced by the lookup table.
                    let ordinal_flag = if ptr == 8 {
                        pe::IMAGE_ORDINAL_FLAG64
                    } else {
                        pe::IMAGE_ORDINAL_FLAG32 as u64
                    };
                    if field == "OriginalFirstThunk"
                        && entry & ordinal_flag == 0
                        && let Some(hoff) = off_of(entry & 0x7fff_ffff)
                    {
                        let name = ctx.bytes.cstr(hoff + 2, 4096).unwrap_or(&[]);
                        let len = util::align_up(2 + name.len() as u64 + 1, 2);
                        let hid = b.region(
                            hoff,
                            len,
                            RegionKind::Linking,
                            format!("Hint/Name {}", util::lossy(name)),
                        );
                        b.set_decoder(hid, Decoder::PeHintNames);
                    }
                    n += 1;
                }
                let tid = b.region(toff, (n + 1) * ptr, RegionKind::Linking, format!("{label} ({dll})"));
                b.set_value(tid, format!("{n} entries"));
                let mut t = Table::new(TableKind::PeThunks, ptr);
                t.param = ctx.image_base + trva;
                b.set_decoder(tid, Decoder::Table(t));
            }
            pos += 20;
            count += 1;
        }
    }

    // IAT directory spans all per-DLL address tables.
    if let Some((rva, size)) = dir(12)
        && let Some(off) = off_of(rva)
    {
        let id = b.region(off, size, RegionKind::Linking, "Import address table directory");
        b.set_note(id, "Slots the loader overwrites with resolved function addresses");
    }

    if let Some((rva, size)) = dir(0)
        && let Some(off) = off_of(rva)
    {
        exports(b, off, size);
    }

    if let Some((rva, size)) = dir(2)
        && let Some(off) = off_of(rva)
    {
        let id = b.region(off, size, RegionKind::Resources, "Resource directory");
        b.set_note(
            id,
            "Tree of type → name/ID → language → data (icons, manifests, version info)",
        );
        if let Some(id) = id {
            resources(b, id, off);
        }
    }

    if let Some((rva, size)) = dir(3)
        && let Some(off) = off_of(rva)
    {
        let arm64 = machine(&ctx) == pe::IMAGE_FILE_MACHINE_ARM64;
        let id = b.region(off, size, RegionKind::Unwind, "Exception directory");
        b.set_note(id, "RUNTIME_FUNCTION entries: function ranges and their unwind info");
        b.set_decoder(
            id,
            Decoder::Table(Table::new(TableKind::PeRuntimeFunctions, if arm64 { 8 } else { 12 })),
        );
    }

    if let Some((rva, size)) = dir(5)
        && let Some(off) = off_of(rva)
    {
        let id = b.region(off, size, RegionKind::Relocations, "Base relocations");
        b.set_note(
            id,
            "Addresses to patch if the image is not loaded at its preferred base",
        );
        b.set_decoder(id, Decoder::PeBaseRelocs);
    }

    if let Some((rva, size)) = dir(6)
        && let Some(off) = off_of(rva)
    {
        let id = b.region(off, size, RegionKind::Debug, "Debug directory");
        b.set_decoder(id, Decoder::Table(Table::new(TableKind::PeDebugDirectory, 28)));
        for i in 0..(size / 28).min(64) {
            let v = fields::decode(&ctx, off + i * 28, DEBUG_DIRECTORY);
            let (ptr_raw, len) = (fields::get(&v, "PointerToRawData"), fields::get(&v, "SizeOfData"));
            if ptr_raw == 0 || len == 0 {
                continue;
            }
            let typ = debug_type(fields::get(&v, "Type")).unwrap_or("unknown");
            let is_rsds = fields::get(&v, "Type") == pe::IMAGE_DEBUG_TYPE_CODEVIEW.0 as u64
                && ctx.bytes.slice(ptr_raw, 4) == Some(b"RSDS");
            if !is_rsds {
                b.region(ptr_raw, len, RegionKind::Debug, format!("Debug data: {typ}"));
                continue;
            }
            let (cid, _) = b.struct_region(ptr_raw, RegionKind::Debug, "CodeView record (RSDS)", CODEVIEW_RSDS);
            if let (Some(cid), Some(g)) = (cid, ctx.bytes.slice(ptr_raw + 4, 16)) {
                let guid: [u8; 16] = g.try_into().unwrap_or([0; 16]);
                let pdb = ctx.bytes.cstr(ptr_raw + 24, len.saturating_sub(24)).unwrap_or(&[]);
                if let Some(p) = b.child(
                    cid,
                    ptr_raw + 24,
                    len.saturating_sub(24),
                    RegionKind::Debug,
                    "PdbFileName",
                ) {
                    b.node_mut(p).is_field = true;
                    b.set_value(Some(p), util::quote(pdb));
                }
                b.node_mut(cid).end = ptr_raw + len;
                b.set_value(
                    Some(cid),
                    format!("{} age {}", util::guid(&guid), ctx.bytes.u32(ptr_raw + 20).unwrap_or(0)),
                );
                b.set_note(Some(cid), "Identifies the PDB file holding this image's debug info");
            }
        }
    }

    if let Some((rva, _)) = dir(9)
        && let Some(off) = off_of(rva)
    {
        let specs = if ctx.is64 { TLS64 } else { TLS32 };
        let (id, v) = b.struct_region(off, RegionKind::Tls, "TLS directory", specs);
        b.set_note(id, "Thread-local storage template and callbacks");
        let callbacks = fields::get(&v, "AddressOfCallBacks");
        if callbacks != 0
            && let Some(coff) = ctx.va_to_offset(callbacks)
        {
            let mut n = 0;
            while n < 256 && ctx.bytes.uint(coff + n * ptr, ptr as u32).unwrap_or(0) != 0 {
                n += 1;
            }
            let cid = b.region(coff, (n + 1) * ptr, RegionKind::Data, "TLS callbacks");
            let mut t = Table::new(TableKind::Pointers, ptr);
            t.param = callbacks;
            b.set_decoder(cid, Decoder::Table(t));
        }
    }

    if let Some((rva, _)) = dir(10)
        && let Some(off) = off_of(rva)
    {
        let size = ctx.bytes.u32(off).unwrap_or(0) as u64;
        let all = if ctx.is64 { LOAD_CONFIG64 } else { LOAD_CONFIG32 };
        // The structure is versioned by its Size field; only decode what is present.
        let mut n = 0;
        let mut total = 0;
        while n < all.len() && total + all[n].ty.size() <= size {
            total += all[n].ty.size();
            n += 1;
        }
        if let Some(id) = b.region(off, size.max(4), RegionKind::Metadata, "Load config directory") {
            b.fields(id, off, &all[..n]);
            b.set_note(
                Some(id),
                "Security cookie, Control Flow Guard tables and other loader settings",
            );
            if size > total
                && let Some(rest) = b.child(id, off + total, size - total, RegionKind::Metadata, "Further fields")
            {
                b.node_mut(rest).is_field = true;
            }
        }
    }

    if let Some((rva, size)) = dir(13)
        && let Some(off) = off_of(rva)
    {
        let id = b.region(off, size, RegionKind::Linking, "Delay import descriptors");
        b.set_decoder(id, Decoder::Table(Table::new(TableKind::PeDelayImportDescriptors, 32)));
    }

    if let Some((rva, _)) = dir(14)
        && let Some(off) = off_of(rva)
    {
        let (id, v) = b.struct_region(off, RegionKind::Metadata, "CLR runtime header", CLR_HEADER);
        b.set_note(id, ".NET assembly: points at the metadata tables and IL");
        let (mrva, msize) = (
            fields::get(&v, "MetaData.VirtualAddress"),
            fields::get(&v, "MetaData.Size"),
        );
        if let Some(moff) = off_of(mrva) {
            let mid = b.region(moff, msize, RegionKind::Metadata, "CLR metadata");
            if let Some(ver) = ctx.bytes.cstr(moff + 16, 256) {
                b.set_value(mid, util::quote(ver));
            }
        }
    }

    if let Some((rva, size)) = dir(11)
        && let Some(off) = off_of(rva)
    {
        b.region(off, size, RegionKind::Linking, "Bound import table");
    }
}

fn exports(b: &mut Builder, off: u64, size: u64) {
    let ctx = b.ctx;
    let (id, v) = b.struct_region(off, RegionKind::Linking, "Export directory", EXPORT_DIRECTORY);
    let Some(id) = id else { return };
    b.node_mut(id).end = (off + size).min(b.node(id).end.max(off + size));
    let dll = ctx
        .cstr_at_rva(fields::get(&v, "Name"))
        .map(util::lossy)
        .unwrap_or_default();
    b.set_value(
        Some(id),
        format!(
            "{dll}: {} functions, {} names",
            fields::get(&v, "NumberOfFunctions"),
            fields::get(&v, "NumberOfNames")
        ),
    );
    let nfuncs = fields::get(&v, "NumberOfFunctions");
    let nnames = fields::get(&v, "NumberOfNames");
    for (field, count, width, kind, label) in [
        (
            "AddressOfFunctions",
            nfuncs,
            4,
            TableKind::PeExportAddresses,
            "Export address table",
        ),
        (
            "AddressOfNames",
            nnames,
            4,
            TableKind::PeExportNames,
            "Export name pointer table",
        ),
        (
            "AddressOfNameOrdinals",
            nnames,
            2,
            TableKind::PeExportOrdinals,
            "Export ordinal table",
        ),
    ] {
        if let Some(toff) = ctx.rva_to_offset(fields::get(&v, field)) {
            let tid = b.region(toff, count * width, RegionKind::Linking, label);
            let mut t = Table::new(kind, width);
            t.param = fields::get(&v, "Base");
            b.set_decoder(tid, Decoder::Table(t));
        }
    }
    // Name strings usually follow the tables inside the directory.
    if let Some(noff) = ctx.rva_to_offset(fields::get(&v, "Name")) {
        let end = off + size;
        if noff < end && noff > off {
            let sid = b.region(noff, end - noff, RegionKind::Strings, "Export name strings");
            b.set_decoder(sid, Decoder::Strings { skip: 0 });
        }
    }
}

fn resources(b: &mut Builder, root: u32, base: u64) {
    let ctx = b.ctx;
    let limit = b.node(root).end;
    // Iterative walk: (table offset, depth, type id).
    let mut stack = vec![(base, 0u32, None::<u32>, String::new())];
    let mut visited = 0;
    while let Some((toff, depth, type_id, path)) = stack.pop() {
        visited += 1;
        if visited > 4096 || toff + 16 > limit {
            continue;
        }
        let named = ctx.bytes.u16(toff + 12).unwrap_or(0) as u64;
        let ids = ctx.bytes.u16(toff + 14).unwrap_or(0) as u64;
        let n = named + ids;
        let label = match depth {
            0 => "Resource type directory".to_string(),
            1 => format!("Resource name directory ({path})"),
            _ => format!("Resource language directory ({path})"),
        };
        let Some(tid) = b.child(root, toff, 16 + n * 8, RegionKind::Resources, label) else {
            continue;
        };
        b.fields(tid, toff, RESOURCE_DIRECTORY);
        for i in 0..n.min(4096) {
            let eoff = toff + 16 + i * 8;
            let name = ctx.bytes.u32(eoff).unwrap_or(0);
            let data = ctx.bytes.u32(eoff + 4).unwrap_or(0);
            let ident = if name & 0x8000_0000 != 0 {
                let soff = base + (name & 0x7fff_ffff) as u64;
                let len = ctx.bytes.u16(soff).unwrap_or(0) as u64;
                let units: Vec<u16> = (0..len.min(256))
                    .filter_map(|k| ctx.bytes.u16(soff + 2 + k * 2))
                    .collect();
                String::from_utf16_lossy(&units)
            } else if depth == 0 {
                pe::NAMES_RT
                    .name(name as u16)
                    .map_or_else(|| format!("#{name}"), str::to_string)
            } else if depth == 2 {
                format!("lang {name}")
            } else {
                format!("#{name}")
            };
            let (eid, _) = b.struct_child(
                tid,
                eoff,
                RegionKind::Resources,
                format!("Entry {ident}"),
                RESOURCE_ENTRY,
            );
            let child_path = if path.is_empty() {
                ident.clone()
            } else {
                format!("{path} / {ident}")
            };
            let type_id = if depth == 0 { Some(name) } else { type_id };
            if data & 0x8000_0000 != 0 {
                b.set_value(eid, "→ subdirectory");
                stack.push((base + (data & 0x7fff_ffff) as u64, depth + 1, type_id, child_path));
            } else {
                b.set_value(eid, "→ data entry");
                let doff = base + data as u64;
                let (_, dv) = b.struct_child(
                    root,
                    doff,
                    RegionKind::Resources,
                    format!("Resource data entry ({child_path})"),
                    RESOURCE_DATA,
                );
                let (drva, dsize) = (fields::get(&dv, "OffsetToData"), fields::get(&dv, "Size"));
                if let Some(off) = ctx.rva_to_offset(drva) {
                    // Data may live inside or outside the directory range.
                    let inside = off >= b.node(root).start && off + dsize <= b.node(root).end;
                    let label = format!("Resource {child_path}");
                    let rid = if inside {
                        b.child(root, off, dsize, RegionKind::Resources, label)
                    } else {
                        b.region(off, dsize, RegionKind::Resources, label)
                    };
                    let preview = ctx.bytes.slice(off, dsize.min(64)).unwrap_or(&[]);
                    let text = type_id == Some(24) || type_id == Some(23) || preview.starts_with(b"<?xml");
                    if text {
                        b.set_value(rid, util::quote(ctx.bytes.slice(off, dsize.min(200)).unwrap_or(&[])));
                    } else {
                        b.set_value(rid, format!("{dsize} bytes"));
                    }
                }
            }
        }
    }
}

pub(crate) fn table_entry(ctx: &Ctx, t: &Table, index: u64, offset: u64) -> Entry {
    let ptr = ctx.addr_size() as u64;
    match t.kind {
        TableKind::PeImportDescriptors => {
            let f = fields::decode(ctx, offset, IMPORT_DESCRIPTOR);
            if f.iter().all(|v| v.raw == 0) {
                return Entry {
                    name: "Terminator".into(),
                    fields: f,
                    ..Entry::default()
                };
            }
            let dll = ctx
                .cstr_at_rva(fields::get(&f, "Name"))
                .map(util::lossy)
                .unwrap_or_default();
            Entry {
                name: format!("Import descriptor {index}: {dll}"),
                value: Some(dll),
                fields: f,
                ..Entry::default()
            }
        }
        TableKind::PeDelayImportDescriptors => {
            let f = fields::decode(ctx, offset, DELAY_DESCRIPTOR);
            let dll = ctx
                .cstr_at_rva(fields::get(&f, "DllNameRVA"))
                .map(util::lossy)
                .unwrap_or_default();
            Entry {
                name: format!("Delay import {index}: {dll}"),
                value: Some(dll),
                fields: f,
                ..Entry::default()
            }
        }
        TableKind::PeThunks => {
            let v = ctx.bytes.uint(offset, ptr as u32).unwrap_or(0);
            let ordinal_flag = if ptr == 8 {
                pe::IMAGE_ORDINAL_FLAG64
            } else {
                pe::IMAGE_ORDINAL_FLAG32 as u64
            };
            let desc = if v == 0 {
                "(end of table)".to_string()
            } else if v & ordinal_flag != 0 {
                format!("ordinal {}", v & 0xffff)
            } else {
                match ctx.rva_to_offset(v & 0x7fff_ffff) {
                    Some(h) => {
                        let hint = ctx.bytes.u16(h).unwrap_or(0);
                        let name = util::lossy(ctx.bytes.cstr(h + 2, 4096).unwrap_or(&[]));
                        format!("{name} (hint {hint})")
                    }
                    None => hex(v),
                }
            };
            Entry {
                name: format!("Thunk {index}"),
                value: Some(desc.clone()),
                note: (t.param != 0).then(|| format!("slot at {}", hex(t.param + index * ptr))),
                fields: vec![FieldValue {
                    start: offset,
                    end: offset + ptr,
                    name: "u1",
                    raw: v,
                    value: format!("{} → {desc}", hex(v)),
                }],
                ..Entry::default()
            }
        }
        TableKind::PeExportAddresses => {
            let rva = ctx.bytes.u32(offset).unwrap_or(0) as u64;
            let target = ctx.symbolize(ctx.image_base + rva);
            Entry {
                name: format!("Export ordinal {}", t.param + index),
                value: Some(match target {
                    Some(s) => format!("RVA {} → {s}", hex(rva)),
                    None => format!("RVA {}", hex(rva)),
                }),
                fields: vec![FieldValue {
                    start: offset,
                    end: offset + 4,
                    name: "rva",
                    raw: rva,
                    value: hex(rva),
                }],
                ..Entry::default()
            }
        }
        TableKind::PeExportNames => {
            let rva = ctx.bytes.u32(offset).unwrap_or(0) as u64;
            let name = ctx.cstr_at_rva(rva).map(util::lossy).unwrap_or_default();
            Entry {
                name: format!("Export name {index}"),
                value: Some(name.clone()),
                fields: vec![FieldValue {
                    start: offset,
                    end: offset + 4,
                    name: "rva",
                    raw: rva,
                    value: format!("{} → {}", hex(rva), util::quote(name.as_bytes())),
                }],
                ..Entry::default()
            }
        }
        TableKind::PeExportOrdinals => {
            let v = ctx.bytes.u16(offset).unwrap_or(0) as u64;
            Entry {
                name: format!("Name ordinal {index}"),
                value: Some(format!("index {v} (ordinal {})", v + t.param)),
                fields: vec![FieldValue {
                    start: offset,
                    end: offset + 2,
                    name: "ordinal",
                    raw: v,
                    value: v.to_string(),
                }],
                ..Entry::default()
            }
        }
        TableKind::PeRuntimeFunctions => {
            let arm64 = t.size == 8;
            let f = fields::decode(
                ctx,
                offset,
                if arm64 {
                    RUNTIME_FUNCTION_ARM64
                } else {
                    RUNTIME_FUNCTION_X64
                },
            );
            let begin = fields::get(&f, "BeginAddress");
            let func = ctx.symbolize(ctx.image_base + begin);
            let value = if arm64 {
                hex(ctx.image_base + begin)
            } else {
                format!(
                    "{}..{}",
                    hex(ctx.image_base + begin),
                    hex(ctx.image_base + fields::get(&f, "EndAddress"))
                )
            };
            Entry {
                name: format!(
                    "Function {index}{}",
                    func.as_ref().map(|s| format!(": {s}")).unwrap_or_default()
                ),
                value: Some(value),
                fields: f,
                ..Entry::default()
            }
        }
        TableKind::PeDebugDirectory => {
            let f = fields::decode(ctx, offset, DEBUG_DIRECTORY);
            let typ = debug_type(fields::get(&f, "Type")).unwrap_or("unknown").to_string();
            Entry {
                name: format!("Debug entry {index}: {typ}"),
                value: Some(format!("{} bytes", fields::get(&f, "SizeOfData"))),
                fields: f,
                ..Entry::default()
            }
        }
        TableKind::PeSectionRelocs => {
            let f = fields::decode(ctx, offset, COFF_RELOC);
            let sym = fields::get(&f, "SymbolTableIndex");
            let target = coff_symbol_name(ctx, t.param, t.strtab, sym);
            let typ = rel_type(ctx, fields::get(&f, "Type")).unwrap_or_default();
            Entry {
                name: format!("Relocation {index}: {typ}"),
                value: Some(format!(
                    "{} → {}",
                    hex(fields::get(&f, "VirtualAddress")),
                    target.unwrap_or_else(|| format!("symbol {sym}"))
                )),
                fields: f,
                ..Entry::default()
            }
        }
        _ => Entry::default(),
    }
}

fn coff_symbol_name(ctx: &Ctx, symtab: u64, strtab: Option<(u64, u64)>, index: u64) -> Option<String> {
    if symtab == 0 {
        return None;
    }
    let off = symtab + index * 18;
    let raw = ctx.bytes.slice(off, 8)?;
    let name = if raw[..4] == [0, 0, 0, 0] {
        let idx = u32::from_le_bytes(raw[4..8].try_into().ok()?) as u64;
        util::lossy(ctx.with_strtab(strtab).string_at(idx)?)
    } else {
        util::fixed_str(raw)
    };
    Some(util::demangle(&name).unwrap_or(name))
}

/// The symbol record starting at `pos`, with its auxiliary records.
pub(crate) fn coff_symbol_entry(ctx: &Ctx, node: &Node, strtab: Option<(u64, u64)>, pos: u64) -> Option<Entry> {
    if pos + 18 > node.end {
        return None;
    }
    let index = (pos - node.start) / 18;
    let naux = ctx.bytes.u8(pos + 17)? as u64;
    let end = (pos + 18 * (1 + naux)).min(node.end);
    let mut f = fields::decode(ctx, pos, COFF_SYMBOL);
    let name = coff_symbol_name(ctx, node.start, strtab, index).unwrap_or_default();
    if let Some(field) = f.iter_mut().find(|f| f.name == "Name") {
        field.value = util::quote(name.as_bytes());
    }
    if let Some(field) = f.iter_mut().find(|f| f.name == "SectionNumber") {
        field.value = sym_section(ctx, field.raw);
    }
    if naux > 0 {
        f.push(FieldValue {
            start: pos + 18,
            end,
            name: "AuxRecords",
            raw: naux,
            value: format!("{naux} auxiliary record(s)"),
        });
    }
    let class = sym_class(fields::get(&f, "StorageClass")).unwrap_or("?");
    Some(Entry {
        start: pos,
        end,
        name: format!("Symbol {index}: {name}"),
        value: Some(format!("{class} value {}", hex(fields::get(&f, "Value")))),
        fields: f,
        ..Entry::default()
    })
}

pub(crate) fn base_reloc_block(ctx: &Ctx, pos: u64, end: u64) -> Option<Entry> {
    let page = ctx.bytes.u32(pos)? as u64;
    let size = ctx.bytes.u32(pos + 4)? as u64;
    if size < 8 || pos + size > end {
        return None;
    }
    let count = (size - 8) / 2;
    let mut fields = vec![
        FieldValue {
            start: pos,
            end: pos + 4,
            name: "VirtualAddress",
            raw: page,
            value: format!("page RVA {}", hex(page)),
        },
        FieldValue {
            start: pos + 4,
            end: pos + 8,
            name: "SizeOfBlock",
            raw: size,
            value: format!("{size} ({count} entries)"),
        },
    ];
    let names = pe::machine_names(machine(ctx));
    for i in 0..count.min(4096) {
        let o = pos + 8 + i * 2;
        let v = ctx.bytes.u16(o)?;
        let typ = pe::BaseRelocationType(v >> 12);
        let target = page + (v & 0xfff) as u64;
        let tname = names.rel_based.name(typ).unwrap_or("?");
        fields.push(FieldValue {
            start: o,
            end: o + 2,
            name: "TypeOffset",
            raw: v as u64,
            value: if typ.0 == 0 {
                format!("{tname} (padding)")
            } else {
                format!("{tname} at RVA {}", hex(target))
            },
        });
    }
    Some(Entry {
        start: pos,
        end: pos + size,
        name: format!("Relocation block for page {}", hex(page)),
        value: Some(format!("{count} entries")),
        fields,
        ..Entry::default()
    })
}

pub(crate) fn hint_name_at(ctx: &Ctx, node: &Node, _offset: u64) -> Option<Entry> {
    let hint = ctx.bytes.u16(node.start)?;
    let name = ctx.bytes.cstr(node.start + 2, node.end - node.start - 2)?;
    Some(Entry {
        start: node.start,
        end: node.end,
        name: format!("Import {}", util::lossy(name)),
        value: Some(format!("hint {hint}")),
        fields: vec![
            FieldValue {
                start: node.start,
                end: node.start + 2,
                name: "Hint",
                raw: hint as u64,
                value: hint.to_string(),
            },
            FieldValue {
                start: node.start + 2,
                end: node.end,
                name: "Name",
                raw: 0,
                value: util::quote(name),
            },
        ],
        ..Entry::default()
    })
}
