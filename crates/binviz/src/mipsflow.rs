//! What a MIPS function does with the pointers in its registers, read once in
//! address order: where each one came from (an argument, a global's address,
//! a local's address on the stack, a word loaded through another pointer, what
//! a call returned), the offsets reached through it, the pointers stored
//! through it, what each call is passed, and the stack slots it uses.
//!
//! One pass, no control flow: a register keeps what it was last given in
//! address order, which is what GCC's code mostly means; branches that join
//! with other values in a register make some of it wrong, so what comes out
//! is evidence to weigh, not a proof. Three things read it: structures matched
//! up across calls ([`crate::structs`]), the arguments callers pass, and a
//! function's stack slots ([`crate::signature`]).

use std::collections::HashMap;

use crate::cpu::mips::{CALL_CLOBBERS, MipsWord};

/// Where a followed pointer came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Origin {
    /// The function's own argument: 0–3 in `$a0`–`$a3`, 4 on from the stack.
    Arg(u8),
    /// The address of a global.
    Global(u64),
    /// The address of a slot in the function's frame (its offset from `$sp`).
    Local(i32),
    /// The word loaded from (another pointer, the offset).
    Deref(u32, i32),
    /// What a call to the function at this address returned.
    Returned(u64),
}

/// A pointer and how far into what it points at: `(node, delta)`.
pub(crate) type Value = (u32, i32);

/// A load or store through a followed pointer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FieldUse {
    pub node: u32,
    pub offset: i32,
    pub width: u8,
    pub store: bool,
    /// Through the pointer itself, not one moved along it (`p + 4`): only
    /// these say much about the argument's own type.
    pub direct: bool,
}

/// What a call is passed.
#[derive(Debug, Clone)]
pub(crate) struct CallSite {
    /// `None` for a call through a register.
    pub target: Option<u64>,
    /// For `$a0`–`$a3`: set up since the function's entry or the call before.
    pub set: [bool; 4],
    /// For `$a0`–`$a3`: the pointer each holds, if followed.
    pub args: [Option<Value>; 4],
    /// Arguments past the fourth, stored to the outgoing area just before the
    /// call and never read back: (index from 4, the pointer, if followed).
    pub stack: Vec<(u32, Option<Value>)>,
}

/// A load or store off `$sp` (or `$fp` set from it).
#[derive(Debug, Clone, Copy)]
pub(crate) struct SlotUse {
    pub offset: i32,
    pub width: u8,
    pub store: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Flow {
    pub nodes: Vec<Origin>,
    pub fields: Vec<FieldUse>,
    /// A pointer stored through another: (node, offset, the node stored).
    pub stores: Vec<(u32, i32, u32)>,
    pub calls: Vec<CallSite>,
    /// Pointers returned in `$v0`.
    pub returns: Vec<u32>,
    /// Bytes the prologue reserves.
    pub frame: u32,
    pub slots: Vec<SlotUse>,
    /// Frame offsets whose address is taken (`addiu $a1, $sp, 0x18`).
    pub taken: Vec<i32>,
    /// Registers the prologue saves, and where.
    pub saved: Vec<(u32, i32)>,
    /// Pointers moved along by a constant (`addiu $t1, $t0, 4`): walked
    /// through, as a copy loop does, or pointing into their middle.
    pub moved: Vec<u32>,
}

/// Reads a function's words at `start`. `is_address` says whether a
/// constant is an address in the image (a global); `gp` is `$gp`'s value,
/// when known.
pub(crate) fn scan(words: &[MipsWord], start: u64, is_address: &dyn Fn(u64) -> bool, gp: Option<u32>) -> Flow {
    let mut f = Flow::default();
    let mut cache: HashMap<Origin, u32> = HashMap::new();
    let mut node = |f: &mut Flow, o: Origin| -> u32 {
        *cache.entry(o).or_insert_with(|| {
            f.nodes.push(o);
            f.nodes.len() as u32 - 1
        })
    };
    let mut val: [Option<Value>; 32] = [None; 32];
    let mut konst: [Option<u32>; 32] = [None; 32];
    konst[0] = Some(0);
    konst[28] = gp;
    for a in 0..4u8 {
        val[4 + a as usize] = Some((node(&mut f, Origin::Arg(a)), 0));
    }
    let mut written_at: [Option<usize>; 32] = [None; 32];
    let mut window = 0usize;
    let mut fp_is_sp = false;
    // What was last stored to each slot of the frame (spills come back).
    let mut spilled: HashMap<i32, Option<Value>> = HashMap::new();
    // Stores to the outgoing area since the call before: (offset, value).
    let mut outgoing: Vec<(i32, Option<Value>)> = Vec::new();
    let mut pending_call: Option<(usize, Option<u64>)> = None;
    let mut pending_return: Option<usize> = None;
    for (n, &w) in words.iter().enumerate() {
        let pc = start + 4 * n as u64;
        let (op, rs, rt) = (w.op(), w.rs(), w.rt());
        let fp = fp_is_sp;
        let on_stack = move |r: u32| r == 29 || (r == 30 && fp);
        let mut value: Option<Value> = None;
        let mut constant: Option<u32> = None;
        match op {
            // Loads and stores (lwl/lwr/swl/swr aside: unaligned halves).
            32 | 33 | 35 | 36 | 37 | 40 | 41 | 43 | 49 | 50 | 57 | 58 => {
                let width = match op {
                    32 | 36 | 40 => 1,
                    33 | 37 | 41 => 2,
                    _ => 4,
                };
                let store = op >= 40;
                let gpr = !matches!(op, 49 | 50 | 57 | 58);
                let off = w.simm() as i32;
                if on_stack(rs) {
                    f.slots.push(SlotUse {
                        offset: off,
                        width,
                        store,
                    });
                    if store {
                        let callee_saved = (16..24).contains(&rt) || rt >= 30;
                        let saving = gpr && n < 24 && callee_saved && written_at[rt as usize].is_none();
                        if saving && f.saved.iter().all(|s| s.0 != rt) {
                            f.saved.push((rt, off));
                        }
                        spilled.insert(off, if op == 43 { val[rt as usize] } else { None });
                        if !saving && f.frame > 0 && off >= 16 && (off as u32) < f.frame && op == 43 {
                            outgoing.push((off, val[rt as usize]));
                        }
                    } else if op == 35 {
                        value = if f.frame > 0 && off as u32 >= f.frame + 16 {
                            let k = (off as u32 - f.frame - 16) / 4;
                            Some((node(&mut f, Origin::Arg(4 + k.min(250) as u8)), 0))
                        } else {
                            spilled.get(&off).copied().flatten()
                        };
                    }
                } else {
                    let base = match konst[rs as usize] {
                        Some(c) => {
                            let a = c.wrapping_add(off as u32) as u64;
                            is_address(a).then(|| (node(&mut f, Origin::Global(a)), 0))
                        }
                        None => val[rs as usize].map(|(p, d)| (p, d + off)),
                    };
                    if let Some((p, at)) = base {
                        f.fields.push(FieldUse {
                            node: p,
                            offset: at,
                            width,
                            store,
                            direct: konst[rs as usize].is_some() || val[rs as usize].is_some_and(|v| v.1 == 0),
                        });
                        if op == 35 {
                            value = Some((node(&mut f, Origin::Deref(p, at)), 0));
                        } else if op == 43
                            && let Some((q, 0)) = val[rt as usize]
                        {
                            f.stores.push((p, at, q));
                        }
                    }
                }
            }
            // lui
            15 => constant = Some(w.imm() << 16),
            // addiu, addi, ori
            8 | 9 | 13 => {
                if rt == 29 {
                    if rs == 29 && w.simm() < 0 && f.frame == 0 {
                        f.frame = (-w.simm()) as u32;
                    }
                } else if on_stack(rs) {
                    f.taken.push(w.simm() as i32);
                    value = Some((node(&mut f, Origin::Local(w.simm() as i32)), 0));
                } else if let Some(c) = konst[rs as usize] {
                    let c = if op == 13 {
                        c | w.imm()
                    } else {
                        c.wrapping_add(w.simm() as u32)
                    };
                    constant = Some(c);
                    if is_address(c as u64) {
                        value = Some((node(&mut f, Origin::Global(c as u64)), 0));
                    }
                } else if op != 13 {
                    value = val[rs as usize].map(|(p, d)| (p, d + w.simm() as i32));
                    if let Some((p, _)) = val[rs as usize]
                        && w.simm() != 0
                        && !f.moved.contains(&p)
                    {
                        f.moved.push(p);
                    }
                }
            }
            0 => match w.funct() {
                // addu, add, or: a move when one side is $zero.
                32 | 33 | 37 => {
                    let rd = w.rd();
                    let other = if rt == 0 {
                        Some(rs)
                    } else if rs == 0 {
                        Some(rt)
                    } else {
                        None
                    };
                    match other {
                        Some(src) => {
                            value = val[src as usize];
                            constant = konst[src as usize];
                            if rd == 30 && src == 29 {
                                fp_is_sp = true;
                            }
                        }
                        None => {
                            if let (Some(a), Some(b)) = (konst[rs as usize], konst[rt as usize]) {
                                constant = Some(if w.funct() == 37 { a | b } else { a.wrapping_add(b) });
                            }
                        }
                    }
                }
                8 if rs == 31 => pending_return = Some(n),
                9 => pending_call = Some((n, None)),
                _ => {}
            },
            3 => {
                let target = ((pc + 4) & 0xF000_0000) | ((w.0 as u64 & 0x03FF_FFFF) << 2);
                pending_call = Some((n, Some(target)));
            }
            _ => {}
        }
        if let Some(reg) = w.writes()
            && reg != 29
        {
            let reg = reg as usize;
            val[reg] = value;
            konst[reg] = constant;
            written_at[reg] = Some(n);
            if reg == 30 && !(op == 0 && rs == 29) {
                fp_is_sp = false;
            }
        }
        // A call happens after its delay slot.
        if let Some((at, target)) = pending_call
            && n == at + 1
        {
            let set = std::array::from_fn(|k| written_at[4 + k].is_some_and(|i| i >= window));
            // Outgoing arguments, the last store to each slot.
            let mut stack: Vec<(u32, Option<Value>)> = Vec::new();
            for &(off, v) in outgoing.iter().rev() {
                let k = (off as u32 - 16) / 4;
                if off % 4 == 0 && stack.iter().all(|s| s.0 != k) {
                    stack.push((k, v));
                }
            }
            stack.sort_by_key(|s| s.0);
            f.calls.push(CallSite {
                target,
                set,
                args: std::array::from_fn(|k| val[4 + k]),
                stack,
            });
            outgoing.clear();
            for r in 0..32 {
                if CALL_CLOBBERS & (1 << r) != 0 && r != 28 {
                    val[r] = None;
                    konst[r] = None;
                }
            }
            val[2] = target.map(|t| (node(&mut f, Origin::Returned(t)), 0));
            written_at[2] = Some(n);
            window = n + 1;
            pending_call = None;
        }
        if let Some(at) = pending_return
            && n == at + 1
        {
            // Only what the function itself put in $v0, not what a call left there.
            if let Some((v, 0)) = val[2]
                && written_at[2].is_some_and(|i| i >= window)
            {
                f.returns.push(v);
            }
            pending_return = None;
        }
    }
    // A slot read back anywhere is a local, not an outgoing argument; so is
    // one at or above a local whose address is taken (the outgoing area is
    // the bottom of the frame, below every local): an array zeroed before
    // its address is passed.
    let read: std::collections::HashSet<i32> = f.slots.iter().filter(|s| !s.store).map(|s| s.offset).collect();
    let lowest_taken = f.taken.iter().copied().min().unwrap_or(i32::MAX);
    for c in &mut f.calls {
        c.stack.retain(|&(k, _)| {
            let off = 16 + 4 * k as i32;
            !read.contains(&off) && off < lowest_taken
        });
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(w: &[u32]) -> Vec<MipsWord> {
        w.iter().map(|&x| MipsWord(x)).collect()
    }

    #[test]
    fn follows_pointers_calls_and_slots() {
        // addiu $sp,-0x28 / sw $ra,0x24($sp) / sw $s0,0x20($sp) / move $s0,$a0 /
        // lw $a0,0x10($s0) (a pointer in a field) / addiu $a1,$sp,0x18 (a local's address) /
        // sw $s0,0x10($sp) (a fifth argument) / jal 0x80010100 / lhu $t0,4($s0) /
        // lui $v1,0x8002 / lw $v1,0x40($v1) (a global) / sw $v1,8($s0) (a pointer stored in a field) /
        // lw $ra,0x24($sp) / lw $s0,0x20($sp) / jr $ra / addiu $sp,0x28
        let w = words(&[
            0x27BD_FFD8,
            0xAFBF_0024,
            0xAFB0_0020,
            0x0080_8021,
            0x8E04_0010,
            0x27A5_0018,
            0xAFB0_0010,
            0x0C00_4040,
            0x9608_0004,
            0x3C03_8002,
            0x8C63_0040,
            0xAE03_0008,
            0x8FBF_0024,
            0x8FB0_0020,
            0x03E0_0008,
            0x27BD_0028,
        ]);
        let f = scan(&w, 0x8001_0000, &|a| (0x8000_0000..0x8020_0000).contains(&a), None);
        assert_eq!(f.frame, 0x28);
        assert_eq!(f.saved, [(31, 0x24), (16, 0x20)]);
        assert_eq!(f.taken, [0x18]);
        let c = &f.calls[0];
        assert_eq!(c.target, Some(0x8001_0100));
        assert_eq!(c.set, [true, true, false, false]);
        let a0 = f.nodes.iter().position(|o| *o == Origin::Arg(0)).unwrap() as u32;
        assert_eq!(f.nodes[c.args[0].unwrap().0 as usize], Origin::Deref(a0, 0x10));
        assert_eq!(f.nodes[c.args[1].unwrap().0 as usize], Origin::Local(0x18));
        assert_eq!(c.stack.len(), 1);
        assert_eq!(f.nodes[c.stack[0].1.unwrap().0 as usize], Origin::Arg(0));
        // $s0 still holds a0 after the call: offset 4 read, 8 written with the global's word.
        assert!(
            f.fields
                .iter()
                .any(|u| u.node == a0 && u.offset == 4 && u.width == 2 && !u.store)
        );
        let g = f.nodes.iter().position(|o| *o == Origin::Global(0x8002_0040)).unwrap() as u32;
        let loaded = f.nodes.iter().position(|o| *o == Origin::Deref(g, 0)).unwrap() as u32;
        assert!(f.stores.contains(&(a0, 8, loaded)));
    }
}
