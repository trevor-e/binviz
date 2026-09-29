//! The fields code reaches through typed pointers: `[esi+0x21c]` is
//! `edict_t.enemy` when `esi` holds the function's `edict_t *self`.
//!
//! The structures come from the binary's debug info or a types file (an
//! object compiled from the program's headers with `-g`, or a PDB). What a
//! pointer points at comes from the function's debug info or the prototype a
//! note gives it, from the type a note gives a global, and from the types of
//! the fields pointers are loaded from; the stack walk says which register
//! holds which argument, and which pointer was loaded from where.

use std::collections::HashMap;
use std::sync::Arc;

use crate::binary::Binary;
use crate::dwarf::{DebugInfo, StructType};
use crate::error::{Result, bail};
use crate::stack::{Frame, Origin};

/// A structure a pointer points at, and the debug info describing it.
#[derive(Clone)]
struct Pointee<'a> {
    /// The name its type spells it by (`edict_t`), or a global's own name.
    name: String,
    st: StructType,
    debug: &'a DebugInfo,
}

impl Binary {
    /// Attaches a file whose debug info describes the program's types: an
    /// object or a binary built with debug info (the program's headers,
    /// compiled with `clang -g -fno-eliminate-unused-debug-types`), or a PDB.
    /// Its structures name the fields the code reaches through pointers that
    /// notes give a type (a function's prototype, a global's type).
    pub fn attach_types(&mut self, name: &str, data: impl Into<Arc<[u8]>>) -> Result<()> {
        let data: Arc<[u8]> = data.into();
        let debug = if crate::dwarf::pdb::is_pdb(&data) {
            let address_size = self.arch.address_size().map_or(8, |s| s.bytes());
            let converted = crate::dwarf::pdb::convert(&data, 0, address_size)?;
            DebugInfo::from_sections(converted.sections, gimli::RunTimeEndian::Little, name, &[], self.arch)?
        } else {
            let other = Binary::parse(data)?;
            match other.debug {
                Some(d) => d,
                None => bail!("{name} has no debug info to take types from (build it with -g)"),
            }
        };
        self.types_file = Some(debug);
        Ok(())
    }

    /// Where types are described: the binary's debug info (unless reading
    /// its types would take long unasked), then the types file.
    fn type_sources(&self) -> impl Iterator<Item = &DebugInfo> {
        self.own_types().into_iter().chain(self.types_file.iter())
    }

    /// The binary's debug info, when its types are cheap enough to read unasked.
    fn own_types(&self) -> Option<&DebugInfo> {
        self.debug.as_ref().filter(|d| d.types_cheap())
    }

    /// The structure or union a C type names (`level_locals_t`, `struct
    /// game_locals_s`), looked up in `first` and then everywhere.
    fn structure<'a>(&'a self, ty: &str, first: Option<&'a DebugInfo>) -> Option<Pointee<'a>> {
        let name = spelled_name(ty);
        if name.is_empty() {
            return None;
        }
        first.into_iter().chain(self.type_sources()).find_map(|debug| {
            Some(Pointee {
                name: name.clone(),
                st: debug.find_struct(ty)?,
                debug,
            })
        })
    }

    /// The structure a pointer type points at (`edict_t *`, `struct edict_s *`).
    fn pointee<'a>(&'a self, ty: &str, first: Option<&'a DebugInfo>) -> Option<Pointee<'a>> {
        let inner = ty.trim().strip_suffix('*')?.trim_end();
        if inner.ends_with('*') || inner.contains('(') {
            return None;
        }
        self.structure(inner, first)
    }

    /// The type a note gives the function or data at `address`.
    pub(crate) fn note_type(&self, address: u64) -> Option<&str> {
        let first = self.annotations.partition_point(|a| a.address < address);
        self.annotations[first..]
            .iter()
            .take_while(|a| a.address == address)
            .find_map(|a| a.ctype.as_deref())
            .filter(|t| !t.trim().is_empty())
    }

    /// The global of a structure type holding `address`, from the notes
    /// that type data or the debug info's variables: the structure (named
    /// after the global), and how far into it `address` is.
    fn typed_global(&self, address: u64) -> Option<(Pointee<'_>, u64)> {
        let end = self.annotations.partition_point(|a| a.address <= address);
        let noted = self.annotations[..end].iter().rev().take(64).find_map(|a| {
            let ty = a
                .ctype
                .as_deref()
                .filter(|t| !t.contains('(') && !t.trim_end().ends_with('*'))?;
            let mut p = self.structure(ty, None)?;
            let into = address - a.address;
            if into >= p.st.size {
                return None;
            }
            if !a.name.is_empty() {
                p.name = a.name.clone();
            }
            Some((p, into))
        });
        noted.or_else(|| {
            // A variable the debug info gives a structure type, where a symbol starts.
            let debug = self.own_types()?;
            let s = self.symbols.lookup(address)?;
            let start = address - s.offset;
            let (name, false, _, st) = debug.variable_struct(start)? else {
                return None;
            };
            (s.offset < st.size).then_some((Pointee { name, st, debug }, s.offset))
        })
    }

    /// The structure the pointer held by the global at `address` points at:
    /// from a note's type, the debug info's variable, or the field of a
    /// typed global it is.
    fn global_pointee(&self, address: u64) -> Option<Pointee<'_>> {
        if let Some(ty) = self.note_type(address) {
            return self.pointee(ty, None);
        }
        if let Some(debug) = self.own_types()
            && let Some((_, true, spelled, st)) = debug.variable_struct(address)
        {
            return Some(Pointee {
                name: spelled,
                st,
                debug,
            });
        }
        let (g, into) = self.typed_global(address)?;
        let f = g.debug.field_at(&g.st, into)?;
        if f.padding || f.delta != 0 {
            return None;
        }
        self.pointee(&f.type_name, Some(g.debug))
    }

    /// The name of the field of a typed global at `address` (`level.framenum`).
    pub(crate) fn global_field(&self, address: u64) -> Option<String> {
        let (p, into) = self.typed_global(address)?;
        if into == 0 {
            return None;
        }
        let f = p.debug.field_at(&p.st, into)?;
        Some(f.label(&p.name))
    }

    /// What each parameter of the function at `start` points at: from a
    /// note's prototype, else from the function's debug info.
    fn parameter_pointees(&self, start: u64) -> Vec<Option<Pointee<'_>>> {
        if let Some(prototype) = self.note_type(start) {
            return parameter_types(prototype)
                .iter()
                .map(|t| self.pointee(t, None))
                .collect();
        }
        let Some(debug) = self.own_types() else {
            return Vec::new();
        };
        debug
            .parameter_structs(start)
            .into_iter()
            .map(|p| p.map(|(name, st)| Pointee { name, st, debug }))
            .collect()
    }

    /// The labels of the fields the code of the function at `start` reaches
    /// through typed pointers: instruction → `edict_t.enemy`.
    pub(crate) fn field_labels(&self, start: u64, frame: &Frame) -> HashMap<u64, String> {
        let mut out = HashMap::new();
        if frame.uses.is_empty() || self.type_sources().next().is_none() {
            return out;
        }
        let parameters = self.parameter_pointees(start);
        let mut known: HashMap<Origin, Option<(Pointee<'_>, u64)>> = HashMap::new();
        for &(pc, origin, offset) in &frame.uses {
            let Some((p, base)) = self.pointed_at(frame, &parameters, origin, &mut known, 0) else {
                continue;
            };
            let Some(at) = base.checked_add_signed(offset as i64) else {
                continue;
            };
            if let Some(f) = p.debug.field_at(&p.st, at) {
                out.insert(pc, f.label(&p.name));
            }
        }
        out
    }

    /// The structure a value that came from `origin` points at, and where in
    /// it (a global's address points into the global).
    fn pointed_at<'a>(
        &'a self,
        frame: &Frame,
        parameters: &[Option<Pointee<'a>>],
        origin: Origin,
        known: &mut HashMap<Origin, Option<(Pointee<'a>, u64)>>,
        depth: u32,
    ) -> Option<(Pointee<'a>, u64)> {
        if let Some(k) = known.get(&origin) {
            return k.clone();
        }
        let found = match origin {
            Origin::Stack(_) | Origin::Register(_) => {
                let i = frame.argument_index(origin)?;
                parameters.get(i).cloned().flatten().map(|p| (p, 0))
            }
            // A pointer read from a field: what the field's type points at.
            Origin::Loaded(i) if depth < 8 => {
                let (from, offset) = *frame.loads.get(i as usize)?;
                let (p, base) = self.pointed_at(frame, parameters, from, known, depth + 1)?;
                let f = p.debug.field_at(&p.st, base.checked_add_signed(offset as i64)?)?;
                if f.padding || f.delta != 0 {
                    None
                } else {
                    self.pointee(&f.type_name, Some(p.debug)).map(|p| (p, 0))
                }
            }
            Origin::Loaded(_) => None,
            Origin::Global(address) => self.global_pointee(address).map(|p| (p, 0)),
            Origin::Address(address) => self.typed_global(address),
        };
        known.insert(origin, found.clone());
        found
    }
}

/// The name a type is spelled by, without `struct`, qualifiers or pointers.
fn spelled_name(ty: &str) -> String {
    let mut t = ty.trim();
    loop {
        let before = t;
        for word in ["struct ", "class ", "union ", "const ", "volatile "] {
            t = t.strip_prefix(word).unwrap_or(t).trim_start();
        }
        t = t.trim_end_matches(['*', '&', ' ']);
        t = t.strip_suffix(" const").unwrap_or(t).trim_end();
        if t == before {
            break;
        }
    }
    t.to_string()
}

/// The types of a C prototype's parameters (`void f(edict_t *self, int n)`
/// or just `(edict_t *self, int n)`): `["edict_t *", "int"]`.
pub(crate) fn parameter_types(prototype: &str) -> Vec<String> {
    let Some(close) = prototype.rfind(')') else {
        return Vec::new();
    };
    // The parenthesis opening the parameter list, past those inside it.
    let mut depth = 0;
    let mut open = None;
    for (i, c) in prototype[..close].char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' if depth == 0 => {
                open = Some(i);
                break;
            }
            '(' => depth -= 1,
            _ => {}
        }
    }
    let Some(open) = open else { return Vec::new() };
    let list = prototype[open + 1..close].trim();
    if list.is_empty() || list == "void" {
        return Vec::new();
    }
    let mut params = Vec::new();
    let (mut depth, mut from) = (0, 0);
    for (i, c) in list.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                params.push(&list[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    params.push(&list[from..]);
    params
        .into_iter()
        .map(str::trim)
        .filter(|p| *p != "...")
        .map(|p| without_name(p).to_string())
        .collect()
}

/// A parameter's type without its name: `edict_t *self` is `edict_t *`,
/// `int n` is `int`, `unsigned int` stays itself.
fn without_name(param: &str) -> &str {
    let p = param.trim_end();
    let start = p
        .char_indices()
        .rev()
        .take_while(|&(_, c)| c.is_ascii_alphanumeric() || c == '_')
        .last()
        .map_or(p.len(), |(i, _)| i);
    let (ty, name) = p.split_at(start);
    const TYPE_WORDS: &[&str] = &[
        "void", "char", "short", "int", "long", "float", "double", "signed", "unsigned", "const", "volatile", "_Bool",
        "bool",
    ];
    let ty_trimmed = ty.trim_end();
    if name.is_empty() || ty_trimmed.is_empty() || TYPE_WORDS.contains(&name) || ty_trimmed.ends_with("struct") {
        p
    } else {
        ty_trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::{parameter_types, spelled_name};

    #[test]
    fn a_prototypes_parameters_are_their_types() {
        assert_eq!(parameter_types("void SP_monster_soldier(edict_t *self)"), ["edict_t *"]);
        assert_eq!(
            parameter_types("(struct edict_s *self, int damage)"),
            ["struct edict_s *", "int"]
        );
        assert_eq!(
            parameter_types("int __cdecl f(unsigned int, edict_t*)"),
            ["unsigned int", "edict_t*"]
        );
        assert_eq!(parameter_types("void (void)"), Vec::<String>::new());
        assert_eq!(
            parameter_types("void f(void (*think)(edict_t *), int n, ...)"),
            ["void (*think)(edict_t *)", "int"]
        );
        assert_eq!(spelled_name("const struct edict_s *"), "edict_s");
    }
}
