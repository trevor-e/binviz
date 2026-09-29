//! What a function's code says about how it is called, for writing its
//! prototype: which argument registers it reads before writing them, the
//! arguments it takes from the stack, whether it returns a value, its frame
//! size and saved registers, whether it calls anything, and the structures
//! it walks (the offsets it loads and stores off each base register).
//!
//! MIPS (o32): `$a0`–`$a3` carry the first four arguments, the rest sit
//! above the caller's frame at `0x10($sp)` on, `$v0` (and `$v1`) carry the
//! result, and `$s0`–`$s7`, `$fp` and `$ra` are the callee's to save.
//!
//! x86 and x86-64: the stack pointer followed from the entry (see
//! [`crate::stack`]) says which arguments are read, and `ret N`, `ecx` and
//! `edx` say the calling convention.

use serde::Serialize;

use crate::binary::Binary;
use crate::cpu::Cpu;
use crate::cpu::mips::MipsWord;
use crate::util::Endian;

/// A guess at a function's prototype, from its code.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSignature {
    pub address: u64,
    pub name: String,
    /// Arguments passed in registers (`$a0`–`$a3`): how many are read before being written.
    pub register_args: u32,
    /// Arguments taken from the stack, past the four registers.
    pub stack_args: u32,
    /// Whether `$v0` is set by the function's own code (not just left over from a call).
    pub returns: bool,
    /// Bytes the function reserves on the stack.
    pub frame: u32,
    /// Registers it saves in its prologue.
    pub saved: Vec<String>,
    /// It calls nothing.
    pub leaf: bool,
    pub calls: u32,
    /// Uses the PlayStation's GTE (coprocessor 2).
    pub uses_cop2: bool,
    /// Uses the FPU (coprocessor 1).
    pub uses_float: bool,
    /// Memory walked off each base register: structure layout hints.
    pub accesses: Vec<Access>,
    /// `int name(int a0, int a1)`.
    pub prototype: String,
    /// The first argument is only written through, and `$v0` is set:
    /// likely the hidden pointer a structure returned by value is built in.
    pub returns_struct: bool,
    /// x86: the calling convention the code implies (`cdecl`, `stdcall`,
    /// `fastcall`, `thiscall`; `win64`, `sysv` for x86-64).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub convention: Option<String>,
    /// x86: bytes of arguments the function pops itself (`ret 8`).
    pub pops: u32,
    /// x86: the argument registers read before they are written, in argument order.
    pub registers: Vec<String>,
    /// x86: `ebp` is set up as a frame pointer.
    pub frame_pointer: bool,
    /// x86: the stack pointer is aligned to this many bytes (`and esp, -16`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aligned: Option<u32>,
    /// x86: the result is left on the x87 stack (a `float` or `double`).
    pub returns_float: bool,
    /// x86: some return isn't reached with the stack pointer where it
    /// started, so what a call pops was guessed wrong somewhere (a call
    /// through a pointer to a function that pops its arguments, say).
    pub unbalanced: bool,
}

/// The offsets a function loads and stores off one base register.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Access {
    /// The register (`$a0`, `$s1`), or the argument it carries (`a0`).
    pub base: String,
    /// (offset, width in bytes, `r`, `w` or `rw`), by offset.
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub offset: i32,
    pub width: u8,
    pub access: String,
}

const REGS: [&str; 32] = [
    "$zero", "$at", "$v0", "$v1", "$a0", "$a1", "$a2", "$a3", "$t0", "$t1", "$t2", "$t3", "$t4", "$t5", "$t6", "$t7",
    "$s0", "$s1", "$s2", "$s3", "$s4", "$s5", "$s6", "$s7", "$t8", "$t9", "$k0", "$k1", "$gp", "$sp", "$fp", "$ra",
];

/// The registers an instruction reads.
fn reads(w: MipsWord) -> u32 {
    let (rs, rt) = (1u32 << w.rs(), 1u32 << w.rt());
    match w.op() {
        0 => match w.funct() {
            0 | 2 | 3 => rt,
            8 | 9 | 17 | 19 => rs,
            12 | 13 | 15 | 16 | 18 => 0,
            _ => rs | rt,
        },
        1 => rs,
        2 | 3 | 15 => 0,
        4 | 5 | 20 | 21 => rs | rt,
        6 | 7 | 22 | 23 => rs,
        8..=14 | 24 | 25 => rs,
        16..=18 => {
            if w.rs() >= 4 && w.rs() <= 6 { rt } else { 0 }
        }
        40..=46 | 56 | 63 => rs | rt,
        _ => rs,
    }
}

/// A load or store: (width, store).
fn access(w: MipsWord) -> Option<(u8, bool)> {
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
    /// The word size and endianness of MIPS code here, if this is MIPS.
    fn mips_endian(&self) -> Option<Endian> {
        match self.rom.as_ref().map(|r| r.cpu) {
            Some(Cpu::MipsR3000) => Some(Endian::Little),
            Some(Cpu::MipsR4300) => Some(Endian::Big),
            Some(_) => None,
            None => (self.arch == object::Architecture::Mips).then_some(self.endian),
        }
    }

    /// What the code of the function at `address` says about its prototype
    /// (MIPS, x86 and x86-64).
    pub fn function_signature(&self, address: u64) -> Option<FunctionSignature> {
        if matches!(self.arch, object::Architecture::I386 | object::Architecture::X86_64) && self.rom.is_none() {
            return self.x86_signature(address);
        }
        let endian = self.mips_endian()?;
        let f = self.symbols.function_containing(address)?;
        if f.size == 0 {
            return None;
        }
        let bytes = self.code_bytes(f.address)?;
        let words = bytes
            .get(..(f.size as usize).min(bytes.len()))?
            .chunks_exact(4)
            .map(|c| {
                let b = [c[0], c[1], c[2], c[3]];
                MipsWord(match endian {
                    Endian::Little => u32::from_le_bytes(b),
                    Endian::Big => u32::from_be_bytes(b),
                })
            })
            .collect::<Vec<_>>();
        let mut sig = FunctionSignature {
            address: f.address,
            name: f.display_name().into_owned(),
            register_args: 0,
            stack_args: 0,
            returns: false,
            frame: 0,
            saved: Vec::new(),
            leaf: true,
            calls: 0,
            uses_cop2: false,
            uses_float: false,
            accesses: Vec::new(),
            prototype: String::new(),
            returns_struct: false,
            convention: None,
            pops: 0,
            registers: Vec::new(),
            frame_pointer: false,
            aligned: None,
            returns_float: false,
            unbalanced: false,
        };
        // Registers written so far; a call clobbers the caller-saved ones after its delay slot.
        let mut written: u32 = 1;
        let mut arg_read: u32 = 0;
        let mut clobber_after = false;
        // Where `$v0` was last written by the function's own code, read, and where a call last was.
        let (mut v0_write, mut v0_read, mut last_call) = (None, None, None);
        // Which write of each base register the accesses off it belong to.
        let mut instance = [0u32; 32];
        // The argument register a register's value came from (`move $s0, $a0`).
        let mut origin: [Option<u32>; 32] = [None; 32];
        // Registers holding an address built from constants (`lui`, then `addiu`/`ori`):
        // what is loaded off them is a global, not a structure's field.
        let mut constant = [false; 32];
        let mut groups: Vec<((u32, u32), Option<u32>, Vec<Field>)> = Vec::new();
        for (n, &w) in words.iter().enumerate() {
            let r = reads(w);
            if r & (1 << 2) != 0 {
                v0_read = Some(n);
            }
            for a in 4..8 {
                if r & (1 << a) != 0 && written & (1 << a) == 0 {
                    arg_read |= 1 << a;
                }
            }
            match (w.op(), w.funct()) {
                (3, _) | (0, 9) => {
                    sig.calls += 1;
                    sig.leaf = false;
                    last_call = Some(n);
                }
                (9, _) if w.rs() == 29 && w.rt() == 29 && w.simm() < 0 && sig.frame == 0 => {
                    sig.frame = (-w.simm()) as u32;
                }
                (17, _) | (49, _) | (53, _) | (57, _) | (61, _) => sig.uses_float = true,
                (18, _) | (50, _) | (58, _) => sig.uses_cop2 = true,
                _ => {}
            }
            if let Some((width, store)) = access(w) {
                let (base, off) = (w.rs(), w.simm() as i32);
                if base == 29 {
                    if store && n < 24 && (16..32).contains(&w.rt()) && (off as u32) < sig.frame.max(1) {
                        let name = REGS[w.rt() as usize].to_string();
                        if !sig.saved.contains(&name) {
                            sig.saved.push(name);
                        }
                    }
                    // An argument past the registers: above this frame and the 16 bytes for $a0-$a3.
                    if !store && off >= 0 && (off as u32) >= sig.frame + 0x10 {
                        let k = ((off as u32) - sig.frame - 0x10) / 4;
                        sig.stack_args = sig.stack_args.max(k + 1);
                    }
                } else if base != 0 && base != 28 && !constant[base as usize] {
                    let key = (base, instance[base as usize]);
                    let from = if (4..8).contains(&base) && written & (1 << base) == 0 {
                        Some(base)
                    } else {
                        origin[base as usize]
                    };
                    let fields = match groups.iter_mut().find(|g| g.0 == key) {
                        Some(g) => &mut g.2,
                        None => {
                            groups.push((key, from, Vec::new()));
                            &mut groups.last_mut().unwrap().2
                        }
                    };
                    let mode = if store { "w" } else { "r" };
                    match fields.iter_mut().find(|f| f.offset == off && f.width == width) {
                        Some(f) if !f.access.contains(mode) => f.access = "rw".into(),
                        Some(_) => {}
                        None => fields.push(Field {
                            offset: off,
                            width,
                            access: mode.into(),
                        }),
                    }
                }
            }
            if let Some(reg) = w.writes() {
                if reg == 2 && !(w.op() == 3 || (w.op() == 0 && w.funct() == 9)) {
                    v0_write = Some(n);
                }
                origin[reg as usize] = if w.op() == 0 && matches!(w.funct(), 33 | 37) && w.rt() == 0 {
                    let rs = w.rs();
                    if (4..8).contains(&rs) && written & (1 << rs) == 0 {
                        Some(rs)
                    } else {
                        origin[rs as usize]
                    }
                } else {
                    None
                };
                constant[reg as usize] = match w.op() {
                    15 => true,
                    9 | 13 => constant[w.rs() as usize],
                    _ => false,
                };
                written |= 1 << reg;
                instance[reg as usize] += 1;
            }
            if clobber_after {
                written |= crate::cpu::mips::CALL_CLOBBERS;
                for a in 2..16 {
                    instance[a] += 1;
                    origin[a] = None;
                    constant[a] = false;
                }
                clobber_after = false;
            }
            if w.op() == 3 || (w.op() == 0 && w.funct() == 9) {
                clobber_after = true;
            }
        }
        // A result: `$v0` set after the last call, and not used again by the function itself.
        sig.returns = v0_write.is_some_and(|w| w > last_call.unwrap_or(0) && v0_read.is_none_or(|r| w >= r));
        sig.register_args = (4..8).rev().find(|&a| arg_read & (1 << a) != 0).map_or(0, |a| a - 3);
        if sig.stack_args > 0 {
            sig.register_args = 4;
        }
        let mut seen: Vec<u32> = Vec::new();
        for ((base, _), from, mut fields) in groups {
            let arg = from.filter(|&a| arg_read & (1 << a) != 0);
            if fields.len() < 2 && arg.is_none() {
                continue;
            }
            fields.sort_by_key(|f| (f.offset, f.width));
            let name = match arg {
                Some(a) => REGS[a as usize].trim_start_matches('$').to_string(),
                None => {
                    let nth = seen.iter().filter(|&&b| b == base).count();
                    seen.push(base);
                    if nth == 0 {
                        REGS[base as usize].to_string()
                    } else {
                        format!("{}#{}", REGS[base as usize], nth + 1)
                    }
                }
            };
            // The same argument through another register (`move $s0, $a0`): one structure.
            match sig.accesses.iter_mut().find(|a| a.base == name) {
                Some(a) => {
                    for f in fields {
                        match a.fields.iter_mut().find(|g| g.offset == f.offset && g.width == f.width) {
                            Some(g) if g.access != f.access => g.access = "rw".into(),
                            Some(_) => {}
                            None => a.fields.push(f),
                        }
                    }
                    a.fields.sort_by_key(|f| (f.offset, f.width));
                }
                None => sig.accesses.push(Access { base: name, fields }),
            }
        }
        // o32 returns a structure through a pointer passed as the first argument and handed back in $v0.
        sig.returns_struct = sig.returns
            && sig.register_args >= 1
            && sig
                .accesses
                .iter()
                .find(|a| a.base == "a0")
                .is_some_and(|a| a.fields.len() >= 2 && a.fields.iter().all(|f| f.access == "w"));
        let mut args: Vec<String> = (0..sig.register_args).map(|i| format!("int a{i}")).collect();
        args.extend((0..sig.stack_args).map(|i| format!("int a{}", 4 + i)));
        if sig.returns_struct {
            args[0] = "struct *ret".into();
        }
        sig.prototype = format!(
            "{} {}({})",
            if sig.returns_struct { "struct *" } else if sig.returns { "int" } else { "void" },
            sig.name,
            if args.is_empty() { "void".to_string() } else { args.join(", ") }
        );
        Some(sig)
    }
}

impl Binary {
    /// [`Self::function_signature`] for x86 code: from its stack frame.
    fn x86_signature(&self, address: u64) -> Option<FunctionSignature> {
        use crate::stack::Convention;
        let f = self.symbols.function_containing(address)?;
        let frame = self.stack_frame(f.address)?;
        let name = f.display_name().into_owned();
        let pointer = |arg: &str| frame.accesses.iter().any(|a| a.base == arg);
        let mut args: Vec<String> = Vec::new();
        match frame.convention {
            Convention::Thiscall => args.push("void *this".into()),
            _ => {
                for i in 0..frame.registers.len() {
                    let arg = format!("arg{}", i + 1);
                    args.push(format!("{}{arg}", if pointer(&arg) { "void *" } else { "int " }));
                }
            }
        }
        // Win64 callers pass four arguments in registers before any on the stack.
        let first = match frame.convention {
            Convention::Win64 if frame.stack_args > 0 => {
                while args.len() < 4 {
                    args.push(format!("int arg{}", args.len() + 1));
                }
                5
            }
            Convention::SysV if frame.stack_args > 0 => {
                while args.len() < 6 {
                    args.push(format!("int arg{}", args.len() + 1));
                }
                7
            }
            Convention::Thiscall => 1,
            _ => frame.registers.len() + 1,
        };
        for (i, a) in frame.args.iter().enumerate() {
            let arg = format!("arg{}", first + i);
            let ty = match (a.float, a.size) {
                (true, 4) => "float ",
                (true, _) => "double ",
                _ if pointer(&arg) => "void *",
                _ => "int ",
            };
            args.push(format!("{ty}{arg}"));
        }
        // An argument on the x87 stack comes first (MSVC's _CIsqrt and its kind take theirs there).
        if frame.fpu_argument {
            args.insert(0, "double st0".into());
        }
        let keyword = match frame.convention {
            Convention::Win64 | Convention::SysV => String::new(),
            c => format!("__{} ", c.as_str()),
        };
        let result = if frame.returns_float {
            "double"
        } else if frame.returns {
            "int"
        } else {
            "void"
        };
        let prototype = format!(
            "{result} {keyword}{name}({})",
            if args.is_empty() { "void".to_string() } else { args.join(", ") }
        );
        Some(FunctionSignature {
            address: f.address,
            name,
            register_args: frame.registers.len() as u32,
            stack_args: frame.stack_args,
            returns: frame.returns,
            frame: frame.locals,
            saved: frame.saved.iter().map(|s| s.0.to_string()).collect(),
            leaf: frame.calls == 0,
            calls: frame.calls,
            uses_cop2: false,
            uses_float: frame.uses_float,
            accesses: frame.accesses.clone(),
            prototype,
            returns_struct: false,
            convention: Some(frame.convention.as_str().into()),
            pops: frame.pops,
            registers: frame.registers.iter().map(|r| r.to_string()).collect(),
            frame_pointer: frame.frame_pointer,
            aligned: frame.aligned,
            returns_float: frame.returns_float,
            unbalanced: frame.unbalanced,
        })
    }
}

impl FunctionSignature {
    /// The signature as a few lines of text.
    pub fn describe(&self) -> String {
        if let Some(convention) = &self.convention {
            return self.describe_x86(convention);
        }
        let mut out = format!("{}\n", self.prototype);
        out.push_str(&format!(
            "  frame {} bytes, saves [{}], {}{}{}\n",
            self.frame,
            self.saved.join(", "),
            if self.leaf {
                "leaf".to_string()
            } else {
                format!("{} calls", self.calls)
            },
            if self.uses_cop2 { ", uses cop2 (GTE)" } else { "" },
            if self.uses_float { ", uses the FPU" } else { "" },
        ));
        if self.returns_struct {
            out.push_str("  returns a structure by value: a0 is the hidden pointer it is built in, handed back in $v0
");
        }
        self.describe_accesses(&mut out);
        out
    }

    /// The offsets walked off each base register, a line each.
    fn describe_accesses(&self, out: &mut String) {
        for a in &self.accesses {
            let fields: Vec<String> = a
                .fields
                .iter()
                .map(|f| {
                    let width = match f.width {
                        1 => "u8".to_string(),
                        2 => "u16".into(),
                        4 => "u32".into(),
                        8 => "u64".into(),
                        n => format!("{n}b"),
                    };
                    format!("{:#x}:{width}{}", f.offset, f.access)
                })
                .collect();
            out.push_str(&format!("  {} -> {{{}}}\n", a.base, fields.join(", ")));
        }
    }

    /// [`Self::describe`] for x86: the convention, what the function pops,
    /// its frame and what it saves.
    fn describe_x86(&self, convention: &str) -> String {
        let mut out = format!("{}\n", self.prototype);
        let how = match convention {
            "cdecl" => "cdecl: the caller pops the arguments".to_string(),
            "stdcall" => format!("stdcall: pops {} bytes of arguments (ret {})", self.pops, self.pops),
            "thiscall" => format!("thiscall: this in ecx{}", pops_text(self.pops)),
            "fastcall" => format!(
                "fastcall: {} then the stack{}",
                if self.registers.is_empty() { "ecx, edx".to_string() } else { self.registers.join(", ") },
                pops_text(self.pops)
            ),
            "win64" => format!("Microsoft x64: {}", self.arguments_text()),
            "sysv" => format!("System V x86-64: {}", self.arguments_text()),
            other => other.to_string(),
        };
        out.push_str(&format!(
            "  {how}; frame {} bytes{}{}, saves [{}], {}{}{}\n",
            self.frame,
            if self.frame_pointer { " (ebp frame)" } else { "" },
            self.aligned.map_or(String::new(), |a| format!(", aligned to {a}")),
            self.saved.join(", "),
            match self.calls {
                0 => "leaf".to_string(),
                1 => "1 call".to_string(),
                n => format!("{n} calls"),
            },
            if self.returns_float {
                ", returns on the FPU stack"
            } else if self.uses_float {
                ", uses floating point"
            } else {
                ""
            },
            if self.unbalanced {
                "; the stack doesn't add up at every return (a call through a pointer that pops its own arguments?)"
            } else {
                ""
            },
        ));
        self.describe_accesses(&mut out);
        out
    }
}

fn pops_text(pops: u32) -> String {
    if pops > 0 { format!("; pops {pops} bytes (ret {pops})") } else { String::new() }
}

impl FunctionSignature {
    /// Where an x86-64 function's arguments come from, as far as it reads them.
    fn arguments_text(&self) -> String {
        match (self.registers.is_empty(), self.stack_args) {
            (true, 0) => "reads no arguments".into(),
            (false, 0) => format!("arguments in {}", self.registers.join(", ")),
            (_, n) => format!(
                "arguments in registers{}, and {n} on the stack",
                if self.registers.is_empty() { String::new() } else { format!(" ({})", self.registers.join(", ")) }
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// A PS-X EXE holding `words` at 0x80010000, its entry there.
    fn exe(words: &[u32]) -> Binary {
        let mut data = vec![0u8; 0x800];
        data[..8].copy_from_slice(b"PS-X EXE");
        for (at, v) in [(0x10, 0x8001_0000u32), (0x14, 0x8001_8000), (0x18, 0x8001_0000)] {
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        let code = le(words);
        data[0x1C..0x20].copy_from_slice(&(code.len() as u32).to_le_bytes());
        data.extend(code);
        Binary::parse(data).unwrap()
    }

    #[test]
    fn mips_signature() {
        // addiu $sp, $sp, -0x20 / sw $ra, 0x14($sp) / sw $s0, 0x10($sp) / move $s0, $a0 /
        // lw $v0, 0x30($sp) (the fifth argument) / lw $t0, 4($s0) / sh $t0, 8($s0) /
        // jal 0x80010034 / move $a0, $a1 / lw $ra, 0x14($sp) / lw $s0, 0x10($sp) /
        // jr $ra / addiu $sp, $sp, 0x20; then the callee: jr $ra / li $v0, 1
        let b = exe(&[
            0x27BD_FFE0,
            0xAFBF_0014,
            0xAFB0_0010,
            0x0080_8021,
            0x8FA2_0030,
            0x8E08_0004,
            0xA608_0008,
            0x0C00_400D,
            0x00A0_2021,
            0x8FBF_0014,
            0x8FB0_0010,
            0x03E0_0008,
            0x27BD_0020,
            0x03E0_0008,
            0x2402_0001,
        ]);
        let s = b.function_signature(0x8001_0000).unwrap();
        assert_eq!(s.prototype, "void entry(int a0, int a1, int a2, int a3, int a4)");
        assert_eq!((s.frame, s.leaf, s.calls, s.stack_args), (0x20, false, 1, 1));
        assert_eq!(s.saved, ["$ra", "$s0"]);
        assert_eq!(s.accesses.len(), 1);
        assert_eq!(s.accesses[0].base, "a0");
        let fields: Vec<(i32, u8, &str)> = s.accesses[0]
            .fields
            .iter()
            .map(|f| (f.offset, f.width, f.access.as_str()))
            .collect();
        assert_eq!(fields, [(4, 4, "r"), (8, 2, "w")]);
        // The callee returns 1 and takes nothing.
        let c = b.function_signature(0x8001_0034).unwrap();
        assert_eq!((c.returns, c.leaf, c.register_args), (true, true, 0));
        assert!(c.prototype.starts_with("int "));
    }
}
