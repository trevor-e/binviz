//! Jump tables: where an indirect jump goes when its targets are listed
//! next to it, in the idioms games' code uses most:
//! - 65816 `jmp ($nnnn,x)` and `jsr ($nnnn,x)`: 16-bit addresses at
//!   `$nnnn`, in the code's bank;
//! - the 6502's return trick, `lda hi,x / pha / lda lo,x / pha / rts`: each
//!   address minus one, split between two tables of bytes;
//! - the 68000's `jmp d(pc,dn.w)`: after `move.w d(pc,dn.w),dn`, 16-bit
//!   offsets from the table; otherwise a table of branches to jump into;
//! - ARM `ldr pc, [pc, rn, lsl #2]`: 32-bit addresses right after it (past
//!   one instruction), as many as a `cmp` before it allows;
//! - Thumb `mov pc, rx` or `bx rx` after `ldr rx, [rx]` and `add rx, ra, rb`,
//!   one of which a literal pool loaded with the table's address;
//! - MIPS `jr $rx` after `lw $rx, lo(table)($at)`, the table's address built
//!   by `lui` (and `addiu`) and the index added to it, scaled by `sll … 2`
//!   (what GCC and IDO write for a `switch`): 32-bit addresses at the table,
//!   as many as the `sltiu` guard before allows.
//!
//! A table ends where its entries stop pointing at code, where the code it
//! points to begins, or at a bound the code checks the index against.

use crate::cpu::Cpu;
use crate::cpu::mips::MipsWord;

/// What reading tables needs from the analysis.
pub(crate) struct Code<'a, 'd> {
    /// The bytes at one of our addresses, to the end of their section.
    pub bytes_at: &'a dyn Fn(u64) -> Option<&'d [u8]>,
    /// Our address for a CPU address, named by code at one of ours.
    pub resolve: &'a dyn Fn(u64, u64) -> Option<u64>,
    /// Whether there may be code at one of our addresses (a code/data log may say it is only data).
    pub may_be_code: &'a dyn Fn(u64) -> bool,
}

/// At most this many entries per table.
const MAX: usize = 256;

impl Code<'_, '_> {
    fn bytes(&self, at: u64, n: usize) -> Option<&[u8]> {
        (self.bytes_at)(at).and_then(|b| b.get(..n))
    }

    fn u8(&self, at: u64) -> Option<u64> {
        self.bytes(at, 1).map(|b| u64::from(b[0]))
    }

    fn u16le(&self, at: u64) -> Option<u64> {
        self.bytes(at, 2).map(|b| u64::from(u16::from_le_bytes([b[0], b[1]])))
    }

    fn u16be(&self, at: u64) -> Option<u64> {
        self.bytes(at, 2).map(|b| u64::from(u16::from_be_bytes([b[0], b[1]])))
    }

    fn u32le(&self, at: u64) -> Option<u64> {
        self.bytes(at, 4)
            .map(|b| u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
    }

    /// Our address for a CPU address, if code may be there.
    fn code(&self, from: u64, cpu: u64) -> Option<u64> {
        let a = (self.resolve)(from, cpu)?;
        ((self.bytes_at)(a).is_some() && (self.may_be_code)(a)).then_some(a)
    }
}

/// The targets of the indirect jump (or return) at `pc` (our address;
/// `cpu_pc` the CPU's), when it goes through a table: our addresses, and
/// for ARM code whether each is Thumb.
pub(crate) fn targets(cpu: Cpu, pc: u64, cpu_pc: u64, thumb: bool, code: &Code) -> Vec<(u64, Option<bool>)> {
    let found = match cpu {
        Cpu::Mos6502 | Cpu::W65816 => m65(cpu, pc, cpu_pc, code),
        Cpu::M68000 => m68k(pc, cpu_pc, code),
        Cpu::Arm7Tdmi if thumb => thumb_table(pc, cpu_pc, code),
        Cpu::Arm7Tdmi => arm(pc, cpu_pc, code),
        Cpu::MipsR3000 => mips(pc, code, false),
        Cpu::MipsR4300 => mips(pc, code, true),
        _ => Vec::new(),
    };
    let mut seen = std::collections::HashSet::new();
    found.into_iter().filter(|t| seen.insert(t.0)).collect()
}

/// Entries read in turn by `entry` (our address, or None to stop) while
/// the table hasn't run into the code its entries point to.
fn read_table(start: u64, stride: u64, max: usize, mut entry: impl FnMut(usize) -> Option<u64>) -> Vec<u64> {
    let mut out = Vec::new();
    let mut lowest = u64::MAX;
    for i in 0..max {
        // A table placed before the code it points to ends where that code starts.
        if start + i as u64 * stride >= lowest && lowest > start {
            break;
        }
        let Some(t) = entry(i) else { break };
        if t > start {
            lowest = lowest.min(t);
        }
        out.push(t);
    }
    out
}

fn m65(cpu: Cpu, pc: u64, cpu_pc: u64, code: &Code) -> Vec<(u64, Option<bool>)> {
    let Some(op) = code.u8(pc) else { return Vec::new() };
    let bank = cpu_pc & 0xFF_0000;
    match op {
        // jmp ($nnnn,x), jsr ($nnnn,x): a table of 16-bit addresses in the code's bank.
        0x7C | 0xFC if cpu == Cpu::W65816 => {
            let Some(table) = code.u16le(pc + 1).and_then(|t| (code.resolve)(pc, bank | t)) else {
                return Vec::new();
            };
            read_table(table, 2, MAX, |i| {
                let e = code.u16le(table + 2 * i as u64)?;
                code.code(pc, bank | e)
            })
            .into_iter()
            .map(|t| (t, None))
            .collect()
        }
        // jmp ($nnnn), the pointer in ROM.
        0x6C => code
            .u16le(pc + 1)
            .and_then(|p| (code.resolve)(pc, bank | p))
            .and_then(|p| code.u16le(p))
            .and_then(|t| code.code(pc, bank | t))
            .map(|t| vec![(t, None)])
            .unwrap_or_default(),
        // rts after pushing a table's entry: lda hi,x / pha / lda lo,x / pha.
        0x60 => {
            let Some(b) = pc.checked_sub(8).and_then(|at| code.bytes(at, 8)) else {
                return Vec::new();
            };
            let lda = |o: u8| o == 0xBD || o == 0xB9;
            if !(lda(b[0]) && b[3] == 0x48 && lda(b[4]) && b[7] == 0x48 && b[0] == b[4]) {
                return Vec::new();
            }
            let hi = u64::from(u16::from_le_bytes([b[1], b[2]]));
            let lo = u64::from(u16::from_le_bytes([b[5], b[6]]));
            // Tables side by side are as long as the space between them.
            let max = if hi.abs_diff(lo) > 0 && hi.abs_diff(lo) <= 128 {
                hi.abs_diff(lo) as usize
            } else {
                128
            };
            let (Some(hi_at), Some(lo_at)) = ((code.resolve)(pc, bank | hi), (code.resolve)(pc, bank | lo)) else {
                return Vec::new();
            };
            read_table(hi_at.min(lo_at), 1, max, |i| {
                let t = (code.u8(hi_at + i as u64)? << 8 | code.u8(lo_at + i as u64)?) + 1;
                code.code(pc, bank | t)
            })
            .into_iter()
            .map(|t| (t, None))
            .collect()
        }
        _ => Vec::new(),
    }
}

fn m68k(pc: u64, cpu_pc: u64, code: &Code) -> Vec<(u64, Option<bool>)> {
    // jmp (d8,pc,xn) or jsr (d8,pc,xn), with a brief extension word.
    let (Some(op), Some(ext)) = (code.u16be(pc), code.u16be(pc + 2)) else {
        return Vec::new();
    };
    if (op != 0x4EFB && op != 0x4EBB) || ext & 0x0100 != 0 {
        return Vec::new();
    }
    let d8 = |ext: u64| (ext & 0xFF) as u8 as i8 as i64;
    let base = (cpu_pc as i64 + 2 + d8(ext)) as u64;
    let Some(table) = (code.resolve)(pc, base) else {
        return Vec::new();
    };
    // move.w (d8,pc,xn),dn just before, reading the same table: offsets from it.
    let offsets = pc.checked_sub(4).is_some_and(|at| {
        code.u16be(at).is_some_and(|o| o & 0xF1FF == 0x303B)
            && code
                .u16be(at + 2)
                .is_some_and(|e| e & 0x0100 == 0 && (cpu_pc as i64 - 4 + 2 + d8(e)) as u64 == base)
    });
    if offsets {
        return read_table(table, 2, MAX, |i| {
            let e = code.u16be(table + 2 * i as u64)? as u16 as i16 as i64;
            if e <= 0 || e & 1 != 0 {
                return None;
            }
            code.code(pc, (base as i64 + e) as u64)
        })
        .into_iter()
        .map(|t| (t, None))
        .collect();
    }
    // A table of branches (bra.w, bra.s, or jmp to absolute addresses) to jump into.
    let first = code.u16be(table).unwrap_or(0);
    let stride = match first {
        0x6000 => 4,
        0x4EF9 => 6,
        o if o & 0xFF00 == 0x6000 && o & 0xFF != 0xFF => 2,
        _ => return Vec::new(),
    };
    read_table(table, stride, MAX, |i| {
        let at = table + stride * i as u64;
        let o = code.u16be(at)?;
        let same = match stride {
            4 => o == 0x6000,
            6 => o == 0x4EF9,
            _ => o & 0xFF00 == 0x6000 && o & 0xFF != 0 && o & 0xFF != 0xFF,
        };
        (same && (code.may_be_code)(at)).then_some(at)
    })
    .into_iter()
    .map(|t| (t, None))
    .collect()
}

/// A bound the code checks an index against just before: `cmp rn, #imm` (ARM).
fn arm_bound(code: &Code, pc: u64, reg: u64) -> Option<usize> {
    for back in [4, 8] {
        let w = code.u32le(pc.checked_sub(back)?)?;
        if w & 0x0FF0_F000 == 0x0350_0000 && (w >> 16) & 0xF == reg {
            let rot = ((w >> 8) & 0xF) * 2;
            let imm = (w & 0xFF) as u32;
            return Some(imm.rotate_right(rot as u32) as usize + 1);
        }
    }
    None
}

fn arm(pc: u64, cpu_pc: u64, code: &Code) -> Vec<(u64, Option<bool>)> {
    let Some(w) = code.u32le(pc) else { return Vec::new() };
    // ldr pc, [pc, rm, lsl #2], under any condition.
    if w & 0x0FFF_FFF0 != 0x079F_F100 {
        return Vec::new();
    }
    let rm = w & 0xF;
    let Some(table) = (code.resolve)(pc, cpu_pc + 8) else {
        return Vec::new();
    };
    let max = arm_bound(code, pc, rm).unwrap_or(MAX).min(MAX);
    read_table(table, 4, max, |i| {
        let t = code.u32le(table + 4 * i as u64)?;
        (t & 3 == 0).then_some(())?;
        code.code(pc, t)
    })
    .into_iter()
    .map(|t| (t, Some(false)))
    .collect()
}

fn thumb_table(pc: u64, cpu_pc: u64, code: &Code) -> Vec<(u64, Option<bool>)> {
    let Some(h) = code.u16le(pc) else { return Vec::new() };
    // mov pc, rx (stays Thumb) or bx rx (bit 0 says), low registers.
    let (reg, bx) = if h & 0xFFC7 == 0x4687 {
        ((h >> 3) & 7, false)
    } else if h & 0xFFC7 == 0x4700 && (h >> 3) & 0xF < 8 {
        ((h >> 3) & 7, true)
    } else {
        return Vec::new();
    };
    // Walk back: ldr rx, [ry, #0]; add ry, ra, rb; ldr ra (or rb), [pc, #n]; cmp rk, #bound.
    let before = |k: u64| pc.checked_sub(2 * k).and_then(|at| code.u16le(at).map(|v| (at, v)));
    let mut load_from = None;
    let mut sum: Option<(u64, u64)> = None;
    let mut base = None;
    let mut bound = None;
    for k in 1..=8 {
        let Some((at, v)) = before(k) else { break };
        if load_from.is_none() {
            if v & 0xFFC0 == 0x6800 && v & 7 == reg {
                load_from = Some((v >> 3) & 7);
            }
            continue;
        }
        let y = load_from.unwrap();
        if sum.is_none() {
            if v & 0xFE00 == 0x1800 && v & 7 == y {
                sum = Some(((v >> 3) & 7, (v >> 6) & 7));
            }
            continue;
        }
        let (a, b) = sum.unwrap();
        if base.is_none() && v & 0xF800 == 0x4800 && [a, b].contains(&((v >> 8) & 7)) {
            let cpu_at = cpu_pc - (pc - at);
            let literal = ((cpu_at + 4) & !3) + (v & 0xFF) * 4;
            base = (code.resolve)(pc, literal).and_then(|l| code.u32le(l));
            continue;
        }
        if base.is_some() && v & 0xF800 == 0x2800 {
            bound = Some((v & 0xFF) as usize + 1);
            break;
        }
    }
    let Some(table) = base.and_then(|t| (code.resolve)(pc, t)) else {
        return Vec::new();
    };
    let found = read_table(table, 4, bound.unwrap_or(MAX).min(MAX), |i| {
        let t = code.u32le(table + 4 * i as u64)?;
        code.code(pc, t & !1)
    });
    found
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            // mov pc stays in Thumb; bx goes by the entry's bit 0.
            let thumb = !bx || code.u32le(table + 4 * i as u64).is_some_and(|raw| raw & 1 == 1);
            (t, Some(thumb))
        })
        .collect()
}

/// MIPS: the `switch` idiom GCC and IDO write, read backwards from the `jr`:
/// ```text
/// sltiu $v0, $a0, N        (the guard, giving the table's length)
/// beqz  $v0, default
/// sll   $v0, $a0, 2        (the index scaled)
/// lui   $at, %hi(table)
/// addu  $at, $at, $v0      (or: addiu $v1, $v1, %lo(table); addu $v0, $v0, $v1)
/// lw    $v0, %lo(table)($at)
/// jr    $v0
/// ```
/// Each register is traced to the instruction that last wrote it; the table's
/// address is the constant side of the `addu` (built by `lui`, `addiu` or
/// `ori`) plus the load's offset, and the entries are 32-bit addresses.
fn mips(pc: u64, code: &Code, big: bool) -> Vec<(u64, Option<bool>)> {
    let word = |k: u64| -> Option<MipsWord> {
        let b = code.bytes(pc.checked_sub(4 * k)?, 4)?;
        let bytes = [b[0], b[1], b[2], b[3]];
        Some(MipsWord(if big {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        }))
    };
    const DEPTH: u64 = 32;
    // The nearest instruction at or before `pc - 4 * from` writing `reg`.
    let writer = |reg: u32, from: u64| -> Option<(u64, MipsWord)> {
        (from..DEPTH).find_map(|k| {
            let w = word(k)?;
            (w.writes() == Some(reg)).then_some((k, w))
        })
    };
    // A register's value when it was built from constants (lui, addiu, ori, move).
    fn constant(
        writer: &dyn Fn(u32, u64) -> Option<(u64, MipsWord)>,
        reg: u32,
        from: u64,
        depth: u32,
    ) -> Option<u32> {
        if reg == 0 {
            return Some(0);
        }
        let (k, w) = writer(reg, from)?;
        match (w.op(), w.funct()) {
            (15, _) => Some(w.imm() << 16),
            (9, _) => constant(writer, w.rs(), k + 1, depth.checked_sub(1)?).map(|v| v.wrapping_add(w.simm() as u32)),
            (13, _) => constant(writer, w.rs(), k + 1, depth.checked_sub(1)?).map(|v| v | w.imm()),
            (0, 33 | 37) if w.rt() == 0 => constant(writer, w.rs(), k + 1, depth.checked_sub(1)?),
            _ => None,
        }
    }
    // jr $rx (not $ra).
    let Some(jr) = word(0) else { return Vec::new() };
    if jr.0 & 0xFC1F_FFFF != 0x0000_0008 || jr.rs() == 31 {
        return Vec::new();
    }
    // lw $rx, off($base)
    let Some((k1, lw)) = writer(jr.rs(), 1) else { return Vec::new() };
    if lw.op() != 35 {
        return Vec::new();
    }
    // addu $base, $a, $b: one side the table's address, the other the index.
    let Some((k2, add)) = writer(lw.rs(), k1 + 1) else { return Vec::new() };
    if add.op() != 0 || !matches!(add.funct(), 32 | 33) {
        return Vec::new();
    }
    let (table, index) = match (
        constant(&writer, add.rs(), k2 + 1, 4),
        constant(&writer, add.rt(), k2 + 1, 4),
    ) {
        (Some(c), _) => (c, add.rt()),
        (None, Some(c)) => (c, add.rs()),
        _ => return Vec::new(),
    };
    let table = table.wrapping_add(lw.simm() as u32) as u64;
    // sll $index, $i, 2: the index scaled to the entries.
    let Some((k3, sll)) = writer(index, k2 + 1) else { return Vec::new() };
    if sll.op() != 0 || sll.funct() != 0 || sll.sa() != 2 {
        return Vec::new();
    }
    let i = sll.rt();
    // sltiu $t, $i, N (or sltu $t, $i, $n with $n a constant): the guard.
    let bound = (k3 + 1..DEPTH).find_map(|k| {
        let w = word(k)?;
        if w.op() == 11 && w.rs() == i {
            Some(w.imm() as usize)
        } else if w.op() == 0 && w.funct() == 43 && w.rs() == i {
            constant(&writer, w.rt(), k + 1, 4).map(|n| n as usize)
        } else {
            None
        }
    });
    let Some(table) = (code.resolve)(pc, table) else {
        return Vec::new();
    };
    let entry = |at: u64| -> Option<u64> {
        let b = code.bytes(at, 4)?;
        let bytes = [b[0], b[1], b[2], b[3]];
        Some(u64::from(if big {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        }))
    };
    read_table(table, 4, bound.unwrap_or(MAX).min(MAX), |n| {
        let t = entry(table + 4 * n as u64)?;
        (t & 3 == 0).then_some(())?;
        code.code(pc, t)
    })
    .into_iter()
    .map(|t| (t, None))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Code at our addresses 0x8000.. (the CPU sees the same), from `bytes`.
    fn run(cpu: Cpu, bytes: &[u8], pc: u64, thumb: bool) -> Vec<u64> {
        let base = 0x8000u64;
        let bytes_at = |a: u64| -> Option<&[u8]> {
            a.checked_sub(base)
                .and_then(|o| bytes.get(o as usize..))
                .filter(|b| !b.is_empty())
        };
        let resolve = |_: u64, t: u64| Some(t);
        let may = |_: u64| true;
        let code = Code {
            bytes_at: &bytes_at,
            resolve: &resolve,
            may_be_code: &may,
        };
        targets(cpu, pc, pc, thumb, &code).into_iter().map(|t| t.0).collect()
    }

    #[test]
    fn m65_tables() {
        // jmp ($8010,x); the table at $8010: $8020, $8030, then code at $8020.
        let mut b = vec![0xEAu8; 0x40];
        b[0..3].copy_from_slice(&[0x7C, 0x10, 0x80]);
        b[0x10..0x14].copy_from_slice(&[0x20, 0x80, 0x30, 0x80]);
        assert_eq!(run(Cpu::W65816, &b, 0x8000, false), [0x8020, 0x8030]);
        // The return trick: lda $8012,x / pha / lda $8010,x / pha / rts; lo at $8010, hi at $8012.
        let mut b = vec![0xEAu8; 0x40];
        b[0..9].copy_from_slice(&[0xBD, 0x12, 0x80, 0x48, 0xBD, 0x10, 0x80, 0x48, 0x60]);
        b[0x10..0x14].copy_from_slice(&[0x1F, 0x2F, 0x80, 0x80]);
        assert_eq!(run(Cpu::Mos6502, &b, 0x8008, false), [0x8020, 0x8030]);
    }

    #[test]
    fn m68k_tables() {
        // move.w $6(pc,d0.w),d0 / jmp $2(pc,d0.w) / table at $8008: offsets 4 and 6.
        let mut b = [0x4Eu8, 0x71].repeat(16);
        b[0..8].copy_from_slice(&[0x30, 0x3B, 0x00, 0x06, 0x4E, 0xFB, 0x00, 0x02]);
        b[8..12].copy_from_slice(&[0x00, 0x04, 0x00, 0x06]);
        assert_eq!(run(Cpu::M68000, &b, 0x8004, false), [0x800C, 0x800E]);
        // jmp $2(pc,d0.w) into two bra.w.
        let mut b = [0x4Eu8, 0x71].repeat(16);
        b[0..4].copy_from_slice(&[0x4E, 0xFB, 0x00, 0x02]);
        b[4..12].copy_from_slice(&[0x60, 0x00, 0x00, 0x10, 0x60, 0x00, 0x00, 0x20]);
        assert_eq!(run(Cpu::M68000, &b, 0x8000, false), [0x8004, 0x8008]);
    }

    #[test]
    fn arm_tables() {
        // cmp r0, #1 / ldrls pc, [pc, r0, lsl #2] / b default / .word $8010, $8014
        let mut b = vec![0u8; 0x40];
        b[0..4].copy_from_slice(&0xE350_0001u32.to_le_bytes());
        b[4..8].copy_from_slice(&0x979F_F100u32.to_le_bytes());
        b[8..12].copy_from_slice(&0xEA00_0002u32.to_le_bytes());
        b[12..16].copy_from_slice(&0x8020u32.to_le_bytes());
        b[16..20].copy_from_slice(&0x8024u32.to_le_bytes());
        b[20..24].copy_from_slice(&0x8028u32.to_le_bytes());
        // Two entries, as `cmp r0, #1` allows.
        assert_eq!(run(Cpu::Arm7Tdmi, &b, 0x8004, false), [0x8020, 0x8024]);
    }

    #[test]
    fn thumb_tables() {
        // cmp r0, #1 / bhi / lsl r0, r0, #2 / ldr r1, [pc, #4] / add r0, r0, r1 / ldr r0, [r0] / mov pc, r0
        let mut b = vec![0u8; 0x40];
        let code: [u16; 7] = [0x2801, 0xD805, 0x0080, 0x4902, 0x1840, 0x6800, 0x4687];
        for (i, h) in code.iter().enumerate() {
            b[2 * i..2 * i + 2].copy_from_slice(&h.to_le_bytes());
        }
        // The literal: ((0x8006 + 4) & !3) + 8 = 0x8010, holding the table's address $8020.
        b[0x10..0x14].copy_from_slice(&0x8020u32.to_le_bytes());
        b[0x20..0x28].copy_from_slice(&[0x31, 0x80, 0, 0, 0x35, 0x80, 0, 0]);
        assert_eq!(run(Cpu::Arm7Tdmi, &b, 0x800C, true), [0x8030, 0x8034]);
    }

    #[test]
    fn mips_tables() {
        // GCC (PlayStation, little-endian): sltiu $v0, $a0, 3 / beqz $v0 / sll $v0, $a0, 2 /
        // lui $at, 1 / addu $at, $at, $v0 / lw $v0, -0x7fd0($at) / nop / jr $v0 / nop;
        // the table at 0x8030 (= 0x10000 - 0x7fd0) holds four addresses, the guard allows three.
        let mut b = vec![0u8; 0x60];
        let code: [u32; 9] = [
            0x2C82_0003,
            0x1040_0005,
            0x0004_1080,
            0x3C01_0001,
            0x0022_0821,
            0x8C22_8030,
            0x0000_0000,
            0x0040_0008,
            0x0000_0000,
        ];
        for (i, w) in code.iter().enumerate() {
            b[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
        }
        for (i, t) in [0x8040u32, 0x8044, 0x8048, 0x804C].iter().enumerate() {
            b[0x30 + 4 * i..0x34 + 4 * i].copy_from_slice(&t.to_le_bytes());
        }
        assert_eq!(run(Cpu::MipsR3000, &b, 0x801C, false), [0x8040, 0x8044, 0x8048]);
        // Not from `jr $ra`.
        assert!(run(Cpu::MipsR3000, &b, 0x8018, false).is_empty());

        // IDO (Nintendo 64, big-endian), the table built with lui + addiu and no guard:
        // sll $t6, $a0, 2 / lui $v1, 1 / addiu $v1, $v1, -0x7fd0 / addu $t7, $t6, $v1 /
        // lw $t8, 0($t7) / jr $t8 / nop; the table ends where its entries stop being aligned code.
        let mut b = vec![0u8; 0x60];
        let code: [u32; 7] = [
            0x0004_7080,
            0x3C03_0001,
            0x2463_8030,
            0x01C3_7821,
            0x8DF8_0000,
            0x0300_0008,
            0x0000_0000,
        ];
        for (i, w) in code.iter().enumerate() {
            b[4 * i..4 * i + 4].copy_from_slice(&w.to_be_bytes());
        }
        for (i, t) in [0x8040u32, 0x8044, 0x1234_5679].iter().enumerate() {
            b[0x30 + 4 * i..0x34 + 4 * i].copy_from_slice(&t.to_be_bytes());
        }
        assert_eq!(run(Cpu::MipsR4300, &b, 0x8014, false), [0x8040, 0x8044]);
    }
}
