//! Pointers a game keeps to its data, found from the code that builds them.
//! A 6502 reads data through a pointer held in two bytes of RAM, `lda
//! (ptr),y`, and where those two bytes come from says where the data is:
//!
//! - two immediates, `lda #<text / sta ptr / lda #>text / sta ptr+1`: the
//!   code takes the address of `text`;
//! - a table of words, `lda table,x / sta ptr / lda table+1,x / sta ptr+1`:
//!   each entry points at something (a string, a level, a sprite's frames);
//! - two tables of bytes, `lda lo,x / sta ptr / lda hi,x / sta ptr+1`: each
//!   entry's low and high bytes kept apart.
//!
//! The 65816 with 16-bit registers does each with one load and one store.
//! Only bytes the code reads through (`(ptr),y`, `(ptr,x)`, `[ptr]`, `jmp
//! (ptr)`) count: two numbers stored side by side are no pointer.
//!
//! When asked where a place is referred to, the ROM's words holding its
//! address count too ([`Binary::rom_pointers_to`]): tables the code reads
//! some other way, or that no code found reads.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::Map;
use crate::binary::Binary;
use crate::cpu::{self, Cpu, Flow, State};
use crate::xrefs::RefKind;

/// A table of pointers the code reads.
#[derive(Debug, Clone)]
pub(crate) struct Table {
    /// Its first entry (of two tables of bytes, the low bytes').
    pub address: u64,
    /// The high bytes' table, when each entry is split between two.
    pub high: Option<u64>,
    pub entries: u32,
}

impl Table {
    /// Bytes each entry takes in the table at `address`.
    pub fn stride(&self) -> u64 {
        if self.high.is_some() { 1 } else { 2 }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Found {
    /// Code taking an address, and table entries pointing at one: (source, target, kind).
    pub refs: Vec<(u64, u64, RefKind)>,
    pub tables: Vec<Table>,
    /// Addresses code builds that fall in a window whose bank can't be told
    /// from where the code is: (instruction, CPU address).
    pub unplaced: Vec<(u64, u64)>,
}

/// What the search needs from the analysis.
pub(crate) struct Code<'a, 'd> {
    pub cpu: Cpu,
    /// Runs of instructions: (start, size, the CPU state at the start).
    pub runs: &'a [(u64, u64, State)],
    /// The bytes at one of our addresses, to the end of their section.
    pub bytes_at: &'a dyn Fn(u64) -> Option<&'d [u8]>,
    /// Our address for a CPU address, named by code at one of ours.
    pub resolve: &'a dyn Fn(u64, u64) -> Option<u64>,
    /// The CPU's address for one of ours.
    pub cpu_of: &'a dyn Fn(u64) -> u64,
    /// Whether one of our addresses is in a run of instructions.
    pub is_code: &'a dyn Fn(u64) -> bool,
    /// Addresses the code reads (where one table ends, another may start).
    pub read: &'a HashSet<u64>,
}

/// Where a register's value came from.
#[derive(Debug, Clone, Copy)]
enum Src {
    /// `lda #value`.
    Imm { value: u64, wide: bool, pc: u64 },
    /// `lda table,x` (a CPU address, in the code's bank).
    Table { table: u64, x: bool, wide: bool, pc: u64 },
}

impl Src {
    fn wide(self) -> bool {
        match self {
            Src::Imm { wide, .. } | Src::Table { wide, .. } => wide,
        }
    }

    fn pc(self) -> u64 {
        match self {
            Src::Imm { pc, .. } | Src::Table { pc, .. } => pc,
        }
    }
}

/// Registers an instruction changes (A, X, Y), besides loads.
fn changes(mnemonic: &str, operands: &str) -> [bool; 3] {
    let a = matches!(
        mnemonic,
        "pla" | "txa" | "tya" | "adc" | "sbc" | "and" | "ora" | "eor" | "xba" | "tdc" | "tsc"
    ) || (matches!(mnemonic, "asl" | "lsr" | "rol" | "ror" | "inc" | "dec") && operands == "a");
    let x = matches!(mnemonic, "plx" | "tax" | "tsx" | "inx" | "dex" | "tyx");
    let y = matches!(mnemonic, "ply" | "tay" | "iny" | "dey" | "txy");
    [a, x, y]
}

/// The register a load or store names: `lda`, `stx`...
fn register(mnemonic: &str) -> Option<usize> {
    match mnemonic.as_bytes().get(2)? {
        b'a' => Some(0),
        b'x' => Some(1),
        b'y' => Some(2),
        _ => None,
    }
}

/// The value of an immediate operand (`#$1F`, `#$0200`) and whether it is 16 bits.
fn immediate(operands: &str) -> Option<(u64, bool)> {
    let digits = operands.strip_prefix("#$")?;
    Some((u64::from_str_radix(digits, 16).ok()?, digits.len() > 2))
}

/// Whether an operand is an address indexed by X or Y (`$C120,x`, not `$10,x`).
fn indexed_absolute(operands: &str) -> Option<bool> {
    let (address, index) = operands.rsplit_once(',')?;
    let digits = address.strip_prefix('$')?;
    (digits.len() >= 4).then_some(index == "x")
}

/// Whether an operand reads through a pointer, and so where the pointer is:
/// `($10),y`, `($10,x)`, `($10)`, `[$10]`, `[$10],y`, `($0200)` (`jmp`). Not
/// `($C000,x)` (a table of jumps) or `($03,s),y`.
fn through_pointer(operands: &str) -> bool {
    let Some(inner) = operands.strip_prefix('(').or_else(|| operands.strip_prefix('[')) else {
        return false;
    };
    if inner.contains(",s") {
        return false;
    }
    let digits = inner
        .trim_start_matches('$')
        .split([',', ')', ']'])
        .next()
        .unwrap_or("");
    !(inner.contains(",x)") && digits.len() > 2)
}

pub(crate) fn find(code: &Code) -> Found {
    let decode = |pc: u64, state: &mut State| -> Option<cpu::Insn> {
        let bytes = (code.bytes_at)(pc)?;
        cpu::decode(code.cpu, bytes, (code.cpu_of)(pc), state)
    };
    // Each run's instructions, decoded in order (told when a run starts).
    let each = |f: &mut dyn FnMut(u64, &cpu::Insn, &State, bool)| {
        for &(start, size, state) in code.runs {
            let mut state = state;
            let mut pc = start;
            while pc < start + size {
                let before = state;
                let Some(insn) = decode(pc, &mut state) else { break };
                f(pc, &insn, &before, pc == start);
                pc += u64::from(insn.len.max(1));
            }
        }
    };
    // First, the bytes the code reads through.
    let mut pointers = HashSet::new();
    each(&mut |pc, insn, _, _| {
        if through_pointer(&insn.operands)
            && let Some((p, _)) = insn.data
            && let Some(p) = (code.resolve)(pc, p)
        {
            pointers.insert(p);
        }
    });
    let bank = |pc: u64| {
        if code.cpu == Cpu::W65816 {
            (code.cpu_of)(pc) & 0xFF_0000
        } else {
            0
        }
    };
    let mut found = Found::default();
    // Tables by their first entry (ours): (high bytes' table, the CPU address, who reads it).
    let mut tables: BTreeMap<u64, Option<u64>> = BTreeMap::new();
    let mut table = |pc: u64, lo: u64, hi: Option<u64>| {
        let (Some(lo), hi) = ((code.resolve)(pc, lo), hi.map(|h| (code.resolve)(pc, h))) else {
            return;
        };
        match hi {
            Some(None) => {}
            Some(Some(h)) => {
                tables.entry(lo).or_insert(Some(h));
            }
            None => {
                tables.entry(lo).or_insert(None);
            }
        }
    };
    let address = |pc: u64, value: u64, found: &mut Found| {
        let cpu = bank(pc) | value;
        match (code.resolve)(pc, cpu) {
            Some(t) => found.refs.push((pc, t, RefKind::Address)),
            None => found.unplaced.push((pc, cpu)),
        }
    };
    // Then the pointers built in them.
    let mut regs: [Option<Src>; 3] = [None; 3];
    let mut stored: HashMap<u64, (Src, usize)> = HashMap::new();
    let mut n = 0usize;
    each(&mut |pc, insn, state, first| {
        n += 1;
        // A new run: nothing carries over.
        if first {
            regs = [None; 3];
            stored.clear();
        }
        let m = insn.mnemonic.as_str();
        let ops = insn.operands.as_str();
        match m {
            "lda" | "ldx" | "ldy" => {
                let r = register(m).unwrap_or(0);
                let wide = code.cpu == Cpu::W65816 && (if r == 0 { !state.m8 } else { !state.x8 });
                regs[r] = if let Some((value, _)) = immediate(ops) {
                    Some(Src::Imm { value, wide, pc })
                } else if let (Some(x), Some((t, RefKind::Read))) = (indexed_absolute(ops), insn.data) {
                    Some(Src::Table { table: t, x, wide, pc })
                } else {
                    None
                };
            }
            "sta" | "stx" | "sty" if !ops.contains(',') && !ops.starts_with('(') && !ops.starts_with('[') => {
                let r = register(m).unwrap_or(0);
                let (Some(src), Some((dest, RefKind::Write))) = (regs[r], insn.data) else {
                    return;
                };
                let Some(dest) = (code.resolve)(pc, dest) else { return };
                // Both bytes at once (16-bit registers).
                if src.wide() {
                    if pointers.contains(&dest) {
                        match src {
                            Src::Imm { value, pc, .. } => address(pc, value, &mut found),
                            Src::Table { table: t, pc, .. } => table(pc, t, None),
                        }
                    }
                    return;
                }
                stored.insert(dest, (src, n));
                let recent = |at: u64| stored.get(&at).filter(|(_, when)| n - when <= 8).map(|s| s.0);
                let (lo_at, lo, hi) = if let Some(lo) = dest.checked_sub(1).and_then(recent) {
                    (dest - 1, lo, src)
                } else if let Some(hi) = recent(dest + 1) {
                    (dest, src, hi)
                } else {
                    return;
                };
                if !pointers.contains(&lo_at) {
                    return;
                }
                // Built: the next pointer in the same bytes starts afresh.
                stored.remove(&lo_at);
                stored.remove(&(lo_at + 1));
                match (lo, hi) {
                    (Src::Imm { value: l, .. }, Src::Imm { value: h, .. }) => {
                        address(lo.pc().min(hi.pc()), (h & 0xFF) << 8 | (l & 0xFF), &mut found);
                    }
                    (Src::Table { table: l, x: lx, .. }, Src::Table { table: h, x: hx, .. }) if lx == hx => {
                        // The same table twice (the index stepped between): words.
                        if h == l + 1 || h == l {
                            table(lo.pc(), l, None);
                        } else {
                            table(lo.pc(), l, Some(h));
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                for (r, changed) in changes(m, ops).into_iter().enumerate() {
                    if changed {
                        regs[r] = None;
                    }
                }
            }
        }
        // A call may change any register, and use (or change) the pointers.
        if matches!(insn.flow, Flow::Call(_)) {
            regs = [None; 3];
            stored.clear();
        }
    });
    // Each table's entries, while they point into the ROM.
    let byte = |at: u64| (code.bytes_at)(at).and_then(|b| b.first().copied()).map(u64::from);
    for (lo, high) in tables {
        let (stride, mut max) = if high.is_some() { (1, 256) } else { (2, 128) };
        // Tables side by side are as long as the space between them.
        if let Some(h) = high
            && (1..=256).contains(&h.abs_diff(lo))
        {
            max = h.abs_diff(lo);
        }
        let mut entries = 0;
        for i in 0..max {
            let at = lo + i * stride;
            let hi_at = high.map_or(at + 1, |h| h + i);
            if (code.is_code)(at) || (code.is_code)(hi_at) {
                break;
            }
            // Where the code reads something else, another table starts.
            if i > 0 && (code.read.contains(&at) || code.read.contains(&hi_at)) {
                break;
            }
            let (Some(l), Some(h)) = (byte(at), byte(hi_at)) else {
                break;
            };
            let Some(target) = (code.resolve)(at, bank(at) | h << 8 | l).filter(|&t| (code.bytes_at)(t).is_some())
            else {
                break;
            };
            found.refs.push((at, target, RefKind::Pointer));
            entries += 1;
        }
        if entries > 0 {
            found.tables.push(Table {
                address: lo,
                high,
                entries,
            });
        }
    }
    found
}

impl Binary {
    /// Words in a ROM's data holding one of the addresses `lo..hi` (a string,
    /// a table): pointers to it that following the code didn't find. A word
    /// is read as the code in its bank would read it; one naming a window
    /// whose bank switches counts when it is in that bank, or among other
    /// words pointing into the same window (a table of them). Addresses code
    /// builds in such a window count too.
    pub(crate) fn rom_pointers_to(&self, lo: u64, hi: u64) -> Vec<(u64, u64, RefKind)> {
        const MAX: usize = 200;
        let Some(rom) = &self.rom else { return Vec::new() };
        let mut out = Vec::new();
        let size = hi.saturating_sub(lo).max(1);
        let target_cpu = rom.map.cpu(lo);
        let words = |sec: &crate::model::Section| {
            let off = sec.file_offset? as usize;
            self.data.get(off..off + sec.file_size as usize)
        };
        // Bytes of tables the code reads are accounted for: their entries, not words across them.
        let tables: Vec<(u64, u64)> = rom
            .analysis
            .tables
            .iter()
            .flat_map(|t| {
                let n = u64::from(t.entries);
                [
                    Some((t.address, t.address + n * t.stride())),
                    t.high.map(|h| (h, h + n)),
                ]
            })
            .flatten()
            .collect();
        let not_code = |at: u64, n: u64| {
            (at..at + n).all(|a| !self.in_rom_code(a) && !tables.iter().any(|&(s, e)| a >= s && a < e))
        };
        match &rom.map {
            Map::Banked(windows) => {
                let Some(w) = windows.iter().find(|w| target_cpu >= w.lo && target_cpu < w.hi) else {
                    return out;
                };
                let (vlo, vhi) = (target_cpu, (target_cpu + size).min(w.hi));
                let switched = w.banks.len() > 1;
                for sec in self.sections.iter().filter(|s| s.loaded && s.file_offset.is_some()) {
                    let Some(bytes) = words(sec) else { continue };
                    let word = |i: usize| bytes.get(i..i + 2).map(|b| u64::from(u16::from_le_bytes([b[0], b[1]])));
                    for i in 0..bytes.len().saturating_sub(1) {
                        let v = u64::from(bytes[i]) | u64::from(bytes[i + 1]) << 8;
                        if v < vlo || v >= vhi {
                            continue;
                        }
                        let at = sec.address + i as u64;
                        if !not_code(at, 2) {
                            continue;
                        }
                        let target = lo + (v - vlo);
                        // Read by code in a bank of the window, the word means that bank;
                        // from elsewhere, any: then only among words pointing into it.
                        let seen = rom.map.resolve(at, v);
                        let in_window = |j: Option<usize>| j.and_then(word).is_some_and(|v| v >= w.lo && v < w.hi);
                        let table = in_window(i.checked_sub(2)) || in_window(Some(i + 2));
                        if seen == Some(target) || (switched && seen.is_none() && table) {
                            out.push((at, target, RefKind::Pointer));
                            if out.len() >= MAX {
                                return out;
                            }
                        }
                    }
                }
                for &(pc, cpu) in &rom.analysis.unplaced {
                    let v = cpu & 0xFFFF;
                    if v >= vlo && v < vhi {
                        out.push((pc, lo + (v - vlo), RefKind::Address));
                    }
                }
            }
            Map::Snes { .. } => {
                let low = target_cpu & 0xFFFF;
                for sec in self.sections.iter().filter(|s| s.loaded && s.file_offset.is_some()) {
                    let Some(bytes) = words(sec) else { continue };
                    for i in 0..bytes.len().saturating_sub(1) {
                        let v16 = u64::from(bytes[i]) | u64::from(bytes[i + 1]) << 8;
                        // Mirrors share the low 16 bits.
                        if v16.wrapping_sub(low) & 0xFFFF >= size {
                            continue;
                        }
                        let at = sec.address + i as u64;
                        if !not_code(at, 2) {
                            continue;
                        }
                        // A long pointer (three bytes), or a short one into its own bank.
                        let long = bytes
                            .get(i + 2)
                            .and_then(|&b| rom.map.resolve(at, v16 | u64::from(b) << 16));
                        let short = rom.map.resolve(at, rom.map.cpu(at) & 0xFF_0000 | v16);
                        if let Some(t) = [long, short].into_iter().flatten().find(|t| *t >= lo && *t < lo + size) {
                            out.push((at, t, RefKind::Pointer));
                            if out.len() >= MAX {
                                return out;
                            }
                        }
                    }
                }
            }
            Map::Flat(_) => {
                let big = self.endian == crate::util::Endian::Big;
                let low = |v: u64| v & 0xFFFF;
                for sec in self.sections.iter().filter(|s| s.loaded && s.file_offset.is_some()) {
                    let Some(bytes) = words(sec) else { continue };
                    let start = (sec.address.wrapping_neg() & 3) as usize;
                    for i in (start..bytes.len().saturating_sub(3)).step_by(4) {
                        let b = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
                        let v = u64::from(if big {
                            u32::from_be_bytes(b)
                        } else {
                            u32::from_le_bytes(b)
                        });
                        // Mirrors differ above the low 16 bits: those must match.
                        if size < 0x1_0000 && (low(v).wrapping_sub(low(target_cpu)) & 0xFFFF) >= size {
                            continue;
                        }
                        let at = sec.address + i as u64;
                        if let Some(t) = rom.map.resolve(at, v).filter(|t| *t >= lo && *t < lo + size)
                            && not_code(at, 4)
                        {
                            out.push((at, t, RefKind::Pointer));
                            if out.len() >= MAX {
                                return out;
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operands() {
        assert_eq!(immediate("#$1F"), Some((0x1F, false)));
        assert_eq!(immediate("#$0200"), Some((0x200, true)));
        assert_eq!(immediate("$10"), None);
        assert_eq!(indexed_absolute("$C120,x"), Some(true));
        assert_eq!(indexed_absolute("$C120,y"), Some(false));
        assert_eq!(indexed_absolute("$10,x"), None);
        assert_eq!(indexed_absolute("$C120"), None);
        for p in ["($10),y", "($10,x)", "($10)", "[$10]", "[$10],y", "($0200)"] {
            assert!(through_pointer(p), "{p}");
        }
        for p in ["($C000,x)", "($03,s),y", "$10", "$C000,x"] {
            assert!(!through_pointer(p), "{p}");
        }
        assert_eq!(changes("tax", ""), [false, true, false]);
        assert_eq!(changes("asl", "a"), [true, false, false]);
        assert_eq!(changes("asl", "$10"), [false, false, false]);
    }
}
