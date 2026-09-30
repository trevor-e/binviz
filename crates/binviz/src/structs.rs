//! Structures matched up across calls, and the arguments callers pass
//! (MIPS). Each function's pointers are followed on their own
//! ([`crate::mipsflow`]); here they are joined up across the program: a
//! pointer passed to a function is the same kind of thing as that function's
//! argument, a pointer stored in a field is what that field points to, and a
//! getter's result is what it returns. Everything joined is one structure,
//! whose layout is every offset any of them reaches, the way a points-to
//! analysis unifies (Steensgaard's). So `f` passing `g->player` to `h` makes
//! `h`'s argument the type of field `player`, and `h`'s own accesses fill
//! that type in for `f`.
//!
//! Unifying through a function that takes any pointer (`memcpy`, `free`, a
//! string routine) would make one structure of everything, so a call joins
//! the caller's pointer to the callee's argument only when the callee treats
//! that argument as a structure: it reaches two or more offsets through it,
//! or one past the first word, or passes it on to a function that does.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

use serde::Serialize;

use crate::binary::Binary;
use crate::cpu::mips::MipsWord;
use crate::mipsflow::{Flow, Origin};
use crate::model::SymbolKind;
use crate::util::Endian;

/// Something a structure is named by: an argument, a result, a global, a local.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Key {
    Arg(u64, u8),
    Ret(u64),
    Global(u64),
    Local(u64, i32),
}

/// One of the things a structure is: `a0 of UpdateActor`, `the global at …`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    /// `argument`, `result`, `global` or `local`.
    pub kind: &'static str,
    /// The function it belongs to (not for a global).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<u64>,
    /// `a0`, `a4` (the first on the stack), `sp+0x18`, or the global's address or name.
    pub what: String,
    pub text: String,
}

/// An offset some function reaches through the structure.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructField {
    pub offset: i32,
    /// Every width it is read or written with, narrowest first.
    pub widths: Vec<u8>,
    /// `r`, `w` or `rw`.
    pub access: String,
    /// The structure the word there points to, when that is one of these.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points_to: Option<String>,
    /// Functions that reach it.
    pub functions: u32,
}

/// One structure, as all its members' code uses it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Structure {
    pub name: String,
    pub members: Vec<Member>,
    pub fields: Vec<StructField>,
    /// Functions whose code reaches it or names it.
    pub functions: u32,
    /// Offsets used with more than one width: a union, or unrelated
    /// structures joined by mistake.
    pub conflicts: u32,
}

/// What callers put in the argument registers before calling a function.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallerArgs {
    /// Direct calls to it that were read.
    pub sites: u32,
    /// For `$a0`–`$a3`: at how many of them it was set up for the call.
    pub set: [u32; 4],
    /// Arguments set up by more than half of the calls: registers, then the stack.
    pub register_args: u32,
    pub stack_args: u32,
    /// The most any one call sets up.
    pub most: u32,
}

/// The program's structures, and what callers pass each function.
pub(crate) struct StructIndex {
    structures: Vec<Structure>,
    /// For each function: the pointers it holds that are listed structures,
    /// as it names them (`a0`, `a0->0x10`, `sp+0x18`), and which.
    used_by: HashMap<u64, Vec<(String, usize)>>,
    callers: HashMap<u64, CallerArgs>,
}

/// Union-find over the program's pointers, with what each class's fields point to.
struct Classes {
    parent: Vec<u32>,
    children: Vec<BTreeMap<i32, u32>>,
}

impl Classes {
    fn fresh(&mut self) -> u32 {
        self.parent.push(self.parent.len() as u32);
        self.children.push(BTreeMap::new());
        self.parent.len() as u32 - 1
    }

    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            let up = self.parent[self.parent[x as usize] as usize];
            self.parent[x as usize] = up;
            x = up;
        }
        x
    }

    /// What the word at `offset` of `x`'s class points to.
    fn deref(&mut self, x: u32, offset: i32) -> u32 {
        let r = self.find(x);
        if let Some(&c) = self.children[r as usize].get(&offset) {
            return c;
        }
        let c = self.fresh();
        self.children[r as usize].insert(offset, c);
        c
    }

    fn union(&mut self, a: u32, b: u32) {
        let mut work = vec![(a, b)];
        while let Some((a, b)) = work.pop() {
            let (mut ra, mut rb) = (self.find(a), self.find(b));
            if ra == rb {
                continue;
            }
            if self.children[ra as usize].len() < self.children[rb as usize].len() {
                std::mem::swap(&mut ra, &mut rb);
            }
            self.parent[rb as usize] = ra;
            for (off, c) in std::mem::take(&mut self.children[rb as usize]) {
                match self.children[ra as usize].get(&off) {
                    Some(&d) => work.push((d, c)),
                    None => {
                        self.children[ra as usize].insert(off, c);
                    }
                }
            }
        }
    }
}

impl Binary {
    /// `$gp` from a PS-X EXE's header.
    fn mips_gp(&self) -> Option<u32> {
        let d = self.data();
        (d.starts_with(b"PS-X EXE") && d.len() >= 0x18)
            .then(|| u32::from_le_bytes([d[0x14], d[0x15], d[0x16], d[0x17]]))
            .filter(|&gp| gp != 0)
    }

    /// The MIPS words of the function containing `address`, and its start.
    pub(crate) fn mips_words(&self, address: u64) -> Option<(u64, Vec<MipsWord>)> {
        let endian = self.mips_endian()?;
        let f = self.symbols.function_containing(address)?;
        if f.size == 0 {
            return None;
        }
        let bytes = self.code_bytes(f.address)?;
        let words = bytes
            .get(..(f.size as usize).min(bytes.len()))?
            .chunks_exact(4)
            .map(|c| {
                let b = [c[0], c[1], c[2], c[3]];
                MipsWord(match endian {
                    Endian::Little => u32::from_le_bytes(b),
                    Endian::Big => u32::from_be_bytes(b),
                })
            })
            .collect();
        Some((f.address, words))
    }

    /// The pointers of the function containing `address` followed ([`crate::mipsflow`]).
    pub(crate) fn mips_flow(&self, address: u64) -> Option<(u64, Flow)> {
        let (start, words) = self.mips_words(address)?;
        let is_address = |a: u64| (0x8000_0000..0x8080_0000).contains(&a) || self.address_to_offset(a).is_some();
        Some((start, crate::mipsflow::scan(&words, start, &is_address, self.mips_gp())))
    }

    pub(crate) fn struct_index(&self) -> &StructIndex {
        self.structs.get_or_init(|| StructIndex::build(self))
    }

    /// The structures matched up across calls, those that more than one
    /// function reaches, the most used first (MIPS).
    pub fn structures(&self) -> &[Structure] {
        &self.struct_index().structures
    }

    /// The structures the function at `address` hands around or reaches,
    /// as other functions' code also uses them: (what it is here, `a0`,
    /// `a0->0x10`, `sp+0x18` or a global, and the structure).
    pub fn structures_of(&self, address: u64) -> Vec<(String, &Structure)> {
        let index = self.struct_index();
        let Some(f) = self.symbols.function_containing(address) else {
            return Vec::new();
        };
        index
            .used_by
            .get(&f.address)
            .map(|list| {
                list.iter()
                    .map(|(what, i)| (what.clone(), &index.structures[*i]))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// What callers pass the function at `address` (MIPS): which argument
    /// registers they set up, and what they store for it on the stack.
    pub fn caller_args(&self, address: u64) -> Option<&CallerArgs> {
        self.struct_index().callers.get(&address)
    }
}

/// The start of the named global holding `a`, and `a`'s offset in it.
fn global_base(bin: &Binary, a: u64) -> (u64, i32) {
    if let Some(r) = bin.symbols().lookup(a)
        && r.offset > 0
        && r.offset < r.size
        && r.offset < 0x1_0000
        && bin.symbols().get(r.index).is_some_and(|s| s.kind == SymbolKind::Data)
    {
        return (r.address, r.offset as i32);
    }
    (a, 0)
}

fn global_text(bin: &Binary, a: u64) -> String {
    match bin.symbols().at(a) {
        Some(s) if s.kind == SymbolKind::Data => format!("{} ({a:#x})", s.display_name()),
        _ => format!("the global at {a:#x}"),
    }
}

fn name_of(bin: &Binary, f: u64) -> String {
    bin.symbols()
        .at(f)
        .map_or_else(|| format!("{f:#x}"), |s| s.display_name().into_owned())
}

/// The arguments a function treats as a structure: it reaches two or more
/// offsets directly through one, or one past the first word. Not a buffer
/// walked a piece at a time (a copy loop unrolled: the same width at 0, 1,
/// 2, 3 through a pointer it moves along).
fn structure_args(flow: &Flow) -> Vec<u8> {
    let mut by_arg: HashMap<u8, BTreeMap<i32, HashSet<u8>>> = HashMap::new();
    for u in flow.fields.iter().filter(|u| u.direct) {
        if let Origin::Arg(k) = flow.nodes[u.node as usize] {
            by_arg
                .entry(k)
                .or_default()
                .entry(u.offset)
                .or_default()
                .insert(u.width);
        }
    }
    let mut out = Vec::new();
    for (k, offsets) in by_arg {
        if offsets.len() < 2 && offsets.keys().all(|&o| o < 4) {
            continue;
        }
        let moved = flow.moved.iter().any(|&n| flow.nodes[n as usize] == Origin::Arg(k));
        let widths: HashSet<u8> = offsets.values().flatten().copied().collect();
        let walked = moved
            && widths.len() == 1
            && offsets
                .keys()
                .enumerate()
                .all(|(i, &o)| o == i as i32 * *widths.iter().next().unwrap() as i32);
        if !walked {
            out.push(k);
        }
    }
    out
}

impl StructIndex {
    fn build(bin: &Binary) -> StructIndex {
        let mut seen = HashSet::new();
        let mut funcs: Vec<(u64, Flow)> = Vec::new();
        let starts: Vec<u64> = bin
            .symbols()
            .functions()
            .filter(|f| f.size > 0 && f.defined)
            .map(|f| f.address)
            .collect();
        for a in starts {
            if seen.insert(a)
                && let Some((start, flow)) = bin.mips_flow(a)
                && start == a
            {
                funcs.push((start, flow));
            }
        }
        let known: HashSet<u64> = funcs.iter().map(|f| f.0).collect();

        // Arguments a function treats as a structure.
        let mut evidence: HashSet<(u64, u8)> = HashSet::new();
        for (start, flow) in &funcs {
            for k in structure_args(flow) {
                evidence.insert((*start, k));
            }
        }
        // …or passes on, unchanged, to a function that does.
        for _ in 0..16 {
            let mut more = Vec::new();
            for (start, flow) in &funcs {
                for c in &flow.calls {
                    let Some(t) = c.target else { continue };
                    let passed = c.args.iter().enumerate().map(|(j, v)| (j as u8, *v));
                    let on_stack = c.stack.iter().map(|&(k, v)| (4 + k as u8, v));
                    for (j, v) in passed.chain(on_stack) {
                        if let Some((n, 0)) = v
                            && let Origin::Arg(k) = flow.nodes[n as usize]
                            && evidence.contains(&(t, j))
                            && !evidence.contains(&(*start, k))
                        {
                            more.push((*start, k));
                        }
                    }
                }
            }
            if more.is_empty() {
                break;
            }
            evidence.extend(more);
        }
        // Functions whose result is a pointer reached from a global (a getter's).
        let rooted = |flow: &Flow, mut n: u32| loop {
            match flow.nodes[n as usize] {
                Origin::Global(_) => return true,
                Origin::Deref(p, _) => n = p,
                _ => return false,
            }
        };
        let getters: HashSet<u64> = funcs
            .iter()
            .filter(|(_, flow)| !flow.returns.is_empty() && flow.returns.iter().all(|&r| rooted(flow, r)))
            .map(|f| f.0)
            .collect();

        let mut cl = Classes {
            parent: Vec::new(),
            children: Vec::new(),
        };
        let mut keys: HashMap<Key, u32> = HashMap::new();
        let mut key = |cl: &mut Classes, k: Key| *keys.entry(k).or_insert_with(|| cl.fresh());
        // (node, offset, width, store, function).
        let mut uses: Vec<(u32, i32, u8, bool, u32)> = Vec::new();
        let mut maps: Vec<Vec<u32>> = Vec::with_capacity(funcs.len());
        for (fi, (start, flow)) in funcs.iter().enumerate() {
            let mut map: Vec<u32> = Vec::with_capacity(flow.nodes.len());
            // Offsets into a named global's middle count from its start.
            let mut shift: Vec<i32> = Vec::with_capacity(flow.nodes.len());
            for o in &flow.nodes {
                let (g, s) = match *o {
                    Origin::Arg(k) => (key(&mut cl, Key::Arg(*start, k)), 0),
                    Origin::Global(a) => {
                        let (base, off) = global_base(bin, a);
                        (key(&mut cl, Key::Global(base)), off)
                    }
                    Origin::Local(off) => (key(&mut cl, Key::Local(*start, off)), 0),
                    Origin::Returned(t) if getters.contains(&t) => (key(&mut cl, Key::Ret(t)), 0),
                    Origin::Returned(_) => (cl.fresh(), 0),
                    Origin::Deref(p, off) => (cl.deref(map[p as usize], off + shift[p as usize]), 0),
                };
                map.push(g);
                shift.push(s);
            }
            for u in &flow.fields {
                let n = u.node as usize;
                uses.push((map[n], u.offset + shift[n], u.width, u.store, fi as u32));
            }
            for &(n, off, m) in &flow.stores {
                let d = cl.deref(map[n as usize], off + shift[n as usize]);
                if shift[m as usize] == 0 {
                    cl.union(d, map[m as usize]);
                }
            }
            if getters.contains(start) {
                let r = key(&mut cl, Key::Ret(*start));
                for &n in &flow.returns {
                    if shift[n as usize] == 0 {
                        cl.union(r, map[n as usize]);
                    }
                }
            }
            for c in &flow.calls {
                let Some(t) = c.target.filter(|t| known.contains(t)) else {
                    continue;
                };
                let passed = c.args.iter().enumerate().map(|(j, v)| (j as u8, *v));
                let on_stack = c.stack.iter().map(|&(k, v)| (4 + k as u8, v));
                for (j, v) in passed.chain(on_stack) {
                    if let Some((n, 0)) = v
                        && shift[n as usize] == 0
                        && evidence.contains(&(t, j))
                    {
                        let a = key(&mut cl, Key::Arg(t, j));
                        cl.union(map[n as usize], a);
                    }
                }
            }
            maps.push(map);
        }

        // Each class's fields, members and the functions involved.
        struct Acc {
            fields: BTreeMap<i32, (Vec<u8>, bool, bool, HashSet<u32>)>,
            members: Vec<Key>,
            functions: HashSet<u64>,
        }
        let mut acc: HashMap<u32, Acc> = HashMap::new();
        let new_acc = || Acc {
            fields: BTreeMap::new(),
            members: Vec::new(),
            functions: HashSet::new(),
        };
        for &(n, off, width, store, fi) in &uses {
            let r = cl.find(n);
            let a = acc.entry(r).or_insert_with(new_acc);
            let f = a
                .fields
                .entry(off)
                .or_insert_with(|| (Vec::new(), false, false, HashSet::new()));
            if !f.0.contains(&width) {
                f.0.push(width);
                f.0.sort_unstable();
            }
            if store {
                f.2 = true;
            } else {
                f.1 = true;
            }
            f.3.insert(fi);
            a.functions.insert(funcs[fi as usize].0);
        }
        let mut members: Vec<(Key, u32)> = keys.iter().map(|(&k, &n)| (k, n)).collect();
        members.sort();
        for (k, n) in members {
            let r = cl.find(n);
            let a = acc.entry(r).or_insert_with(new_acc);
            a.members.push(k);
            match k {
                Key::Arg(f, _) | Key::Ret(f) | Key::Local(f, _) => {
                    a.functions.insert(f);
                }
                Key::Global(_) => {}
            }
        }
        // Listed: reached by two or more functions, at two or more offsets
        // (or pointing somewhere), or two of the program's things joined.
        let mut roots: Vec<u32> = acc
            .iter()
            .filter(|(_, a)| a.functions.len() >= 2 && (a.fields.len() >= 2 || a.members.len() >= 2))
            .map(|(&r, _)| r)
            .collect();
        // And a pointer (a global, say) to one of those.
        let base: HashSet<u32> = roots.iter().copied().collect();
        let pointing: Vec<u32> = acc
            .iter()
            .filter(|(r, a)| a.functions.len() >= 2 && !base.contains(r))
            .filter(|(r, _)| {
                cl.children[**r as usize].values().any(|&c| {
                    let mut x = c;
                    while cl.parent[x as usize] != x {
                        x = cl.parent[x as usize];
                    }
                    base.contains(&x)
                })
            })
            .map(|(&r, _)| r)
            .collect();
        roots.extend(pointing);
        roots.sort_by_key(|r| {
            let a = &acc[r];
            (std::cmp::Reverse(a.functions.len()), a.members.first().copied())
        });
        let listed: HashMap<u32, usize> = roots.iter().enumerate().map(|(i, &r)| (r, i)).collect();
        // Named after a named global if one is among them, else an argument,
        // a result, a local, an unnamed global.
        let named_global = |k: &Key| match *k {
            Key::Global(a) => bin
                .symbols()
                .at(a)
                .filter(|s| s.kind == SymbolKind::Data && !crate::names::is_made_up(s.name()))
                .map(|s| format!("{}_t", s.display_name())),
            _ => None,
        };
        let names: Vec<String> = roots
            .iter()
            .map(|r| {
                let m = &acc[r].members;
                if let Some(n) = m.iter().find_map(named_global) {
                    return n;
                }
                let rank = |k: &&Key| match k {
                    Key::Arg(..) => 0,
                    Key::Ret(_) => 1,
                    Key::Local(..) => 2,
                    Key::Global(_) => 3,
                };
                match m.iter().min_by_key(rank) {
                    Some(Key::Global(a)) => format!("s_{a:x}"),
                    Some(Key::Arg(f, k)) => format!("s_{f:x}_a{k}"),
                    Some(Key::Ret(f)) => format!("s_{f:x}_ret"),
                    Some(Key::Local(f, off)) => format!("s_{f:x}_sp{off:x}"),
                    None => String::new(),
                }
            })
            .collect();
        // A structure named by nothing is named after a field that points to it.
        let mut names = names;
        for _ in 0..4 {
            for (i, &r) in roots.iter().enumerate() {
                let kids: Vec<(i32, u32)> = cl.children[r as usize].iter().map(|(&o, &c)| (o, c)).collect();
                for (off, c) in kids {
                    let c = cl.find(c);
                    if let Some(&j) = listed.get(&c)
                        && names[j].is_empty()
                        && !names[i].is_empty()
                    {
                        names[j] = format!("{}_f{off:x}", names[i]);
                    }
                }
            }
        }
        for (i, n) in names.iter_mut().enumerate() {
            if n.is_empty() {
                *n = format!("s_anon{i}");
            }
        }
        let mut structures = Vec::new();
        for (i, &r) in roots.iter().enumerate() {
            let a = &acc[&r];
            let mut fields: Vec<StructField> = a
                .fields
                .iter()
                .map(|(&offset, (widths, read, written, fns))| StructField {
                    offset,
                    widths: widths.clone(),
                    access: match (read, written) {
                        (true, true) => "rw",
                        (false, true) => "w",
                        _ => "r",
                    }
                    .into(),
                    points_to: None,
                    functions: fns.len() as u32,
                })
                .collect();
            for (&off, &c) in &cl.children[r as usize] {
                let c = {
                    let mut x = c;
                    while cl.parent[x as usize] != x {
                        x = cl.parent[x as usize];
                    }
                    x
                };
                let Some(&j) = listed.get(&c) else { continue };
                match fields.iter_mut().find(|f| f.offset == off) {
                    Some(f) => f.points_to = Some(names[j].clone()),
                    None => fields.push(StructField {
                        offset: off,
                        widths: vec![4],
                        access: "r".into(),
                        points_to: Some(names[j].clone()),
                        functions: 0,
                    }),
                }
            }
            fields.sort_by_key(|f| f.offset);
            let members = a
                .members
                .iter()
                .map(|&k| match k {
                    Key::Arg(f, n) => Member {
                        kind: "argument",
                        function: Some(f),
                        what: format!("a{n}"),
                        text: format!("a{n} of {}", name_of(bin, f)),
                    },
                    Key::Ret(f) => Member {
                        kind: "result",
                        function: Some(f),
                        what: "result".into(),
                        text: format!("what {} returns", name_of(bin, f)),
                    },
                    Key::Global(g) => Member {
                        kind: "global",
                        function: None,
                        what: format!("{g:#x}"),
                        text: global_text(bin, g),
                    },
                    Key::Local(f, off) => Member {
                        kind: "local",
                        function: Some(f),
                        what: format!("sp+{off:#x}"),
                        text: format!("the local at sp+{off:#x} of {}", name_of(bin, f)),
                    },
                })
                .collect();
            let conflicts = fields.iter().filter(|f| f.widths.len() > 1).count() as u32;
            structures.push(Structure {
                name: names[i].clone(),
                members,
                fields,
                functions: a.functions.len() as u32,
                conflicts,
            });
        }

        // What callers set up for each function.
        let mut callers: HashMap<u64, CallerArgs> = HashMap::new();
        let mut stack_counts: HashMap<u64, Vec<u32>> = HashMap::new();
        for (_, flow) in &funcs {
            for c in &flow.calls {
                let Some(t) = c.target.filter(|t| known.contains(t)) else {
                    continue;
                };
                let e = callers.entry(t).or_default();
                e.sites += 1;
                for k in 0..4 {
                    e.set[k] += c.set[k] as u32;
                }
                let regs = (0..4).rev().find(|&k| c.set[k]).map_or(0, |k| k as u32 + 1);
                let on_stack = c.stack.iter().map(|s| s.0 + 1).max().unwrap_or(0);
                e.most = e.most.max(if on_stack > 0 { 4 + on_stack } else { regs });
                let s = stack_counts.entry(t).or_default();
                for &(k, _) in &c.stack {
                    if s.len() <= k as usize {
                        s.resize(k as usize + 1, 0);
                    }
                    s[k as usize] += 1;
                }
            }
        }
        for (t, e) in callers.iter_mut() {
            let most = |n: u32| n * 2 > e.sites;
            e.register_args = (0..4).rev().find(|&k| most(e.set[k])).map_or(0, |k| k as u32 + 1);
            if let Some(s) = stack_counts.get(t) {
                e.stack_args = (0..s.len()).rev().find(|&k| most(s[k])).map_or(0, |k| k as u32 + 1);
                if e.stack_args > 0 {
                    e.register_args = 4;
                }
            }
        }
        // What each function holds of them, as it would name it.
        let mut used_by: HashMap<u64, Vec<(String, usize)>> = HashMap::new();
        for ((start, flow), map) in funcs.iter().zip(&maps) {
            let mut names: Vec<String> = Vec::with_capacity(flow.nodes.len());
            let mut out: Vec<(String, usize)> = Vec::new();
            for (n, o) in flow.nodes.iter().enumerate() {
                let text = match *o {
                    Origin::Arg(k) => format!("a{k}"),
                    Origin::Global(a) => global_text(bin, a),
                    Origin::Local(off) => format!("sp+{off:#x}"),
                    Origin::Returned(t) => format!("what {} returns", name_of(bin, t)),
                    Origin::Deref(p, 0) if matches!(flow.nodes[p as usize], Origin::Global(_)) => {
                        format!("the pointer in {}", names[p as usize])
                    }
                    Origin::Deref(p, off) => format!("{}->{off:#x}", names[p as usize]),
                };
                let mut r = map[n];
                while cl.parent[r as usize] != r {
                    r = cl.parent[r as usize];
                }
                if let Some(&i) = listed.get(&r)
                    && out.iter().all(|o| o.1 != i)
                {
                    out.push((text.clone(), i));
                }
                names.push(text);
            }
            if !out.is_empty() {
                used_by.insert(*start, out);
            }
        }
        StructIndex {
            structures,
            used_by,
            callers,
        }
    }
}

fn width_type(w: u8) -> &'static str {
    match w {
        1 => "u8",
        2 => "u16",
        8 => "u64",
        _ => "u32",
    }
}

impl Structure {
    /// A C declaration: each field at its offset, the gaps as padding. A
    /// field also reached with a wider access that runs into the next one
    /// (a structure copied a word at a time) is declared at its narrowest.
    pub fn to_c(&self) -> String {
        let mut out = format!("struct {} {{\n", self.name);
        let fields: Vec<&StructField> = self.fields.iter().filter(|f| f.offset >= 0).collect();
        let mut at = 0i32;
        let mut skip_to = 0usize;
        for (i, f) in fields.iter().enumerate() {
            if i < skip_to {
                continue;
            }
            // Three or more of the same field the same distance apart: an array
            // of structures (where each element starts isn't known, only the stride).
            let same = |g: &&&StructField| g.widths == f.widths && g.points_to.is_none() && f.points_to.is_none();
            if let Some(g) = fields.get(i + 1).filter(|g| same(g)) {
                let stride = g.offset - f.offset;
                let run = fields[i..]
                    .windows(2)
                    .take_while(|w| same(&&w[1]) && w[1].offset - w[0].offset == stride)
                    .count()
                    + 1;
                let widest = *f.widths.last().unwrap_or(&4) as i32;
                if run >= 3 && stride > widest && f.offset >= at {
                    if f.offset > at {
                        let _ = writeln!(out, "    u8 pad_{at:x}[{:#x}];", f.offset - at);
                    }
                    let end = f.offset + stride * (run as i32 - 1) + widest;
                    let _ = writeln!(
                        out,
                        "    /* {:#06x} */ u8 arr_{:x}[{:#x}]; /* an array: {} at {:#x} + {:#x} * i, {run} of them (elements {:#x} bytes long) */",
                        f.offset,
                        f.offset,
                        end - f.offset,
                        width_type(widest as u8),
                        f.offset,
                        stride,
                        stride
                    );
                    at = end;
                    skip_to = i + run;
                    continue;
                }
            }
            let next = fields.get(i + 1).map_or(i32::MAX, |g| g.offset);
            let widest = *f.widths.last().unwrap_or(&4);
            let width = if f.offset + widest as i32 > next {
                f.widths[0]
            } else {
                widest
            };
            if f.offset < at {
                let _ = writeln!(out, "    /* {:#x}: also {} */", f.offset, width_type(width));
                continue;
            }
            if f.offset > at {
                let _ = writeln!(out, "    u8 pad_{at:x}[{:#x}];", f.offset - at);
            }
            let ty = match &f.points_to {
                Some(s) => format!("struct {s} *"),
                None => format!("{} ", width_type(width)),
            };
            let others: Vec<&str> = f
                .widths
                .iter()
                .filter(|&&w| w != width)
                .map(|&w| width_type(w))
                .collect();
            let also = if others.is_empty() {
                String::new()
            } else if width < widest {
                format!(
                    " /* also read or written as {} with what follows: a copy of the whole */",
                    others.join(", ")
                )
            } else {
                format!(" /* also used as {} */", others.join(", "))
            };
            let _ = writeln!(out, "    /* {:#06x} */ {ty}f_{:x};{also}", f.offset, f.offset);
            at = f.offset + width as i32;
        }
        out.push_str("};\n");
        out
    }

    /// The members and the C declaration, for a terminal or a prompt.
    pub fn describe(&self, members: usize) -> String {
        let mut out = String::new();
        let shown: Vec<&str> = self.members.iter().take(members).map(|m| m.text.as_str()).collect();
        let _ = write!(
            out,
            "{}: reached by {} functions; is {}{}",
            self.name,
            self.functions,
            shown.join(", "),
            if self.members.len() > members {
                format!(" and {} more", self.members.len() - members)
            } else {
                String::new()
            }
        );
        if self.conflicts > 0 {
            let _ = write!(
                out,
                "; {} offset{} used with more than one width (a union, a structure copied a word at a time, or structures joined by mistake)",
                self.conflicts,
                if self.conflicts == 1 { "" } else { "s" }
            );
        }
        out.push('\n');
        out.push_str(&self.to_c());
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::binary::Binary;

    /// A PS-X EXE holding `words` at 0x80010000, its entry there.
    fn exe(words: &[u32]) -> Binary {
        let mut data = vec![0u8; 0x800];
        data[..8].copy_from_slice(b"PS-X EXE");
        for (at, v) in [(0x10, 0x8001_0000u32), (0x14, 0x8001_8000), (0x18, 0x8001_0000)] {
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        let code: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        data[0x1C..0x20].copy_from_slice(&(code.len() as u32).to_le_bytes());
        data.extend(code);
        Binary::parse(data).unwrap()
    }

    #[test]
    fn structures_across_calls() {
        let mut w = vec![0u32; 0x44];
        // f: passes g the pointer kept in the global at 0x80010100, then calls h.
        w[..11].copy_from_slice(&[
            0x27BD_FFE8, // addiu $sp, $sp, -0x18
            0xAFBF_0014, // sw $ra, 0x14($sp)
            0x3C02_8001, // lui $v0, 0x8001
            0x8C44_0100, // lw $a0, 0x100($v0)
            0x0C00_4010, // jal g
            0x2405_0003, // li $a1, 3
            0x0C00_4014, // jal h
            0x0000_0000,
            0x8FBF_0014, // lw $ra, 0x14($sp)
            0x03E0_0008, // jr $ra
            0x27BD_0018, // addiu $sp, $sp, 0x18
        ]);
        // g (0x80010040): reads a0->4 and a0->8, writes a1 to a0->0xc.
        w[16..20].copy_from_slice(&[0x8C82_0004, 0x9483_0008, 0x03E0_0008, 0xAC85_000C]);
        // h (0x80010050): passes g the address of its local at sp+0x18.
        w[20..29].copy_from_slice(&[
            0x27BD_FFD8, // addiu $sp, $sp, -0x28
            0xAFBF_0024, // sw $ra, 0x24($sp)
            0x27A4_0018, // addiu $a0, $sp, 0x18
            0xAFA0_0018, // sw $zero, 0x18($sp)
            0x0C00_4010, // jal g
            0x2405_0001, // li $a1, 1
            0x8FBF_0024, // lw $ra, 0x24($sp)
            0x03E0_0008, // jr $ra
            0x27BD_0028, // addiu $sp, $sp, 0x28
        ]);
        w[0x40] = 0x8001_0200;
        let b = exe(&w);

        let list = b.structures();
        assert_eq!(list.len(), 1, "{list:#?}");
        let s = &list[0];
        assert_eq!(s.name, "s_80010040_a0");
        let members: Vec<&str> = s.members.iter().map(|m| m.text.as_str()).collect();
        assert!(members.contains(&"a0 of sub_80010040"), "{members:?}");
        assert!(members.contains(&"the local at sp+0x18 of sub_80010050"), "{members:?}");
        let fields: Vec<(i32, &[u8], &str)> = s
            .fields
            .iter()
            .map(|f| (f.offset, f.widths.as_slice(), f.access.as_str()))
            .collect();
        assert_eq!(fields, [(4, &[4u8][..], "r"), (8, &[2][..], "r"), (0xc, &[4][..], "w")]);
        let c = s.to_c();
        assert!(c.contains("u8 pad_0[0x4];") && c.contains("u16 f_8;"), "{c}");

        // f sees it as what the global points to.
        let of_f = b.structures_of(0x8001_0000);
        assert_eq!(of_f.len(), 1);
        assert_eq!(of_f[0].0, "the pointer in the global at 0x80010100");

        // Both callers set up $a0 and $a1.
        let callers = b.caller_args(0x8001_0040).unwrap();
        assert_eq!(
            (callers.sites, callers.set, callers.register_args),
            (2, [2, 2, 0, 0], 2)
        );

        // h's local, its address passed to g, written directly at its start.
        let sig = b.function_signature(0x8001_0050).unwrap();
        let slot = sig.stack.iter().find(|s| s.offset == 0x18).unwrap();
        assert!(slot.address_taken);
        assert_eq!(slot.passed_to, ["sub_80010040 as a0"]);
        assert_eq!(slot.fields, [0]);
        assert_eq!(slot.size, 0xc, "up to the saved $ra at 0x24");
    }

    #[test]
    fn memory_routines_do_not_join_structures() {
        let mut w = vec![0u32; 0x40];
        // f passes g the address of one local, then h the address of another.
        w[..11].copy_from_slice(&[
            0x27BD_FFD0, // addiu $sp, $sp, -0x30
            0xAFBF_002C, // sw $ra, 0x2c($sp)
            0x27A4_0010, // addiu $a0, $sp, 0x10
            0x0C00_4010, // jal copy
            0x2406_0004, // li $a2, 4
            0x27A4_0020, // addiu $a0, $sp, 0x20
            0x0C00_4010, // jal copy
            0x2406_0004, // li $a2, 4
            0x8FBF_002C,
            0x03E0_0008,
            0x27BD_0030,
        ]);
        // copy (0x80010040): four bytes unrolled, then moves both pointers along.
        w[16..28].copy_from_slice(&[
            0x90A1_0000, // lbu $at, 0($a1)
            0xA081_0000, // sb $at, 0($a0)
            0x90A1_0001,
            0xA081_0001,
            0x90A1_0002,
            0xA081_0002,
            0x90A1_0003,
            0xA081_0003,
            0x2484_0004, // addiu $a0, $a0, 4
            0x24A5_0004, // addiu $a1, $a1, 4
            0x03E0_0008,
            0x0000_0000,
        ]);
        let b = exe(&w);
        assert!(b.structures().is_empty(), "{:#?}", b.structures());
    }
}
