//! # binviz
//!
//! Explains every byte and every address of ELF, Mach-O and PE binaries.
//!
//! - [`Binary`] parses a file (via the [`object`] crate) into sections,
//!   segments, symbols, imports and exports, and builds a *layout*: a tree of
//!   labelled byte ranges that covers the whole file down to individual header
//!   fields, table entries and strings.
//! - [`Binary::inspect`] answers "what is here?" for a file offset or a virtual
//!   address: the layout path, segment, section, symbol, source line, inlined
//!   call stack and instruction.
//! - [`DebugInfo`] exposes DWARF (via [`gimli`] and [`addr2line`]): units, DIE
//!   trees with readable attributes and types, line tables, and the mapping
//!   between addresses and source lines in both directions.
//! - [`Binary::disassemble`] decodes x86/x86-64 and AArch64/ARM code with
//!   branch targets resolved to symbols and each instruction's source line.
//!
//! ```no_run
//! let data = std::fs::read("a.out").unwrap();
//! let bin = binviz::Binary::parse(data).unwrap();
//! let info = bin.inspect(binviz::Target::Address(bin.summary().entry.unwrap()));
//! for entry in &info.path {
//!     println!("{} {}", entry.name, entry.value.as_deref().unwrap_or(""));
//! }
//! ```

mod binary;
mod container;
pub mod coverage;
pub mod cpu;
pub mod crash;
pub mod diff;
mod disasm;
pub mod disc;
mod discover;
pub mod dwarf;
mod error;
pub mod fndiff;
mod inspect;
mod layout;
pub mod matching;
pub mod model;
pub mod objc;
pub mod package;
pub mod patch;
pub mod plist;
mod pointers;
pub mod rom;
pub mod search;
pub mod signature;
pub mod size;
pub mod splat;
mod strings;
mod stubs;
mod symbols;
pub mod tables;
mod util;
pub mod xrefs;
pub mod zip;

pub use binary::Binary;
pub use container::{Container, ContainerInfo, Member};
pub use coverage::{Coverage, MapStatus};
pub use disasm::{Disassembly, is_supported as disassembly_supported};
pub use dwarf::DebugInfo;
pub use dwarf::attribution::{AttributedRange, Attribution, AttributionMode};
pub use error::{Error, Result};
pub use inspect::Target;
pub use model::*;
pub use search::{HitKind, SearchHit, SearchResults};
pub use size::{GroupKind, SizeReport};
pub use strings::{FoundString, StringPage};
pub use symbols::{Binding, FunctionPage, Sym, SymbolPage, SymbolQuery, SymbolTable};
pub use util::demangle;
pub use xrefs::{
    CallEdge, CallGraph, FunctionSummary, GraphEdge, GraphNode, NodeKind, PathStep, RefCounts, RefKind, RefPage,
    Reference, StringUse,
};

/// Reads a file straight into shared storage: a large binary is never held
/// twice while it is parsed.
pub fn read_file(path: impl AsRef<std::path::Path>) -> std::io::Result<std::sync::Arc<[u8]>> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let len = usize::try_from(file.metadata()?.len()).map_err(|_| std::io::Error::other("file too large"))?;
    let mut buf = std::sync::Arc::<[u8]>::new_uninit_slice(len);
    let slots = std::sync::Arc::get_mut(&mut buf).expect("new buffer");
    let mut chunk = vec![0u8; len.clamp(1, 1 << 20)];
    let mut filled = 0;
    while filled < len {
        let want = chunk.len().min(len - filled);
        file.read_exact(&mut chunk[..want])?;
        for (dst, &src) in slots[filled..filled + want].iter_mut().zip(&chunk[..want]) {
            dst.write(src);
        }
        filled += want;
    }
    // SAFETY: every byte was written above.
    Ok(unsafe { buf.assume_init() })
}
