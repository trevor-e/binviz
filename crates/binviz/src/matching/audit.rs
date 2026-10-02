//! The original and the rebuild side by side, for what a percent hides: the
//! immediates and load/store offsets present on one side only, the mnemonic
//! mix (`lb` for `lbu`, `sra` for `srl`), the stores to the stack, and the
//! argument registers set before each call. A decompilation reads these for
//! the bugs a near-match keeps: a constant copied from another unit, a wrong
//! width, an argument fewer.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Words, text};
use crate::cpu::mips::MipsWord;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Audit {
    /// Immediates of arithmetic, logic and compare instructions and shift
    /// amounts (not addresses, not the frame) on one side only: (value, times).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub immediates_original_only: Vec<(String, u32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub immediates_rebuilt_only: Vec<(String, u32)>,
    /// Load and store offsets off a register (not the stack, not relocated),
    /// as "lw 0x24", on one side only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub offsets_original_only: Vec<(String, u32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub offsets_rebuilt_only: Vec<(String, u32)>,
    /// Mnemonics whose counts differ: (mnemonic, original, rebuilt).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mnemonics: Vec<(String, u32, u32)>,
    /// Stores to the stack, as "sw 0x10", on one side only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack_stores_original_only: Vec<(String, u32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack_stores_rebuilt_only: Vec<(String, u32)>,
    /// Calls to the same function whose arguments set before them differ.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<CallArity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallArity {
    pub callee: String,
    /// Arguments set before the call: the highest `$a` register written in the
    /// instructions leading up to it (and its delay slot) plus one, or four
    /// plus the stores to the stack's argument area.
    pub original_args: u32,
    pub rebuilt_args: u32,
    /// What the callee's own code reads, when the original knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_args: Option<u32>,
}

impl Audit {
    pub fn is_empty(&self) -> bool {
        *self == Audit::default()
    }

    pub fn to_text(&self) -> String {
        let list = |v: &[(String, u32)]| {
            v.iter()
                .map(|(k, n)| if *n > 1 { format!("{k} x{n}") } else { k.clone() })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut out = String::new();
        if !self.immediates_original_only.is_empty() || !self.immediates_rebuilt_only.is_empty() {
            out.push_str(&format!(
                "  immediates only in the original: {}; only in the rebuild: {}\n",
                list(&self.immediates_original_only),
                list(&self.immediates_rebuilt_only)
            ));
        }
        if !self.offsets_original_only.is_empty() || !self.offsets_rebuilt_only.is_empty() {
            out.push_str(&format!(
                "  offsets only in the original: {}; only in the rebuild: {}\n",
                list(&self.offsets_original_only),
                list(&self.offsets_rebuilt_only)
            ));
        }
        if !self.mnemonics.is_empty() {
            out.push_str("  mnemonic counts (original/rebuilt): ");
            out.push_str(
                &self
                    .mnemonics
                    .iter()
                    .map(|(m, a, b)| format!("{m} {a}/{b}"))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push('\n');
        }
        if !self.stack_stores_original_only.is_empty() || !self.stack_stores_rebuilt_only.is_empty() {
            out.push_str(&format!(
                "  stack stores only in the original: {}; only in the rebuild: {}\n",
                list(&self.stack_stores_original_only),
                list(&self.stack_stores_rebuilt_only)
            ));
        }
        for c in &self.calls {
            out.push_str(&format!(
                "  call {}: {} arguments set in the original, {} in the rebuild{}\n",
                c.callee,
                c.original_args,
                c.rebuilt_args,
                c.signature_args
                    .map_or(String::new(), |n| format!(" (the callee reads {n})"))
            ));
        }
        out
    }
}

/// What one side's instructions say.
#[derive(Default)]
struct Side {
    immediates: BTreeMap<String, u32>,
    offsets: BTreeMap<String, u32>,
    mnemonics: BTreeMap<String, u32>,
    stack_stores: BTreeMap<String, u32>,
    /// (callee, arguments set before the call), in order.
    calls: Vec<(String, u32)>,
}

fn is_branch(w: MipsWord) -> bool {
    matches!(w.op(), 1..=7) || (w.op() == 0 && matches!(w.funct(), 8 | 9))
}

fn is_store(w: MipsWord) -> bool {
    matches!(w.op(), 0x28 | 0x29 | 0x2A | 0x2B | 0x2E)
}

fn is_memory(w: MipsWord) -> bool {
    matches!(w.op(), 0x20..=0x26 | 0x28..=0x2B | 0x2E | 0x31 | 0x32 | 0x39 | 0x3A)
}

/// Reads one side: `relocated(i)` says whether instruction `i` carries a
/// relocation (the rebuild), `callee(i)` names the function a `jal` at `i`
/// calls.
fn read(
    words: &[u32],
    start: u64,
    big: bool,
    relocated: &dyn Fn(usize) -> bool,
    callee: &dyn Fn(usize) -> Option<String>,
) -> Side {
    let mut side = Side::default();
    // Which registers hold an address: set by `lui`, kept by the `addiu`/`ori`
    // that completes it and by an `addu` off it, lost to any other writer and
    // to a call (the caller-saved ones). An immediate or offset off such a
    // register is part of an address, not a constant.
    let mut holds_address = [false; 32];
    let mut clobber_after: Option<usize> = None;
    for (i, &raw) in words.iter().enumerate() {
        let w = MipsWord(raw);
        if clobber_after == Some(i) {
            for r in (1..16).chain(24..26) {
                holds_address[r] = false;
            }
            clobber_after = None;
        }
        if raw == 0 {
            continue;
        }
        let t = text(raw, start + 4 * i as u64, big);
        let mnemonic = t.split(' ').next().unwrap_or("").to_string();
        *side.mnemonics.entry(mnemonic.clone()).or_default() += 1;
        let paired = |r: u32| holds_address[r as usize];
        let keeps = match w.op() {
            0xF => true,
            9 | 0xD => paired(w.rs()),
            0 if matches!(w.funct(), 0x21 | 0x23) => paired(w.rs()) || paired(w.rt()),
            _ => false,
        };
        let written = w.writes();
        if w.op() == 3 || (w.op() == 0 && w.funct() == 9) {
            clobber_after = Some(i + 2);
        }
        if w.op() == 0xF {
            holds_address[w.rt() as usize] = true;
            continue;
        }
        let hex = |v: i64| {
            if v < 0 {
                format!("-{:#x}", -v)
            } else {
                format!("{v:#x}")
            }
        };
        // A relocated field is an address, not a constant (the rebuild's `jal`
        // fields are relocated too: the call itself still counts below).
        match w.op() {
            _ if relocated(i) => {}
            // addi, addiu, slti, sltiu, andi, ori, xori
            8..=0xE => {
                if w.rs() != 29 && !paired(w.rs()) {
                    *side.immediates.entry(hex(w.simm())).or_default() += 1;
                }
            }
            0 if matches!(w.funct(), 0 | 2 | 3) => {
                *side.immediates.entry(format!("{}<{}", mnemonic, w.sa())).or_default() += 1;
            }
            _ if is_memory(w) => {
                if w.rs() == 29 {
                    if is_store(w) {
                        *side
                            .stack_stores
                            .entry(format!("{mnemonic} {}", hex(w.simm())))
                            .or_default() += 1;
                    }
                } else if !paired(w.rs()) {
                    *side.offsets.entry(format!("{mnemonic} {}", hex(w.simm()))).or_default() += 1;
                }
            }
            _ => {}
        }
        if let Some(r) = written
            && r != 0
        {
            holds_address[r as usize] = keeps;
        }
        if w.op() == 3
            && let Some(name) = callee(i)
        {
            // The arguments set on the way to the call: back to the previous
            // branch or call, and the delay slot.
            let mut a_regs = 0u32;
            let mut stack = 0u32;
            let mut seen = |k: usize| {
                let x = MipsWord(words[k]);
                if let Some(r) = x.writes()
                    && (4..=7).contains(&r)
                {
                    a_regs = a_regs.max(r - 4 + 1);
                }
                if is_store(x) && x.rs() == 29 && x.simm() >= 0x10 {
                    stack += 1;
                }
            };
            let lo = i.saturating_sub(12);
            for k in (lo..i).rev() {
                let x = MipsWord(words[k]);
                if is_branch(x) {
                    break;
                }
                // The delay slot of a branch before: that branch's, not ours.
                if k > 0 && is_branch(MipsWord(words[k - 1])) {
                    seen(k);
                    break;
                }
                seen(k);
            }
            if i + 1 < words.len() {
                seen(i + 1);
            }
            let args = if stack > 0 { 4 + stack } else { a_regs };
            side.calls.push((name, args));
        }
    }
    side
}

fn only_in(a: &BTreeMap<String, u32>, b: &BTreeMap<String, u32>) -> Vec<(String, u32)> {
    a.iter()
        .filter_map(|(k, &n)| {
            let m = b.get(k).copied().unwrap_or(0);
            (n > m).then(|| (k.clone(), n - m))
        })
        .collect()
}

/// The audit of `orig` against the rebuild `func` at `start`: `callee_of`
/// names the function the original's `jal` at an address calls,
/// `signature_args` says how many arguments a function of that name reads.
pub(crate) fn audit(
    orig: &[u32],
    start: u64,
    big: bool,
    func: &Words,
    callee_of: &dyn Fn(u64) -> Option<String>,
    signature_args: &dyn Fn(&str) -> Option<u32>,
) -> Option<Audit> {
    let a = read(orig, start, big, &|_| false, &|i| {
        let w = MipsWord(orig[i]);
        let pc = start + 4 * i as u64;
        let dest = ((pc + 4) & 0xF000_0000) | (u64::from(w.0 & 0x03FF_FFFF) << 2);
        callee_of(dest)
    });
    let b = read(
        &func.words,
        start,
        func.big_endian,
        &|i| func.relocs.contains_key(&i),
        &|i| func.relocs.get(&i).map(|r| r.symbol.clone()),
    );
    let mut mnemonics: Vec<(String, u32, u32)> = a
        .mnemonics
        .keys()
        .chain(b.mnemonics.keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|m| {
            (
                m.clone(),
                a.mnemonics.get(m).copied().unwrap_or(0),
                b.mnemonics.get(m).copied().unwrap_or(0),
            )
        })
        .filter(|(_, x, y)| x != y)
        .collect();
    mnemonics.sort_by_key(|(m, x, y)| (std::cmp::Reverse(x.abs_diff(*y)), m.clone()));
    mnemonics.truncate(16);
    // Calls paired by callee, in order of appearance on each side.
    let mut calls = Vec::new();
    let mut by_name: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    for (name, n) in &b.calls {
        by_name.entry(name.as_str()).or_default().push(*n);
    }
    let mut taken: BTreeMap<&str, usize> = BTreeMap::new();
    for (name, n) in &a.calls {
        let k = taken.entry(name.as_str()).or_default();
        if let Some(&m) = by_name.get(name.as_str()).and_then(|v| v.get(*k)) {
            *k += 1;
            if m != *n {
                calls.push(CallArity {
                    callee: name.clone(),
                    original_args: *n,
                    rebuilt_args: m,
                    signature_args: signature_args(name),
                });
            }
        }
    }
    let out = Audit {
        immediates_original_only: only_in(&a.immediates, &b.immediates),
        immediates_rebuilt_only: only_in(&b.immediates, &a.immediates),
        offsets_original_only: only_in(&a.offsets, &b.offsets),
        offsets_rebuilt_only: only_in(&b.offsets, &a.offsets),
        mnemonics,
        stack_stores_original_only: only_in(&a.stack_stores, &b.stack_stores),
        stack_stores_rebuilt_only: only_in(&b.stack_stores, &a.stack_stores),
        calls,
    };
    (!out.is_empty()).then_some(out)
}
