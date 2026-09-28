//! Decoders for the CPUs of game consoles: the 6502 (NES) and 65816 (SNES),
//! the Game Boy's SM83, the 68000 (Mega Drive) and MIPS (Nintendo 64,
//! PlayStation). Each decodes one instruction at a time into text and what
//! the instruction does to the flow of control and to memory, with every
//! address as the CPU sees it; placing those addresses in the ROM (which
//! bank?) is up to the caller.

pub(crate) mod arm;
pub(crate) mod m65;
pub(crate) mod m68k;
pub(crate) mod mips;
pub(crate) mod sm83;

use serde::Serialize;

use crate::model::FlowKind;
use crate::xrefs::RefKind;

/// A console CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cpu {
    /// MOS 6502 (the NES's Ricoh 2A03).
    Mos6502,
    /// WDC 65C816 (the SNES's Ricoh 5A22).
    W65816,
    /// Sharp SM83 (the Game Boy's).
    Sm83,
    /// Motorola 68000 (the Mega Drive's).
    M68000,
    /// MIPS R4300i (the Nintendo 64's), big-endian.
    MipsR4300,
    /// MIPS R3000A (the PlayStation's), little-endian.
    MipsR3000,
    /// ARM7TDMI (the Game Boy Advance's): ARM and Thumb.
    Arm7Tdmi,
}

impl Cpu {
    pub fn name(self) -> &'static str {
        match self {
            Cpu::Mos6502 => "6502",
            Cpu::W65816 => "65816",
            Cpu::Sm83 => "SM83",
            Cpu::M68000 => "68000",
            Cpu::MipsR4300 => "MIPS R4300i",
            Cpu::MipsR3000 => "MIPS R3000A",
            Cpu::Arm7Tdmi => "ARM7TDMI",
        }
    }
}

/// Where control goes after an instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flow {
    /// On to the next instruction.
    Next,
    /// A jump; `None` when the target is computed (through a register or a table).
    Jump(Option<u64>),
    /// A conditional branch: to the target or on to the next instruction.
    Branch(u64),
    /// A call, which comes back to the next instruction.
    Call(Option<u64>),
    /// A return, or the end of an interrupt handler.
    Return,
    /// A conditional return: back to the caller or on to the next instruction.
    CondReturn,
    /// A software interrupt (`brk`, `cop`, `trap`): where it resumes, if at
    /// all, is up to the handler.
    Trap,
    /// Execution stops here (an invalid opcode, a halt that never resumes).
    Stop,
}

impl Flow {
    pub fn kind(self) -> FlowKind {
        match self {
            Flow::Next => FlowKind::Normal,
            Flow::Jump(_) => FlowKind::Jump,
            Flow::Branch(_) => FlowKind::CondJump,
            Flow::Call(_) => FlowKind::Call,
            Flow::Return => FlowKind::Return,
            Flow::CondReturn => FlowKind::CondJump,
            Flow::Trap => FlowKind::Interrupt,
            Flow::Stop => FlowKind::Invalid,
        }
    }

    pub fn target(self) -> Option<u64> {
        match self {
            Flow::Jump(t) | Flow::Call(t) => t,
            Flow::Branch(t) => Some(t),
            _ => None,
        }
    }
}

/// One decoded instruction.
#[derive(Debug, Clone)]
pub(crate) struct Insn {
    pub len: u32,
    pub mnemonic: String,
    pub operands: String,
    pub flow: Flow,
    /// Memory an operand names, as the CPU sees it, and how it is used.
    pub data: Option<(u64, RefKind)>,
    /// MIPS: the next instruction runs before a jump or branch takes effect.
    pub delay_slot: bool,
}

impl Insn {
    pub fn new(len: u32, mnemonic: &str, operands: String, flow: Flow) -> Insn {
        Insn {
            len,
            mnemonic: mnemonic.to_string(),
            operands,
            flow,
            data: None,
            delay_slot: false,
        }
    }

    /// Bytes that don't decode: shown as data, and the end of any path through them.
    pub fn bad(len: u32, bytes: &[u8]) -> Insn {
        let text: Vec<String> = bytes.iter().take(len as usize).map(|b| format!("${b:02X}")).collect();
        Insn::new(len, ".db", text.join(", "), Flow::Stop)
    }

    pub fn with_data(mut self, address: u64, kind: RefKind) -> Insn {
        self.data = Some((address, kind));
        self
    }
}

/// CPU state that changes how instructions decode or what they refer to:
/// the 65816's register widths, the MIPS registers known to hold addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct State {
    /// 65816: the accumulator is 8 bits wide (the M flag).
    pub m8: bool,
    /// 65816: the index registers are 8 bits wide (the X flag).
    pub x8: bool,
    /// MIPS and ARM: register values, where known (bit n of `known` for register n).
    pub regs: [u32; 32],
    pub known: u32,
    /// ARM7TDMI: running Thumb code.
    pub thumb: bool,
    /// ARM7TDMI: the link register was just set to return here (`mov lr, pc`):
    /// the `bx` that follows is a call.
    pub link: bool,
}

impl State {
    /// How a 65816 starts, and runs until told otherwise: emulation mode, 8-bit registers.
    pub const RESET: State = State {
        m8: true,
        x8: true,
        regs: [0; 32],
        known: 0,
        thumb: false,
        link: false,
    };

    /// MIPS: code starting with `$gp` (and nothing else) known.
    pub fn with_gp(gp: u32) -> State {
        let mut s = State::default();
        s.regs[28] = gp;
        s.known = 1 << 28;
        s
    }
}

/// Whether there is a decoder for `cpu`.
pub(crate) fn supported(cpu: Cpu) -> bool {
    matches!(
        cpu,
        Cpu::Mos6502 | Cpu::W65816 | Cpu::Sm83 | Cpu::M68000 | Cpu::MipsR4300 | Cpu::MipsR3000 | Cpu::Arm7Tdmi
    )
}

/// Decodes the instruction at the start of `bytes`, which the CPU sees at `pc`.
pub(crate) fn decode(cpu: Cpu, bytes: &[u8], pc: u64, state: &mut State) -> Option<Insn> {
    if bytes.is_empty() {
        return None;
    }
    match cpu {
        Cpu::Mos6502 => Some(m65::decode(bytes, pc, None)),
        Cpu::W65816 => Some(m65::decode(bytes, pc, Some(state))),
        Cpu::Sm83 => Some(sm83::decode(bytes, pc)),
        Cpu::MipsR4300 => Some(mips::decode(bytes, pc, state, true)),
        Cpu::MipsR3000 => Some(mips::decode(bytes, pc, state, false)),
        Cpu::Arm7Tdmi => Some(arm::decode(bytes, pc, state)),
        Cpu::M68000 => Some(m68k::decode(bytes, pc, state)),
    }
}

/// Where a code target is, and for the ARM7TDMI whether it is Thumb code
/// (bit 0 of the target says so).
pub(crate) fn code_target(cpu: Cpu, t: u64) -> (u64, Option<bool>) {
    match cpu {
        Cpu::Arm7Tdmi => (t & !1, Some(t & 1 == 1)),
        _ => (t, None),
    }
}

/// `$1F`, `$0200`: numbers as 8-bit assemblers write them.
pub(crate) fn hex(v: u64, digits: usize) -> String {
    format!("${v:0digits$X}")
}
