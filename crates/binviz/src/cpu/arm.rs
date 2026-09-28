//! The Game Boy Advance's ARM7TDMI: ARMv4T, in ARM mode (32-bit
//! instructions) and Thumb mode (16-bit), switched by `bx` to an address
//! with bit 0 set or clear. Targets carry the mode they run in the same
//! way: bit 0 set for Thumb.
//!
//! Constants come from literal pools (`ldr r0, [pc, #0x40]`), read here
//! when the pool is in the bytes given; registers holding them are
//! tracked, so that `str r1, [r0]` names the register `r0` points at, and
//! `bx r0` goes where `r0` says.

use super::{Flow, Insn, State};
use crate::xrefs::RefKind;

const COND: [&str; 16] = [
    "eq", "ne", "cs", "cc", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le", "", "nv",
];
const DP: [&str; 16] = [
    "and", "eor", "sub", "rsb", "add", "adc", "sbc", "rsc", "tst", "teq", "cmp", "cmn", "orr", "mov", "bic", "mvn",
];
const SHIFT: [&str; 4] = ["lsl", "lsr", "asr", "ror"];

fn reg(r: u32) -> &'static str {
    [
        "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "sp", "lr", "pc",
    ][r as usize & 15]
}

fn reglist(bits: u32) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < 16 {
        if bits & (1 << i) != 0 {
            let start = i;
            while i + 1 < 16 && bits & (1 << (i + 1)) != 0 && i + 1 < 13 {
                i += 1;
            }
            if i > start + 1 {
                out.push(format!("{}-{}", reg(start), reg(i)));
            } else {
                out.push(reg(start).to_string());
                if i > start {
                    out.push(reg(i).to_string());
                }
            }
        }
        i += 1;
    }
    format!("{{{}}}", out.join(", "))
}

fn imm(v: u32) -> String {
    if v < 10 { format!("#{v}") } else { format!("#{v:#x}") }
}

/// Registers a call may change (r0-r3, r12, lr).
const CALL_CLOBBERS: u32 = 0b0101_0000_0000_1111;

impl State {
    fn arm_reg(&self, r: u32) -> Option<u32> {
        (self.known & (1 << r) != 0).then_some(self.regs[r as usize])
    }

    fn arm_set(&mut self, r: u32, v: Option<u32>) {
        match v {
            Some(v) => {
                self.regs[r as usize] = v;
                self.known |= 1 << r;
            }
            None => self.known &= !(1 << r),
        }
    }
}

/// Decodes one instruction, in Thumb mode when `state.thumb`.
pub(crate) fn decode(bytes: &[u8], pc: u64, state: &mut State) -> Insn {
    if state.thumb {
        thumb(bytes, pc, state)
    } else {
        arm(bytes, pc, state)
    }
}

/// The word at `offset` of `bytes`, if there.
fn literal(bytes: &[u8], offset: i64) -> Option<u32> {
    let at = usize::try_from(offset).ok()?;
    let w = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
}

fn arm(bytes: &[u8], pc: u64, state: &mut State) -> Insn {
    let Some(word) = bytes.get(..4) else {
        return Insn::bad(bytes.len() as u32, bytes);
    };
    let w = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
    let cond = w >> 28;
    let c = COND[cond as usize];
    let always = cond == 14;
    let pc8 = (pc as u32).wrapping_add(8);
    let rn = (w >> 16) & 15;
    let rd = (w >> 12) & 15;
    let rm = w & 15;
    let link = state.link;
    state.link = false;
    let plain = |m: String, ops: String| Insn::new(4, &m, ops, Flow::Next);
    // Branch and branch with link.
    if w & 0x0E00_0000 == 0x0A00_0000 {
        let off = ((w & 0x00FF_FFFF) << 8) as i32 >> 6;
        let t = pc8.wrapping_add(off as u32) as u64;
        let l = w & 0x0100_0000 != 0;
        let m = format!("{}{c}", if l { "bl" } else { "b" });
        let flow = match (l, always) {
            (true, _) => Flow::Call(Some(t)),
            (false, true) => Flow::Jump(Some(t)),
            (false, false) => Flow::Branch(t),
        };
        if l {
            state.known &= !CALL_CLOBBERS;
        }
        return Insn::new(4, &m, format!("{t:#x}"), flow);
    }
    // bx: to Thumb when the address is odd.
    if w & 0x0FFF_FFF0 == 0x012F_FF10 {
        let flow = if rm == 14 {
            if always { Flow::Return } else { Flow::CondReturn }
        } else {
            let t = state.arm_reg(rm).map(u64::from);
            if link { Flow::Call(t) } else { Flow::Jump(t) }
        };
        if matches!(flow, Flow::Call(_)) {
            state.known &= !CALL_CLOBBERS;
        }
        return Insn::new(4, &format!("bx{c}"), reg(rm).into(), flow);
    }
    // swi: a BIOS call.
    if w & 0x0F00_0000 == 0x0F00_0000 {
        return Insn::new(4, &format!("swi{c}"), format!("{:#x}", (w >> 16) & 0xFF), Flow::Next);
    }
    // Multiplies, swaps and halfword transfers share the data processing space.
    if w & 0x0E00_0090 == 0x0000_0090 {
        let sh = (w >> 5) & 3;
        if sh == 0 {
            if w & 0x0F80_0000 == 0x0080_0000 {
                let m = ["umull", "umlal", "smull", "smlal"][((w >> 21) & 3) as usize];
                let s = if w & (1 << 20) != 0 { "s" } else { "" };
                state.arm_set(rd, None);
                state.arm_set(rn, None);
                return plain(
                    format!("{m}{c}{s}"),
                    format!("{}, {}, {}, {}", reg(rd), reg(rn), reg(rm), reg((w >> 8) & 15)),
                );
            }
            if w & 0x0FC0_0000 == 0x0000_0000 {
                let acc = w & (1 << 21) != 0;
                let s = if w & (1 << 20) != 0 { "s" } else { "" };
                state.arm_set(rn, None);
                return if acc {
                    plain(
                        format!("mla{c}{s}"),
                        format!("{}, {}, {}, {}", reg(rn), reg(rm), reg((w >> 8) & 15), reg(rd)),
                    )
                } else {
                    plain(
                        format!("mul{c}{s}"),
                        format!("{}, {}, {}", reg(rn), reg(rm), reg((w >> 8) & 15)),
                    )
                };
            }
            if w & 0x0FB0_0FF0 == 0x0100_0090 {
                let b = if w & (1 << 22) != 0 { "b" } else { "" };
                state.arm_set(rd, None);
                return plain(format!("swp{c}{b}"), format!("{}, {}, [{}]", reg(rd), reg(rm), reg(rn)));
            }
            return Insn::bad(4, word);
        }
        // ldrh/strh/ldrsb/ldrsh
        let load = w & (1 << 20) != 0;
        let m = match (load, sh) {
            (false, 1) => "strh",
            (true, 1) => "ldrh",
            (true, 2) => "ldrsb",
            (true, 3) => "ldrsh",
            _ => return Insn::bad(4, word),
        };
        let (p, u, wb) = (w & (1 << 24) != 0, w & (1 << 23) != 0, w & (1 << 21) != 0);
        let immediate = w & (1 << 22) != 0;
        let off = (((w >> 4) & 0xF0) | (w & 15)) as i64;
        let off = if u { off } else { -off };
        let text = if immediate {
            address_text(rn, off, p, wb)
        } else {
            let sign = if u { "" } else { "-" };
            if p {
                format!("[{}, {sign}{}]{}", reg(rn), reg(rm), if wb { "!" } else { "" })
            } else {
                format!("[{}], {sign}{}", reg(rn), reg(rm))
            }
        };
        let mut i = plain(format!("{m}{c}"), format!("{}, {text}", reg(rd)));
        if immediate && let Some(base) = base_value(state, rn, pc8) {
            let a = if p { base.wrapping_add(off as u32) } else { base };
            i.data = Some((a as u64, if load { RefKind::Read } else { RefKind::Write }));
        }
        if load {
            state.arm_set(rd, None);
        }
        if wb || !p {
            state.arm_set(rn, None);
        }
        return i;
    }
    // mrs / msr
    if w & 0x0FBF_0FFF == 0x010F_0000 {
        state.arm_set(rd, None);
        let psr = if w & (1 << 22) != 0 { "spsr" } else { "cpsr" };
        return plain(format!("mrs{c}"), format!("{}, {psr}", reg(rd)));
    }
    if w & 0x0DB0_F000 == 0x0120_F000 {
        let psr = if w & (1 << 22) != 0 { "spsr" } else { "cpsr" };
        let fields: String = ["c", "x", "s", "f"]
            .iter()
            .enumerate()
            .filter(|(i, _)| w & (1 << (16 + i)) != 0)
            .map(|(_, f)| *f)
            .collect();
        let src = if w & (1 << 25) != 0 {
            imm(rotated(w))
        } else {
            reg(rm).to_string()
        };
        return plain(format!("msr{c}"), format!("{psr}_{fields}, {src}"));
    }
    // Data processing.
    if w & 0x0C00_0000 == 0 {
        let op = (w >> 21) & 15;
        let s = w & (1 << 20) != 0;
        let m = DP[op as usize];
        let (op2, value) = if w & (1 << 25) != 0 {
            let v = rotated(w);
            (imm(v), Some(v))
        } else {
            let shift = (w >> 5) & 3;
            let text = if w & (1 << 4) != 0 {
                format!("{}, {} {}", reg(rm), SHIFT[shift as usize], reg((w >> 8) & 15))
            } else {
                let amount = (w >> 7) & 31;
                match (shift, amount) {
                    (0, 0) => reg(rm).to_string(),
                    (3, 0) => format!("{}, rrx", reg(rm)),
                    (1 | 2, 0) => format!("{}, {} #32", reg(rm), SHIFT[shift as usize]),
                    _ => format!("{}, {} #{amount}", reg(rm), SHIFT[shift as usize]),
                }
            };
            let v = if w & 0xFF0 == 0 {
                if rm == 15 { Some(pc8) } else { state.arm_reg(rm) }
            } else {
                None
            };
            (text, v)
        };
        let sflag = if s && !(8..=11).contains(&op) { "s" } else { "" };
        let mut i = match op {
            8..=11 => plain(format!("{m}{c}"), format!("{}, {op2}", reg(rn))),
            13 | 15 => plain(format!("{m}{c}{sflag}"), format!("{}, {op2}", reg(rd))),
            _ => plain(format!("{m}{c}{sflag}"), format!("{}, {}, {op2}", reg(rd), reg(rn))),
        };
        // Results we can follow: moves, and adds to the pc (adr) or to a known register.
        let base = if rn == 15 { Some(pc8) } else { state.arm_reg(rn) };
        let result = match op {
            13 => value,
            15 => value.map(|v| !v),
            4 => base.zip(value).map(|(a, b)| a.wrapping_add(b)),
            2 => base.zip(value).map(|(a, b)| a.wrapping_sub(b)),
            12 => base.zip(value).map(|(a, b)| a | b),
            _ => None,
        };
        if rd == 15 && !(8..=11).contains(&op) {
            // Writing the pc: a return (mov pc, lr) or a jump.
            i.flow = if op == 13 && w & (1 << 25) == 0 && rm == 14 && w & 0xFF0 == 0 {
                if always { Flow::Return } else { Flow::CondReturn }
            } else {
                Flow::Jump(result.map(u64::from))
            };
        } else if !(8..=11).contains(&op) {
            if rd == 14 && rn == 15 || (op == 13 && rm == 15 && w & (1 << 25) == 0) {
                // mov lr, pc: a call follows.
                state.link = true;
            }
            state.arm_set(rd, result);
            if let Some(v) = result
                && rn == 15
            {
                i.data = Some((v as u64, RefKind::Address));
            }
        }
        return i;
    }
    // ldr/str
    if w & 0x0C00_0000 == 0x0400_0000 {
        let load = w & (1 << 20) != 0;
        let byte = if w & (1 << 22) != 0 { "b" } else { "" };
        let (p, u, wb) = (w & (1 << 24) != 0, w & (1 << 23) != 0, w & (1 << 21) != 0);
        let m = format!("{}{c}{byte}", if load { "ldr" } else { "str" });
        let mut i;
        if w & (1 << 25) == 0 {
            let off = (w & 0xFFF) as i64;
            let off = if u { off } else { -off };
            i = plain(m, format!("{}, {}", reg(rd), address_text(rn, off, p, wb)));
            if let Some(base) = base_value(state, rn, pc8) {
                let a = if p { base.wrapping_add(off as u32) } else { base };
                i.data = Some((a as u64, if load { RefKind::Read } else { RefKind::Write }));
                // A literal pool: the value loaded is known.
                if load && rn == 15 && byte.is_empty() {
                    let v = literal(bytes, a as i64 - pc as i64);
                    state.arm_set(rd, v);
                    if let Some(v) = v {
                        i.operands.push_str(&format!("  ; ={v:#x}"));
                    }
                    if rd == 15 {
                        i.flow = Flow::Jump(v.map(u64::from));
                    }
                    return i;
                }
            }
        } else {
            let sign = if u { "" } else { "-" };
            let amount = (w >> 7) & 31;
            let shifted = if amount == 0 && (w >> 5) & 3 == 0 {
                reg(rm).to_string()
            } else {
                format!("{}, {} #{amount}", reg(rm), SHIFT[((w >> 5) & 3) as usize])
            };
            let text = if p {
                format!("[{}, {sign}{shifted}]{}", reg(rn), if wb { "!" } else { "" })
            } else {
                format!("[{}], {sign}{shifted}", reg(rn))
            };
            i = plain(m, format!("{}, {text}", reg(rd)));
        }
        if load {
            state.arm_set(rd, None);
            if rd == 15 {
                i.flow = Flow::Jump(None);
            }
        }
        if wb || !p {
            state.arm_set(rn, None);
        }
        return i;
    }
    // ldm/stm (push/pop on sp).
    if w & 0x0E00_0000 == 0x0800_0000 {
        let load = w & (1 << 20) != 0;
        let (p, u, wb) = (w & (1 << 24) != 0, w & (1 << 23) != 0, w & (1 << 21) != 0);
        let list = w & 0xFFFF;
        let (m, ops) = if rn == 13 && wb && load && !p && u {
            (format!("pop{c}"), reglist(list))
        } else if rn == 13 && wb && !load && p && !u {
            (format!("push{c}"), reglist(list))
        } else {
            let mode = ["da", "ia", "db", "ib"][((p as usize) << 1) | u as usize];
            (
                format!("{}{c}{mode}", if load { "ldm" } else { "stm" }),
                format!("{}{}, {}", reg(rn), if wb { "!" } else { "" }, reglist(list)),
            )
        };
        let mut i = plain(m, ops);
        if load {
            state.known &= !list;
            if list & (1 << 15) != 0 {
                i.flow = if rn == 13 {
                    if always { Flow::Return } else { Flow::CondReturn }
                } else {
                    Flow::Jump(None)
                };
            }
        }
        if wb {
            state.arm_set(rn, None);
        }
        return i;
    }
    Insn::bad(4, word)
}

/// An immediate operand: 8 bits rotated right by twice the rotation field.
fn rotated(w: u32) -> u32 {
    (w & 0xFF).rotate_right(((w >> 8) & 15) * 2)
}

/// `[r0, #0x10]`, `[r0], #-4`, `[r0, #8]!`
fn address_text(rn: u32, off: i64, pre: bool, wb: bool) -> String {
    let o = if off < 0 {
        format!("#-{:#x}", -off)
    } else {
        format!("#{off:#x}")
    };
    match (pre, off) {
        (true, 0) => format!("[{}]{}", reg(rn), if wb { "!" } else { "" }),
        (true, _) => format!("[{}, {o}]{}", reg(rn), if wb { "!" } else { "" }),
        (false, _) => format!("[{}], {o}", reg(rn)),
    }
}

/// A base register's value, if known (the pc reads as `pc8`, ARM, or the Thumb equivalent).
fn base_value(state: &State, rn: u32, pc_value: u32) -> Option<u32> {
    if rn == 15 { Some(pc_value) } else { state.arm_reg(rn) }
}

fn thumb(bytes: &[u8], pc: u64, state: &mut State) -> Insn {
    let Some(half) = bytes.get(..2) else {
        return Insn::bad(bytes.len() as u32, bytes);
    };
    let h = u16::from_le_bytes([half[0], half[1]]) as u32;
    let pc4 = (pc as u32).wrapping_add(4);
    let lo = |i: u32| reg(i & 7);
    let (rd, rs) = (h & 7, (h >> 3) & 7);
    let plain = |m: &str, ops: String| Insn::new(2, m, ops, Flow::Next);
    // Targets run in Thumb mode: bit 0 set.
    let thumb_target = |t: u32| (t | 1) as u64;
    let link = state.link;
    state.link = false;
    match h >> 11 {
        // lsl/lsr/asr rd, rs, #n
        0..=2 => {
            let m = SHIFT[(h >> 11) as usize];
            state.arm_set(rd, None);
            plain(m, format!("{}, {}, #{}", lo(rd), lo(rs), (h >> 6) & 31))
        }
        // add/sub rd, rs, rn|#n
        3 => {
            let m = if h & (1 << 9) != 0 { "sub" } else { "add" };
            let n = (h >> 6) & 7;
            let (op, v) = if h & (1 << 10) != 0 {
                (imm(n), Some(n))
            } else {
                (lo(n).to_string(), state.arm_reg(n))
            };
            let result = state.arm_reg(rs).zip(v).map(|(a, b)| {
                if m == "add" {
                    a.wrapping_add(b)
                } else {
                    a.wrapping_sub(b)
                }
            });
            state.arm_set(rd, result);
            plain(m, format!("{}, {}, {op}", lo(rd), lo(rs)))
        }
        // mov/cmp/add/sub rd, #imm8
        4..=7 => {
            let r = (h >> 8) & 7;
            let v = h & 0xFF;
            let m = ["mov", "cmp", "add", "sub"][((h >> 11) - 4) as usize];
            let result = match m {
                "mov" => Some(v),
                "add" => state.arm_reg(r).map(|a| a.wrapping_add(v)),
                "sub" => state.arm_reg(r).map(|a| a.wrapping_sub(v)),
                _ => state.arm_reg(r),
            };
            if m != "cmp" {
                state.arm_set(r, result);
            }
            plain(m, format!("{}, {}", lo(r), imm(v)))
        }
        8 => {
            if h & (1 << 10) == 0 {
                // ALU operations
                let op = (h >> 6) & 15;
                let m = [
                    "and", "eor", "lsl", "lsr", "asr", "adc", "sbc", "ror", "tst", "neg", "cmp", "cmn", "orr", "mul",
                    "bic", "mvn",
                ][op as usize];
                if !matches!(op, 8 | 10 | 11) {
                    let result = match op {
                        12 => state.arm_reg(rd).zip(state.arm_reg(rs)).map(|(a, b)| a | b),
                        _ => None,
                    };
                    state.arm_set(rd, result);
                }
                return plain(m, format!("{}, {}", lo(rd), lo(rs)));
            }
            // High register operations and bx.
            let op = (h >> 8) & 3;
            let d = rd | ((h >> 4) & 8);
            let s = (h >> 3) & 15;
            match op {
                0 => {
                    let result = if d == 15 {
                        None
                    } else {
                        state
                            .arm_reg(d)
                            .zip(if s == 15 { Some(pc4) } else { state.arm_reg(s) })
                            .map(|(a, b)| a.wrapping_add(b))
                    };
                    let mut i = plain("add", format!("{}, {}", reg(d), reg(s)));
                    if d == 15 {
                        i.flow = Flow::Jump(None);
                    } else {
                        state.arm_set(d, result);
                    }
                    i
                }
                1 => plain("cmp", format!("{}, {}", reg(d), reg(s))),
                2 => {
                    let v = if s == 15 { Some(pc4) } else { state.arm_reg(s) };
                    let mut i = plain("mov", format!("{}, {}", reg(d), reg(s)));
                    if d == 15 {
                        i.flow = if s == 14 {
                            Flow::Return
                        } else {
                            Flow::Jump(v.map(|t| thumb_target(t & !1)))
                        };
                    } else {
                        if d == 14 && s == 15 {
                            state.link = true;
                        }
                        state.arm_set(d, v);
                    }
                    i
                }
                _ => {
                    let flow = if s == 14 {
                        Flow::Return
                    } else {
                        let t = state.arm_reg(s).map(u64::from);
                        if link { Flow::Call(t) } else { Flow::Jump(t) }
                    };
                    if matches!(flow, Flow::Call(_)) {
                        state.known &= !CALL_CLOBBERS;
                    }
                    Insn::new(2, "bx", reg(s).into(), flow)
                }
            }
        }
        // ldr rd, [pc, #imm]: a literal pool.
        9 => {
            let r = (h >> 8) & 7;
            let at = (pc4 & !3).wrapping_add((h & 0xFF) * 4);
            let v = literal(bytes, at as i64 - pc as i64);
            state.arm_set(r, v);
            let mut i = plain(
                "ldr",
                format!(
                    "{}, [pc, #{:#x}]{}",
                    lo(r),
                    (h & 0xFF) * 4,
                    v.map(|v| format!("  ; ={v:#x}")).unwrap_or_default()
                ),
            );
            i.data = Some((at as u64, RefKind::Read));
            i
        }
        // Loads and stores with a register offset.
        10 | 11 => {
            let ro = (h >> 6) & 7;
            let m = if h & (1 << 9) == 0 {
                ["str", "strb", "ldr", "ldrb"][((h >> 10) & 3) as usize]
            } else {
                ["strh", "ldsb", "ldrh", "ldsh"][((h >> 10) & 3) as usize]
            };
            if m.starts_with('l') {
                state.arm_set(rd, None);
            }
            plain(m, format!("{}, [{}, {}]", lo(rd), lo(rs), lo(ro)))
        }
        // Loads and stores with an immediate offset.
        12..=17 => {
            let (m, scale) = match h >> 11 {
                12 => ("str", 4),
                13 => ("ldr", 4),
                14 => ("strb", 1),
                15 => ("ldrb", 1),
                16 => ("strh", 2),
                _ => ("ldrh", 2),
            };
            let off = ((h >> 6) & 31) * scale;
            let mut i = plain(
                m,
                format!(
                    "{}, [{}{}]",
                    lo(rd),
                    lo(rs),
                    if off > 0 { format!(", #{off:#x}") } else { String::new() }
                ),
            );
            if let Some(base) = state.arm_reg(rs) {
                i.data = Some((
                    (base.wrapping_add(off)) as u64,
                    if m.starts_with('l') {
                        RefKind::Read
                    } else {
                        RefKind::Write
                    },
                ));
            }
            if m.starts_with('l') {
                state.arm_set(rd, None);
            }
            i
        }
        // sp-relative loads and stores.
        18 | 19 => {
            let r = (h >> 8) & 7;
            let load = h >> 11 == 19;
            if load {
                state.arm_set(r, None);
            }
            plain(
                if load { "ldr" } else { "str" },
                format!("{}, [sp, #{:#x}]", lo(r), (h & 0xFF) * 4),
            )
        }
        // add rd, pc|sp, #imm (adr)
        20 | 21 => {
            let r = (h >> 8) & 7;
            let off = (h & 0xFF) * 4;
            if h >> 11 == 20 {
                let v = (pc4 & !3).wrapping_add(off);
                state.arm_set(r, Some(v));
                let mut i = plain("add", format!("{}, pc, #{off:#x}", lo(r)));
                i.data = Some((v as u64, RefKind::Address));
                i
            } else {
                state.arm_set(r, None);
                plain("add", format!("{}, sp, #{off:#x}", lo(r)))
            }
        }
        22 | 23 => {
            if h & 0x0F00 == 0 {
                let off = (h & 0x7F) * 4;
                return plain(if h & 0x80 != 0 { "sub" } else { "add" }, format!("sp, #{off:#x}"));
            }
            if h & 0x0600 == 0x0400 {
                let pop = h & (1 << 11) != 0;
                let extra = if h & 0x100 != 0 {
                    if pop { 1 << 15 } else { 1 << 14 }
                } else {
                    0
                };
                let list = (h & 0xFF) | extra;
                let mut i = plain(if pop { "pop" } else { "push" }, reglist(list));
                if pop {
                    state.known &= !list;
                    if list & (1 << 15) != 0 {
                        i.flow = Flow::Return;
                    }
                }
                return i;
            }
            Insn::bad(2, half)
        }
        // stmia/ldmia rb!, {list}
        24 | 25 => {
            let r = (h >> 8) & 7;
            let load = h >> 11 == 25;
            if load {
                state.known &= !(h & 0xFF);
            }
            state.arm_set(r, None);
            plain(
                if load { "ldmia" } else { "stmia" },
                format!("{}!, {}", lo(r), reglist(h & 0xFF)),
            )
        }
        // Conditional branches, and swi.
        26 | 27 => {
            let cond = (h >> 8) & 15;
            if cond == 15 {
                return Insn::new(2, "swi", format!("{:#x}", h & 0xFF), Flow::Next);
            }
            if cond == 14 {
                return Insn::bad(2, half);
            }
            let t = pc4.wrapping_add((((h & 0xFF) as i8 as i32) * 2) as u32);
            Insn::new(
                2,
                &format!("b{}", COND[cond as usize]),
                format!("{t:#x}"),
                Flow::Branch(thumb_target(t)),
            )
        }
        // b
        28 => {
            let off = ((h & 0x7FF) << 21) as i32 >> 20;
            let t = pc4.wrapping_add(off as u32);
            Insn::new(2, "b", format!("{t:#x}"), Flow::Jump(Some(thumb_target(t))))
        }
        // bl: two halves, the high part of the offset first.
        30 => {
            let Some(second) = bytes.get(2..4) else {
                return Insn::bad(2, half);
            };
            let l = u16::from_le_bytes([second[0], second[1]]) as u32;
            if l >> 11 != 31 {
                return Insn::bad(2, half);
            }
            let high = ((h & 0x7FF) << 21) as i32 >> 9;
            let t = pc4.wrapping_add(high as u32).wrapping_add((l & 0x7FF) << 1);
            state.known &= !CALL_CLOBBERS;
            Insn::new(4, "bl", format!("{t:#x}"), Flow::Call(Some(thumb_target(t))))
        }
        _ => Insn::bad(2, half),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(i: &Insn) -> String {
        format!("{} {}", i.mnemonic, i.operands).trim_end().to_string()
    }

    #[test]
    fn arm_code() {
        let mut s = State::default();
        let le = |w: u32| w.to_le_bytes();
        // b 0x080000c0 (from the header's first word)
        let i = decode(&le(0xEA00_002E), 0x0800_0000, &mut s);
        assert_eq!(
            (text(&i).as_str(), i.flow),
            ("b 0x80000c0", Flow::Jump(Some(0x0800_00C0)))
        );
        // mov r0, #0x04000000 / str r1, [r0] names what r0 points at
        let i = decode(&le(0xE3A0_0301), 0, &mut s);
        assert_eq!(text(&i), "mov r0, #0x4000000");
        let i = decode(&le(0xE580_1000), 0, &mut s);
        assert_eq!(
            (text(&i).as_str(), i.data),
            ("str r1, [r0]", Some((0x0400_0000, RefKind::Write)))
        );
        // ldr r1, [pc, #4] (the pc reads 8 ahead) then mov lr, pc / bx r1: a call into Thumb code.
        let code = [le(0xE59F_1004), le(0xE1A0_E00F), le(0xE12F_FF11), le(0x0800_0201)].concat();
        let i = decode(&code, 0x0800_0100, &mut s);
        assert_eq!(i.data, Some((0x0800_010C, RefKind::Read)));
        decode(&code[4..], 0x0800_0104, &mut s);
        let i = decode(&code[8..], 0x0800_0108, &mut s);
        assert_eq!(i.flow, Flow::Call(Some(0x0800_0201)));
        // bx lr, and pop {r4, pc}
        assert_eq!(decode(&le(0xE12F_FF1E), 0, &mut s).flow, Flow::Return);
        let i = decode(&le(0xE8BD_8010), 0, &mut s);
        assert_eq!((text(&i).as_str(), i.flow), ("pop {r4, pc}", Flow::Return));
    }

    #[test]
    fn thumb_code() {
        let mut s = State {
            thumb: true,
            ..State::default()
        };
        let le = |h: u16| h.to_le_bytes();
        // ldr r0, [pc, #4] (from the word-aligned pc + 4: the pool word 0x04000000) / strh r1, [r0]
        let code = [le(0x4801), le(0x8001), le(0x0000), le(0x0000), le(0x0000), le(0x0400)].concat();
        let i = decode(&code, 0x0800_0200, &mut s);
        assert_eq!(i.data, Some((0x0800_0208, RefKind::Read)));
        let i = decode(&code[2..], 0x0800_0202, &mut s);
        assert_eq!(
            (text(&i).as_str(), i.data),
            ("strh r1, [r0]", Some((0x0400_0000, RefKind::Write)))
        );
        // bl: two halves, one call.
        let bl = [le(0xF000), le(0xF802)].concat();
        let i = decode(&bl, 0x0800_0300, &mut s);
        assert_eq!((i.len, i.flow), (4, Flow::Call(Some(0x0800_0309))));
        // push {r4, lr} / pop {r4, pc} / bx lr / beq
        assert_eq!(text(&decode(&le(0xB510), 0, &mut s)), "push {r4, lr}");
        assert_eq!(decode(&le(0xBD10), 0, &mut s).flow, Flow::Return);
        assert_eq!(decode(&le(0x4770), 0, &mut s).flow, Flow::Return);
        assert_eq!(decode(&le(0xD0FE), 0x0800_0400, &mut s).flow, Flow::Branch(0x0800_0401));
    }
}
