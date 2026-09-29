//! Disassembly: iced-x86 for x86/x86-64, yaxpeax-arm for AArch64 and ARM,
//! and binviz's own decoders for game consoles' CPUs (see [`crate::cpu`]).

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
    /// The function's pieces away from its entry, whose instructions follow
    /// the entry's: (start, end).
    pub parts: Vec<(u64, u64)>,
    /// Lines to show before instructions, in address order: where a piece of
    /// the function starts, a switch's cases, a jump table in the code.
    pub marks: Vec<Mark>,
}

/// A line shown before the instruction at `address`: `its piece away from
/// the entry`, `cases 1, 4:`, `default:`, `jump table of the switch at …`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mark {
    pub address: u64,
    pub text: String,
}

impl Disassembly {
    /// Ends the block where its code does: the `nop`s and `int3`s that pad a
    /// function out to the next one's alignment (an export's extent, cut at
    /// the next symbol, takes them in) are not part of it. With
    /// `delay_slots` (MIPS), a `nop` after a jump or branch is its delay
    /// slot, and stays.
    fn drop_padding(&mut self, delay_slots: bool) {
        if self.truncated {
            return;
        }
        while let [.., before, last] = self.instructions.as_slice()
            && matches!(last.mnemonic.as_str(), "nop" | "int3")
            && matches!(last.flow, FlowKind::Normal | FlowKind::Interrupt)
            && !(delay_slots && before.flow != FlowKind::Normal)
        {
            self.instructions.pop();
        }
        if let Some(last) = self.instructions.last() {
            self.end = self.end.min(last.address + last.len as u64);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Isa {
    X86(u32),
    A64,
    Arm,
    /// A game ROM's CPU.
    Rom,
    /// WebAssembly bytecode (see [`crate::wasm::code`]).
    Wasm,
    None,
}

fn isa(arch: Architecture) -> Isa {
    match arch {
        Architecture::X86_64 => Isa::X86(64),
        Architecture::X86_64_X32 | Architecture::I386 => Isa::X86(32),
        Architecture::Aarch64 | Architecture::Aarch64_Ilp32 => Isa::A64,
        Architecture::Arm => Isa::Arm,
        Architecture::Wasm32 | Architecture::Wasm64 => Isa::Wasm,
        _ => Isa::None,
    }
}

pub fn is_supported(arch: Architecture) -> bool {
    isa(arch) != Isa::None
}

impl Binary {
    fn isa(&self) -> Isa {
        if self.rom.as_ref().is_some_and(|r| crate::cpu::supported(r.cpu)) {
            return Isa::Rom;
        }
        if self.wasm.is_none() && isa(self.arch) == Isa::Wasm {
            return Isa::None;
        }
        isa(self.arch)
    }

    /// The value of a float or double an x86 instruction reads from an
    /// absolute address (`fld qword ptr [0x40e100]`, `mulss xmm0, [0x40e104]`):
    /// `f64 -1.0`. MSVC folds `x + 1` into `x - (-1)`, so the value is what
    /// makes that code readable.
    pub(crate) fn float_operand(&self, ins: &iced_x86::Instruction) -> Option<String> {
        use iced_x86::{MemorySize, OpKind, Register};
        if ins.is_invalid() || !(0..ins.op_count()).any(|i| ins.op_kind(i) == OpKind::Memory) {
            return None;
        }
        let address = if ins.is_ip_rel_memory_operand() {
            ins.ip_rel_memory_address()
        } else if ins.memory_base() == Register::None && ins.memory_index() == Register::None {
            ins.memory_displacement64()
        } else {
            return None;
        };
        let offset = self.address_to_offset(address)? as usize;
        match ins.memory_size() {
            MemorySize::Float32 => {
                let b = self.data.get(offset..offset + 4)?;
                Some(format!("f32 {:?}", f32::from_le_bytes(b.try_into().ok()?)))
            }
            MemorySize::Float64 => {
                let b = self.data.get(offset..offset + 8)?;
                Some(format!("f64 {:?}", f64::from_le_bytes(b.try_into().ok()?)))
            }
            _ => None,
        }
    }

    /// The import slot a 32-bit instruction names by its address, with no
    /// register (`call dword ptr [0x4021b4]`; `mov eax, [slot]` for a kernel
    /// variable an XBE imports): 64-bit code names its slots RIP-relative,
    /// which the instruction's target covers already.
    fn import_slot_operand(&self, ins: &iced_x86::Instruction) -> Option<u64> {
        use iced_x86::{OpKind, Register};
        if !(0..ins.op_count()).any(|i| ins.op_kind(i) == OpKind::Memory)
            || ins.memory_base() != Register::None
            || ins.memory_index() != Register::None
        {
            return None;
        }
        let slot = ins.memory_displacement64();
        self.symbols
            .at(slot)
            .is_some_and(|s| s.source == crate::model::SymbolSource::Import && s.kind == crate::model::SymbolKind::Data)
            .then_some(slot)
    }

    /// [`Self::float_operand`] for the instruction at `site`.
    pub(crate) fn float_operand_at(&self, site: u64) -> Option<String> {
        let Isa::X86(bits) = self.isa() else { return None };
        let offset = self.address_to_offset(site)? as usize;
        let bytes = self.data.get(offset..(offset + 16).min(self.data.len()))?;
        let mut decoder = iced_x86::Decoder::with_ip(bits, bytes, site, iced_x86::DecoderOptions::NONE);
        self.float_operand(&decoder.decode())
    }

    /// Whether this binary's code can be disassembled.
    pub fn can_disassemble(&self) -> bool {
        self.isa() != Isa::None
    }

    /// Disassembles `start..end` (at most `limit` instructions).
    pub fn disassemble(&self, start: u64, end: u64, limit: usize) -> Disassembly {
        let mut d = self.disassemble_range(start, end, limit);
        d.marks = self.switch_marks(&d.instructions);
        d
    }

    /// Disassembles `start..end` (at most `limit` instructions), without marks.
    fn disassemble_range(&self, start: u64, end: u64, limit: usize) -> Disassembly {
        let isa = self.isa();
        let mut out = Disassembly {
            start,
            end,
            function: self.symbols.lookup(start),
            instructions: Vec::new(),
            truncated: false,
            supported: isa != Isa::None,
            parts: Vec::new(),
            marks: Vec::new(),
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
                // The function's stack frame names what `[esp+N]` and `[ebp-N]` hold,
                // and the fields reached through the pointers it knows the types of.
                let function = self.symbols.function_containing(start).map(|f| f.address);
                let frame = function.and_then(|f| self.stack_frame(f));
                let fields = match (function, &frame) {
                    (Some(f), Some(frame)) => self.field_labels(f, frame),
                    _ => Default::default(),
                };
                // An image loaded well above zero: numbers in it that fall inside it are its addresses
                // (`push offset string`, `mov eax, [global]`), as the reference index takes them.
                let absolute = self.summary.format == crate::model::Format::Xbe
                    || self
                        .sections
                        .iter()
                        .filter(|s| s.loaded && s.size > 0)
                        .map(|s| s.address)
                        .min()
                        .is_some_and(|lo| lo >= 0x10_0000);
                let inside = |a: u64| self.section_at(a).is_some();
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
                    // A jump table in the code: its entries, not instructions.
                    if let Some(row) = self.table_row(start + pos as u64, &bytes[pos..]) {
                        let (address, len) = (start + pos as u64, row.len);
                        out.instructions.push(Instruction {
                            address,
                            offset: Some(offset + pos as u64),
                            len: len as u32,
                            bytes: util::hex_bytes(&bytes[pos..pos + len]),
                            mnemonic: row.mnemonic.into(),
                            operands: row.operands,
                            flow: FlowKind::Normal,
                            target: row.target,
                            target_symbol: row.target.and_then(|t| self.symbol_name(t)),
                            source: source_for(address),
                        });
                        if decoder.set_position(pos + len).is_err() {
                            break;
                        }
                        decoder.set_ip(address + len as u64);
                        continue;
                    }
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
                    let memory = !invalid && (0..ins.op_count()).any(|i| ins.op_kind(i) == iced_x86::OpKind::Memory);
                    let immediate = (0..ins.op_count())
                        .filter(|&i| {
                            matches!(
                                ins.op_kind(i),
                                iced_x86::OpKind::Immediate32 | iced_x86::OpKind::Immediate32to64 | iced_x86::OpKind::Immediate64
                            )
                        })
                        .map(|i| ins.immediate(i))
                        .find(|&v| inside(v));
                    let (target, data) = if !invalid && ins.near_branch_target() != 0 {
                        (Some(ins.near_branch_target()), false)
                    } else if !invalid && ins.is_ip_rel_memory_operand() {
                        (Some(ins.ip_rel_memory_address()), true)
                    } else if !invalid
                        && bits == 32
                        && let Some(slot) = self.import_slot_operand(&ins)
                    {
                        (Some(slot), true)
                    } else if absolute
                        && memory
                        && ins.memory_base() == iced_x86::Register::None
                        && inside(ins.memory_displacement64())
                    {
                        (Some(ins.memory_displacement64()), true)
                    } else if absolute && !invalid && immediate.is_some() {
                        (immediate, true)
                    } else {
                        (None, false)
                    };
                    let address = ins.ip();
                    let value = self.float_operand(&ins);
                    out.instructions.push(Instruction {
                        address,
                        offset: Some(offset + pos as u64),
                        len: len as u32,
                        bytes: util::hex_bytes(&bytes[pos..pos + len]),
                        mnemonic,
                        operands,
                        flow,
                        target_symbol: {
                            // MSVC's x64 code takes the image base to read its jump tables with.
                            let base = |t: u64| {
                                (t == self.image_base && self.summary.format == crate::model::Format::Pe && bits == 64)
                                    .then(|| "__ImageBase".to_string())
                            };
                            let name = |t: u64| if data { self.name_for(t).or_else(|| base(t)) } else { self.symbol_name(t) };
                            let named = match (target.and_then(name), value) {
                                (Some(name), Some(value)) => Some(format!("{name} = {value}")),
                                (name, value) => name.or(value),
                            };
                            // A stack slot or a structure's field, and what is stored in it or read with it.
                            let place = frame.as_ref().and_then(|f| f.slot_at(address)).or_else(|| fields.get(&address).cloned());
                            match (place, named) {
                                (Some(place), Some(named)) => Some(format!("{place}; {named}")),
                                (place, named) => place.or(named),
                            }
                        },
                        target,
                        source: source_for(address),
                    });
                }
            }
            Isa::A64 => {
                let decoder = yaxpeax_arm::armv8::a64::InstDecoder::default();
                // Registers holding a page address from `adrp`.
                let mut pages = [None::<u64>; 32];
                let mut pos = 0;
                while pos + 4 <= bytes.len() {
                    if out.instructions.len() >= limit {
                        out.truncated = true;
                        break;
                    }
                    let address = start + pos as u64;
                    let word = &bytes[pos..pos + 4];
                    let mut reader = yaxpeax_arch::U8Reader::new(word);
                    let (mnemonic, operands, flow, mut target) = match decoder.decode(&mut reader) {
                        Ok(ins) => a64_parts(&ins, address),
                        Err(_) => (
                            ".word".to_string(),
                            format!("{:#010x}", u32::from_le_bytes(word.try_into().unwrap_or([0; 4]))),
                            FlowKind::Invalid,
                            None,
                        ),
                    };
                    let w = u32::from_le_bytes(word.try_into().unwrap_or([0; 4]));
                    let mut data = matches!(flow, FlowKind::Normal);
                    if w & 0x9F00_0000 == 0x9000_0000 {
                        // adrp: the page alone names nothing.
                        pages[(w & 31) as usize] = target;
                        data = false;
                    } else {
                        let page = pages[((w >> 5) & 31) as usize];
                        let ldst = crate::xrefs::a64_ldst(w);
                        if let Some(page) = page {
                            if w & 0xFF80_0000 == 0x9100_0000 {
                                target = Some(page + ((((w >> 10) & 0xFFF) as u64) << (12 * ((w >> 22) & 1))));
                            } else if let Some((scale, _)) = ldst {
                                target = Some(page + ((w >> 10) & 0xFFF) as u64 * scale);
                            }
                        }
                        // The destination register no longer holds a page (stores only read theirs).
                        if !ldst.is_some_and(|(_, store)| store) {
                            pages[(w & 31) as usize] = None;
                        }
                        if flow == FlowKind::Call {
                            pages[..19].fill(None);
                        }
                    }
                    let target_symbol = match target {
                        Some(t) if data => self.name_for(t),
                        Some(t) if w & 0x9F00_0000 != 0x9000_0000 => self.symbol_name(t),
                        _ => None,
                    };
                    out.instructions.push(Instruction {
                        address,
                        offset: Some(offset + pos as u64),
                        len: 4,
                        bytes: util::hex_bytes(word),
                        mnemonic,
                        operands,
                        flow,
                        target_symbol,
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
            Isa::Rom => {
                let rom = self.rom.as_ref().expect("a ROM");
                let mut state = self.rom_state_at(start);
                // Decoding sees past the range: literal pools follow their functions.
                let tail = self.code_bytes(start).unwrap_or(bytes);
                let mut pos = 0;
                while pos < bytes.len() {
                    if out.instructions.len() >= limit {
                        out.truncated = true;
                        break;
                    }
                    let address = start + pos as u64;
                    let Some(insn) = crate::cpu::decode(rom.cpu, &tail[pos..], rom.map.cpu(address), &mut state) else {
                        break;
                    };
                    let len = (insn.len as usize).clamp(1, bytes.len() - pos);
                    let code = insn
                        .flow
                        .target()
                        .and_then(|t| rom.map.resolve(address, crate::cpu::code_target(rom.cpu, t).0));
                    let data = insn.data.and_then(|(t, _)| rom.map.resolve(address, t));
                    out.instructions.push(Instruction {
                        address,
                        offset: Some(offset + pos as u64),
                        len: len as u32,
                        bytes: util::hex_bytes(&bytes[pos..pos + len]),
                        flow: insn.flow.kind(),
                        target: code.or(data),
                        target_symbol: match (code, data) {
                            (Some(t), _) => self.symbol_name(t),
                            (None, Some(t)) => self.name_for(t),
                            _ => None,
                        },
                        mnemonic: insn.mnemonic,
                        operands: insn.operands,
                        source: source_for(address),
                    });
                    pos += len;
                }
            }
            Isa::Wasm => {
                let (instructions, truncated) = crate::wasm::analysis::disassemble(self, start, start + avail, limit);
                for mut i in instructions {
                    i.source = source_for(i.address);
                    out.instructions.push(i);
                }
                out.truncated = truncated;
            }
            Isa::None => {}
        }
        out.end = out.instructions.last().map_or(start, |i| i.address + i.len as u64);
        out
    }

    /// A token per instruction in `start..end` (at most `limit`): what the
    /// instruction does, its operation and the kinds of its operands, and not
    /// where things are, so that the same code at another address reads the
    /// same. Alignment padding at the end (nops, `int3`, zeros) is left out;
    /// also returns how many bytes the instructions before it take.
    pub(crate) fn instruction_tokens(&self, start: u64, end: u64, limit: usize) -> (Vec<u32>, u64) {
        use std::hash::{Hash, Hasher};
        let token = |f: &dyn Fn(&mut std::collections::hash_map::DefaultHasher)| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            f(&mut h);
            h.finish() as u32
        };
        let Some(offset) = self.address_to_offset(start) else {
            return (Vec::new(), 0);
        };
        let mut avail = end.saturating_sub(start);
        if let Some(sec) = self.section_at(start)
            && let Some(sec_off) = sec.file_offset
        {
            avail = avail.min((sec_off + sec.file_size).saturating_sub(offset));
        }
        avail = avail.min(self.data.len() as u64 - offset.min(self.data.len() as u64));
        let bytes = &self.data[offset as usize..(offset + avail) as usize];
        let mut out = Vec::new();
        // Past the last instruction that isn't padding: (tokens, bytes).
        let mut real = (0, 0u64);
        let zeros = |b: &[u8]| b.iter().all(|&x| x == 0);
        match self.isa() {
            Isa::X86(bits) => {
                let mut decoder = iced_x86::Decoder::with_ip(bits, bytes, start, iced_x86::DecoderOptions::NONE);
                let mut ins = iced_x86::Instruction::default();
                while decoder.can_decode() && out.len() < limit {
                    let pos = decoder.position();
                    // A jump table: a token per entry, whatever its address.
                    if let Some(row) = self.table_row(start + pos as u64, &bytes[pos..]) {
                        out.push(token(&|h| row.mnemonic.hash(h)));
                        real = (out.len(), (pos + row.len) as u64);
                        if decoder.set_position(pos + row.len).is_err() {
                            break;
                        }
                        decoder.set_ip(start + (pos + row.len) as u64);
                        continue;
                    }
                    decoder.decode_out(&mut ins);
                    out.push(token(&|h| {
                        (ins.mnemonic() as u32).hash(h);
                        for i in 0..ins.op_count() {
                            (ins.op_kind(i) as u32).hash(h);
                        }
                    }));
                    let pad = matches!(ins.mnemonic(), iced_x86::Mnemonic::Nop | iced_x86::Mnemonic::Int3)
                        || zeros(&bytes[pos..pos + ins.len()]);
                    if !pad {
                        real = (out.len(), (pos + ins.len()) as u64);
                    }
                }
            }
            Isa::A64 => {
                let decoder = yaxpeax_arm::armv8::a64::InstDecoder::default();
                for (n, word) in bytes.as_chunks::<4>().0.iter().take(limit).enumerate() {
                    let mut reader = yaxpeax_arch::U8Reader::new(word);
                    out.push(match decoder.decode(&mut reader) {
                        Ok(ins) => token(&|h| {
                            std::mem::discriminant(&ins.opcode).hash(h);
                            for op in &ins.operands {
                                std::mem::discriminant(op).hash(h);
                            }
                        }),
                        Err(_) => token(&|h| u32::from_le_bytes(*word).hash(h)),
                    });
                    let w = u32::from_le_bytes(*word);
                    if w != 0xD503_201F && w != 0 {
                        real = (out.len(), (n as u64 + 1) * 4);
                    }
                }
            }
            Isa::Arm => {
                let decoder = yaxpeax_arm::armv7::InstDecoder::default();
                let mut pos = 0;
                while pos + 4 <= bytes.len() && out.len() < limit {
                    let mut reader = yaxpeax_arch::U8Reader::new(&bytes[pos..]);
                    let len = match decoder.decode(&mut reader) {
                        Ok(ins) => {
                            out.push(token(&|h| {
                                std::mem::discriminant(&ins.opcode).hash(h);
                                for op in &ins.operands {
                                    std::mem::discriminant(op).hash(h);
                                }
                            }));
                            (ins.len().to_const() as usize).max(2)
                        }
                        Err(_) => {
                            out.push(token(&|h| bytes[pos..pos + 4].hash(h)));
                            4
                        }
                    };
                    let w = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
                    if !matches!(w, 0 | 0xE320_F000 | 0xE1A0_0000) {
                        real = (out.len(), (pos + len) as u64);
                    }
                    pos += len;
                }
            }
            Isa::Rom => {
                let rom = self.rom.as_ref().expect("a ROM");
                let mut state = self.rom_state_at(start);
                let tail = self.code_bytes(start).unwrap_or(bytes);
                let mut pos = 0;
                while pos < bytes.len() && out.len() < limit {
                    let address = start + pos as u64;
                    let Some(insn) = crate::cpu::decode(rom.cpu, &tail[pos..], rom.map.cpu(address), &mut state) else {
                        break;
                    };
                    // The addressing mode without its numbers: `lda $2000,x` reads `lda $N,x`.
                    let shape = operand_shape(&insn.operands);
                    out.push(token(&|h| {
                        insn.mnemonic.hash(h);
                        shape.hash(h);
                    }));
                    pos += (insn.len as usize).max(1);
                    real = (out.len(), pos.min(bytes.len()) as u64);
                }
            }
            Isa::Wasm => {
                let (tokens, len) = crate::wasm::analysis::tokens(self, start, start + bytes.len() as u64, limit);
                real = (tokens.len(), len);
                out = tokens;
            }
            Isa::None => return (Vec::new(), bytes.len() as u64),
        }
        out.truncate(real.0);
        (out, real.1)
    }

    /// Disassembles the function containing `address`: its symbol if there is
    /// one, else a window starting at the closest known instruction boundary.
    pub fn disassemble_function(&self, address: u64, limit: usize) -> Disassembly {
        // A piece of a function away from its entry is disassembled with its function.
        let address = self.symbols.part_owner(address).unwrap_or(address);
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
            let delay_slots = self.mips_endian().is_some();
            let mut d = self.disassemble_range(sym.address, end, limit);
            d.drop_padding(delay_slots);
            // Then its pieces away from its entry.
            for (ps, pe) in self.symbols.parts_of(sym.address) {
                if d.truncated {
                    break;
                }
                let mut p = self.disassemble_range(ps, pe, limit.saturating_sub(d.instructions.len()));
                p.drop_padding(delay_slots);
                d.truncated |= p.truncated;
                d.instructions.extend(p.instructions);
                d.parts.push((ps, pe));
                d.marks.push(Mark {
                    address: ps,
                    text: format!("its piece away from the entry, {ps:#x}..{pe:#x}:"),
                });
            }
            d.marks.extend(self.switch_marks(&d.instructions));
            // In the order the instructions come, a piece's marks after the entry's.
            let order: std::collections::HashMap<u64, usize> =
                d.instructions.iter().enumerate().map(|(i, x)| (x.address, i)).collect();
            d.marks.sort_by_key(|m| order.get(&m.address).copied().unwrap_or(usize::MAX));
            return d;
        }
        let start = self.instruction_boundary_before(address);
        let end = self.section_at(address).map_or(address + 0x400, |s| s.address + s.size);
        self.disassemble(start, end.min(start + 0x4000), limit)
    }

    /// The instruction covering `address`, if it is in code.
    pub fn instruction_at(&self, address: u64) -> Option<Instruction> {
        if !self.can_disassemble() {
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
        match self.isa() {
            Isa::A64 | Isa::Arm => address & !3,
            // Where the function it is in starts (the analysis found those).
            Isa::Rom => self.symbols.function_containing(address).map_or(address, |f| f.address),
            // A body decodes only from its start.
            Isa::Wasm => crate::wasm::analysis::boundary_before(self, address),
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

    /// The jump table (or index table) of the code at `address`, if one is there.
    pub(crate) fn code_table_at(&self, address: u64) -> Option<&crate::discover::x86::Table> {
        let i = self
            .code_tables
            .partition_point(|t| t.address <= address)
            .checked_sub(1)?;
        let t = &self.code_tables[i];
        (address < t.end()).then_some(t)
    }

    /// A row of a jump table at `address` (with its bytes from there): an
    /// address, or up to 8 of the bytes an index table picks entries with.
    fn table_row(&self, address: u64, bytes: &[u8]) -> Option<TableRow> {
        let t = self.code_table_at(address)?;
        let size = t.entry as usize;
        let into = (address - t.address) as usize % size;
        if !t.is_index() && into == 0 && bytes.len() >= size {
            let v = match size {
                8 => u64::from_le_bytes(bytes[..8].try_into().expect("8 bytes")),
                _ => u32::from_le_bytes(bytes[..4].try_into().expect("4 bytes")) as u64,
            };
            return Some(TableRow {
                len: size,
                mnemonic: if size == 8 { "dq" } else { "dd" },
                operands: format!("{v:#x}"),
                target: Some(t.target(v)),
            });
        }
        // Index bytes (or what is left of an entry a disassembly starts in).
        let n = ((t.end() - address) as usize)
            .min(if t.is_index() { 8 } else { size - into })
            .min(bytes.len());
        (n > 0).then(|| TableRow {
            len: n,
            mnemonic: "db",
            operands: bytes[..n].iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", "),
            target: None,
        })
    }

    fn symbol_name(&self, address: u64) -> Option<String> {
        if let Some(name) = self.part_name(address) {
            return Some(name);
        }
        let r = self.symbols.lookup(address)?;
        let name = r.demangled.as_deref().unwrap_or(&r.name);
        Some(if r.offset == 0 {
            name.to_string()
        } else {
            format!("{name}+{:#x}", r.offset)
        })
    }
}

impl Binary {
    /// The name of an address in a piece of a function away from its entry:
    /// `find_char.part+0x2`, after the function it belongs to.
    pub(crate) fn part_name(&self, address: u64) -> Option<String> {
        let (start, _, owner) = self.symbols.part_at(address)?;
        let name = self
            .symbols
            .at(owner)
            .map_or_else(|| format!("sub_{owner:x}"), |s| s.display_name().into_owned());
        Some(if address == start {
            format!("{name}.part")
        } else {
            format!("{name}.part+{:#x}", address - start)
        })
    }
}

/// A row of a jump table, shown in place of instructions.
struct TableRow {
    len: usize,
    mnemonic: &'static str,
    operands: String,
    target: Option<u64>,
}

/// Operands with their numbers (`$2000`, `#$3f`, `0x1c`) read as `N`: the shape of an addressing mode.
pub(crate) fn operand_shape(operands: &str) -> String {
    let mut out = String::with_capacity(operands.len());
    let mut chars = operands.chars().peekable();
    while let Some(c) = chars.next() {
        let number = match c {
            '$' => true,
            '0' if chars.peek() == Some(&'x') => {
                chars.next();
                true
            }
            c if c.is_ascii_digit() && !out.ends_with(|p: char| p.is_ascii_alphanumeric()) => {
                out.push('N');
                while chars.peek().is_some_and(|d| d.is_ascii_hexdigit()) {
                    chars.next();
                }
                continue;
            }
            _ => false,
        };
        if number {
            out.push(if c == '$' { '$' } else { '0' });
            out.push('N');
            while chars.peek().is_some_and(|d| d.is_ascii_hexdigit()) {
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
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
