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
mod disasm;
mod discover;
pub mod dwarf;
mod error;
mod inspect;
mod layout;
pub mod model;
pub mod search;
mod strings;
mod symbols;
mod util;

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
pub use strings::{FoundString, StringPage};
pub use symbols::{SymbolPage, SymbolQuery, SymbolTable};
pub use util::demangle;
