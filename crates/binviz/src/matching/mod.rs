//! Matching a decompilation's rebuilt code against the original: the
//! compiler's object file, function by function, against the functions of
//! the binary it is meant to reproduce, with the fields the linker fills in
//! (relocations: call targets, globals' addresses, the halves of an address)
//! masked and checked against where the original points, and each
//! difference explained (a register chosen differently, a stack slot at
//! another offset, an instruction reordered, a `nop` missing from a delay
//! slot) so that whoever is writing the C knows what to change. A project's
//! objects are matched unit by unit, and summed up as objdiff does.
//!
//! Also reads objdiff's report (what decomp.dev shows) to place its verdicts
//! on the binary's functions.
//!
//! MIPS (PlayStation, Nintendo 64) objects in ELF; x86 and x86-64 ones in
//! COFF (MSVC, clang-cl) or ELF, lined up and explained by `x86.rs`.

mod x86;

pub(crate) use x86::padding_only;
#[cfg(test)]
pub(crate) use x86::tests as x86_tests;

use std::collections::BTreeMap;
use std::sync::Arc;

use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
use serde::{Deserialize, Serialize};

use crate::binary::Binary;
use crate::cpu::mips::MipsWord;
use crate::error::{Error, Result};
use crate::fndiff::{Edit, LineKind, line_up};

/// A function in a compiled object file.
#[derive(Debug, Clone)]
pub struct ObjectFunction {
    pub name: String,
    /// Its offset in its section.
    pub offset: u64,
    /// Its code, in the object's byte order; for x86, without the alignment
    /// padding that follows it.
    pub code: Vec<u8>,
    pub isa: ObjectIsa,
    /// Relocations, what the linker fills in, by the offset in `code` of the
    /// field each fills (for MIPS, the instruction word's).
    pub relocs: BTreeMap<u64, Reloc>,
    /// Its section's index in the object.
    pub(crate) section: usize,
    /// The functions of its section (offset, name), for a call the assembler
    /// resolved itself, with no relocation.
    pub(crate) neighbours: Arc<[(u64, String)]>,
    /// The object's data, for what a reference to a section of it (a static,
    /// a string, a constant) reaches.
    pub(crate) data: Arc<ObjectData>,
}

/// What an object holds besides its code.
#[derive(Debug, Default)]
pub(crate) struct ObjectData {
    /// Its data sections by index: name and bytes (none for zero-filled ones).
    pub sections: std::collections::HashMap<usize, (String, Vec<u8>)>,
    /// Its named data: (section, offset, size, name).
    pub symbols: Vec<(usize, u64, u64, String)>,
    /// Where in its data sections the linker writes (a jump table's entries):
    /// those bytes aren't the object's to compare.
    pub relocated: std::collections::HashMap<usize, Vec<u64>>,
}

impl ObjectData {
    fn read(file: &object::File) -> ObjectData {
        let mut out = ObjectData::default();
        for section in file.sections() {
            if matches!(
                section.kind(),
                object::SectionKind::Text | object::SectionKind::Metadata | object::SectionKind::Debug
            ) || section
                .name()
                .is_ok_and(|n| n.starts_with(".debug") || n.starts_with(".rela"))
            {
                continue;
            }
            let bytes = section.uncompressed_data().map(|d| d.into_owned()).unwrap_or_default();
            out.sections
                .insert(section.index().0, (section.name().unwrap_or("").to_string(), bytes));
            let mut at: Vec<u64> = section.relocations().map(|(o, _)| o).collect();
            at.sort_unstable();
            out.relocated.insert(section.index().0, at);
        }
        for s in file.symbols() {
            let (Some(x), Ok(name)) = (s.section_index(), s.name()) else {
                continue;
            };
            // An assembler's local labels (`.LCPI0_0`, `.L.str`) name no data the binary knows.
            if name.is_empty()
                || name.starts_with(".L")
                || s.kind() == object::SymbolKind::Section
                || !out.sections.contains_key(&x.0)
            {
                continue;
            }
            let base = file.section_by_index(x).map_or(0, |sec| sec.address());
            out.symbols
                .push((x.0, s.address().wrapping_sub(base), s.size(), name.to_string()));
        }
        out
    }

    /// The named data at `offset` in section `section`: its name and the offset into it.
    pub(crate) fn symbol_at(&self, section: usize, offset: u64) -> Option<(&str, u64)> {
        self.symbols
            .iter()
            .filter(|s| s.0 == section && s.1 <= offset && offset < s.1 + s.2.max(1))
            .min_by_key(|s| s.2)
            .map(|s| (s.3.as_str(), offset - s.1))
    }

    /// The anonymous data at `offset` in section `section` as the code reads
    /// it: a string (to its NUL) in a string section, a constant (its size
    /// from a `.rodata.cst8`-style name, else 8 bytes) otherwise.
    pub(crate) fn bytes_at(&self, section: usize, offset: u64) -> Option<(&[u8], bool)> {
        let (name, bytes) = self.sections.get(&section)?;
        let rest = bytes.get(offset as usize..)?;
        if name.contains(".str") {
            let n = rest.iter().position(|&b| b == 0)?;
            return Some((&rest[..n], true));
        }
        let size = name
            .rsplit("cst")
            .next()
            .filter(|_| name.contains(".cst"))
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(8);
        let ours = rest.get(..size).unwrap_or(rest);
        // Bytes the linker writes aren't known here.
        let written = self.relocated.get(&section).is_some_and(|r| {
            let i = r.partition_point(|&o| o + 8 <= offset);
            r.get(i).is_some_and(|&o| o < offset + ours.len() as u64)
        });
        (!written).then_some((ours, false))
    }
}

/// The instruction set an object's code is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectIsa {
    Mips {
        big_endian: bool,
    },
    /// x86 (32) or x86-64 (64).
    X86 {
        bits: u32,
    },
}

/// What a relocated field holds once linked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelocForm {
    /// The symbol's address, plus the addend.
    Absolute,
    /// That less the field's own address: a call's or a jump's displacement, `[rip+N]`.
    Relative,
    /// Something else (a MIPS address half, an image offset, a GOT slot): masked, and
    /// checked only where the instruction set's matcher knows the kind.
    Other,
}

#[derive(Debug, Clone)]
pub struct Reloc {
    /// The relocation type in its format: ELF's `r_type` (`R_MIPS_26`,
    /// `R_386_PC32`…) or COFF's (`IMAGE_REL_I386_REL32`…).
    pub kind: u32,
    pub form: RelocForm,
    /// The width of the field, in bits.
    pub bits: u8,
    pub symbol: String,
    /// Added to the symbol's address. Formats that keep the addend in the
    /// field itself (ELF's REL, COFF) set `implicit`: the field's value is
    /// added too.
    pub addend: i64,
    pub implicit: bool,
    /// Where the symbol is when this object defines it: its section's index
    /// and its offset there.
    pub(crate) defined: Option<(usize, u64)>,
    /// The symbol stands for a section of this object (a static's data, a
    /// jump table in `.rdata`), a name the binary can't know.
    pub(crate) section_symbol: bool,
    /// The field holds the symbol's offset from the image base (COFF's
    /// `ADDR32NB`: MSVC's x64 jump tables and the code reading them).
    pub(crate) image_offset: bool,
}

/// One instruction of the original lined up with one of the rebuild.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchLine {
    pub kind: LineKind,
    pub address: Option<u64>,
    pub original: Option<String>,
    pub rebuilt: Option<String>,
    /// Why they differ, when they do.
    pub note: Option<String>,
}

/// How well a rebuilt function matches the original.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchResult {
    pub name: String,
    pub address: u64,
    pub original_instructions: u32,
    pub rebuilt_instructions: u32,
    pub matched_instructions: u32,
    /// The bytes of code compared on each side.
    pub original_bytes: u64,
    pub rebuilt_bytes: u64,
    /// 100 when every instruction matches, relocations aside.
    pub percent: f32,
    pub lines: Vec<MatchLine>,
    /// Kinds of difference, with how many of each.
    pub differences: Vec<(String, u32)>,
    /// What the original's code says about the compiler that built it (MIPS: the
    /// shape of its epilogue), and what the rebuild's says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_compiler: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebuilt_compiler: Option<&'static str>,
}

/// What a MIPS function's epilogue says about the compiler that built it.
/// GCC 2.7.2 (the PlayStation's, `lw $ra; addiu $sp; jr $ra; nop`) pops the
/// frame before the return and leaves the delay slot empty; GCC 2.8 and
/// later (and maspsx) pop it in the delay slot (`jr $ra; addiu $sp`). None
/// for a function with no frame to pop, or no `jr $ra`.
pub fn compiler_tell(words: &[u32]) -> Option<&'static str> {
    let pops_sp = |w: u32| w & 0xFFFF_0000 == 0x27BD_0000 && ((w & 0xFFFF) as u16 as i16) > 0;
    let mut tell = None;
    for (i, &w) in words.iter().enumerate() {
        if w != 0x03E0_0008 {
            continue;
        }
        let slot = words.get(i + 1).copied().unwrap_or(0);
        if pops_sp(slot) {
            tell = Some("GCC 2.8 or later (the frame is popped in the return's delay slot)");
        } else if slot == 0 && words[i.saturating_sub(3)..i].iter().any(|&w| pops_sp(w)) {
            return Some("GCC 2.7.2 (the frame is popped before the return, a nop in its delay slot)");
        }
    }
    tell
}

/// MIPS ELF relocation types.
mod r {
    pub const MIPS_16: u32 = 1;
    pub const MIPS_32: u32 = 2;
    pub const MIPS_26: u32 = 4;
    pub const MIPS_HI16: u32 = 5;
    pub const MIPS_LO16: u32 = 6;
    pub const MIPS_GPREL16: u32 = 7;
    pub const MIPS_LITERAL: u32 = 8;
    pub const MIPS_PC16: u32 = 10;
}

/// The bits of an instruction a relocation of `kind` fills in.
fn reloc_mask(kind: u32) -> u32 {
    match kind {
        r::MIPS_26 => 0x03FF_FFFF,
        r::MIPS_32 => 0xFFFF_FFFF,
        r::MIPS_16 | r::MIPS_HI16 | r::MIPS_LO16 | r::MIPS_GPREL16 | r::MIPS_LITERAL | r::MIPS_PC16 => 0xFFFF,
        _ => 0,
    }
}

/// The bits of an instruction that depend on where things were placed:
/// jump targets and immediates (addresses' halves, branch offsets).
fn layout_mask(w: MipsWord) -> u32 {
    match w.op() {
        2 | 3 => 0x03FF_FFFF,
        1 | 4..=15 | 20..=25 | 32..=63 => 0xFFFF,
        _ => 0,
    }
}

/// The functions of a compiled object file (ELF or COFF; MIPS, x86 or
/// x86-64), with their relocations.
pub fn object_functions(bytes: &[u8]) -> Result<Vec<ObjectFunction>> {
    let file = object::File::parse(bytes).map_err(|e| Error::new(format!("not an object file: {e}")))?;
    file_functions(&file)
}

/// The functions of a parsed object file (see [`object_functions`]).
pub(crate) fn file_functions(file: &object::File) -> Result<Vec<ObjectFunction>> {
    let isa = match file.architecture() {
        object::Architecture::Mips => ObjectIsa::Mips {
            big_endian: !file.is_little_endian(),
        },
        object::Architecture::I386 => ObjectIsa::X86 { bits: 32 },
        object::Architecture::X86_64 | object::Architecture::X86_64_X32 => ObjectIsa::X86 { bits: 64 },
        other => {
            return Err(Error::new(format!(
                "matching is for MIPS, x86 and x86-64 objects; this one is {other:?}"
            )));
        }
    };
    let mut out = Vec::new();
    let object_data = Arc::new(ObjectData::read(file));
    for section in file.sections() {
        if section.kind() != object::SectionKind::Text {
            continue;
        }
        let data = section.data().map_err(|e| Error::new(e.to_string()))?;
        let relocs = section_relocs(file, &section);
        // Where functions start: MIPS keeps to typed functions; for x86, the
        // global symbols of hand-written assembly count too, and labels
        // inside functions (MSVC's `$LN` ones) don't.
        let starts = |s: &object::Symbol| match isa {
            ObjectIsa::Mips { .. } => s.kind() == object::SymbolKind::Text,
            ObjectIsa::X86 { .. } => {
                s.kind() == object::SymbolKind::Text
                    || (s.is_global() && matches!(s.kind(), object::SymbolKind::Data | object::SymbolKind::Unknown))
            }
        };
        let mut symbols: Vec<(u64, u64, String)> = file
            .symbols()
            .filter(|s| s.section_index() == Some(section.index()) && starts(s))
            .filter(|s| !s.name().unwrap_or("").is_empty())
            .map(|s| {
                let offset = s.address().wrapping_sub(section.address());
                (offset, s.size(), s.name().unwrap_or("").to_string())
            })
            .collect();
        symbols.sort();
        let neighbours: Arc<[(u64, String)]> = symbols.iter().map(|(o, _, n)| (*o, n.clone())).collect();
        for (offset, size, name) in &symbols {
            let end = if *size > 0 {
                offset + size
            } else {
                // The next function, past any others at the same place (aliases).
                symbols
                    .iter()
                    .map(|s| s.0)
                    .find(|&o| o > *offset)
                    .unwrap_or(data.len() as u64)
            };
            let Some(code) = data.get(*offset as usize..end as usize) else {
                continue;
            };
            let mut relocs: BTreeMap<u64, Reloc> = relocs
                .range(*offset..end)
                .map(|(at, r)| (at - offset, r.clone()))
                .collect();
            let len = match isa {
                ObjectIsa::Mips { .. } => code.len(),
                // The last relocated field stays in, whatever follows it.
                ObjectIsa::X86 { bits } => {
                    let fields = relocs
                        .iter()
                        .map(|(at, r)| *at as usize + usize::from(r.bits).div_ceil(8))
                        .max();
                    x86::code_len(code, bits).max(fields.unwrap_or(0)).min(code.len())
                }
            };
            relocs.retain(|at, _| (*at as usize) < len);
            out.push(ObjectFunction {
                name: name.clone(),
                offset: *offset,
                code: code[..len].to_vec(),
                isa,
                relocs,
                section: section.index().0,
                neighbours: neighbours.clone(),
                data: object_data.clone(),
            });
        }
    }
    Ok(out)
}

/// A section's relocations, by the offset of the field each fills.
fn section_relocs(file: &object::File, section: &object::Section) -> BTreeMap<u64, Reloc> {
    let mut out = BTreeMap::new();
    for (offset, rel) in section.relocations() {
        let kind = match rel.flags() {
            object::RelocationFlags::Elf { r_type } => r_type.0,
            object::RelocationFlags::Coff { typ } => u32::from(typ.0),
            _ => continue,
        };
        let (symbol, defined, section_symbol) = match rel.target() {
            RelocationTarget::Symbol(i) => match file.symbol_by_index(i) {
                Ok(s) => {
                    let defined = s.section_index().and_then(|x| {
                        let base = file.section_by_index(x).ok()?.address();
                        Some((x.0, s.address().wrapping_sub(base)))
                    });
                    // A section symbol is shown as its section's name.
                    let section_symbol = s.kind() == object::SymbolKind::Section;
                    let name = match s.name() {
                        Ok(n) if !n.is_empty() => n.to_string(),
                        _ => s
                            .section_index()
                            .and_then(|x| file.section_by_index(x).ok())
                            .and_then(|x| x.name().ok().map(str::to_string))
                            .unwrap_or_default(),
                    };
                    (name, defined, section_symbol)
                }
                Err(_) => (String::new(), None, false),
            },
            _ => (String::new(), None, false),
        };
        let form = match rel.kind() {
            object::RelocationKind::Absolute => RelocForm::Absolute,
            object::RelocationKind::Relative | object::RelocationKind::PltRelative => RelocForm::Relative,
            _ => RelocForm::Other,
        };
        // Unknown kinds say no width: an x86 field is 32 bits unless said otherwise.
        let bits = match rel.size() {
            0 => 32,
            n => n,
        };
        out.insert(
            offset,
            Reloc {
                kind,
                form,
                bits,
                symbol,
                addend: rel.addend(),
                implicit: rel.has_implicit_addend(),
                defined,
                section_symbol,
                image_offset: rel.kind() == object::RelocationKind::ImageOffset,
            },
        );
    }
    out
}

/// Where names of the objects are in the binary, found once each.
#[derive(Default)]
struct Lookups(std::cell::RefCell<std::collections::HashMap<String, Option<u64>>>);

/// A function's code as instruction words (MIPS), with its relocations by
/// instruction index.
struct Words {
    words: Vec<u32>,
    big_endian: bool,
    relocs: BTreeMap<usize, Reloc>,
}

impl Words {
    fn of(f: &ObjectFunction, big_endian: bool) -> Words {
        let words = f
            .code
            .chunks_exact(4)
            .map(|c| {
                let b = [c[0], c[1], c[2], c[3]];
                if big_endian {
                    u32::from_be_bytes(b)
                } else {
                    u32::from_le_bytes(b)
                }
            })
            .collect();
        let relocs = f.relocs.iter().map(|(at, r)| ((at / 4) as usize, r.clone())).collect();
        Words {
            words,
            big_endian,
            relocs,
        }
    }
}

impl ObjectFunction {
    /// Which bytes of `code` to compare: 0 where the linker fills them in.
    pub fn mask(&self) -> Vec<u8> {
        let mut mask = vec![0xFF; self.code.len()];
        for (&at, r) in &self.relocs {
            let at = at as usize;
            match self.isa {
                ObjectIsa::Mips { big_endian } => {
                    let bits = reloc_mask(r.kind);
                    let bytes = if big_endian {
                        bits.to_be_bytes()
                    } else {
                        bits.to_le_bytes()
                    };
                    for (i, b) in bytes.iter().enumerate() {
                        if let Some(m) = mask.get_mut(at + i) {
                            *m &= !b;
                        }
                    }
                }
                ObjectIsa::X86 { .. } => {
                    for m in mask.iter_mut().skip(at).take(usize::from(r.bits).div_ceil(8)) {
                        *m = 0;
                    }
                }
            }
        }
        mask
    }
}

fn text(w: u32, pc: u64, big: bool) -> String {
    let bytes = if big { w.to_be_bytes() } else { w.to_le_bytes() };
    let mut state = crate::cpu::State::default();
    let i = crate::cpu::mips::decode(&bytes, pc, &mut state, big);
    format!("{} {}", i.mnemonic, i.operands).trim_end().to_string()
}

impl Binary {
    /// The original's words for the function at `address`.
    fn function_words(&self, address: u64) -> Option<(String, u64, Vec<u32>, bool)> {
        let f = self.symbols().function_containing(address)?;
        let big = self.endian == crate::util::Endian::Big;
        let bytes = self.code_bytes(f.address)?;
        let words = bytes
            .get(..(f.size as usize).min(bytes.len()))?
            .chunks_exact(4)
            .map(|c| {
                let b = [c[0], c[1], c[2], c[3]];
                if big { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) }
            })
            .collect();
        Some((f.display_name().into_owned(), f.address, words, big))
    }

    /// Where the rebuild's symbol `name` is in the original, if known: from
    /// the file's symbols and the user's names, by the name as the object
    /// has it or as C or C++ source would write it (see [`source_names`]).
    pub fn object_symbol_address(&self, name: &str) -> Option<u64> {
        source_names(name)
            .iter()
            .find_map(|n| self.symbols().by_name(n))
            .map(|s| s.address)
    }

    /// [`Binary::object_symbol_address`], each name looked up once: a name
    /// the binary doesn't have (a string literal's, say) costs a search of
    /// every symbol, and an object's functions share most of theirs.
    fn cached_symbol_address(&self, cache: &Lookups, name: &str) -> Option<u64> {
        if let Some(&address) = cache.0.borrow().get(name) {
            return address;
        }
        let address = self.object_symbol_address(name);
        cache.0.borrow_mut().insert(name.to_string(), address);
        address
    }

    /// Whether this binary's code is in the instruction set of an object's.
    pub fn check_isa(&self, isa: ObjectIsa) -> Result<()> {
        let fits = match isa {
            ObjectIsa::Mips { .. } => matches!(self.arch, object::Architecture::Mips | object::Architecture::Mips64),
            ObjectIsa::X86 { bits: 32 } => self.arch == object::Architecture::I386,
            ObjectIsa::X86 { .. } => {
                matches!(
                    self.arch,
                    object::Architecture::X86_64 | object::Architecture::X86_64_X32
                )
            }
        };
        if fits {
            Ok(())
        } else {
            let object = match isa {
                ObjectIsa::Mips { .. } => "MIPS",
                ObjectIsa::X86 { bits: 32 } => "x86",
                ObjectIsa::X86 { .. } => "x86-64",
            };
            Err(Error::new(format!(
                "the object's code is {object}, the binary's {}",
                self.summary().arch
            )))
        }
    }

    /// The rebuilt `func` against the original's function at `address`.
    pub fn match_function(&self, address: u64, func: &ObjectFunction) -> Option<MatchResult> {
        self.match_function_with(address, func, &Lookups::default())
    }

    fn match_function_with(&self, address: u64, func: &ObjectFunction, lookups: &Lookups) -> Option<MatchResult> {
        match func.isa {
            ObjectIsa::Mips { big_endian } => self.match_mips(address, &Words::of(func, big_endian)),
            ObjectIsa::X86 { bits } => self.match_x86(address, func, bits, lookups),
        }
    }

    fn match_mips(&self, address: u64, func: &Words) -> Option<MatchResult> {
        let (name, start, orig, big) = self.function_words(address)?;
        Some(self.match_mips_words(name, start, &orig, big, func))
    }

    /// The rebuilt `func` against the original's code in `start..end`,
    /// whatever the notes say the function's extent is: for a function the
    /// analysis split or merged wrongly, or a piece of a huge one (MIPS).
    pub fn match_range(&self, start: u64, end: u64, func: &ObjectFunction) -> Result<MatchResult> {
        let ObjectIsa::Mips { big_endian } = func.isa else {
            return Err(Error::new("a range is compared for MIPS objects only"));
        };
        if end <= start || !start.is_multiple_of(4) {
            return Err(Error::new(format!("not a range of MIPS code: {start:#x}..{end:#x}")));
        }
        let big = self.endian == crate::util::Endian::Big;
        let bytes = self
            .code_bytes(start)
            .filter(|b| b.len() as u64 >= end - start)
            .ok_or_else(|| Error::new(format!("{start:#x}..{end:#x} is not all in the file's code")))?;
        let orig: Vec<u32> = bytes[..(end - start) as usize]
            .chunks_exact(4)
            .map(|c| {
                let b = [c[0], c[1], c[2], c[3]];
                if big {
                    u32::from_be_bytes(b)
                } else {
                    u32::from_le_bytes(b)
                }
            })
            .collect();
        let name = match self.symbols().function_containing(start) {
            Some(f) if f.address == start => format!("{} ({start:#x}..{end:#x})", f.display_name()),
            Some(f) => format!("{}+{:#x} ({start:#x}..{end:#x})", f.display_name(), start - f.address),
            None => format!("{start:#x}..{end:#x}"),
        };
        Ok(self.match_mips_words(name, start, &orig, big, &Words::of(func, big_endian)))
    }

    fn match_mips_words(&self, name: String, start: u64, orig: &[u32], big: bool, func: &Words) -> MatchResult {
        let cand = &func.words;
        // Lined up by their shapes: everything the layout decides masked off.
        let shape = |w: u32| w & !layout_mask(MipsWord(w));
        let ta: Vec<u32> = orig.iter().map(|&w| shape(w)).collect();
        let tb: Vec<u32> = cand.iter().map(|&w| shape(w)).collect();
        let script = line_up(&ta, &tb, 4000);
        let mut lines = Vec::new();
        let mut counts: BTreeMap<String, u32> = BTreeMap::new();
        let mut matched = 0u32;
        let (mut i, mut j) = (0usize, 0usize);
        let mut k = 0;
        while k < script.len() {
            match script[k] {
                Edit::Keep => {
                    let (a, b) = (orig[i], cand[j]);
                    let pc = start + 4 * i as u64;
                    let note = self.compare(a, b, pc, func.relocs.get(&j), func, j);
                    let kind = if note.is_none() { LineKind::Same } else { LineKind::Changed };
                    if note.is_none() {
                        matched += 1;
                    } else if let Some(n) = &note {
                        *counts.entry(category(n)).or_default() += 1;
                    }
                    lines.push(MatchLine {
                        kind,
                        address: Some(pc),
                        original: Some(text(a, pc, big)),
                        rebuilt: Some(text(b, pc, func.big_endian)),
                        note,
                    });
                    i += 1;
                    j += 1;
                    k += 1;
                }
                _ => {
                    let mut dels: Vec<(usize, u32)> = Vec::new();
                    let mut ins: Vec<(usize, u32)> = Vec::new();
                    while k < script.len() && script[k] != Edit::Keep {
                        if script[k] == Edit::Delete {
                            dels.push((i, orig[i]));
                            i += 1;
                        } else {
                            ins.push((j, cand[j]));
                            j += 1;
                        }
                        k += 1;
                    }
                    // Reordered: the same instruction removed here and added here.
                    let reordered: Vec<u32> = dels
                        .iter()
                        .filter(|(_, w)| ins.iter().any(|(_, x)| shape(*x) == shape(*w)))
                        .map(|(_, w)| shape(*w))
                        .collect();
                    let n = dels.len().max(ins.len());
                    let mut dels = dels.into_iter();
                    let mut ins = ins.into_iter();
                    for _ in 0..n {
                        let (old, new) = (dels.next(), ins.next());
                        let (kind, note) = match (&old, &new) {
                            (Some((_, a)), Some((_, b))) => (LineKind::Changed, Some(explain(*a, *b, "instruction differs"))),
                            (Some((_, a)), None) if reordered.contains(&shape(*a)) => {
                                (LineKind::Removed, Some("reordered: this instruction is elsewhere in the rebuild".into()))
                            }
                            (Some((_, a)), None) if *a == 0 => (LineKind::Removed, Some("nop missing in the rebuild (a delay slot?)".into())),
                            (Some(_), None) => (LineKind::Removed, Some("missing in the rebuild".into())),
                            (None, Some((_, b))) if reordered.contains(&shape(*b)) => {
                                (LineKind::Added, Some("reordered: this instruction is elsewhere in the original".into()))
                            }
                            (None, Some((_, b))) if *b == 0 => (LineKind::Added, Some("extra nop in the rebuild".into())),
                            _ => (LineKind::Added, Some("extra in the rebuild".into())),
                        };
                        if let Some(n) = &note {
                            *counts.entry(category(n)).or_default() += 1;
                        }
                        lines.push(MatchLine {
                            kind,
                            address: old.map(|(i, _)| start + 4 * i as u64),
                            original: old.map(|(i, w)| text(w, start + 4 * i as u64, big)),
                            rebuilt: new.map(|(j, w)| text(w, start + 4 * j as u64, func.big_endian)),
                            note,
                        });
                    }
                }
            }
        }
        let total = orig.len().max(cand.len()) as u32;
        MatchResult {
            name,
            address: start,
            original_instructions: orig.len() as u32,
            rebuilt_instructions: cand.len() as u32,
            matched_instructions: matched,
            original_bytes: 4 * orig.len() as u64,
            rebuilt_bytes: 4 * cand.len() as u64,
            percent: if total == 0 { 100.0 } else { matched as f32 * 100.0 / total as f32 },
            lines,
            differences: counts.into_iter().collect(),
            original_compiler: compiler_tell(orig),
            rebuilt_compiler: compiler_tell(cand),
        }
    }

    /// Why the original's word `a` and the rebuild's `b`, lined up, differ (None: they match).
    fn compare(&self, a: u32, b: u32, pc: u64, reloc: Option<&Reloc>, func: &Words, j: usize) -> Option<String> {
        let (wa, wb) = (MipsWord(a), MipsWord(b));
        let mask = reloc.map_or(0, |r| reloc_mask(r.kind));
        if a & !mask != b & !mask {
            return Some(explain(a, b, "instruction differs"));
        }
        let rel = reloc.filter(|r| !r.section_symbol)?;
        // The linker's field: the same symbol in the original, when it is known there.
        let target = self.object_symbol_address(&rel.symbol)?;
        let field = |w: MipsWord| w.0 & mask;
        match rel.kind {
            r::MIPS_26 => {
                let dest = ((pc + 4) & 0xF000_0000) | (u64::from(field(wa)) << 2);
                (dest != target.wrapping_add(rel.addend as u64)).then(|| {
                    format!(
                        "calls {} in the rebuild; the original calls {}",
                        rel.symbol,
                        self.symbols().lookup(dest).map_or(format!("{dest:#x}"), |s| s.name)
                    )
                })
            }
            r::MIPS_LO16 => {
                let addend = wb.simm();
                let expect = (target as i64 + addend) as u32 & 0xFFFF;
                (field(wa) != expect).then(|| format!("%lo({}{addend:+#x}) differs from the original", rel.symbol))
            }
            r::MIPS_HI16 => {
                // The addend is split: the high half here, the low half in the LO16 that follows.
                let lo = func
                    .relocs
                    .range(j + 1..)
                    .find(|(_, r)| r.kind == r::MIPS_LO16)
                    .map_or(0, |(at, _)| MipsWord(func.words[*at]).simm());
                let addend = (i64::from(wb.imm()) << 16) + lo;
                let expect = ((target as i64 + addend + 0x8000) >> 16) as u32 & 0xFFFF;
                (field(wa) != expect).then(|| format!("%hi({}{addend:+#x}) differs from the original", rel.symbol))
            }
            _ => None,
        }
    }

    /// Every function of the object file `bytes` that the original names,
    /// matched; sorted worst first.
    pub fn match_object(&self, bytes: &[u8]) -> Result<Vec<MatchResult>> {
        Ok(self.match_unit("", bytes)?.functions)
    }

    /// The object file `bytes`, a unit of the project named `unit`: each of
    /// its functions matched against the original's function of that name
    /// (as the object has it, undecorated or demangled), worst first, and
    /// the functions the original has no name for.
    pub fn match_unit(&self, unit: &str, bytes: &[u8]) -> Result<UnitMatch> {
        self.match_unit_with(unit, bytes, &Lookups::default())
    }

    fn match_unit_with(&self, unit: &str, bytes: &[u8], lookups: &Lookups) -> Result<UnitMatch> {
        let functions = object_functions(bytes)?;
        if let Some(f) = functions.first() {
            self.check_isa(f.isa)?;
        }
        let mut out = UnitMatch {
            unit: unit.to_string(),
            functions: Vec::new(),
            unplaced: Vec::new(),
        };
        for f in &functions {
            match self
                .cached_symbol_address(lookups, &f.name)
                .and_then(|a| self.match_function_with(a, f, lookups))
            {
                Some(m) => out.functions.push(m),
                None => out.unplaced.push(f.name.clone()),
            }
        }
        out.functions
            .sort_by(|a, b| a.percent.total_cmp(&b.percent).then(a.address.cmp(&b.address)));
        Ok(out)
    }

    /// A project's object files (name, bytes) matched against the original,
    /// unit by unit (see [`Binary::match_unit`]); units worst first.
    pub fn match_project(&self, objects: &[(String, Vec<u8>)]) -> ProjectMatch {
        let mut p = ProjectMatch::default();
        let lookups = Lookups::default();
        for (name, bytes) in objects {
            match self.match_unit_with(name, bytes, &lookups) {
                Ok(u) => p.units.push(u),
                Err(e) => p.failed.push((name.clone(), e.to_string())),
            }
        }
        for u in &p.units {
            let (code, matched) = u.bytes();
            p.functions += u.functions.len() as u32;
            p.exact += u.exact();
            p.unplaced += u.unplaced.len() as u32;
            p.code_bytes += code;
            p.matched_bytes += matched;
        }
        let weighted: f64 = p
            .units
            .iter()
            .flat_map(|u| &u.functions)
            .map(|m| f64::from(m.percent) * m.original_bytes as f64)
            .sum();
        p.fuzzy_percent = if p.code_bytes == 0 {
            0.0
        } else {
            (weighted / p.code_bytes as f64) as f32
        };
        p.units.sort_by(|a, b| {
            a.fuzzy_percent()
                .total_cmp(&b.fuzzy_percent())
                .then_with(|| a.unit.cmp(&b.unit))
        });
        p
    }
}

/// An object file matched against the original: a unit of the project.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitMatch {
    pub unit: String,
    /// Its functions the original has, matched; worst first.
    pub functions: Vec<MatchResult>,
    /// Its functions no function of the original is named like.
    pub unplaced: Vec<String>,
}

impl UnitMatch {
    /// How many functions match exactly.
    pub fn exact(&self) -> u32 {
        self.functions.iter().filter(|m| m.percent >= 100.0).count() as u32
    }

    /// The original's bytes of code the functions compared cover, and those
    /// of the functions matching exactly.
    pub fn bytes(&self) -> (u64, u64) {
        let all = self.functions.iter().map(|m| m.original_bytes).sum();
        let exact = self
            .functions
            .iter()
            .filter(|m| m.percent >= 100.0)
            .map(|m| m.original_bytes)
            .sum();
        (all, exact)
    }

    /// The functions' percents, weighted by their size.
    pub fn fuzzy_percent(&self) -> f32 {
        let (all, _) = self.bytes();
        let weighted: f64 = self
            .functions
            .iter()
            .map(|m| f64::from(m.percent) * m.original_bytes as f64)
            .sum();
        if all == 0 { 0.0 } else { (weighted / all as f64) as f32 }
    }
}

/// How one build of a unit (its source compiled with some flags, or by
/// some compiler) matches the original.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildScore {
    /// The flags, or whatever tells this build from the others.
    pub label: String,
    pub functions: u32,
    pub exact: u32,
    /// The functions' percents weighted by their size.
    pub percent: f32,
    /// Why it couldn't be matched (it didn't compile, or isn't an object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Binary {
    /// Ranks several builds of one unit against the original, best first:
    /// the flags (or the compiler) a matching decompilation should use for
    /// it are those of the first. Each build is (label, object file).
    pub fn rank_builds(&self, builds: &[(String, Result<Vec<u8>>)]) -> Vec<BuildScore> {
        let mut out: Vec<BuildScore> = builds
            .iter()
            .map(|(label, object)| {
                let unit = object.as_ref().map_err(|e| e.to_string()).and_then(|bytes| {
                    self.match_unit(label, bytes).map_err(|e| e.to_string())
                });
                match unit {
                    Ok(u) => BuildScore {
                        label: label.clone(),
                        functions: u.functions.len() as u32,
                        exact: u.exact(),
                        percent: u.fuzzy_percent(),
                        error: None,
                    },
                    Err(e) => BuildScore {
                        label: label.clone(),
                        functions: 0,
                        exact: 0,
                        percent: 0.0,
                        error: Some(e),
                    },
                }
            })
            .collect();
        out.sort_by(|a, b| {
            b.exact
                .cmp(&a.exact)
                .then(b.percent.total_cmp(&a.percent))
                .then(a.label.cmp(&b.label))
        });
        out
    }
}

/// Builds ranked, one line each.
pub fn builds_text(scores: &[BuildScore]) -> String {
    let mut out = String::new();
    for s in scores {
        match &s.error {
            Some(e) => out.push_str(&format!("   —                         {}: {e}\n", s.label)),
            None => out.push_str(&format!(
                "{:>5.1}%  {:>4} of {:>4} exact  {}\n",
                s.percent, s.exact, s.functions, s.label
            )),
        }
    }
    out
}

/// A project's objects matched against the original (see [`Binary::match_project`]).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMatch {
    /// Worst first.
    pub units: Vec<UnitMatch>,
    /// Objects that couldn't be read or aren't for this binary's processor, with why.
    pub failed: Vec<(String, String)>,
    pub functions: u32,
    /// Functions matching exactly.
    pub exact: u32,
    /// Functions of the objects the original has no function named like.
    pub unplaced: u32,
    /// The original's code in the functions compared, and in those matching exactly.
    pub code_bytes: u64,
    pub matched_bytes: u64,
    /// The functions' percents, weighted by their size.
    pub fuzzy_percent: f32,
}

impl ProjectMatch {
    /// The summary as text: the totals, each unit (worst first) with its
    /// worst functions, then the `limit` worst functions of all.
    pub fn to_text(&self, limit: usize) -> String {
        let pct = |n: u64, of: u64| if of == 0 { 0.0 } else { n as f64 * 100.0 / of as f64 };
        let mut out = format!(
            "{} objects, {} functions compared, {} match exactly; {} of {} bytes of code in exact matches ({:.1}%), {:.1}% overall.\n",
            self.units.len(),
            self.functions,
            self.exact,
            self.matched_bytes,
            self.code_bytes,
            pct(self.matched_bytes, self.code_bytes),
            self.fuzzy_percent
        );
        match self.unplaced {
            0 => {}
            1 => out.push_str("1 function of the objects has none named like it here (name it first).\n"),
            n => out.push_str(&format!(
                "{n} functions of the objects have none named like them here (name them first).\n"
            )),
        }
        for (name, why) in &self.failed {
            out.push_str(&format!("  not matched: {name}: {why}\n"));
        }
        out.push_str("\nUnits, worst first:\n");
        for u in &self.units {
            let (code, matched) = u.bytes();
            let worst: Vec<String> = u
                .functions
                .iter()
                .filter(|m| m.percent < 100.0)
                .take(3)
                .map(|m| format!("{} {:.1}%", m.name, m.percent))
                .collect();
            out.push_str(&format!(
                "  {:>5.1}%  {}  {} of {} functions exact, {} of {} bytes{}{}\n",
                u.fuzzy_percent(),
                u.unit,
                u.exact(),
                u.functions.len(),
                matched,
                code,
                if u.unplaced.is_empty() {
                    String::new()
                } else {
                    format!(", {} not found here", u.unplaced.len())
                },
                if worst.is_empty() {
                    String::new()
                } else {
                    format!("; worst: {}", worst.join(", "))
                }
            ));
        }
        let mut all: Vec<(&MatchResult, &str)> = self
            .units
            .iter()
            .flat_map(|u| u.functions.iter().map(move |m| (m, u.unit.as_str())))
            .filter(|(m, _)| m.percent < 100.0)
            .collect();
        all.sort_by(|a, b| a.0.percent.total_cmp(&b.0.percent).then(a.0.address.cmp(&b.0.address)));
        if !all.is_empty() {
            out.push_str("\nFunctions not matching yet, worst first:\n");
            for (m, unit) in all.iter().take(limit) {
                let kinds: Vec<String> = m.differences.iter().map(|(k, n)| format!("{n} × {k}")).collect();
                out.push_str(&format!(
                    "  {:#x} {:>6.1}%  {}  ({unit}){}\n",
                    m.address,
                    m.percent,
                    m.name,
                    if kinds.is_empty() {
                        String::new()
                    } else {
                        format!("  {}", kinds.join(", "))
                    }
                ));
            }
            if all.len() > limit {
                out.push_str(&format!("  … {} more\n", all.len() - limit));
            }
        }
        out
    }

    /// Each function's outcome, as objdiff's report would place it: for
    /// recording in the notes.
    pub fn progress(&self) -> Vec<FunctionProgress> {
        self.units
            .iter()
            .flat_map(|u| {
                u.functions.iter().map(|m| FunctionProgress {
                    address: m.address,
                    name: m.name.clone(),
                    unit: u.unit.clone(),
                    size: m.original_bytes,
                    percent: m.percent,
                })
            })
            .collect()
    }
}

/// A range of addresses as written on a command line: `start..end`,
/// `start-end` or `start+length`, each number decimal or `0x` hex; or two
/// numbers given separately (`end` then holds the second).
pub fn parse_range(text: &str, end: Option<&str>) -> Option<(u64, u64)> {
    let number = |t: &str| {
        let t = t.trim();
        match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            Some(h) => u64::from_str_radix(h, 16).ok(),
            None => t.parse().ok(),
        }
    };
    if let Some(e) = end {
        return Some((number(text)?, number(e)?));
    }
    if let Some((a, b)) = text.split_once("..") {
        return Some((number(a)?, number(b)?));
    }
    if let Some((a, n)) = text.split_once('+') {
        let a = number(a)?;
        return Some((a, a.checked_add(number(n)?)?));
    }
    // `a-b`, when neither side is a bare negative number.
    if let Some((a, b)) = text.rsplit_once('-')
        && !a.is_empty()
    {
        return Some((number(a)?, number(b)?));
    }
    None
}

/// The object files (`.o`, `.obj`) in a folder and its subfolders, sorted:
/// a project's build output.
pub fn object_files(dir: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    fn walk(dir: &std::path::Path, depth: u32, out: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                if depth < 16 {
                    walk(&path, depth + 1, out)?;
                }
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("o") || e.eq_ignore_ascii_case("obj"))
            {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, 0, &mut out)?;
    out.sort();
    Ok(out)
}

/// The function of an object named `name`: as the object has it, or as
/// source writes it (`clamp` for `_clamp`, `Shape::scaled` for its mangling).
pub fn find_function<'a>(functions: &'a [ObjectFunction], name: &str) -> Option<&'a ObjectFunction> {
    functions.iter().find(|f| f.name == name).or_else(|| {
        functions
            .iter()
            .find(|f| source_names(&f.name).iter().any(|n| n == name))
    })
}

/// The names the symbol `name` of an object file may have in the binary,
/// most likely first: as it is; undecorated as C source writes it (MSVC's
/// 32-bit `_foo` for cdecl, `_foo@8` for stdcall, `@foo@8` for fastcall,
/// `foo@@8` for vectorcall, and an import's `__imp_` slot); and for C++,
/// the qualified name its mangling spells (`?scaled@Shape@@QBEHH@Z` and
/// `_ZNK5Shape6scaledEi` are `Shape::scaled`).
pub fn source_names(name: &str) -> Vec<String> {
    let mut out = vec![name.to_string()];
    let mut add = |n: String| {
        if !n.is_empty() && !out.contains(&n) {
            out.push(n);
        }
    };
    if let Some(rest) = name.strip_prefix("__imp_") {
        for n in source_names(rest).into_iter().skip(1) {
            add(format!("__imp_{n}"));
        }
        return out;
    }
    if name.starts_with('?') {
        // Not the compiler's own names (`` `string' ``, `` Shape::`vftable' ``): many share one.
        if let Ok(n) = msvc_demangler::demangle(name, msvc_demangler::DemangleFlags::NAME_ONLY)
            && !n.ends_with('\'')
        {
            add(n);
        }
        return out;
    }
    if name.starts_with("_Z") {
        if let Some(d) = crate::util::demangle(name) {
            add(without_parameters(&d).to_string());
        }
        return out;
    }
    add(undecorated(name).to_string());
    out
}

/// A 32-bit MSVC C name without its decoration: `_foo`, `_foo@8`, `@foo@8`
/// and `foo@@8` are all `foo`.
pub(crate) fn undecorated(name: &str) -> &str {
    let (body, fastcall) = match name.strip_prefix('@') {
        Some(rest) => (rest, true),
        None => (name, false),
    };
    // The argument bytes after the last `@` (or `@@`), when it's a number.
    let body = match body.rsplit_once('@') {
        Some((base, n)) if !base.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
            base.strip_suffix('@').unwrap_or(base)
        }
        _ if fastcall => return name,
        _ => body,
    };
    if fastcall {
        body
    } else {
        body.strip_prefix('_').unwrap_or(body)
    }
}

/// A demangled C++ name without its parameter list: `Shape::scaled(int) const` is `Shape::scaled`.
fn without_parameters(d: &str) -> &str {
    let mut depth = 0;
    for (i, c) in d.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            '(' if depth == 0 && i > 0 => return &d[..i],
            _ => {}
        }
    }
    d
}

/// The kind of difference a note describes, for counting.
fn category(note: &str) -> String {
    note.split([':', ';']).next().unwrap_or(note).trim().to_string()
}

/// Two instructions lined up that differ: what changed between them.
fn explain(a: u32, b: u32, otherwise: &str) -> String {
    let (wa, wb) = (MipsWord(a), MipsWord(b));
    let same_op = wa.op() == wb.op() && (wa.op() != 0 || wa.funct() == wb.funct());
    if a == 0 || b == 0 {
        return if a == 0 { "nop in the original, an instruction in the rebuild".into() } else { "an instruction in the original, nop in the rebuild".into() };
    }
    if !same_op {
        // The same operation with a register on one side and a constant on the other.
        let immediate_form = |w: MipsWord| match (w.op(), w.funct()) {
            (0, 32 | 33) => Some(8),
            (0, 34 | 35) => Some(8),
            (0, 36) => Some(12),
            (0, 37) => Some(13),
            (0, 38) => Some(14),
            (0, 42) => Some(10),
            (0, 43) => Some(11),
            _ => None,
        };
        let pair = |r: MipsWord, i: MipsWord| immediate_form(r).is_some_and(|op| op == i.op() || (op == 8 && i.op() == 9));
        if pair(wa, wb) {
            return "uses a register in the original, a constant in the rebuild (a variable became a constant?)".into();
        }
        if pair(wb, wa) {
            return "uses a constant in the original, a register in the rebuild (a constant became a variable?)".into();
        }
        return otherwise.to_string();
    }
    let regs = |w: MipsWord| (w.rs(), w.rt(), w.rd());
    if regs(wa) != regs(wb) {
        return "registers differ: the compiler allocated them differently".into();
    }
    if wa.op() == 0 && wa.sa() != wb.sa() {
        return "shift amount differs".into();
    }
    if wa.imm() != wb.imm() {
        return match wa.op() {
            1 | 4..=7 | 20..=23 => "branch offset differs: the code between is a different length".into(),
            9 if wa.rt() == 29 && wa.rs() == 29 => "stack frame size differs".into(),
            32..=46 | 48 | 55..=63 if wa.rs() == 29 => "stack slot offset differs".into(),
            _ => "immediate differs".into(),
        };
    }
    if wa.op() == 2 || wa.op() == 3 {
        return "jump target differs".into();
    }
    otherwise.to_string()
}

/// What to try in the C for a kind of difference (a count's name in
/// [`MatchResult::differences`]): the rewrites that usually fix it, as
/// decompilers apply them by hand, and as rule-based rewriting does before
/// asking a model.
pub fn rewrite_hint(kind: &str) -> Option<&'static str> {
    let hints: &[(&str, &str)] = &[
        ("registers differ", "reorder the declarations or the statements computing them, reuse a temporary where the original does (or split one), inline or pull out a subexpression, or give a variable another type (a pointer, a char, an unsigned): register allocation follows the code's shape, and decomp-permuter searches these"),
        ("stack slot offset differs", "arguments read in another order are operands swapped (a - b for b - a, the parameters' order); locals in another order or size: reorder their declarations, merge two into one or split one, or give one another type or array size"),
        ("stack frame size differs", "the frame holds other locals: remove or add a temporary, size an array as the original does, or keep a value from living across a call"),
        ("stack alignment differs", "the frame is aligned otherwise: a double or an aligned local, or other compiler flags"),
        ("arguments popped differ", "the call passes other arguments: check the callee's prototype (a parameter missing or extra, a double for a float, a structure by value)"),
        ("immediate differs", "a constant differs: check the literal (a #define, an enum, a sizeof), and what the compiler folded (x + 1 written as x - -1, a multiply as shifts)"),
        ("shift amount differs", "a constant differs: check the literal, and what the compiler folded (a multiply or divide by a power of two)"),
        ("offset differs", "another field or element: check the structure's layout (a member's type or order) or the index (off by one, a pointer stepped by another size)"),
        ("condition inverted", "swap the if and else branches, negate the test (if (!x) for if (x)), or turn a while into a do-while (or back): the compiler lays out branches in the order it is given them"),
        ("condition differs", "another comparison: < for <=, signed for unsigned, or the operands swapped (a > b for b < a)"),
        ("signedness differs", "make the variable or the cast unsigned where the original's is, or signed (s32/u32, char/unsigned char, int/unsigned)"),
        ("operand size differs", "a variable of another width: a short for an int, a char for a short"),
        ("short vs near jump", "the code jumped over is a different length: this follows from the other differences"),
        ("branch offset differs", "the code between is a different length: this follows from the other differences"),
        ("branch target differs", "the control flow differs: an else missing, a break or continue, a goto, a loop tested at the top rather than the bottom, or a switch's cases in another order"),
        ("jump target differs", "the control flow differs: a tail call the original makes (or doesn't), a goto, or a switch's cases in another order"),
        ("jump table differs", "the switch's cases lead elsewhere: cases in another order, merged or split, or the default placed otherwise"),
        ("call target differs", "another function is called: check which one the original calls, and whether one was inlined (or a macro expanded) on either side"),
        ("global differs", "another global: check which variable the original uses, or a static placed otherwise"),
        ("reordered", "the same instructions in another order: move a statement above or below its neighbour (often an assignment across a call); the compiler schedules within what a statement allows"),
        ("missing in the rebuild", "the rebuild lacks code the original has: a statement, a check (a NULL test, a bound), or an expression the rebuild's compiler folded away"),
        ("extra in the rebuild", "the rebuild has code the original doesn't: a statement too many, a check the original skips, or an expression the original's compiler folded"),
        ("nop missing in the rebuild", "MIPS: the rebuild fills a delay slot the original leaves empty: move a statement across the branch or call, or check the optimization level and the assembler's reordering"),
        ("an instruction in the original, nop in the rebuild", "MIPS: the original fills a delay slot the rebuild leaves empty: move a statement next to the branch or call"),
        ("nop in the original, an instruction in the rebuild", "MIPS: the rebuild fills a delay slot the original leaves empty: move a statement across the branch or call"),
        ("uses a register in the original, a constant in the rebuild", "the original keeps the value in a variable: use one (the compiler didn't fold it), or pass it in"),
        ("uses a constant in the original, a register in the rebuild", "the original uses the constant itself: write it as a literal (or a #define) rather than a variable"),
        ("reads memory in the original, a constant in the rebuild", "the original reads a global the rebuild doesn't: make it a variable the code reads, not a #define or a const"),
        ("encoding differs", "the same instruction in other bytes: another assembler or compiler version (check the compiler the Rich header or the SDK release names)"),
        ("alignment padding differs", "padding only: it follows from the length of the code before it"),
        ("instruction differs", "another instruction: often another operator or expression (a shift for a multiply, an lea for an add, a load of another width)"),
    ];
    hints.iter().find(|(k, _)| kind.starts_with(k)).map(|(_, h)| *h)
}

impl MatchResult {
    /// For each kind of difference found, what to try in the C.
    pub fn hints(&self) -> Vec<(String, &'static str)> {
        self.differences
            .iter()
            .filter_map(|(k, _)| Some((k.clone(), rewrite_hint(k)?)))
            .collect()
    }

    /// The result as text: the score, the kinds of difference and what to
    /// try for each, then the instructions lined up (`=` the same, `~`
    /// changed, `-` only in the original, `+` only in the rebuild) with each
    /// difference explained.
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "{}: {:.1}% ({} of {} instructions match; original {}, rebuilt {})\n",
            self.name,
            self.percent,
            self.matched_instructions,
            self.original_instructions.max(self.rebuilt_instructions),
            self.original_instructions,
            self.rebuilt_instructions
        );
        for (what, n) in &self.differences {
            out.push_str(&format!("  {n} × {what}\n"));
        }
        match (self.original_compiler, self.rebuilt_compiler) {
            (Some(a), Some(b)) if a != b => {
                out.push_str(&format!("Compiler: the original looks built by {a}; the rebuild by {b}. Another compiler won't match: build with the original's.\n"));
            }
            (Some(a), _) if self.percent < 100.0 => {
                out.push_str(&format!("Compiler: the original looks built by {a}.\n"))
            }
            _ => {}
        }
        if self.percent >= 100.0 {
            return out;
        }
        let hints = self.hints();
        if !hints.is_empty() {
            out.push_str("To try:\n");
            for (kind, hint) in hints {
                out.push_str(&format!("  {kind}: {hint}\n"));
            }
        }
        out.push('\n');
        for l in &self.lines {
            let mark = match l.kind {
                LineKind::Same => '=',
                LineKind::Changed => '~',
                LineKind::Removed => '-',
                LineKind::Added => '+',
            };
            let at = l.address.map_or("        ".to_string(), |a| format!("{a:08x}"));
            out.push_str(&format!(
                "{mark} {at}  {:<32} {:<32}{}\n",
                l.original.as_deref().unwrap_or(""),
                l.rebuilt.as_deref().unwrap_or(""),
                l.note.as_deref().map_or(String::new(), |n| format!(" ; {n}"))
            ));
        }
        out
    }
}

// ---- objdiff reports ------------------------------------------------------

/// objdiff's report (`objdiff-cli report generate`), the JSON decomp.dev shows.
#[derive(Debug, Clone, Deserialize)]
pub struct ObjdiffReport {
    #[serde(default)]
    pub measures: Measures,
    #[serde(default)]
    pub units: Vec<ReportUnit>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Measures {
    #[serde(default)]
    pub fuzzy_match_percent: f32,
    #[serde(default, deserialize_with = "number")]
    pub total_code: u64,
    #[serde(default, deserialize_with = "number")]
    pub matched_code: u64,
    #[serde(default)]
    pub matched_code_percent: f32,
    #[serde(default)]
    pub total_functions: u32,
    #[serde(default)]
    pub matched_functions: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReportUnit {
    pub name: String,
    #[serde(default)]
    pub measures: Measures,
    #[serde(default)]
    pub functions: Vec<ReportItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReportItem {
    pub name: String,
    #[serde(default, deserialize_with = "number")]
    pub size: u64,
    #[serde(default)]
    pub fuzzy_match_percent: f32,
    #[serde(default)]
    pub metadata: Option<ReportItemMetadata>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReportItemMetadata {
    #[serde(default)]
    pub demangled_name: Option<String>,
    #[serde(default, deserialize_with = "some_number")]
    pub virtual_address: Option<u64>,
}

/// A number the JSON gives as a number or, as protobuf's JSON (objdiff's)
/// writes 64-bit integers, as a string.
#[derive(Deserialize)]
#[serde(untagged)]
enum Number {
    Int(u64),
    Text(String),
    Float(f64),
}

impl Number {
    fn value<E: serde::de::Error>(self) -> std::result::Result<u64, E> {
        match self {
            Number::Int(n) => Ok(n),
            Number::Text(t) => t.trim().parse().map_err(E::custom),
            Number::Float(f) => Ok(f as u64),
        }
    }
}

fn number<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<u64, D::Error> {
    Number::deserialize(d)?.value()
}

fn some_number<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Option<u64>, D::Error> {
    Option::<Number>::deserialize(d)?.map(Number::value).transpose()
}

impl ObjdiffReport {
    pub fn parse(json: &[u8]) -> Result<ObjdiffReport> {
        serde_json::from_slice(json).map_err(|e| Error::new(format!("not an objdiff report: {e}")))
    }
}

/// objdiff's verdict on one of the binary's functions.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionProgress {
    pub address: u64,
    pub name: String,
    pub unit: String,
    pub size: u64,
    pub percent: f32,
}

/// An objdiff report placed on the binary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub total_code: u64,
    pub matched_code: u64,
    pub fuzzy_match_percent: f32,
    pub total_functions: u32,
    pub matched_functions: u32,
    /// Functions of the binary the report speaks of, by address.
    pub functions: Vec<FunctionProgress>,
    /// Report entries no function of the binary was found for.
    pub unplaced: Vec<String>,
}

impl Binary {
    /// objdiff's report placed on this binary's functions: by the virtual
    /// address the report records, else by name.
    pub fn place_report(&self, report: &ObjdiffReport) -> Progress {
        let mut functions = Vec::new();
        let mut unplaced = Vec::new();
        for unit in &report.units {
            for f in &unit.functions {
                let by_address = f
                    .metadata
                    .as_ref()
                    .and_then(|m| m.virtual_address)
                    .and_then(|a| self.symbols().function_containing(a).map(|s| s.address));
                let by_name = || {
                    self.object_symbol_address(&f.name).or_else(|| {
                        f.metadata
                            .as_ref()
                            .and_then(|m| m.demangled_name.as_deref())
                            .and_then(|n| self.object_symbol_address(n))
                    })
                };
                match by_address.or_else(by_name) {
                    Some(address) => functions.push(FunctionProgress {
                        address,
                        name: f.name.clone(),
                        unit: unit.name.clone(),
                        size: f.size,
                        percent: f.fuzzy_match_percent,
                    }),
                    None => unplaced.push(format!("{}:{}", unit.name, f.name)),
                }
            }
        }
        functions.sort_by_key(|f| f.address);
        Progress {
            total_code: report.measures.total_code,
            matched_code: report.measures.matched_code,
            fuzzy_match_percent: report.measures.fuzzy_match_percent,
            total_functions: report.measures.total_functions,
            matched_functions: report.measures.matched_functions,
            functions,
            unplaced,
        }
    }

    /// The functions with the extents the notes give them: the analysis's
    /// functions, except that a note with a size (a function made whole from
    /// the pieces the analysis split it into, say) stands for everything it
    /// covers, so a merged range counts once, at its size. (start, size).
    pub fn functions_as_noted(&self) -> Vec<(u64, u64)> {
        let mut list: Vec<(u64, u64)> = self.similar_index().functions().map(|(a, n, _)| (a, n)).collect();
        let sized: Vec<(u64, u64)> = self
            .symbols()
            .functions()
            .filter(|s| s.source == crate::model::SymbolSource::User && s.size > 0)
            .filter(|s| {
                self.section_at(s.address)
                    .is_some_and(|sec| sec.kind == crate::model::RegionKind::Code)
            })
            .map(|s| (s.address, s.size))
            .collect();
        for (start, size) in sized {
            list.retain(|&(a, _)| a <= start || a >= start + size);
            match list.binary_search_by_key(&start, |f| f.0) {
                Ok(i) => list[i].1 = size,
                Err(i) => list.insert(i, (start, size)),
            }
        }
        list
    }

    /// Where the decompilation stands, as objdiff's report: the JSON
    /// decomp.dev reads (64-bit numbers written as strings, as protobuf's
    /// JSON has them). A unit per source file the notes record for matched
    /// and nonmatching functions; each function with its size and match
    /// percent (100 when its C matches, else the best tried); the functions
    /// not decompiled yet in a unit of their own; library code in its own
    /// units and category, so the game's progress can be told apart.
    pub fn progress_report(&self) -> serde_json::Value {
        use crate::model::DecompState;
        use serde_json::json;
        struct F {
            name: String,
            demangled: Option<String>,
            address: u64,
            size: u64,
            percent: f32,
        }
        let mut units: BTreeMap<(bool, String), Vec<F>> = BTreeMap::new();
        for (address, size) in self.functions_as_noted() {
            let d = self.decomp_at(address);
            let state = d.map_or(DecompState::Todo, |d| d.state);
            let percent = match (state, d.and_then(|d| d.percent)) {
                (DecompState::Matched, _) => 100.0,
                (_, Some(p)) => p.clamp(0.0, 99.99),
                _ => 0.0,
            };
            let library = state == DecompState::Library;
            let source = d.map(|d| d.source.as_str()).filter(|s| !s.is_empty());
            let unit = match (library, source) {
                (true, Some(s)) => format!("library/{s}"),
                (true, None) => "library".to_string(),
                (false, Some(s)) if matches!(state, DecompState::Matched | DecompState::Nonmatching) => s.to_string(),
                _ => "(not decompiled)".to_string(),
            };
            let (name, demangled) = match self.symbols().at(address) {
                Some(s) => (s.name().to_string(), s.demangled().map(|d| d.into_owned())),
                None => (format!("sub_{address:x}"), None),
            };
            units.entry((library, unit)).or_default().push(F {
                name,
                demangled,
                address,
                size,
                percent,
            });
        }
        // Measures of a list of functions, objdiff's way; `units`: (all, complete).
        let measures = |fs: &mut dyn Iterator<Item = &F>, units: (u32, u32), complete_code: u64| {
            let (mut total, mut matched, mut fuzzy, mut n, mut m) = (0u64, 0u64, 0f64, 0u32, 0u32);
            for f in fs {
                total += f.size;
                fuzzy += f.size as f64 * f.percent as f64;
                n += 1;
                if f.percent >= 100.0 {
                    matched += f.size;
                    m += 1;
                }
            }
            let pct = |a: f64, b: f64| if b > 0.0 { a * 100.0 / b } else { 0.0 };
            json!({
                "fuzzy_match_percent": if total > 0 { fuzzy / total as f64 } else { 0.0 },
                "total_code": total.to_string(),
                "matched_code": matched.to_string(),
                "matched_code_percent": pct(matched as f64, total as f64),
                "total_functions": n,
                "matched_functions": m,
                "matched_functions_percent": pct(m as f64, n as f64),
                "complete_code": complete_code.to_string(),
                "complete_code_percent": pct(complete_code as f64, total as f64),
                "total_units": units.0,
                "complete_units": units.1,
            })
        };
        let complete = |fs: &[F]| fs.iter().all(|f| f.percent >= 100.0);
        let mut out_units = Vec::new();
        for ((library, name), fs) in &units {
            let done = complete(fs);
            let code: u64 = if done { fs.iter().map(|f| f.size).sum() } else { 0 };
            let functions: Vec<serde_json::Value> = fs
                .iter()
                .map(|f| {
                    let mut metadata = json!({ "virtual_address": f.address.to_string() });
                    if let Some(d) = &f.demangled {
                        metadata["demangled_name"] = json!(d);
                    }
                    json!({
                        "name": f.name,
                        "size": f.size.to_string(),
                        "fuzzy_match_percent": f.percent,
                        "metadata": metadata,
                    })
                })
                .collect();
            let mut unit_meta = json!({
                "complete": done,
                "progress_categories": [if *library { "library" } else { "game" }],
            });
            if name != "(not decompiled)" && !name.starts_with("library") {
                unit_meta["source_path"] = json!(name);
            }
            out_units.push(json!({
                "name": name,
                "measures": measures(&mut fs.iter(), (1, done as u32), code),
                "functions": functions,
                "metadata": unit_meta,
            }));
        }
        let category = |library: bool| {
            let us: Vec<&Vec<F>> = units.iter().filter(|((l, _), _)| *l == library).map(|(_, f)| f).collect();
            let done: Vec<&&Vec<F>> = us.iter().filter(|f| complete(f)).collect();
            let code = done.iter().flat_map(|f| f.iter()).map(|f| f.size).sum();
            measures(
                &mut us.iter().flat_map(|f| f.iter()),
                (us.len() as u32, done.len() as u32),
                code,
            )
        };
        let all_done: Vec<&Vec<F>> = units.values().filter(|f| complete(f)).collect();
        let total = measures(
            &mut units.values().flat_map(|f| f.iter()),
            (units.len() as u32, all_done.len() as u32),
            all_done.iter().flat_map(|f| f.iter()).map(|f| f.size).sum(),
        );
        json!({
            "measures": total,
            "units": out_units,
            "version": 1,
            "categories": [
                { "id": "game", "name": "Game", "measures": category(false) },
                { "id": "library", "name": "Library", "measures": category(true) },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// A PS-X EXE with `words` at 0x80010000.
    fn exe(words: &[u32]) -> Binary {
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

    /// A little-endian ELF32 MIPS relocatable object: `.text` holding
    /// `words` as `func`, with `relocs` (word index, type, symbol name).
    fn object(words: &[u32], relocs: &[(usize, u32, &str)]) -> Vec<u8> {
        let text = le(words);
        let mut names: Vec<&str> = vec!["func"];
        for (_, _, s) in relocs {
            if !names.contains(s) {
                names.push(s);
            }
        }
        let mut strtab = vec![0u8];
        let mut name_off = Vec::new();
        for n in &names {
            name_off.push(strtab.len() as u32);
            strtab.extend(n.as_bytes());
            strtab.push(0);
        }
        let mut symtab = vec![0u8; 16];
        for (i, _) in names.iter().enumerate() {
            let mut s = [0u8; 16];
            s[0..4].copy_from_slice(&name_off[i].to_le_bytes());
            if i == 0 {
                s[4..8].copy_from_slice(&0u32.to_le_bytes());
                s[8..12].copy_from_slice(&(text.len() as u32).to_le_bytes());
                s[12] = 0x12; // STB_GLOBAL, STT_FUNC
                s[14..16].copy_from_slice(&1u16.to_le_bytes());
            } else {
                s[12] = 0x10; // STB_GLOBAL, STT_NOTYPE, undefined
            }
            symtab.extend(s);
        }
        let mut rel = Vec::new();
        for (at, kind, s) in relocs {
            let sym = names.iter().position(|n| n == s).unwrap() as u32 + 1;
            rel.extend((*at as u32 * 4).to_le_bytes());
            rel.extend(((sym << 8) | kind).to_le_bytes());
        }
        let shstr = b"\0.text\0.rel.text\0.symtab\0.strtab\0.shstrtab\0".to_vec();
        let mut out = vec![0u8; 52];
        let mut place = |bytes: &[u8]| {
            while out.len() % 4 != 0 {
                out.push(0);
            }
            let at = out.len();
            out.extend_from_slice(bytes);
            (at as u32, bytes.len() as u32)
        };
        let t = place(&text);
        let r = place(&rel);
        let sy = place(&symtab);
        let st = place(&strtab);
        let sh = place(&shstr);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        let shoff = out.len() as u32;
        let mut section = |name: u32, kind: u32, flags: u32, (off, size): (u32, u32), link: u32, info: u32, entsize: u32| {
            let mut h = [0u8; 40];
            for (i, v) in [name, kind, flags, 0, off, size, link, info, 4, entsize].iter().enumerate() {
                h[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
            }
            out.extend(h);
        };
        section(0, 0, 0, (0, 0), 0, 0, 0);
        section(1, 1, 6, t, 0, 0, 0);
        section(7, 9, 0, r, 3, 1, 8);
        section(17, 2, 0, sy, 4, 1, 16);
        section(25, 3, 0, st, 0, 0, 0);
        section(33, 3, 0, sh, 0, 0, 0);
        let header: [(usize, &[u8]); 2] = [(0, &[0x7F, b'E', b'L', b'F', 1, 1, 1, 0]), (16, &[1, 0, 8, 0, 1, 0, 0, 0])];
        for (at, b) in header {
            out[at..at + b.len()].copy_from_slice(b);
        }
        out[32..36].copy_from_slice(&shoff.to_le_bytes());
        out[40..42].copy_from_slice(&52u16.to_le_bytes());
        out[46..48].copy_from_slice(&40u16.to_le_bytes());
        out[48..50].copy_from_slice(&6u16.to_le_bytes());
        out[50..52].copy_from_slice(&5u16.to_le_bytes());
        out
    }

    /// entry: a frame, a call to the leaf at 0x80010030, a global read, return.
    const ORIGINAL: [u32; 12] = [
        0x27BD_FFE8, // addiu $sp, $sp, -0x18
        0xAFBF_0014, // sw $ra, 0x14($sp)
        0x0C00_400C, // jal 0x80010030
        0x0000_0000, // nop
        0x3C02_8012, // lui $v0, 0x8012
        0x8C42_0010, // lw $v0, 0x10($v0)
        0x8FBF_0014, // lw $ra, 0x14($sp)
        0x0000_0000, // nop
        0x03E0_0008, // jr $ra
        0x27BD_0018, // addiu $sp, $sp, 0x18
        0x0000_0000, // (padding)
        0x0000_0000,
    ];

    #[test]
    fn rebuilt_function_scored_and_explained() {
        let mut words = ORIGINAL.to_vec();
        words.extend([0x03E0_0008, 0x2402_0001]); // the leaf at 0x80010030: jr $ra / li $v0, 1
        let bin = exe(&words);
        // The rebuild, relocations in place of the addresses: a perfect match.
        let rebuilt = [
            0x27BD_FFE8,
            0xAFBF_0014,
            0x0C00_0000,
            0x0000_0000,
            0x3C02_0000,
            0x8C42_0010,
            0x8FBF_0014,
            0x0000_0000,
            0x03E0_0008,
            0x27BD_0018,
        ];
        let relocs = [(2, r::MIPS_26, "sub_80010030"), (4, r::MIPS_HI16, "gState"), (5, r::MIPS_LO16, "gState")];
        let obj = object(&rebuilt, &relocs);
        let funcs = object_functions(&obj).unwrap();
        assert_eq!((funcs.len(), funcs[0].name.as_str(), funcs[0].relocs.len()), (1, "func", 3));
        let m = bin.match_function(0x8001_0000, &funcs[0]).unwrap();
        assert_eq!((m.percent, m.matched_instructions), (100.0, 10));

        // A wrong frame size, a different register, a call to the wrong function, a missing nop.
        let worse = [
            0x27BD_FFE0, // addiu $sp, $sp, -0x20
            0xAFBF_0014,
            0x0C00_0000,
            0x0000_0000,
            0x3C03_0000, // lui $v1
            0x8C43_0010, // lw $v1, 0x10($v1)
            0x8FBF_0014,
            0x03E0_0008, // jr $ra with no nop before it
            0x27BD_0018,
        ];
        let relocs = [(2, r::MIPS_26, "entry"), (4, r::MIPS_HI16, "gState"), (5, r::MIPS_LO16, "gState")];
        let obj = object(&worse, &relocs);
        let results = bin.match_object(&obj).unwrap();
        assert!(results.is_empty(), "func is not a name the original knows");
        let funcs = object_functions(&obj).unwrap();
        let m = bin.match_function(0x8001_0000, &funcs[0]).unwrap();
        let notes: Vec<&str> = m.lines.iter().filter_map(|l| l.note.as_deref()).collect();
        assert!(notes.iter().any(|n| n.starts_with("stack frame size differs")), "{notes:?}");
        assert!(notes.iter().any(|n| n.starts_with("registers differ")), "{notes:?}");
        assert!(notes.iter().any(|n| n.contains("calls entry in the rebuild; the original calls sub_80010030")), "{notes:?}");
        assert!(notes.iter().any(|n| n.starts_with("nop missing")), "{notes:?}");
        assert_eq!(m.matched_instructions, 5);
        assert!((m.percent - 50.0).abs() < 0.01, "{}", m.percent);
        assert!(m.to_text().contains("stack frame size differs"));
    }

    #[test]
    fn a_range_is_scored_and_the_epilogue_names_the_compiler() {
        let mut words = ORIGINAL.to_vec();
        words.extend([0x03E0_0008, 0x2402_0001]);
        let bin = exe(&words);
        let rebuilt = [
            0x27BD_FFE8,
            0xAFBF_0014,
            0x0C00_0000,
            0x0000_0000,
            0x3C02_0000,
            0x8C42_0010,
            0x8FBF_0014,
            0x0000_0000,
            0x03E0_0008,
            0x27BD_0018,
        ];
        let relocs = [
            (2, r::MIPS_26, "sub_80010030"),
            (4, r::MIPS_HI16, "gState"),
            (5, r::MIPS_LO16, "gState"),
        ];
        let obj = object(&rebuilt, &relocs);
        let funcs = object_functions(&obj).unwrap();
        // The function's own extent, and a range cut short.
        let m = bin.match_range(0x8001_0000, 0x8001_0028, &funcs[0]).unwrap();
        assert_eq!(
            (m.percent, m.matched_instructions, m.name.as_str()),
            (100.0, 10, "entry (0x80010000..0x80010028)")
        );
        let m = bin.match_range(0x8001_0008, 0x8001_0020, &funcs[0]).unwrap();
        assert_eq!((m.original_instructions, m.rebuilt_instructions), (6, 10));
        assert!(m.name.starts_with("entry+0x8"), "{}", m.name);
        assert!(bin.match_range(0x8001_0000, 0x8010_0000, &funcs[0]).is_err());
        assert_eq!(
            parse_range("0x80010000..0x80010028", None),
            Some((0x8001_0000, 0x8001_0028))
        );
        assert_eq!(parse_range("0x80010000+0x28", None), Some((0x8001_0000, 0x8001_0028)));
        assert_eq!(parse_range("0x80010000", Some("40")), Some((0x8001_0000, 40)));
        assert_eq!(parse_range("nonsense", None), None);

        // Both pop the frame in the delay slot: GCC 2.8's way, nothing to say.
        assert_eq!(
            compiler_tell(&ORIGINAL),
            Some("GCC 2.8 or later (the frame is popped in the return's delay slot)")
        );
        assert!(!m.to_text().contains("Compiler:") || m.percent < 100.0);
        // GCC 2.7.2 pops it before the return and leaves the slot empty: told apart.
        let older = [
            0x27BD_FFE8,
            0xAFBF_0014,
            0x0C00_400C,
            0x0000_0000,
            0x3C02_8012,
            0x8C42_0010,
            0x8FBF_0014,
            0x27BD_0018,
            0x03E0_0008,
            0x0000_0000,
        ];
        assert_eq!(
            compiler_tell(&older),
            Some("GCC 2.7.2 (the frame is popped before the return, a nop in its delay slot)")
        );
        assert_eq!(compiler_tell(&[0x03E0_0008, 0x2402_0001]), None);
        let bin = exe(&older);
        let m = bin.match_function(0x8001_0000, &funcs[0]).unwrap();
        assert!(m.percent < 100.0);
        let text = m.to_text();
        assert!(
            text.contains("the original looks built by GCC 2.7.2") && text.contains("the rebuild by GCC 2.8"),
            "{text}"
        );
    }

    #[test]
    fn objdiff_report_placed() {
        let bin = exe(&ORIGINAL);
        let json = br#"{"measures":{"fuzzy_match_percent":50.0,"total_code":40,"matched_code":20,"total_functions":2,"matched_functions":1},
            "units":[{"name":"src/main","measures":{},"functions":[
              {"name":"entry","size":40,"fuzzy_match_percent":100.0,"metadata":{"virtual_address":2147549184}},
              {"name":"nothing","size":8,"fuzzy_match_percent":0.0}]}]}"#;
        let report = ObjdiffReport::parse(json).unwrap();
        let p = bin.place_report(&report);
        assert_eq!((p.total_code, p.matched_code, p.functions.len()), (40, 20, 1));
        assert_eq!((p.functions[0].address, p.functions[0].percent), (0x8001_0000, 100.0));
        assert_eq!(p.unplaced, ["src/main:nothing"]);
        // As objdiff writes it: 64-bit numbers as strings.
        let json = br#"{"measures":{"fuzzy_match_percent":50.0,"total_code":"40","matched_code":"20","total_functions":2},
            "units":[{"name":"src/main","functions":[
              {"name":"entry","size":"40","fuzzy_match_percent":100.0,"metadata":{"virtual_address":"2147549184"}}]}]}"#;
        let p = bin.place_report(&ObjdiffReport::parse(json).unwrap());
        assert_eq!((p.total_code, p.matched_code, p.functions[0].address), (40, 20, 0x8001_0000));
    }
}
