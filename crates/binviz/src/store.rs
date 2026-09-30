//! A store of matched functions that outlives a project: your own C for each
//! function that compiled back to the original bytes, kept with what built
//! it, so that the next project which contains the same code (the same SDK
//! library, the same middleware, the same engine routine) starts from a
//! match instead of from nothing.
//!
//! A function's *key* is a hash of what its code does, with everything that
//! depends on where it was linked left out: the addresses of data (a
//! `lui`/`addiu` pair on MIPS, an absolute address elsewhere) and branches
//! written relative to the function. A call is named by a hash of the
//! callee's own code, not by its address, so a wrapper of one function is not
//! taken for a wrapper of another. The same routine linked into two games at
//! two addresses has one key. Constants, registers and the shape of the code
//! stay in it, so a different routine does not.
//! A hit is a candidate, never a verdict: the C still has to compile to the
//! bytes here, which `match_function` checks.
//!
//! One JSON file per entry, `<key>.<build>.json`, in a directory of the
//! user's own (`BINVIZ_STORE`, else `~/.local/share/binviz/store`). It holds
//! only what the user wrote and matched, and hashes: none of the game's bytes.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::binary::Binary;
use crate::model::{DecompState, Instruction, RegionKind};

/// Functions of fewer instructions than this are not kept or looked up: a
/// dozen unrelated functions are `jr $ra; nop`.
pub const MIN_INSTRUCTIONS: usize = 6;
/// At most this many instructions of a function are read.
const MAX_INSTRUCTIONS: usize = 4000;

/// A matched function and how it was built.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub key: String,
    pub instructions: u32,
    /// The function's name in its project, as its C calls it.
    pub name: String,
    /// The project it was matched in (the binary's label).
    pub project: String,
    pub compiler: String,
    #[serde(default)]
    pub flags: String,
    #[serde(default)]
    pub sdk: String,
    /// The source file it was in there.
    #[serde(default)]
    pub source: String,
    /// The function's C, as written.
    pub c: String,
    /// When it was recorded: seconds since 1970.
    #[serde(default)]
    pub recorded: u64,
}

impl Entry {
    /// What built it, in a line.
    pub fn build(&self) -> String {
        let mut out = self.compiler.clone();
        for part in [&self.flags, &self.sdk] {
            if !part.is_empty() {
                out.push_str(", ");
                out.push_str(part);
            }
        }
        out
    }
}

/// A directory of entries.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

/// Every entry, by key.
pub type Index = HashMap<String, Vec<Entry>>;

impl Store {
    pub fn at(dir: impl Into<PathBuf>) -> Store {
        Store { dir: dir.into() }
    }

    /// `BINVIZ_STORE`, else `~/.local/share/binviz/store`.
    pub fn default_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("BINVIZ_STORE").filter(|d| !d.is_empty()) {
            return Some(PathBuf::from(dir));
        }
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .filter(|h| !h.is_empty())?;
        Some(Path::new(&home).join(".local/share/binviz/store"))
    }

    /// The user's store, if there is a place to keep one.
    pub fn open_default() -> Option<Store> {
        Store::default_dir().map(Store::at)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Keeps `entry`, replacing the one recorded for the same function and build.
    pub fn add(&self, entry: &Entry) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let build = fnv(&[&entry.compiler, &entry.flags, &entry.sdk, &entry.project, &entry.name]);
        let file = self.dir.join(format!("{}.{build:08x}.json", entry.key));
        let text = serde_json::to_string_pretty(entry).map_err(std::io::Error::other)?;
        // Written whole and then moved, so another session never reads half of it.
        let tmp = file.with_extension("tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &file)
    }

    /// The entries for one key.
    pub fn find(&self, key: &str) -> Vec<Entry> {
        let prefix = format!("{key}.");
        let Ok(dir) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out: Vec<Entry> = dir
            .filter_map(Result::ok)
            .filter(|f| {
                let name = f.file_name();
                let name = name.to_string_lossy();
                name.starts_with(&prefix) && name.ends_with(".json")
            })
            .filter_map(|f| read(&f.path()))
            .collect();
        out.sort_by(|a, b| b.recorded.cmp(&a.recorded).then(a.project.cmp(&b.project)));
        out
    }

    /// Every entry, by key: for looking up a whole binary at once.
    pub fn index(&self) -> Index {
        let mut index = Index::new();
        let Ok(dir) = std::fs::read_dir(&self.dir) else {
            return index;
        };
        for file in dir.filter_map(Result::ok) {
            let path = file.path();
            if path.extension().is_some_and(|e| e == "json")
                && let Some(entry) = read(&path)
            {
                index.entry(entry.key.clone()).or_default().push(entry);
            }
        }
        for list in index.values_mut() {
            list.sort_by(|a, b| b.recorded.cmp(&a.recorded).then(a.project.cmp(&b.project)));
        }
        index
    }
}

fn read(path: &Path) -> Option<Entry> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// FNV-1a over the parts, each ended by a byte no part contains.
fn fnv(parts: &[&str]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for &b in part.as_bytes().iter().chain(&[0x1f]) {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// Mnemonics whose first register is read, not written.
const NO_DESTINATION: &[&str] = &[
    "sw", "sb", "sh", "swl", "swr", "sc", "swc1", "swc2", "lwc1", "lwc2", "beq", "bne", "beqz", "bnez", "blez", "bgtz",
    "bltz", "bgez", "bltzal", "bgezal", "b", "bal", "j", "jr", "jal", "mtc0", "mtc2", "ctc0", "ctc2", "mthi", "mtlo",
    "nop", "break", "syscall", "sync", "cache", "tge", "tgeu", "tlt", "tltu", "teq", "tne",
];

/// The upper half of an address in the PlayStation's RAM (cached or not): a
/// `lui` of one is the start of an address. The I/O ports' `0x1f80` is the
/// same in every game and stays as it is.
fn address_high(v: i128) -> bool {
    (0x8000..=0x807f).contains(&v) || (0xa000..=0xa07f).contains(&v)
}

/// One instruction with what depends on where it was linked replaced: a
/// call or jump out of the function by `T` and a hash of what it calls (`callee`), a branch inside it by its offset
/// from the function's start, an address by `A`, and (on MIPS) the halves of
/// an address built by `lui` and `addiu` or added to a load's offset by `HI`
/// and `LO`. `tainted` holds the registers that hold the high half of an address.
fn normalize(
    ins: &Instruction,
    start: u64,
    end: u64,
    mapped: &dyn Fn(u64) -> bool,
    callee: &dyn Fn(u64) -> String,
    tainted: &mut HashSet<String>,
) -> String {
    let operands = ins.operands.split('<').next().unwrap_or("").trim();
    let mnemonic = ins.mnemonic.as_str();
    let mut words: Vec<(bool, String)> = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, words: &mut Vec<(bool, String)>| {
        if !word.is_empty() {
            words.push((true, std::mem::take(word)));
        }
    };
    let chars: Vec<char> = operands.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let negative = c == '-' && word.is_empty() && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit());
        if c.is_alphanumeric() || matches!(c, '$' | '_' | '.') || negative {
            word.push(c);
        } else {
            flush(&mut word, &mut words);
            words.push((false, c.to_string()));
        }
    }
    flush(&mut word, &mut words);
    let registers: Vec<&str> = words
        .iter()
        .filter(|(w, t)| *w && t.starts_with('$'))
        .map(|(_, t)| t.as_str())
        .collect();
    let writes = !NO_DESTINATION.contains(&mnemonic) && !registers.is_empty();
    let destination = writes.then(|| registers[0].to_string());
    let sources = registers.iter().skip(usize::from(writes));
    let from_address = sources.clone().any(|r| tainted.contains(*r));
    let is_lui = mnemonic == "lui";
    let mut high = false;
    let mut out = format!("{mnemonic} ");
    for (is_word, text) in &words {
        if !is_word {
            out.push_str(text);
            continue;
        }
        let number = text.strip_prefix('-').map_or((false, text.as_str()), |t| (true, t));
        let value = match number.1.strip_prefix("0x") {
            Some(hex) => i128::from_str_radix(hex, 16).ok(),
            None => number.1.parse::<i128>().ok(),
        }
        .map(|v| if number.0 { -v } else { v });
        let Some(v) = value else {
            out.push_str(text);
            continue;
        };
        if v >= 0 && ins.target == Some(v as u64) {
            let t = v as u64;
            if t >= start && t < end {
                out.push_str(&format!("@{}", t - start));
            } else {
                out.push_str(&callee(t));
            }
        } else if is_lui && address_high(v) {
            high = true;
            out.push_str("HI");
        } else if from_address {
            out.push_str("LO");
        } else if v >= 0x1_0000 && mapped(v as u64) {
            out.push('A');
        } else {
            out.push_str(text);
        }
    }
    // What holds the high half of an address, from here on.
    if let Some(d) = destination {
        let copies = mnemonic == "move" && from_address;
        if high || copies {
            tainted.insert(d);
        } else {
            tainted.remove(&d);
        }
    }
    out
}

/// A function's key, and how many instructions it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionKey {
    pub key: String,
    pub instructions: usize,
}

/// The hash of a function's instructions, with what depends on where it was linked left out.
fn hash_of(
    arch: &str,
    start: u64,
    end: u64,
    code: &[Instruction],
    mapped: &dyn Fn(u64) -> bool,
    callee: &dyn Fn(u64) -> String,
) -> u64 {
    let mut tainted = HashSet::new();
    let mut lines: Vec<String> = Vec::with_capacity(code.len() + 1);
    lines.push(arch.to_string());
    for ins in code {
        lines.push(normalize(ins, start, end, mapped, callee, &mut tainted));
    }
    let parts: Vec<&str> = lines.iter().map(String::as_str).collect();
    fnv(&parts)
}

/// The hashes of functions already worked out, by start: a callee is read once.
type Seen = RefCell<HashMap<u64, Option<u64>>>;

/// A function of this binary the store has a match for.
#[derive(Debug, Clone)]
pub struct Hit {
    pub address: u64,
    pub name: String,
    pub instructions: usize,
    pub entries: Vec<Entry>,
}

/// What recording a project's matches found.
#[derive(Debug, Default)]
pub struct Recording {
    pub entries: Vec<Entry>,
    /// Matched functions not recorded: (address, name, why).
    pub skipped: Vec<(u64, String, String)>,
}

impl Binary {
    /// The key of the function at `address` (its start), or None for
    /// something that is not code or has under [`MIN_INSTRUCTIONS`].
    pub fn function_key(&self, address: u64) -> Option<FunctionKey> {
        self.function_key_seen(address, &Seen::default())
    }

    /// The hash of the code of the function starting at `address`, its calls
    /// left as `T` (so that it does not look into its callees' callees).
    fn flat_hash(&self, address: u64, seen: &Seen) -> Option<u64> {
        if let Some(h) = seen.borrow().get(&address) {
            return *h;
        }
        let h = (|| {
            let f = self.symbols().function_containing(address)?;
            if f.address != address {
                return None;
            }
            let (lo, hi) = (f.address, f.address + f.size.max(1));
            let dis = self.disassemble_function(lo, MAX_INSTRUCTIONS);
            if dis.instructions.is_empty() {
                return None;
            }
            let mapped = |a: u64| self.section_at(a).is_some();
            Some(hash_of(
                &self.summary().arch,
                lo,
                hi,
                &dis.instructions,
                &mapped,
                &|_| "T".into(),
            ))
        })();
        seen.borrow_mut().insert(address, h);
        h
    }

    fn function_key_seen(&self, address: u64, seen: &Seen) -> Option<FunctionKey> {
        let f = self.symbols().function_containing(address)?;
        if f.address != address {
            return None;
        }
        let (lo, hi) = (f.address, f.address + f.size.max(1));
        let dis = self.disassemble_function(lo, MAX_INSTRUCTIONS);
        if dis.instructions.len() < MIN_INSTRUCTIONS {
            return None;
        }
        let mapped = |a: u64| self.section_at(a).is_some();
        let callee = |t: u64| {
            self.flat_hash(t, seen)
                .map_or_else(|| "T".to_string(), |h| format!("T{h:x}"))
        };
        let h = hash_of(&self.summary().arch, lo, hi, &dis.instructions, &mapped, &callee);
        Some(FunctionKey {
            key: format!("{h:016x}-{:x}", dis.instructions.len()),
            instructions: dis.instructions.len(),
        })
    }

    /// The functions here that no match is recorded for, and the store holds
    /// a match for: candidates to try. Functions already matched or marked as
    /// library code are left out.
    pub fn store_hits(&self, index: &Index) -> Vec<Hit> {
        let mut out = Vec::new();
        if index.is_empty() {
            return out;
        }
        let seen = Seen::default();
        for sec in self.sections.iter().filter(|s| s.loaded && s.kind == RegionKind::Code) {
            for (start, _) in self.symbols.own_functions_in(sec.address, sec.address + sec.size) {
                if self
                    .decomp_at(start)
                    .is_some_and(|d| matches!(d.state, DecompState::Matched | DecompState::Library))
                {
                    continue;
                }
                let Some(k) = self.function_key_seen(start, &seen) else {
                    continue;
                };
                if let Some(entries) = index.get(&k.key) {
                    out.push(Hit {
                        address: start,
                        name: self
                            .symbols()
                            .at(start)
                            .map_or_else(|| format!("{start:#x}"), |s| s.display_name().into_owned()),
                        instructions: k.instructions,
                        entries: entries.clone(),
                    });
                }
            }
        }
        out.sort_by_key(|h| h.address);
        out
    }

    /// The entries to keep for this project's matched functions: each one
    /// with its C read out of the source file its note names (under `root`),
    /// and what built it. A matched function with no compiler on record is
    /// skipped, since its match says nothing about another project's build.
    pub fn store_entries(&self, root: &Path, project: &str, now: u64) -> Recording {
        let mut out = Recording::default();
        let mut files: HashMap<PathBuf, Option<String>> = HashMap::new();
        let seen = Seen::default();
        for a in self.annotations() {
            let Some(d) = a.decomp.as_ref().filter(|d| d.state == DecompState::Matched) else {
                continue;
            };
            let name = if !a.name.is_empty() {
                a.name.clone()
            } else {
                self.symbols()
                    .at(a.address)
                    .map_or_else(|| format!("{:#x}", a.address), |s| s.display_name().into_owned())
            };
            let mut skip = |why: &str| out.skipped.push((a.address, name.clone(), why.to_string()));
            if d.compiler.is_empty() {
                skip("no compiler recorded (mark it again with compiler, flags, sdk)");
                continue;
            }
            if d.source.is_empty() {
                skip("no source file recorded");
                continue;
            }
            let path = if Path::new(&d.source).is_absolute() {
                PathBuf::from(&d.source)
            } else {
                root.join(&d.source)
            };
            let text = files
                .entry(path.clone())
                .or_insert_with(|| std::fs::read_to_string(&path).ok());
            let Some(text) = text else {
                skip(&format!("{} not found", path.display()));
                continue;
            };
            let Some(c) = c_function_text(text, &name) else {
                skip(&format!("no definition of {name} in {}", d.source));
                continue;
            };
            let Some(k) = self.function_key_seen(a.address, &seen) else {
                skip(&format!("under {MIN_INSTRUCTIONS} instructions, or not a function"));
                continue;
            };
            out.entries.push(Entry {
                key: k.key,
                instructions: k.instructions as u32,
                name,
                project: project.to_string(),
                compiler: d.compiler.clone(),
                flags: d.flags.clone(),
                sdk: d.sdk.clone(),
                source: d.source.clone(),
                c,
                recorded: now,
            });
        }
        out
    }
}

/// The definition of function `name` in C source `text`: from the line its
/// return type is on through its closing brace. None if the text has no
/// definition of it (a call or a prototype is not one).
pub fn c_function_text(text: &str, name: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let word = |i: usize| bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_';
    let mut from = 0;
    while let Some(found) = text[from..].find(name) {
        let at = from + found;
        from = at + name.len();
        let end = at + name.len();
        if (at > 0 && word(at - 1)) || (end < bytes.len() && word(end)) {
            continue;
        }
        // `name (` … `)` then `{`.
        let mut i = end;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) != Some(&b'(') {
            continue;
        }
        let Some(close) = matching(text, i, b'(', b')') else {
            continue;
        };
        let mut j = close + 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if bytes.get(j) != Some(&b'{') {
            continue;
        }
        let Some(body_end) = matching(text, j, b'{', b'}') else {
            continue;
        };
        // From the start of the line the name is on, and the line above when
        // that holds the return type alone (`void` newline `name(...)`).
        let mut line_start = text[..at].rfind('\n').map_or(0, |n| n + 1);
        if let Some(prev_end) = line_start.checked_sub(1) {
            let prev_start = text[..prev_end].rfind('\n').map_or(0, |n| n + 1);
            let prev = text[prev_start..prev_end].trim();
            let ends_a_declaration = prev.is_empty()
                || prev.ends_with([';', '}', '{'])
                || prev.ends_with("*/")
                || prev.starts_with('#')
                || prev.starts_with("//");
            if !ends_a_declaration {
                line_start = prev_start;
            }
        }
        return Some(text[line_start..=body_end].trim_end().to_string());
    }
    None
}

/// The index of the bracket closing the one at `open`, skipping strings,
/// character literals and comments.
fn matching(text: &str, open: usize, up: u8, down: u8) -> Option<usize> {
    let b = text.as_bytes();
    let (mut depth, mut i) = (0i32, open);
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                let quote = b[i];
                i += 1;
                while i < b.len() && b[i] != quote {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            c if c == up => depth += 1,
            c if c == down => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FlowKind;

    fn ins(address: u64, mnemonic: &str, operands: &str, target: Option<u64>) -> Instruction {
        Instruction {
            address,
            offset: None,
            len: 4,
            bytes: String::new(),
            mnemonic: mnemonic.into(),
            operands: operands.into(),
            flow: FlowKind::Normal,
            target,
            target_symbol: None,
            source: None,
        }
    }

    /// The same routine linked into two games: another address, its data and
    /// its callee elsewhere.
    fn routine(base: u64, data_hi: u32, data_lo: u32, callee: u64, count: u32) -> (u64, u64, Vec<Instruction>) {
        let code = vec![
            ins(base, "addiu", "$sp, $sp, -0x18", None),
            ins(base + 4, "sw", "$ra, 0x10($sp)", None),
            ins(base + 8, "lui", &format!("$v0, {data_hi:#x}"), None),
            ins(base + 12, "lw", &format!("$a0, {data_lo:#x}($v0)"), None),
            ins(base + 16, "beqz", &format!("$a0, {:#x}", base + 32), Some(base + 32)),
            ins(base + 20, "nop", "", None),
            ins(base + 24, "jal", &format!("{callee:#x}"), Some(callee)),
            ins(base + 28, "li", &format!("$a1, {count:#x}"), None),
            ins(base + 32, "lw", "$ra, 0x10($sp)", None),
            ins(base + 36, "jr", "$ra", None),
        ];
        (base, base + 40, code)
    }

    fn key(r: &(u64, u64, Vec<Instruction>)) -> String {
        // What each callee is, by the address the test gives it: 0x8001e3c0 and 0x80020000 are the same code.
        let callee = |t: u64| {
            if t == 0x8001_e3c0 || t == 0x8002_0000 {
                "Tsame".to_string()
            } else {
                format!("T{t:x}")
            }
        };
        format!("{:x}", hash_of("mips", r.0, r.1, &r.2, &|_| false, &callee))
    }

    #[test]
    fn the_same_routine_at_another_address_has_the_same_key() {
        let a = routine(0x8001_0000, 0x8006, 0x794c, 0x8001_e3c0, 4);
        let b = routine(0x800a_7800, 0x8009, 0x1234, 0x8002_0000, 4);
        assert_eq!(key(&a), key(&b));
        // A different constant is different code, and so is calling different code.
        assert_ne!(key(&a), key(&routine(0x8001_0000, 0x8006, 0x794c, 0x8001_e3c0, 5)));
        assert_ne!(key(&a), key(&routine(0x8001_0000, 0x8006, 0x794c, 0x8003_0000, 4)));
    }

    #[test]
    fn a_pointer_carried_in_another_register_is_still_an_address() {
        let mk = |base: u64, hi: u32, lo: u32| {
            let code = vec![
                ins(base, "lui", &format!("$v0, {hi:#x}"), None),
                ins(base + 4, "move", "$s2, $v0", None),
                ins(base + 8, "lw", &format!("$v1, {lo:#x}($s2)"), None),
                ins(base + 12, "lw", &format!("$a0, {:#x}($s2)", lo + 4), None),
                ins(base + 16, "addu", "$v0, $v1, $a0", None),
                ins(base + 20, "sw", "$v0, 0x8($sp)", None),
                ins(base + 24, "jr", "$ra", None),
            ];
            (base, base + 28, code)
        };
        assert_eq!(
            key(&mk(0x8001_0000, 0x8006, 0x794c)),
            key(&mk(0x8003_0000, 0x8007, 0x1000))
        );
        // A stack offset is not an address, whatever comes before it.
        let other = {
            let (s, e, mut code) = mk(0x8001_0000, 0x8006, 0x794c);
            code[5] = ins(0x8001_0014, "sw", "$v0, 0xc($sp)", None);
            (s, e, code)
        };
        assert_ne!(key(&mk(0x8001_0000, 0x8006, 0x794c)), key(&other));
    }

    #[test]
    fn a_function_of_the_c_is_found_by_name() {
        let text = "static int helper(int x);\n\nint helper(int x)\n{\n    return x + 1; /* } */\n}\n\n/* ff9_go */\nvoid\nff9_go(void)\n{\n    char *s = \"}\";\n    if (s) {\n        helper(1);\n    }\n}\n";
        let go = c_function_text(text, "ff9_go").unwrap();
        assert!(go.starts_with("void\nff9_go(void)"), "{go}");
        assert!(go.ends_with("    }\n}"), "{go}");
        let helper = c_function_text(text, "helper").unwrap();
        assert!(helper.starts_with("int helper(int x)\n{"));
        assert!(helper.ends_with("/* } */\n}"), "{helper}");
        // A prototype or a call is not a definition.
        assert!(c_function_text("int f(void);\nvoid g(void) { f(); }", "f").is_none());
    }

    #[test]
    fn an_entry_is_kept_and_found_again() {
        let dir = std::env::temp_dir().join(format!("binviz-store-{}", std::process::id()));
        let store = Store::at(&dir);
        let entry = Entry {
            key: "00ff-a".into(),
            instructions: 10,
            name: "f".into(),
            project: "game".into(),
            compiler: "gcc 2.8.1".into(),
            flags: "-O2".into(),
            sdk: String::new(),
            source: "src/f.c".into(),
            c: "void f(void) {}".into(),
            recorded: 5,
        };
        store.add(&entry).unwrap();
        assert_eq!(store.find("00ff-a"), vec![entry.clone()]);
        assert!(store.find("00ff").is_empty());
        assert_eq!(store.index()["00ff-a"], vec![entry.clone()]);
        assert_eq!(entry.build(), "gcc 2.8.1, -O2");
        let _ = std::fs::remove_dir_all(dir);
    }
}
