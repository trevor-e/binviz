//! Everything someone (an agent, say) needs to write a function's C in one
//! place: its code with names resolved, what its code says about its
//! prototype, the callers and callees with theirs, the strings and globals
//! it touches, the structures it walks, and the notes already left on it.

use std::fmt::Write;

use serde::Serialize;

use crate::binary::Binary;
use crate::model::{Annotation, Instruction};
use crate::signature::FunctionSignature;
use crate::xrefs::{Reference, StringUse};

/// A function a function calls or is called by, with what is known of its prototype.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Neighbour {
    pub address: u64,
    pub name: String,
    pub calls: u32,
    pub prototype: Option<String>,
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
    pub callers: Vec<Neighbour>,
    pub callees: Vec<Neighbour>,
    pub strings: Vec<StringUse>,
    pub data: Vec<Reference>,
    /// Notes on the function and the addresses in it.
    pub notes: Vec<Annotation>,
}

impl Binary {
    /// The context for decompiling the function containing `address`: up to
    /// `limit` instructions, and that many callers, callees, strings and globals.
    pub fn decomp_context(&self, address: u64, limit: usize) -> Option<DecompContext> {
        let f = self.symbols().function_containing(address)?;
        let (lo, hi) = (f.address, f.address + f.size.max(1));
        let dis = self.disassemble_function(lo, limit);
        let summary = self.function_summary(lo, limit)?;
        let neighbour = |e: &crate::xrefs::CallEdge| Neighbour {
            address: e.address,
            name: e.name.clone(),
            calls: e.calls,
            prototype: self.function_signature(e.address).map(|s| s.prototype),
        };
        let notes = self
            .annotations()
            .iter()
            .filter(|a| a.address >= lo && a.address < hi)
            .cloned()
            .collect();
        Some(DecompContext {
            address: lo,
            name: f.display_name().into_owned(),
            size: hi - lo,
            signature: self.function_signature(lo),
            instructions: dis.instructions,
            truncated: dis.truncated,
            callers: summary.callers.iter().map(neighbour).collect(),
            callees: summary.callees.iter().map(neighbour).collect(),
            strings: summary.strings,
            data: summary.data,
            notes,
        })
    }
}

impl DecompContext {
    /// The context as text, for a prompt or a terminal.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "{} at {:#x}, {} bytes", self.name, self.address, self.size);
        if let Some(s) = &self.signature {
            out.push_str(&s.describe());
        }
        for (label, list) in [("Called by", &self.callers), ("Calls", &self.callees)] {
            if list.is_empty() {
                continue;
            }
            let _ = writeln!(out, "{label}:");
            for n in list {
                let _ = writeln!(
                    out,
                    "  {:#x} {}{}{}",
                    n.address,
                    n.name,
                    if n.calls > 1 { format!(" (×{})", n.calls) } else { String::new() },
                    n.prototype.as_deref().map_or(String::new(), |p| format!("  {p}"))
                );
            }
        }
        if !self.strings.is_empty() {
            let _ = writeln!(out, "Strings:");
            for s in &self.strings {
                let _ = writeln!(out, "  {:#x} {:?} (at {:#x})", s.address, s.text, s.site);
            }
        }
        if !self.data.is_empty() {
            let _ = writeln!(out, "Globals:");
            for d in &self.data {
                let _ = writeln!(
                    out,
                    "  {:#x} {} {}",
                    d.target,
                    format!("{:?}", d.kind).to_lowercase(),
                    d.to.as_deref().unwrap_or("")
                );
            }
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
            out.push_str("  …\n");
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
            kind: None,
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
