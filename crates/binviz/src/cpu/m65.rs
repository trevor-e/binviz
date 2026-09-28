//! The 6502 (NES) and 65816 (SNES): one opcode table, the 6502 using the 151
//! opcodes it documents. The 65816's immediates are 8 or 16 bits wide as its
//! M and X flags say, which `REP` and `SEP` change as the code runs.

use super::{Flow, Insn, State, hex};
use crate::xrefs::RefKind;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Imp,
    Acc,
    /// Immediate as wide as the accumulator (the M flag).
    ImmM,
    /// Immediate as wide as the index registers (the X flag).
    ImmX,
    Imm8,
    Imm16,
    Dp,
    DpX,
    DpY,
    DpInd,
    DpIndX,
    DpIndY,
    DpIndLong,
    DpIndLongY,
    Abs,
    AbsX,
    AbsY,
    AbsInd,
    AbsIndX,
    AbsIndLong,
    Long,
    LongX,
    Sr,
    SrIndY,
    Rel,
    RelLong,
    Move,
}

use Mode::*;

#[rustfmt::skip]
const OPS: [(&str, Mode); 256] = [
    // 0x00
    ("brk", Imm8), ("ora", DpIndX), ("cop", Imm8), ("ora", Sr), ("tsb", Dp), ("ora", Dp), ("asl", Dp), ("ora", DpIndLong),
    ("php", Imp), ("ora", ImmM), ("asl", Acc), ("phd", Imp), ("tsb", Abs), ("ora", Abs), ("asl", Abs), ("ora", Long),
    // 0x10
    ("bpl", Rel), ("ora", DpIndY), ("ora", DpInd), ("ora", SrIndY), ("trb", Dp), ("ora", DpX), ("asl", DpX), ("ora", DpIndLongY),
    ("clc", Imp), ("ora", AbsY), ("inc", Acc), ("tcs", Imp), ("trb", Abs), ("ora", AbsX), ("asl", AbsX), ("ora", LongX),
    // 0x20
    ("jsr", Abs), ("and", DpIndX), ("jsl", Long), ("and", Sr), ("bit", Dp), ("and", Dp), ("rol", Dp), ("and", DpIndLong),
    ("plp", Imp), ("and", ImmM), ("rol", Acc), ("pld", Imp), ("bit", Abs), ("and", Abs), ("rol", Abs), ("and", Long),
    // 0x30
    ("bmi", Rel), ("and", DpIndY), ("and", DpInd), ("and", SrIndY), ("bit", DpX), ("and", DpX), ("rol", DpX), ("and", DpIndLongY),
    ("sec", Imp), ("and", AbsY), ("dec", Acc), ("tsc", Imp), ("bit", AbsX), ("and", AbsX), ("rol", AbsX), ("and", LongX),
    // 0x40
    ("rti", Imp), ("eor", DpIndX), ("wdm", Imm8), ("eor", Sr), ("mvp", Move), ("eor", Dp), ("lsr", Dp), ("eor", DpIndLong),
    ("pha", Imp), ("eor", ImmM), ("lsr", Acc), ("phk", Imp), ("jmp", Abs), ("eor", Abs), ("lsr", Abs), ("eor", Long),
    // 0x50
    ("bvc", Rel), ("eor", DpIndY), ("eor", DpInd), ("eor", SrIndY), ("mvn", Move), ("eor", DpX), ("lsr", DpX), ("eor", DpIndLongY),
    ("cli", Imp), ("eor", AbsY), ("phy", Imp), ("tcd", Imp), ("jml", Long), ("eor", AbsX), ("lsr", AbsX), ("eor", LongX),
    // 0x60
    ("rts", Imp), ("adc", DpIndX), ("per", RelLong), ("adc", Sr), ("stz", Dp), ("adc", Dp), ("ror", Dp), ("adc", DpIndLong),
    ("pla", Imp), ("adc", ImmM), ("ror", Acc), ("rtl", Imp), ("jmp", AbsInd), ("adc", Abs), ("ror", Abs), ("adc", Long),
    // 0x70
    ("bvs", Rel), ("adc", DpIndY), ("adc", DpInd), ("adc", SrIndY), ("stz", DpX), ("adc", DpX), ("ror", DpX), ("adc", DpIndLongY),
    ("sei", Imp), ("adc", AbsY), ("ply", Imp), ("tdc", Imp), ("jmp", AbsIndX), ("adc", AbsX), ("ror", AbsX), ("adc", LongX),
    // 0x80
    ("bra", Rel), ("sta", DpIndX), ("brl", RelLong), ("sta", Sr), ("sty", Dp), ("sta", Dp), ("stx", Dp), ("sta", DpIndLong),
    ("dey", Imp), ("bit", ImmM), ("txa", Imp), ("phb", Imp), ("sty", Abs), ("sta", Abs), ("stx", Abs), ("sta", Long),
    // 0x90
    ("bcc", Rel), ("sta", DpIndY), ("sta", DpInd), ("sta", SrIndY), ("sty", DpX), ("sta", DpX), ("stx", DpY), ("sta", DpIndLongY),
    ("tya", Imp), ("sta", AbsY), ("txs", Imp), ("txy", Imp), ("stz", Abs), ("sta", AbsX), ("stz", AbsX), ("sta", LongX),
    // 0xA0
    ("ldy", ImmX), ("lda", DpIndX), ("ldx", ImmX), ("lda", Sr), ("ldy", Dp), ("lda", Dp), ("ldx", Dp), ("lda", DpIndLong),
    ("tay", Imp), ("lda", ImmM), ("tax", Imp), ("plb", Imp), ("ldy", Abs), ("lda", Abs), ("ldx", Abs), ("lda", Long),
    // 0xB0
    ("bcs", Rel), ("lda", DpIndY), ("lda", DpInd), ("lda", SrIndY), ("ldy", DpX), ("lda", DpX), ("ldx", DpY), ("lda", DpIndLongY),
    ("clv", Imp), ("lda", AbsY), ("tsx", Imp), ("tyx", Imp), ("ldy", AbsX), ("lda", AbsX), ("ldx", AbsY), ("lda", LongX),
    // 0xC0
    ("cpy", ImmX), ("cmp", DpIndX), ("rep", Imm8), ("cmp", Sr), ("cpy", Dp), ("cmp", Dp), ("dec", Dp), ("cmp", DpIndLong),
    ("iny", Imp), ("cmp", ImmM), ("dex", Imp), ("wai", Imp), ("cpy", Abs), ("cmp", Abs), ("dec", Abs), ("cmp", Long),
    // 0xD0
    ("bne", Rel), ("cmp", DpIndY), ("cmp", DpInd), ("cmp", SrIndY), ("pei", DpInd), ("cmp", DpX), ("dec", DpX), ("cmp", DpIndLongY),
    ("cld", Imp), ("cmp", AbsY), ("phx", Imp), ("stp", Imp), ("jml", AbsIndLong), ("cmp", AbsX), ("dec", AbsX), ("cmp", LongX),
    // 0xE0
    ("cpx", ImmX), ("sbc", DpIndX), ("sep", Imm8), ("sbc", Sr), ("cpx", Dp), ("sbc", Dp), ("inc", Dp), ("sbc", DpIndLong),
    ("inx", Imp), ("sbc", ImmM), ("nop", Imp), ("xba", Imp), ("cpx", Abs), ("sbc", Abs), ("inc", Abs), ("sbc", Long),
    // 0xF0
    ("beq", Rel), ("sbc", DpIndY), ("sbc", DpInd), ("sbc", SrIndY), ("pea", Imm16), ("sbc", DpX), ("inc", DpX), ("sbc", DpIndLongY),
    ("sed", Imp), ("sbc", AbsY), ("plx", Imp), ("xce", Imp), ("jsr", AbsIndX), ("sbc", AbsX), ("inc", AbsX), ("sbc", LongX),
];

/// The opcodes the 6502 documents (the NES's code uses these).
#[rustfmt::skip]
const NMOS: [u8; 151] = [
    0x00, 0x01, 0x05, 0x06, 0x08, 0x09, 0x0A, 0x0D, 0x0E, 0x10, 0x11, 0x15, 0x16, 0x18, 0x19, 0x1D, 0x1E,
    0x20, 0x21, 0x24, 0x25, 0x26, 0x28, 0x29, 0x2A, 0x2C, 0x2D, 0x2E, 0x30, 0x31, 0x35, 0x36, 0x38, 0x39,
    0x3D, 0x3E, 0x40, 0x41, 0x45, 0x46, 0x48, 0x49, 0x4A, 0x4C, 0x4D, 0x4E, 0x50, 0x51, 0x55, 0x56, 0x58,
    0x59, 0x5D, 0x5E, 0x60, 0x61, 0x65, 0x66, 0x68, 0x69, 0x6A, 0x6C, 0x6D, 0x6E, 0x70, 0x71, 0x75, 0x76,
    0x78, 0x79, 0x7D, 0x7E, 0x81, 0x84, 0x85, 0x86, 0x88, 0x8A, 0x8C, 0x8D, 0x8E, 0x90, 0x91, 0x94, 0x95,
    0x96, 0x98, 0x99, 0x9A, 0x9D, 0xA0, 0xA1, 0xA2, 0xA4, 0xA5, 0xA6, 0xA8, 0xA9, 0xAA, 0xAC, 0xAD, 0xAE,
    0xB0, 0xB1, 0xB4, 0xB5, 0xB6, 0xB8, 0xB9, 0xBA, 0xBC, 0xBD, 0xBE, 0xC0, 0xC1, 0xC4, 0xC5, 0xC6, 0xC8,
    0xC9, 0xCA, 0xCC, 0xCD, 0xCE, 0xD0, 0xD1, 0xD5, 0xD6, 0xD8, 0xD9, 0xDD, 0xDE, 0xE0, 0xE1, 0xE4, 0xE5,
    0xE6, 0xE8, 0xE9, 0xEA, 0xEC, 0xED, 0xEE, 0xF0, 0xF1, 0xF5, 0xF6, 0xF8, 0xF9, 0xFD, 0xFE,
];

/// How an instruction uses the memory its operand names.
fn access(mnemonic: &str) -> RefKind {
    match mnemonic {
        "sta" | "stx" | "sty" | "stz" | "asl" | "lsr" | "rol" | "ror" | "inc" | "dec" | "tsb" | "trb" => RefKind::Write,
        _ => RefKind::Read,
    }
}

/// Decodes a 6502 instruction, or with `state` a 65816 one (updating the
/// register widths on `REP`, `SEP` and `XCE`).
pub(crate) fn decode(bytes: &[u8], pc: u64, state: Option<&mut State>) -> Insn {
    let op = bytes[0];
    let w65816 = state.is_some();
    if !w65816 && NMOS.binary_search(&op).is_err() {
        return Insn::bad(1, bytes);
    }
    let (mnemonic, mode) = OPS[op as usize];
    let (m8, x8) = state.as_ref().map_or((true, true), |s| (s.m8, s.x8));
    let len: u32 = 1 + match mode {
        Imp | Acc => 0,
        ImmM => 2 - m8 as u32,
        ImmX => 2 - x8 as u32,
        Imm8 | Dp | DpX | DpY | DpInd | DpIndX | DpIndY | DpIndLong | DpIndLongY | Sr | SrIndY | Rel => 1,
        Imm16 | Abs | AbsX | AbsY | AbsInd | AbsIndX | AbsIndLong | RelLong | Move => 2,
        Long | LongX => 3,
    };
    if bytes.len() < len as usize {
        return Insn::bad(bytes.len() as u32, bytes);
    }
    let b = |i: usize| bytes[i] as u64;
    let operand = match len {
        2 => b(1),
        3 => b(1) | b(2) << 8,
        4 => b(1) | b(2) << 8 | b(3) << 16,
        _ => 0,
    };
    // The 65816 runs in 64 KiB banks: a 16-bit address is in the program's bank,
    // and so (as a guess) is data; the 6502 has only the one.
    let bank = if w65816 { pc & 0xFF_0000 } else { 0 };
    let in_bank = |a: u64| bank | (a & 0xFFFF);
    let operands = match mode {
        Imp => String::new(),
        Acc => "a".into(),
        ImmM | ImmX | Imm8 | Imm16 => format!("#{}", hex(operand, (len as usize - 1) * 2)),
        Dp => hex(operand, 2),
        DpX | Sr => format!("{},{}", hex(operand, 2), if mode == Sr { 's' } else { 'x' }),
        DpY => format!("{},y", hex(operand, 2)),
        DpInd => format!("({})", hex(operand, 2)),
        DpIndX => format!("({},x)", hex(operand, 2)),
        DpIndY => format!("({}),y", hex(operand, 2)),
        DpIndLong => format!("[{}]", hex(operand, 2)),
        DpIndLongY => format!("[{}],y", hex(operand, 2)),
        SrIndY => format!("({},s),y", hex(operand, 2)),
        Abs => hex(operand, 4),
        AbsX => format!("{},x", hex(operand, 4)),
        AbsY => format!("{},y", hex(operand, 4)),
        AbsInd => format!("({})", hex(operand, 4)),
        AbsIndX => format!("({},x)", hex(operand, 4)),
        AbsIndLong => format!("[{}]", hex(operand, 4)),
        Long => hex(operand, 6),
        LongX => format!("{},x", hex(operand, 6)),
        Rel | RelLong => String::new(),
        Move => format!("{},{}", hex(b(2), 2), hex(b(1), 2)),
    };
    let next = pc + len as u64;
    // Branches wrap within the bank.
    let relative = |offset: i64| in_bank((next as i64 + offset) as u64);
    let mut insn = match mode {
        Rel => {
            let t = relative(bytes[1] as i8 as i64);
            let flow = if mnemonic == "bra" {
                Flow::Jump(Some(t))
            } else {
                Flow::Branch(t)
            };
            return Insn::new(len, mnemonic, hex(t & 0xFFFF, 4), flow);
        }
        RelLong => {
            let t = relative(operand as u16 as i16 as i64);
            if mnemonic == "per" {
                return Insn::new(len, mnemonic, hex(t & 0xFFFF, 4), Flow::Next).with_data(t, RefKind::Address);
            }
            return Insn::new(len, mnemonic, hex(t & 0xFFFF, 4), Flow::Jump(Some(t)));
        }
        _ => Insn::new(len, mnemonic, operands, Flow::Next),
    };
    insn.flow = match (mnemonic, mode) {
        ("jmp", Abs) => Flow::Jump(Some(in_bank(operand))),
        ("jml", Long) => Flow::Jump(Some(operand)),
        ("jmp" | "jml", _) => Flow::Jump(None),
        ("jsr", Abs) => Flow::Call(Some(in_bank(operand))),
        ("jsl", Long) => Flow::Call(Some(operand)),
        ("jsr", _) => Flow::Call(None),
        ("rts" | "rtl" | "rti", _) => Flow::Return,
        ("brk" | "cop", _) => Flow::Trap,
        ("stp", _) => Flow::Stop,
        _ => Flow::Next,
    };
    // The memory an operand names: for indirect modes, the pointer.
    insn.data = match mode {
        Dp | DpX | DpY => Some((operand, access(mnemonic))),
        DpInd | DpIndX | DpIndY | DpIndLong | DpIndLongY => Some((operand, RefKind::Read)),
        Abs | AbsX | AbsY if !matches!(mnemonic, "jmp" | "jsr") => Some((in_bank(operand), access(mnemonic))),
        AbsInd | AbsIndLong => Some((operand, RefKind::Read)),
        AbsIndX => Some((in_bank(operand), RefKind::Read)),
        Long | LongX if !matches!(mnemonic, "jml" | "jsl") => Some((operand, access(mnemonic))),
        _ => None,
    };
    if let Some(s) = state {
        match mnemonic {
            "rep" => {
                s.m8 &= operand & 0x20 == 0;
                s.x8 &= operand & 0x10 == 0;
            }
            "sep" => {
                s.m8 |= operand & 0x20 != 0;
                s.x8 |= operand & 0x10 != 0;
            }
            _ => {}
        }
    }
    insn
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(i: &Insn) -> String {
        format!("{} {}", i.mnemonic, i.operands).trim_end().to_string()
    }

    #[test]
    fn nes_code() {
        // lda #$00 / sta $2000 / lda $2002 / bpl -5 / jsr $C123 / jmp ($FFFC) / rts
        let code = [
            0xA9, 0x00, 0x8D, 0x00, 0x20, 0xAD, 0x02, 0x20, 0x10, 0xFB, 0x20, 0x23, 0xC1, 0x6C, 0xFC, 0xFF, 0x60,
        ];
        let mut pc = 0xC000u64;
        let mut at = 0;
        let mut out = Vec::new();
        while at < code.len() {
            let i = decode(&code[at..], pc, None);
            out.push((text(&i), i.flow, i.data));
            at += i.len as usize;
            pc += i.len as u64;
        }
        assert_eq!(out[0].0, "lda #$00");
        assert_eq!(
            (out[1].0.as_str(), out[1].2),
            ("sta $2000", Some((0x2000, RefKind::Write)))
        );
        assert_eq!(out[2].2, Some((0x2002, RefKind::Read)));
        assert_eq!((out[3].0.as_str(), out[3].1), ("bpl $C005", Flow::Branch(0xC005)));
        assert_eq!(out[4].1, Flow::Call(Some(0xC123)));
        assert_eq!((out[5].0.as_str(), out[5].1), ("jmp ($FFFC)", Flow::Jump(None)));
        assert_eq!(out[6].1, Flow::Return);
        // 65816-only opcodes are data to a 6502.
        assert_eq!(decode(&[0x22, 0, 0, 0], 0, None).flow, Flow::Stop);
    }

    #[test]
    fn snes_widths_follow_rep_and_sep() {
        let mut s = State::RESET;
        // rep #$30: 16-bit accumulator and index registers.
        decode(&[0xC2, 0x30], 0x80_8000, Some(&mut s));
        assert!(!s.m8 && !s.x8);
        let i = decode(&[0xA9, 0x34, 0x12], 0x80_8002, Some(&mut s));
        assert_eq!((text(&i).as_str(), i.len), ("lda #$1234", 3));
        let i = decode(&[0xA2, 0x00, 0x01], 0x80_8005, Some(&mut s));
        assert_eq!(text(&i), "ldx #$0100");
        // sep #$20: 8-bit accumulator again, index registers still 16.
        decode(&[0xE2, 0x20], 0x80_8008, Some(&mut s));
        assert!(s.m8 && !s.x8);
        assert_eq!(decode(&[0xA9, 0x01], 0, Some(&mut s)).len, 2);
        // jsl $018000 / jml [$FFFC] / bra back / sta $2100 (in the program bank)
        let i = decode(&[0x22, 0x00, 0x80, 0x01], 0x80_800A, Some(&mut s));
        assert_eq!(i.flow, Flow::Call(Some(0x01_8000)));
        assert_eq!(decode(&[0xDC, 0xFC, 0xFF], 0, Some(&mut s)).flow, Flow::Jump(None));
        let i = decode(&[0x80, 0xFE], 0x80_8010, Some(&mut s));
        assert_eq!(i.flow, Flow::Jump(Some(0x80_8010)));
        let i = decode(&[0x8D, 0x00, 0x21], 0x80_8012, Some(&mut s));
        assert_eq!(i.data, Some((0x80_2100, RefKind::Write)));
        // mvn: source bank, then destination.
        assert_eq!(text(&decode(&[0x54, 0x7E, 0x01], 0, Some(&mut s))), "mvn $01,$7E");
    }
}
