//! Bit dependencies carried by the existing auditor's states and CFG edges.
//! Unknown lowering and cross-register callees remain explicit frontiers.
use super::Transfer;
use crate::cpu::mips::{MipsWord, Ps1GprEffects};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct Bits {
    regs: [u32; 32],
    constants: [Option<u32>; 32],
    hi: u32,
    lo: u32,
    pub pending: Option<(u8, u32)>,
}
pub(super) enum Effect {
    Continue,
    Dead,
    Consumed(&'static str),
    Frontier(&'static str),
}
impl Bits {
    pub fn new(register: u8, mask: u32) -> Self {
        let mut regs = [0; 32];
        regs[register as usize] = mask;
        let mut constants = [None; 32];
        constants[0] = Some(0);
        Self {
            regs,
            constants,
            hi: 0,
            lo: 0,
            pending: None,
        }
    }
    pub fn masks(&self) -> BTreeMap<String, String> {
        let mut out: BTreeMap<_, _> = self
            .regs
            .iter()
            .enumerate()
            .filter(|(_, v)| **v != 0)
            .map(|(i, v)| (format!("gpr{i}"), format!("0x{v:08x}")))
            .collect();
        for (name, mask) in [("hi", self.hi), ("lo", self.lo)] {
            if mask != 0 {
                out.insert(name.into(), format!("0x{mask:08x}"));
            }
        }
        if let Some((r, m)) = self.pending {
            out.insert(format!("pending-gpr{r}"), format!("0x{m:08x}"));
        }
        out
    }
    pub fn has_aliases(&self, register: u8) -> bool {
        self.hi != 0
            || self.lo != 0
            || self
                .regs
                .iter()
                .enumerate()
                .any(|(i, m)| i != register as usize && *m != 0)
    }
    pub fn registers(&self) -> Vec<u8> {
        self.regs
            .iter()
            .enumerate()
            .filter_map(|(i, m)| (*m != 0).then_some(i as u8))
            .collect()
    }
    pub fn non_gpr(&self) -> bool {
        self.hi != 0 || self.lo != 0 || self.pending.is_some()
    }
    pub fn kill(&mut self, register: u8) {
        self.regs[register as usize] = 0;
    }
    pub fn forget_constants(&mut self) {
        self.constants = [None; 32];
        self.constants[0] = Some(0);
    }
    /// Only exact locally established constants prune branches. Calls forget
    /// them, loads stay unknown, and counter mutation naturally changes states.
    pub fn narrow_transfer(&self, pc: u32, w: MipsWord, transfer: &mut Transfer) {
        let a = self.constants[w.rs() as usize];
        let b = self.constants[w.rt() as usize];
        let taken = match w.op() {
            4 => a.zip(b).map(|(a, b)| a == b),
            5 => a.zip(b).map(|(a, b)| a != b),
            6 => a.map(|a| (a as i32) <= 0),
            7 => a.map(|a| (a as i32) > 0),
            1 if w.rt() == 0 => a.map(|a| (a as i32) < 0),
            1 if w.rt() == 1 => a.map(|a| (a as i32) >= 0),
            _ => None,
        };
        if let (Some(taken), Transfer::Flow(targets)) = (taken, transfer) {
            *targets = vec![if taken {
                pc.wrapping_add(4)
                    .wrapping_add((w.simm() as i32).wrapping_mul(4) as u32)
            } else {
                pc.wrapping_add(8)
            }];
        }
    }
    pub fn execute(&mut self, pc: u32, w: MipsWord, e: Ps1GprEffects) -> Effect {
        let (rs, rt, rd) = (w.rs() as usize, w.rt() as usize, w.rd() as usize);
        let (a, b) = (self.regs[rs], self.regs[rt]);
        let (ca, cb) = (self.constants[rs], self.constants[rt]);
        let reads_live = (0..32).any(|r| e.reads & (1 << r) != 0 && self.regs[r] != 0);
        // Control operands, addresses and externally written bits are observable.
        if matches!(w.op(), 1 | 4..=7) || (w.op() == 0 && matches!(w.funct(), 8 | 9)) {
            if reads_live {
                return Effect::Consumed("incoming bits affect a branch or indirect transfer");
            }
        }
        if matches!(w.op(),32..=38|40..=43|46|50|58) && a != 0 {
            return Effect::Consumed("incoming bits affect a memory address");
        }
        if matches!(w.op(), 40 | 41 | 42 | 43 | 46)
            && b & match w.op() {
                40 => 0xff,
                41 => 0xffff,
                _ => u32::MAX,
            } != 0
        {
            return Effect::Consumed("incoming bits reach an observable store lane");
        }
        if matches!(w.op(), 16 | 18) && reads_live {
            return Effect::Consumed("incoming bits reach a coprocessor boundary");
        }
        if (w.op() == 8 || (w.op() == 0 && matches!(w.funct(), 32 | 34))) && reads_live {
            return Effect::Consumed("incoming bits may affect checked arithmetic exceptions");
        }
        let carry = |mask: u32| {
            if mask == 0 {
                0
            } else {
                u32::MAX << mask.trailing_zeros()
            }
        };
        let mut value = 0;
        let mut constant = None;
        match (w.op(), w.funct()) {
            (0, 0) => {
                value = b << w.sa();
                constant = cb.map(|b| b << w.sa());
            }
            (0, 2) => {
                value = b >> w.sa();
                constant = cb.map(|b| b >> w.sa());
            }
            (0, 3) => {
                value = ((b as i32) >> w.sa()) as u32;
                constant = cb.map(|b| ((b as i32) >> w.sa()) as u32);
            }
            (0, 4 | 6 | 7) => {
                if let Some(n) = ca {
                    let n = n & 31;
                    value = match w.funct() {
                        4 => b << n,
                        6 => b >> n,
                        _ => ((b as i32) >> n) as u32,
                    };
                    constant = cb.map(|b| match w.funct() {
                        4 => b << n,
                        6 => b >> n,
                        _ => ((b as i32) >> n) as u32,
                    });
                } else if cb == Some(0) {
                    constant = Some(0);
                } else if a | b != 0 {
                    value = u32::MAX;
                }
            }
            (0, 33) => {
                value = carry(a | b);
                constant = ca.zip(cb).map(|(a, b)| a.wrapping_add(b));
            }
            (0, 35) => {
                if rs != rt {
                    value = carry(a | b);
                    constant = ca.zip(cb).map(|(a, b)| a.wrapping_sub(b));
                } else {
                    constant = Some(0);
                }
            }
            (0, 36) => {
                value = (a & cb.unwrap_or(u32::MAX)) | (b & ca.unwrap_or(u32::MAX));
                constant = if ca == Some(0) || cb == Some(0) {
                    Some(0)
                } else {
                    ca.zip(cb).map(|(a, b)| a & b)
                };
            }
            (0, 37) => {
                value = (a & !cb.unwrap_or(0)) | (b & !ca.unwrap_or(0));
                constant = ca.zip(cb).map(|(a, b)| a | b);
            }
            (0, 38) => {
                if rs != rt {
                    value = a | b;
                    constant = ca.zip(cb).map(|(a, b)| a ^ b);
                } else {
                    constant = Some(0);
                }
            }
            (0, 39) => {
                value = (a & !cb.unwrap_or(0)) | (b & !ca.unwrap_or(0));
                constant = ca.zip(cb).map(|(a, b)| !(a | b));
            }
            (0, 42 | 43) => {
                value = u32::from(a | b != 0);
                constant = ca.zip(cb).map(|(a, b)| {
                    u32::from(if w.funct() == 42 {
                        (a as i32) < (b as i32)
                    } else {
                        a < b
                    })
                });
            }
            (0, 16) => value = self.hi,
            (0, 18) => value = self.lo,
            (0, 17) => self.hi = a,
            (0, 19) => self.lo = a,
            (0, 24..=27) => {
                let mask = if a | b == 0 { 0 } else { u32::MAX };
                self.hi = mask;
                self.lo = mask;
            }
            (0, 9) | (3, _) => {
                constant = Some(pc.wrapping_add(8));
            }
            (0, 8) | (2, _) => {}
            (1 | 4..=7, _) => {}
            (9, _) => {
                value = carry(a);
                constant = ca.map(|a| a.wrapping_add(w.simm() as u32));
            }
            (10 | 11, _) => {
                value = u32::from(a != 0);
                constant = ca.map(|a| {
                    u32::from(if w.op() == 10 {
                        (a as i32) < (w.simm() as i32)
                    } else {
                        a < (w.simm() as u32)
                    })
                });
            }
            (12, _) => {
                value = a & w.imm();
                constant = ca.map(|a| a & w.imm());
            }
            (13, _) => {
                value = a & !w.imm();
                constant = ca.map(|a| a | w.imm());
            }
            (14, _) => {
                value = a;
                constant = ca.map(|a| a ^ w.imm());
            }
            (15, _) => constant = Some(w.imm() << 16),
            (34 | 38, _) => {
                let old = self
                    .pending
                    .filter(|(r, _)| usize::from(*r) == rt)
                    .map_or(b, |(_, m)| m);
                let keep = ca
                    .map(|a| {
                        let offset = a.wrapping_add(w.simm() as u32) & 3;
                        if w.op() == 34 {
                            let n = (3 - offset) * 8;
                            if n == 0 { 0 } else { (1u32 << n) - 1 }
                        } else {
                            let n = (4 - offset) * 8;
                            if n == 32 { 0 } else { u32::MAX << n }
                        }
                    })
                    .unwrap_or(u32::MAX);
                value = old & keep;
            }
            (32 | 33 | 35 | 36 | 37 | 40 | 41 | 42 | 43 | 46 | 50 | 58 | 16 | 18, _) => {}
            // ADD/SUB without tracked input still need exact constants to keep
            // branch feasibility honest; overflow is an explicit frontier.
            (8, _) => {
                if let Some(a) = ca {
                    if let Some(v) = (a as i32).checked_add(w.simm() as i32) {
                        constant = Some(v as u32);
                    } else {
                        return Effect::Frontier("checked arithmetic overflow frontier");
                    }
                }
            }
            (0, 32 | 34) => {
                if let Some((a, b)) = ca.zip(cb) {
                    let v = if w.funct() == 32 {
                        (a as i32).checked_add(b as i32)
                    } else {
                        (a as i32).checked_sub(b as i32)
                    };
                    if let Some(v) = v {
                        constant = Some(v as u32);
                    } else {
                        return Effect::Frontier("checked arithmetic overflow frontier");
                    }
                }
            }
            _ => return Effect::Frontier("instruction needs an observable-bit lowering"),
        }
        // Every instruction reads old architectural GPRs before the previous
        // delayed load commits. An immediate write then replaces its destination.
        if let Some((r, m)) = self.pending.take() {
            self.regs[r as usize] = m;
            self.constants[r as usize] = None;
        }
        if let Some(r) = e.write {
            if e.delayed_write {
                self.pending = Some((r, value));
            } else {
                self.regs[r as usize] = value;
                self.constants[r as usize] = if value == 0 { constant } else { None };
            }
        }
        let _ = rd;
        if self.regs.iter().all(|m| *m == 0) && self.hi == 0 && self.lo == 0 && self.pending.is_none_or(|(_, m)| m == 0)
        {
            Effect::Dead
        } else {
            Effect::Continue
        }
    }
}
