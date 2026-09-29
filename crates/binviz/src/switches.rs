//! A switch's cases in x86 code: the values that lead to each piece of code
//! a jump through a table reaches, for the disassembly and the stack walk.
//! Following a PE's code finds its switches; elsewhere (an ELF or Mach-O
//! binary lists its functions, so nothing follows its code) each one is read
//! from its function's code when asked for.

use std::collections::{BTreeMap, HashSet};

use iced_x86::{Decoder, DecoderOptions, FlowControl, Instruction};

use crate::binary::Binary;
use crate::disasm::Mark;
use crate::discover::x86::{Switch, SwitchCode, Table, read_switch};
use crate::model::{FlowKind, Format, RegionKind};

/// The code of one function, decoded in a line from its start.
struct Linear<'a> {
    bin: &'a Binary,
    code: Vec<Instruction>,
}

impl SwitchCode for Linear<'_> {
    fn instruction_before(&mut self, pc: u64) -> Option<Instruction> {
        let i = self.code.partition_point(|x| x.ip() < pc).checked_sub(1)?;
        let before = self.code[i];
        (before.next_ip() == pc).then_some(before)
    }

    fn bytes(&self, a: u64, n: usize) -> Option<&[u8]> {
        let offset = self.bin.address_to_offset(a)? as usize;
        self.bin.data.get(offset..offset.checked_add(n)?)
    }

    fn in_code(&self, a: u64) -> bool {
        self.bin.section_at(a).is_some_and(|s| s.kind == RegionKind::Code)
    }

    fn table_ends(&self, a: u64, _addresses: bool) -> bool {
        self.code.binary_search_by_key(&a, |x| x.ip()).is_ok()
    }

    fn image_base(&self) -> Option<u64> {
        (self.bin.summary.format == Format::Pe).then_some(self.bin.image_base)
    }
}

impl Binary {
    fn x86_bits(&self) -> Option<u32> {
        match self.arch {
            object::Architecture::I386 => Some(32),
            object::Architecture::X86_64 => Some(64),
            _ => None,
        }
    }

    /// The switch the indirect jump at `jump` makes: found when the code was
    /// followed, else read from the code of the function holding it.
    pub(crate) fn switch_at(&self, jump: u64) -> Option<Switch> {
        if let Ok(i) = self.code_switches.binary_search_by_key(&jump, |s| s.table.jump) {
            return Some(self.code_switches[i].clone());
        }
        if let Some(x) = self.xrefs.get()
            && let Ok(i) = x.switches.binary_search_by_key(&jump, |s| s.table.jump)
        {
            return Some(x.switches[i].clone());
        }
        let bits = self.x86_bits()?;
        let start = match self.symbols.part_at(jump) {
            Some(p) => p.0,
            None => self.symbols.function_containing(jump)?.address,
        };
        if jump < start || jump - start > 0x10000 {
            return None;
        }
        let offset = self.address_to_offset(start)? as usize;
        let end = (offset + (jump - start) as usize + 16).min(self.data.len());
        let mut decoder = Decoder::with_ip(bits, self.data.get(offset..end)?, start, DecoderOptions::NONE);
        let mut code = Vec::new();
        let mut ins = Instruction::default();
        while decoder.can_decode() && decoder.ip() <= jump {
            decoder.decode_out(&mut ins);
            code.push(ins);
        }
        let jmp = *code.last().filter(|i| i.ip() == jump)?;
        if jmp.flow_control() != FlowControl::IndirectBranch {
            return None;
        }
        read_switch(&mut Linear { bin: self, code }, bits, &jmp)
    }

    /// The jump table (or table of index bytes) starting at `address`: one
    /// following the code found, or (once references are indexed) one read
    /// from an indirect jump's code.
    pub(crate) fn jump_table_at(&self, address: u64) -> Option<Table> {
        if let Some(t) = self.code_tables.iter().find(|t| t.address == address) {
            return Some(*t);
        }
        self.xrefs
            .get()?
            .switches
            .iter()
            .flat_map(|s| std::iter::once(s.table).chain(s.index))
            .find(|t| t.address == address)
    }

    /// The jump table (or table of index bytes) holding `address`.
    pub(crate) fn jump_table_holding(&self, address: u64) -> Option<Table> {
        if let Some(t) = self.code_table_at(address) {
            return Some(*t);
        }
        self.xrefs
            .get()?
            .switches
            .iter()
            .flat_map(|s| std::iter::once(s.table).chain(s.index))
            .find(|t| (t.address..t.end()).contains(&address))
    }

    /// Each entry of a jump table and where it leads: (the entry's address, its target).
    pub(crate) fn table_entries(&self, t: &Table) -> Vec<(u64, u64)> {
        if t.is_index() {
            return Vec::new();
        }
        let size = t.entry as u64;
        (0..t.count as u64)
            .filter_map(|i| {
                let at = t.address + i * size;
                let offset = self.address_to_offset(at)? as usize;
                let raw = match *self.data.get(offset..offset + size as usize)? {
                    [a, b, c, d] => u32::from_le_bytes([a, b, c, d]) as u64,
                    ref w => u64::from_le_bytes(w.try_into().ok()?),
                };
                Some((at, t.target(raw)))
            })
            .collect()
    }

    /// Lines marking the switches in a function's x86 code: at each jump
    /// through a table, its cases and where the rest go; at the code of each
    /// case, the values that lead there (`cases 1, 4:`), and `default:`; at a
    /// table kept in the code, what it is.
    pub(crate) fn switch_marks(&self, instructions: &[crate::model::Instruction]) -> Vec<Mark> {
        let Some(bits) = self.x86_bits() else {
            return Vec::new();
        };
        let listed: HashSet<u64> = instructions.iter().map(|i| i.address).collect();
        let mut marks = Vec::new();
        for i in instructions.iter().filter(|i| i.flow == FlowKind::Jump) {
            if self.decode_x86(bits, i.address).map(|d| d.flow_control()) != Some(FlowControl::IndirectBranch) {
                continue;
            }
            let Some(s) = self.switch_at(i.address) else { continue };
            let t = &s.table;
            let through = match s.index {
                Some(x) => format!(
                    "through the bytes at {:#x}, which pick one of the {} entries of the table at {:#x}",
                    x.address, t.count, t.address
                ),
                None => format!("through the table at {:#x}", t.address),
            };
            let rest = s.default.map_or(String::new(), |d| format!("; the rest go to {d:#x}"));
            let cases = match s.cases.len() {
                0 => "switch".to_string(),
                1 => format!("switch: case {}", s.first),
                n => format!("switch: cases {}–{}", s.first, s.first + n as i64 - 1),
            };
            marks.push(Mark {
                address: i.address,
                text: format!("{cases} {through}{rest}"),
            });
            // The values leading to each case, leaving out those that go where the rest do.
            let mut values: BTreeMap<u64, Vec<i64>> = BTreeMap::new();
            for (k, &target) in s.cases.iter().enumerate() {
                if Some(target) != s.default {
                    values.entry(target).or_default().push(s.first + k as i64);
                }
            }
            for (target, v) in values {
                if listed.contains(&target) {
                    marks.push(Mark {
                        address: target,
                        text: format!("{}:", case_list(&v)),
                    });
                }
            }
            if let Some(d) = s.default.filter(|d| listed.contains(d)) {
                marks.push(Mark {
                    address: d,
                    text: "default:".into(),
                });
            }
            if listed.contains(&t.address) {
                marks.push(Mark {
                    address: t.address,
                    text: format!("jump table of the switch at {:#x}, {} entries:", i.address, t.count),
                });
            }
            if let Some(x) = s.index.filter(|x| listed.contains(&x.address)) {
                marks.push(Mark {
                    address: x.address,
                    text: format!(
                        "the entry each case of the switch at {:#x} takes, a byte each:",
                        i.address
                    ),
                });
            }
        }
        let mut seen = HashSet::new();
        marks.retain(|m| seen.insert((m.address, m.text.clone())));
        marks
    }
}

/// `case 3`, or `cases 0–2, 5` for several values, sorted.
fn case_list(values: &[i64]) -> String {
    let mut runs: Vec<(i64, i64)> = Vec::new();
    for &v in values {
        match runs.last_mut() {
            Some(r) if r.1 + 1 == v => r.1 = v,
            _ => runs.push((v, v)),
        }
    }
    let text: Vec<String> = runs
        .iter()
        .map(|&(a, b)| match b - a {
            0 => a.to_string(),
            1 => format!("{a}, {b}"),
            _ if a < 0 => format!("{a} to {b}"),
            _ => format!("{a}–{b}"),
        })
        .collect();
    let word = if values.len() == 1 { "case" } else { "cases" };
    format!("{word} {}", text.join(", "))
}

#[cfg(test)]
mod tests {
    use super::case_list;

    #[test]
    fn case_values_read_as_runs() {
        assert_eq!(case_list(&[3]), "case 3");
        assert_eq!(case_list(&[1, 2]), "cases 1, 2");
        assert_eq!(case_list(&[0, 1, 2, 5, 7, 8]), "cases 0–2, 5, 7, 8");
        assert_eq!(case_list(&[-2, -1, 0]), "cases -2 to 0");
    }
}
