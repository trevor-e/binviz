//! Names for import stubs and pointer slots, which symbol tables leave unnamed:
//!
//! - Mach-O `__stubs` / `__auth_stubs` entries and `__got` / `__la_symbol_ptr`
//!   slots, through the indirect symbol table;
//! - ELF PLT entries and GOT slots, through their `JUMP_SLOT` / `GLOB_DAT`
//!   relocations;
//! - PE import thunks (`jmp [__imp_name]`), through the import address table.
//!
//! With these, a call into a stub reads as a call to the function it imports.

use std::collections::HashMap;

use iced_x86::{Mnemonic, OpKind, Register};
use object::{Architecture, Object, ObjectSection, ObjectSymbol, ObjectSymbolTable};

use crate::model::{Format, Import, Section};
use crate::util::Bytes;

pub(crate) struct Stub {
    pub address: u64,
    pub size: u64,
    pub name: String,
    /// Code (a stub, a PLT entry, a thunk) rather than a pointer slot.
    pub code: bool,
}

pub(crate) fn import_stubs(
    file: &object::File<'_>,
    format: Format,
    b: &Bytes,
    sections: &[Section],
    imports: &[Import],
    is64: bool,
) -> Vec<Stub> {
    let mut out = match format {
        Format::MachO => macho(b, is64),
        Format::Elf => elf(file, sections, is64),
        Format::Pe => pe_thunks(b, file.architecture(), sections, imports, is64),
        _ => Vec::new(),
    };
    out.sort_by_key(|s| s.address);
    out.dedup_by_key(|s| s.address);
    out
}

/// Mach-O: each entry of a stub or pointer section names its import through
/// `reserved1`, the section's first index into the indirect symbol table.
fn macho(b: &Bytes, is64: bool) -> Vec<Stub> {
    use object::macho;
    let mut out = Vec::new();
    let ncmds = b.u32(16).unwrap_or(0);
    let mut off: u64 = if is64 { 32 } else { 28 };
    let (mut symoff, mut nsyms, mut stroff, mut strsize) = (0u64, 0u64, 0u64, 0u64);
    let (mut indoff, mut nind) = (0u64, 0u64);
    // (address, size, flags, reserved1, reserved2)
    let mut secs: Vec<(u64, u64, u32, u32, u32)> = Vec::new();
    for _ in 0..ncmds.min(65536) {
        let (Some(cmd), Some(size)) = (b.u32(off), b.u32(off + 4)) else {
            break;
        };
        if cmd == macho::LC_SYMTAB.0 {
            symoff = b.u32(off + 8).unwrap_or(0) as u64;
            nsyms = b.u32(off + 12).unwrap_or(0) as u64;
            stroff = b.u32(off + 16).unwrap_or(0) as u64;
            strsize = b.u32(off + 20).unwrap_or(0) as u64;
        } else if cmd == macho::LC_DYSYMTAB.0 {
            indoff = b.u32(off + 56).unwrap_or(0) as u64;
            nind = b.u32(off + 60).unwrap_or(0) as u64;
        } else if cmd == macho::LC_SEGMENT_64.0 || cmd == macho::LC_SEGMENT.0 {
            let wide = cmd == macho::LC_SEGMENT_64.0;
            let nsects = b.u32(off + if wide { 64 } else { 48 }).unwrap_or(0) as u64;
            let (first, each) = if wide { (72, 80) } else { (56, 68) };
            for i in 0..nsects.min(4096) {
                let s = off + first + i * each;
                let (addr, size, rest) = if wide {
                    (b.u64(s + 32), b.u64(s + 40), s + 64)
                } else {
                    (b.u32(s + 32).map(u64::from), b.u32(s + 36).map(u64::from), s + 56)
                };
                if let (Some(addr), Some(size), Some(flags), Some(r1), Some(r2)) =
                    (addr, size, b.u32(rest), b.u32(rest + 4), b.u32(rest + 8))
                {
                    secs.push((addr, size, flags, r1, r2));
                }
            }
        }
        if size < 8 {
            break;
        }
        off += size as u64;
    }
    if nind == 0 || nsyms == 0 {
        return out;
    }
    let nlist = if is64 { 16 } else { 12 };
    let ptr = if is64 { 8 } else { 4 };
    let name_of = |k: u64| -> Option<String> {
        if k >= nind {
            return None;
        }
        let sym = b.u32(indoff + 4 * k)?;
        // INDIRECT_SYMBOL_LOCAL / INDIRECT_SYMBOL_ABS: no import behind it.
        if sym & 0xC000_0000 != 0 || sym as u64 >= nsyms {
            return None;
        }
        let strx = b.u32(symoff + sym as u64 * nlist)? as u64;
        if strx == 0 || strx >= strsize {
            return None;
        }
        let name = b.cstr(stroff + strx, (strsize - strx).min(4096))?;
        (!name.is_empty()).then(|| crate::util::lossy(name))
    };
    for (addr, size, flags, r1, r2) in secs {
        let typ = macho::SectionFlags(flags).typ();
        let (entry, code) = match typ {
            macho::S_SYMBOL_STUBS => (r2 as u64, true),
            macho::S_NON_LAZY_SYMBOL_POINTERS
            | macho::S_LAZY_SYMBOL_POINTERS
            | macho::S_LAZY_DYLIB_SYMBOL_POINTERS
            | macho::S_THREAD_LOCAL_VARIABLE_POINTERS => (ptr, false),
            _ => continue,
        };
        if entry == 0 {
            continue;
        }
        for i in 0..(size / entry).min(1 << 22) {
            if let Some(name) = name_of(r1 as u64 + i) {
                out.push(Stub {
                    address: addr + i * entry,
                    size: entry,
                    name: if code { name } else { format!("{name}@got") },
                    code,
                });
            }
        }
    }
    out
}

/// ELF: GOT slots are named by their relocations, and PLT entries by the slot
/// they jump through.
fn elf(file: &object::File<'_>, sections: &[Section], is64: bool) -> Vec<Stub> {
    use object::elf;
    let mut out = Vec::new();
    let arch = file.architecture();
    let Some(relocs) = file.dynamic_relocations() else {
        return out;
    };
    let dynsyms = file.dynamic_symbol_table();
    let mut slots: HashMap<u64, String> = HashMap::new();
    for (offset, r) in relocs {
        let object::RelocationFlags::Elf { r_type } = r.flags() else {
            continue;
        };
        let slot = matches!(
            (arch, r_type),
            (Architecture::X86_64, elf::R_X86_64_JUMP_SLOT | elf::R_X86_64_GLOB_DAT)
                | (Architecture::I386, elf::R_386_JMP_SLOT | elf::R_386_GLOB_DAT)
                | (
                    Architecture::Aarch64,
                    elf::R_AARCH64_JUMP_SLOT | elf::R_AARCH64_GLOB_DAT
                )
                | (Architecture::Arm, elf::R_ARM_JUMP_SLOT | elf::R_ARM_GLOB_DAT)
        );
        if !slot {
            continue;
        }
        let object::RelocationTarget::Symbol(index) = r.target() else {
            continue;
        };
        let Some(name) = dynsyms
            .as_ref()
            .and_then(|t| t.symbol_by_index(index).ok())
            .and_then(|s| s.name().ok().map(str::to_string))
            .filter(|n| !n.is_empty())
        else {
            continue;
        };
        slots.insert(offset, name);
    }
    if slots.is_empty() {
        return out;
    }
    let ptr = if is64 { 8 } else { 4 };
    for (&address, name) in &slots {
        out.push(Stub {
            address,
            size: ptr,
            name: format!("{name}@got"),
            code: false,
        });
    }
    let got_plt = sections
        .iter()
        .find(|s| s.name == ".got.plt")
        .or_else(|| sections.iter().find(|s| s.name == ".got"))
        .map(|s| s.address);
    for sec in file.sections() {
        let name = sec.name().unwrap_or("");
        if !matches!(name, ".plt" | ".plt.sec" | ".plt.got" | ".iplt") {
            continue;
        }
        let Ok(bytes) = sec.data() else { continue };
        let mut entries: Vec<(u64, &str)> = Vec::new();
        match arch {
            Architecture::X86_64 | Architecture::I386 => {
                let bits = if arch == Architecture::X86_64 { 64 } else { 32 };
                let mut decoder =
                    iced_x86::Decoder::with_ip(bits, bytes, sec.address(), iced_x86::DecoderOptions::NONE);
                let mut endbr: Option<u64> = None;
                for ins in &mut decoder {
                    if matches!(ins.mnemonic(), Mnemonic::Endbr64 | Mnemonic::Endbr32) {
                        endbr = Some(ins.ip());
                        continue;
                    }
                    if ins.mnemonic() == Mnemonic::Jmp && ins.op0_kind() == OpKind::Memory {
                        let slot = if ins.is_ip_rel_memory_operand() {
                            Some(ins.ip_rel_memory_address())
                        } else if ins.memory_index() == Register::None {
                            match ins.memory_base() {
                                Register::None => Some(ins.memory_displacement64()),
                                Register::EBX => got_plt.map(|g| g.wrapping_add(ins.memory_displacement64())),
                                _ => None,
                            }
                        } else {
                            None
                        };
                        if let Some(name) = slot.and_then(|s| slots.get(&s)) {
                            let start = endbr.filter(|&e| e + 4 == ins.ip()).unwrap_or(ins.ip());
                            entries.push((start, name));
                        }
                    }
                    endbr = None;
                }
            }
            Architecture::Aarch64 => {
                // adrp x16, slot@page; ldr x17, [x16, slot@pageoff]; add x16, ...; br x17
                let words: Vec<u32> = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|w| u32::from_le_bytes(*w))
                    .collect();
                for i in 0..words.len().saturating_sub(1) {
                    let (w, v) = (words[i], words[i + 1]);
                    if w & 0x9F00_001F != 0x9000_0010 || v & 0xFFC0_03FF != 0xF940_0211 {
                        continue;
                    }
                    let pc = sec.address() + 4 * i as u64;
                    let imm = (((w >> 5) & 0x7FFFF) << 2 | ((w >> 29) & 3)) as u64;
                    let page = (pc & !0xFFF).wrapping_add(((imm << 43) as i64 >> 43 << 12) as u64);
                    let slot = page + ((v >> 10) & 0xFFF) as u64 * 8;
                    if let Some(name) = slots.get(&slot) {
                        // A `bti c` landing pad before the adrp belongs to the entry.
                        let start = if i > 0 && words[i - 1] == 0xD503_245F {
                            pc - 4
                        } else {
                            pc
                        };
                        entries.push((start, name));
                    }
                }
            }
            _ => {}
        }
        let end = sec.address() + bytes.len() as u64;
        for (k, &(start, name)) in entries.iter().enumerate() {
            let next = entries.get(k + 1).map_or(end, |e| e.0);
            out.push(Stub {
                address: start,
                size: next.saturating_sub(start).clamp(1, 16),
                name: format!("{name}@plt"),
                code: true,
            });
        }
    }
    out
}

/// PE: `jmp [slot]` thunks in code, where the slot is an import address table
/// entry. (Compilers call through the table directly; linkers add thunks for
/// calls that were not declared as imports.)
fn pe_thunks(b: &Bytes, arch: Architecture, sections: &[Section], imports: &[Import], is64: bool) -> Vec<Stub> {
    let mut out = Vec::new();
    let iat: HashMap<u64, &str> = imports
        .iter()
        .filter_map(|i| Some((i.address?, i.name.as_str())))
        .collect();
    if iat.is_empty() || !matches!(arch, Architecture::X86_64 | Architecture::I386) {
        return out;
    }
    for sec in sections
        .iter()
        .filter(|s| s.kind == crate::model::RegionKind::Code && s.loaded)
    {
        let Some(bytes) = sec.file_offset.and_then(|o| b.slice(o, sec.file_size.min(sec.size))) else {
            continue;
        };
        for pos in memchr::memmem::find_iter(bytes, &[0xFF, 0x25]) {
            let Some(disp) = bytes.get(pos + 2..pos + 6) else { break };
            let disp = u32::from_le_bytes(disp.try_into().expect("4 bytes"));
            let at = sec.address + pos as u64;
            let slot = if is64 {
                (at + 6).wrapping_add_signed(disp as i32 as i64)
            } else {
                disp as u64
            };
            if let Some(name) = iat.get(&slot) {
                out.push(Stub {
                    address: at,
                    size: 6,
                    name: name.to_string(),
                    code: true,
                });
            }
        }
    }
    out
}
