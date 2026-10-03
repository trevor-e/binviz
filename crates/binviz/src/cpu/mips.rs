//! MIPS: the Nintendo 64's R4300i (MIPS III, big-endian) and the
//! PlayStation's R3000A (MIPS I, little-endian, with the GTE as coprocessor
//! 2). Written as N64 and PlayStation decompilation projects do (`$a0`,
//! `lw $t0, 0x10($sp)`), with the common pseudo-instructions (`nop`, `move`,
//! `li`, `b`).
//!
//! Addresses are built in two instructions (`lui $at, 0x8034` then
//! `lw $t0, 0x1230($at)`): the registers `lui` fills are tracked through
//! each function, so that the load above reads `0x80341230`.

use super::{Flow, Insn, State};
use crate::xrefs::RefKind;

const REGS: [&str; 32] = [
    "$zero", "$at", "$v0", "$v1", "$a0", "$a1", "$a2", "$a3", "$t0", "$t1", "$t2", "$t3", "$t4", "$t5", "$t6", "$t7",
    "$s0", "$s1", "$s2", "$s3", "$s4", "$s5", "$s6", "$s7", "$t8", "$t9", "$k0", "$k1", "$gp", "$sp", "$fp", "$ra",
];

/// Registers a call may change (the others are the caller's to keep).
pub(crate) const CALL_CLOBBERS: u32 = 0b1000_0011_0000_0000_1111_1111_1111_1110;

/// The fields of a MIPS instruction word.
#[derive(Clone, Copy)]
pub(crate) struct MipsWord(pub u32);

impl MipsWord {
    pub(crate) fn op(self) -> u32 {
        self.0 >> 26
    }
    pub(crate) fn rs(self) -> u32 {
        (self.0 >> 21) & 31
    }
    pub(crate) fn rt(self) -> u32 {
        (self.0 >> 16) & 31
    }
    pub(crate) fn rd(self) -> u32 {
        (self.0 >> 11) & 31
    }
    pub(crate) fn sa(self) -> u32 {
        (self.0 >> 6) & 31
    }
    pub(crate) fn funct(self) -> u32 {
        self.0 & 63
    }
    pub(crate) fn imm(self) -> u32 {
        self.0 & 0xFFFF
    }
    pub(crate) fn simm(self) -> i64 {
        (self.0 & 0xFFFF) as u16 as i16 as i64
    }

    /// The general register this instruction writes, if any.
    pub(crate) fn writes(self) -> Option<u32> {
        let reg = match self.op() {
            0 => match self.funct() {
                8 | 12 | 13 | 15 | 17 | 19 | 24..=31 | 48..=54 => return None,
                _ => self.rd(),
            },
            1 => return (self.rt() >= 16).then_some(31),
            3 => 31,
            8..=15 | 24 | 25 => self.rt(),
            16..=18 if self.rs() <= 2 => self.rt(),
            26 | 27 | 32..=39 | 48 | 55 => self.rt(),
            _ => return None,
        };
        (reg != 0).then_some(reg)
    }
}

/// PS1 GPR effects, without any assumed ABI call clobbers. Unknown MIPS III
/// encodings intentionally have no result. Merge loads can forward a pending
/// load value rather than reading the old architectural destination register.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ps1GprEffects {
    pub reads: u32,
    pub merge_reads: u32,
    pub write: Option<u8>,
    pub delayed_write: bool,
}

pub(crate) fn ps1_gpr_effects(word: u32) -> Option<Ps1GprEffects> {
    let w = MipsWord(word);
    let (rs, rt, rd) = (w.rs(), w.rt(), w.rd());
    let bit = |r: u32| if r == 0 { 0 } else { 1u32 << r };
    let (reads, merge_reads, write, delayed_write) = match w.op() {
        0 => match w.funct() {
            0 | 2 | 3 => (bit(rt), 0, Some(rd), false),
            4 | 6 | 7 | 32..=39 | 42 | 43 => (bit(rs) | bit(rt), 0, Some(rd), false),
            8 | 17 | 19 => (bit(rs), 0, None, false),
            9 => (bit(rs), 0, Some(rd), false),
            12 | 13 => (0, 0, None, false),
            16 | 18 => (0, 0, Some(rd), false),
            24..=27 => (bit(rs) | bit(rt), 0, None, false),
            _ => return None,
        },
        1 if rt <= 1 => (bit(rs), 0, None, false),
        2 => (0, 0, None, false),
        3 => (0, 0, Some(31), false),
        4 | 5 => (bit(rs) | bit(rt), 0, None, false),
        6 | 7 => (bit(rs), 0, None, false),
        8..=14 => (bit(rs), 0, Some(rt), false),
        15 => (0, 0, Some(rt), false),
        16 if rs == 0 => (0, 0, Some(rt), true),
        16 if rs == 4 => (bit(rt), 0, None, false),
        18 if word & (1 << 25) != 0 => (0, 0, None, false),
        18 if rs == 0 || rs == 2 => (0, 0, Some(rt), true),
        18 if rs == 4 || rs == 6 => (bit(rt), 0, None, false),
        32 | 33 | 35 | 36 | 37 => (bit(rs), 0, Some(rt), true),
        34 | 38 => (bit(rs), bit(rt), Some(rt), true),
        40 | 41 | 42 | 43 | 46 => (bit(rs) | bit(rt), 0, None, false),
        50 | 58 => (bit(rs), 0, None, false),
        _ => return None,
    };
    Some(Ps1GprEffects {
        reads,
        merge_reads,
        write: write.filter(|&r| r != 0).map(|r| r as u8),
        delayed_write,
    })
}

fn r(i: u32) -> &'static str {
    REGS[i as usize & 31]
}

fn signed_hex(v: i64) -> String {
    if v < 0 {
        format!("-{:#x}", -v)
    } else {
        format!("{v:#x}")
    }
}

impl State {
    fn reg(&self, i: u32) -> Option<u32> {
        if i == 0 {
            return Some(0);
        }
        (self.known & (1 << i) != 0).then_some(self.regs[i as usize])
    }

    fn set(&mut self, i: u32, v: Option<u32>) {
        if i == 0 {
            return;
        }
        match v {
            Some(v) => {
                self.regs[i as usize] = v;
                self.known |= 1 << i;
            }
            None => self.known &= !(1 << i),
        }
    }
}

/// Decodes the instruction at the start of `bytes` (big-endian for the
/// Nintendo 64, little-endian for the PlayStation), updating the registers
/// known to hold addresses.
pub(crate) fn decode(bytes: &[u8], pc: u64, state: &mut State, big: bool) -> Insn {
    // The call before last: its delay slot has run, the callee clobbers now.
    if state.clobber_in > 0 {
        state.clobber_in -= 1;
        if state.clobber_in == 0 {
            state.known &= !CALL_CLOBBERS;
        }
    }
    let Some(word) = bytes.get(..4) else {
        return Insn::bad(bytes.len() as u32, bytes);
    };
    let w = if big {
        u32::from_be_bytes([word[0], word[1], word[2], word[3]])
    } else {
        u32::from_le_bytes([word[0], word[1], word[2], word[3]])
    };
    let (op, rs, rt, rd, sa, funct) = (
        w >> 26,
        (w >> 21) & 31,
        (w >> 16) & 31,
        (w >> 11) & 31,
        (w >> 6) & 31,
        w & 63,
    );
    let imm = w & 0xFFFF;
    let simm = imm as i16 as i64;
    let next = pc.wrapping_add(4) & 0xFFFF_FFFF;
    let branch = next.wrapping_add((simm << 2) as u64) & 0xFFFF_FFFF;
    let jump = (next & 0xF000_0000) | ((w as u64 & 0x03FF_FFFF) << 2);
    let insn = |m: &str, ops: String| Insn::new(4, m, ops, Flow::Next);
    let delayed = |mut i: Insn| {
        i.delay_slot = true;
        i
    };
    // The register this instruction writes, whose value (unless set below) is no longer known.
    let mut writes: Option<u32> = None;
    let mut value: Option<u32> = None;
    let mut out = match op {
        0 => {
            writes = Some(rd);
            match funct {
                0 if w == 0 => {
                    writes = None;
                    insn("nop", String::new())
                }
                0 | 2 | 3 | 56 | 58 | 59 | 60 | 62 | 63 => {
                    let m = match funct {
                        0 => "sll",
                        2 => "srl",
                        3 => "sra",
                        56 => "dsll",
                        58 => "dsrl",
                        59 => "dsra",
                        60 => "dsll32",
                        62 => "dsrl32",
                        _ => "dsra32",
                    };
                    insn(m, format!("{}, {}, {sa}", r(rd), r(rt)))
                }
                4 | 6 | 7 | 20 | 22 | 23 => {
                    let m = [
                        "sllv", "", "srlv", "srav", "", "", "", "", "", "", "", "", "", "", "", "", "dsllv", "",
                        "dsrlv", "dsrav",
                    ][funct as usize - 4];
                    insn(m, format!("{}, {}, {}", r(rd), r(rt), r(rs)))
                }
                10 | 11 => insn(
                    if funct == 10 { "movz" } else { "movn" },
                    format!("{}, {}, {}", r(rd), r(rs), r(rt)),
                ),
                8 => {
                    writes = None;
                    if rs == 31 {
                        delayed(Insn::new(4, "jr", r(rs).into(), Flow::Return))
                    } else {
                        delayed(Insn::new(4, "jr", r(rs).into(), Flow::Jump(None)))
                    }
                }
                9 => {
                    let ops = if rd == 31 {
                        r(rs).to_string()
                    } else {
                        format!("{}, {}", r(rd), r(rs))
                    };
                    delayed(Insn::new(4, "jalr", ops, Flow::Call(None)))
                }
                12 => {
                    writes = None;
                    let code = (w >> 6) & 0xF_FFFF;
                    let ops = if code == 0 { String::new() } else { format!("{code:#x}") };
                    Insn::new(4, "syscall", ops, Flow::Trap)
                }
                13 => {
                    writes = None;
                    // The code, as GNU as reads it back: `break 7` (GCC's division by zero), `break 7, 1`.
                    let (code, low) = ((w >> 16) & 0x3FF, (w >> 6) & 0x3FF);
                    let ops = match (code, low) {
                        (0, 0) => String::new(),
                        (c, 0) => format!("{c:#x}"),
                        (c, l) => format!("{c:#x}, {l:#x}"),
                    };
                    Insn::new(4, "break", ops, Flow::Trap)
                }
                15 => {
                    writes = None;
                    insn("sync", String::new())
                }
                16 | 18 => insn(if funct == 16 { "mfhi" } else { "mflo" }, r(rd).into()),
                17 | 19 => {
                    writes = None;
                    insn(if funct == 17 { "mthi" } else { "mtlo" }, r(rs).into())
                }
                24..=31 => {
                    writes = None;
                    let m = ["mult", "multu", "div", "divu", "dmult", "dmultu", "ddiv", "ddivu"][funct as usize - 24];
                    insn(m, format!("{}, {}", r(rs), r(rt)))
                }
                32..=39 | 42..=47 => {
                    let m = match funct {
                        32 => "add",
                        33 => "addu",
                        34 => "sub",
                        35 => "subu",
                        36 => "and",
                        37 => "or",
                        38 => "xor",
                        39 => "nor",
                        42 => "slt",
                        43 => "sltu",
                        44 => "dadd",
                        45 => "daddu",
                        46 => "dsub",
                        _ => "dsubu",
                    };
                    // move rd, rs: addu/or/daddu with $zero.
                    if matches!(funct, 33 | 37 | 45) && rt == 0 {
                        value = state.reg(rs);
                        insn("move", format!("{}, {}", r(rd), r(rs)))
                    } else {
                        insn(m, format!("{}, {}, {}", r(rd), r(rs), r(rt)))
                    }
                }
                48..=54 => {
                    writes = None;
                    let m = ["tge", "tgeu", "tlt", "tltu", "teq", "", "tne"][funct as usize - 48];
                    let code = (w >> 6) & 0x3FF;
                    if m.is_empty() {
                        Insn::bad(4, word)
                    } else if code != 0 {
                        insn(m, format!("{}, {}, {code:#x}", r(rs), r(rt)))
                    } else {
                        insn(m, format!("{}, {}", r(rs), r(rt)))
                    }
                }
                _ => Insn::bad(4, word),
            }
        }
        1 => {
            let m = match rt {
                0 => "bltz",
                1 => "bgez",
                2 => "bltzl",
                3 => "bgezl",
                16 => "bltzal",
                17 => "bgezal",
                18 => "bltzall",
                19 => "bgezall",
                _ => "",
            };
            if m.is_empty() {
                Insn::bad(4, word)
            } else if rt == 17 && rs == 0 {
                delayed(Insn::new(4, "bal", format!("{branch:#x}"), Flow::Call(Some(branch))))
            } else if rt == 1 && rs == 0 {
                delayed(Insn::new(4, "b", format!("{branch:#x}"), Flow::Jump(Some(branch))))
            } else if rt >= 16 {
                delayed(Insn::new(
                    4,
                    m,
                    format!("{}, {branch:#x}", r(rs)),
                    Flow::Call(Some(branch)),
                ))
            } else {
                delayed(Insn::new(4, m, format!("{}, {branch:#x}", r(rs)), Flow::Branch(branch)))
            }
        }
        2 => delayed(Insn::new(4, "j", format!("{jump:#x}"), Flow::Jump(Some(jump)))),
        3 => delayed(Insn::new(4, "jal", format!("{jump:#x}"), Flow::Call(Some(jump)))),
        4 | 5 | 20 | 21 => {
            let m = ["beq", "bne"][(op & 1) as usize];
            let likely = if op >= 20 { "l" } else { "" };
            if op == 4 && rs == 0 && rt == 0 {
                delayed(Insn::new(4, "b", format!("{branch:#x}"), Flow::Jump(Some(branch))))
            } else if rt == 0 {
                delayed(Insn::new(
                    4,
                    &format!("{m}z{likely}"),
                    format!("{}, {branch:#x}", r(rs)),
                    Flow::Branch(branch),
                ))
            } else {
                delayed(Insn::new(
                    4,
                    &format!("{m}{likely}"),
                    format!("{}, {}, {branch:#x}", r(rs), r(rt)),
                    Flow::Branch(branch),
                ))
            }
        }
        6 | 7 | 22 | 23 => {
            let m = ["blez", "bgtz", "blezl", "bgtzl"][(if op >= 22 { op - 20 } else { op - 6 }) as usize];
            delayed(Insn::new(4, m, format!("{}, {branch:#x}", r(rs)), Flow::Branch(branch)))
        }
        8..=15 | 24 | 25 => {
            writes = Some(rt);
            match op {
                15 => {
                    value = Some(imm << 16);
                    insn("lui", format!("{}, {imm:#x}", r(rt)))
                }
                9 | 25 if rs == 0 => {
                    value = Some(simm as u32);
                    insn("li", format!("{}, {}", r(rt), signed_hex(simm)))
                }
                13 if rs == 0 => {
                    value = Some(imm);
                    insn("li", format!("{}, {imm:#x}", r(rt)))
                }
                12..=14 => {
                    let m = ["andi", "ori", "xori"][op as usize - 12];
                    if op == 13 {
                        value = state.reg(rs).map(|v| v | imm);
                    }
                    insn(m, format!("{}, {}, {imm:#x}", r(rt), r(rs)))
                }
                _ => {
                    let m = match op {
                        8 => "addi",
                        9 => "addiu",
                        10 => "slti",
                        11 => "sltiu",
                        24 => "daddi",
                        _ => "daddiu",
                    };
                    if matches!(op, 8 | 9 | 24 | 25) {
                        value = state.reg(rs).map(|v| v.wrapping_add(simm as u32));
                    }
                    insn(m, format!("{}, {}, {}", r(rt), r(rs), signed_hex(simm)))
                }
            }
        }
        16..=18 => {
            let cop = op - 16;
            match rs {
                0..=2 => {
                    writes = Some(rt);
                    let m = ["mfc", "dmfc", "cfc"][rs as usize];
                    insn(&format!("{m}{cop}"), format!("{}, ${rd}", r(rt)))
                }
                4..=6 => {
                    let m = ["mtc", "dmtc", "ctc"][rs as usize - 4];
                    insn(&format!("{m}{cop}"), format!("{}, ${rd}", r(rt)))
                }
                8 if cop == 1 => {
                    let m = ["bc1f", "bc1t", "bc1fl", "bc1tl"][(rt & 3) as usize];
                    delayed(Insn::new(4, m, format!("{branch:#x}"), Flow::Branch(branch)))
                }
                16.. if cop == 0 => match funct {
                    1 => insn("tlbr", String::new()),
                    2 => insn("tlbwi", String::new()),
                    6 => insn("tlbwr", String::new()),
                    8 => insn("tlbp", String::new()),
                    16 => Insn::new(4, "rfe", String::new(), Flow::Next),
                    24 => Insn::new(4, "eret", String::new(), Flow::Return),
                    _ => Insn::bad(4, word),
                },
                16.. if cop == 1 => {
                    let fmt = match rs {
                        16 => "s",
                        17 => "d",
                        20 => "w",
                        21 => "l",
                        _ => "?",
                    };
                    let (fd, fs, ft) = (sa, rd, rt);
                    let two = |m: &str| insn(&format!("{m}.{fmt}"), format!("$f{fd}, $f{fs}"));
                    match funct {
                        0..=3 => {
                            let m = ["add", "sub", "mul", "div"][funct as usize];
                            insn(&format!("{m}.{fmt}"), format!("$f{fd}, $f{fs}, $f{ft}"))
                        }
                        4 => two("sqrt"),
                        5 => two("abs"),
                        6 => two("mov"),
                        7 => two("neg"),
                        8..=15 => two([
                            "round.l", "trunc.l", "ceil.l", "floor.l", "round.w", "trunc.w", "ceil.w", "floor.w",
                        ][funct as usize - 8]),
                        32 => two("cvt.s"),
                        33 => two("cvt.d"),
                        36 => two("cvt.w"),
                        37 => two("cvt.l"),
                        48..=63 => {
                            let cond = [
                                "f", "un", "eq", "ueq", "olt", "ult", "ole", "ule", "sf", "ngle", "seq", "ngl", "lt",
                                "nge", "le", "ngt",
                            ][funct as usize - 48];
                            insn(&format!("c.{cond}.{fmt}"), format!("$f{fs}, $f{ft}"))
                        }
                        _ => Insn::bad(4, word),
                    }
                }
                // The PlayStation's GTE: a command in the low bits.
                16.. => insn("cop2", format!("{:#x}", w & 0x01FF_FFFF)),
                _ => Insn::bad(4, word),
            }
        }
        // SPECIAL2 (MIPS32; the R3000 lacks these, but compilers emit `mul` for it anyway).
        28 => {
            let m = match funct {
                0 => "madd",
                1 => "maddu",
                2 => "mul",
                4 => "msub",
                5 => "msubu",
                32 => "clz",
                33 => "clo",
                _ => "",
            };
            if m.is_empty() {
                Insn::bad(4, word)
            } else if funct == 2 || funct >= 32 {
                writes = Some(rd);
                if funct == 2 {
                    insn(m, format!("{}, {}, {}", r(rd), r(rs), r(rt)))
                } else {
                    insn(m, format!("{}, {}", r(rd), r(rs)))
                }
            } else {
                insn(m, format!("{}, {}", r(rs), r(rt)))
            }
        }
        26..=63 => {
            let (m, store, gpr) = match op {
                26 => ("ldl", false, true),
                27 => ("ldr", false, true),
                32 => ("lb", false, true),
                33 => ("lh", false, true),
                34 => ("lwl", false, true),
                35 => ("lw", false, true),
                36 => ("lbu", false, true),
                37 => ("lhu", false, true),
                38 => ("lwr", false, true),
                39 => ("lwu", false, true),
                40 => ("sb", true, true),
                41 => ("sh", true, true),
                42 => ("swl", true, true),
                43 => ("sw", true, true),
                44 => ("sdl", true, true),
                45 => ("sdr", true, true),
                46 => ("swr", true, true),
                47 => ("cache", false, false),
                48 => ("ll", false, true),
                49 => ("lwc1", false, false),
                50 => ("lwc2", false, false),
                53 => ("ldc1", false, false),
                55 => ("ld", false, true),
                56 => ("sc", true, true),
                57 => ("swc1", true, false),
                58 => ("swc2", true, false),
                61 => ("sdc1", true, false),
                63 => ("sd", true, true),
                _ => ("", false, false),
            };
            if m.is_empty() {
                Insn::bad(4, word)
            } else {
                let target = match op {
                    47 => format!("{rt:#x}"),
                    49 | 53 | 57 | 61 => format!("$f{rt}"),
                    50 | 58 => format!("${rt}"),
                    _ => r(rt).to_string(),
                };
                if gpr && !store {
                    writes = Some(rt);
                }
                let mut i = insn(m, format!("{target}, {}({})", signed_hex(simm), r(rs)));
                if op != 47
                    && let Some(base) = state.reg(rs)
                {
                    let a = base.wrapping_add(simm as u32) as u64;
                    i.data = Some((a, if store { RefKind::Write } else { RefKind::Read }));
                }
                i
            }
        }
        _ => Insn::bad(4, word),
    };
    // An address in a register (lui + addiu, lui + ori) is a reference too; a
    // plain `li` (from $zero) of a small number is a number (0xa0 and up may
    // be the PlayStation kernel's entry points), and so is one worked out
    // from small numbers (a loop's counter, `li 10` then `addiu -1`): an
    // address has its high half, as a `lui` leaves it.
    let li = matches!(op, 9 | 13 | 25) && rs == 0;
    if let (Some(v), Some(reg)) = (value, writes)
        && out.data.is_none()
        && op != 15
        && if li { v >= 0x80 } else { v >= 0x1_0000 }
        && reg != 0
    {
        out.data = Some((v as u64, RefKind::Address));
    }
    if let Some(reg) = writes {
        state.set(reg, value);
    }
    if matches!(out.flow, Flow::Call(_)) {
        state.clobber_in = 2;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(i: &Insn) -> String {
        format!("{} {}", i.mnemonic, i.operands).trim_end().to_string()
    }

    fn be(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_be_bytes()).collect()
    }

    #[test]
    fn n64_code() {
        // lui $at, 0x8034 / lw $t0, 0x1230($at) / addiu $sp, $sp, -0x18 / jal 0x80001000 / nop / jr $ra
        let code = be(&[
            0x3C01_8034,
            0x8C28_1230,
            0x27BD_FFE8,
            0x0C00_0400,
            0x0000_0000,
            0x03E0_0008,
        ]);
        let mut s = State::default();
        let mut pc = 0x8000_0400u64;
        let mut out = Vec::new();
        for w in code.chunks(4) {
            let i = decode(w, pc, &mut s, true);
            out.push(i);
            pc += 4;
        }
        assert_eq!(text(&out[0]), "lui $at, 0x8034");
        assert_eq!(text(&out[1]), "lw $t0, 0x1230($at)");
        assert_eq!(out[1].data, Some((0x8034_1230, RefKind::Read)));
        assert_eq!(text(&out[2]), "addiu $sp, $sp, -0x18");
        assert_eq!(out[3].flow, Flow::Call(Some(0x8000_1000)));
        assert!(out[3].delay_slot);
        assert_eq!(text(&out[4]), "nop");
        assert_eq!(out[5].flow, Flow::Return);
        // lui + addiu: an address.
        let mut s = State::default();
        decode(&be(&[0x3C04_8010]), 0, &mut s, true);
        let i = decode(&be(&[0x2484_FFF0]), 4, &mut s, true);
        assert_eq!(
            (text(&i).as_str(), i.data),
            ("addiu $a0, $a0, -0x10", Some((0x800F_FFF0, RefKind::Address)))
        );
    }

    #[test]
    fn codes_and_counters() {
        let one = |w: u32, s: &mut State| decode(&be(&[w]), 0x8000_0400, s, true);
        let mut s = State::default();
        // The codes, as an assembler writes them back: GCC's division by zero, a trap's.
        assert_eq!(text(&one(0x0007_000D, &mut s)), "break 0x7");
        assert_eq!(text(&one(0x0007_004D, &mut s)), "break 0x7, 0x1");
        assert_eq!(text(&one(0x0000_000D, &mut s)), "break");
        assert_eq!(text(&one(0x0000_000C, &mut s)), "syscall");
        assert_eq!(text(&one(0x0020_01F4, &mut s)), "teq $at, $zero, 0x7");
        // A number worked out from small ones (a loop's counter) is no address.
        one(0x2406_000A, &mut s);
        let i = one(0x24C6_FFFF, &mut s);
        assert_eq!((text(&i).as_str(), i.data), ("addiu $a2, $a2, -0x1", None));
    }

    #[test]
    fn playstation_code() {
        // li $t1, 0x3f / j 0xa0 (BIOS A-functions) / sw $v0, 0x10($sp), little-endian
        let le = |w: u32| w.to_le_bytes();
        let mut s = State::default();
        assert_eq!(
            text(&decode(&le(0x2409_003F), 0x8001_0000, &mut s, false)),
            "li $t1, 0x3f"
        );
        let i = decode(&le(0x0800_0028), 0x8001_0004, &mut s, false);
        assert_eq!(
            (text(&i).as_str(), i.flow),
            ("j 0x800000a0", Flow::Jump(Some(0x8000_00A0)))
        );
        assert_eq!(text(&decode(&le(0xAFA2_0010), 0, &mut s, false)), "sw $v0, 0x10($sp)");
        assert_eq!(text(&decode(&le(0x4A18_0001), 0, &mut s, false)), "cop2 0x180001");
        assert_eq!(text(&decode(&le(0x70E6_0802), 0, &mut s, false)), "mul $at, $a3, $a2");
        assert_eq!(text(&decode(&le(0x0061_200B), 0, &mut s, false)), "movn $a0, $v1, $at");
        // lui $at / jal / addiu $a0, $at, 0x9c8 (the delay slot still sees $at) / addiu $a0, $at, 0 (no longer).
        let mut s = State::default();
        decode(&le(0x3C01_8001), 0, &mut s, false);
        decode(&le(0x0C00_40AB), 4, &mut s, false);
        let slot = decode(&le(0x2424_09C8), 8, &mut s, false);
        assert_eq!(slot.data, Some((0x8001_09C8, RefKind::Address)));
        let after = decode(&le(0x2424_0000), 12, &mut s, false);
        assert_eq!(after.data, None);
        assert_eq!(
            decode(&le(0x1000_FFFF), 0x100, &mut s, false).flow,
            Flow::Jump(Some(0x100))
        );
    }
}
