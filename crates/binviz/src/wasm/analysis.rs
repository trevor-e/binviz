//! What a module's code does, for binviz's views: each function's
//! instructions with the names of what they touch, tokens that compare
//! functions across builds, and the references each instruction makes for
//! the cross-reference index and the call graph: calls (to functions and
//! imports), tail calls, `ref.func`, globals read and written, and linear
//! memory where the code gives the address outright (a load or store off
//! a constant, a constant that points at a string or a variable).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::code::{self, Decoded, Imm, Insn, Stack, decode_body};
use super::read::{self, Mode, Module, ValType};
use crate::binary::Binary;
use crate::model::{FlowKind, Instruction, RegionKind, SymbolKind};
use crate::util;
use crate::xrefs::RefKind;

/// A body's locals declarations as text: `i32 ×2, f64`, or `none`.
pub(crate) fn locals_text(decls: &[(u32, ValType)]) -> String {
    let parts: Vec<String> = decls
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, t)| if *n == 1 { t.to_string() } else { format!("{t} ×{n}") })
        .collect();
    if parts.is_empty() {
        "none".into()
    } else {
        parts.join(", ")
    }
}

impl Module {
    /// How many functions of type `ty` table `table` holds, from its active
    /// element segments: the functions a `call_indirect` of that type may
    /// call. Types count as the same when their signatures are.
    pub fn indirect_targets(&self, table: u32, ty: u32) -> u32 {
        let (canonical, counts) = self.indirect.get_or_init(|| {
            let canonical = self.canonical_types();
            let mut out: HashMap<(u32, u32), u32> = HashMap::new();
            for e in &self.elements {
                let Mode::Active { index, .. } = e.mode else { continue };
                for f in e.items.iter().filter_map(|i| i.0) {
                    if let Some(t) = self.funcs.get(f as usize).map(|f| f.type_index)
                        && let Some(&c) = canonical.get(t as usize)
                    {
                        *out.entry((index, c)).or_default() += 1;
                    }
                }
            }
            (canonical, out)
        });
        canonical
            .get(ty as usize)
            .and_then(|c| counts.get(&(table, *c)))
            .copied()
            .unwrap_or(0)
    }

    /// Each type index's first index with the same signature.
    fn canonical_types(&self) -> Vec<u32> {
        let mut first: HashMap<&read::FuncType, u32> = HashMap::new();
        self.types
            .iter()
            .enumerate()
            .map(|(i, t)| match t {
                Some(t) => *first.entry(t).or_insert(i as u32),
                None => i as u32,
            })
            .collect()
    }
}

/// Whether `address` (in memory) is where a variable or a string starts:
/// what makes a number in the code an address rather than a size.
fn points_at_data(bin: &Binary, address: u64) -> bool {
    if bin.symbols.at(address).is_some_and(|s| s.kind == SymbolKind::Data) {
        return true;
    }
    let Some(sec) = bin.section_at(address).filter(|s| s.file_offset.is_some()) else {
        return false;
    };
    let Some(off) = bin.address_to_offset(address) else {
        return false;
    };
    let starts = address == sec.address || off.checked_sub(1).and_then(|o| bin.data.get(o as usize)) == Some(&0);
    starts && bin.string_preview(address, 8).is_some()
}

/// What an instruction refers to, and how. `stack` follows the constants
/// on the operand stack, so every instruction of a body must go through
/// here in order.
fn refers(bin: &Binary, m: &Module, insn: &Insn, stack: &mut Stack) -> Option<(u64, RefKind)> {
    let access = stack.step(m, insn);
    match (&insn.imm, insn.op) {
        (Imm::Func(f), code::CALL) => Some((m.function_address(*f)?, RefKind::Call)),
        (Imm::Func(f), code::RETURN_CALL) => Some((m.function_address(*f)?, RefKind::Jump)),
        (Imm::Func(f), code::REF_FUNC) => Some((m.function_address(*f)?, RefKind::Address)),
        (Imm::Global(g), code::GLOBAL_GET) => Some((m.global_address(*g)?, RefKind::Read)),
        (Imm::Global(g), code::GLOBAL_SET) => Some((m.global_address(*g)?, RefKind::Write)),
        (Imm::I32(v), code::I32_CONST) => {
            let a = m.memory_address(u64::from(*v as u32))?;
            points_at_data(bin, a).then_some((a, RefKind::Address))
        }
        _ => {
            if insn.memarg()?.memory != 0 {
                return None;
            }
            let a = m.memory_address(access?)?;
            let kind = if insn.stores() == Some(true) {
                RefKind::Write
            } else {
                RefKind::Read
            };
            bin.section_at(a).is_some().then_some((a, kind))
        }
    }
}

fn flow(d: &Decoded, i: usize) -> FlowKind {
    match d.insns[i].1.op {
        code::UNREACHABLE | code::THROW | code::RETHROW | code::THROW_REF => FlowKind::Interrupt,
        code::BR
        | code::BR_TABLE
        | code::ELSE
        | code::DELEGATE
        | code::RETURN_CALL
        | code::RETURN_CALL_INDIRECT
        | code::RETURN_CALL_REF => FlowKind::Jump,
        code::BR_IF | code::IF | code::BR_ON_NULL | code::BR_ON_NON_NULL => FlowKind::CondJump,
        code::RETURN => FlowKind::Return,
        code::END if d.is_last_end(i) => FlowKind::Return,
        code::CALL | code::CALL_INDIRECT | code::CALL_REF => FlowKind::Call,
        _ => FlowKind::Normal,
    }
}

/// A line of the disassembly that isn't an instruction: a body's size,
/// its locals, bytes that don't decode.
fn pseudo(bin: &Binary, at: u64, len: u64, mnemonic: &str, operands: String, flow: FlowKind) -> Instruction {
    let bytes = bin.data.get(at as usize..(at + len) as usize).unwrap_or_default();
    Instruction {
        address: at,
        offset: Some(at),
        len: len as u32,
        bytes: util::hex_bytes(bytes),
        mnemonic: mnemonic.into(),
        operands,
        flow,
        target: None,
        target_symbol: None,
        source: None,
    }
}

/// Disassembles the code in `start..end`: every function body there,
/// decoded from its start (a body can only be read from the beginning)
/// with the instructions in the range listed; a body's size before it and
/// its locals as lines of their own. At most `limit` lines; also says
/// whether the limit cut it short.
pub(crate) fn disassemble(bin: &Binary, start: u64, end: u64, limit: usize) -> (Vec<Instruction>, bool) {
    let Some(m) = &bin.wasm else {
        return (Vec::new(), false);
    };
    let data = &bin.data;
    let mut out = Vec::new();
    // The count of bodies, where the code section's contents start.
    if let Some(s) = m.section(read::CODE)
        && (start..end).contains(&s.payload)
        && let Some((n, len)) = read::uleb(&data[s.payload as usize..s.end as usize], 32)
    {
        out.push(pseudo(
            bin,
            s.payload,
            len as u64,
            ".count",
            format!("{n} function bodies"),
            FlowKind::Normal,
        ));
    }
    for (f, body) in m.functions_in(start, end) {
        if out.len() >= limit {
            return (out, true);
        }
        let name = bin.name_for(body.start);
        if (start..end).contains(&body.entry) {
            let mut size = pseudo(
                bin,
                body.entry,
                body.start - body.entry,
                ".size",
                (body.end - body.start).to_string(),
                FlowKind::Normal,
            );
            size.target = Some(body.start);
            size.target_symbol = name.clone();
            out.push(size);
        }
        let d = decode_body(data, body);
        if (start..end).contains(&body.start) {
            let mut locals = pseudo(
                bin,
                body.start,
                body.code - body.start,
                ".local",
                d.locals.as_deref().map_or_else(|| "?".into(), locals_text),
                FlowKind::Normal,
            );
            locals.target_symbol = m
                .func_type(f)
                .map(|t| format!("{} {t}", name.as_deref().unwrap_or("")).trim().to_string());
            out.push(locals);
        }
        let local_names: HashMap<u32, &str> = m
            .names
            .locals
            .get(&f)
            .map(|l| l.iter().map(|(i, n)| (*i, n.as_str())).collect())
            .unwrap_or_default();
        let mut stack = Stack::default();
        for (i, (at, insn)) in d.insns.iter().enumerate() {
            let r = refers(bin, m, insn, &mut stack);
            if *at < start || *at >= end {
                continue;
            }
            if out.len() >= limit {
                return (out, true);
            }
            let len = u64::from(insn.len);
            let (target, target_symbol) = match (&insn.imm, r, d.targets[i]) {
                (_, _, Some(t)) => (Some(t), bin.name_for(t)),
                (_, Some((t, _)), _) => (Some(t), bin.name_for(t)),
                (Imm::Indirect { ty, table }, ..) => {
                    let sig = m.types.get(*ty as usize).and_then(Option::as_ref);
                    let n = m.indirect_targets(*table, *ty);
                    let what = format!(
                        "through table {table}: {} of this type",
                        if n == 1 {
                            "1 function".into()
                        } else {
                            format!("{n} functions")
                        }
                    );
                    (None, Some(sig.map_or(what.clone(), |s| format!("{s}, {what}"))))
                }
                (Imm::Local(l), ..) => (None, local_names.get(l).map(|n| n.to_string())),
                _ => (None, None),
            };
            out.push(Instruction {
                address: *at,
                offset: Some(*at),
                len: insn.len,
                bytes: util::hex_bytes(&data[*at as usize..(*at + len) as usize]),
                mnemonic: insn.name.into(),
                operands: insn.operands(),
                flow: flow(&d, i),
                target,
                target_symbol,
                source: None,
            });
        }
        // What doesn't decode is shown as bytes, up to the body's end.
        if let Some(stop) = d.stopped {
            let mut at = stop.max(start);
            while at < body.end.min(end) {
                if out.len() >= limit {
                    return (out, true);
                }
                let len = (body.end.min(end) - at).min(8);
                let text = data[at as usize..(at + len) as usize]
                    .iter()
                    .map(|b| format!("{b:#04x}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push(pseudo(bin, at, len, ".byte", text, FlowKind::Invalid));
                at += len;
            }
        }
    }
    (out, false)
}

/// A token per instruction in `start..end` (at most `limit`): its opcode,
/// and the immediates that don't depend on where things are (locals,
/// branch depths, lanes, alignment), so that the same code in another build
/// reads the same even with its functions, globals and data moved. Also
/// the bytes the instructions take.
pub(crate) fn tokens(bin: &Binary, start: u64, end: u64, limit: usize) -> (Vec<u32>, u64) {
    let Some(m) = &bin.wasm else {
        return (Vec::new(), 0);
    };
    let mut out = Vec::new();
    let mut last = start;
    let token = |f: &dyn Fn(&mut std::collections::hash_map::DefaultHasher)| {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        f(&mut h);
        h.finish() as u32
    };
    for (_, body) in m.functions_in(start, end) {
        let d = decode_body(&bin.data, body);
        if (start..end).contains(&body.start) {
            out.push(token(&|h| d.locals.hash(h)));
            last = last.max(body.code);
        }
        for (at, insn) in &d.insns {
            if *at < start || *at >= end {
                continue;
            }
            if out.len() >= limit {
                return (out, last - start);
            }
            out.push(token(&|h| {
                insn.op.hash(h);
                match &insn.imm {
                    Imm::Local(n) | Imm::Label(n) => n.hash(h),
                    Imm::Labels(l) => l.hash(h),
                    Imm::Lane(l) => l.hash(h),
                    Imm::Block(t) => format!("{t:?}").hash(h),
                    Imm::Mem(a) => a.align.hash(h),
                    _ => {}
                }
            }));
            last = last.max(*at + u64::from(insn.len)).min(end);
        }
    }
    (out, last.saturating_sub(start))
}

/// The references the code in `lo..hi` makes: (instruction, target, kind).
pub(crate) fn scan(bin: &Binary, lo: u64, hi: u64, emit: &mut dyn FnMut(u64, u64, RefKind)) {
    let Some(m) = &bin.wasm else { return };
    for (_, body) in m.functions_in(lo, hi) {
        let d = decode_body(&bin.data, body);
        let mut stack = Stack::default();
        for (at, insn) in &d.insns {
            let r = refers(bin, m, insn, &mut stack);
            if (lo..hi).contains(at)
                && let Some((target, kind)) = r
            {
                emit(*at, target, kind);
            }
        }
    }
}

/// The pointers the module stores, as (where, to what), sorted by where:
/// the slots of its function tables (in the element section), each holding
/// a function `call_indirect` can reach, and the 32-bit words of its data
/// segments whose value, read as an address in linear memory, lands in the
/// data (a table of messages, a structure's pointer to its name). No word
/// in memory points at code, which has no address there.
pub(crate) fn pointers(bin: &Binary) -> Vec<(u64, u64)> {
    let Some(m) = bin.wasm.as_ref() else { return Vec::new() };
    let mut out: Vec<(u64, u64)> = m
        .elements
        .iter()
        .flat_map(|e| e.items.iter())
        .filter_map(|&(f, at)| Some((at, m.function_address(f?)?)))
        .collect();
    let in_memory = |s: &&crate::model::Section| {
        s.loaded
            && s.address >= super::MEMORY_BASE
            && matches!(
                s.kind,
                RegionKind::Data | RegionKind::Rodata | RegionKind::Bss | RegionKind::Tls
            )
    };
    let mut data: Vec<(u64, u64)> = bin
        .sections
        .iter()
        .filter(in_memory)
        .map(|s| (s.address, s.address + s.size))
        .collect();
    data.sort_unstable();
    let lands = |t: u64| {
        let i = data.partition_point(|r| r.0 <= t);
        i > 0 && t < data[i - 1].1
    };
    for sec in bin.sections.iter().filter(in_memory) {
        let (Some(offset), true) = (sec.file_offset, m.memory_mapped()) else {
            continue;
        };
        if sec.kind == RegionKind::Bss || crate::pointers::stringy(&sec.name) {
            continue;
        }
        let Some(bytes) = bin.data.get(offset as usize..(offset + sec.file_size) as usize) else {
            continue;
        };
        let skip = ((4 - sec.address % 4) % 4) as usize;
        for (i, w) in bytes.get(skip..).unwrap_or_default().chunks_exact(4).enumerate() {
            let v = u32::from_le_bytes([w[0], w[1], w[2], w[3]]) as u64;
            if v != 0 && lands(super::MEMORY_BASE + v) {
                out.push((sec.address + (skip + 4 * i) as u64, super::MEMORY_BASE + v));
            }
        }
    }
    out.sort_unstable();
    out
}

/// The start of the function body holding `address`, or `address` itself.
pub(crate) fn boundary_before(bin: &Binary, address: u64) -> u64 {
    bin.wasm
        .as_ref()
        .and_then(|m| m.function_at(address))
        .and_then(|f| bin.wasm.as_ref()?.funcs[f as usize].body)
        .map_or(address, |b| b.start)
}
