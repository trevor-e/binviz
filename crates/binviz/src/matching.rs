//! Matching a decompilation's rebuilt code against the original: the
//! compiler's object file, function by function, against the functions of
//! the binary it is meant to reproduce, with the fields the linker fills in
//! (relocations: call targets, the halves of an address) masked, and each
//! difference explained (a register chosen differently, a stack slot at
//! another offset, an instruction reordered, a `nop` missing from a delay
//! slot) so that whoever is writing the C knows what to change.
//!
//! Also reads objdiff's report (what decomp.dev shows) to place its verdicts
//! on the binary's functions.
//!
//! MIPS (PlayStation, Nintendo 64) so far.

use std::collections::BTreeMap;

use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
use serde::{Deserialize, Serialize};

use crate::binary::Binary;
use crate::cpu::mips::MipsWord;
use crate::error::{Error, Result};
use crate::fndiff::{Edit, LineKind, edit_script};

/// A function in a compiled object file.
#[derive(Debug, Clone)]
pub struct ObjectFunction {
    pub name: String,
    /// Its offset in its section.
    pub offset: u64,
    /// Instruction words, in the object's byte order.
    pub words: Vec<u32>,
    pub big_endian: bool,
    /// Relocations by instruction index: what the linker fills in.
    pub relocs: BTreeMap<usize, Reloc>,
}

#[derive(Debug, Clone)]
pub struct Reloc {
    /// The ELF relocation type (`R_MIPS_26`, `R_MIPS_HI16`…).
    pub kind: u32,
    pub symbol: String,
    pub addend: i64,
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
    /// 100 when every instruction matches, relocations aside.
    pub percent: f32,
    pub lines: Vec<MatchLine>,
    /// Kinds of difference, with how many of each.
    pub differences: Vec<(String, u32)>,
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

/// The functions of a compiled object file (ELF, MIPS), with their relocations.
pub fn object_functions(bytes: &[u8]) -> Result<Vec<ObjectFunction>> {
    let file = object::File::parse(bytes).map_err(|e| Error::new(format!("not an object file: {e}")))?;
    if file.architecture() != object::Architecture::Mips {
        return Err(Error::new(format!(
            "matching is for MIPS objects so far; this one is {:?}",
            file.architecture()
        )));
    }
    let big_endian = !file.is_little_endian();
    let mut out = Vec::new();
    for section in file.sections() {
        if section.kind() != object::SectionKind::Text {
            continue;
        }
        let data = section.data().map_err(|e| Error::new(e.to_string()))?;
        let mut relocs: BTreeMap<u64, Reloc> = BTreeMap::new();
        for (offset, rel) in section.relocations() {
            let object::RelocationFlags::Elf { r_type } = rel.flags() else { continue };
            let symbol = match rel.target() {
                RelocationTarget::Symbol(i) => file
                    .symbol_by_index(i)
                    .ok()
                    .and_then(|s| s.name().ok().map(str::to_string))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            relocs.insert(
                offset,
                Reloc {
                    kind: r_type.0,
                    symbol,
                    addend: rel.addend(),
                },
            );
        }
        let mut symbols: Vec<(u64, u64, String)> = file
            .symbols()
            .filter(|s| s.section_index() == Some(section.index()) && s.kind() == object::SymbolKind::Text)
            .filter(|s| !s.name().unwrap_or("").is_empty())
            .map(|s| (s.address(), s.size(), s.name().unwrap_or("").to_string()))
            .collect();
        symbols.sort();
        for (i, (offset, size, name)) in symbols.iter().enumerate() {
            let end = if *size > 0 {
                offset + size
            } else {
                symbols.get(i + 1).map_or(data.len() as u64, |s| s.0)
            };
            let Some(code) = data.get(*offset as usize..end as usize) else { continue };
            let words = code
                .chunks_exact(4)
                .map(|c| {
                    let b = [c[0], c[1], c[2], c[3]];
                    if big_endian { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) }
                })
                .collect();
            let relocs = relocs
                .range(*offset..end)
                .map(|(at, r)| (((at - offset) / 4) as usize, r.clone()))
                .collect();
            out.push(ObjectFunction {
                name: name.clone(),
                offset: *offset,
                words,
                big_endian,
                relocs,
            });
        }
    }
    Ok(out)
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
    /// the file's symbols and the user's names.
    fn symbol_address(&self, name: &str) -> Option<u64> {
        self.symbols().by_name(name).map(|s| s.address)
    }

    /// The rebuilt `func` against the original's function at `address`.
    pub fn match_function(&self, address: u64, func: &ObjectFunction) -> Option<MatchResult> {
        let (name, start, orig, big) = self.function_words(address)?;
        let cand = &func.words;
        // Lined up by their shapes: everything the layout decides masked off.
        let shape = |w: u32| w & !layout_mask(MipsWord(w));
        let ta: Vec<u32> = orig.iter().map(|&w| shape(w)).collect();
        let tb: Vec<u32> = cand.iter().map(|&w| shape(w)).collect();
        let script = edit_script(&ta, &tb, 4000).unwrap_or_else(|| {
            let mut s = vec![Edit::Delete; orig.len()];
            s.extend(std::iter::repeat_n(Edit::Insert, cand.len()));
            s
        });
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
        Some(MatchResult {
            name,
            address: start,
            original_instructions: orig.len() as u32,
            rebuilt_instructions: cand.len() as u32,
            matched_instructions: matched,
            percent: if total == 0 { 100.0 } else { matched as f32 * 100.0 / total as f32 },
            lines,
            differences: counts.into_iter().collect(),
        })
    }

    /// Why the original's word `a` and the rebuild's `b`, lined up, differ (None: they match).
    fn compare(&self, a: u32, b: u32, pc: u64, reloc: Option<&Reloc>, func: &ObjectFunction, j: usize) -> Option<String> {
        let (wa, wb) = (MipsWord(a), MipsWord(b));
        let mask = reloc.map_or(0, |r| reloc_mask(r.kind));
        if a & !mask != b & !mask {
            return Some(explain(a, b, "instruction differs"));
        }
        let Some(rel) = reloc else { return None };
        // The linker's field: the same symbol in the original, when it is known there.
        let Some(target) = self.symbol_address(&rel.symbol) else {
            return None;
        };
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
        let mut out = Vec::new();
        for f in object_functions(bytes)? {
            if let Some(address) = self.symbol_address(&f.name)
                && let Some(m) = self.match_function(address, &f)
            {
                out.push(m);
            }
        }
        out.sort_by(|a, b| a.percent.total_cmp(&b.percent).then(a.address.cmp(&b.address)));
        Ok(out)
    }
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

impl MatchResult {
    /// The result as text: the score, the kinds of difference, then the
    /// instructions lined up (`=` the same, `~` changed, `-` only in the
    /// original, `+` only in the rebuild) with each difference explained.
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
        if self.percent >= 100.0 {
            return out;
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
    #[serde(default)]
    pub total_code: u64,
    #[serde(default)]
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
    #[serde(default)]
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
    #[serde(default)]
    pub virtual_address: Option<u64>,
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
                    self.symbol_address(&f.name).or_else(|| {
                        f.metadata
                            .as_ref()
                            .and_then(|m| m.demangled_name.as_deref())
                            .and_then(|n| self.symbol_address(n))
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
    }
}
