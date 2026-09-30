//! What the code does through a pointer kept in a global: which offsets it
//! loads, stores and takes the address of, and in which functions. A game's
//! state lives in a few big structures reached through global pointers
//! (FF9 keeps its file manager behind one, read 295 times); `refs` on the
//! global lists every read of the pointer, where this lists the fields.
//!
//! MIPS: after `lw $rt, global`, the register holds the pointer. It is
//! followed down the straight line of code that follows (through `move`s and
//! `addiu`s, which keep the offset), until every register that held it has
//! been overwritten, or the path ends.

use serde::Serialize;

use crate::binary::Binary;
use crate::cpu::mips::MipsWord;
use crate::util::Endian;

/// Instructions followed after loading the pointer.
const WINDOW: usize = 32;
/// Read sites of one global looked at.
const MAX_SITES: usize = 20_000;

/// One use of a field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldAccess {
    /// The field's offset in the structure the global points to.
    pub offset: i64,
    /// Bytes loaded or stored; 0 when the field's address is taken (`&p->field`).
    pub width: u8,
    pub store: bool,
    /// The instruction.
    pub site: u64,
}

/// Loads and stores: (width, store).
fn memory_access(w: MipsWord) -> Option<(u8, bool)> {
    Some(match w.op() {
        32 | 36 => (1, false),
        33 | 37 => (2, false),
        34 | 35 | 38 | 39 | 48 | 49 | 50 => (4, false),
        26 | 27 | 53 | 55 => (8, false),
        40 => (1, true),
        41 => (2, true),
        42 | 43 | 46 | 56 | 57 | 58 => (4, true),
        44 | 45 | 61 | 63 => (8, true),
        _ => return None,
    })
}

impl Binary {
    fn mips_word_at(&self, pc: u64) -> Option<MipsWord> {
        let endian = self.mips_endian()?;
        let at = self.address_to_offset(pc)? as usize;
        let b: [u8; 4] = self.data.get(at..at + 4)?.try_into().ok()?;
        Some(MipsWord(match endian {
            Endian::Little => u32::from_le_bytes(b),
            Endian::Big => u32::from_be_bytes(b),
        }))
    }

    /// Every load, store and address-of through the pointer the global at `global` holds (MIPS),
    /// by site.
    pub fn field_accesses(&self, global: u64) -> Vec<FieldAccess> {
        let mut out = Vec::new();
        if self.mips_endian().is_none() {
            return out;
        }
        let index = self.xref_index();
        for &v in index
            .range(crate::xrefs::RefKind::Read, global, global + 1)
            .iter()
            .take(MAX_SITES)
        {
            let site = index.source(v);
            let Some(load) = self.mips_word_at(site) else { continue };
            // `lw $rt, %lo(global)($rs)`: the pointer is now in $rt.
            if load.op() != 35 || load.rt() == 0 {
                continue;
            }
            // Where each register stands relative to the pointer, if it holds it.
            let mut held: [Option<i64>; 32] = [None; 32];
            held[load.rt() as usize] = Some(0);
            let mut pc = site + 4;
            for _ in 0..WINDOW {
                let Some(w) = self.mips_word_at(pc) else { break };
                // A load or store off a register that holds it.
                if let Some((width, store)) = memory_access(w)
                    && let Some(base) = held[w.rs() as usize]
                {
                    out.push(FieldAccess {
                        offset: base + w.simm(),
                        width,
                        store,
                        site: pc,
                    });
                }
                // Copies and offsets keep it; anything else written over a register drops it.
                let copy = match (w.op(), w.funct()) {
                    // move: addu/or rd, rs, $zero
                    (0, 33 | 37) if w.rt() == 0 => held[w.rs() as usize].map(|o| (w.rd() as usize, o, false)),
                    (0, 33 | 37) if w.rs() == 0 => held[w.rt() as usize].map(|o| (w.rd() as usize, o, false)),
                    (9, _) | (8, _) => held[w.rs() as usize].map(|o| (w.rt() as usize, o + w.simm(), true)),
                    _ => None,
                };
                if let Some((to, offset, address_of)) = copy {
                    if address_of && w.simm() != 0 {
                        out.push(FieldAccess {
                            offset,
                            width: 0,
                            store: false,
                            site: pc,
                        });
                    }
                    if to != 0 {
                        held[to] = Some(offset);
                    }
                } else if let Some(reg) = w.writes() {
                    held[reg as usize] = None;
                }
                let is_call = (w.op() == 3) || (w.op() == 0 && w.funct() == 9);
                let is_jump = (w.op() == 2) || (w.op() == 0 && w.funct() == 8);
                if is_call || is_jump {
                    // The delay slot runs before the callee; then the callee clobbers the
                    // registers it may (a call's $v, $a, $t and $at); $s registers survive.
                    pc += 4;
                    if let Some(slot) = self.mips_word_at(pc) {
                        if let Some((width, store)) = memory_access(slot)
                            && let Some(base) = held[slot.rs() as usize]
                        {
                            out.push(FieldAccess {
                                offset: base + slot.simm(),
                                width,
                                store,
                                site: pc,
                            });
                        }
                        // `jal f` / `addiu $a0, $v1, 0x20`: the field's address is the argument.
                        if matches!(slot.op(), 8 | 9)
                            && slot.simm() != 0
                            && let Some(base) = held[slot.rs() as usize]
                        {
                            out.push(FieldAccess {
                                offset: base + slot.simm(),
                                width: 0,
                                store: false,
                                site: pc,
                            });
                        }
                    }
                    if is_jump {
                        break;
                    }
                    // Caller-saved registers: $at, $v0-$v1, $a0-$a3, $t0-$t9, $ra.
                    for r in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 24, 25, 31] {
                        held[r] = None;
                    }
                }
                if held.iter().all(Option::is_none) {
                    break;
                }
                pc += 4;
            }
        }
        out.sort_by_key(|a| (a.offset, a.site));
        out.dedup();
        out
    }

    /// The fields reached through the global, as [`crate::globals::GlobalField`] rows.
    pub(crate) fn mips_fields_through(&self, global: u64) -> Vec<crate::globals::GlobalField> {
        let mut fields: Vec<crate::globals::GlobalField> = Vec::new();
        for a in self.field_accesses(global) {
            if a.width == 0 || a.offset < 0 || a.offset >= 0x10000 {
                continue;
            }
            let mode = if a.store { "w" } else { "r" };
            match fields
                .iter_mut()
                .find(|f| f.offset == a.offset as u64 && f.width == a.width)
            {
                Some(f) if !f.access.contains(mode) => f.access = "rw".into(),
                Some(_) => {}
                None => fields.push(crate::globals::GlobalField {
                    offset: a.offset as u64,
                    width: a.width,
                    access: mode.into(),
                    float: false,
                }),
            }
        }
        fields.sort_by_key(|f| (f.offset, f.width));
        fields
    }

    /// The fields reached through a global pointer, or, with `offset`, the functions that use
    /// that field and how.
    pub fn field_refs_text(&self, global: u64, offset: Option<i64>) -> String {
        let accesses = self.field_accesses(global);
        let name = self
            .symbols()
            .at(global)
            .map_or_else(|| format!("{global:#x}"), |s| s.display_name().to_string());
        if accesses.is_empty() {
            return format!("Nothing is loaded or stored through a pointer read from {name} ({global:#x}).\n");
        }
        let function_of = |site: u64| {
            self.symbols()
                .function_containing(site)
                .map_or_else(|| (site, "?".to_string()), |f| (f.address, f.name().to_string()))
        };
        let width = |a: &FieldAccess| match a.width {
            0 => "address".to_string(),
            w => format!("u{}", w as u32 * 8),
        };
        let mut out = String::new();
        match offset {
            None => {
                out.push_str(&format!(
                    "Through the pointer in {name} ({global:#x}), {} accesses:\n",
                    accesses.len()
                ));
                let mut i = 0;
                while i < accesses.len() {
                    let o = accesses[i].offset;
                    let group: Vec<&FieldAccess> = accesses[i..].iter().take_while(|a| a.offset == o).collect();
                    i += group.len();
                    let loads = group.iter().filter(|a| a.width > 0 && !a.store).count();
                    let stores = group.iter().filter(|a| a.store).count();
                    let addrs = group.iter().filter(|a| a.width == 0).count();
                    let mut widths: Vec<String> = group.iter().map(|a| width(a)).collect();
                    widths.sort();
                    widths.dedup();
                    let mut functions: Vec<u64> = group.iter().map(|a| function_of(a.site).0).collect();
                    functions.sort_unstable();
                    functions.dedup();
                    let sign = if o < 0 { "-" } else { "+" };
                    out.push_str(&format!(
                        "  {sign}{:#x}  {:<12} {loads} loads, {stores} stores, {addrs} address-of, in {} functions\n",
                        o.unsigned_abs(),
                        widths.join("/"),
                        functions.len()
                    ));
                }
                out.push_str("An offset with fieldrefs lists the functions that use it.\n");
            }
            Some(o) => {
                let group: Vec<&FieldAccess> = accesses.iter().filter(|a| a.offset == o).collect();
                if group.is_empty() {
                    out.push_str(&format!("Nothing uses offset {o:#x} through {name} ({global:#x}).\n"));
                    return out;
                }
                out.push_str(&format!(
                    "{} uses of offset {o:#x} through the pointer in {name} ({global:#x}), by function:\n",
                    group.len()
                ));
                let mut by_function: Vec<(u64, String, Vec<&FieldAccess>)> = Vec::new();
                for a in group {
                    let (start, fname) = function_of(a.site);
                    match by_function.iter_mut().find(|f| f.0 == start) {
                        Some(f) => f.2.push(a),
                        None => by_function.push((start, fname, vec![a])),
                    }
                }
                by_function.sort_by_key(|f| f.0);
                for (start, fname, uses) in by_function {
                    let list: Vec<String> = uses
                        .iter()
                        .map(|a| {
                            let what = match (a.width, a.store) {
                                (0, _) => "address".to_string(),
                                (w, true) => format!("store u{}", w as u32 * 8),
                                (w, false) => format!("load u{}", w as u32 * 8),
                            };
                            format!("{what} at {:#x}", a.site)
                        })
                        .collect();
                    out.push_str(&format!("  {fname} ({start:#x}): {}\n", list.join(", ")));
                }
            }
        }
        out
    }
}
