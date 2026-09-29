//! Everything someone (an agent, say) needs to write a function's C in one
//! place: its code with names resolved, what its code says about its
//! prototype, the callers and callees with theirs, the strings and globals
//! it touches, the structures it walks, the notes already left on it, where
//! decompiling it stands, and the functions already decompiled that are
//! shaped like it (their C is the best worked example).

use std::fmt::Write;

use serde::Serialize;

use crate::binary::Binary;
use crate::model::{Annotation, Decomp, DecompState, Instruction};
use crate::signature::FunctionSignature;
use crate::xrefs::{NodeKind, RefCounts, Reference, StringUse};

/// A function a function calls or is called by, with what is known of its prototype.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Neighbour {
    pub address: u64,
    pub name: String,
    pub calls: u32,
    pub prototype: Option<String>,
    /// `Data` for a call through a pointer (`call [0x20066f10]`): the address is where the pointer is.
    pub kind: NodeKind,
}

/// A function already decompiled that is shaped like the one to write: its C
/// is a worked example.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Example {
    pub address: u64,
    pub name: String,
    /// How alike their instructions are: 1 for the same, down to 0.5.
    pub similarity: f32,
    pub state: DecompState,
    /// The source file its C is in.
    pub source: String,
    /// For a sibling decompilation's function, which binary it is in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary: Option<String>,
}

/// What a piece of data a function uses is, where the code's use of it
/// says more than its name: a table of callbacks, a pointer to a structure
/// and the offsets reached through it, a function whose address is stored.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypedData {
    pub address: u64,
    pub what: String,
}

/// The context for decompiling one function.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecompContext {
    pub address: u64,
    pub name: String,
    pub size: u64,
    pub signature: Option<FunctionSignature>,
    pub instructions: Vec<Instruction>,
    pub truncated: bool,
    /// The first callers and callees (most call sites first) of how many.
    pub callers: Vec<Neighbour>,
    pub caller_count: u32,
    pub callees: Vec<Neighbour>,
    pub callee_count: u32,
    /// How the rest of the image refers to it: a function only reached through
    /// a pointer in data (a callback) has no callers, but is referenced.
    pub referenced_by: RefCounts,
    /// Where its address is taken or stored (a callback's), the first of them.
    pub taken_from: Vec<Reference>,
    /// The first strings and globals it uses, of how many.
    pub strings: Vec<StringUse>,
    pub string_count: u32,
    pub data: Vec<Reference>,
    pub data_count: u32,
    /// What the data in `data` is, where that says more than its name.
    pub typed: Vec<TypedData>,
    /// The vtable slots holding it (a virtual function): `const Square::`vftable'[1]`.
    pub vtables: Vec<String>,
    /// Its pieces away from its entry, whose code follows the entry's in `instructions`: (start, end).
    pub parts: Vec<(u64, u64)>,
    /// Lines to show before instructions: where a piece starts, a switch's cases.
    pub marks: Vec<crate::disasm::Mark>,
    /// The function its code runs on into at its end, with no return or jump:
    /// it is another way into that one (an alternate entry).
    pub falls_into: Option<(u64, String)>,
    /// The function that runs on into this one at its end: another way in.
    pub entered_from: Option<(u64, String)>,
    /// Other names of it: functions with the same code the linker folded into one.
    pub aliases: Vec<String>,
    /// Notes on the function and the addresses in it.
    pub notes: Vec<Annotation>,
    /// Where decompiling it stands.
    pub decomp: Option<Decomp>,
    /// Functions already decompiled that are shaped like it, most alike first.
    pub examples: Vec<Example>,
    /// The same from sibling decompilations (other games built with the
    /// same compiler), for before this one has its own.
    pub sibling_examples: Vec<Example>,
}

impl Binary {
    /// The context for decompiling the function containing `address`: up to
    /// `limit` instructions, and the first callers, callees, strings and
    /// globals (up to [`CONTEXT_LIST`] of each, with how many there are).
    pub fn decomp_context(&self, address: u64, limit: usize) -> Option<DecompContext> {
        self.decomp_context_with(address, limit, &[])
    }

    /// [`Binary::decomp_context`], with worked examples from `siblings` too:
    /// other games' decompilations built with the same compiler, each with
    /// its notes and a label to say where an example is from.
    pub fn decomp_context_with(
        &self,
        address: u64,
        limit: usize,
        siblings: &[(&str, &Binary)],
    ) -> Option<DecompContext> {
        let f = self.symbols().function_containing(address)?;
        let (lo, hi) = (f.address, f.address + f.size.max(1));
        // The references first: they name the data the code uses.
        if self.xrefs_supported() {
            self.prepare_xrefs();
        }
        let dis = self.disassemble_function(lo, limit);
        let summary = self.function_summary(lo, CONTEXT_LIST.min(limit.max(1)))?;
        let neighbour = |e: &crate::xrefs::CallEdge| Neighbour {
            address: e.address,
            name: e.name.clone(),
            calls: e.calls,
            prototype: self.function_signature(e.address).map(|s| s.prototype),
            kind: e.kind,
        };
        let notes = self
            .annotations()
            .iter()
            .filter(|a| a.address >= lo && a.address < hi && a.is_note())
            .cloned()
            .collect();
        let examples = self
            .worked_examples(lo, 3)
            .into_iter()
            .map(|s| {
                let d = self.decomp_at(s.address).cloned().unwrap_or_default();
                Example {
                    address: s.address,
                    name: self
                        .symbols()
                        .at(s.address)
                        .map_or_else(|| format!("{:#x}", s.address), |f| f.display_name().into_owned()),
                    similarity: s.similarity,
                    state: d.state,
                    source: d.source,
                    binary: None,
                }
            })
            .collect();
        let mut sibling_examples: Vec<Example> = siblings
            .iter()
            .flat_map(|&(label, sibling)| {
                self.sibling_examples(sibling, lo, 3).into_iter().map(move |s| {
                    let d = sibling.decomp_at(s.address).cloned().unwrap_or_default();
                    Example {
                        address: s.address,
                        name: sibling
                            .symbols()
                            .at(s.address)
                            .map_or_else(|| format!("{:#x}", s.address), |f| f.display_name().into_owned()),
                        similarity: s.similarity,
                        state: d.state,
                        source: d.source,
                        binary: Some(label.to_string()),
                    }
                })
            })
            .collect();
        sibling_examples.sort_by(|a, b| b.similarity.total_cmp(&a.similarity));
        sibling_examples.truncate(3);
        let typed = summary
            .data
            .iter()
            .filter_map(|r| {
                let what = if self.symbols().function_containing(r.target).is_some_and(|f| f.address == r.target)
                    && self.section_at(r.target).is_some_and(|s| s.kind == crate::model::RegionKind::Code)
                {
                    "a function, its address stored or passed: a callback".to_string()
                } else {
                    use crate::globals::GlobalKind as K;
                    let g = self.global_at(r.target)?;
                    let says_more = match g.kind {
                        K::Functions
                        | K::Strings
                        | K::Pointers
                        | K::Array
                        | K::Records
                        | K::Structure
                        | K::FunctionPointers
                        | K::JumpTable => true,
                        K::Pointer => !g.fields.is_empty(),
                        _ => false,
                    };
                    if !says_more {
                        return None;
                    }
                    g.description
                };
                Some(TypedData {
                    address: r.target,
                    what,
                })
            })
            .collect();
        // The vtables it is a slot of: a virtual function of those classes.
        let word = if self.is64 { 8 } else { 4 };
        let vtables = self
            .references_to(lo, lo + 1, 0, 64)
            .refs
            .iter()
            .filter(|r| r.kind == crate::xrefs::RefKind::Pointer)
            .filter_map(|r| {
                let s = self.symbols().lookup(r.source)?;
                let name = s.demangled.unwrap_or(s.name);
                name.contains("`vftable'").then(|| format!("{name}[{}]", s.offset / word))
            })
            .collect();
        // A function whose last instruction runs on into the next is another way into it.
        let name_at = |a: u64| self.symbols().at(a).map(|s| (a, s.display_name().into_owned()));
        let falls_into = self.falls_into(lo, lo + f.size).and_then(name_at);
        let entered_from = self
            .symbols()
            .functions()
            .take_while(|g| g.address < lo)
            .filter(|g| g.address + g.size == lo)
            .last()
            .filter(|g| self.falls_into(g.address, lo).is_some())
            .and_then(|g| name_at(g.address));
        Some(DecompContext {
            address: lo,
            name: f.display_name().into_owned(),
            size: if dis.end > lo && !dis.truncated { dis.end - lo } else { hi - lo },
            signature: self.function_signature(lo),
            instructions: dis.instructions,
            truncated: dis.truncated,
            callers: summary.callers.iter().map(neighbour).collect(),
            caller_count: summary.caller_count,
            callees: summary.callees.iter().map(neighbour).collect(),
            callee_count: summary.callee_count,
            referenced_by: summary.referenced_by,
            taken_from: self.taken_from(lo, hi, 8),
            strings: summary.strings,
            string_count: summary.string_count,
            data: summary.data,
            data_count: summary.data_count,
            typed,
            vtables,
            parts: dis.parts.clone(),
            marks: dis.marks.clone(),
            falls_into,
            entered_from,
            aliases: self.symbols().aliases(lo),
            notes,
            decomp: self.decomp_at(lo).cloned(),
            examples,
            sibling_examples,
        })
    }
}

/// At most this many callers, callees, strings and globals each in a
/// context: a function called from everywhere would bury its code.
pub const CONTEXT_LIST: usize = 24;

/// `  … and 12 more` after a list showing `shown` of `count`.
fn more(out: &mut String, count: u32, shown: usize) {
    if count as usize > shown {
        let _ = writeln!(out, "  … and {} more", count as usize - shown);
    }
}

impl DecompContext {
    /// The context as text, for a prompt or a terminal.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "{} at {:#x}, {} bytes", self.name, self.address, self.size);
        if let Some(d) = &self.decomp {
            let _ = write!(out, "Decompilation: {}", d.state.as_str());
            if !d.source.is_empty() {
                let _ = write!(out, " in {}", d.source);
            }
            if d.attempts > 0 {
                let _ = write!(out, ", tried {}×", d.attempts);
            }
            if let Some(p) = d.percent.filter(|&p| p < 100.0) {
                let _ = write!(out, ", best {p:.1}%");
            }
            out.push('\n');
        }
        if let Some(s) = &self.signature {
            out.push_str(&s.describe());
        }
        for (list, heading) in [
            (
                &self.examples,
                "Worked examples (already decompiled, shaped like it: their C is the place to start):",
            ),
            (
                &self.sibling_examples,
                "Worked examples from sibling decompilations (other games, the same compiler: their C is a place to start):",
            ),
        ] {
            if list.is_empty() {
                continue;
            }
            let _ = writeln!(out, "{heading}");
            for e in list {
                let _ = writeln!(
                    out,
                    "  {:>3.0}%  {:#x} {}  {}{}{}",
                    e.similarity * 100.0,
                    e.address,
                    e.name,
                    e.state.as_str(),
                    if e.source.is_empty() {
                        String::new()
                    } else {
                        format!(" in {}", e.source)
                    },
                    e.binary.as_ref().map_or(String::new(), |b| format!("  ({b})"))
                );
            }
        }
        for (label, list, count) in [
            ("Called by", &self.callers, self.caller_count),
            ("Calls", &self.callees, self.callee_count),
        ] {
            if list.is_empty() {
                continue;
            }
            let _ = writeln!(out, "{label}:");
            for n in list {
                let name = if n.kind == NodeKind::Data {
                    let slot = if n.name.starts_with("0x") { String::new() } else { format!(" {}", n.name) };
                    format!("(indirect, through the pointer{slot} at {:#x})", n.address)
                } else {
                    n.name.clone()
                };
                let _ = writeln!(
                    out,
                    "  {:#x} {}{}{}",
                    n.address,
                    name,
                    if n.calls > 1 { format!(" (×{})", n.calls) } else { String::new() },
                    n.prototype.as_deref().map_or(String::new(), |p| format!("  {p}"))
                );
            }
            more(&mut out, count, list.len());
        }
        if !self.aliases.is_empty() {
            let _ = writeln!(
                out,
                "Also named: {} (functions with the same code, folded into one by the linker)",
                self.aliases.join(", ")
            );
        }
        for (s, e) in &self.parts {
            let _ = writeln!(
                out,
                "Has a piece away from its entry at {s:#x}..{e:#x} ({} bytes; its code follows the entry's below)",
                e - s
            );
        }
        if let Some((a, name)) = &self.falls_into {
            let _ = writeln!(
                out,
                "Runs on into {name} at {a:#x} at its end, with no return: another way into that function (an alternate entry)"
            );
        }
        if let Some((a, name)) = &self.entered_from {
            let _ = writeln!(
                out,
                "Also entered from {name} at {a:#x}, whose code runs on into this one: an alternate entry"
            );
        }
        if !self.vtables.is_empty() {
            let _ = writeln!(out, "In vtables (a virtual function): {}", self.vtables.join(", "));
        }
        // Besides the calls: tail calls, and the address taken or stored (a callback's).
        let r = RefCounts {
            call: 0,
            jump: 0,
            ..self.referenced_by
        };
        let uses = r.describe();
        if !uses.is_empty() {
            let _ = writeln!(out, "Also referenced by: {uses}");
        }
        if !self.taken_from.is_empty() {
            let callback = if self.callers.is_empty() {
                " (a callback: called through a pointer, not by name)"
            } else {
                ""
            };
            let _ = writeln!(out, "Its address is taken or stored at{callback}:");
            for t in &self.taken_from {
                let _ = writeln!(
                    out,
                    "  {:#x} {} {}",
                    t.source,
                    t.kind.as_str(),
                    t.from.as_deref().unwrap_or("")
                );
            }
            // A callback whose code reads no argument may still be passed some.
            if self.callers.is_empty()
                && self.signature.as_ref().is_some_and(|s| s.prototype.ends_with("(void)"))
            {
                let _ = writeln!(
                    out,
                    "Its code reads no arguments; called through a pointer, it may be passed some it ignores (the pointer's type says which)"
                );
            }
        }
        if !self.strings.is_empty() {
            let _ = writeln!(out, "Strings:");
            for s in &self.strings {
                let _ = writeln!(out, "  {:#x} {:?} (at {:#x})", s.address, s.text, s.site);
            }
            more(&mut out, self.string_count, self.strings.len());
        }
        if !self.data.is_empty() {
            let _ = writeln!(out, "Globals:");
            for d in &self.data {
                let what = self
                    .typed
                    .iter()
                    .find(|t| t.address == d.target)
                    .map_or(String::new(), |t| format!(" — {}", t.what));
                let _ = writeln!(
                    out,
                    "  {:#x} {} {}{what}",
                    d.target,
                    format!("{:?}", d.kind).to_lowercase(),
                    d.to.as_deref().unwrap_or("")
                );
            }
            more(&mut out, self.data_count, self.data.len());
        }
        if !self.notes.is_empty() {
            let _ = writeln!(out, "Notes:");
            for n in &self.notes {
                let _ = writeln!(
                    out,
                    "  {:#x} {}{}",
                    n.address,
                    if n.name.is_empty() { String::new() } else { format!("{} ", n.name) },
                    n.comment
                );
            }
        }
        let _ = writeln!(out, "Code:");
        for i in &self.instructions {
            for m in self.marks.iter().filter(|m| m.address == i.address) {
                let _ = writeln!(out, "  ; {}", m.text);
            }
            let _ = writeln!(
                out,
                "  {:08x}  {} {}{}",
                i.address,
                i.mnemonic,
                i.operands,
                i.target_symbol.as_deref().map_or(String::new(), |s| format!("  ; {s}"))
            );
        }
        if self.truncated {
            let _ = writeln!(out, "  … stopped at {} instructions of a {}-byte function; pass a larger count for the rest", self.instructions.len(), self.size);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_for_a_playstation_function() {
        let mut data = vec![0u8; 0x800];
        data[..8].copy_from_slice(b"PS-X EXE");
        for (at, v) in [(0x10, 0x8001_0000u32), (0x14, 0x8001_8000), (0x18, 0x8001_0000), (0x1C, 0x30)] {
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        // entry: addiu $sp,-0x18 / sw $ra / jal leaf / nop / lw $ra / jr $ra / addiu $sp; leaf: lui $v0, 0x8001 / lw $v0, 0x2c($v0) / jr $ra / nop
        let words = [
            0x27BD_FFE8u32,
            0xAFBF_0014,
            0x0C00_4007,
            0,
            0x8FBF_0014,
            0x03E0_0008,
            0x27BD_0018,
            0x3C02_8001,
            0x8C42_002C,
            0x03E0_0008,
            0,
            0x0000_0001,
        ];
        data.extend(words.iter().flat_map(|w| w.to_le_bytes()));
        let mut bin = Binary::parse(data).unwrap();
        bin.set_annotations(vec![Annotation {
            address: 0x8001_001C,
            size: 0,
            name: "GetState".into(),
            comment: "reads the state word".into(),
            reviewed: false,
            kind: None, decomp: None, ctype: None,
        }]);
        let c = bin.decomp_context(0x8001_0000, 64).unwrap();
        assert_eq!((c.name.as_str(), c.size), ("entry", 0x1C));
        assert_eq!(c.callees.len(), 1);
        assert_eq!(c.callees[0].name, "GetState");
        assert!(c.callees[0].prototype.as_deref().unwrap().starts_with("int GetState("));
        let text = c.describe();
        assert!(text.contains("jal 0x8001001c  ; GetState"), "{text}");
        assert!(text.contains("frame 24 bytes, saves [$ra]"), "{text}");
        let leaf = bin.decomp_context(0x8001_001C, 64).unwrap();
        assert_eq!(leaf.callers[0].name, "entry");
        assert!(leaf.notes.iter().any(|n| n.comment == "reads the state word"));
        assert!(leaf.data.iter().any(|d| d.target == 0x8001_002C), "{:?}", leaf.data);
    }
}
