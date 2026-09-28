//! Objective-C metadata in Mach-O images: the classes and categories the
//! runtime registers (`__objc_classlist`, `__objc_catlist`) with their
//! methods, instance variables, properties and protocols; the protocols
//! (`__objc_protolist`); the selectors code refers to (`__objc_selrefs`); and
//! the `objc_msgSend$selector` stubs linkers make to send them
//! (`__objc_stubs`).
//!
//! Stripped binaries keep all of this, so it names what their symbol table no
//! longer does: each method's implementation (`-[Greeter hello]`), the
//! metadata (`_OBJC_CLASS_$_Greeter`) and the references code loads
//! (`@selector(hello)`, `@class(NSObject)`). With the references found in
//! code, it also says which functions send a selector.
//!
//! Swift classes visible to Objective-C are listed too, their mangled names
//! (`_TtC4Shop8CartView`) shown as Swift writes them (`Shop.CartView`); the
//! rest of Swift's metadata is not read.

use std::collections::HashMap;

use object::Architecture;
use serde::Serialize;

use crate::binary::Binary;
use crate::model::{Format, Section};
use crate::pointers::{Scheme, section_bytes};
use crate::util::Bytes;
use crate::xrefs::{CallEdge, RefKind};

/// What the Objective-C runtime reads from an image.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjcInfo {
    pub classes: Vec<ObjcClass>,
    pub categories: Vec<ObjcCategory>,
    pub protocols: Vec<ObjcProtocol>,
    /// The selectors code refers to, by name.
    pub selectors: Vec<ObjcSelector>,
    /// Names for the metadata, the methods and the references, for the symbol table.
    #[serde(skip)]
    pub(crate) names: Vec<ObjcName>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjcClass {
    /// The class object (`class_t`).
    pub address: u64,
    /// As written: `Greeter`, or for a Swift class `Shop.CartView`.
    pub name: String,
    /// The name the runtime knows, when it differs (a Swift class's `_TtC4Shop8CartView`).
    pub runtime_name: Option<String>,
    /// The superclass (`NSObject`), when there is one.
    pub superclass: Option<String>,
    pub swift: bool,
    pub instance_size: u32,
    pub protocols: Vec<String>,
    /// Instance methods, then class methods.
    pub methods: Vec<ObjcMethod>,
    pub ivars: Vec<ObjcIvar>,
    pub properties: Vec<ObjcProperty>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjcCategory {
    pub address: u64,
    pub name: String,
    /// The class it adds to, defined here or elsewhere (`NSObject`).
    pub class: String,
    pub protocols: Vec<String>,
    pub methods: Vec<ObjcMethod>,
    pub properties: Vec<ObjcProperty>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjcProtocol {
    pub address: u64,
    pub name: String,
    pub runtime_name: Option<String>,
    /// The protocols it adopts.
    pub protocols: Vec<String>,
    pub required: Vec<ObjcMethod>,
    pub optional: Vec<ObjcMethod>,
    pub properties: Vec<ObjcProperty>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjcMethod {
    pub selector: String,
    /// The type encoding: `v16@0:8`.
    pub types: String,
    /// The implementation (0 for none: protocols declare methods, they don't implement them).
    pub imp: u64,
    /// A class method (`+`) rather than an instance method (`-`).
    pub class_method: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjcIvar {
    pub name: String,
    /// The type encoding: `@"NSString"`.
    pub types: String,
    /// Where it is in an instance, as the file records it.
    pub offset: Option<u32>,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjcProperty {
    pub name: String,
    /// `T@"NSString",&,N,V_name`
    pub attributes: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjcSelector {
    pub name: String,
    /// Selector references: the slots code loads it from.
    pub references: Vec<u64>,
    /// `objc_msgSend$name` stubs, which send it.
    pub stubs: Vec<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct ObjcName {
    pub address: u64,
    pub name: String,
    pub size: u64,
    pub code: bool,
}

/// How much Objective-C an image has.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjcCounts {
    pub classes: u32,
    /// Of the classes, those written in Swift.
    pub swift_classes: u32,
    pub categories: u32,
    pub protocols: u32,
    pub methods: u32,
    pub selectors: u32,
}

impl ObjcCounts {
    /// `1 class, 2 categories, 0 protocols; 12 methods, 30 selectors referenced`
    pub fn to_text(&self) -> String {
        let swift = self.swift_classes;
        format!(
            "{}{}, {}, {}; {}, {} referenced",
            count(self.classes as usize, "class"),
            if swift > 0 {
                format!(" ({swift} Swift)")
            } else {
                String::new()
            },
            count(self.categories as usize, "category"),
            count(self.protocols as usize, "protocol"),
            count(self.methods as usize, "method"),
            count(self.selectors as usize, "selector"),
        )
    }
}

/// A class, category or protocol, for lists.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjcEntry {
    /// `class`, `category` or `protocol`.
    pub kind: &'static str,
    /// `Greeter`, `NSObject (Extras)`, `NSCopying`.
    pub name: String,
    pub address: u64,
    /// The superclass, or for a category the class it adds to.
    pub base: Option<String>,
    pub methods: u32,
    pub swift: bool,
}

/// One line of an interface as a header would declare it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceLine {
    pub text: String,
    /// A method's implementation.
    pub address: Option<u64>,
    /// A method's selector.
    pub selector: Option<String>,
}

/// A class, category or protocol declared as in a header (what class-dump prints).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjcInterface {
    pub kind: &'static str,
    pub name: String,
    pub address: u64,
    pub lines: Vec<InterfaceLine>,
}

impl ObjcInterface {
    pub fn to_text(&self) -> String {
        let width = self
            .lines
            .iter()
            .filter(|l| l.address.is_some())
            .map(|l| l.text.chars().count())
            .max()
            .unwrap_or(0)
            .min(72);
        let mut out = String::new();
        for l in &self.lines {
            match l.address {
                Some(a) => out.push_str(&format!("{:<width$}  // {a:#x}\n", l.text)),
                None => out.push_str(&format!("{}\n", l.text)),
            }
        }
        out
    }
}

/// A method implementing a selector.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Implementation {
    /// `-[Greeter hello]`
    pub name: String,
    pub address: u64,
}

/// Where a selector is implemented and who sends it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorUses {
    pub selector: String,
    pub implementations: Vec<Implementation>,
    pub references: Vec<u64>,
    pub stubs: Vec<u64>,
    /// Functions loading one of its references (to send it through
    /// `objc_msgSend`, or to name it: `@selector(hello)`) or calling one of
    /// its stubs, most sites first.
    pub senders: Vec<CallEdge>,
}

impl SelectorUses {
    pub fn to_text(&self) -> String {
        let mut out = format!("selector {}\n", self.selector);
        if self.implementations.is_empty() {
            out.push_str("implemented: nowhere in this image\n");
        } else {
            out.push_str(&format!(
                "implemented by {}:\n",
                count(self.implementations.len(), "method")
            ));
            for m in &self.implementations {
                out.push_str(&format!("  {:#x}  {}\n", m.address, m.name));
            }
        }
        let refs: Vec<String> = self.references.iter().map(|a| format!("{a:#x}")).collect();
        let stubs: Vec<String> = self.stubs.iter().map(|a| format!("{a:#x}")).collect();
        if !refs.is_empty() {
            out.push_str(&format!("selector references: {}\n", refs.join(", ")));
        }
        if !stubs.is_empty() {
            out.push_str(&format!("objc_msgSend$ stubs: {}\n", stubs.join(", ")));
        }
        if self.senders.is_empty() {
            out.push_str("sent by: no code found\n");
        } else {
            out.push_str(&format!("sent by {}:\n", count(self.senders.len(), "function")));
            for s in &self.senders {
                out.push_str(&format!(
                    "  {:#x}  {}  ({})\n",
                    s.address,
                    s.name,
                    count(s.calls as usize, "site")
                ));
            }
        }
        out
    }
}

impl ObjcMethod {
    /// `-[Greeter hello]`, `+[Greeter(Extras) make]`
    pub fn name(&self, class: &str, category: Option<&str>) -> String {
        let sign = if self.class_method { '+' } else { '-' };
        match category {
            Some(c) => format!("{sign}[{class}({c}) {}]", self.selector),
            None => format!("{sign}[{class} {}]", self.selector),
        }
    }
}

impl ObjcInfo {
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.categories.is_empty() && self.protocols.is_empty() && self.selectors.is_empty()
    }

    pub fn method_count(&self) -> usize {
        self.classes.iter().map(|c| c.methods.len()).sum::<usize>()
            + self.categories.iter().map(|c| c.methods.len()).sum::<usize>()
    }

    pub fn counts(&self) -> ObjcCounts {
        ObjcCounts {
            classes: self.classes.len() as u32,
            swift_classes: self.classes.iter().filter(|c| c.swift).count() as u32,
            categories: self.categories.len() as u32,
            protocols: self.protocols.len() as u32,
            methods: self.method_count() as u32,
            selectors: self.selectors.len() as u32,
        }
    }

    /// Classes, categories and protocols, each kind sorted by name.
    pub fn entries(&self) -> Vec<ObjcEntry> {
        let mut out: Vec<ObjcEntry> = Vec::new();
        for c in &self.classes {
            out.push(ObjcEntry {
                kind: "class",
                name: c.name.clone(),
                address: c.address,
                base: c.superclass.clone(),
                methods: c.methods.len() as u32,
                swift: c.swift,
            });
        }
        for c in &self.categories {
            out.push(ObjcEntry {
                kind: "category",
                name: format!("{} ({})", c.class, c.name),
                address: c.address,
                base: Some(c.class.clone()),
                methods: c.methods.len() as u32,
                swift: false,
            });
        }
        for p in &self.protocols {
            out.push(ObjcEntry {
                kind: "protocol",
                name: p.name.clone(),
                address: p.address,
                base: None,
                methods: (p.required.len() + p.optional.len()) as u32,
                swift: p.runtime_name.is_some(),
            });
        }
        let order = |k: &str| match k {
            "class" => 0,
            "category" => 1,
            _ => 2,
        };
        out.sort_by(|a, b| {
            order(a.kind)
                .cmp(&order(b.kind))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        out
    }

    /// The class, category (`NSObject (Extras)`, or just `Extras`) or protocol
    /// named `name`, declared as a header would.
    pub fn interface(&self, name: &str) -> Option<ObjcInterface> {
        ["class", "category", "protocol"]
            .into_iter()
            .find_map(|kind| self.interface_of(kind, name))
    }

    /// Like [`Self::interface`], for one kind: `class`, `category` or `protocol`.
    pub fn interface_of(&self, kind: &str, name: &str) -> Option<ObjcInterface> {
        let wanted = |n: &str| n == name || n.eq_ignore_ascii_case(name);
        match kind {
            "class" => self
                .classes
                .iter()
                .find(|c| wanted(&c.name) || c.runtime_name.as_deref().is_some_and(wanted))
                .map(class_interface),
            "category" => {
                let compact: String = name.chars().filter(|c| !c.is_whitespace()).collect();
                self.categories
                    .iter()
                    .find(|c| wanted(&c.name) || format!("{}({})", c.class, c.name).eq_ignore_ascii_case(&compact))
                    .map(category_interface)
            }
            "protocol" => self
                .protocols
                .iter()
                .find(|p| wanted(&p.name) || p.runtime_name.as_deref().is_some_and(wanted))
                .map(protocol_interface),
            _ => None,
        }
    }

    /// Methods implementing `selector`: `-[Greeter hello]` and where.
    pub fn implementations(&self, selector: &str) -> Vec<Implementation> {
        let mut out = Vec::new();
        for c in &self.classes {
            for m in c.methods.iter().filter(|m| m.selector == selector && m.imp != 0) {
                out.push(Implementation {
                    name: m.name(&c.name, None),
                    address: m.imp,
                });
            }
        }
        for c in &self.categories {
            for m in c.methods.iter().filter(|m| m.selector == selector && m.imp != 0) {
                out.push(Implementation {
                    name: m.name(&c.class, Some(&c.name)),
                    address: m.imp,
                });
            }
        }
        out
    }

    /// What `binviz objc` prints: counts, then every class, category and protocol.
    pub fn to_text(&self, limit: usize) -> String {
        if self.is_empty() {
            return "no Objective-C metadata\n".into();
        }
        let mut out = format!("Objective-C: {}\n", self.counts().to_text());
        let entries = self.entries();
        let width = entries
            .iter()
            .take(limit)
            .map(|e| e.name.chars().count() + 3)
            .max()
            .unwrap_or(0)
            .min(60);
        let mut kind = "";
        for e in entries.iter().take(limit) {
            if e.kind != kind {
                kind = e.kind;
                out.push_str(match kind {
                    "class" => "classes:\n",
                    "category" => "categories:\n",
                    _ => "protocols:\n",
                });
            }
            let label = match (&e.base, e.kind) {
                (Some(base), "class") => format!("{} : {base}", e.name),
                _ => e.name.clone(),
            };
            out.push_str(&format!(
                "  {:#x}  {label:<width$}  {}\n",
                e.address,
                count(e.methods as usize, "method")
            ));
        }
        if entries.len() > limit {
            out.push_str(&format!("… {} more\n", entries.len() - limit));
        }
        out
    }
}

/// `1 method`, `2 classes`
fn count(n: usize, noun: &str) -> String {
    match (n, noun) {
        (1, _) => format!("1 {noun}"),
        (_, "class") => format!("{n} classes"),
        (_, "category") => format!("{n} categories"),
        _ => format!("{n} {noun}s"),
    }
}

/// A Swift class or protocol's Objective-C name as Swift writes it:
/// `_TtC4Shop8CartView` is `Shop.CartView`, `_TtP4Shop8Delegate_` is
/// `Shop.Delegate`. Only simple names (no generics, no private discriminators).
pub fn swift_name(name: &str) -> Option<String> {
    // A class's metadata symbol, when a superclass is bound: $s4Shop4BaseCN.
    if let Some(rest) = name.strip_prefix("$s").and_then(|r| r.strip_suffix("CN")) {
        let mut rest = rest;
        let module = take_identifier(&mut rest)?;
        let class = take_identifier(&mut rest)?;
        return rest.is_empty().then(|| format!("{module}.{class}"));
    }
    let rest = name.strip_prefix("_Tt")?;
    // One letter per level of nesting: C class, P protocol, V struct, O enum.
    let levels = rest.bytes().take_while(|c| b"CPVO".contains(c)).count();
    if levels == 0 {
        return None;
    }
    let mut rest = &rest[levels..];
    let take = take_identifier;
    let mut parts = Vec::new();
    // `s` is the standard library.
    if rest.starts_with('s') && rest[1..].starts_with(|c: char| c.is_ascii_digit()) {
        parts.push("Swift".to_string());
        rest = &rest[1..];
    } else {
        parts.push(take(&mut rest)?);
    }
    for _ in 0..levels {
        parts.push(take(&mut rest)?);
    }
    // Protocols end with `_`.
    let rest = rest.strip_prefix('_').unwrap_or(rest);
    rest.is_empty().then(|| parts.join("."))
}

/// A length-prefixed identifier of a Swift mangled name: `4Shop`.
fn take_identifier(rest: &mut &str) -> Option<String> {
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let n: usize = rest[..digits].parse().ok()?;
    let s = rest.get(digits..digits + n)?.to_string();
    *rest = &rest[digits + n..];
    Some(s)
}

/// The C spelling of the first type in an Objective-C type encoding, and the rest.
fn read_type(s: &str) -> (String, &str) {
    let mut qualifiers = String::new();
    let mut s = s;
    while let Some(q) = match s.as_bytes().first() {
        Some(b'r') => Some("const "),
        Some(b'n') => Some("in "),
        Some(b'N') => Some("inout "),
        Some(b'o') => Some("out "),
        Some(b'O') => Some("bycopy "),
        Some(b'R') => Some("byref "),
        Some(b'V') => Some("oneway "),
        Some(b'A') => Some("_Atomic "),
        Some(b'j') => Some("_Complex "),
        _ => None,
    } {
        qualifiers.push_str(q);
        s = &s[1..];
    }
    let Some(c) = s.chars().next() else {
        return (qualifiers + "void", s);
    };
    let rest = &s[c.len_utf8()..];
    let simple = |t: &str| (t.to_string(), rest);
    let (ty, rest) = match c {
        'c' => simple("char"),
        'i' => simple("int"),
        's' => simple("short"),
        'l' => simple("long"),
        'q' => simple("long long"),
        'C' => simple("unsigned char"),
        'I' => simple("unsigned int"),
        'S' => simple("unsigned short"),
        'L' => simple("unsigned long"),
        'Q' => simple("unsigned long long"),
        'f' => simple("float"),
        'd' => simple("double"),
        'D' => simple("long double"),
        'B' => simple("BOOL"),
        'v' => simple("void"),
        '*' => simple("char *"),
        '#' => simple("Class"),
        ':' => simple("SEL"),
        't' => simple("__int128"),
        'T' => simple("unsigned __int128"),
        '?' => simple("void *"),
        '@' => {
            if let Some(r) = rest.strip_prefix('?') {
                // A block, maybe with its signature: @?<v@?@>
                let r = match r.strip_prefix('<') {
                    Some(inner) => skip_nested(inner, '<', '>'),
                    None => r,
                };
                ("id /* block */".to_string(), r)
            } else if let Some(r) = rest.strip_prefix('"') {
                let end = r.find('"').unwrap_or(r.len());
                let name = &r[..end];
                let r = r.get(end + 1..).unwrap_or("");
                let ty = if name.is_empty() {
                    "id".to_string()
                } else if name.starts_with('<') {
                    format!("id{name}")
                } else {
                    format!("{name} *")
                };
                (ty, r)
            } else {
                simple("id")
            }
        }
        '^' => {
            if let Some(r) = rest.strip_prefix('?') {
                ("void * /* function */".to_string(), r)
            } else {
                let (inner, r) = read_type(rest);
                let ty = if inner.ends_with('*') {
                    format!("{inner}*")
                } else {
                    format!("{inner} *")
                };
                (ty, r)
            }
        }
        '[' => {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            let (inner, r) = read_type(&rest[digits..]);
            let r = skip_nested(r, '[', ']');
            (format!("{inner}[{}]", &rest[..digits]), r)
        }
        '{' | '(' => {
            let close = if c == '{' { '}' } else { ')' };
            let r = skip_nested(rest, c, close);
            let body = &rest[..rest.len() - r.len()];
            let name = body.split(['=', close]).next().unwrap_or("");
            let keyword = if c == '{' { "struct" } else { "union" };
            let ty = if name.is_empty() || name == "?" {
                format!("{keyword} {{…}}")
            } else {
                format!("{keyword} {name}")
            };
            (ty, r)
        }
        'b' => {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            (format!("unsigned int :{}", &rest[..digits]), &rest[digits..])
        }
        _ => simple("?"),
    };
    (qualifiers + &ty, rest)
}

/// What follows the `close` matching an `open` already read.
fn skip_nested(s: &str, open: char, close: char) -> &str {
    let mut depth = 1;
    let mut quoted = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => quoted = !quoted,
            _ if quoted => {}
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return &s[i + 1..];
                }
            }
            _ => {}
        }
    }
    ""
}

/// Skips the stack offset after a type in a method's type encoding.
fn skip_offset(s: &str) -> &str {
    s.trim_start_matches(|c: char| c.is_ascii_digit() || c == '-')
}

/// A method as a header declares it: `- (void)greetWith:(id)arg1 times:(int)arg2;`.
pub fn method_declaration(selector: &str, types: &str, class_method: bool) -> String {
    let sign = if class_method { '+' } else { '-' };
    let colons = selector.matches(':').count();
    if types.is_empty() {
        return format!("{sign} {selector};");
    }
    let (ret, rest) = read_type(types);
    let mut rest = skip_offset(rest);
    let mut args = Vec::new();
    while !rest.is_empty() && args.len() < 64 {
        let (ty, r) = read_type(rest);
        if r.len() == rest.len() {
            break;
        }
        args.push(ty);
        rest = skip_offset(r);
    }
    // The first two arguments are self and _cmd.
    let args = args.get(2..).unwrap_or(&[]);
    if colons == 0 || colons != args.len() {
        return format!("{sign} ({ret}){selector};");
    }
    let mut out = format!("{sign} ({ret})");
    for (i, (part, ty)) in selector.split_inclusive(':').zip(args).enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{part}({ty})arg{}", i + 1));
    }
    out.push(';');
    out
}

/// A property as a header declares it: `@property (nonatomic, copy) NSString *name;`.
pub fn property_declaration(name: &str, attributes: &str) -> String {
    let mut ty = "id".to_string();
    let (mut nonatomic, mut readonly) = (false, false);
    let (mut memory, mut getter, mut setter) = (None, None, None);
    for a in attributes.split(',') {
        let mut chars = a.chars();
        match chars.next() {
            Some('T') => ty = read_type(chars.as_str()).0,
            Some('R') => readonly = true,
            Some('N') => nonatomic = true,
            Some('C') => memory = Some("copy"),
            Some('&') => memory = Some("strong"),
            Some('W') => memory = Some("weak"),
            Some('G') => getter = Some(chars.as_str()),
            Some('S') => setter = Some(chars.as_str()),
            _ => {}
        }
    }
    let mut list: Vec<String> = Vec::new();
    if nonatomic {
        list.push("nonatomic".into());
    }
    if let Some(m) = memory {
        list.push(m.into());
    }
    if readonly {
        list.push("readonly".into());
    }
    if let Some(g) = getter {
        list.push(format!("getter={g}"));
    }
    if let Some(s) = setter {
        list.push(format!("setter={s}"));
    }
    let list = if list.is_empty() {
        String::new()
    } else {
        format!(" ({})", list.join(", "))
    };
    format!("@property{list} {};", declare(&ty, name))
}

/// `NSString *name`, `int count`
fn declare(ty: &str, name: &str) -> String {
    if let Some((ty, bits)) = ty.split_once(" :") {
        format!("{ty} {name}:{bits}")
    } else if ty.ends_with('*') {
        format!("{ty}{name}")
    } else if let Some(at) = ty.find('[') {
        // int[3] x is int x[3]
        format!("{} {name}{}", &ty[..at], &ty[at..])
    } else {
        format!("{ty} {name}")
    }
}

fn line(text: impl Into<String>) -> InterfaceLine {
    InterfaceLine {
        text: text.into(),
        address: None,
        selector: None,
    }
}

fn method_lines(methods: &[ObjcMethod], lines: &mut Vec<InterfaceLine>) {
    // Class methods first, as headers list them.
    let (class, instance): (Vec<&ObjcMethod>, Vec<&ObjcMethod>) = methods.iter().partition(|m| m.class_method);
    for m in class.into_iter().chain(instance) {
        lines.push(InterfaceLine {
            text: method_declaration(&m.selector, &m.types, m.class_method),
            address: (m.imp != 0).then_some(m.imp),
            selector: Some(m.selector.clone()),
        });
    }
}

fn adopts(protocols: &[String]) -> String {
    if protocols.is_empty() {
        String::new()
    } else {
        format!(" <{}>", protocols.join(", "))
    }
}

fn class_interface(c: &ObjcClass) -> ObjcInterface {
    let mut lines = Vec::new();
    let mut header = format!("@interface {}", c.name);
    if let Some(s) = &c.superclass {
        header.push_str(&format!(" : {s}"));
    }
    header.push_str(&adopts(&c.protocols));
    if let Some(r) = &c.runtime_name {
        header.push_str(&format!("  // {r}"));
    }
    lines.push(line(header));
    if !c.ivars.is_empty() {
        lines.push(line("{"));
        for v in &c.ivars {
            let ty = read_type(&v.types).0;
            let at = v.offset.map(|o| format!("  // +{o:#x}")).unwrap_or_default();
            lines.push(line(format!("    {};{at}", declare(&ty, &v.name))));
        }
        lines.push(line("}"));
    }
    for p in &c.properties {
        lines.push(line(property_declaration(&p.name, &p.attributes)));
    }
    method_lines(&c.methods, &mut lines);
    lines.push(line("@end"));
    ObjcInterface {
        kind: "class",
        name: c.name.clone(),
        address: c.address,
        lines,
    }
}

fn category_interface(c: &ObjcCategory) -> ObjcInterface {
    let mut lines = vec![line(format!(
        "@interface {} ({}){}",
        c.class,
        c.name,
        adopts(&c.protocols)
    ))];
    for p in &c.properties {
        lines.push(line(property_declaration(&p.name, &p.attributes)));
    }
    method_lines(&c.methods, &mut lines);
    lines.push(line("@end"));
    ObjcInterface {
        kind: "category",
        name: format!("{} ({})", c.class, c.name),
        address: c.address,
        lines,
    }
}

fn protocol_interface(p: &ObjcProtocol) -> ObjcInterface {
    let mut lines = vec![line(format!("@protocol {}{}", p.name, adopts(&p.protocols)))];
    for prop in &p.properties {
        lines.push(line(property_declaration(&prop.name, &prop.attributes)));
    }
    method_lines(&p.required, &mut lines);
    if !p.optional.is_empty() {
        lines.push(line("@optional"));
        method_lines(&p.optional, &mut lines);
    }
    lines.push(line("@end"));
    ObjcInterface {
        kind: "protocol",
        name: p.name.clone(),
        address: p.address,
        lines,
    }
}

/// The fields of a `class_ro_t`.
struct Ro {
    flags: u32,
    instance_size: u32,
    name: String,
    methods: u64,
    protocols: u64,
    ivars: u64,
    properties: u64,
}

/// `class_ro_t.flags`: the metaclass's.
const RO_META: u32 = 1;

struct Reader<'a> {
    bin: &'a Binary,
    b: Bytes<'a>,
    scheme: &'a Scheme,
    /// Pointer size.
    p: u64,
    names: Vec<ObjcName>,
}

impl Reader<'_> {
    fn u32(&self, address: u64) -> Option<u32> {
        self.b.u32(self.bin.address_to_offset(address)?)
    }

    fn i32(&self, address: u64) -> Option<i64> {
        self.u32(address).map(|v| v as i32 as i64)
    }

    /// The address stored at `address`; 0 for none (or a bind: see `bound`).
    fn ptr(&self, address: u64) -> u64 {
        if address == 0 {
            return 0;
        }
        self.bin.decode_pointer(self.scheme, address).unwrap_or(0)
    }

    /// The address a field `address` holds an offset to (a relative method list's).
    fn relative(&self, address: u64) -> Option<u64> {
        self.i32(address).map(|d| address.wrapping_add_signed(d))
    }

    fn str(&self, address: u64) -> Option<String> {
        if address == 0 {
            return None;
        }
        let s = self.b.cstr(self.bin.address_to_offset(address)?, 4096)?;
        Some(String::from_utf8_lossy(s).into_owned())
    }

    /// Masks the flags out of `class_t.data`.
    fn data(&self, class: u64) -> (u64, bool) {
        let raw = self.ptr(class + 4 * self.p);
        let mask = if self.p == 8 {
            0x0000_7FFF_FFFF_FFF8
        } else {
            0xFFFF_FFFC
        };
        // FAST_IS_SWIFT_LEGACY, FAST_IS_SWIFT_STABLE
        (raw & mask, raw & 3 != 0)
    }

    /// Where `class_ro_t`'s pointers begin (after flags, sizes and, on 64-bit, padding).
    fn ro_fields(&self) -> u64 {
        if self.p == 8 { 16 } else { 12 }
    }

    fn ro(&self, address: u64) -> Option<Ro> {
        if address == 0 {
            return None;
        }
        let (p, f) = (self.p, self.ro_fields());
        Some(Ro {
            flags: self.u32(address)?,
            instance_size: self.u32(address + 8)?,
            name: self.str(self.ptr(address + f + p))?,
            methods: self.ptr(address + f + 2 * p),
            protocols: self.ptr(address + f + 3 * p),
            ivars: self.ptr(address + f + 4 * p),
            properties: self.ptr(address + f + 6 * p),
        })
    }

    /// The runtime name of the class object at `address`, in this image.
    fn class_name(&self, address: u64) -> Option<String> {
        let (data, _) = self.data(address);
        self.str(self.ptr(data + self.ro_fields() + self.p))
    }

    /// The runtime name of the class a slot refers to: one bound from
    /// elsewhere (`_OBJC_CLASS_$_NSObject`) or a class object in this image.
    fn class_runtime_name_at(&self, slot: u64) -> Option<String> {
        if let Some(bound) = self.bin.bound_symbol(slot) {
            let bare = bound.strip_prefix('_').unwrap_or(bound);
            let bare = bare
                .strip_prefix("OBJC_CLASS_$_")
                .or_else(|| bare.strip_prefix("OBJC_METACLASS_$_"))
                .unwrap_or(bare);
            return Some(bare.to_string());
        }
        let target = self.ptr(slot);
        if target == 0 {
            return None;
        }
        self.class_name(target)
    }

    /// The class a slot refers to, as written (Swift names demangled).
    fn class_at(&self, slot: u64) -> Option<String> {
        let name = self.class_runtime_name_at(slot)?;
        Some(swift_name(&name).unwrap_or(name))
    }

    fn name(&mut self, address: u64, name: String, size: u64, code: bool) {
        if address != 0 {
            self.names.push(ObjcName {
                address,
                name,
                size,
                code,
            });
        }
    }

    /// Names a list with an `entsize, count` header, sized by it.
    fn name_list(&mut self, list: u64, name: String) {
        if list == 0 {
            return;
        }
        let (Some(flags), Some(count)) = (self.u32(list), self.u32(list + 4)) else {
            return;
        };
        let size = 8 + (flags & 0xFFFC) as u64 * count.min(1 << 20) as u64;
        self.name(list, name, size, false);
    }

    fn methods(&self, list: u64, class_method: bool) -> Vec<ObjcMethod> {
        let mut out = Vec::new();
        let (Some(flags), Some(count)) = (self.u32(list), self.u32(list + 4)) else {
            return out;
        };
        let entsize = (flags & 0xFFFC) as u64;
        // Relative ("small") method lists hold 32-bit offsets. In the shared
        // cache their selectors are offsets into its strings, which aren't here.
        let small = flags & 0x8000_0000 != 0;
        let direct = flags & 0x4000_0000 != 0;
        if entsize == 0 || count > 1 << 20 {
            return out;
        }
        for i in 0..count as u64 {
            let e = list + 8 + i * entsize;
            let (selector, types, imp) = if small {
                let selector = if direct {
                    None
                } else {
                    self.relative(e).and_then(|r| self.str(self.ptr(r)))
                };
                let types = self.relative(e + 4).and_then(|t| self.str(t));
                let imp = match self.i32(e + 8) {
                    Some(0) | None => 0,
                    Some(d) => (e + 8).wrapping_add_signed(d),
                };
                (selector, types, imp)
            } else {
                let p = self.p;
                (self.str(self.ptr(e)), self.str(self.ptr(e + p)), self.ptr(e + 2 * p))
            };
            out.push(ObjcMethod {
                selector: selector.unwrap_or_else(|| "?".into()),
                types: types.unwrap_or_default(),
                imp,
                class_method,
            });
        }
        out
    }

    fn ivars(&mut self, list: u64, class: &str) -> Vec<ObjcIvar> {
        let mut out = Vec::new();
        let (Some(entsize), Some(count)) = (self.u32(list), self.u32(list + 4)) else {
            return out;
        };
        let p = self.p;
        let entsize = entsize as u64;
        if entsize < 3 * p + 8 || count > 1 << 20 {
            return out;
        }
        for i in 0..count as u64 {
            let e = list + 8 + i * entsize;
            let offset_var = self.ptr(e);
            let name = self.str(self.ptr(e + p)).unwrap_or_else(|| "?".into());
            self.name(offset_var, format!("_OBJC_IVAR_$_{class}.{name}"), 4, false);
            out.push(ObjcIvar {
                offset: (offset_var != 0).then(|| self.u32(offset_var)).flatten(),
                types: self.str(self.ptr(e + 2 * p)).unwrap_or_default(),
                size: self.u32(e + 3 * p + 4).unwrap_or(0),
                name,
            });
        }
        out
    }

    fn properties(&self, list: u64) -> Vec<ObjcProperty> {
        let mut out = Vec::new();
        let (Some(entsize), Some(count)) = (self.u32(list), self.u32(list + 4)) else {
            return out;
        };
        let (p, entsize) = (self.p, entsize as u64);
        if entsize < 2 * p || count > 1 << 20 {
            return out;
        }
        for i in 0..count as u64 {
            let e = list + 8 + i * entsize;
            out.push(ObjcProperty {
                name: self.str(self.ptr(e)).unwrap_or_else(|| "?".into()),
                attributes: self.str(self.ptr(e + p)).unwrap_or_default(),
            });
        }
        out
    }

    /// The protocols of a `protocol_list_t` (a pointer-sized count, then pointers).
    fn protocol_names(&mut self, list: u64, name: String) -> Vec<String> {
        let mut out = Vec::new();
        if list == 0 {
            return out;
        }
        let p = self.p;
        let count = self.bin.read_word(list).unwrap_or(0).min(4096);
        for i in 0..count {
            let proto = self.ptr(list + p + i * p);
            if let Some(n) = self.str(self.ptr(proto + p)) {
                out.push(swift_name(&n).unwrap_or(n));
            }
        }
        self.name(list, name, p + count * p, false);
        out
    }

    fn class(&mut self, address: u64) -> Option<ObjcClass> {
        let p = self.p;
        let (data, swift) = self.data(address);
        let ro = self.ro(data)?;
        if ro.flags & RO_META != 0 {
            return None;
        }
        let meta = self.ptr(address);
        let (meta_data, _) = if meta != 0 { self.data(meta) } else { (0, false) };
        let meta_ro = self.ro(meta_data);
        let raw = ro.name.clone();
        let name = swift_name(&raw).unwrap_or_else(|| raw.clone());
        let mut methods = self.methods(ro.methods, false);
        if let Some(m) = &meta_ro {
            methods.extend(self.methods(m.methods, true));
        }
        let ro_size = self.ro_fields() + 7 * p;
        self.name(address, format!("_OBJC_CLASS_$_{raw}"), 5 * p, false);
        self.name(meta, format!("_OBJC_METACLASS_$_{raw}"), 5 * p, false);
        self.name(data, format!("__OBJC_CLASS_RO_$_{raw}"), ro_size, false);
        self.name_list(ro.methods, format!("__OBJC_$_INSTANCE_METHODS_{raw}"));
        self.name_list(ro.ivars, format!("__OBJC_$_INSTANCE_VARIABLES_{raw}"));
        self.name_list(ro.properties, format!("__OBJC_$_PROP_LIST_{raw}"));
        if let Some(m) = &meta_ro {
            self.name(meta_data, format!("__OBJC_METACLASS_RO_$_{raw}"), ro_size, false);
            self.name_list(m.methods, format!("__OBJC_$_CLASS_METHODS_{raw}"));
            self.name_list(m.properties, format!("__OBJC_$_CLASS_PROP_LIST_{raw}"));
        }
        for m in &methods {
            if m.imp != 0 {
                let n = m.name(&name, None);
                self.name(m.imp, n, 0, true);
            }
        }
        let ivars = self.ivars(ro.ivars, &raw);
        let protocols = self.protocol_names(ro.protocols, format!("__OBJC_CLASS_PROTOCOLS_$_{raw}"));
        Some(ObjcClass {
            address,
            superclass: self.class_at(address + p),
            runtime_name: (name != raw).then_some(raw),
            name,
            swift,
            instance_size: ro.instance_size,
            protocols,
            methods,
            ivars,
            properties: self.properties(ro.properties),
        })
    }

    fn category(&mut self, address: u64) -> Option<ObjcCategory> {
        let p = self.p;
        let name = self.str(self.ptr(address))?;
        // The runtime name, for the metadata's names; the class as written, for the methods'.
        let raw_class = self.class_runtime_name_at(address + p).unwrap_or_else(|| "?".into());
        let class = swift_name(&raw_class).unwrap_or_else(|| raw_class.clone());
        let (instance, class_methods) = (self.ptr(address + 2 * p), self.ptr(address + 3 * p));
        let mut methods = self.methods(instance, false);
        methods.extend(self.methods(class_methods, true));
        let label = format!("{raw_class}_$_{name}");
        self.name(address, format!("__OBJC_$_CATEGORY_{label}"), 6 * p, false);
        self.name_list(instance, format!("__OBJC_$_CATEGORY_INSTANCE_METHODS_{label}"));
        self.name_list(class_methods, format!("__OBJC_$_CATEGORY_CLASS_METHODS_{label}"));
        let properties = self.ptr(address + 5 * p);
        self.name_list(properties, format!("__OBJC_$_PROP_LIST_{label}"));
        for m in &methods {
            if m.imp != 0 {
                let n = m.name(&class, Some(&name));
                self.name(m.imp, n, 0, true);
            }
        }
        let protocols = self.protocol_names(
            self.ptr(address + 4 * p),
            format!("__OBJC_CATEGORY_PROTOCOLS_$_{label}"),
        );
        Some(ObjcCategory {
            address,
            name,
            class,
            protocols,
            methods,
            properties: self.properties(properties),
        })
    }

    fn protocol(&mut self, address: u64) -> Option<ObjcProtocol> {
        let p = self.p;
        let raw = self.str(self.ptr(address + p))?;
        let name = swift_name(&raw).unwrap_or_else(|| raw.clone());
        let lists: Vec<u64> = (3..7).map(|i| self.ptr(address + i * p)).collect();
        let mut required = self.methods(lists[0], false);
        required.extend(self.methods(lists[1], true));
        let mut optional = self.methods(lists[2], false);
        optional.extend(self.methods(lists[3], true));
        let size = self.u32(address + 8 * p).unwrap_or(0) as u64;
        self.name(
            address,
            format!("__OBJC_PROTOCOL_$_{raw}"),
            size.clamp(8 * p + 8, 16 * p),
            false,
        );
        for (list, prefix) in lists.iter().zip([
            "__OBJC_$_PROTOCOL_INSTANCE_METHODS_",
            "__OBJC_$_PROTOCOL_CLASS_METHODS_",
            "__OBJC_$_PROTOCOL_INSTANCE_METHODS_OPT_",
            "__OBJC_$_PROTOCOL_CLASS_METHODS_OPT_",
        ]) {
            self.name_list(*list, format!("{prefix}{raw}"));
        }
        // Extended method types: one string per method, when the protocol has them.
        if size >= 8 * p + 8 + p {
            let types = self.ptr(address + 8 * p + 8);
            let n = (required.len() + optional.len()) as u64;
            self.name(types, format!("__OBJC_$_PROTOCOL_METHOD_TYPES_{raw}"), n * p, false);
        }
        let properties = self.ptr(address + 7 * p);
        self.name_list(properties, format!("__OBJC_$_PROP_LIST_{raw}"));
        let protocols = self.protocol_names(self.ptr(address + 2 * p), format!("__OBJC_$_PROTOCOL_REFS_{raw}"));
        Some(ObjcProtocol {
            address,
            runtime_name: (name != raw).then_some(raw),
            name,
            protocols,
            required,
            optional,
            properties: self.properties(properties),
        })
    }

    /// `objc_msgSend$selector` stubs in `sec`: (address, size, selector reference).
    fn stubs(&self, sec: &Section) -> Vec<(u64, u64, u64)> {
        let Some(bytes) = section_bytes(&self.bin.data, sec) else {
            return Vec::new();
        };
        let mut starts: Vec<(u64, u64)> = Vec::new();
        match self.bin.arch {
            Architecture::Aarch64 | Architecture::Aarch64_Ilp32 => {
                let words: Vec<u32> = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|w| u32::from_le_bytes(*w))
                    .collect();
                for (i, pair) in words.windows(2).enumerate() {
                    // adrp x1, selref@page; ldr x1, [x1, selref@pageoff]
                    let (a, l) = (pair[0], pair[1]);
                    if a & 0x9F00_001F == 0x9000_0001 && l & 0xFFC0_03FF == 0xF940_0021 {
                        let pc = sec.address + 4 * i as u64;
                        let imm = ((((a >> 5) & 0x7FFFF) << 2 | ((a >> 29) & 3)) as i64) << 43 >> 31;
                        let page = (pc & !0xFFF).wrapping_add_signed(imm);
                        starts.push((pc, page + ((l >> 10) & 0xFFF) as u64 * 8));
                    }
                }
            }
            Architecture::X86_64 => {
                // movq selref(%rip), %rsi; jmpq *objc_msgSend@GOT(%rip)
                let mut i = 0;
                while i + 7 <= bytes.len() {
                    if bytes[i..i + 3] == [0x48, 0x8B, 0x35] {
                        let d = i32::from_le_bytes(bytes[i + 3..i + 7].try_into().expect("4 bytes"));
                        let pc = sec.address + i as u64;
                        starts.push((pc, (pc + 7).wrapping_add_signed(d as i64)));
                        i += 7;
                    } else {
                        i += 1;
                    }
                }
            }
            _ => {}
        }
        let end = sec.address + bytes.len() as u64;
        (0..starts.len())
            .map(|i| {
                let next = starts.get(i + 1).map_or(end, |s| s.0);
                (starts[i].0, (next - starts[i].0).min(32), starts[i].1)
            })
            .collect()
    }
}

/// Every pointer-sized slot of the sections named `name`.
fn slots(bin: &Binary, name: &str) -> Vec<u64> {
    let p = if bin.is64 { 8 } else { 4 };
    bin.sections
        .iter()
        .filter(|s| s.loaded && s.name == name && s.file_offset.is_some())
        .flat_map(|s| (0..s.size / p).map(move |i| s.address + i * p))
        .collect()
}

/// Reads the metadata of a linked Mach-O image (executables, libraries,
/// bundles: objects and dSYMs don't hold it resolved).
pub(crate) fn parse(bin: &Binary) -> ObjcInfo {
    use object::macho;
    let b = Bytes::new(&bin.data, bin.endian);
    let file_type = macho::FileType(b.u32(12).unwrap_or(0));
    if bin.summary.format != Format::MachO
        || !matches!(file_type, macho::MH_EXECUTE | macho::MH_DYLIB | macho::MH_BUNDLE)
        || !bin.sections.iter().any(|s| s.name.starts_with("__objc_"))
    {
        return ObjcInfo::default();
    }
    let mut r = Reader {
        bin,
        b,
        scheme: bin.scheme(),
        p: if bin.is64 { 8 } else { 4 },
        names: Vec::new(),
    };
    let p = r.p;
    let mut info = ObjcInfo::default();
    for slot in slots(bin, "__objc_classlist") {
        let class = r.ptr(slot);
        if let Some(c) = r.class(class) {
            info.classes.push(c);
        }
    }
    for list in ["__objc_catlist", "__objc_catlist2"] {
        for slot in slots(bin, list) {
            let cat = r.ptr(slot);
            if let Some(c) = r.category(cat) {
                info.categories.push(c);
            }
        }
    }
    for slot in slots(bin, "__objc_protolist") {
        let proto = r.ptr(slot);
        if let Some(pr) = r.protocol(proto) {
            let raw = pr.runtime_name.clone().unwrap_or_else(|| pr.name.clone());
            r.name(slot, format!("__OBJC_LABEL_PROTOCOL_$_{raw}"), p, false);
            info.protocols.push(pr);
        }
    }
    // Protocols are defined in each image that uses them: list each once.
    info.protocols.sort_by_key(|pr| pr.address);
    info.protocols.dedup_by_key(|pr| pr.address);
    let mut selectors: HashMap<String, ObjcSelector> = HashMap::new();
    for slot in slots(bin, "__objc_selrefs") {
        let Some(name) = r.str(r.ptr(slot)) else { continue };
        r.name(slot, format!("@selector({name})"), p, false);
        selectors
            .entry(name.clone())
            .or_insert_with(|| ObjcSelector {
                name,
                references: Vec::new(),
                stubs: Vec::new(),
            })
            .references
            .push(slot);
    }
    for sec in bin.sections.iter().filter(|s| s.loaded && s.name == "__objc_stubs") {
        for (address, size, selref) in r.stubs(sec) {
            let Some(name) = r.str(r.ptr(selref)) else { continue };
            r.name(address, format!("_objc_msgSend${name}"), size, true);
            selectors
                .entry(name.clone())
                .or_insert_with(|| ObjcSelector {
                    name,
                    references: vec![selref],
                    stubs: Vec::new(),
                })
                .stubs
                .push(address);
        }
    }
    for list in ["__objc_classrefs", "__objc_superrefs"] {
        for slot in slots(bin, list) {
            if let Some(class) = r.class_at(slot) {
                r.name(slot, format!("@class({class})"), p, false);
            }
        }
    }
    for slot in slots(bin, "__objc_protorefs") {
        let proto = r.ptr(slot);
        if let Some(n) = r.str(r.ptr(proto + p)) {
            let n = swift_name(&n).unwrap_or(n);
            r.name(slot, format!("@protocol({n})"), p, false);
        }
    }
    info.selectors = selectors.into_values().collect();
    info.selectors.sort_by(|a, b| a.name.cmp(&b.name));
    // One name per address: the first given (a class's before its methods' lists).
    let mut names = r.names;
    names.sort_by_key(|n| n.address);
    names.dedup_by_key(|n| n.address);
    info.names = names;
    info
}

impl Binary {
    /// The image's Objective-C metadata (empty for images without any).
    pub fn objc(&self) -> &ObjcInfo {
        self.objc.get_or_init(|| parse(self))
    }

    /// Where `selector` is implemented, and which functions send it: those
    /// loading one of its selector references or calling one of its stubs.
    /// Builds the reference index if it isn't yet.
    pub fn objc_selector(&self, selector: &str) -> Option<SelectorUses> {
        let info = self.objc();
        let implementations = info.implementations(selector);
        let sel = info.selectors.iter().find(|s| s.name == selector);
        if sel.is_none() && implementations.is_empty() {
            return None;
        }
        let (references, stubs) = sel.map_or((Vec::new(), Vec::new()), |s| (s.references.clone(), s.stubs.clone()));
        let p = if self.is64 { 8 } else { 4 };
        let mut targets: Vec<(u64, u64, RefKind)> = Vec::new();
        for &r in &references {
            targets.push((r, r + p, RefKind::Read));
            targets.push((r, r + p, RefKind::Address));
        }
        for &s in &stubs {
            targets.push((s, s + 1, RefKind::Call));
            targets.push((s, s + 1, RefKind::Jump));
        }
        // The stubs load the references too: they aren't senders themselves.
        let in_stubs = |site: u64| self.section_at(site).is_some_and(|sec| sec.name == "__objc_stubs");
        Some(SelectorUses {
            selector: selector.to_string(),
            implementations,
            references,
            stubs,
            senders: self.referrers(&targets, &in_stubs),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swift_names() {
        assert_eq!(swift_name("_TtC4Shop8CartView").as_deref(), Some("Shop.CartView"));
        assert_eq!(
            swift_name("_TtCC4Shop5Outer5Inner").as_deref(),
            Some("Shop.Outer.Inner")
        );
        assert_eq!(swift_name("_TtCs12_SwiftObject").as_deref(), Some("Swift._SwiftObject"));
        assert_eq!(swift_name("_TtP4Shop8Delegate_").as_deref(), Some("Shop.Delegate"));
        assert_eq!(swift_name("_TtGC4Shop3BoxSi_"), None);
        assert_eq!(swift_name("$s4Shop4BaseCN").as_deref(), Some("Shop.Base"));
        assert_eq!(swift_name("Greeter"), None);
    }

    #[test]
    fn declarations() {
        assert_eq!(method_declaration("hello", "v16@0:8", false), "- (void)hello;");
        assert_eq!(
            method_declaration("greetWith:times:", "v28@0:8@16i24", false),
            "- (void)greetWith:(id)arg1 times:(int)arg2;"
        );
        assert_eq!(
            method_declaration("setFrame:", "v48@0:8{CGRect={CGPoint=dd}{CGSize=dd}}16", false),
            "- (void)setFrame:(struct CGRect)arg1;"
        );
        assert_eq!(
            method_declaration("objectForKey:", "@24@0:8@\"NSString\"16", true),
            "+ (id)objectForKey:(NSString *)arg1;"
        );
        assert_eq!(
            method_declaration("run:", "v24@0:8@?<v@?B>16", false),
            "- (void)run:(id /* block */)arg1;"
        );
        assert_eq!(
            method_declaration("count", "Q16@0:8", false),
            "- (unsigned long long)count;"
        );
        assert_eq!(
            method_declaration("name", "r^{__CFString=}16@0:8", false),
            "- (const struct __CFString *)name;"
        );
        // Without a signature that fits, the selector alone.
        assert_eq!(method_declaration("a:b:", "v16@0:8", false), "- (void)a:b:;");
        assert_eq!(
            property_declaration("name", "T@\"NSString\",C,N,V_name"),
            "@property (nonatomic, copy) NSString *name;"
        );
        assert_eq!(
            property_declaration("enabled", "TB,R,N,GisEnabled"),
            "@property (nonatomic, readonly, getter=isEnabled) BOOL enabled;"
        );
        assert_eq!(declare("int[4]", "values"), "int values[4]");
        assert_eq!(declare(&read_type("b3").0, "flags"), "unsigned int flags:3");
    }
}
