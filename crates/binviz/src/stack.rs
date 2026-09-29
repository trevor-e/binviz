//! Where x86 code keeps its arguments and locals: the stack pointer followed
//! from a function's entry, where `[esp]` holds the return address and
//! `[esp+4]` the first argument, through every path of the function, so that
//! each `[esp+N]` and `[ebp±N]` reads as the same argument or local wherever
//! it is. Pushes move `esp` before each call, and MSVC merges the `add esp, N`
//! that pops the arguments of several calls into one, so the same `[esp+8]`
//! names different things between them; relative to the entry, it doesn't.
//!
//! A call pops what the callee pops: `ret N` in its code (stdcall, thiscall
//! and fastcall callees clean up after themselves), for an imported function
//! the arguments pushed for it (Windows' own DLLs are stdcall; the C
//! runtime's are cdecl), and for a call through a pointer nothing, unless
//! the stack only adds up at the returns if it did.
//!
//! What the function reads tells its calling convention: `ret N` (the callee
//! pops), `ecx` and `edx` read before they are written (thiscall passes
//! `this` in `ecx`, fastcall the first two arguments in `ecx` and `edx`), and
//! for cdecl the highest argument it reads. In x86-64 code the arguments
//! are in registers first (`rcx`, `rdx`, `r8`, `r9` for Windows; `rdi`,
//! `rsi`, `rdx`, `rcx`, `r8`, `r9` elsewhere), then on the stack.

use std::collections::{BTreeMap, HashMap};

use iced_x86::{
    Decoder, DecoderOptions, FlowControl, Instruction, InstructionInfoFactory, Mnemonic, OpAccess, OpKind, Register,
};

use crate::binary::Binary;
use crate::model::{Format, SymbolSource};
use crate::signature::{Access, Field};

/// Where the stack pointer is, relative to a point that doesn't move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sp {
    /// Relative to the function's entry, where it is 0.
    Entry(i64),
    /// Relative to where `and esp, -N` aligned it, somewhere below the entry.
    Aligned(i64),
    Unknown,
}

impl Sp {
    fn add(self, n: i64) -> Sp {
        match self {
            Sp::Entry(v) => Sp::Entry(v + n),
            Sp::Aligned(v) => Sp::Aligned(v + n),
            Sp::Unknown => Sp::Unknown,
        }
    }
}

/// How x86 code expects to be called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Convention {
    Cdecl,
    Stdcall,
    Fastcall,
    Thiscall,
    /// x86-64, Microsoft's: `rcx`, `rdx`, `r8`, `r9`, then the stack above 32 bytes of home space.
    Win64,
    /// x86-64, the System V ABI: `rdi`, `rsi`, `rdx`, `rcx`, `r8`, `r9`, then the stack.
    SysV,
}

impl Convention {
    pub fn as_str(self) -> &'static str {
        match self {
            Convention::Cdecl => "cdecl",
            Convention::Stdcall => "stdcall",
            Convention::Fastcall => "fastcall",
            Convention::Thiscall => "thiscall",
            Convention::Win64 => "win64",
            Convention::SysV => "sysv",
        }
    }
}

/// What a value in a register came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// An argument on the stack, at this offset from the entry.
    Stack(i64),
    /// The argument register with this number (`ecx` is 1), as it was at the entry.
    Register(usize),
}

/// One path's state at an instruction.
#[derive(Debug, Clone)]
struct State {
    sp: Sp,
    /// The frame pointer (`ebp` after `mov ebp, esp`), while it is one.
    fp: Sp,
    /// Registers written on this path, by number.
    written: u32,
    origin: [Option<Origin>; 16],
    /// Which write of each register its value is, for grouping the offsets walked off it.
    instance: [u32; 16],
    /// Bytes pushed since the last call: the arguments of the next one.
    pushed: i64,
    /// A constant in `eax` (`mov eax, 0x1000` before `call __chkstk`).
    eax: Option<i64>,
    /// Values on the x87 stack.
    fpu: i32,
    /// `eax` was set by the function's own code since the last call.
    result: bool,
    /// Registers holding an address the code names outright (`mov ecx, offset table`):
    /// what is read off them is a global's, not a structure's.
    constant: u32,
}

/// What a callee does to its caller's stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pops {
    /// It pops this many bytes of arguments.
    Bytes(i64),
    /// It pops the arguments pushed for it (a stdcall import).
    Pushed,
    /// It moves the stack pointer down by the number in `eax` (`__chkstk`, `_alloca_probe`).
    Alloca,
    /// Not known: a call through a pointer.
    Unknown,
}

/// A function's stack frame and what its code says about how it is called.
#[derive(Debug, Clone)]
pub(crate) struct Frame {
    pub bits: u32,
    pub convention: Convention,
    /// Bytes of arguments `ret N` pops.
    pub pops: u32,
    /// Argument registers read before they are written, in argument order (`ecx`, `edx`).
    pub registers: Vec<&'static str>,
    /// Arguments passed on the stack: those the callee pops, or (cdecl) up to the highest one read.
    pub stack_args: u32,
    /// Bytes the prologue reserves for locals.
    pub locals: u32,
    /// Registers saved on entry and where (offset from the entry).
    pub saved: Vec<(&'static str, i64)>,
    pub frame_pointer: bool,
    /// The stack pointer is aligned to this many bytes (`and esp, -16`).
    pub aligned: Option<u32>,
    pub returns: bool,
    /// The result is left on the x87 stack (a `float` or `double`).
    pub returns_float: bool,
    pub calls: u32,
    pub uses_float: bool,
    /// Some return isn't reached with the stack pointer where it started.
    pub unbalanced: bool,
    /// The stack slot each instruction uses: (instruction, slot).
    pub slots: Vec<(u64, Slot)>,
    /// Offsets walked off each base register.
    pub accesses: Vec<Access>,
}

/// A place in the stack frame an instruction reads or writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Slot {
    pub at: Sp,
    /// The instruction takes its address (`lea`) rather than using it.
    pub address: bool,
}

const MAX_INSTRUCTIONS: usize = 20_000;

/// The x86 register numbers (`Register::number` of the 64-bit register).
const EAX: usize = 0;
const ECX: usize = 1;
const EDX: usize = 2;
const EBX: usize = 3;
const ESP: usize = 4;
const EBP: usize = 5;
const ESI: usize = 6;
const EDI: usize = 7;

fn reg32_name(n: usize) -> &'static str {
    ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"][n & 7]
}

fn reg64_name(n: usize) -> &'static str {
    [
        "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15",
    ][n & 15]
}

/// The general register a register is part of, by number.
fn gpr(r: Register) -> Option<usize> {
    (r.is_gpr()).then(|| r.full_register().number() & 15)
}

/// Libraries whose functions are cdecl: the C runtimes. Windows' own are stdcall.
fn cdecl_library(library: &str) -> bool {
    let l = library.to_ascii_lowercase();
    l.starts_with("msvcr")
        || l.starts_with("msvcp")
        || l.starts_with("ucrtbase")
        || l.starts_with("api-ms-win-crt")
        || l.starts_with("vcruntime")
        || l.starts_with("libc")
        || l.starts_with("crtdll")
}

/// Imported functions that take a variable number of arguments, so their callers pop them.
fn varargs(name: &str) -> bool {
    matches!(
        name,
        "wsprintfA" | "wsprintfW" | "DbgPrint" | "DbgPrintEx" | "_snprintf" | "sprintf" | "printf"
    )
}

impl Binary {
    /// The stack frame of the x86 function containing `address`, and the
    /// calling convention its code implies.
    pub(crate) fn stack_frame(&self, address: u64) -> Option<Frame> {
        let bits = match self.arch {
            object::Architecture::I386 => 32,
            object::Architecture::X86_64 => 64,
            _ => return None,
        };
        let f = self.symbols.function_containing(address)?;
        let (start, end) = (f.address, f.address + f.size.max(1));
        let mut frame = self.walk(bits, start, end, false);
        // A call through a pointer that pops its own arguments (a COM method,
        // a stdcall callback) leaves the stack lower than we think; if assuming
        // they all do makes every return add up, they do.
        if frame.unbalanced {
            let again = self.walk(bits, start, end, true);
            if !again.unbalanced {
                frame = again;
            }
        }
        Some(frame)
    }

    /// Follows every path of the function at `start..end`.
    fn walk(&self, bits: u32, start: u64, end: u64, indirect_pops: bool) -> Frame {
        let word = (bits / 8) as i64;
        let mut info = InstructionInfoFactory::new();
        let mut seen: HashMap<u64, ()> = HashMap::new();
        let first = State {
            sp: Sp::Entry(0),
            fp: Sp::Unknown,
            written: 0,
            origin: [None; 16],
            instance: [0; 16],
            pushed: 0,
            eax: None,
            fpu: 0,
            result: false,
            constant: 0,
        };
        let mut paths = vec![(start, first)];
        let mut read_first: u32 = 0;
        // `ecx` used as a base register before it was written: an object's `this`.
        let mut ecx_base = false;
        let mut pops: Option<i64> = None;
        let mut max_arg = 0i64;
        let mut locals = 0i64;
        let mut saved: Vec<(&'static str, i64)> = Vec::new();
        let mut frame_pointer = false;
        let mut aligned = None;
        let (mut returns, mut returns_float, mut unbalanced) = (false, false, false);
        let mut calls = 0u32;
        let mut uses_float = false;
        let mut slots = Vec::new();
        let mut groups: BTreeMap<(usize, u32), (Option<Origin>, Vec<Field>)> = BTreeMap::new();
        let mut budget = MAX_INSTRUCTIONS;
        while let Some((pc, mut st)) = paths.pop() {
            let mut pc = pc;
            loop {
                if pc < start || pc >= end || seen.insert(pc, ()).is_some() || budget == 0 {
                    break;
                }
                budget -= 1;
                let Some(ins) = self.decode_x86(bits, pc) else { break };
                let next = ins.next_ip();
                let (regs, stores) = {
                    let used = info.info(&ins);
                    let regs: Vec<(Register, OpAccess)> = used
                        .used_registers()
                        .iter()
                        .map(|u| (u.register(), u.access()))
                        .collect();
                    (regs, used.used_memory().iter().any(|m| writes(m.access())))
                };
                // Registers read before this path wrote them.
                let zeroing = matches!(ins.mnemonic(), Mnemonic::Xor | Mnemonic::Sub)
                    && ins.op_count() == 2
                    && ins.op0_kind() == OpKind::Register
                    && ins.op1_kind() == OpKind::Register
                    && ins.op0_register() == ins.op1_register();
                let push_only = ins.mnemonic() == Mnemonic::Push && ins.op0_kind() == OpKind::Register;
                if !zeroing && !push_only {
                    for &(r, access) in &regs {
                        let Some(n) = gpr(r) else { continue };
                        if reads(access) && st.written & (1 << n) == 0 && n != ESP {
                            read_first |= 1 << n;
                        }
                    }
                }
                // eax used after it was set (as a base, stored, pushed) was scratch, not the result;
                // a comparison only looks at it.
                if !zeroing
                    && !matches!(ins.mnemonic(), Mnemonic::Cmp | Mnemonic::Test)
                    && regs.iter().any(|&(r, access)| gpr(r) == Some(EAX) && reads(access))
                {
                    st.result = false;
                }
                if bits == 32
                    && (0..ins.op_count()).any(|i| ins.op_kind(i) == OpKind::Memory)
                    && ins.memory_base() == Register::ECX
                    && st.written & (1 << ECX) == 0
                {
                    ecx_base = true;
                }
                if regs.iter().any(|&(r, _)| r.is_st() || r.is_xmm() || r.is_ymm()) {
                    uses_float = true;
                }
                // Values on the x87 stack; a callee's float result isn't counted, so never below none.
                let fpu = ins.fpu_stack_increment_info();
                if fpu.writes_top() {
                    st.fpu = (st.fpu - fpu.increment()).max(0);
                }
                // The stack slot it uses.
                let memory = (0..ins.op_count()).any(|i| ins.op_kind(i) == OpKind::Memory);
                if memory {
                    let disp = if bits == 32 {
                        ins.memory_displacement32() as i32 as i64
                    } else {
                        ins.memory_displacement64() as i64
                    };
                    let base = match ins.memory_base() {
                        Register::ESP | Register::RSP => Some(st.sp),
                        Register::EBP | Register::RBP if st.fp != Sp::Unknown => Some(st.fp),
                        _ => None,
                    };
                    if let Some(at) = base.map(|b| b.add(disp)).filter(|s| *s != Sp::Unknown) {
                        // Past the return address (and, in x86-64, past what a push-less call of it wrote).
                        let lea = ins.mnemonic() == Mnemonic::Lea;
                        slots.push((pc, Slot { at, address: lea }));
                        if let Sp::Entry(off) = at
                            && off >= word
                            && !lea
                        {
                            max_arg = max_arg.max(off + ins.memory_size().size().max(1) as i64);
                        }
                        // A register loaded from an argument holds that argument.
                        if let (Sp::Entry(off), Mnemonic::Mov | Mnemonic::Movzx | Mnemonic::Movsx) =
                            (at, ins.mnemonic())
                            && off >= word
                            && ins.op0_kind() == OpKind::Register
                            && let Some(n) = gpr(ins.op0_register())
                            && n != ESP
                            && n != EBP
                        {
                            write(&mut st, n);
                            st.origin[n] = Some(Origin::Stack(off));
                            if n == EAX {
                                st.result = true;
                            }
                            pc = next;
                            continue;
                        }
                    } else if let Some(b) = gpr(ins.memory_base())
                        && b != ESP
                        && !(b == EBP && st.fp != Sp::Unknown)
                        && st.constant & (1 << b) == 0
                    {
                        // Memory walked off a base register: a structure, perhaps.
                        let from = if st.written & (1 << b) == 0 && is_argument_register(bits, self.summary.format, b) {
                            Some(Origin::Register(b))
                        } else {
                            st.origin[b]
                        };
                        let width = ins.memory_size().size();
                        if ins.mnemonic() != Mnemonic::Lea && (1..=16).contains(&width) {
                            let (store, width) = (stores, width as u8);
                            let fields = &mut groups.entry((b, st.instance[b])).or_insert((from, Vec::new())).1;
                            let mode = if store { "w" } else { "r" };
                            let offset = disp as i32;
                            match fields.iter_mut().find(|f| f.offset == offset && f.width == width) {
                                Some(f) if !f.access.contains(mode) => f.access = "rw".into(),
                                Some(_) => {}
                                None => fields.push(Field {
                                    offset,
                                    width,
                                    access: mode.into(),
                                }),
                            }
                        }
                    }
                }
                // What it does to the stack pointer, the frame pointer and the registers.
                let flow = ins.flow_control();
                match flow {
                    FlowControl::Call | FlowControl::IndirectCall => {
                        calls += 1;
                        let callee = self.callee_pops(bits, &ins);
                        let popped = match callee {
                            Pops::Bytes(n) => n,
                            Pops::Pushed => st.pushed.max(0),
                            Pops::Unknown if indirect_pops => st.pushed.max(0),
                            Pops::Unknown => 0,
                            Pops::Alloca => -st.eax.unwrap_or(0),
                        };
                        if callee == Pops::Alloca {
                            locals = locals.max(st.eax.unwrap_or(0));
                        }
                        st.sp = st.sp.add(popped);
                        st.pushed = 0;
                        // What the callee may change.
                        let clobbered: &[usize] = match bits {
                            32 => &[EAX, ECX, EDX],
                            _ if matches!(self.summary.format, Format::Pe | Format::Coff) => {
                                &[EAX, ECX, EDX, 8, 9, 10, 11]
                            }
                            _ => &[EAX, ECX, EDX, ESI, EDI, 8, 9, 10, 11],
                        };
                        for &r in clobbered {
                            write(&mut st, r);
                        }
                        st.eax = None;
                        st.result = false;
                        pc = next;
                        continue;
                    }
                    FlowControl::Return => {
                        let n = if ins.op_count() > 0 && ins.op0_kind() == OpKind::Immediate16 {
                            ins.immediate16() as i64
                        } else {
                            0
                        };
                        pops = Some(pops.map_or(n, |p| p.max(n)));
                        if st.sp != Sp::Entry(0) && st.sp != Sp::Unknown {
                            unbalanced = true;
                        }
                        returns |= st.result;
                        returns_float |= st.fpu > 0;
                        break;
                    }
                    FlowControl::Exception | FlowControl::Interrupt if ins.mnemonic() != Mnemonic::Int => break,
                    _ => {}
                }
                if ins.mnemonic() == Mnemonic::Hlt {
                    break;
                }
                match self.sp_change(&ins, &st, bits) {
                    Some(new) => {
                        // The first `sub esp, N` makes room for the locals; the arguments of the next call come after.
                        let reserved = match (st.sp, new) {
                            (Sp::Entry(old), Sp::Entry(now)) | (Sp::Aligned(old), Sp::Aligned(now)) => old - now,
                            _ => 0,
                        };
                        if reserved > 0 && ins.mnemonic() == Mnemonic::Sub {
                            if locals == 0 {
                                locals = reserved;
                            }
                            st.pushed = 0;
                        }
                        if ins.mnemonic() == Mnemonic::And {
                            aligned = Some((immediate(&ins, 1, bits) as u32).wrapping_neg());
                        }
                        st.sp = new;
                    }
                    None => {
                        let inc = ins.stack_pointer_increment() as i64;
                        if inc != 0 && ins.mnemonic() != Mnemonic::Leave {
                            // A register saved: one the function must give back, pushed before it wrote it.
                            let save = (ins.mnemonic() == Mnemonic::Push && ins.op0_kind() == OpKind::Register)
                                .then(|| gpr(ins.op0_register()))
                                .flatten()
                                .filter(|&n| {
                                    st.written & (1 << n) == 0 && is_callee_saved(bits, self.summary.format, n)
                                });
                            match (save, st.sp.add(inc)) {
                                (Some(n), Sp::Entry(v)) => {
                                    if !saved.iter().any(|s| s.0 == name_of(bits, n)) {
                                        saved.push((name_of(bits, n), v));
                                    }
                                }
                                // Anything else pushed is an argument of the next call.
                                _ if inc < 0 => st.pushed -= inc,
                                _ => st.pushed = (st.pushed - inc).max(0),
                            }
                            st.sp = st.sp.add(inc);
                        }
                    }
                }
                // The frame pointer.
                match ins.mnemonic() {
                    Mnemonic::Mov
                        if ins.op0_kind() == OpKind::Register
                            && matches!(ins.op0_register(), Register::EBP | Register::RBP)
                            && ins.op1_kind() == OpKind::Register
                            && matches!(ins.op1_register(), Register::ESP | Register::RSP) =>
                    {
                        st.fp = st.sp;
                        frame_pointer = true;
                    }
                    Mnemonic::Lea
                        if ins.op0_kind() == OpKind::Register
                            && matches!(ins.op0_register(), Register::EBP | Register::RBP)
                            && matches!(ins.memory_base(), Register::ESP | Register::RSP)
                            && ins.memory_index() == Register::None =>
                    {
                        let disp = if bits == 32 {
                            ins.memory_displacement32() as i32 as i64
                        } else {
                            ins.memory_displacement64() as i64
                        };
                        st.fp = st.sp.add(disp);
                        frame_pointer = true;
                    }
                    Mnemonic::Leave => {
                        st.sp = st.fp.add(word);
                        st.fp = Sp::Unknown;
                    }
                    _ => {
                        // Any other write to ebp makes it a register like the others.
                        if regs
                            .iter()
                            .any(|&(r, access)| matches!(r, Register::EBP | Register::RBP) && writes(access))
                        {
                            st.fp = Sp::Unknown;
                        }
                    }
                }
                // Registers written, and where their values came from.
                let copy = (ins.mnemonic() == Mnemonic::Mov
                    && ins.op0_kind() == OpKind::Register
                    && ins.op1_kind() == OpKind::Register)
                    .then(|| (gpr(ins.op0_register()), gpr(ins.op1_register())));
                let before = st.clone();
                for &(r, access) in &regs {
                    let Some(n) = gpr(r) else { continue };
                    if n == ESP || !writes(access) {
                        continue;
                    }
                    write(&mut st, n);
                    if n == EAX {
                        st.result = true;
                        st.eax = (ins.mnemonic() == Mnemonic::Mov && is_immediate(ins.op1_kind()))
                            .then(|| immediate(&ins, 1, bits));
                    }
                }
                // A register set to a number, an address the code names, or one worked out from those.
                let constant_in = |r: Register| gpr(r).is_some_and(|m| before.constant & (1 << m) != 0);
                let constant = match ins.mnemonic() {
                    _ if zeroing => true,
                    Mnemonic::Mov if is_immediate(ins.op1_kind()) => true,
                    Mnemonic::Mov if ins.op1_kind() == OpKind::Register => constant_in(ins.op1_register()),
                    Mnemonic::Lea => {
                        ins.is_ip_rel_memory_operand()
                            || ((ins.memory_base() == Register::None || constant_in(ins.memory_base()))
                                && (ins.memory_index() == Register::None || constant_in(ins.memory_index())))
                    }
                    Mnemonic::Add | Mnemonic::Sub | Mnemonic::Inc | Mnemonic::Dec | Mnemonic::Shl | Mnemonic::And => {
                        ins.op0_kind() == OpKind::Register
                            && constant_in(ins.op0_register())
                            && (ins.op_count() == 1 || is_immediate(ins.op1_kind()))
                    }
                    _ => false,
                };
                if constant
                    && ins.op0_kind() == OpKind::Register
                    && let Some(n) = gpr(ins.op0_register())
                {
                    st.constant |= 1 << n;
                }
                if let Some((Some(dst), Some(src))) = copy {
                    st.origin[dst] =
                        if before.written & (1 << src) == 0 && is_argument_register(bits, self.summary.format, src) {
                            Some(Origin::Register(src))
                        } else {
                            before.origin[src]
                        };
                }
                match flow {
                    FlowControl::UnconditionalBranch => {
                        match near_target(&ins) {
                            Some(t) if t >= start && t < end => paths.push((t, st.clone())),
                            // A tail call: the stack must be back where it started.
                            Some(_) => {
                                if st.sp != Sp::Entry(0) && st.sp != Sp::Unknown {
                                    unbalanced = true;
                                }
                            }
                            None => {}
                        }
                        break;
                    }
                    FlowControl::IndirectBranch => {
                        for t in self.switch_targets(pc) {
                            paths.push((t, st.clone()));
                        }
                        break;
                    }
                    FlowControl::ConditionalBranch => {
                        if let Some(t) = near_target(&ins) {
                            paths.push((t, st.clone()));
                        }
                    }
                    _ => {}
                }
                pc = next;
            }
        }
        // The arguments: in registers, then on the stack.
        let convention = if bits == 64 {
            if matches!(self.summary.format, Format::Pe | Format::Coff) {
                Convention::Win64
            } else {
                Convention::SysV
            }
        } else if read_first & (1 << EDX) != 0 {
            Convention::Fastcall
        } else if read_first & (1 << ECX) != 0 {
            if ecx_base {
                Convention::Thiscall
            } else {
                Convention::Fastcall
            }
        } else if pops.unwrap_or(0) > 0 {
            Convention::Stdcall
        } else {
            Convention::Cdecl
        };
        let order: &[usize] = match convention {
            Convention::Win64 => &[ECX, EDX, 8, 9],
            Convention::SysV => &[EDI, ESI, EDX, ECX, 8, 9],
            Convention::Fastcall => &[ECX, EDX],
            Convention::Thiscall => &[ECX],
            _ => &[],
        };
        let count = order
            .iter()
            .rposition(|&r| read_first & (1 << r) != 0)
            .map_or(0, |i| i + 1);
        let registers: Vec<&'static str> = order[..count].iter().map(|&r| name_of(bits, r)).collect();
        let pops = pops.unwrap_or(0).max(0) as u32;
        let first_stack = match convention {
            Convention::Win64 => 5 * word,
            _ => word,
        };
        let read = ((max_arg - first_stack).max(0) + word - 1) / word;
        let stack_args = if pops > 0 { pops / word as u32 } else { read as u32 };
        slots.sort_by_key(|s: &(u64, Slot)| s.0);
        slots.dedup_by_key(|s| s.0);
        let mut frame = Frame {
            bits,
            convention,
            pops,
            registers,
            stack_args,
            locals: locals.max(0) as u32,
            saved,
            frame_pointer,
            aligned,
            returns: returns && !returns_float,
            returns_float,
            calls,
            uses_float,
            unbalanced,
            slots,
            accesses: Vec::new(),
        };
        // Offsets walked off each base register, named after the argument it holds.
        let mut seen_bases: Vec<usize> = Vec::new();
        for ((base, _), (from, mut fields)) in groups {
            let name = from.and_then(|o| frame.origin_name(o));
            if fields.len() < 2 && name.is_none() {
                continue;
            }
            fields.sort_by_key(|f| (f.offset, f.width));
            let name = name.unwrap_or_else(|| {
                let nth = seen_bases.iter().filter(|&&b| b == base).count();
                seen_bases.push(base);
                let r = name_of(bits, base);
                if nth == 0 {
                    r.to_string()
                } else {
                    format!("{r}#{}", nth + 1)
                }
            });
            match frame.accesses.iter_mut().find(|a| a.base == name) {
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
                None => frame.accesses.push(Access { base: name, fields }),
            }
        }
        frame
    }

    /// The instruction at `pc`, decoded.
    fn decode_x86(&self, bits: u32, pc: u64) -> Option<Instruction> {
        let offset = self.address_to_offset(pc)? as usize;
        let bytes = self.data.get(offset..(offset + 15).min(self.data.len()))?;
        let mut decoder = Decoder::with_ip(bits, bytes, pc, DecoderOptions::NONE);
        let ins = decoder.decode();
        (!ins.is_invalid()).then_some(ins)
    }

    /// The cases of the jump table the indirect jump at `pc` reads.
    fn switch_targets(&self, pc: u64) -> Vec<u64> {
        let Some(t) = self.code_tables.iter().find(|t| t.jump == pc && t.entry == 4) else {
            return Vec::new();
        };
        (0..t.count as u64)
            .filter_map(|i| {
                let o = self.address_to_offset(t.address + 4 * i)? as usize;
                Some(u32::from_le_bytes(self.data.get(o..o + 4)?.try_into().ok()?) as u64)
            })
            .collect()
    }

    /// Where an instruction that writes the stack pointer outright leaves it:
    /// `sub`/`add esp, N`, `lea esp, [...]`, `mov esp, ebp`, `and esp, -N`.
    /// None for the instructions that only push and pop.
    fn sp_change(&self, ins: &Instruction, st: &State, bits: u32) -> Option<Sp> {
        let sp = if bits == 32 { Register::ESP } else { Register::RSP };
        if ins.op_count() == 0 || ins.op0_kind() != OpKind::Register || ins.op0_register() != sp {
            return None;
        }
        let disp = || {
            if bits == 32 {
                ins.memory_displacement32() as i32 as i64
            } else {
                ins.memory_displacement64() as i64
            }
        };
        Some(match ins.mnemonic() {
            Mnemonic::Sub if is_immediate(ins.op1_kind()) => st.sp.add(-immediate(ins, 1, bits)),
            Mnemonic::Add if is_immediate(ins.op1_kind()) => st.sp.add(immediate(ins, 1, bits)),
            Mnemonic::Lea if matches!(ins.memory_base(), Register::ESP | Register::RSP) => st.sp.add(disp()),
            Mnemonic::Lea if matches!(ins.memory_base(), Register::EBP | Register::RBP) => st.fp.add(disp()),
            Mnemonic::Mov
                if ins.op1_kind() == OpKind::Register
                    && matches!(ins.op1_register(), Register::EBP | Register::RBP) =>
            {
                st.fp
            }
            Mnemonic::And if is_immediate(ins.op1_kind()) => Sp::Aligned(0),
            Mnemonic::Push | Mnemonic::Pop => return None,
            _ => Sp::Unknown,
        })
    }

    /// What the callee of a call instruction pops.
    fn callee_pops(&self, bits: u32, ins: &Instruction) -> Pops {
        if bits == 64 {
            return Pops::Bytes(0);
        }
        match ins.flow_control() {
            FlowControl::Call => self.pops_at(near_target(ins).unwrap_or(0), 0),
            _ => {
                // call [slot]: an import.
                if ins.op0_kind() == OpKind::Memory
                    && ins.memory_base() == Register::None
                    && ins.memory_index() == Register::None
                    && let Some(import) = self.import_at_slot(ins.memory_displacement32() as u64)
                {
                    return import;
                }
                Pops::Unknown
            }
        }
    }

    /// What an imported function whose address is in `slot` pops: the
    /// arguments pushed for it, unless it is the C runtime's or takes a
    /// variable number of them.
    fn import_at_slot(&self, slot: u64) -> Option<Pops> {
        let i = self.imports.iter().find(|i| i.address == Some(slot))?;
        Some(if cdecl_library(&i.library) || varargs(&i.name) {
            Pops::Bytes(0)
        } else {
            Pops::Pushed
        })
    }

    /// What the function at `f` pops: `ret N` in its code, an import thunk's
    /// import, or the function a tail call leads to.
    fn pops_at(&self, f: u64, depth: u32) -> Pops {
        let Some(sym) = self.symbols.function_containing(f).filter(|s| s.address == f) else {
            return Pops::Bytes(0);
        };
        let end = f + sym.size.max(1);
        let mut pc = f;
        let mut n = 0;
        while pc < end && n < 4000 {
            let Some(ins) = self.decode_x86(32, pc) else { break };
            // An import thunk: jmp [slot].
            if pc == f
                && ins.flow_control() == FlowControl::IndirectBranch
                && ins.op0_kind() == OpKind::Memory
                && ins.memory_base() == Register::None
                && ins.memory_index() == Register::None
            {
                if let Some(p) = self.import_at_slot(ins.memory_displacement32() as u64) {
                    return p;
                }
                if sym.source == SymbolSource::Import {
                    return Pops::Pushed;
                }
            }
            // __chkstk and _alloca_probe move the caller's stack pointer.
            if ins.op0_kind() == OpKind::Register
                && ins.op0_register() == Register::ESP
                && matches!(ins.mnemonic(), Mnemonic::Mov | Mnemonic::Xchg)
                && ins.op1_kind() == OpKind::Register
                && ins.op1_register() != Register::EBP
            {
                return Pops::Alloca;
            }
            match ins.flow_control() {
                FlowControl::Return => {
                    return Pops::Bytes(if ins.op_count() > 0 {
                        ins.immediate16() as i64
                    } else {
                        0
                    });
                }
                FlowControl::UnconditionalBranch => {
                    if let Some(t) = near_target(&ins)
                        && (t < f || t >= end)
                        && depth < 4
                    {
                        return self.pops_at(t, depth + 1);
                    }
                }
                _ => {}
            }
            pc = ins.next_ip();
            n += 1;
        }
        Pops::Bytes(0)
    }
}

fn write(st: &mut State, n: usize) {
    st.written |= 1 << n;
    st.constant &= !(1 << n);
    st.instance[n] += 1;
    st.origin[n] = None;
    if n == EAX {
        st.eax = None;
    }
}

fn near_target(ins: &Instruction) -> Option<u64> {
    matches!(
        ins.op0_kind(),
        OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64
    )
    .then(|| ins.near_branch_target())
}

fn is_immediate(k: OpKind) -> bool {
    matches!(
        k,
        OpKind::Immediate8
            | OpKind::Immediate8to16
            | OpKind::Immediate8to32
            | OpKind::Immediate8to64
            | OpKind::Immediate16
            | OpKind::Immediate32
            | OpKind::Immediate32to64
    )
}

/// Immediate operand `i` as a signed number: 32-bit code's `add esp, 0xfffffff8` adds -8.
fn immediate(ins: &Instruction, i: u32, bits: u32) -> i64 {
    let v = ins.immediate(i);
    if bits == 32 { v as u32 as i32 as i64 } else { v as i64 }
}

fn reads(a: OpAccess) -> bool {
    matches!(
        a,
        OpAccess::Read | OpAccess::ReadWrite | OpAccess::CondRead | OpAccess::ReadCondWrite
    )
}

fn writes(a: OpAccess) -> bool {
    matches!(
        a,
        OpAccess::Write | OpAccess::ReadWrite | OpAccess::CondWrite | OpAccess::ReadCondWrite
    )
}

fn name_of(bits: u32, n: usize) -> &'static str {
    if bits == 32 { reg32_name(n) } else { reg64_name(n) }
}

/// Registers a function must give back as it found them.
fn is_callee_saved(bits: u32, format: Format, n: usize) -> bool {
    match bits {
        32 => matches!(n, EBX | EBP | ESI | EDI),
        _ if matches!(format, Format::Pe | Format::Coff) => matches!(n, EBX | EBP | ESI | EDI | 12..=15),
        _ => matches!(n, EBX | EBP | 12..=15),
    }
}

/// Registers that carry arguments into a function (in 32-bit code, those fastcall and thiscall use).
fn is_argument_register(bits: u32, format: Format, n: usize) -> bool {
    match bits {
        32 => matches!(n, ECX | EDX),
        _ if matches!(format, Format::Pe | Format::Coff) => matches!(n, ECX | EDX | 8 | 9),
        _ => matches!(n, EDI | ESI | EDX | ECX | 8 | 9),
    }
}

impl Frame {
    /// The name of what a register's value came from.
    fn origin_name(&self, o: Origin) -> Option<String> {
        match o {
            Origin::Stack(off) => Some(self.slot_name(Slot {
                at: Sp::Entry(off),
                address: false,
            })),
            Origin::Register(r) => {
                let name = name_of(self.bits, r);
                if self.convention == Convention::Thiscall && r == ECX {
                    return Some("this".into());
                }
                let i = self.registers.iter().position(|&x| x == name)?;
                Some(format!("arg{}", i + 1))
            }
        }
    }

    /// The first stack argument's number: after those passed in registers.
    fn first_stack_arg(&self) -> u32 {
        match self.convention {
            Convention::Fastcall => self.registers.len() as u32 + 1,
            Convention::Win64 => 1,
            Convention::SysV => 7,
            _ => 1,
        }
    }

    /// What a stack slot is: `arg2`, `local_10`, `saved esi`, `return address`.
    pub fn slot_name(&self, slot: Slot) -> String {
        let word = (self.bits / 8) as i64;
        let name = match slot.at {
            Sp::Entry(0) => "return address".to_string(),
            Sp::Entry(off) if off > 0 => {
                // Win64 callers leave 32 bytes above the return address for the register arguments.
                let (base, first) = match self.convention {
                    Convention::Win64 if off < 5 * word => {
                        let k = (off - word) / word;
                        let within = (off - word) % word;
                        let name = format!("home of arg{}", k + 1);
                        return if within == 0 { name } else { format!("{name}+{within}") };
                    }
                    Convention::Win64 => (5 * word, 5),
                    _ => (word, self.first_stack_arg() as i64),
                };
                let k = (off - base) / word;
                let within = (off - base) % word;
                let name = format!("arg{}", first + k);
                if within == 0 { name } else { format!("{name}+{within}") }
            }
            Sp::Entry(off) => match self.saved.iter().find(|s| s.1 == off) {
                Some((r, _)) => format!("saved {r}"),
                None => format!("local_{:x}", -off),
            },
            Sp::Aligned(off) if off < 0 => format!("aligned_{:x}", -off),
            Sp::Aligned(off) => format!("aligned+{off:#x}"),
            Sp::Unknown => String::new(),
        };
        if slot.address { format!("&{name}") } else { name }
    }

    /// The stack slot the instruction at `address` uses, named.
    pub fn slot_at(&self, address: u64) -> Option<String> {
        let i = self.slots.partition_point(|s| s.0 < address);
        self.slots
            .get(i)
            .filter(|s| s.0 == address)
            .map(|s| self.slot_name(s.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 32-bit COFF object whose `.text` holds `code`, with a function at each offset.
    fn object(code: &[u8], functions: &[(&str, u32)]) -> Binary {
        let text = 20 + 40;
        let symbols = text + code.len();
        let mut o = Vec::new();
        o.extend(0x14Cu16.to_le_bytes());
        o.extend(1u16.to_le_bytes());
        o.extend(0u32.to_le_bytes());
        o.extend((symbols as u32).to_le_bytes());
        o.extend((functions.len() as u32).to_le_bytes());
        o.extend([0u8; 4]);
        o.extend(b".text\0\0\0");
        o.extend([0u8; 8]);
        o.extend((code.len() as u32).to_le_bytes());
        o.extend((text as u32).to_le_bytes());
        o.extend([0u8; 12]);
        o.extend(0x6000_0020u32.to_le_bytes());
        o.extend(code);
        for (name, at) in functions {
            let mut n = [0u8; 8];
            n[..name.len()].copy_from_slice(name.as_bytes());
            o.extend(n);
            o.extend(at.to_le_bytes());
            o.extend(1i16.to_le_bytes());
            // A function (DT_FCN), external.
            o.extend(0x20u16.to_le_bytes());
            o.extend([2, 0]);
        }
        o.extend(4u32.to_le_bytes());
        Binary::parse(o).unwrap()
    }

    fn slots(bin: &Binary, f: u64) -> Vec<(u64, String)> {
        let frame = bin.stack_frame(f).unwrap();
        frame.slots.iter().map(|&(a, s)| (a - f, frame.slot_name(s))).collect()
    }

    #[test]
    fn arguments_keep_their_names_across_merged_pops() {
        let code = [
            0x56, // push esi
            0x8B, 0x74, 0x24, 0x08, // mov esi, [esp+8]
            0x6A, 0x01, // push 1
            0x56, // push esi
            0xE8, 0x15, 0, 0, 0, // call g
            0x6A, 0x02, // push 2
            0x50, // push eax
            0xE8, 0x0D, 0, 0, 0, // call g
            0x83, 0xC4, 0x10, // add esp, 0x10: both calls' arguments at once
            0x8B, 0x44, 0x24, 0x0C, // mov eax, [esp+0xc]
            0x03, 0x44, 0x24, 0x08, // add eax, [esp+8]
            0x5E, // pop esi
            0xC3, // ret
            0x8B, 0x44, 0x24, 0x04, // g: mov eax, [esp+4]
            0xC3, // ret
        ];
        let bin = object(&code, &[("_f", 0), ("_g", 34)]);
        let f = bin.symbols().by_name("_f").unwrap().address;
        assert_eq!(
            slots(&bin, f),
            [(1, "arg1".to_string()), (24, "arg2".into()), (28, "arg1".into())]
        );
        let s = bin.function_signature(f).unwrap();
        assert_eq!(s.prototype, "int __cdecl _f(int arg1, int arg2)");
        assert_eq!(
            (s.saved.as_slice(), s.calls, s.unbalanced),
            (&["esi".to_string()][..], 2, false)
        );
        let g = bin.function_signature(f + 34).unwrap();
        assert_eq!((g.prototype.as_str(), g.leaf), ("int __cdecl _g(int arg1)", true));
    }

    #[test]
    fn a_callee_that_pops_its_arguments_moves_the_stack_back() {
        let code = [
            0x6A, 0x01, // push 1
            0x6A, 0x02, // push 2
            0xE8, 0x06, 0, 0, 0, // call h (ret 8)
            0x03, 0x44, 0x24, 0x04, // add eax, [esp+4]
            0xC3, // ret
            0x8B, 0x44, 0x24, 0x04, // h: mov eax, [esp+4]
            0x03, 0x44, 0x24, 0x08, // add eax, [esp+8]
            0xC2, 0x08, 0x00, // ret 8
        ];
        let bin = object(&code, &[("_f", 0), ("_h@8", 15)]);
        let f = bin.symbols().by_name("_f").unwrap().address;
        assert_eq!(slots(&bin, f), [(9, "arg1".to_string())]);
        let h = bin.function_signature(f + 15).unwrap();
        assert_eq!(h.prototype, "int __stdcall _h@8(int arg1, int arg2)");
        assert_eq!((h.convention.as_deref(), h.pops, h.stack_args), (Some("stdcall"), 8, 2));
        assert!(
            h.describe().contains("pops 8 bytes of arguments (ret 8)"),
            "{}",
            h.describe()
        );
    }

    #[test]
    fn a_frame_pointer_names_locals_and_arguments() {
        let code = [
            0x55, // push ebp
            0x89, 0xE5, // mov ebp, esp
            0x83, 0xEC, 0x08, // sub esp, 8
            0x8B, 0x45, 0x08, // mov eax, [ebp+8]
            0x89, 0x45, 0xFC, // mov [ebp-4], eax
            0x8D, 0x45, 0xF8, // lea eax, [ebp-8]
            0x8B, 0x45, 0xFC, // mov eax, [ebp-4]
            0xC9, // leave
            0xC3, // ret
        ];
        let bin = object(&code, &[("_f", 0)]);
        let f = bin.symbols().by_name("_f").unwrap().address;
        assert_eq!(
            slots(&bin, f),
            [
                (6, "arg1".to_string()),
                (9, "local_8".into()),
                (12, "&local_c".into()),
                (15, "local_8".into())
            ]
        );
        let s = bin.function_signature(f).unwrap();
        assert_eq!(
            (s.frame, s.frame_pointer, s.saved.as_slice()),
            (8, true, &["ebp".to_string()][..])
        );
    }

    #[test]
    fn a_call_through_a_pointer_that_pops_its_arguments_is_found_by_the_balance() {
        // A COM-style method call: the callee pops what was pushed for it,
        // which only the stack adding up at the return says.
        let code = [
            0x56, // push esi
            0x8B, 0x74, 0x24, 0x08, // mov esi, [esp+8]
            0x8B, 0x06, // mov eax, [esi]
            0x6A, 0x05, // push 5
            0x56, // push esi
            0xFF, 0x50, 0x08, // call [eax+8]
            0x8B, 0x44, 0x24, 0x08, // mov eax, [esp+8]
            0x5E, // pop esi
            0xC3, // ret
        ];
        let bin = object(&code, &[("_f", 0)]);
        let f = bin.symbols().by_name("_f").unwrap().address;
        assert_eq!(slots(&bin, f), [(1, "arg1".to_string()), (13, "arg1".into())]);
        let s = bin.function_signature(f).unwrap();
        assert!(!s.unbalanced);
        assert_eq!(s.prototype, "int __cdecl _f(void *arg1)");
        assert_eq!(s.accesses[0].base, "arg1");
    }

    #[test]
    fn registers_read_first_say_fastcall_or_thiscall() {
        let code = [
            0x8B, 0x41, 0x04, // mov eax, [ecx+4]
            0x03, 0x44, 0x24, 0x04, // add eax, [esp+4]
            0xC2, 0x04, 0x00, // ret 4
            0x8D, 0x04, 0x11, // f2: lea eax, [ecx+edx]
            0xC3, // ret
        ];
        let bin = object(&code, &[("_m", 0), ("@f2@8", 10)]);
        let m = bin.symbols().by_name("_m").unwrap().address;
        let s = bin.function_signature(m).unwrap();
        assert_eq!(s.prototype, "int __thiscall _m(void *this, int arg1)");
        assert_eq!(s.accesses[0].base, "this");
        let f = bin.function_signature(m + 10).unwrap();
        assert_eq!(f.prototype, "int __fastcall @f2@8(int arg1, int arg2)");
        assert_eq!(f.registers, ["ecx", "edx"]);
    }
}
