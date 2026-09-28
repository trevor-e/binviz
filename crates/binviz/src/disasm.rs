//! Disassembly: iced-x86 for x86/x86-64, yaxpeax-arm for AArch64 and ARM.

use iced_x86::{FlowControl, Formatter};
use object::Architecture;
use serde::Serialize;
use yaxpeax_arch::{Decoder as _, LengthedInstruction as _};

use crate::binary::Binary;
use crate::model::{FlowKind, Instruction, SymbolRef};
use crate::util;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disassembly {
    pub start: u64,
    pub end: u64,
    /// The function (symbol) the block was taken from, if any.
    pub function: Option<SymbolRef>,
    pub instructions: Vec<Instruction>,
    /// Stopped at the instruction limit before reaching `end`.
    pub truncated: bool,
    /// False if the architecture has no disassembler (bytes are shown as data).
    pub supported: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Isa {
    X86(u32),
    A64,
    Arm,
    None,
}

fn isa(arch: Architecture) -> Isa {
    match arch {
        Architecture::X86_64 => Isa::X86(64),
        Architecture::X86_64_X32 | Architecture::I386 => Isa::X86(32),
        Architecture::Aarch64 | Architecture::Aarch64_Ilp32 => Isa::A64,
        Architecture::Arm => Isa::Arm,
        _ => Isa::None,
    }
}

pub fn is_supported(arch: Architecture) -> bool {
    isa(arch) != Isa::None
}

impl Binary {
    /// Disassembles `start..end` (at most `limit` instructions).
    pub fn disassemble(&self, start: u64, end: u64, limit: usize) -> Disassembly {
        let isa = isa(self.arch);
        let mut out = Disassembly {
            start,
            end,
            function: self.symbols.lookup(start),
            instructions: Vec::new(),
            truncated: false,
            supported: isa != Isa::None,
        };
        let Some(offset) = self.address_to_offset(start) else {
            return out;
        };
        // Stay within the file bytes of the segment/section that holds `start`.
        let mut avail = end.saturating_sub(start);
        if let Some(sec) = self.section_at(start)
            && let Some(sec_off) = sec.file_offset
        {
            let sec_end = sec_off + sec.file_size;
            avail = avail.min(sec_end.saturating_sub(offset));
        }
        avail = avail.min(self.data.len() as u64 - offset.min(self.data.len() as u64));
        let bytes = &self.data[offset as usize..(offset + avail) as usize];
        let locations = self
            .debug
            .as_ref()
            .map(|d| d.locations_in(start, start + avail))
            .unwrap_or_default();
        let mut loc_idx = 0;
        let mut source_for = |addr: u64| {
            while loc_idx < locations.len() && locations[loc_idx].1 <= addr {
                loc_idx += 1;
            }
            locations
                .get(loc_idx)
                .filter(|l| l.0 <= addr && addr < l.1)
                .map(|l| l.2.clone())
        };

        match isa {
            Isa::X86(bits) => {
                let mut decoder = iced_x86::Decoder::with_ip(bits, bytes, start, iced_x86::DecoderOptions::NONE);
                let mut formatter = iced_x86::IntelFormatter::new();
                let o = formatter.options_mut();
                o.set_hex_prefix("0x");
                o.set_hex_suffix("");
                o.set_uppercase_hex(false);
                o.set_space_after_operand_separator(true);
                o.set_branch_leading_zeros(false);
                let mut ins = iced_x86::Instruction::default();
                while decoder.can_decode() {
                    if out.instructions.len() >= limit {
                        out.truncated = true;
                        break;
                    }
                    let pos = decoder.position();
                    decoder.decode_out(&mut ins);
                    let len = ins.len();
                    let mut mnemonic = String::new();
                    let mut operands = String::new();
                    let invalid = ins.is_invalid();
                    if invalid {
                        mnemonic.push_str("(bad)");
                    } else {
                        formatter.format_mnemonic(&ins, &mut mnemonic);
                        formatter.format_all_operands(&ins, &mut operands);
                    }
                    let flow = if invalid {
                        FlowKind::Invalid
                    } else {
                        match ins.flow_control() {
                            FlowControl::Next | FlowControl::XbeginXabortXend => FlowKind::Normal,
                            FlowControl::UnconditionalBranch | FlowControl::IndirectBranch => FlowKind::Jump,
                            FlowControl::ConditionalBranch => FlowKind::CondJump,
                            FlowControl::Return => FlowKind::Return,
                            FlowControl::Call | FlowControl::IndirectCall => FlowKind::Call,
                            FlowControl::Interrupt | FlowControl::Exception => FlowKind::Interrupt,
                        }
                    };
                    let target = if !invalid && ins.near_branch_target() != 0 {
                        Some(ins.near_branch_target())
                    } else if !invalid && ins.is_ip_rel_memory_operand() {
                        Some(ins.ip_rel_memory_address())
                    } else {
                        None
                    };
                    let address = ins.ip();
                    out.instructions.push(Instruction {
                        address,
                        offset: Some(offset + pos as u64),
                        len: len as u32,
                        bytes: util::hex_bytes(&bytes[pos..pos + len]),
                        mnemonic,
                        operands,
                        flow,
                        target_symbol: target.and_then(|t| self.symbol_name(t)),
                        target,
                        source: source_for(address),
                    });
                }
            }
            Isa::A64 => {
                let decoder = yaxpeax_arm::armv8::a64::InstDecoder::default();
                let mut pos = 0;
                while pos + 4 <= bytes.len() {
                    if out.instructions.len() >= limit {
                        out.truncated = true;
                        break;
                    }
                    let address = start + pos as u64;
                    let word = &bytes[pos..pos + 4];
                    let mut reader = yaxpeax_arch::U8Reader::new(word);
                    let (mnemonic, operands, flow, target) = match decoder.decode(&mut reader) {
                        Ok(ins) => a64_parts(&ins, address),
                        Err(_) => (
                            ".word".to_string(),
                            format!("{:#010x}", u32::from_le_bytes(word.try_into().unwrap_or([0; 4]))),
                            FlowKind::Invalid,
                            None,
                        ),
                    };
                    out.instructions.push(Instruction {
                        address,
                        offset: Some(offset + pos as u64),
                        len: 4,
                        bytes: util::hex_bytes(word),
                        mnemonic,
                        operands,
                        flow,
                        target_symbol: target.and_then(|t| self.symbol_name(t)),
                        target,
                        source: source_for(address),
                    });
                    pos += 4;
                }
            }
            Isa::Arm => {
                let decoder = yaxpeax_arm::armv7::InstDecoder::default();
                let mut pos = 0;
                while pos + 4 <= bytes.len() {
                    if out.instructions.len() >= limit {
                        out.truncated = true;
                        break;
                    }
                    let address = start + pos as u64;
                    let mut reader = yaxpeax_arch::U8Reader::new(&bytes[pos..]);
                    let (text, len) = match decoder.decode(&mut reader) {
                        Ok(ins) => (ins.to_string(), ins.len().to_const() as usize),
                        Err(_) => (".word".to_string(), 4),
                    };
                    let (mnemonic, operands) = split_text(&text);
                    let len = len.max(2);
                    out.instructions.push(Instruction {
                        address,
                        offset: Some(offset + pos as u64),
                        len: len as u32,
                        bytes: util::hex_bytes(&bytes[pos..(pos + len).min(bytes.len())]),
                        mnemonic,
                        operands,
                        flow: FlowKind::Normal,
                        target: None,
                        target_symbol: None,
                        source: source_for(address),
                    });
                    pos += len;
                }
            }
            Isa::None => {}
        }
        out.end = out.instructions.last().map_or(start, |i| i.address + i.len as u64);
        out
    }

    /// Disassembles the function containing `address`: its symbol if there is
    /// one, else a window starting at the closest known instruction boundary.
    pub fn disassemble_function(&self, address: u64, limit: usize) -> Disassembly {
        if let Some(sym) = self.symbols.lookup(address)
            && sym.size > 0
            && let Some(s) = self.symbols.get(sym.index)
            && (s.kind == crate::model::SymbolKind::Function
                || self
                    .section_at(s.address)
                    .is_some_and(|sec| sec.kind == crate::model::RegionKind::Code))
        {
            let mut end = sym.address + sym.size;
            // Sizes inferred from the next symbol include alignment padding; DWARF knows better.
            if s.size_inferred
                && let Some((lo, hi)) = self.debug.as_ref().and_then(|d| d.function_range(sym.address))
                && lo == sym.address
                && hi > lo
            {
                end = end.min(hi);
            }
            return self.disassemble(sym.address, end, limit);
        }
        let start = self.instruction_boundary_before(address);
        let end = self.section_at(address).map_or(address + 0x400, |s| s.address + s.size);
        self.disassemble(start, end.min(start + 0x4000), limit)
    }

    /// The instruction covering `address`, if it is in code.
    pub fn instruction_at(&self, address: u64) -> Option<Instruction> {
        if !is_supported(self.arch) {
            return None;
        }
        let sec = self.section_at(address)?;
        if sec.kind != crate::model::RegionKind::Code {
            return None;
        }
        let start = match self.symbols.lookup(address) {
            Some(s) if address - s.address < 0x10000 => s.address,
            _ => self.instruction_boundary_before(address),
        };
        let block = self.disassemble(start, address + 16, 0x4000);
        block
            .instructions
            .into_iter()
            .find(|i| address >= i.address && address < i.address + i.len as u64)
    }

    /// A likely instruction boundary at or before `address`: a line table row
    /// start or, for fixed-width ISAs, the aligned address.
    fn instruction_boundary_before(&self, address: u64) -> u64 {
        match isa(self.arch) {
            Isa::A64 | Isa::Arm => address & !3,
            _ => {
                let from_lines = self.debug.as_ref().and_then(|d| {
                    let rows = d.locations_in(address.saturating_sub(0x1000), address + 1);
                    rows.iter().rev().map(|r| r.0).find(|&s| s <= address)
                });
                let floor = self.section_at(address).map_or(0, |s| s.address);
                from_lines.unwrap_or(address.saturating_sub(0x40).max(floor))
            }
        }
    }

    fn symbol_name(&self, address: u64) -> Option<String> {
        let r = self.symbols.lookup(address)?;
        let name = r.demangled.as_deref().unwrap_or(&r.name);
        Some(if r.offset == 0 {
            name.to_string()
        } else {
            format!("{name}+{:#x}", r.offset)
        })
    }
}

fn split_text(text: &str) -> (String, String) {
    match text.split_once(' ') {
        Some((m, rest)) => (m.to_string(), rest.trim().to_string()),
        None => (text.to_string(), String::new()),
    }
}

/// Splits a yaxpeax a64 instruction into mnemonic and operands, rewriting
/// PC-relative offsets (`$+0x10`) into absolute addresses.
fn a64_parts(ins: &yaxpeax_arm::armv8::a64::Instruction, address: u64) -> (String, String, FlowKind, Option<u64>) {
    use yaxpeax_arm::armv8::a64::{Opcode, Operand};
    let text = ins.to_string();
    let (mnemonic, operands) = split_text(&text);
    let mut target = None;
    for op in &ins.operands {
        if let Operand::PCOffset(off) = op {
            let base = if ins.opcode == Opcode::ADRP {
                address & !0xfff
            } else {
                address
            };
            target = Some(base.wrapping_add(*off as u64));
        }
    }
    let operands = match target {
        Some(t) => rewrite_pc_offset(&operands, t),
        None => operands,
    };
    let flow = match ins.opcode {
        Opcode::BL | Opcode::BLR | Opcode::BLRAA | Opcode::BLRAAZ | Opcode::BLRAB | Opcode::BLRABZ => FlowKind::Call,
        Opcode::B | Opcode::BR | Opcode::BRAA | Opcode::BRAAZ | Opcode::BRAB | Opcode::BRABZ => FlowKind::Jump,
        Opcode::Bcc(_) | Opcode::CBZ | Opcode::CBNZ | Opcode::TBZ | Opcode::TBNZ => FlowKind::CondJump,
        Opcode::RET | Opcode::RETAA | Opcode::RETAB | Opcode::ERET => FlowKind::Return,
        Opcode::SVC | Opcode::HVC | Opcode::SMC | Opcode::BRK | Opcode::HLT | Opcode::UDF => FlowKind::Interrupt,
        _ => FlowKind::Normal,
    };
    (mnemonic, operands, flow, target)
}

fn rewrite_pc_offset(operands: &str, target: u64) -> String {
    for prefix in ["$+0x", "$-0x", "$+", "$-"] {
        if let Some(pos) = operands.find(prefix) {
            let rest = &operands[pos + prefix.len()..];
            let len = rest.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(rest.len());
            return format!("{}{target:#x}{}", &operands[..pos], &rest[len..]);
        }
    }
    operands.to_string()
}
