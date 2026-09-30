//! The debug files the cc65 tools write (`ld65 --dbgfile game.dbg`): what
//! they know about a program they built, for the NES, the SNES and other 6502
//! and 65816 homebrew — its modules and source lines, its `.proc` scopes (and
//! C functions), its labels. Written as DWARF (see [`super::synth`]), so that
//! a game gets source lines, functions and units like a binary with debug
//! info. Records are lines of `kind<TAB>key=value,...`:
//!
//! ```text
//! seg    id=1,name="CODE",start=0x008000,size=0x0123,addrsize=absolute,type=ro,oname="game.nes",ooffs=16
//! span   id=5,seg=1,start=16,size=3
//! line   id=40,file=1,line=12,span=5
//! scope  id=3,name="update",mod=0,type=scope,size=64,parent=0,sym=7,span=5+6
//! sym    id=7,name="update",addrsize=absolute,size=64,scope=0,def=38,val=0x8010,seg=1,type=lab
//! ```
//!
//! A span is part of a segment, `start` bytes in; a segment written to the
//! output file (`oname`) begins `ooffs` bytes into it, so each span is found
//! by its place in the file, bank and all.

use std::collections::{BTreeMap, HashMap};

use gimli::SectionId;

use super::synth::{self, Function, Global, Module, Row};
use crate::error::{Error, Result};
use crate::model::SymbolKind;

/// Whether a file is one of ld65's debug files.
pub(crate) fn is_ca65(data: &[u8]) -> bool {
    data.starts_with(b"version\tmajor=")
}

/// A debug file read and turned into DWARF.
pub(crate) struct Converted {
    pub sections: Vec<(SectionId, Vec<u8>)>,
    /// `.proc`s and C functions, then labels: (name, address, size, kind).
    pub symbols: Vec<(String, u64, u64, SymbolKind)>,
    pub modules: u32,
    /// Spans the file places in the ROM, and of those how many land in it.
    pub spans: u32,
    pub placed: u32,
}

/// One record's `key=value` pairs.
struct Record<'a>(Vec<(&'a str, &'a str)>);

impl<'a> Record<'a> {
    fn parse(text: &'a str) -> Record<'a> {
        let mut out = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            let Some((key, after)) = rest.split_once('=') else {
                break;
            };
            let (value, next) = if let Some(quoted) = after.strip_prefix('"') {
                match quoted.find('"') {
                    Some(end) => (&quoted[..end], quoted[end + 1..].strip_prefix(',').unwrap_or("")),
                    None => (quoted, ""),
                }
            } else {
                match after.split_once(',') {
                    Some((v, n)) => (v, n),
                    None => (after, ""),
                }
            };
            out.push((key.trim(), value));
            rest = next;
        }
        Record(out)
    }

    fn str(&self, key: &str) -> Option<&'a str> {
        self.0.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }

    fn num(&self, key: &str) -> Option<u64> {
        let v = self.str(key)?;
        match v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
            Some(h) => u64::from_str_radix(h, 16).ok(),
            None => v.parse().ok(),
        }
    }

    fn id(&self, key: &str) -> Option<u32> {
        self.num(key).map(|n| n as u32)
    }

    /// `1+2+3`.
    fn list(&self, key: &str) -> Vec<u32> {
        self.str(key)
            .map(|v| v.split('+').filter_map(|n| n.parse().ok()).collect())
            .unwrap_or_default()
    }
}

struct Seg {
    start: u64,
    /// Where it is in the output file.
    offset: Option<u64>,
}

struct Span {
    seg: u32,
    start: u64,
    size: u64,
}

struct Scope {
    name: String,
    module: u32,
    kind: String,
    size: u64,
    parent: Option<u32>,
    sym: Option<u32>,
    spans: Vec<u32>,
}

struct Sym {
    name: String,
    scope: Option<u32>,
    value: u64,
    seg: Option<u32>,
    size: u64,
    label: bool,
}

/// Reads a debug file. `place` gives our address for a byte of the output
/// file (by its offset), `resolve` ours for a CPU address outside the file (RAM).
pub(crate) fn convert(
    text: &str,
    place: &dyn Fn(u64) -> Option<u64>,
    resolve: &dyn Fn(u64) -> Option<u64>,
) -> Result<Converted> {
    let mut files: BTreeMap<u32, (String, Vec<u32>)> = BTreeMap::new();
    let mut mods: BTreeMap<u32, (String, u32)> = BTreeMap::new();
    let mut segs: HashMap<u32, Seg> = HashMap::new();
    let mut spans: HashMap<u32, Span> = HashMap::new();
    // (file, line, type, spans)
    let mut lines: Vec<(u32, u32, u64, Vec<u32>)> = Vec::new();
    let mut scopes: BTreeMap<u32, Scope> = BTreeMap::new();
    let mut syms: BTreeMap<u32, Sym> = BTreeMap::new();
    // C names for assembler symbols: (sym, name).
    let mut csyms: HashMap<u32, String> = HashMap::new();
    let mut version = false;
    for raw in text.lines() {
        let Some((kind, rest)) = raw.trim_start().split_once('\t') else {
            continue;
        };
        let r = Record::parse(rest.trim_end());
        match kind {
            "version" => version = r.num("major") == Some(2),
            "file" => {
                if let Some(id) = r.id("id") {
                    files.insert(id, (r.str("name").unwrap_or("").to_string(), r.list("mod")));
                }
            }
            "mod" => {
                if let (Some(id), Some(file)) = (r.id("id"), r.id("file")) {
                    mods.insert(id, (r.str("name").unwrap_or("").to_string(), file));
                }
            }
            "seg" => {
                if let (Some(id), Some(start)) = (r.id("id"), r.num("start")) {
                    segs.insert(
                        id,
                        Seg {
                            start,
                            offset: r.num("ooffs").filter(|_| r.str("oname").is_some()),
                        },
                    );
                }
            }
            "span" => {
                if let (Some(id), Some(seg), Some(start)) = (r.id("id"), r.id("seg"), r.num("start")) {
                    spans.insert(
                        id,
                        Span {
                            seg,
                            start,
                            size: r.num("size").unwrap_or(0),
                        },
                    );
                }
            }
            "line" => {
                if let (Some(file), Some(line)) = (r.id("file"), r.id("line")) {
                    lines.push((file, line, r.num("type").unwrap_or(0), r.list("span")));
                }
            }
            "scope" => {
                if let Some(id) = r.id("id") {
                    scopes.insert(
                        id,
                        Scope {
                            name: r.str("name").unwrap_or("").to_string(),
                            module: r.id("mod").unwrap_or(0),
                            kind: r.str("type").unwrap_or("file").to_string(),
                            size: r.num("size").unwrap_or(0),
                            parent: r.id("parent"),
                            sym: r.id("sym"),
                            spans: r.list("span"),
                        },
                    );
                }
            }
            "sym" => {
                if let Some(id) = r.id("id") {
                    syms.insert(
                        id,
                        Sym {
                            name: r.str("name").unwrap_or("").to_string(),
                            // Cheap locals (`@loop`) have a parent symbol instead.
                            scope: r.id("scope"),
                            value: r.num("val").unwrap_or(0),
                            seg: r.id("seg"),
                            size: r.num("size").unwrap_or(0),
                            label: r.str("type") == Some("lab"),
                        },
                    );
                }
            }
            "csym" => {
                if let (Some(sym), Some(name)) = (r.id("sym"), r.str("name")) {
                    csyms.insert(sym, name.to_string());
                }
            }
            _ => {}
        }
    }
    if !version {
        return Err(Error::new("not a debug file ld65 wrote (version 2)"));
    }
    // Where a span is: in the output file, found by its offset there; else (RAM) by its address.
    let span_at = |id: u32| -> Option<(u64, u64, bool)> {
        let s = spans.get(&id)?;
        let seg = segs.get(&s.seg)?;
        match seg.offset {
            Some(o) => Some((place(o + s.start)?, s.size, true)),
            None => Some((resolve(seg.start + s.start)?, s.size, false)),
        }
    };
    let sym_at = |s: &Sym| -> Option<u64> {
        match s.seg.and_then(|id| segs.get(&id)) {
            Some(seg) => match seg.offset {
                Some(o) => place(o + s.value.checked_sub(seg.start)?),
                None => resolve(s.value),
            },
            None => resolve(s.value),
        }
    };
    let file_bytes = spans
        .values()
        .filter(|s| segs.get(&s.seg).is_some_and(|g| g.offset.is_some()))
        .count() as u32;
    let placed = spans
        .keys()
        .filter(|&&id| span_at(id).is_some_and(|(_, _, in_file)| in_file))
        .count() as u32;
    // A scope's name within the scopes enclosing it: `player::update`.
    let qualified = |mut id: u32| -> String {
        let mut parts = Vec::new();
        for _ in 0..64 {
            let Some(s) = scopes.get(&id) else { break };
            if !s.name.is_empty() {
                parts.push(s.name.as_str());
            }
            match s.parent.filter(|&p| p != id) {
                Some(p) => id = p,
                None => break,
            }
        }
        parts.reverse();
        parts.join("::")
    };
    // Which module each span belongs to, by the scopes that list it.
    let mut span_module: HashMap<u32, u32> = HashMap::new();
    for s in scopes.values() {
        for &id in &s.spans {
            span_module.entry(id).or_insert(s.module);
        }
    }
    let mut modules: BTreeMap<u32, Module> = mods
        .iter()
        .map(|(&id, (name, file))| {
            let c = files.get(file).is_some_and(|f| f.0.ends_with(".c"));
            let module = Module {
                name: name.clone(),
                main: files.get(file).map(|f| f.0.clone()),
                producer: format!(
                    "{} (from {name}, in an ld65 debug file)",
                    if c { "cc65" } else { "ca65" }
                ),
                language: Some(if c {
                    gimli::DW_LANG_C89
                } else {
                    gimli::DW_LANG_Mips_Assembler
                }),
                ..Default::default()
            };
            (id, module)
        })
        .collect();
    // Source lines, the best one for each span: a C line over the assembler's,
    // the assembler's over a macro's.
    let rank = |t: u64| match t {
        1 => 0,
        0 => 1,
        _ => 2,
    };
    let mut best: HashMap<u32, (u64, u32, u32)> = HashMap::new();
    for (file, line, kind, list) in &lines {
        for &id in list {
            let e = best.entry(id).or_insert((*kind, *file, *line));
            if rank(*kind) < rank(e.0) {
                *e = (*kind, *file, *line);
            }
        }
    }
    let mut best: Vec<(u32, (u64, u32, u32))> = best.into_iter().collect();
    best.sort_unstable_by_key(|(id, _)| *id);
    for (span, (_, file, line)) in best {
        let Some((address, size, true)) = span_at(span) else {
            continue;
        };
        let module = span_module
            .get(&span)
            .copied()
            .or_else(|| files.get(&file).and_then(|f| f.1.first().copied()));
        let Some(m) = module.and_then(|id| modules.get_mut(&id)) else {
            continue;
        };
        if let Some((name, _)) = files.get(&file) {
            m.files.entry(file).or_insert_with(|| name.clone());
        }
        m.rows.push(Row {
            address,
            end: address + size.max(1),
            file,
            line,
        });
    }
    // Functions: `.proc` scopes (a C function's, by its C name).
    let mut symbols = Vec::new();
    let mut starts = std::collections::HashSet::new();
    for (&id, s) in &scopes {
        if s.kind != "scope" || s.size == 0 {
            continue;
        }
        let Some(address) = s
            .spans
            .iter()
            .filter_map(|&sp| span_at(sp))
            .filter(|p| p.2)
            .map(|p| p.0)
            .min()
        else {
            continue;
        };
        let name = s
            .sym
            .and_then(|sym| csyms.get(&sym).cloned())
            .unwrap_or_else(|| qualified(id));
        if let Some(m) = modules.get_mut(&s.module) {
            m.functions.push(Function {
                name: name.clone(),
                address,
                size: s.size,
                external: s.parent.is_some(),
                ..Default::default()
            });
        }
        starts.insert(address);
        symbols.push((name, address, s.size, SymbolKind::Function));
    }
    // Labels, named within their scopes; those in RAM are variables.
    for s in syms.values().filter(|s| s.label && !s.name.starts_with('@')) {
        let Some(address) = sym_at(s) else { continue };
        if starts.contains(&address) {
            continue;
        }
        let name = match s.scope {
            Some(scope) => {
                let outer = qualified(scope);
                if outer.is_empty() {
                    s.name.clone()
                } else {
                    format!("{outer}::{}", s.name)
                }
            }
            None => s.name.clone(),
        };
        let in_rom = s.seg.and_then(|id| segs.get(&id)).is_some_and(|g| g.offset.is_some());
        if !in_rom {
            let module = s.scope.and_then(|id| scopes.get(&id)).map(|sc| sc.module);
            if let Some(m) = module.and_then(|id| modules.get_mut(&id)) {
                m.variables.push(Global {
                    name: name.clone(),
                    address,
                    external: true,
                    ty: None,
                });
            }
        }
        symbols.push((
            name,
            address,
            s.size,
            if in_rom { SymbolKind::Label } else { SymbolKind::Data },
        ));
    }
    let modules: Vec<Module> = modules.into_values().filter(|m| !m.is_empty()).collect();
    let count = modules.len() as u32;
    Ok(Converted {
        sections: synth::write(modules, &[], "debug file")?,
        symbols,
        modules: count,
        spans: file_bytes,
        placed,
    })
}

#[cfg(test)]
mod tests {
    use super::{Record, convert};
    use crate::model::SymbolKind;

    #[test]
    fn c_functions_by_their_c_names() {
        let dbg = "version	major=2,minor=0
            file	id=0,name=\"game.c\",size=10,mtime=0x0,mod=0
            mod	id=0,name=\"game.o\",file=0
            seg	id=0,name=\"CODE\",start=0x008000,size=0x0010,addrsize=absolute,type=ro,oname=\"game.nes\",ooffs=16
            span	id=0,seg=0,start=0,size=16
            line	id=0,file=0,line=7,type=1,span=0
            scope	id=0,name=\"\",mod=0,size=16,span=0
            scope	id=1,name=\"_main\",mod=0,type=scope,size=16,parent=0,sym=0,span=0
            sym	id=0,name=\"_main\",addrsize=absolute,size=16,scope=0,val=0x8000,seg=0,type=lab
            csym	id=0,name=\"main\",scope=1,type=0,sc=ext,sym=0
";
        let c = convert(dbg, &|o| Some(0x8000 + o - 16), &Some).unwrap();
        assert_eq!(c.symbols[0], ("main".to_string(), 0x8000, 16, SymbolKind::Function));
        assert_eq!((c.modules, c.spans, c.placed), (1, 1, 1));
        assert!(convert("not a debug file", &|_| None, &|_| None).is_err());
    }

    #[test]
    fn records() {
        let r = Record::parse(r#"id=3,name="a, b",mod=0,type=scope,size=0x40,span=1+2+3"#);
        assert_eq!(r.str("name"), Some("a, b"));
        assert_eq!(r.num("size"), Some(0x40));
        assert_eq!(r.list("span"), [1, 2, 3]);
        assert_eq!(r.str("type"), Some("scope"));
        assert_eq!(r.num("missing"), None);
    }
}
