//! Plain data types describing a binary. All of them serialize with serde so they
//! can be handed to JavaScript or dumped as JSON unchanged.
//!
//! Convention: addresses, offsets and sizes are `u64`; counts, indices, line and
//! column numbers are `u32`.

use serde::Serialize;

/// Broad classification of a byte range, used for colouring and legends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegionKind {
    /// The container file itself (fat header, archive member headers).
    Container,
    /// File headers: ELF header, Mach header, DOS/COFF/optional headers.
    Header,
    /// Structural tables: program/section headers, load commands, PE section table.
    Metadata,
    Code,
    /// Read-only data: constants, string literals.
    Rodata,
    /// Writable initialized data.
    Data,
    /// Zero-initialized data; occupies memory but no file bytes.
    Bss,
    /// Thread-local data.
    Tls,
    Symbols,
    Strings,
    Relocations,
    /// Dynamic linking: .dynamic, GOT, import/export tables, dyld info.
    Linking,
    /// DWARF, stabs, CodeView and other debug information.
    Debug,
    /// Exception handling and unwind tables.
    Unwind,
    Resources,
    /// Notes, comments, build IDs.
    Notes,
    /// Code signatures and certificates.
    Signature,
    /// Alignment padding (all zero bytes).
    Padding,
    /// Bytes not covered by any known structure.
    Unknown,
    /// Data appended after the end of the image.
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    Elf,
    MachO,
    Pe,
    Coff,
    Xcoff,
    Wasm,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Property {
    pub key: String,
    pub value: String,
}

/// High level facts about a binary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub format: Format,
    /// e.g. "ELF64", "Mach-O 64-bit", "PE32+".
    pub format_name: String,
    /// e.g. "Executable", "Shared library", "Relocatable object".
    pub kind: String,
    pub arch: String,
    pub bits: u32,
    pub little_endian: bool,
    pub file_size: u64,
    pub entry: Option<u64>,
    pub image_base: Option<u64>,
    /// ELF build ID, Mach-O UUID, or PE PDB GUID+age.
    pub build_id: Option<String>,
    /// Where the debug info lives if not here: .gnu_debuglink, PDB path.
    pub debug_link: Option<String>,
    pub has_dwarf: bool,
    pub has_symbols: bool,
    /// True for relocatable objects whose sections were given synthetic,
    /// non-overlapping addresses by binviz.
    pub synthetic_addresses: bool,
    pub section_count: u32,
    pub segment_count: u32,
    pub symbol_count: u32,
    /// Format-specific key/value facts for display.
    pub properties: Vec<Property>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub index: u32,
    pub name: String,
    pub segment_name: Option<String>,
    pub kind: RegionKind,
    pub address: u64,
    /// Size in memory.
    pub size: u64,
    /// Offset of the section's bytes in the file, if it has any.
    pub file_offset: Option<u64>,
    /// Number of bytes in the file (may differ from `size`: bss, compression, PE alignment).
    pub file_size: u64,
    pub align: u64,
    pub flags: String,
    pub perms: String,
    pub compressed: bool,
    /// Index of the segment containing this section.
    pub segment: Option<u32>,
    /// Whether the section occupies memory when the image is loaded.
    pub loaded: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub index: u32,
    pub name: String,
    /// e.g. "PT_LOAD", "LC_SEGMENT_64", "Section".
    pub kind: String,
    pub address: u64,
    pub mem_size: u64,
    pub file_offset: u64,
    pub file_size: u64,
    pub align: u64,
    pub perms: String,
    /// True for segments that define the memory image (PT_LOAD, Mach-O segments,
    /// PE sections). Others (PT_NOTE, PT_DYNAMIC...) just describe part of it.
    pub mapped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SymbolKind {
    Function,
    Data,
    Section,
    File,
    Label,
    Tls,
    Debug,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SymbolSource {
    /// Static symbol table (.symtab, LC_SYMTAB, COFF symbols).
    Symtab,
    /// Dynamic symbol table (.dynsym).
    Dynsym,
    /// PE export table.
    Export,
    /// DWARF subprograms, for binaries without a symbol table.
    Dwarf,
    /// Function boundaries recovered from unwind tables or function-start lists
    /// (named `sub_<address>`).
    Discovered,
    /// The user's own annotations.
    User,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    pub index: u32,
    pub name: String,
    pub demangled: Option<String>,
    pub address: u64,
    pub size: u64,
    /// The size was inferred from the next symbol rather than recorded in the file.
    pub size_inferred: bool,
    pub kind: SymbolKind,
    /// "global", "local", "weak", "undefined"...
    pub binding: String,
    pub section: Option<u32>,
    pub source: SymbolSource,
    pub defined: bool,
}

impl Symbol {
    pub fn display_name(&self) -> &str {
        self.demangled.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Import {
    pub library: String,
    pub name: String,
    pub demangled: Option<String>,
    pub ordinal: Option<u32>,
    /// Address of the slot the loader fills in (PE IAT entry), when known.
    pub address: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    pub name: String,
    pub demangled: Option<String>,
    pub address: u64,
    pub ordinal: Option<u32>,
    pub forwarder: Option<String>,
}

/// A symbol plus the offset of an address within it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolRef {
    pub index: u32,
    pub name: String,
    pub demangled: Option<String>,
    pub address: u64,
    pub size: u64,
    pub offset: u64,
}

/// One step of the "what is this byte" path, from the outermost region inwards.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathEntry {
    /// Layout node id for static regions; None for regions decoded on demand.
    pub id: Option<u32>,
    pub start: u64,
    pub end: u64,
    pub kind: RegionKind,
    pub name: String,
    pub value: Option<String>,
    pub note: Option<String>,
}

/// A node of the file layout tree.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionInfo {
    pub id: u32,
    pub parent: Option<u32>,
    pub start: u64,
    pub end: u64,
    pub kind: RegionKind,
    pub name: String,
    pub value: Option<String>,
    pub note: Option<String>,
    pub child_count: u32,
    /// Number of entries that are decoded on demand (table rows, strings...),
    /// when it is known without walking the region.
    pub entry_count: Option<u32>,
    /// The region has entries decoded on demand (see `Binary::region_entries`).
    pub decoded: bool,
    pub section: Option<u32>,
}

/// A coloured span for the hex view: the innermost region covering `start..end`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub start: u64,
    pub end: u64,
    pub kind: RegionKind,
    /// Nesting depth of the region; used to tint nested structures.
    pub depth: u32,
    /// Alternates between neighbouring entries/fields so boundaries are visible.
    pub shade: u8,
}

/// A source position resolved from DWARF line tables.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLoc {
    /// Index into the source file table.
    pub file: u32,
    pub path: String,
    pub line: u32,
    pub column: u32,
}

/// One frame of the (possibly inlined) call stack at an address.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub function: Option<String>,
    pub demangled: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// Source file index, if the file is in the source table.
    pub file_index: Option<u32>,
    pub unit: Option<u32>,
    pub die: Option<u64>,
    pub inlined: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlowKind {
    Normal,
    Call,
    Jump,
    CondJump,
    Return,
    Interrupt,
    Invalid,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Instruction {
    pub address: u64,
    pub offset: Option<u64>,
    pub len: u32,
    pub bytes: String,
    pub mnemonic: String,
    pub operands: String,
    pub flow: FlowKind,
    /// Branch target or PC-relative memory reference.
    pub target: Option<u64>,
    pub target_symbol: Option<String>,
    /// Line table entry that covers this instruction.
    pub source: Option<SourceLoc>,
}

/// Everything known about one location, whether it was picked by file offset or
/// by virtual address.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub offset: Option<u64>,
    pub address: Option<u64>,
    /// The byte at `offset`, if file-backed.
    pub byte: Option<u8>,
    pub path: Vec<PathEntry>,
    pub segment: Option<u32>,
    pub section: Option<u32>,
    pub symbol: Option<SymbolRef>,
    pub source: Option<SourceLoc>,
    /// Innermost frame first.
    pub frames: Vec<Frame>,
    pub instruction: Option<Instruction>,
    pub unit: Option<u32>,
    /// The user's annotation covering this address, if any.
    pub annotation: Option<Annotation>,
}

/// A note the user attached to an address range while reverse engineering.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub address: u64,
    /// Length in bytes; 0 means "the symbol or instruction at `address`".
    #[serde(default)]
    pub size: u64,
    /// A name for the range; becomes a symbol everywhere (disassembly, search...).
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub comment: String,
    /// Marked as fully understood.
    #[serde(default)]
    pub reviewed: bool,
}
