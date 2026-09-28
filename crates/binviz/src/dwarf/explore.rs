//! Finding your way around the DIEs: a unit's DIEs by kind, the DIE at a
//! section offset (to follow `llvm-dwarfdump` output or a crash report), name
//! search, and the variables in scope at an address.

use std::collections::HashMap;

use gimli::{AttributeValue, Reader as _, UnitOffset};
use serde::Serialize;

use super::die::{DieSummary, die_name, is_type_tag, tag_name};
use super::{DebugInfo, R, expr};
use crate::model::SourceLoc;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    pub tag: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiePage {
    pub total: u32,
    pub offset: u32,
    pub dies: Vec<DieSummary>,
}

/// A variable or parameter in scope at an address.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeVar {
    pub name: String,
    /// "parameter", "variable" or "constant".
    pub kind: String,
    pub type_name: Option<String>,
    /// Where the value is at this address (`DW_OP_fbreg -24`, `DW_OP_reg5 (rdi)`),
    /// its constant value, or why there is none.
    pub location: String,
    pub decl: Option<SourceLoc>,
    pub unit: u32,
    pub die: u64,
    /// Which of the scopes (outermost first) declares it.
    pub scope: u32,
}

/// What DWARF says about an address: the scopes around it, outermost first
/// (the function, inlined calls, blocks), and the variables they declare.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeInfo {
    pub address: u64,
    pub unit: u32,
    pub scopes: Vec<DieSummary>,
    pub variables: Vec<ScopeVar>,
}

/// A cached listing: which unit and filter it is for, and the matching DIEs.
pub(crate) struct Listing {
    unit: u32,
    key: String,
    /// (DIE offset, index into `scopes`).
    entries: Vec<(u32, u32)>,
    scopes: Vec<String>,
}

/// Whether `tag` is one of `tokens`: tag names (`DW_TAG_member` or `member`)
/// or kinds (`functions`, `variables`, `types`, `scopes`).
fn tag_matches(tokens: &[String], tag: gimli::DwTag) -> bool {
    tokens.is_empty()
        || tokens.iter().any(|t| match t.as_str() {
            "functions" => matches!(
                tag,
                gimli::DW_TAG_subprogram | gimli::DW_TAG_inlined_subroutine | gimli::DW_TAG_entry_point
            ),
            "variables" => matches!(
                tag,
                gimli::DW_TAG_variable | gimli::DW_TAG_formal_parameter | gimli::DW_TAG_constant
            ),
            "types" => is_type_tag(tag),
            "scopes" => matches!(
                tag,
                gimli::DW_TAG_namespace | gimli::DW_TAG_module | gimli::DW_TAG_lexical_block
            ),
            t => {
                let name = tag_name(tag).to_ascii_lowercase();
                name == t || name.strip_prefix("dw_tag_") == Some(t.strip_prefix("dw_tag_").unwrap_or(t))
            }
        })
}

/// A demangled name without its parameter list (and qualifiers after it).
fn without_params(name: &str) -> &str {
    let b = name.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < b.len() {
        // An operator's own characters: `operator()`, `operator<<=`.
        if b[..i].ends_with(b"operator") {
            if b[i..].starts_with(b"()") {
                i += 2;
                continue;
            }
            let start = i;
            while i < b.len() && b"<>=!+-*/%&|^~[],".contains(&b[i]) {
                i += 1;
            }
            if i > start {
                continue;
            }
        }
        match b[i] {
            b'<' => depth += 1,
            b'>' => depth -= 1,
            b'(' if depth == 0 && i > 0 => return &name[..i],
            _ => {}
        }
        i += 1;
    }
    name
}

/// The part of a qualified name before its last `::`, outside template
/// arguments and parameter lists.
fn split_scope(name: &str) -> Option<(&str, &str)> {
    let b = name.as_bytes();
    let mut depth = 0i32;
    let mut cut = None;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'<' | b'(' => depth += 1,
            b'>' | b')' => depth -= 1,
            b':' if depth == 0 && b.get(i + 1) == Some(&b':') => {
                cut = Some(i);
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let c = cut?;
    Some((&name[..c], &name[c + 2..]))
}

type Die = gimli::DebuggingInformationEntry<R>;

/// An attribute of a DIE, or of the DIE it is an instance of
/// (`DW_AT_abstract_origin`, `DW_AT_specification`).
fn attr_through_origin(unit: &gimli::UnitRef<'_, R>, die: &Die, name: gimli::DwAt) -> Option<AttributeValue<R>> {
    let mut current = die.clone();
    for _ in 0..4 {
        if let Some(v) = current.attr_value(name) {
            return Some(v);
        }
        match current
            .attr_value(gimli::DW_AT_abstract_origin)
            .or_else(|| current.attr_value(gimli::DW_AT_specification))
        {
            Some(AttributeValue::UnitRef(o)) => current = unit.entry(o).ok()?,
            _ => return None,
        }
    }
    None
}

impl DebugInfo {
    /// How many DIEs of each tag a unit has, the commonest first.
    pub fn tag_counts(&self, unit_idx: u32) -> Vec<TagCount> {
        let Some(unit) = self.unit(unit_idx) else {
            return Vec::new();
        };
        let mut counts: HashMap<u16, u32> = HashMap::new();
        let mut cursor = unit.entries();
        while let Ok(Some(die)) = cursor.next_dfs() {
            *counts.entry(die.tag().0).or_default() += 1;
        }
        let mut out: Vec<TagCount> = counts
            .into_iter()
            .map(|(t, count)| TagCount {
                tag: tag_name(gimli::DwTag(t)),
                count,
            })
            .collect();
        out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
        out
    }

    /// A page of a unit's DIEs, flattened: those whose tag matches `filter`
    /// (comma-separated tag names, or `functions`, `variables`, `types`,
    /// `scopes`; empty for all) and whose name contains `name`. Each comes
    /// with its enclosing named scopes.
    pub fn list_dies(&self, unit_idx: u32, filter: &str, name: &str, offset: u32, limit: u32) -> DiePage {
        let key = format!("{}\u{0}{}", filter.to_ascii_lowercase(), name.to_ascii_lowercase());
        let mut cache = self.listing.lock().unwrap_or_else(|p| p.into_inner());
        if !cache.as_ref().is_some_and(|l| l.unit == unit_idx && l.key == key) {
            *cache = Some(self.make_listing(unit_idx, filter, name, key));
        }
        let listing = cache.as_ref().expect("filled");
        let Some(unit) = self.unit(unit_idx) else {
            return DiePage {
                total: 0,
                offset,
                dies: Vec::new(),
            };
        };
        let dies = listing
            .entries
            .iter()
            .skip(offset as usize)
            .take(limit as usize)
            .filter_map(|&(o, s)| {
                let die = unit.entry(UnitOffset(o as usize)).ok()?;
                let mut summary = self.summary_of(unit_idx, &unit, &die);
                let scope = &listing.scopes[s as usize];
                summary.scope = (!scope.is_empty()).then(|| scope.clone());
                Some(summary)
            })
            .collect();
        DiePage {
            total: listing.entries.len() as u32,
            offset,
            dies,
        }
    }

    fn make_listing(&self, unit_idx: u32, filter: &str, name: &str, key: String) -> Listing {
        let tokens: Vec<String> = filter
            .split(',')
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        let needle = name.trim().as_bytes().to_vec();
        let mut listing = Listing {
            unit: unit_idx,
            key,
            entries: Vec::new(),
            scopes: vec![String::new()],
        };
        let Some(unit) = self.unit(unit_idx) else {
            return listing;
        };
        let mut interned: HashMap<String, u32> = HashMap::from([(String::new(), 0)]);
        // Enclosing named scopes: (depth, name).
        let mut stack: Vec<(isize, String)> = Vec::new();
        let mut cursor = unit.entries();
        while let Ok(Some(die)) = cursor.next_dfs() {
            let depth = die.depth();
            while stack.last().is_some_and(|&(d, _)| d >= depth) {
                stack.pop();
            }
            let tag = die.tag();
            let own = die.attr_value(gimli::DW_AT_name).and_then(|v| unit.attr_string(v).ok());
            if tag_matches(&tokens, tag) {
                // Inlined calls and out-of-line definitions are named by the DIE they instantiate.
                let named = needle.is_empty()
                    || match &own {
                        Some(s) => s
                            .to_slice()
                            .is_ok_and(|b| crate::search::find_ci(&b, &needle, 0).is_some()),
                        None => die_name(&unit, die, false)
                            .is_some_and(|n| crate::search::find_ci(n.as_bytes(), &needle, 0).is_some()),
                    };
                if named {
                    let scope = stack.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join("::");
                    let next = interned.len() as u32;
                    let id = *interned.entry(scope.clone()).or_insert_with(|| {
                        listing.scopes.push(scope);
                        next
                    });
                    listing.entries.push((die.offset().0 as u32, id));
                    if listing.entries.len() >= 5_000_000 {
                        break;
                    }
                }
            }
            let names_scope = matches!(
                tag,
                gimli::DW_TAG_namespace
                    | gimli::DW_TAG_module
                    | gimli::DW_TAG_structure_type
                    | gimli::DW_TAG_class_type
                    | gimli::DW_TAG_union_type
                    | gimli::DW_TAG_enumeration_type
                    | gimli::DW_TAG_subprogram
                    | gimli::DW_TAG_inlined_subroutine
            );
            if names_scope && die.has_children() {
                let label = die_name(&unit, die, false).unwrap_or_else(|| "{anonymous}".into());
                stack.push((depth, label));
            }
        }
        listing
    }

    /// The DIE at a `.debug_info` offset, or the one containing it: follows
    /// `<0x…>` references in `llvm-dwarfdump` output and error messages.
    pub fn die_at_offset(&self, offset: u64) -> Option<(u32, u64)> {
        let ui = self.unit_for_info_offset(offset)?;
        let unit = self.unit(ui)?;
        let within = offset - unit.header.offset().0 as u64;
        let root = unit.header.root_offset().0 as u64;
        let mut best = root;
        let mut cursor = unit.entries();
        while let Ok(Some(die)) = cursor.next_dfs() {
            let o = die.offset().0 as u64;
            if o > within {
                break;
            }
            best = o;
        }
        Some((ui, best))
    }

    /// Named DIEs whose qualified name matches `query` (functions with code,
    /// static variables, types, members, enumerators), best matches first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<DieSummary> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let index = self.name_index();
        let mut hits: Vec<(i32, usize)> = index
            .iter()
            .enumerate()
            .filter_map(|(i, d)| crate::search::score(without_params(&d.name), &needle).map(|s| (s, i)))
            .collect();
        hits.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| index[a.1].name.len().cmp(&index[b.1].name.len()))
        });
        hits.truncate(limit);
        hits.into_iter()
            .filter_map(|(_, i)| {
                let d = &index[i];
                let unit = self.unit(d.unit)?;
                let die = unit.entry(UnitOffset(d.offset as usize)).ok()?;
                let mut s = self.summary_of(d.unit, &unit, &die);
                s.scope = split_scope(&d.name).map(|(scope, _)| scope.to_string());
                Some(s)
            })
            .collect()
    }

    /// Every DIE with a name containing `query`, locals and parameters
    /// included: slower than [`Self::search`], it reads all of them.
    pub fn search_all(&self, query: &str, limit: usize) -> Vec<DieSummary> {
        let needle = query.trim().as_bytes().to_vec();
        let mut out = Vec::new();
        if needle.is_empty() {
            return out;
        }
        for ui in 0..self.units.len() as u32 {
            let Some(unit) = self.unit(ui) else { continue };
            let mut cursor = unit.entries();
            while let Ok(Some(die)) = cursor.next_dfs() {
                let Some(v) = die.attr_value(gimli::DW_AT_name) else {
                    continue;
                };
                let Ok(s) = unit.attr_string(v) else { continue };
                if s.to_slice()
                    .is_ok_and(|b| crate::search::find_ci(&b, &needle, 0).is_some())
                {
                    out.push(self.summary_of(ui, &unit, die));
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
        }
        out
    }

    /// The scopes around `address` (function, inlined calls, lexical blocks),
    /// and the parameters and variables they declare, with where each one's
    /// value lives at that address.
    pub fn scope_at(&self, address: u64) -> Option<ScopeInfo> {
        let ui = self.unit_at(address)?;
        let unit = self.unit(ui)?;
        let covers = |die: &Die| -> (bool, bool) {
            let mut has = false;
            if let Ok(mut iter) = unit.die_ranges(die) {
                while let Ok(Some(r)) = iter.next() {
                    has = true;
                    if address >= r.begin && address < r.end {
                        return (true, true);
                    }
                }
            }
            (false, has)
        };
        // Scopes covering the address form one path down the tree.
        let mut path: Vec<(isize, UnitOffset)> = Vec::new();
        let mut open: Vec<isize> = Vec::new();
        let mut vars: Vec<(UnitOffset, u32)> = Vec::new();
        let mut skip_below: Option<isize> = None;
        let mut cursor = unit.entries();
        while let Ok(Some(die)) = cursor.next_dfs() {
            let depth = die.depth();
            if let Some(d) = skip_below {
                if depth > d {
                    continue;
                }
                skip_below = None;
            }
            while open.last().is_some_and(|&d| d >= depth) {
                open.pop();
            }
            match die.tag() {
                gimli::DW_TAG_subprogram | gimli::DW_TAG_inlined_subroutine | gimli::DW_TAG_lexical_block => {
                    let (covered, _) = covers(die);
                    if covered {
                        path.push((depth, die.offset()));
                        open.push(depth);
                    } else if die.has_children() {
                        skip_below = Some(depth);
                    }
                }
                gimli::DW_TAG_variable | gimli::DW_TAG_formal_parameter | gimli::DW_TAG_constant => {
                    // Declared directly in the innermost open scope.
                    if open.last() == Some(&(depth - 1))
                        && let Some(level) = path.iter().position(|&(d, _)| d == depth - 1)
                    {
                        vars.push((die.offset(), level as u32));
                    }
                }
                _ => {}
            }
        }
        if path.is_empty() {
            return None;
        }
        let scopes = path
            .iter()
            .filter_map(|&(_, o)| unit.entry(o).ok().map(|d| self.summary_of(ui, &unit, &d)))
            .collect();
        let variables = vars
            .into_iter()
            .filter_map(|(o, scope)| {
                let die = unit.entry(o).ok()?;
                Some(self.scope_var(ui, &unit, &die, address, scope))
            })
            .collect();
        Some(ScopeInfo {
            address,
            unit: ui,
            scopes,
            variables,
        })
    }

    fn scope_var(&self, ui: u32, unit: &gimli::UnitRef<'_, R>, die: &Die, address: u64, scope: u32) -> ScopeVar {
        let kind = match die.tag() {
            gimli::DW_TAG_formal_parameter => "parameter",
            gimli::DW_TAG_constant => "constant",
            _ => "variable",
        };
        let type_name = attr_through_origin(unit, die, gimli::DW_AT_type)
            .and_then(|v| self.resolve_ref(ui, v))
            .map(|(u, o)| self.type_name(u, o, 0));
        let location = match die.attr_value(gimli::DW_AT_location) {
            Some(AttributeValue::Exprloc(e)) => expr::format(&e, unit.encoding(), self.arch),
            Some(v @ (AttributeValue::LocationListsRef(_) | AttributeValue::DebugLocListsIndex(_))) => {
                let mut found = None;
                if let Ok(Some(mut iter)) = unit.attr_locations(v) {
                    while let Ok(Some(entry)) = iter.next() {
                        if address >= entry.range.begin && address < entry.range.end {
                            found = Some(expr::format(&entry.data, unit.encoding(), self.arch));
                            break;
                        }
                    }
                }
                found.unwrap_or_else(|| "not available at this address (optimized out)".into())
            }
            _ => match die.attr_value(gimli::DW_AT_const_value) {
                Some(AttributeValue::Sdata(s)) => format!("constant {s}"),
                Some(v) => match v.udata_value() {
                    Some(u) => format!("constant {u} ({u:#x})"),
                    None => "constant".into(),
                },
                None => "no location (optimized out)".into(),
            },
        };
        let decl = match (
            attr_through_origin(unit, die, gimli::DW_AT_decl_file),
            attr_through_origin(unit, die, gimli::DW_AT_decl_line),
        ) {
            (Some(AttributeValue::FileIndex(f)), line) => self.files.unit_file(ui, f).map(|file| SourceLoc {
                file,
                path: self.files.files[file as usize].path.clone(),
                line: line.and_then(|l| l.udata_value()).unwrap_or(0) as u32,
                column: 0,
            }),
            _ => None,
        };
        ScopeVar {
            name: die_name(unit, die, false).unwrap_or_else(|| "(unnamed)".into()),
            kind: kind.into(),
            type_name,
            location,
            decl,
            unit: ui,
            die: die.offset().0 as u64,
            scope,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_split_outside_templates() {
        assert_eq!(split_scope("geo::Rect::area"), Some(("geo::Rect", "area")));
        assert_eq!(
            split_scope("std::vector<a::b>::push_back"),
            Some(("std::vector<a::b>", "push_back"))
        );
        assert_eq!(split_scope("main"), None);
        assert_eq!(without_params("total_area(std::vector<a::b> const&)"), "total_area");
        assert_eq!(without_params("operator()(int)"), "operator()");
        assert_eq!(without_params("a::operator<(a const&)"), "a::operator<");
        assert_eq!(without_params("f<int>(x)"), "f<int>");
        assert_eq!(split_scope("f(a::b)"), None);
    }

    #[test]
    fn tag_filters() {
        let t = |s: &str| vec![s.to_string()];
        assert!(tag_matches(&t("functions"), gimli::DW_TAG_subprogram));
        assert!(tag_matches(&t("member"), gimli::DW_TAG_member));
        assert!(tag_matches(&t("dw_tag_member"), gimli::DW_TAG_member));
        assert!(!tag_matches(&t("types"), gimli::DW_TAG_variable));
        assert!(tag_matches(&[], gimli::DW_TAG_variable));
    }
}
