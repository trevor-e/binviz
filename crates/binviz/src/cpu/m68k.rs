//! The Motorola 68000 (the Mega Drive's): 16-bit opcode words followed by
//! extension words for immediates, displacements and absolute addresses;
//! twelve addressing modes. Written in Motorola syntax (`move.w
//! #$8144,(a4)`, `lea ($FF0000).l,a0`).
//!
//! Address registers loaded with a known address (`lea`, `movea.l #`) are
//! tracked, so that `move.w d0,(a4)` names the register `a4` points at.

use super::{Flow, Insn, State};
use crate::xrefs::RefKind;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Size {
    B,
    W,
    L,
}

impl Size {
    fn suffix(self) -> &'static str {
        match self {
            Size::B => ".b",
            Size::W => ".w",
            Size::L => ".l",
        }
    }

    /// The two-bit size field used by most instructions.
    fn field(v: u16) -> Option<Size> {
        match v {
            0 => Some(Size::B),
            1 => Some(Size::W),
            2 => Some(Size::L),
            _ => None,
        }
    }
}

const COND: [&str; 16] = [
    "t", "f", "hi", "ls", "cc", "cs", "ne", "eq", "vc", "vs", "pl", "mi", "ge", "lt", "gt", "le",
];

fn areg(r: u16) -> &'static str {
    ["a0", "a1", "a2", "a3", "a4", "a5", "a6", "sp"][r as usize & 7]
}

fn hex(v: u32) -> String {
    if v < 10 { v.to_string() } else { format!("${v:X}") }
}

fn signed(v: i32) -> String {
    if v < 0 {
        format!("-{}", hex(v.unsigned_abs()))
    } else {
        hex(v as u32)
    }
}

/// What decoding an effective address gives: its text, the address it
/// names (when known), and whether it is a register.
struct Ea {
    text: String,
    address: Option<u32>,
}

struct Dec<'a> {
    bytes: &'a [u8],
    pc: u32,
    at: usize,
    state: &'a mut State,
}

impl Dec<'_> {
    fn word(&mut self) -> Option<u16> {
        let w = self.bytes.get(self.at..self.at + 2)?;
        self.at += 2;
        Some(u16::from_be_bytes([w[0], w[1]]))
    }

    fn long(&mut self) -> Option<u32> {
        Some((self.word()? as u32) << 16 | self.word()? as u32)
    }

    fn a(&self, r: u16) -> Option<u32> {
        let i = 8 + (r as u32 & 7);
        (self.state.known & (1 << i) != 0).then_some(self.state.regs[i as usize])
    }

    fn set_a(&mut self, r: u16, v: Option<u32>) {
        let i = 8 + (r as usize & 7);
        match v {
            Some(v) => {
                self.state.regs[i] = v & 0xFF_FFFF;
                self.state.known |= 1 << i;
            }
            None => self.state.known &= !(1 << i),
        }
    }

    fn immediate(&mut self, size: Size) -> Option<(String, u32)> {
        let v = match size {
            Size::B => self.word()? as u32 & 0xFF,
            Size::W => self.word()? as u32,
            Size::L => self.long()?,
        };
        Some((format!("#{}", hex(v)), v))
    }

    /// An effective address: mode and register fields, the operand size.
    fn ea(&mut self, mode: u16, reg: u16, size: Size) -> Option<Ea> {
        let plain = |text: String| Some(Ea { text, address: None });
        match mode {
            0 => plain(format!("d{reg}")),
            1 => plain(areg(reg).into()),
            2 => Some(Ea {
                text: format!("({})", areg(reg)),
                address: self.a(reg),
            }),
            3 => {
                let text = format!("({})+", areg(reg));
                let address = self.a(reg);
                self.set_a(reg, None);
                Some(Ea { text, address })
            }
            4 => {
                self.set_a(reg, None);
                plain(format!("-({})", areg(reg)))
            }
            5 => {
                let d = self.word()? as i16 as i32;
                Some(Ea {
                    text: format!("{}({})", signed(d), areg(reg)),
                    address: self.a(reg).map(|a| a.wrapping_add(d as u32) & 0xFF_FFFF),
                })
            }
            6 => {
                let ext = self.word()?;
                let d = ext as u8 as i8 as i32;
                let index = format!(
                    "{}{}.{}",
                    if ext & 0x8000 != 0 { "a" } else { "d" },
                    (ext >> 12) & 7,
                    if ext & 0x800 != 0 { 'l' } else { 'w' }
                );
                Some(Ea {
                    text: format!("{}({},{index})", signed(d), areg(reg)),
                    address: self.a(reg).map(|a| a.wrapping_add(d as u32) & 0xFF_FFFF),
                })
            }
            7 => match reg {
                0 => {
                    let v = self.word()? as i16 as i32 as u32 & 0xFF_FFFF;
                    Some(Ea {
                        text: format!("(${:X}).w", self.bytes_word_text(v)),
                        address: Some(v),
                    })
                }
                1 => {
                    let v = self.long()? & 0xFF_FFFF;
                    Some(Ea {
                        text: format!("(${v:X}).l"),
                        address: Some(v),
                    })
                }
                2 => {
                    let base = self.pc.wrapping_add(self.at as u32);
                    let d = self.word()? as i16 as i32;
                    let t = base.wrapping_add(d as u32) & 0xFF_FFFF;
                    Some(Ea {
                        text: format!("${t:X}(pc)"),
                        address: Some(t),
                    })
                }
                3 => {
                    let base = self.pc.wrapping_add(self.at as u32);
                    let ext = self.word()?;
                    let d = ext as u8 as i8 as i32;
                    let t = base.wrapping_add(d as u32) & 0xFF_FFFF;
                    let index = format!(
                        "{}{}.{}",
                        if ext & 0x8000 != 0 { "a" } else { "d" },
                        (ext >> 12) & 7,
                        if ext & 0x800 != 0 { 'l' } else { 'w' }
                    );
                    Some(Ea {
                        text: format!("${t:X}(pc,{index})"),
                        address: Some(t),
                    })
                }
                4 => {
                    let (text, _) = self.immediate(size)?;
                    plain(text)
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// An absolute short address as written: its low 16 bits.
    fn bytes_word_text(&self, v: u32) -> u32 {
        v & 0xFFFF
    }

    fn done(&self, m: &str, ops: String, flow: Flow) -> Insn {
        Insn::new(self.at as u32, m, ops, flow)
    }
}

/// Decodes the instruction at the start of `bytes`, which the CPU sees at `pc`.
pub(crate) fn decode(bytes: &[u8], pc: u64, state: &mut State) -> Insn {
    let mut d = Dec {
        bytes,
        pc: pc as u32,
        at: 0,
        state,
    };
    match decode_in(&mut d) {
        Some(i) => i,
        None => Insn::bad(2.min(bytes.len() as u32), bytes),
    }
}

/// A data reference for an operand, read or written.
fn with(mut i: Insn, ea: &Ea, kind: RefKind) -> Insn {
    if i.data.is_none()
        && let Some(a) = ea.address
    {
        i.data = Some((a as u64, kind));
    }
    i
}

fn decode_in(d: &mut Dec<'_>) -> Option<Insn> {
    let op = d.word()?;
    let (hi, mode, reg) = (op >> 12, (op >> 3) & 7, op & 7);
    let rx = (op >> 9) & 7;
    match hi {
        0x0 => line0(d, op),
        0x1..=0x3 => {
            let size = match hi {
                1 => Size::B,
                3 => Size::W,
                _ => Size::L,
            };
            let src = d.ea(mode, reg, size)?;
            let (dmode, dreg) = ((op >> 6) & 7, rx);
            if dmode == 1 {
                // movea: the address register may now hold a known address.
                let v = if mode == 7 && reg == 4 {
                    let raw = d.bytes.get(d.at - if size == Size::L { 4 } else { 2 }..d.at)?;
                    Some(if size == Size::L {
                        u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]])
                    } else {
                        u16::from_be_bytes([raw[0], raw[1]]) as i16 as i32 as u32
                    })
                } else if mode == 1 {
                    d.a(reg)
                } else {
                    None
                };
                d.set_a(dreg, v);
                let i = d.done(
                    &format!("movea{}", size.suffix()),
                    format!("{},{}", src.text, areg(dreg)),
                    Flow::Next,
                );
                return Some(with(i, &src, RefKind::Read));
            }
            let dst = d.ea(dmode, dreg, size)?;
            let i = d.done(
                &format!("move{}", size.suffix()),
                format!("{},{}", src.text, dst.text),
                Flow::Next,
            );
            Some(with(with(i, &dst, RefKind::Write), &src, RefKind::Read))
        }
        0x4 => line4(d, op),
        0x5 => {
            let cond = (op >> 8) & 15;
            if op & 0xF8 == 0xC8 {
                let base = d.pc.wrapping_add(2);
                let disp = d.word()? as i16 as i32;
                let t = base.wrapping_add(disp as u32) as u64 & 0xFF_FFFF;
                let m = if cond == 1 {
                    "dbra".to_string()
                } else {
                    format!("db{}", COND[cond as usize])
                };
                return Some(d.done(&m, format!("d{reg},${t:X}"), Flow::Branch(t)));
            }
            if op & 0xC0 == 0xC0 {
                let ea = d.ea(mode, reg, Size::B)?;
                let i = d.done(&format!("s{}", COND[cond as usize]), ea.text.clone(), Flow::Next);
                return Some(with(i, &ea, RefKind::Write));
            }
            let size = Size::field((op >> 6) & 3)?;
            let n = if rx == 0 { 8 } else { rx };
            let m = if op & 0x100 != 0 { "subq" } else { "addq" };
            let ea = d.ea(mode, reg, size)?;
            if mode == 1 {
                d.set_a(reg, None);
            }
            let i = d.done(
                &format!("{m}{}", size.suffix()),
                format!("#{n},{}", ea.text),
                Flow::Next,
            );
            Some(with(i, &ea, RefKind::Write))
        }
        0x6 => {
            let cond = (op >> 8) & 15;
            let base = d.pc.wrapping_add(2);
            let disp = match op & 0xFF {
                0 => d.word()? as i16 as i32,
                b => b as u8 as i8 as i32,
            };
            let t = base.wrapping_add(disp as u32) as u64 & 0xFF_FFFF;
            let short = if op & 0xFF == 0 { ".w" } else { ".s" };
            let (m, flow) = match cond {
                0 => ("bra".to_string(), Flow::Jump(Some(t))),
                1 => ("bsr".to_string(), Flow::Call(Some(t))),
                _ => (format!("b{}", COND[cond as usize]), Flow::Branch(t)),
            };
            if cond == 1 {
                d.set_a(0, None);
                d.set_a(1, None);
            }
            Some(d.done(&format!("{m}{short}"), format!("${t:X}"), flow))
        }
        0x7 => {
            if op & 0x100 != 0 {
                return None;
            }
            Some(d.done("moveq", format!("#{},d{rx}", signed(op as u8 as i8 as i32)), Flow::Next))
        }
        0x8 | 0x9 | 0xB | 0xC | 0xD => line_alu(d, op),
        0xE => {
            let kinds = ["as", "ls", "rox", "ro"];
            let dir = if op & 0x100 != 0 { "l" } else { "r" };
            if op & 0xC0 == 0xC0 {
                let ea = d.ea(mode, reg, Size::W)?;
                let i = d.done(
                    &format!("{}{dir}.w", kinds[((op >> 9) & 3) as usize]),
                    ea.text.clone(),
                    Flow::Next,
                );
                return Some(with(i, &ea, RefKind::Write));
            }
            let size = Size::field((op >> 6) & 3)?;
            let count = if op & 0x20 != 0 {
                format!("d{rx}")
            } else {
                format!("#{}", if rx == 0 { 8 } else { rx })
            };
            Some(d.done(
                &format!("{}{dir}{}", kinds[((op >> 3) & 3) as usize], size.suffix()),
                format!("{count},d{reg}"),
                Flow::Next,
            ))
        }
        // Line A and line F: unimplemented, a trap.
        _ => None,
    }
}

/// Immediate operations and bit operations.
fn line0(d: &mut Dec<'_>, op: u16) -> Option<Insn> {
    let (mode, reg) = ((op >> 3) & 7, op & 7);
    if op & 0x0138 == 0x0108 {
        // movep
        let dx = (op >> 9) & 7;
        let disp = d.word()? as i16 as i32;
        let size = if op & 0x40 != 0 { ".l" } else { ".w" };
        let mem = format!("{}({})", signed(disp), areg(reg));
        let ops = if op & 0x80 != 0 {
            format!("d{dx},{mem}")
        } else {
            format!("{mem},d{dx}")
        };
        return Some(d.done(&format!("movep{size}"), ops, Flow::Next));
    }
    let bitop = ["btst", "bchg", "bclr", "bset"][((op >> 6) & 3) as usize];
    if op & 0x100 != 0 {
        let size = if mode == 0 { Size::L } else { Size::B };
        let ea = d.ea(mode, reg, size)?;
        let i = d.done(bitop, format!("d{},{}", (op >> 9) & 7, ea.text), Flow::Next);
        return Some(with(
            i,
            &ea,
            if bitop == "btst" { RefKind::Read } else { RefKind::Write },
        ));
    }
    let kind = (op >> 9) & 7;
    if kind == 4 {
        let bit = d.word()? & 0xFF;
        let size = if mode == 0 { Size::L } else { Size::B };
        let ea = d.ea(mode, reg, size)?;
        let i = d.done(bitop, format!("#{bit},{}", ea.text), Flow::Next);
        return Some(with(
            i,
            &ea,
            if bitop == "btst" { RefKind::Read } else { RefKind::Write },
        ));
    }
    let m = match kind {
        0 => "ori",
        1 => "andi",
        2 => "subi",
        3 => "addi",
        5 => "eori",
        6 => "cmpi",
        _ => return None,
    };
    // ori/andi/eori to the condition codes or the status register.
    if matches!(kind, 0 | 1 | 5) && op & 0xFF == 0x3C {
        let v = d.word()? & 0xFF;
        return Some(d.done(&format!("{m}.b"), format!("#{},ccr", hex(v as u32)), Flow::Next));
    }
    if matches!(kind, 0 | 1 | 5) && op & 0xFF == 0x7C {
        let v = d.word()?;
        return Some(d.done(&format!("{m}.w"), format!("#{},sr", hex(v as u32)), Flow::Next));
    }
    let size = Size::field((op >> 6) & 3)?;
    let (imm, _) = d.immediate(size)?;
    let ea = d.ea(mode, reg, size)?;
    let i = d.done(
        &format!("{m}{}", size.suffix()),
        format!("{imm},{}", ea.text),
        Flow::Next,
    );
    Some(with(i, &ea, if kind == 6 { RefKind::Read } else { RefKind::Write }))
}

fn line4(d: &mut Dec<'_>, op: u16) -> Option<Insn> {
    let (mode, reg) = ((op >> 3) & 7, op & 7);
    let rx = (op >> 9) & 7;
    match op {
        0x4AFC => return Some(d.done("illegal", String::new(), Flow::Stop)),
        0x4E70 => return Some(d.done("reset", String::new(), Flow::Next)),
        0x4E71 => return Some(d.done("nop", String::new(), Flow::Next)),
        0x4E72 => {
            let v = d.word()?;
            return Some(d.done("stop", format!("#{}", hex(v as u32)), Flow::Next));
        }
        0x4E73 => return Some(d.done("rte", String::new(), Flow::Return)),
        0x4E75 => return Some(d.done("rts", String::new(), Flow::Return)),
        0x4E76 => return Some(d.done("trapv", String::new(), Flow::Next)),
        0x4E77 => return Some(d.done("rtr", String::new(), Flow::Return)),
        _ => {}
    }
    match op & 0xFFF0 {
        0x4E40 => return Some(d.done("trap", format!("#{}", op & 15), Flow::Trap)),
        0x4E50 if op & 8 == 0 => {
            let disp = d.word()? as i16 as i32;
            return Some(d.done("link", format!("{},#{}", areg(reg), signed(disp)), Flow::Next));
        }
        0x4E50 => return Some(d.done("unlk", areg(reg).into(), Flow::Next)),
        0x4E60 => {
            let ops = if op & 8 == 0 {
                format!("{},usp", areg(reg))
            } else {
                format!("usp,{}", areg(reg))
            };
            return Some(d.done("move.l", ops, Flow::Next));
        }
        _ => {}
    }
    match op & 0xFFC0 {
        0x4E80 | 0x4EC0 => {
            let call = op & 0xFFC0 == 0x4E80;
            let ea = d.ea(mode, reg, Size::L)?;
            // A target we know: absolute, pc-relative, or through a known register.
            let t = if matches!(mode, 2 | 5 | 7) {
                ea.address.map(u64::from)
            } else {
                None
            };
            let flow = if call { Flow::Call(t) } else { Flow::Jump(t) };
            if call {
                d.set_a(0, None);
                d.set_a(1, None);
            }
            return Some(d.done(if call { "jsr" } else { "jmp" }, ea.text, flow));
        }
        0x40C0 => {
            let ea = d.ea(mode, reg, Size::W)?;
            let i = d.done("move.w", format!("sr,{}", ea.text), Flow::Next);
            return Some(with(i, &ea, RefKind::Write));
        }
        0x44C0 | 0x46C0 => {
            let ea = d.ea(mode, reg, Size::W)?;
            let to = if op & 0xFFC0 == 0x44C0 { "ccr" } else { "sr" };
            let i = d.done("move.w", format!("{},{to}", ea.text), Flow::Next);
            return Some(with(i, &ea, RefKind::Read));
        }
        0x4800 => {
            let ea = d.ea(mode, reg, Size::B)?;
            let i = d.done("nbcd", ea.text.clone(), Flow::Next);
            return Some(with(i, &ea, RefKind::Write));
        }
        0x4840 if mode == 0 => return Some(d.done("swap", format!("d{reg}"), Flow::Next)),
        0x4840 => {
            let ea = d.ea(mode, reg, Size::L)?;
            let i = d.done("pea", ea.text.clone(), Flow::Next);
            return Some(with(i, &ea, RefKind::Address));
        }
        0x4880 | 0x48C0 if mode == 0 => {
            let size = if op & 0x40 != 0 { ".l" } else { ".w" };
            return Some(d.done(&format!("ext{size}"), format!("d{reg}"), Flow::Next));
        }
        0x4AC0 => {
            let ea = d.ea(mode, reg, Size::B)?;
            let i = d.done("tas", ea.text.clone(), Flow::Next);
            return Some(with(i, &ea, RefKind::Write));
        }
        _ => {}
    }
    if op & 0xFB80 == 0x4880 {
        // movem: a register mask, reversed for -(An).
        let size = if op & 0x40 != 0 { Size::L } else { Size::W };
        let mask = d.word()?;
        let mask = if mode == 4 { mask.reverse_bits() } else { mask };
        let list = register_list(mask);
        let to_regs = op & 0x400 != 0;
        let ea = d.ea(mode, reg, size)?;
        if to_regs {
            for r in 0..8 {
                if mask & (1 << (8 + r)) != 0 {
                    d.set_a(r, None);
                }
            }
        }
        let (ops, kind) = if to_regs {
            (format!("{},{list}", ea.text), RefKind::Read)
        } else {
            (format!("{list},{}", ea.text), RefKind::Write)
        };
        let i = d.done(&format!("movem{}", size.suffix()), ops, Flow::Next);
        return Some(with(i, &ea, kind));
    }
    if op & 0xF1C0 == 0x41C0 {
        let ea = d.ea(mode, reg, Size::L)?;
        d.set_a(rx, ea.address);
        let i = d.done("lea", format!("{},{}", ea.text, areg(rx)), Flow::Next);
        return Some(with(i, &ea, RefKind::Address));
    }
    if op & 0xF1C0 == 0x4180 {
        let ea = d.ea(mode, reg, Size::W)?;
        let i = d.done("chk.w", format!("{},d{rx}", ea.text), Flow::Next);
        return Some(with(i, &ea, RefKind::Read));
    }
    let size = Size::field((op >> 6) & 3)?;
    let m = match op & 0xFF00 {
        0x4000 => "negx",
        0x4200 => "clr",
        0x4400 => "neg",
        0x4600 => "not",
        0x4A00 => "tst",
        _ => return None,
    };
    let ea = d.ea(mode, reg, size)?;
    let i = d.done(&format!("{m}{}", size.suffix()), ea.text.clone(), Flow::Next);
    Some(with(i, &ea, if m == "tst" { RefKind::Read } else { RefKind::Write }))
}

/// `d0-d3/a0/a6`
fn register_list(mask: u16) -> String {
    let mut parts = Vec::new();
    for (bank, base) in [('d', 0), ('a', 8)] {
        let mut r = 0;
        while r < 8 {
            if mask & (1 << (base + r)) != 0 {
                let start = r;
                while r + 1 < 8 && mask & (1 << (base + r + 1)) != 0 {
                    r += 1;
                }
                let name = |i: u16| {
                    if bank == 'a' {
                        areg(i).to_string()
                    } else {
                        format!("d{i}")
                    }
                };
                parts.push(if r > start {
                    format!("{}-{}", name(start), name(r))
                } else {
                    name(start)
                });
            }
            r += 1;
        }
    }
    parts.join("/")
}

/// OR, SUB, CMP/EOR, AND, ADD and their relatives (DIV, MUL, ABCD, EXG...).
fn line_alu(d: &mut Dec<'_>, op: u16) -> Option<Insn> {
    let (mode, reg) = ((op >> 3) & 7, op & 7);
    let rx = (op >> 9) & 7;
    let opmode = (op >> 6) & 7;
    let hi = op >> 12;
    // Word multiplies and divides.
    if opmode == 3 || opmode == 7 {
        if matches!(hi, 0x8 | 0xC) {
            let m = match (hi, opmode) {
                (0x8, 3) => "divu.w",
                (0x8, _) => "divs.w",
                (_, 3) => "mulu.w",
                _ => "muls.w",
            };
            let ea = d.ea(mode, reg, Size::W)?;
            let i = d.done(m, format!("{},d{rx}", ea.text), Flow::Next);
            return Some(with(i, &ea, RefKind::Read));
        }
        // suba/cmpa/adda
        let size = if opmode == 7 { Size::L } else { Size::W };
        let m = match hi {
            0x9 => "suba",
            0xB => "cmpa",
            _ => "adda",
        };
        let ea = d.ea(mode, reg, size)?;
        if m != "cmpa" {
            d.set_a(rx, None);
        }
        let i = d.done(
            &format!("{m}{}", size.suffix()),
            format!("{},{}", ea.text, areg(rx)),
            Flow::Next,
        );
        return Some(with(i, &ea, RefKind::Read));
    }
    let size = Size::field(opmode & 3)?;
    let to_ea = opmode & 4 != 0;
    // The register-to-register and predecrement forms that aren't what the line says.
    if to_ea && mode <= 1 {
        let (m, ok) = match hi {
            0x8 if size == Size::B => ("sbcd", true),
            0xC if size == Size::B => ("abcd", true),
            0x9 => ("subx", true),
            0xD => ("addx", true),
            _ => ("", false),
        };
        if ok {
            let ops = if mode == 1 {
                format!("-({}),-({})", areg(reg), areg(rx))
            } else {
                format!("d{reg},d{rx}")
            };
            let suffix = if matches!(m, "sbcd" | "abcd") {
                ".b"
            } else {
                size.suffix()
            };
            return Some(d.done(&format!("{m}{suffix}"), ops, Flow::Next));
        }
        if hi == 0xC {
            // exg
            let ops = match op & 0xF8 {
                0x40 => format!("d{rx},d{reg}"),
                0x48 => format!("{},{}", areg(rx), areg(reg)),
                0x88 => format!("d{rx},{}", areg(reg)),
                _ => return None,
            };
            return Some(d.done("exg", ops, Flow::Next));
        }
        if hi == 0xB && mode == 1 {
            return Some(d.done(
                &format!("cmpm{}", size.suffix()),
                format!("({})+,({})+", areg(reg), areg(rx)),
                Flow::Next,
            ));
        }
    }
    let m = match (hi, to_ea) {
        (0x8, _) => "or",
        (0x9, _) => "sub",
        (0xB, false) => "cmp",
        (0xB, true) => "eor",
        (0xC, _) => "and",
        _ => "add",
    };
    let ea = d.ea(mode, reg, size)?;
    let (ops, kind) = if to_ea {
        (format!("d{rx},{}", ea.text), RefKind::Write)
    } else {
        (format!("{},d{rx}", ea.text), RefKind::Read)
    };
    let i = d.done(&format!("{m}{}", size.suffix()), ops, Flow::Next);
    Some(with(i, &ea, kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(i: &Insn) -> String {
        format!("{} {}", i.mnemonic, i.operands).trim_end().to_string()
    }

    fn words(ws: &[u16]) -> Vec<u8> {
        ws.iter().flat_map(|w| w.to_be_bytes()).collect()
    }

    #[test]
    fn mega_drive_code() {
        let cases: &[(&[u16], &str, u32)] = &[
            (&[0x46FC, 0x2700], "move.w #$2700,sr", 4),
            (&[0x49F9, 0x00C0, 0x0004], "lea ($C00004).l,a4", 6),
            (&[0x38BC, 0x8144], "move.w #$8144,(a4)", 4),
            (&[0x28BC, 0x4000, 0x0000], "move.l #$40000000,(a4)", 6),
            (&[0x4298], "clr.l (a0)+", 2),
            (&[0x303C, 0x3FFF], "move.w #$3FFF,d0", 4),
            (&[0x5279, 0x00FF, 0x0010], "addq.w #1,($FF0010).l", 6),
            (&[0x4E75], "rts", 2),
            (&[0x48E7, 0xC0C0], "movem.l d0-d1/a0-a1,-(sp)", 4),
            (&[0x7001], "moveq #1,d0", 2),
            (&[0xE348], "lsl.w #1,d0", 2),
            (&[0x4A79, 0x00FF, 0x0000], "tst.w ($FF0000).l", 6),
            (&[0x0839, 0x0003, 0x00A1, 0x0001], "btst #3,($A10001).l", 8),
            (&[0xD041], "add.w d1,d0", 2),
        ];
        for (ws, want, len) in cases {
            let mut s = State::default();
            let i = decode(&words(ws), 0x200, &mut s);
            assert_eq!((text(&i).as_str(), i.len), (*want, *len), "{ws:04X?}");
        }
        // lea into a4, then (a4) is the VDP's control port.
        let mut s = State::default();
        decode(&words(&[0x49F9, 0x00C0, 0x0004]), 0x200, &mut s);
        let i = decode(&words(&[0x38BC, 0x8144]), 0x206, &mut s);
        assert_eq!(i.data, Some((0xC0_0004, RefKind::Write)));
        // bsr.w, bra.s, dbra, jsr (abs), jmp (a0)
        let i = decode(&words(&[0x6100, 0x0010]), 0x210, &mut s);
        assert_eq!(i.flow, Flow::Call(Some(0x222)));
        assert_eq!(decode(&words(&[0x60FE]), 0x230, &mut s).flow, Flow::Jump(Some(0x230)));
        let i = decode(&words(&[0x51C8, 0xFFFC]), 0x240, &mut s);
        assert_eq!((text(&i).as_str(), i.flow), ("dbra d0,$23E", Flow::Branch(0x23E)));
        assert_eq!(
            decode(&words(&[0x4EB9, 0x0000, 0x1234]), 0, &mut s).flow,
            Flow::Call(Some(0x1234))
        );
        assert_eq!(decode(&words(&[0x4ED0]), 0, &mut s).flow, Flow::Jump(None));
        assert_eq!(decode(&words(&[0x4E73]), 0, &mut s).flow, Flow::Return);
    }
}
