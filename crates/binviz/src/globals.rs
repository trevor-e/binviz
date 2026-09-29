//! The data a binary's code uses, where nothing names it. Every address in
//! data that the code reads, writes, calls through or takes, or that a
//! pointer in data points at, starts a global; what it is comes from how it
//! is used:
//!
//! - the width the code reads and writes it with: a `float`, a `double`, a
//!   byte, a word…, and an index register reaching into it makes it an array
//!   of elements that size;
//! - a pointer read from it and then used as a base (`mov eax, [cvar];
//!   fld [eax+0x14]`) makes it a pointer to a structure, with the offsets
//!   reached through it;
//! - pointers stored in it make it a table: of functions (callbacks, a
//!   vtable), of strings, of other data;
//! - calls through it (`call [0x20066f10]`) make it a table of function
//!   pointers filled at run time, such as the table of engine functions a
//!   game DLL is handed: its address taken where it is filled (by copying
//!   the engine's table, `rep movsd`), each slot called through;
//! - the jump tables the code follower found are jump tables.
//!
//! Each is named the way disassemblers do, after what it is and where:
//! `flt_4020a0`, `dword_403000`, `funcs_402000`, `fptrs_20066ee0` (its slots
//! `fptrs_20066ee0+0x30`), and a note naming the address wins.

use iced_x86::{Decoder, DecoderOptions, Instruction, MemorySize, Mnemonic, OpAccess, OpKind, Register};
use serde::Serialize;

use crate::binary::Binary;
use crate::model::RegionKind;
use crate::xrefs::RefKind;

/// What a global is, from how the code uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GlobalKind {
    String,
    Float,
    Double,
    Integer,
    /// Holds a pointer: to a structure, when the code reaches through it.
    Pointer,
    /// A table of pointers to code: callbacks, a vtable.
    Functions,
    /// A table of pointers to strings.
    Strings,
    /// A table of pointers to other data.
    Pointers,
    /// Indexed by the code: elements of `stride` bytes.
    Array,
    /// Records of `stride` bytes, each holding a pointer in the same place.
    Records,
    /// A structure holding pointers among its fields.
    Structure,
    /// Function pointers filled at run time and called through.
    FunctionPointers,
    /// A table of the addresses a `switch` jumps to.
    JumpTable,
    /// Its address is taken, and nothing more is known.
    Unknown,
}

/// An offset the code reaches through a global.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalField {
    pub offset: u64,
    pub width: u8,
    /// `r`, `w`, `rw`, or `call` for a slot called through.
    pub access: String,
    pub float: bool,
}

/// One global: where it is, what it is and how it is used.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Global {
    pub address: u64,
    pub size: u64,
    pub name: String,
    pub kind: GlobalKind,
    /// The width the code reads or writes it with, when it does.
    pub width: u8,
    /// A table's entries, or an array's elements as far as the next global.
    pub count: u32,
    /// An array's element size.
    pub stride: u32,
    pub reads: u32,
    pub writes: u32,
    pub addressed: u32,
    pub calls: u32,
    /// Pointers in data to it.
    pub pointed: u32,
    /// Offsets reached through it: the fields of what a pointer points to,
    /// the slots of a table of function pointers, an array element's fields.
    pub fields: Vec<GlobalField>,
    /// What it is, in a few words.
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalPage {
    pub total: u32,
    pub offset: u32,
    pub globals: Vec<Global>,
}

/// Where the globals start: every data address referred to, sorted.
pub(crate) struct GlobalIndex {
    starts: Vec<u64>,
}

/// How far a structure, an array or a table may reach past its start.
const MAX_EXTENT: u64 = 1 << 20;
/// Slots of a table of function pointers are no further apart than this.
const MAX_SLOT_GAP: u64 = 0x400;
/// Instructions looked at per global to work out its width and use.
const MAX_SITES: usize = 16;

/// How one instruction uses the memory of a global.
#[derive(Debug, Clone, Copy, Default)]
struct Use {
    width: u8,
    float: bool,
    /// An index register reaches into it, scaled by this.
    stride: u8,
}

impl Binary {
    pub(crate) fn global_index(&self) -> &GlobalIndex {
        let index = self.xref_index();
        index.globals.get_or_init(|| {
            let mut starts: Vec<u64> = [
                RefKind::Read,
                RefKind::Write,
                RefKind::Address,
                RefKind::Call,
                RefKind::Jump,
                RefKind::Pointer,
            ]
            .into_iter()
            .flat_map(|k| index.targets(k))
            .filter(|&t| self.is_data(t))
            .collect();
            starts.sort_unstable();
            starts.dedup();
            GlobalIndex { starts }
        })
    }

    /// Whether an address holds data: in a loaded section that isn't code
    /// or, where a console mixes the two, outside every function.
    fn is_data(&self, a: u64) -> bool {
        match self.section_at(a) {
            Some(s) if s.kind == RegionKind::Code => {
                self.rom.is_some() && self.symbols.function_containing(a).is_none()
            }
            Some(s) => s.loaded,
            None => false,
        }
    }

    /// The globals whose names contain `filter` (all of them for an empty
    /// one), in address order, a page at a time.
    pub fn globals(&self, filter: &str, offset: u32, limit: u32) -> GlobalPage {
        let starts = &self.global_index().starts;
        let filter = filter.to_ascii_lowercase();
        let limit = if limit == 0 { 100 } else { limit } as usize;
        let mut total = 0u32;
        let mut out = Vec::new();
        let mut i = 0;
        while i < starts.len() {
            let g = self
                .slot_table(i)
                .filter(|t| t.address == starts[i])
                .unwrap_or_else(|| self.classify(i));
            // What a global covers isn't listed again (the entries of a table).
            let next = starts.partition_point(|&s| s < g.address + g.size.max(1)).max(i + 1);
            if filter.is_empty()
                || g.name.to_ascii_lowercase().contains(&filter)
                || g.description.to_ascii_lowercase().contains(&filter)
            {
                if total as usize >= offset as usize && out.len() < limit {
                    out.push(g);
                }
                total += 1;
            }
            i = next;
        }
        GlobalPage {
            total,
            offset,
            globals: out,
        }
    }

    /// The global covering `address`, if the code or data refers to one there.
    pub fn global_at(&self, address: u64) -> Option<Global> {
        let starts = &self.global_index().starts;
        let i = starts.partition_point(|&s| s <= address).checked_sub(1)?;
        // A slot of a table of function pointers is the table's.
        if let Some(t) = self.slot_table(i)
            && address < t.address + t.size
        {
            return Some(t);
        }
        let g = self.classify(i);
        (address < g.address + g.size.max(1)).then_some(g)
    }

    /// The name of the global covering `address`, with the offset into it:
    /// `flt_4020a0`, `fptrs_20066ee0+0x30`. Only once references are indexed.
    pub(crate) fn global_name(&self, address: u64) -> Option<String> {
        self.xrefs.get()?;
        let g = self.global_at(address)?;
        // A lone pointer is best named by what it points to.
        if g.kind == GlobalKind::Pointer && g.fields.is_empty() {
            return None;
        }
        Some(if address == g.address {
            g.name
        } else {
            format!("{}+{:#x}", g.name, address - g.address)
        })
    }

    /// References to exactly `a`, by kind.
    fn uses_of(&self, a: u64) -> (u32, u32, u32, u32, u32) {
        let index = self.xref_index();
        let n = |k: RefKind| index.range(k, a, a + 1).len() as u32;
        (
            n(RefKind::Read),
            n(RefKind::Write),
            n(RefKind::Address),
            n(RefKind::Call) + n(RefKind::Jump),
            n(RefKind::Pointer),
        )
    }

    /// The table of function pointers the global `i` is a slot of: slots
    /// called through, the first of them (or the address taken before them,
    /// where the table is filled by copying) its start.
    fn slot_table(&self, i: usize) -> Option<Global> {
        let starts = &self.global_index().starts;
        let (.., calls, _) = self.uses_of(starts[i]);
        // Slots the file already fills are a table of callbacks, not filled at run time.
        if calls == 0 || self.symbols.at(starts[i]).is_some() || self.table_word(starts[i]).is_some() {
            return None;
        }
        // A slot: called through (a slot loaded into a register first is read too), its address never taken.
        let slot = |j: usize| {
            let (_, _, addressed, calls, _) = self.uses_of(starts[j]);
            calls > 0 && addressed == 0
        };
        // Back to the first slot, or to where the table is filled by copying another over it.
        let mut first = i;
        while first > 0 && starts[first] - starts[first - 1] <= MAX_SLOT_GAP {
            if slot(first - 1) {
                first -= 1;
                continue;
            }
            let (_, _, addressed, _, _) = self.uses_of(starts[first - 1]);
            if addressed > 0 && self.filled_by_copy(starts[first - 1]).is_some() {
                first -= 1;
            }
            break;
        }
        let mut last = i;
        while last + 1 < starts.len() && starts[last + 1] - starts[last] <= MAX_SLOT_GAP && slot(last + 1) {
            last += 1;
        }
        let word = if self.is64 { 8 } else { 4 };
        let start = starts[first];
        let filled = self.filled_by_copy(start);
        // As far as the last slot called through, or the words the copy that fills it writes.
        let end = (starts[last] + word).max(start + filled.map_or(0, |f| f.1 * word));
        let (reads, writes, addressed, _, pointed) = self.uses_of(start);
        let mut fields = Vec::new();
        let mut calls = 0;
        for &a in &starts[first..=last] {
            let (.., c, _) = self.uses_of(a);
            if c > 0 {
                calls += c;
                fields.push(GlobalField {
                    offset: a - start,
                    width: word as u8,
                    access: "call".into(),
                    float: false,
                });
            }
        }
        let description = format!(
            "table of function pointers filled at run time: {} slot{} called through ({} calls){}",
            fields.len(),
            if fields.len() == 1 { "" } else { "s" },
            calls,
            filled.map_or(String::new(), |(site, words)| format!(
                "; filled at {} by copying {words} words",
                self.place(site).unwrap_or_else(|| format!("{site:#x}"))
            ))
        );
        Some(Global {
            address: start,
            size: end - start,
            name: format!("fptrs_{start:x}"),
            kind: GlobalKind::FunctionPointers,
            width: word as u8,
            count: ((end - start) / word) as u32,
            stride: 0,
            reads,
            writes,
            addressed,
            calls,
            pointed,
            fields,
            description,
        })
    }

    /// Where a table at `start` is filled by copying another over it (`mov
    /// edi, offset table` … `rep movsd`): the copy, and how many words.
    fn filled_by_copy(&self, start: u64) -> Option<(u64, u64)> {
        if !self.is_x86() {
            return None;
        }
        let index = self.xref_index();
        for &v in index.range(RefKind::Address, start, start + 1) {
            let site = index.source(v);
            // The count may be set before the address is taken: read the function up to the copy.
            let Some(f) = self.symbols.function_containing(site) else {
                continue;
            };
            let mut pc = f.address;
            let mut count = None;
            let mut after = None;
            while pc < f.address + f.size && after.is_none_or(|n| n < 16) {
                let Some(ins) = self.x86_at(pc) else { break };
                if ins.mnemonic() == Mnemonic::Mov
                    && ins.op0_kind() == OpKind::Register
                    && ins.op0_register() == Register::ECX
                    && matches!(ins.op1_kind(), OpKind::Immediate32 | OpKind::Immediate8to32)
                {
                    count = Some(ins.immediate(1) & 0xFFFF_FFFF);
                }
                if let Some(n) = after.as_mut() {
                    *n += 1;
                    if ins.mnemonic() == Mnemonic::Movsd && ins.has_rep_prefix() {
                        return Some((pc, count.unwrap_or(0)));
                    }
                }
                if pc == site {
                    after = Some(0);
                }
                pc = ins.next_ip();
            }
        }
        None
    }

    fn is_x86(&self) -> bool {
        self.rom.is_none() && matches!(self.arch, object::Architecture::I386 | object::Architecture::X86_64)
    }

    fn x86_at(&self, pc: u64) -> Option<Instruction> {
        let bits = if self.is64 { 64 } else { 32 };
        let offset = self.address_to_offset(pc)? as usize;
        let bytes = self.data.get(offset..(offset + 15).min(self.data.len()))?;
        let ins = Decoder::with_ip(bits, bytes, pc, DecoderOptions::NONE).decode();
        (!ins.is_invalid()).then_some(ins)
    }

    /// How the instruction at `site` uses the memory at `target`.
    fn use_at(&self, site: u64, _target: u64) -> Option<Use> {
        if self.is_x86() {
            let ins = self.x86_at(site)?;
            if !(0..ins.op_count()).any(|i| ins.op_kind(i) == OpKind::Memory) || ins.mnemonic() == Mnemonic::Lea {
                return None;
            }
            let size = ins.memory_size();
            return Some(Use {
                width: size.size().min(255) as u8,
                float: matches!(
                    size,
                    MemorySize::Float32 | MemorySize::Float64 | MemorySize::Float80 | MemorySize::Float16
                ),
                stride: if ins.memory_index() != Register::None {
                    ins.memory_index_scale() as u8
                } else {
                    0
                },
            });
        }
        // MIPS: the load or store naming the address with its `lui` pair.
        if matches!(self.arch, object::Architecture::Mips) {
            let offset = self.address_to_offset(site)? as usize;
            let b: [u8; 4] = self.data.get(offset..offset + 4)?.try_into().ok()?;
            let w = crate::cpu::mips::MipsWord(match self.endian {
                crate::util::Endian::Little => u32::from_le_bytes(b),
                crate::util::Endian::Big => u32::from_be_bytes(b),
            });
            // Loads and stores (lwc1/swc1, ldc1/sdc1 for the FPU): width, and a float.
            let (width, float) = match w.op() {
                32 | 36 | 40 => (1, false),
                33 | 37 | 41 => (2, false),
                34 | 35 | 38 | 39 | 42 | 43 | 46 => (4, false),
                49 | 57 => (4, true),
                53 | 61 => (8, true),
                _ => return None,
            };
            return Some(Use {
                width,
                float,
                stride: 0,
            });
        }
        None
    }

    /// The offsets reached through a pointer read from the global at `a`:
    /// after `mov reg, [a]`, what `[reg+N]` the next instructions use.
    fn fields_through(&self, a: u64) -> Vec<GlobalField> {
        let mut fields: Vec<GlobalField> = Vec::new();
        if !self.is_x86() {
            return fields;
        }
        let index = self.xref_index();
        let mut info = iced_x86::InstructionInfoFactory::new();
        for &v in index.range(RefKind::Read, a, a + 1).iter().take(MAX_SITES) {
            let site = index.source(v);
            let Some(load) = self.x86_at(site) else { continue };
            if load.mnemonic() != Mnemonic::Mov || load.op0_kind() != OpKind::Register || !load.op0_register().is_gpr()
            {
                continue;
            }
            let reg = load.op0_register().full_register();
            let mut pc = load.next_ip();
            for _ in 0..8 {
                let Some(ins) = self.x86_at(pc) else { break };
                let memory = (0..ins.op_count()).any(|i| ins.op_kind(i) == OpKind::Memory);
                if memory && ins.memory_base().full_register() == reg && ins.mnemonic() != Mnemonic::Lea {
                    let size = ins.memory_size();
                    let used = info.info(&ins);
                    let store = used.used_memory().iter().any(|m| {
                        !matches!(m.base(), Register::ESP | Register::RSP)
                            && matches!(
                                m.access(),
                                OpAccess::Write | OpAccess::CondWrite | OpAccess::ReadWrite | OpAccess::ReadCondWrite
                            )
                    });
                    let offset = if self.is64 {
                        ins.memory_displacement64()
                    } else {
                        ins.memory_displacement32() as u64
                    };
                    if offset < 0x10000 {
                        let mode = if store { "w" } else { "r" };
                        let width = size.size().min(255) as u8;
                        match fields.iter_mut().find(|f| f.offset == offset && f.width == width) {
                            Some(f) if !f.access.contains(mode) => f.access = "rw".into(),
                            Some(_) => {}
                            None => fields.push(GlobalField {
                                offset,
                                width,
                                access: mode.into(),
                                float: matches!(size, MemorySize::Float32 | MemorySize::Float64),
                            }),
                        }
                    }
                }
                // The register's value is gone once it is written, or the path leaves.
                let used = info.info(&ins);
                let written = used.used_registers().iter().any(|u| {
                    u.register().is_gpr()
                        && u.register().full_register() == reg
                        && matches!(u.access(), OpAccess::Write | OpAccess::ReadWrite | OpAccess::CondWrite)
                });
                if written || ins.flow_control() != iced_x86::FlowControl::Next {
                    break;
                }
                pc = ins.next_ip();
            }
        }
        fields.sort_by_key(|f| (f.offset, f.width));
        fields
    }

    /// The pointer-sized word at `a`, when it is an address in the image:
    /// what the reference index found there, or in a console's image, any
    /// aligned word pointing into it.
    fn table_word(&self, a: u64) -> Option<u64> {
        if self.rom.is_none() {
            return self.pointer_at(a);
        }
        let word = if self.is64 { 8 } else { 4 };
        if !a.is_multiple_of(word) {
            return None;
        }
        self.read_word(a)
            .filter(|&t| t != 0 && t.is_multiple_of(4) && self.section_at(t).is_some())
    }

    /// What the global starting at `starts[i]` is.
    fn classify(&self, i: usize) -> Global {
        let mut g = self.classify_by_use(i);
        // A symbol that says how big it is knows better.
        if let Some(sym) = self.symbols.at(g.address).filter(|s| s.size > 0 && !s.size_inferred) {
            g.size = sym.size;
        }
        g
    }

    /// [`Self::classify`], from the global's uses alone.
    fn classify_by_use(&self, i: usize) -> Global {
        let starts = &self.global_index().starts;
        let a = starts[i];
        let word = if self.is64 { 8 } else { 4 };
        let section_end = self.section_at(a).map_or(a + 1, |s| s.address + s.size);
        let next = starts.get(i + 1).copied().unwrap_or(section_end).min(section_end);
        let (reads, writes, addressed, calls, pointed) = self.uses_of(a);
        let mut g = Global {
            address: a,
            size: (next - a).max(1),
            name: String::new(),
            kind: GlobalKind::Unknown,
            width: 0,
            count: 0,
            stride: 0,
            reads,
            writes,
            addressed,
            calls,
            pointed,
            fields: Vec::new(),
            description: String::new(),
        };
        let uses = format!(
            "{}{}{}{}",
            if reads > 0 {
                format!(", read {reads}×")
            } else {
                String::new()
            },
            if writes > 0 {
                format!(", written {writes}×")
            } else {
                String::new()
            },
            if addressed > 0 {
                format!(", address taken {addressed}×")
            } else {
                String::new()
            },
            if calls > 0 {
                format!(", called through {calls}×")
            } else {
                String::new()
            }
        );
        // A jump table the code follower found.
        if let Some(t) = self.jump_table_at(a) {
            g.kind = GlobalKind::JumpTable;
            g.size = t.end() - t.address;
            g.count = t.count;
            g.name = format!("jpt_{a:x}");
            use crate::discover::x86::Entries;
            g.description = match t.entries {
                _ if t.is_index() => format!("the {} bytes picking a case of the switch at {:#x}", t.count, t.jump),
                Entries::Relative { base, .. } if base == self.image_base => format!(
                    "jump table of {} cases (offsets from the image base), for the switch at {:#x}",
                    t.count, t.jump
                ),
                Entries::Relative { .. } => format!(
                    "jump table of {} cases (offsets from the table), for the switch at {:#x}",
                    t.count, t.jump
                ),
                Entries::Absolute => format!("jump table of {} cases, for the switch at {:#x}", t.count, t.jump),
            };
            return g;
        }
        if let Some(text) = self.string_at_address(a) {
            g.kind = GlobalKind::String;
            g.size = text.len() as u64 + 1;
            g.name = format!("str_{a:x}");
            g.description = format!("string {text:?}{uses}");
            return g;
        }
        // Pointers stored in it: a table, as long as they go on and nothing else starts.
        if self.table_word(a).is_some() {
            let limit = starts[i + 1..]
                .iter()
                .copied()
                .find(|&s| {
                    let (r, w, ad, c, _) = self.uses_of(s);
                    r + w + ad + c > 0
                })
                .unwrap_or(section_end)
                .min(section_end)
                .min(a + MAX_EXTENT);
            let mut targets = Vec::new();
            let mut at = a;
            while at + word <= limit {
                let Some(t) = self.table_word(at) else { break };
                targets.push(t);
                at += word;
            }
            let code = |t: u64| self.section_at(t).is_some_and(|s| s.kind == RegionKind::Code);
            let n = targets.len() as u32;
            g.size = n as u64 * word;
            g.count = n;
            g.width = word as u8;
            let names: Vec<String> = targets
                .iter()
                .take(4)
                .map(|&t| self.name_for(t).unwrap_or_else(|| format!("{t:#x}")))
                .collect();
            let more = if n > 4 {
                format!(", … {} more", n - 4)
            } else {
                String::new()
            };
            if n == 1 {
                // Other fields between pointers: records, or a structure holding pointers.
                if let Some(rec) = self.records(&mut g, next, &uses) {
                    return rec;
                }
                g.kind = GlobalKind::Pointer;
                g.name = format!("ptr_{a:x}");
                g.description = format!("pointer to {}{uses}", names[0]);
            } else if targets.iter().all(|&t| code(t)) {
                g.kind = GlobalKind::Functions;
                g.name = format!("funcs_{a:x}");
                g.description = format!("table of {n} code pointers: {}{more}{uses}", names.join(", "));
            } else if targets.iter().all(|&t| self.string_at_address(t).is_some()) {
                g.kind = GlobalKind::Strings;
                g.name = format!("strs_{a:x}");
                g.description = format!("table of {n} string pointers: {}{more}{uses}", names.join(", "));
            } else {
                g.kind = GlobalKind::Pointers;
                g.name = format!("ptrs_{a:x}");
                g.description = format!("table of {n} pointers: {}{more}{uses}", names.join(", "));
            }
            return g;
        }
        // How the code reads and writes it.
        let index = self.xref_index();
        let mut all: Vec<Use> = Vec::new();
        for kind in [RefKind::Read, RefKind::Write] {
            for &v in index.range(kind, a, a + 1).iter().take(MAX_SITES) {
                if let Some(u) = self.use_at(index.source(v), a) {
                    all.push(u);
                }
            }
        }
        let width = all.iter().map(|u| u.width).max().unwrap_or(0);
        let float = all.iter().any(|u| u.float);
        let stride = all.iter().map(|u| u.stride).max().unwrap_or(0);
        g.width = width;
        if stride > 0 {
            g.kind = GlobalKind::Array;
            g.stride = stride as u32;
            g.size = (next - a).clamp(1, MAX_EXTENT);
            g.count = (g.size / stride as u64) as u32;
            g.name = format!("arr_{a:x}");
            g.description = format!(
                "array of {}-byte elements{}, about {} to the next global{uses}{}",
                stride,
                if float { " (floats)" } else { "" },
                g.count,
                self.value_range(a, stride as u64, g.count.min(256)).unwrap_or_default()
            );
            return g;
        }
        if width > 0 {
            g.size = (width as u64).min(next - a).max(1);
            let constant = writes == 0 && self.address_to_offset(a).is_some();
            let value = if constant { self.value_at(a, width, float) } else { None };
            let fields = if reads > 0 { self.fields_through(a) } else { Vec::new() };
            if !fields.is_empty() && width as u64 == word {
                g.kind = GlobalKind::Pointer;
                g.name = format!("ptr_{a:x}");
                let list: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        format!(
                            "{:#x}:{}{}",
                            f.offset,
                            if f.float {
                                format!("f{}", f.width as u32 * 8)
                            } else {
                                format!("u{}", f.width as u32 * 8)
                            },
                            f.access
                        )
                    })
                    .collect();
                g.description = format!(
                    "pointer to a structure: the code reaches {{{}}} through it{uses}",
                    list.join(", ")
                );
                g.fields = fields;
                return g;
            }
            let (kind, prefix, what) = match (float, width) {
                (true, 4) => (GlobalKind::Float, "flt", "float".to_string()),
                (true, 8) => (GlobalKind::Double, "dbl", "double".to_string()),
                (true, _) => (GlobalKind::Float, "flt", format!("{}-byte float", width)),
                (false, 1) => (GlobalKind::Integer, "byte", "byte".to_string()),
                (false, 2) => (GlobalKind::Integer, "word", "16-bit integer".to_string()),
                (false, 4) => (GlobalKind::Integer, "dword", "32-bit integer".to_string()),
                (false, 8) => (GlobalKind::Integer, "qword", "64-bit integer".to_string()),
                (false, n) => (GlobalKind::Integer, "xmmword", format!("{n}-byte value")),
            };
            g.kind = kind;
            g.name = format!("{prefix}_{a:x}");
            // Read-only data is a constant; writable data the code never writes may be written from outside.
            let read_only = self.section_at(a).is_some_and(|s| !s.perms.contains('w'));
            g.description = format!(
                "{what}{}{uses}",
                match value {
                    Some(v) if read_only => format!(" = {v}, never written: a constant"),
                    Some(v) => format!(" = {v} in the file, never written by the code"),
                    None if writes == 0 => ", never written by the code".to_string(),
                    None => String::new(),
                }
            );
            return g;
        }
        if addressed + pointed > 0
            && let Some(rec) = self.records(&mut g, next, &uses)
        {
            return rec;
        }
        g.size = (next - a).clamp(1, 0x1000);
        g.name = format!("unk_{a:x}");
        g.description = if addressed > 0 {
            format!("its address is taken ({addressed}×), nothing reads it directly")
        } else if pointed > 0 {
            format!(
                "pointed to by {pointed} pointer{} in data",
                if pointed == 1 { "" } else { "s" }
            )
        } else {
            format!("used by the code{uses}")
        };
        if calls > 0
            && self
                .symbols
                .at(a)
                .is_some_and(|s| s.source == crate::model::SymbolSource::Import)
        {
            g.description = format!("import address table slot{uses}");
        }
        // A console's table of numbers, reached by its address.
        if self.rom.is_some() && g.size >= 8 && a.is_multiple_of(2) {
            let halves = (g.size / 2).min(256) as u32;
            if let Some(range) = self.value_range(a, 2, halves) {
                g.description
                    .push_str(&format!("; as half-words{}", range.trim_start_matches(';')));
            }
        }
        g
    }

    /// Data up to `next` that holds pointers between other fields: records
    /// of `stride` bytes, each with a pointer in the same place (a table of
    /// frames, each naming its callback), or one structure holding pointers.
    fn records(&self, g: &mut Global, next: u64, uses: &str) -> Option<Global> {
        let word = if self.is64 { 8 } else { 4 };
        let a = g.address;
        let end = next.min(a + 0x1000);
        if end < a + 2 * word {
            return None;
        }
        let pointers: Vec<(u64, u64)> = (a..end)
            .step_by(word as usize)
            .filter_map(|at| Some((at - a, self.table_word(at)?)))
            .collect();
        let first = pointers.first()?.0;
        let len = end - a;
        let name = |t: u64| self.name_for(t).unwrap_or_else(|| format!("{t:#x}"));
        // Records: a stride that divides the extent, with a pointer at the first one's place in each.
        let stride = (2..=64)
            .map(|k| k * word)
            .filter(|&s| s < len && len.is_multiple_of(s))
            .find(|&s| (0..len / s).all(|k| pointers.binary_search_by_key(&(k * s + first), |p| p.0).is_ok()));
        let mut out = g.clone();
        out.size = len;
        if let Some(stride) = stride {
            let count = len / stride;
            // What each field of the records points at: `+0 → ai_stand ×3`.
            let mut fields: Vec<(u64, Vec<String>)> = Vec::new();
            for &(off, t) in &pointers {
                let f = off % stride;
                match fields.iter_mut().find(|x| x.0 == f) {
                    Some(x) => x.1.push(name(t)),
                    None => fields.push((f, vec![name(t)])),
                }
            }
            fields.sort_by_key(|x| x.0);
            let described: Vec<String> = fields
                .iter()
                .map(|(f, names)| {
                    let mut distinct: Vec<&String> = names.iter().collect();
                    distinct.dedup();
                    let list: Vec<String> = distinct
                        .iter()
                        .take(3)
                        .map(|n| {
                            let k = names.iter().filter(|m| m == n).count();
                            if k > 1 { format!("{n} ×{k}") } else { n.to_string() }
                        })
                        .collect();
                    format!("+{f:#x} → {}", list.join(", "))
                })
                .collect();
            out.kind = GlobalKind::Records;
            out.stride = stride as u32;
            out.count = count as u32;
            out.name = format!("stru_{a:x}");
            out.description = format!(
                "table of {count} records of {stride} bytes holding pointers: {}{uses}",
                described.join("; ")
            );
            return Some(out);
        }
        let described: Vec<String> = pointers
            .iter()
            .take(6)
            .map(|&(off, t)| format!("+{off:#x} → {}", name(t)))
            .collect();
        out.kind = GlobalKind::Structure;
        out.name = format!("stru_{a:x}");
        out.description = format!(
            "structure of {len} bytes (to the next global) holding pointers: {}{uses}",
            described.join(", ")
        );
        Some(out)
    }

    /// The value of a constant of `width` bytes at `a`, as the code reads it.
    fn value_at(&self, a: u64, width: u8, float: bool) -> Option<String> {
        let off = self.address_to_offset(a)? as usize;
        let b = self.data.get(off..off + width as usize)?;
        let little = self.endian == crate::util::Endian::Little;
        let int = |b: &[u8]| {
            let mut v = 0u64;
            for (i, &x) in b.iter().enumerate() {
                let shift = if little { i } else { b.len() - 1 - i };
                v |= (x as u64) << (8 * shift);
            }
            v
        };
        Some(match (float, width) {
            (true, 4) => format!("{:?}", f32::from_bits(int(b) as u32)),
            (true, 8) => format!("{:?}", f64::from_bits(int(b))),
            (false, 1 | 2 | 4 | 8) => {
                let v = int(b);
                if v < 10 { v.to_string() } else { format!("{v:#x}") }
            }
            _ => return None,
        })
    }

    /// The range of an array's values, and on the PlayStation, whether they
    /// read as 4.12 fixed point (4096 is 1.0, what the GTE works in).
    fn value_range(&self, a: u64, stride: u64, count: u32) -> Option<String> {
        if !matches!(stride, 2 | 4) || count < 4 {
            return None;
        }
        let mut lo = i64::MAX;
        let mut hi = i64::MIN;
        for k in 0..count as u64 {
            let off = self.address_to_offset(a + k * stride)? as usize;
            let b = self.data.get(off..off + stride as usize)?;
            let v = match (stride, self.endian) {
                (2, crate::util::Endian::Little) => i16::from_le_bytes([b[0], b[1]]) as i64,
                (2, _) => i16::from_be_bytes([b[0], b[1]]) as i64,
                (_, crate::util::Endian::Little) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64,
                _ => i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as i64,
            };
            lo = lo.min(v);
            hi = hi.max(v);
        }
        let psx = self.rom.as_ref().is_some_and(|r| r.cpu == crate::cpu::Cpu::MipsR3000);
        let fixed = psx && lo >= -4096 && hi <= 4096 && hi - lo > 256;
        Some(format!(
            "; values {lo}..{hi}{}",
            if fixed {
                ", within ±4096: 4.12 fixed point (4096 is 1.0), perhaps"
            } else {
                ""
            }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    #[test]
    fn playstation_data_is_typed_by_its_use() {
        // A PS-X EXE at 0x80010000: code reading a word and a half-word
        // through lui pairs, then a table of two code pointers and a table
        // of fixed-point values.
        let mut data = vec![0u8; 0x800];
        data[..8].copy_from_slice(b"PS-X EXE");
        let words = [
            0x3C02_8001u32, // lui $v0, 0x8001
            0x8C43_0040,    // lw $v1, 0x40($v0): the word at 0x80010040
            0x3C04_8001,    // lui $a0, 0x8001
            0x2484_0050,    // addiu $a0, $a0, 0x50: the address of the table at 0x80010050
            0x3C05_8001,    // lui $a1, 0x8001
            0x24A5_0048,    // addiu $a1, $a1, 0x48: the address of 0x80010048
            0x03E0_0008,    // jr $ra
            0x0000_0000,
        ];
        let mut code = le(&words);
        code.resize(0x40, 0);
        code.extend(le(&[0x1234_5678, 0])); // 0x80010040: the word
        code.extend(le(&[0x8001_0000, 0x8001_0000])); // 0x80010048: two pointers to entry
        let table: Vec<u8> = (0..16i16).flat_map(|k| (k * 256 - 2048).to_le_bytes()).collect();
        code.extend(&table); // 0x80010050: 16 half-words, -2048..1792
        for (at, v) in [
            (0x10, 0x8001_0000u32),
            (0x14, 0x8001_8000),
            (0x18, 0x8001_0000),
            (0x1C, code.len() as u32),
        ] {
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        data.extend(&code);
        let bin = Binary::parse(data).unwrap();
        bin.prepare_xrefs();
        let word = bin.global_at(0x8001_0040).unwrap();
        assert_eq!(
            (word.name.as_str(), word.kind, word.width),
            ("dword_80010040", GlobalKind::Integer, 4)
        );
        assert!(
            word.description.contains("= 0x12345678, never written: a constant"),
            "{word:?}"
        );
        let funcs = bin.global_at(0x8001_004C).unwrap();
        assert_eq!(
            (funcs.name.as_str(), funcs.kind, funcs.count),
            ("funcs_80010048", GlobalKind::Functions, 2)
        );
        assert_eq!(bin.name_for(0x8001_004C).as_deref(), Some("funcs_80010048+0x4"));
        let table = bin.global_at(0x8001_0050).unwrap();
        assert_eq!(table.name, "unk_80010050");
        assert!(
            table
                .description
                .contains("-2048..1792, within ±4096: 4.12 fixed point"),
            "{table:?}"
        );
        let listed: Vec<String> = bin.globals("", 0, 10).globals.into_iter().map(|g| g.name).collect();
        assert_eq!(listed, ["dword_80010040", "funcs_80010048", "unk_80010050"]);
    }

    #[test]
    fn a_string_of_one_character_is_one_when_its_address_is_taken() {
        let mut data = vec![0u8; 0x800];
        data[..8].copy_from_slice(b"PS-X EXE");
        let words = [
            0x3C04_8001u32, // lui $a0, 0x8001
            0x2484_0020,    // addiu $a0, $a0, 0x20: the address of "m"
            0x3C05_8001,    // lui $a1, 0x8001
            0x8CA5_0024,    // lw $a1, 0x24($a1): a word read, not a string
            0x03E0_0008,    // jr $ra
            0x0000_0000,
            0,
            0,
        ];
        let mut code = le(&words);
        code.extend(b"m\0\0\0");
        code.extend(b"m\0\0\0");
        for (at, v) in [
            (0x10, 0x8001_0000u32),
            (0x14, 0x8001_8000),
            (0x18, 0x8001_0000),
            (0x1C, code.len() as u32),
        ] {
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        data.extend(&code);
        let bin = Binary::parse(data).unwrap();
        bin.prepare_xrefs();
        assert_eq!(bin.string_at_address(0x8001_0020).as_deref(), Some("m"));
        assert_eq!(bin.string_at_address(0x8001_0024), None);
        let f = bin.function_summary(0x8001_0000, 10).unwrap();
        assert_eq!(f.strings.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["m"]);
    }
}
