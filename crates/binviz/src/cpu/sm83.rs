//! The Game Boy's SM83: a Z80 relative with fewer registers, its own loads
//! from the I/O page (`ldh`), and the `CB`-prefixed bit operations. Written
//! as RGBDS does.

use super::{Flow, Insn, hex};
use crate::xrefs::RefKind;

const R: [&str; 8] = ["b", "c", "d", "e", "h", "l", "[hl]", "a"];
const RP: [&str; 4] = ["bc", "de", "hl", "sp"];
const RP2: [&str; 4] = ["bc", "de", "hl", "af"];
const CC: [&str; 4] = ["nz", "z", "nc", "c"];
const ROT: [&str; 8] = ["rlc", "rrc", "rl", "rr", "sla", "sra", "swap", "srl"];

/// `add a, b`, `sub b`, `cp $10`: the eight operations on the accumulator.
fn alu(y: u8, operand: &str) -> (&'static str, String) {
    let m = ["add", "adc", "sub", "sbc", "and", "xor", "or", "cp"][y as usize];
    let ops = if matches!(y, 0 | 1 | 3) {
        format!("a, {operand}")
    } else {
        operand.to_string()
    };
    (m, ops)
}

pub(crate) fn decode(bytes: &[u8], pc: u64) -> Insn {
    let op = bytes[0];
    let (x, y, z) = (op >> 6, (op >> 3) & 7, op & 7);
    let (p, q) = (y >> 1, y & 1);
    let need = |n: usize| bytes.len() >= n;
    let d8 = || bytes[1] as u64;
    let d16 = || bytes[1] as u64 | (bytes[2] as u64) << 8;
    let rel = |len: u64| (pc + len).wrapping_add(bytes[1] as i8 as u64) & 0xFFFF;
    let len = match (x, z) {
        (0, 1) => 3,
        (0, 0) if y == 1 => 3,
        (0, 0) if y >= 2 => 2,
        (0, 6) => 2,
        (3, 0) if y >= 4 => 2,
        (3, 2) if y < 4 || y == 5 || y == 7 => 3,
        (3, 3) if y == 0 => 3,
        (3, 3) if y == 1 => 2,
        (3, 4) => 3,
        (3, 5) if q == 1 && p == 0 => 3,
        (3, 6) => 2,
        _ => 1,
    };
    if !need(len) {
        return Insn::bad(bytes.len() as u32, bytes);
    }
    let len32 = len as u32;
    let plain = |m: &str, ops: String| Insn::new(len32, m, ops, Flow::Next);
    match x {
        0 => match z {
            0 => match y {
                0 => plain("nop", String::new()),
                1 => plain("ld", format!("[{}], sp", hex(d16(), 4))).with_data(d16(), RefKind::Write),
                2 => plain("stop", String::new()),
                3 => Insn::new(len32, "jr", hex(rel(2), 4), Flow::Jump(Some(rel(2)))),
                _ => Insn::new(
                    len32,
                    "jr",
                    format!("{}, {}", CC[y as usize - 4], hex(rel(2), 4)),
                    Flow::Branch(rel(2)),
                ),
            },
            1 => {
                if q == 0 {
                    let i = plain("ld", format!("{}, {}", RP[p as usize], hex(d16(), 4)));
                    // A pointer, likely, when it goes to de or hl and points past the header.
                    if matches!(p, 1 | 2) && d16() >= 0x150 {
                        i.with_data(d16(), RefKind::Address)
                    } else {
                        i
                    }
                } else {
                    plain("add", format!("hl, {}", RP[p as usize]))
                }
            }
            2 => {
                let mem = ["[bc]", "[de]", "[hl+]", "[hl-]"][p as usize];
                if q == 0 {
                    plain("ld", format!("{mem}, a"))
                } else {
                    plain("ld", format!("a, {mem}"))
                }
            }
            3 => plain(if q == 0 { "inc" } else { "dec" }, RP[p as usize].into()),
            4 => plain("inc", R[y as usize].into()),
            5 => plain("dec", R[y as usize].into()),
            6 => plain("ld", format!("{}, {}", R[y as usize], hex(d8(), 2))),
            _ => plain(
                ["rlca", "rrca", "rla", "rra", "daa", "cpl", "scf", "ccf"][y as usize],
                String::new(),
            ),
        },
        1 => {
            if y == 6 && z == 6 {
                plain("halt", String::new())
            } else {
                plain("ld", format!("{}, {}", R[y as usize], R[z as usize]))
            }
        }
        2 => {
            let (m, ops) = alu(y, R[z as usize]);
            plain(m, ops)
        }
        _ => match z {
            0 => match y {
                0..=3 => Insn::new(len32, "ret", CC[y as usize].into(), Flow::CondReturn),
                4 => plain("ldh", format!("[{}], a", hex(0xFF00 | d8(), 4))).with_data(0xFF00 | d8(), RefKind::Write),
                5 => plain("add", format!("sp, {}", bytes[1] as i8)),
                6 => plain("ldh", format!("a, [{}]", hex(0xFF00 | d8(), 4))).with_data(0xFF00 | d8(), RefKind::Read),
                _ => {
                    let d = bytes[1] as i8;
                    plain(
                        "ld",
                        format!("hl, sp{}{}", if d < 0 { "-" } else { "+" }, d.unsigned_abs()),
                    )
                }
            },
            1 => {
                if q == 0 {
                    return plain("pop", RP2[p as usize].into());
                }
                match p {
                    0 => Insn::new(1, "ret", String::new(), Flow::Return),
                    1 => Insn::new(1, "reti", String::new(), Flow::Return),
                    2 => Insn::new(1, "jp", "hl".into(), Flow::Jump(None)),
                    _ => plain("ld", "sp, hl".into()),
                }
            }
            2 => match y {
                0..=3 => Insn::new(
                    len32,
                    "jp",
                    format!("{}, {}", CC[y as usize], hex(d16(), 4)),
                    Flow::Branch(d16()),
                ),
                4 => plain("ldh", "[c], a".into()),
                5 => plain("ld", format!("[{}], a", hex(d16(), 4))).with_data(d16(), RefKind::Write),
                6 => plain("ldh", "a, [c]".into()),
                _ => plain("ld", format!("a, [{}]", hex(d16(), 4))).with_data(d16(), RefKind::Read),
            },
            3 => match y {
                0 => Insn::new(len32, "jp", hex(d16(), 4), Flow::Jump(Some(d16()))),
                1 => {
                    let cb = bytes[1];
                    let (cx, cy, cz) = (cb >> 6, (cb >> 3) & 7, cb & 7);
                    let r = R[cz as usize];
                    match cx {
                        0 => plain(ROT[cy as usize], r.into()),
                        1 => plain("bit", format!("{cy}, {r}")),
                        2 => plain("res", format!("{cy}, {r}")),
                        _ => plain("set", format!("{cy}, {r}")),
                    }
                }
                6 => plain("di", String::new()),
                7 => plain("ei", String::new()),
                _ => Insn::bad(1, bytes),
            },
            4 => {
                if y < 4 {
                    Insn::new(
                        len32,
                        "call",
                        format!("{}, {}", CC[y as usize], hex(d16(), 4)),
                        Flow::Call(Some(d16())),
                    )
                } else {
                    Insn::bad(1, bytes)
                }
            }
            5 => {
                if q == 0 {
                    plain("push", RP2[p as usize].into())
                } else if p == 0 {
                    Insn::new(len32, "call", hex(d16(), 4), Flow::Call(Some(d16())))
                } else {
                    Insn::bad(1, bytes)
                }
            }
            6 => {
                let (m, ops) = alu(y, &hex(d8(), 2));
                plain(m, ops)
            }
            _ => {
                let t = y as u64 * 8;
                Insn::new(1, "rst", hex(t, 2), Flow::Call(Some(t)))
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(i: &Insn) -> String {
        format!("{} {}", i.mnemonic, i.operands).trim_end().to_string()
    }

    #[test]
    fn game_boy_code() {
        let cases: &[(&[u8], &str)] = &[
            (&[0x00], "nop"),
            (&[0xC3, 0x50, 0x01], "jp $0150"),
            (&[0x3E, 0x91], "ld a, $91"),
            (&[0xE0, 0x40], "ldh [$FF40], a"),
            (&[0xF0, 0x44], "ldh a, [$FF44]"),
            (&[0xFE, 0x90], "cp $90"),
            (&[0x20, 0xFA], "jr nz, $0FFC"),
            (&[0x21, 0x00, 0xC0], "ld hl, $C000"),
            (&[0x22], "ld [hl+], a"),
            (&[0xCD, 0x00, 0x40], "call $4000"),
            (&[0xCB, 0x7F], "bit 7, a"),
            (&[0xCB, 0x37], "swap a"),
            (&[0xEA, 0x00, 0x20], "ld [$2000], a"),
            (&[0xF8, 0xFE], "ld hl, sp-2"),
            (&[0xC9], "ret"),
            (&[0xD8], "ret c"),
            (&[0xFF], "rst $38"),
            (&[0x76], "halt"),
            (&[0x80], "add a, b"),
            (&[0xAF], "xor a"),
        ];
        for (bytes, want) in cases {
            let i = decode(bytes, 0x1000);
            assert_eq!(text(&i), *want, "{bytes:02X?}");
            assert_eq!(i.len as usize, bytes.len(), "{want}");
        }
        assert_eq!(decode(&[0xE0, 0x40], 0).data, Some((0xFF40, RefKind::Write)));
        assert_eq!(decode(&[0x20, 0xFA], 0x1000).flow, Flow::Branch(0x0FFC));
        assert_eq!(decode(&[0xC9], 0).flow, Flow::Return);
        assert_eq!(decode(&[0xD8], 0).flow, Flow::CondReturn);
        assert_eq!(decode(&[0xE9], 0).flow, Flow::Jump(None));
        // Holes in the opcode map are data.
        assert_eq!(decode(&[0xD3], 0).flow, Flow::Stop);
    }
}
