//! What a C or C++ source tree defines, read without a compiler: the
//! functions it defines and the string literals it has. Set against a
//! binary, it says what the binary has that the source doesn't (another
//! version of the program than the source, or code from elsewhere: a
//! `CheckNeedPass`, a `needpass` cvar) and which functions of the source
//! nothing in the binary is named after yet.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::binary::Binary;
use crate::model::{DecompState, SymbolKind, SymbolSource};

/// A function a source file defines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFunction {
    /// As written, qualified in C++ (`Shape::area`).
    pub name: String,
    pub file: String,
    pub line: u32,
}

/// What a source tree defines.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceScan {
    pub files: u32,
    pub functions: Vec<SourceFunction>,
    /// Its string literals, adjacent ones joined and escapes read.
    pub strings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String, u32),
    Punct(char),
    Str(String),
}

/// Reads the functions and string literals of source files: (path, text).
pub fn scan_sources(files: &[(String, String)]) -> SourceScan {
    let mut out = SourceScan::default();
    for (path, text) in files {
        out.files += 1;
        let toks = tokens(text);
        definitions(&toks, path, &mut out.functions);
        let mut last_str = false;
        for t in toks {
            match t {
                // "abc" "def" is one literal.
                Tok::Str(s) if last_str => out.strings.last_mut().expect("a literal").push_str(&s),
                Tok::Str(s) => {
                    out.strings.push(s);
                    last_str = true;
                    continue;
                }
                _ => {}
            }
            last_str = false;
        }
    }
    out
}

/// The tokens that matter here: identifiers (with their lines), string
/// literals, punctuation. Comments, preprocessor lines, character literals
/// and numbers are left out.
fn tokens(text: &str) -> Vec<Tok> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let (mut i, mut line, mut line_start) = (0usize, 1u32, true);
    while i < b.len() {
        let c = b[i];
        match c {
            b'\n' => {
                line += 1;
                line_start = true;
                i += 1;
                continue;
            }
            b' ' | b'\t' | b'\r' | 0x0c => {
                i += 1;
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                    if b[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 2;
                continue;
            }
            // A preprocessor line, continued by backslashes.
            b'#' if line_start => {
                while i < b.len() && b[i] != b'\n' {
                    if b[i] == b'\\' && b.get(i + 1) == Some(&b'\n') {
                        line += 1;
                        i += 1;
                    }
                    i += 1;
                }
                continue;
            }
            b'"' => {
                let (s, next, lines) = literal(b, i + 1, b'"');
                out.push(Tok::Str(s));
                line += lines;
                i = next;
            }
            b'\'' => {
                let (_, next, lines) = literal(b, i + 1, b'\'');
                line += lines;
                i = next;
            }
            _ if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                let word = &text[start..i];
                // A literal's prefix (L"…", u8"…") is part of it.
                if matches!(word, "L" | "u" | "U" | "u8") && matches!(b.get(i), Some(b'"' | b'\'')) {
                    continue;
                }
                out.push(Tok::Ident(word.to_string(), line));
            }
            _ if c.is_ascii_digit() => {
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                    i += 1;
                }
            }
            _ => {
                if c.is_ascii() {
                    out.push(Tok::Punct(c as char));
                }
                i += 1;
            }
        }
        line_start = false;
    }
    out
}

/// A string or character literal from `i` (past its opening quote) to its
/// closing `quote`: its text with escapes read, where it ends, and the lines it spans.
fn literal(b: &[u8], mut i: usize, quote: u8) -> (String, usize, u32) {
    let mut bytes = Vec::new();
    let mut lines = 0;
    while i < b.len() && b[i] != quote {
        if b[i] == b'\n' {
            // An unterminated literal ends at the line.
            break;
        }
        if b[i] != b'\\' {
            bytes.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        let Some(&e) = b.get(i) else { break };
        i += 1;
        match e {
            b'n' => bytes.push(b'\n'),
            b't' => bytes.push(b'\t'),
            b'r' => bytes.push(b'\r'),
            b'a' => bytes.push(7),
            b'b' => bytes.push(8),
            b'f' => bytes.push(12),
            b'v' => bytes.push(11),
            b'\n' => lines += 1,
            b'x' => {
                let start = i;
                while i < b.len() && b[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let v = std::str::from_utf8(&b[start..i])
                    .ok()
                    .and_then(|h| u32::from_str_radix(h, 16).ok());
                bytes.push(v.unwrap_or(0) as u8);
            }
            b'0'..=b'7' => {
                let start = i - 1;
                while i < b.len() && i - start < 3 && (b'0'..=b'7').contains(&b[i]) {
                    i += 1;
                }
                let v = std::str::from_utf8(&b[start..i])
                    .ok()
                    .and_then(|o| u32::from_str_radix(o, 8).ok());
                bytes.push(v.unwrap_or(0) as u8);
            }
            other => bytes.push(other),
        }
    }
    (
        String::from_utf8_lossy(&bytes).into_owned(),
        (i + 1).min(b.len()),
        lines,
    )
}

/// Words a name before `(` can't be: what follows them is no function.
const NOT_NAMES: &[&str] = &[
    "if",
    "for",
    "while",
    "switch",
    "return",
    "sizeof",
    "catch",
    "__attribute__",
    "__declspec",
    "defined",
    "alignof",
    "_Alignof",
    "decltype",
    "typeof",
    "__typeof__",
    "operator",
];

/// What an open brace opened.
#[derive(Clone, Copy, PartialEq)]
enum Brace {
    /// A function's body.
    Body,
    /// A namespace, a class, an `extern "C"` block: functions may be defined inside.
    Scope,
    /// Anything else (an initializer, an enum, a statement block).
    Other,
}

/// The functions defined in a file's tokens, found where a `{` follows a
/// parameter list outside any function.
fn definitions(toks: &[Tok], file: &str, out: &mut Vec<SourceFunction>) {
    let mut open: Vec<Brace> = Vec::new();
    for (k, t) in toks.iter().enumerate() {
        match t {
            Tok::Punct('{') => {
                if open.iter().any(|&b| b != Brace::Scope) {
                    open.push(Brace::Other);
                    continue;
                }
                let kind = match function_before(toks, k) {
                    Some((name, line)) => {
                        out.push(SourceFunction {
                            name,
                            file: file.to_string(),
                            line,
                        });
                        Brace::Body
                    }
                    None if opens_scope(toks, k) => Brace::Scope,
                    None => Brace::Other,
                };
                open.push(kind);
            }
            Tok::Punct('}') => {
                open.pop();
            }
            _ => {}
        }
    }
}

/// Whether the `{` at `k` opens a namespace, a class or an `extern "C"` block.
fn opens_scope(toks: &[Tok], k: usize) -> bool {
    let mut j = k;
    for _ in 0..8 {
        let Some(p) = j.checked_sub(1) else { return false };
        j = p;
        match &toks[j] {
            Tok::Ident(w, _) if matches!(w.as_str(), "namespace" | "class" | "struct" | "extern") => {
                // `struct s {` at file scope holds no functions in C, but a C++ class's methods can be in it.
                return true;
            }
            Tok::Str(_) | Tok::Ident(..) | Tok::Punct(':' | ',' | '<' | '>') => {}
            _ => return false,
        }
    }
    false
}

/// The function whose body the `{` at `k` opens, if it opens one: the name
/// before the parameter list the `{` follows (after K&R parameter
/// declarations, `const`, a constructor's initializers), and its line.
fn function_before(toks: &[Tok], k: usize) -> Option<(String, u32)> {
    // Back to the `)` closing the parameter list.
    let mut j = k;
    let close = loop {
        j = j.checked_sub(1)?;
        match &toks[j] {
            Tok::Punct(')') => {
                // A constructor's initializer `: a(0), b(1)`: keep going back past it.
                let open = matching_open(toks, j)?;
                if open > 0 && matches!(toks[open - 1], Tok::Ident(..)) && is_initializer(toks, open - 1) {
                    j = open - 1;
                    continue;
                }
                break j;
            }
            Tok::Ident(..) | Tok::Punct('*' | ',' | ';' | '[' | ']' | '&' | ':') => {}
            _ => return None,
        }
    };
    let open = matching_open(toks, close)?;
    let (name, line) = match open.checked_sub(1).map(|p| &toks[p])? {
        Tok::Ident(w, line) if !NOT_NAMES.contains(&w.as_str()) => (w.clone(), *line),
        _ => return None,
    };
    // A qualified name: `Shape::area`, `ns::Shape::area`, `Shape::~Shape`.
    let mut qualified = name;
    let mut p = open - 1;
    loop {
        let tilde = matches!(p.checked_sub(1).map(|q| &toks[q]), Some(Tok::Punct('~')));
        let q = if tilde { p - 1 } else { p };
        match (q.checked_sub(3).map(|x| &toks[x..q])?, tilde) {
            ([Tok::Ident(scope, _), Tok::Punct(':'), Tok::Punct(':')], _) => {
                qualified = format!("{scope}::{}{qualified}", if tilde { "~" } else { "" });
                p = q - 3;
            }
            _ => break,
        }
        if p == 0 {
            break;
        }
    }
    Some((qualified, line))
}

/// Whether the call-shaped `name(…)` at `k` is one of a constructor's
/// initializers: preceded by `:` or `,` that follow the parameter list.
fn is_initializer(toks: &[Tok], k: usize) -> bool {
    let Some(before) = k.checked_sub(1) else { return false };
    match &toks[before] {
        Tok::Punct(':') => !matches!(before.checked_sub(1).map(|b| &toks[b]), Some(Tok::Punct(':'))),
        Tok::Punct(',') => {
            // Only when what comes before is another initializer.
            let Some(prev_close) = before.checked_sub(1) else {
                return false;
            };
            matches!(toks[prev_close], Tok::Punct(')'))
                && matching_open(toks, prev_close)
                    .and_then(|o| o.checked_sub(1))
                    .is_some_and(|n| matches!(toks[n], Tok::Ident(..)) && is_initializer(toks, n))
        }
        _ => false,
    }
}

/// The `(` a `)` at `close` closes.
fn matching_open(toks: &[Tok], close: usize) -> Option<usize> {
    let mut depth = 0;
    for j in (0..=close).rev() {
        match toks[j] {
            Tok::Punct(')') => depth += 1,
            Tok::Punct('(') => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            // A parameter list holds no braces.
            Tok::Punct('{' | '}') => return None,
            _ => {}
        }
    }
    None
}

/// A function's name as the source would spell it: without the decoration
/// 32-bit Windows compilers add (`_f`, `_f@8`, `@f@8`), parameters, return
/// type or qualifiers (`Shape::area`, `int __cdecl f(int)`: `area`, `f`).
pub(crate) fn source_name(name: &str) -> String {
    let n = name.split('(').next().unwrap_or(name).trim();
    let n = n.rsplit(' ').next().unwrap_or(n);
    let n = n.rsplit("::").next().unwrap_or(n);
    let n = n.trim_start_matches('@');
    let n = n.split('@').next().unwrap_or(n);
    n.to_string()
}

/// A binary set against the source it may have been built from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCounterparts {
    pub files: u32,
    pub source_functions: u32,
    /// Functions here with a name of their own.
    pub named_functions: u32,
    /// Named functions here that the source defines nowhere: (address, name).
    pub only_here: Vec<(u64, String)>,
    /// Functions the source defines that nothing here is named after:
    /// inlined, unused, not named here yet, or from another version.
    pub only_in_source: Vec<SourceFunction>,
    /// Strings the code uses that the source has nowhere: (address, text,
    /// the functions using it).
    pub strings_only_here: Vec<(u64, String, Vec<String>)>,
}

impl Binary {
    /// What this binary has that the source (files: path, text) doesn't,
    /// and the other way round: functions by name, and the strings its code
    /// uses. Library code (notes marking it so) and imports are left out.
    pub fn source_counterparts(&self, files: &[(String, String)]) -> SourceCounterparts {
        let scan = scan_sources(files);
        let defined: HashSet<String> = scan.functions.iter().map(|f| source_name(&f.name)).collect();
        let mut named_here: HashSet<String> = HashSet::new();
        let mut only_here = Vec::new();
        let mut named_functions = 0;
        let mut name_at: BTreeMap<u64, String> = BTreeMap::new();
        for f in self.symbols().functions() {
            let name = f.display_name().into_owned();
            name_at.insert(f.address, name.clone());
            if matches!(f.source, SymbolSource::Discovered | SymbolSource::Import)
                || f.kind != SymbolKind::Function
                || name.starts_with("sub_")
                || name.starts_with("func[")
                || self
                    .decomp_at(f.address)
                    .is_some_and(|d| d.state == DecompState::Library)
            {
                continue;
            }
            named_functions += 1;
            let base = source_name(&name);
            // `_f` is `f` from C on 32-bit Windows, but a function the source calls `_f` is `_f`.
            let found = defined.contains(&base) || base.strip_prefix('_').is_some_and(|b| defined.contains(b));
            named_here.insert(base.clone());
            if let Some(b) = base.strip_prefix('_') {
                named_here.insert(b.to_string());
            }
            if !found {
                only_here.push((f.address, name));
            }
        }
        let only_in_source: Vec<SourceFunction> = scan
            .functions
            .iter()
            .filter(|f| !named_here.contains(&source_name(&f.name)))
            .cloned()
            .collect();
        // Strings: a literal of the source holding the text (a string's tail is shared by the linker).
        let literals = scan.strings.join("\0");
        let mut used: BTreeMap<u64, (String, Vec<String>)> = BTreeMap::new();
        for (&address, name) in &name_at {
            let Some(s) = self.function_summary(address, 400) else {
                continue;
            };
            for u in s.strings {
                if u.text.chars().count() < 3 {
                    continue;
                }
                let e = used.entry(u.address).or_insert_with(|| (u.text, Vec::new()));
                if !e.1.contains(name) {
                    e.1.push(name.clone());
                }
            }
        }
        let strings_only_here = used
            .into_iter()
            .filter(|(_, (text, _))| !literals.contains(text.as_str()))
            .map(|(a, (text, users))| (a, text, users))
            .collect();
        SourceCounterparts {
            files: scan.files,
            source_functions: scan.functions.len() as u32,
            named_functions,
            only_here,
            only_in_source,
            strings_only_here,
        }
    }
}

impl SourceCounterparts {
    pub fn to_text(&self, limit: usize) -> String {
        let mut out = format!(
            "{} source files define {} functions; {} functions here have names.\n",
            self.files, self.source_functions, self.named_functions
        );
        let more = |out: &mut String, n: usize| {
            if n > limit {
                out.push_str(&format!("  … and {} more\n", n - limit));
            }
        };
        out.push_str(&format!(
            "\nNamed here, defined nowhere in the source ({}): another version, or code from elsewhere\n",
            self.only_here.len()
        ));
        for (a, n) in self.only_here.iter().take(limit) {
            out.push_str(&format!("  {a:#x}  {n}\n"));
        }
        more(&mut out, self.only_here.len());
        out.push_str(&format!(
            "\nStrings the code uses that the source doesn't have ({}):\n",
            self.strings_only_here.len()
        ));
        for (a, t, users) in self.strings_only_here.iter().take(limit) {
            let text: String = t.chars().take(80).collect();
            out.push_str(&format!("  {a:#x}  {text:?}  (in {})\n", users.join(", ")));
        }
        more(&mut out, self.strings_only_here.len());
        out.push_str(&format!(
            "\nDefined in the source, nothing here named after them ({}): inlined, unused, not named yet, or not in this version\n",
            self.only_in_source.len()
        ));
        for f in self.only_in_source.iter().take(limit) {
            out.push_str(&format!("  {}  ({}:{})\n", f.name, f.file, f.line));
        }
        more(&mut out, self.only_in_source.len());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{scan_sources, source_name};

    #[test]
    fn definitions_and_literals_are_read_without_a_compiler() {
        let c = r#"
#include <stdio.h>
#define TWICE(x) ((x) * 2)
static const char *names[] = { "zero", "one" };
struct point { int x, y; };
int add(int a, int b) { return a + b; }
/* void commented(void) { } */
static void greet(const char *who)
{
    printf("hello, " "%s\n", who);   // joined
    if (who) { puts("\x41\102"); }
}
int old_style(a, b)
    int a;
    char *b;
{
    return a;
}
int (*handler)(int) = 0;
void declared(void);
"#;
        let cpp = r#"
namespace geo {
class Shape {
public:
    virtual int area() const { return 0; }
};
int Shape::perimeter() const { return 1; }
Rect::Rect(int w) : w_(w), h_(2) {}
Rect::~Rect() {}
}
extern "C" { void from_c(void) {} }
"#;
        let s = scan_sources(&[("a.c".into(), c.into()), ("b.cpp".into(), cpp.into())]);
        let names: Vec<&str> = s.functions.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "add",
                "greet",
                "old_style",
                "area",
                "Shape::perimeter",
                "Rect::Rect",
                "Rect::~Rect",
                "from_c"
            ]
        );
        assert_eq!((s.functions[1].file.as_str(), s.functions[1].line), ("a.c", 8));
        for t in ["zero", "one", "hello, %s\n", "AB"] {
            assert!(s.strings.iter().any(|x| x == t), "{t}: {:?}", s.strings);
        }
        assert!(!s.strings.iter().any(|x| x.contains("stdio")));
        assert_eq!(source_name("_find_char"), "_find_char");
        assert_eq!(source_name("_mix@8"), "_mix");
        assert_eq!(source_name("@scale@8"), "scale");
        assert_eq!(source_name("int __cdecl geo::Shape::area(void)"), "area");
    }
}

/// The C and C++ files under `dir` (sources and headers), read: (path under `dir`, text).
pub fn read_source_tree(dir: &std::path::Path) -> std::io::Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&d)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(
                ext.as_str(),
                "c" | "cc" | "cpp" | "cxx" | "h" | "hh" | "hpp" | "hxx" | "inl" | "m" | "mm"
            ) {
                continue;
            }
            let bytes = std::fs::read(&p)?;
            let name = p.strip_prefix(dir).unwrap_or(&p).to_string_lossy().into_owned();
            out.push((name, String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
    out.sort();
    Ok(out)
}
