//! A function's code as GNU assembler source, the way splat writes it, for
//! the tools that take it: m2c's first draft of the C, a decompilation's
//! `asm/nonmatchings` files, decomp-permuter's target. MIPS (the
//! PlayStation's, the Nintendo 64's) for now.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::binary::Binary;
use crate::cpu::mips::{CALL_CLOBBERS, MipsWord};
use crate::model::{FlowKind, Instruction, SymbolKind};

/// `$gp` and `$sp`.
const GP: u32 = 28;
const SP: u32 = 29;

impl Binary {
    /// The function at `address` as GNU assembler source (MIPS): `glabel`,
    /// each instruction with its file offset, address and word in a
    /// comment, branches to `.L` labels, calls by name, `%hi`/`%lo` where
    /// one `lui` starts an address on every path to the instruction that
    /// finishes it, `%gp_rel` into the small data, and the jump tables its
    /// `switch`es read, as `jtbl_` in `.rodata`. `None` for code that isn't
    /// MIPS.
    pub fn gnu_asm(&self, address: u64) -> Option<String> {
        let little = self.mips_endian()? == crate::util::Endian::Little;
        let f = self.symbols().function_containing(address)?;
        let start = f.address;
        let name = asm_name(f.name(), start, true);
        let d = self.disassemble_function(start, 20_000);
        let ins = &d.instructions;
        if !d.supported || ins.is_empty() {
            return None;
        }
        let words: Vec<MipsWord> = ins.iter().map(|i| MipsWord(word(&i.bytes, little))).collect();
        let index: HashMap<u64, usize> = ins.iter().enumerate().map(|(k, i)| (i.address, k)).collect();
        // The tables its indirect jumps read, when every entry leads into the function.
        let jumps: Vec<(usize, _)> = ins
            .iter()
            .enumerate()
            .filter(|(_, i)| i.flow == FlowKind::Jump && i.target.is_none())
            .filter_map(|(k, i)| Some((k, self.mips_jump_table(i.address)?)))
            .filter(|(_, t)| !t.targets.is_empty() && t.targets.iter().all(|a| index.contains_key(a)))
            .collect();
        let tables: BTreeMap<u64, &[u64]> = jumps.iter().map(|(_, t)| (t.address, &t.targets[..])).collect();
        let labels: BTreeSet<u64> = ins
            .iter()
            .filter(|i| matches!(i.flow, FlowKind::Jump | FlowKind::CondJump))
            .filter_map(|i| i.target)
            .chain(tables.values().flat_map(|t| t.iter().copied()))
            .filter(|t| index.contains_key(t))
            .collect();
        let flow = Flow::new(ins, &|k| {
            jumps
                .iter()
                .find(|(j, _)| *j == k)
                .map(|(_, t)| t.targets.iter().filter_map(|a| index.get(a).copied()).collect())
        });
        // Each low half: the `lui` its address starts with, and the address.
        let mut low: BTreeMap<usize, (usize, u64)> = BTreeMap::new();
        let mut gp_rel: HashMap<usize, u64> = HashMap::new();
        for (k, w) in words.iter().enumerate() {
            if !is_low_half(*w) {
                continue;
            }
            match (w.rs(), flow.writer(&words, k, w.rs())) {
                // The stack's top is no symbol (and m2c reads `addiu $sp` as the frame).
                (SP, _) => {}
                (GP, Writer::Entry) => {
                    if let Some(t) = ins[k].target {
                        gp_rel.insert(k, t);
                    }
                }
                (_, Writer::Only(j)) if words[j].op() == 15 => {
                    low.insert(k, (j, u64::from(hi_lo(words[j], *w))));
                }
                _ => {}
            }
        }
        // A switch's `lw` reads the table through the index added to the `lui`.
        for (_, t) in &jumps {
            if let Some((hi, lw)) = t.halves
                && let (Some(&j), Some(&k)) = (index.get(&hi), index.get(&lw))
                && u64::from(hi_lo(words[j], words[k])) == t.address
            {
                low.insert(k, (j, t.address));
            }
        }
        // The addresses a `lui` starts share their high half: any of them names it.
        let mut high: HashMap<usize, u64> = HashMap::new();
        for &(j, t) in low.values() {
            high.entry(j).or_insert(t);
        }
        let rom = self.rom.as_ref();
        let data_name = |from: u64, t: u64| -> String {
            if tables.contains_key(&t) {
                return format!("jtbl_{t:08X}");
            }
            // Where the CPU's address is another of ours (an overlay), the CPU's names it.
            if rom.and_then(|r| r.map.resolve(from, t)).is_some_and(|ours| ours != t) {
                return format!("D_{t:08X}");
            }
            match self.symbols().lookup(t) {
                Some(s) => {
                    let code = self
                        .symbols()
                        .get(s.index)
                        .is_some_and(|s| s.kind == SymbolKind::Function);
                    let base = asm_name(&s.name, s.address, code);
                    if s.offset == 0 {
                        base
                    } else {
                        format!("{base}+{:#x}", s.offset)
                    }
                }
                None => format!("D_{t:08X}"),
            }
        };
        let label = |t: u64| {
            if t == start { name.clone() } else { format!(".L{t:08X}") }
        };
        let mut out = String::from(".set noat\n.set noreorder\n\n.section .text\n\n");
        out.push_str(&format!("glabel {name}\n"));
        for (k, i) in ins.iter().enumerate() {
            if labels.contains(&i.address) && i.address != start {
                out.push_str(&format!(".L{:08X}:\n", i.address));
            }
            let (mnemonic, mut operands) = spelled_out(words[k], &i.mnemonic, &i.operands)
                .unwrap_or_else(|| (i.mnemonic.clone(), i.operands.clone()));
            match i.target {
                Some(t) if matches!(i.flow, FlowKind::Jump | FlowKind::CondJump) && labels.contains(&t) => {
                    operands = replace_last_number(&operands, &label(t));
                }
                Some(t) if matches!(i.flow, FlowKind::Call | FlowKind::Jump | FlowKind::CondJump) => {
                    let callee = self
                        .symbols()
                        .at(t)
                        .map_or_else(|| format!("func_{t:08X}"), |s| asm_name(s.name(), t, true));
                    operands = replace_last_number(&operands, &callee);
                }
                _ => {}
            }
            if let Some(&t) = high.get(&k) {
                operands = replace_last_number(&operands, &format!("%hi({})", data_name(i.address, t)));
            } else if let Some(&(_, t)) = low.get(&k) {
                operands = replace_last_number(&operands, &format!("%lo({})", data_name(i.address, t)));
            } else if let Some(&t) = gp_rel.get(&k) {
                operands = replace_last_number(&operands, &format!("%gp_rel({})", data_name(i.address, t)));
            }
            let offset = i.offset.map_or(String::new(), |o| format!("{o:X} "));
            let line = format!(
                "/* {offset}{:08X} {:08X} */  {mnemonic:<9} {operands}",
                i.address, words[k].0
            );
            out.push_str(line.trim_end());
            out.push('\n');
        }
        for (address, targets) in &tables {
            out.push_str(&format!("\n.section .rodata\n\nglabel jtbl_{address:08X}\n"));
            for &t in targets.iter() {
                out.push_str(&format!(".word {}\n", label(t)));
            }
        }
        Some(out)
    }
}

/// How control reaches each of a function's instructions, MIPS delay slots
/// included: a jump's next instruction runs before it lands; a
/// branch-likely's, only when it's taken.
struct Flow {
    preds: Vec<Vec<usize>>,
    /// Delay slots of calls: the callee runs after them.
    before_callee: Vec<bool>,
}

/// Which write to a register an instruction sees.
#[derive(Debug, PartialEq)]
enum Writer {
    /// This instruction's, on every path.
    Only(usize),
    /// None in the function: the caller's value.
    Entry,
    /// Several, or not known.
    Mixed,
}

impl Flow {
    /// The flow between `ins`, an indirect jump at `k` going where
    /// `table(k)` says (none known: it leaves).
    fn new(ins: &[Instruction], table: &dyn Fn(usize) -> Option<Vec<usize>>) -> Flow {
        let n = ins.len();
        let mut preds = vec![Vec::new(); n];
        let mut before_callee = vec![false; n];
        let at: HashMap<u64, usize> = ins.iter().enumerate().map(|(k, i)| (i.address, k)).collect();
        let next =
            |k: usize| (k + 1 < n && ins[k + 1].address == ins[k].address + u64::from(ins[k].len)).then_some(k + 1);
        let delayed = |i: &Instruction| {
            matches!(
                i.flow,
                FlowKind::Jump | FlowKind::CondJump | FlowKind::Call | FlowKind::Return
            ) && i.mnemonic != "eret"
        };
        let mut edge = |from: usize, to: Option<usize>| {
            if let Some(to) = to {
                preds[to].push(from);
            }
        };
        let mut k = 0;
        while k < n {
            let i = &ins[k];
            if !delayed(i) {
                if !matches!(i.flow, FlowKind::Return | FlowKind::Invalid) {
                    edge(k, next(k));
                }
                k += 1;
                continue;
            }
            let Some(slot) = next(k) else { break };
            edge(k, Some(slot));
            let target = i.target.and_then(|t| at.get(&t).copied());
            match i.flow {
                FlowKind::Jump => match target {
                    Some(t) => edge(slot, Some(t)),
                    None => table(k)
                        .unwrap_or_default()
                        .into_iter()
                        .for_each(|t| edge(slot, Some(t))),
                },
                FlowKind::CondJump => {
                    edge(slot, target);
                    // Branch-likely (`beql`): not taken, it skips its delay slot.
                    if i.mnemonic.ends_with('l') {
                        edge(k, next(slot));
                    } else {
                        edge(slot, next(slot));
                    }
                }
                FlowKind::Call => {
                    before_callee[slot] = true;
                    edge(slot, next(slot));
                }
                _ => {}
            }
            k = slot + 1;
        }
        Flow { preds, before_callee }
    }

    /// The write to `reg` the instruction at `k` sees.
    fn writer(&self, words: &[MipsWord], k: usize, reg: u32) -> Writer {
        if k != 0 && self.preds[k].is_empty() {
            return Writer::Mixed;
        }
        let (mut found, mut entry) = (None, k == 0);
        let mut seen = vec![false; words.len()];
        let mut stack = self.preds[k].clone();
        while let Some(p) = stack.pop() {
            if std::mem::replace(&mut seen[p], true) {
                continue;
            }
            // The callee ran after this, and changed the registers it may.
            if self.before_callee[p] && CALL_CLOBBERS & (1 << reg) != 0 {
                return Writer::Mixed;
            }
            if words[p].writes() == Some(reg) {
                if found.is_some_and(|f| f != p) {
                    return Writer::Mixed;
                }
                found = Some(p);
                continue;
            }
            if p == 0 {
                entry = true;
            } else if self.preds[p].is_empty() {
                return Writer::Mixed;
            }
            stack.extend(&self.preds[p]);
        }
        match (found, entry) {
            (Some(j), false) => Writer::Only(j),
            (None, true) => Writer::Entry,
            _ => Writer::Mixed,
        }
    }
}

/// The instruction as an assembler reads it back to the same word, where
/// the disassembly prints it another way: pseudo-instructions with more
/// than one encoding spelled out (`move` is `addu`, `or` or `daddu` with
/// `$zero`; `li`, `addiu` or `ori`; `b`, `bgez $zero` as well as `beq`),
/// and `div`'s `$zero` written (to GNU as, `div rs, rt` is the macro that
/// divides into `rs` and checks for zero).
fn spelled_out(w: MipsWord, mnemonic: &str, operands: &str) -> Option<(String, String)> {
    let (first, rest) = operands.split_once(", ").unwrap_or((operands, ""));
    let (m, ops) = match (mnemonic, w.op()) {
        ("move", _) => {
            let m = match w.funct() {
                33 => "addu",
                37 => "or",
                _ => "daddu",
            };
            (m, format!("{first}, {rest}, $zero"))
        }
        ("li", 13) => ("ori", format!("{first}, $zero, {rest}")),
        ("li", 25) => ("daddiu", format!("{first}, $zero, {rest}")),
        ("li", _) => ("addiu", format!("{first}, $zero, {rest}")),
        ("b", 1) => ("bgez", format!("$zero, {operands}")),
        ("div" | "divu" | "ddiv" | "ddivu", _) => (mnemonic, format!("$zero, {operands}")),
        _ => return None,
    };
    Some((m.to_string(), ops))
}

/// Loads and stores (`lw $v0, lo($t0)`) and `addiu` (`addiu $a0, $t0, lo`):
/// the instructions that finish an address a `lui` starts.
fn is_low_half(w: MipsWord) -> bool {
    matches!(w.op(), 9 | 26 | 27 | 32..=46 | 49 | 50 | 53..=55 | 57 | 58 | 61..=63)
}

/// The address a `lui` and a low half make together.
fn hi_lo(hi: MipsWord, lo: MipsWord) -> u32 {
    (hi.imm() << 16).wrapping_add(lo.simm() as u32)
}

/// A name as an assembler symbol: its own if the assembler takes it (a
/// mangled C++ name does), splat's for one made up here (`func_80010000`
/// for code, `D_80020000` for data).
fn asm_name(name: &str, address: u64, code: bool) -> String {
    if name.is_empty() || crate::names::is_made_up(name) {
        return if code {
            format!("func_{address:08X}")
        } else {
            format!("D_{address:08X}")
        };
    }
    let ok = !name.starts_with(|c: char| c.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$'));
    if ok {
        name.to_string()
    } else {
        name.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    }
}

/// The instruction word, from its bytes in hex.
fn word(bytes: &str, little: bool) -> u32 {
    let mut b: Vec<&str> = bytes.split_whitespace().collect();
    if little {
        b.reverse();
    }
    u32::from_str_radix(&b.concat(), 16).unwrap_or(0)
}

/// `operands` with its last number (`0x80010040`, `-0x10`, `0x1074` in
/// `0x1074($t0)`) replaced; registers (`$t0`, `$f12`) aren't numbers.
fn replace_last_number(operands: &str, with: &str) -> String {
    let b = operands.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$';
    let mut last = None;
    let mut i = 0;
    while i < b.len() {
        if !word(b[i]) {
            i += 1;
            continue;
        }
        let s = i;
        while i < b.len() && word(b[i]) {
            i += 1;
        }
        if b[s].is_ascii_digit() {
            let s = if s > 0 && b[s - 1] == b'-' { s - 1 } else { s };
            last = Some((s, i));
        }
    }
    match last {
        Some((s, e)) => format!("{}{with}{}", &operands[..s], &operands[e..]),
        None => operands.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{Cpu, State, decode};

    #[test]
    fn operands_take_labels_and_halves() {
        assert_eq!(
            replace_last_number("$a0, $zero, 0x80010040", ".L80010040"),
            "$a0, $zero, .L80010040"
        );
        assert_eq!(
            replace_last_number("$zero, 0x1074($t0)", "%lo(I_MASK)"),
            "$zero, %lo(I_MASK)($t0)"
        );
        assert_eq!(replace_last_number("$sp, $sp, -0x18", "X"), "$sp, $sp, X");
        assert_eq!(replace_last_number("$t0, 0x1f80", "%hi(I_MASK)"), "$t0, %hi(I_MASK)");
        assert_eq!(replace_last_number("0x80010020", "func"), "func");
        assert_eq!(replace_last_number("$zero, 0x0($t0)", "%lo(X)"), "$zero, %lo(X)($t0)");
        assert_eq!(replace_last_number("$f12, $f14", "X"), "$f12, $f14");
        let spelled = |w: u32, m: &str, ops: &str| spelled_out(MipsWord(w), m, ops).map(|(m, o)| format!("{m} {o}"));
        assert_eq!(
            spelled(0x0000_1021, "move", "$v0, $zero").as_deref(),
            Some("addu $v0, $zero, $zero")
        );
        assert_eq!(
            spelled(0x0080_1025, "move", "$v0, $a0").as_deref(),
            Some("or $v0, $a0, $zero")
        );
        assert_eq!(
            spelled(0x240A_00A0, "li", "$t2, 0xa0").as_deref(),
            Some("addiu $t2, $zero, 0xa0")
        );
        assert_eq!(
            spelled(0x3408_0010, "li", "$t0, 0x10").as_deref(),
            Some("ori $t0, $zero, 0x10")
        );
        assert_eq!(
            spelled(0x0401_0003, "b", "0x80010010").as_deref(),
            Some("bgez $zero, 0x80010010")
        );
        assert_eq!(spelled(0x1000_0003, "b", "0x80010010"), None);
        assert_eq!(
            spelled(0x0085_001A, "div", "$a0, $a1").as_deref(),
            Some("div $zero, $a0, $a1")
        );
        assert_eq!(hi_lo(MipsWord(0x3C08_1F80), MipsWord(0xAD00_1074)), 0x1F80_1074);
        assert_eq!(hi_lo(MipsWord(0x3C1D_8040), MipsWord(0x27BD_FFF0)), 0x803F_FFF0);
        assert_eq!(asm_name("sub_80010020", 0x8001_0020, true), "func_80010020");
        assert_eq!(asm_name("dword_80020000", 0x8002_0000, false), "D_80020000");
        assert_eq!(asm_name("Shape::area", 0x8001_0020, true), "Shape__area");
        assert_eq!(asm_name("_ZN5Shape4areaEv", 0x8001_0020, true), "_ZN5Shape4areaEv");
    }

    /// Instructions decoded from `words` (R3000) at 0x80010000.
    fn code(words: &[u32]) -> (Vec<Instruction>, Vec<MipsWord>) {
        let mut state = State::default();
        let mut out = Vec::new();
        for (k, &w) in words.iter().enumerate() {
            let address = 0x8001_0000 + 4 * k as u64;
            let insn = decode(Cpu::MipsR3000, &w.to_le_bytes(), address, &mut state).expect("an instruction");
            out.push(Instruction {
                address,
                offset: None,
                len: 4,
                bytes: String::new(),
                mnemonic: insn.mnemonic,
                operands: insn.operands,
                flow: insn.flow.kind(),
                target: insn.flow.target(),
                target_symbol: None,
                source: None,
            });
        }
        (out, words.iter().map(|&w| MipsWord(w)).collect())
    }

    #[test]
    fn a_high_half_is_the_one_every_path_brings() {
        let (ins, words) = code(&[
            0x3C04_8002, // 0: lui   $a0, 0x8002
            0x8C82_0010, // 1: lw    $v0, 0x10($a0)    (the lui's)
            0x1040_0003, // 2: beqz  $v0, 6
            0x0000_0000, // 3: nop
            0x3C04_8003, // 4: lui   $a0, 0x8003
            0x0000_0000, // 5: nop
            0x8C83_0020, // 6: lw    $v1, 0x20($a0)    (either lui's)
            0x1000_FFF8, // 7: b     0 (the loop)
            0x0000_0000, // 8: nop
        ]);
        let flow = Flow::new(&ins, &|_| None);
        assert_eq!(flow.preds[6], vec![3, 5]);
        assert_eq!(flow.writer(&words, 1, 4), Writer::Only(0));
        assert_eq!(flow.writer(&words, 6, 4), Writer::Mixed);
        // $a1 is never written: what the caller left, even round the loop.
        assert_eq!(flow.writer(&words, 6, 5), Writer::Entry);
    }

    #[test]
    fn a_callee_changes_the_registers_it_may() {
        let (ins, words) = code(&[
            0x3C04_8002, // 0: lui   $a0, 0x8002
            0x3C10_8002, // 1: lui   $s0, 0x8002
            0x0C00_4010, // 2: jal   0x80010040
            0x8C82_0010, // 3: lw    $v0, 0x10($a0)    (the delay slot: before the callee)
            0x8C83_0020, // 4: lw    $v1, 0x20($a0)    ($a0 is the callee's now)
            0x8E03_0020, // 5: lw    $v1, 0x20($s0)    ($s0 is kept)
        ]);
        let flow = Flow::new(&ins, &|_| None);
        assert_eq!(flow.writer(&words, 3, 4), Writer::Only(0));
        assert_eq!(flow.writer(&words, 4, 4), Writer::Mixed);
        assert_eq!(flow.writer(&words, 5, 16), Writer::Only(1));
    }

    /// A PS-X EXE whose code at 0x80010000 is `words`.
    fn psx(words: &[u32]) -> Binary {
        let mut exe = vec![0u8; 0x1000];
        exe[..8].copy_from_slice(b"PS-X EXE");
        for (at, v) in [
            (0x10, 0x8001_0000u32),
            (0x14, 0x8001_8000),
            (0x18, 0x8001_0000),
            (0x1C, 0x800),
        ] {
            exe[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        for (k, w) in words.iter().enumerate() {
            exe[0x800 + 4 * k..0x800 + 4 * k + 4].copy_from_slice(&w.to_le_bytes());
        }
        Binary::parse(exe).expect("a PS-X EXE")
    }

    #[test]
    fn a_switch_reads_its_table_from_rodata() {
        let b = psx(&[
            0x2C81_0003, // sltiu $at, $a0, 3
            0x1020_000C, // beqz  $at, 0x80010038 (the default)
            0x0004_7080, // sll   $t6, $a0, 2
            0x3C01_8001, // lui   $at, 0x8001
            0x002E_0821, // addu  $at, $at, $t6
            0x8C2E_0040, // lw    $t6, 0x40($at): the table at 0x80010040
            0x01C0_0008, // jr    $t6
            0x0000_0000, // nop
            0x03E0_0008, // case 0: jr $ra
            0x2402_0001, //   li $v0, 1
            0x03E0_0008, // case 1: jr $ra
            0x2402_0002, //   li $v0, 2
            0x03E0_0008, // case 2: jr $ra
            0x2402_0003, //   li $v0, 3
            0x03E0_0008, // default: jr $ra
            0x0000_1021, //   move $v0, $zero
            0x8001_0020, // the table
            0x8001_0028,
            0x8001_0030,
        ]);
        let asm = b.gnu_asm(0x8001_0000).expect("MIPS");
        assert!(asm.contains("lui       $at, %hi(jtbl_80010040)\n"), "{asm}");
        assert!(asm.contains("lw        $t6, %lo(jtbl_80010040)($at)\n"), "{asm}");
        assert!(asm.contains("beqz      $at, .L80010038\n"), "{asm}");
        assert!(asm.contains(".L80010028:\n"), "{asm}");
        assert!(
            asm.ends_with(
                ".section .rodata\n\nglabel jtbl_80010040\n.word .L80010020\n.word .L80010028\n.word .L80010030\n"
            ),
            "{asm}"
        );
    }

    #[test]
    fn a_likely_branch_runs_its_delay_slot_only_when_taken() {
        let (ins, words) = code(&[
            0x5040_0002, // 0: beqzl $v0, 3
            0x3C04_8002, // 1: lui   $a0, 0x8002       (taken only)
            0x3C04_8003, // 2: lui   $a0, 0x8003       (not taken)
            0x8C82_0010, // 3: lw    $v0, 0x10($a0)
        ]);
        let flow = Flow::new(&ins, &|_| None);
        assert_eq!(flow.preds[2], vec![0]);
        assert_eq!(flow.preds[3], vec![1, 2]);
        assert_eq!(flow.writer(&words, 3, 4), Writer::Mixed);
    }
}
