//! Checking DWARF for problems: data that can't be read (a unit header, an
//! abbreviation, a DIE, a string, a range or location list, a line program)
//! and data that reads but doesn't add up (references to nothing, ranges that
//! end before they start, files the line table doesn't define).
//!
//! Tools stop at the first problem, or silently skip it; this lists them all,
//! each with the unit, DIE and section offset it was found at.

use gimli::{AttributeValue, Reader as _, UnitOffset};
use serde::Serialize;

use super::die::{is_type_tag, pc_range};
use super::{DebugInfo, R};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// Data that can't be read.
    Error,
    /// Data that reads but is inconsistent.
    Warning,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DwarfProblem {
    pub severity: Severity,
    /// What was being read: unit, DIE, reference, string, ranges, location,
    /// expression, file, address, line program.
    pub area: String,
    pub message: String,
    pub unit: Option<u32>,
    /// The DIE's offset within its unit.
    pub die: Option<u64>,
    /// The DIE's tag, when there is one.
    pub tag: Option<String>,
    /// Where it is: the section and the offset in it.
    pub section: String,
    pub offset: Option<u64>,
}

/// Problem counts for one unit.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitProblems {
    pub unit: u32,
    pub errors: u32,
    pub warnings: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DwarfCheck {
    pub units: u32,
    pub dies: u64,
    pub line_rows: u64,
    pub errors: u32,
    pub warnings: u32,
    pub by_unit: Vec<UnitProblems>,
    /// The first `MAX_PROBLEMS`, errors first.
    pub problems: Vec<DwarfProblem>,
    pub truncated: bool,
}

const MAX_PROBLEMS: usize = 2000;

struct Collector {
    problems: Vec<DwarfProblem>,
    errors: u32,
    warnings: u32,
    by_unit: Vec<(u32, u32)>,
}

impl Collector {
    fn add(&mut self, p: DwarfProblem) {
        match p.severity {
            Severity::Error => self.errors += 1,
            Severity::Warning => self.warnings += 1,
        }
        if let Some(u) = p.unit
            && let Some(c) = self.by_unit.get_mut(u as usize)
        {
            match p.severity {
                Severity::Error => c.0 += 1,
                Severity::Warning => c.1 += 1,
            }
        }
        if self.problems.len() < MAX_PROBLEMS * 4 {
            self.problems.push(p);
        }
    }
}

fn attr_name(name: gimli::DwAt) -> String {
    name.static_string()
        .map_or_else(|| format!("DW_AT_{:#x}", name.0), str::to_string)
}

fn tag_name(tag: gimli::DwTag) -> String {
    tag.static_string()
        .map_or_else(|| format!("DW_TAG_{:#x}", tag.0), str::to_string)
}

/// A problem found while loading: a unit that couldn't be read at all.
pub(crate) fn load_problem(section: &str, offset: u64, message: String) -> DwarfProblem {
    DwarfProblem {
        severity: Severity::Error,
        area: "unit".into(),
        message,
        unit: None,
        die: None,
        tag: None,
        section: section.into(),
        offset: Some(offset),
    }
}

impl DebugInfo {
    /// Problems found while loading (units that couldn't be read at all).
    pub fn load_problems(&self) -> &[DwarfProblem] {
        &self.load_problems
    }

    /// Reads every unit, DIE, attribute and line program, and lists what
    /// can't be read or doesn't add up.
    pub fn check(&self) -> DwarfCheck {
        let mut c = Collector {
            problems: self.load_problems.clone(),
            errors: self.load_problems.len() as u32,
            warnings: 0,
            by_unit: vec![(0, 0); self.units.len()],
        };
        let mut dies = 0u64;
        let mut line_rows = 0u64;
        for ui in 0..self.units.len() as u32 {
            dies += self.check_unit(ui, &mut c);
            line_rows += self.check_lines(ui, &mut c);
        }
        let mut problems = c.problems;
        problems.sort_by_key(|p| (p.severity, p.unit, p.offset));
        let truncated = problems.len() > MAX_PROBLEMS;
        problems.truncate(MAX_PROBLEMS);
        DwarfCheck {
            units: self.units.len() as u32,
            dies,
            line_rows,
            errors: c.errors,
            warnings: c.warnings,
            by_unit: c
                .by_unit
                .iter()
                .enumerate()
                .filter(|(_, (e, w))| e + w > 0)
                .map(|(u, &(errors, warnings))| UnitProblems {
                    unit: u as u32,
                    errors,
                    warnings,
                })
                .collect(),
            problems,
            truncated,
        }
    }

    fn check_unit(&self, ui: u32, c: &mut Collector) -> u64 {
        let Some(unit) = self.unit(ui) else { return 0 };
        let section = unit.header.section().name().to_string();
        let section_offset = |o: UnitOffset| o.to_unit_section_offset(&unit.header).0 as u64;
        // Every entry's offset, to tell references to a DIE from references into one.
        let mut starts: Vec<usize> = Vec::new();
        if let Ok(mut raw) = unit.entries_raw(None) {
            while !raw.is_empty() {
                starts.push(raw.next_offset().0);
                match raw.read_abbreviation() {
                    Ok(Some(abbrev)) => {
                        if raw.skip_attributes(abbrev.attributes()).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
        }
        let files = self.files.unit_files_len(ui);
        let version = unit.header.version();
        let mut count = 0u64;
        let mut cursor = unit.entries();
        loop {
            let die = match cursor.next_dfs() {
                Ok(Some(die)) => die,
                Ok(None) => break,
                Err(e) => {
                    let at = cursor.next_offset();
                    c.add(DwarfProblem {
                        severity: Severity::Error,
                        area: "DIE".into(),
                        message: format!(
                            "can't read the entry at unit offset {:#x}: {e}; the rest of the unit is unreadable",
                            at.0
                        ),
                        unit: Some(ui),
                        die: Some(at.0 as u64),
                        tag: None,
                        section: section.clone(),
                        offset: Some(section_offset(at)),
                    });
                    break;
                }
            };
            count += 1;
            let tag = die.tag();
            let problem = |severity: Severity, area: &str, message: String| DwarfProblem {
                severity,
                area: area.into(),
                message,
                unit: Some(ui),
                die: Some(die.offset().0 as u64),
                tag: Some(tag_name(tag)),
                section: section.clone(),
                offset: Some(section_offset(die.offset())),
            };
            for attr in die.attrs() {
                let name = attr_name(attr.name());
                match attr.value() {
                    AttributeValue::UnitRef(o) => {
                        if starts.binary_search(&o.0).is_err() {
                            c.add(problem(
                                Severity::Error,
                                "reference",
                                format!(
                                    "{name} points to unit offset {:#x}, where no DIE starts (the unit is {:#x} bytes)",
                                    o.0,
                                    unit.header.length_including_self()
                                ),
                            ));
                        } else if attr.name() == gimli::DW_AT_type
                            && let Ok(target) = unit.entry(o)
                            && !is_type_tag(target.tag())
                        {
                            c.add(problem(
                                Severity::Warning,
                                "reference",
                                format!("{name} points to a {}, not a type", tag_name(target.tag())),
                            ));
                        }
                    }
                    v @ (AttributeValue::DebugInfoRef(_) | AttributeValue::DebugTypesRef(_)) => {
                        let resolved = self.resolve_ref(ui, v.clone());
                        match resolved {
                            None => c.add(problem(
                                Severity::Error,
                                "reference",
                                format!("{name} ({v:?}) doesn't resolve to any unit"),
                            )),
                            Some((u, o)) => {
                                if self.unit(u).and_then(|tu| tu.entry(o).ok()).is_none() {
                                    c.add(problem(
                                        Severity::Error,
                                        "reference",
                                        format!(
                                            "{name} points to unit {u} offset {:#x}, which isn't a readable DIE",
                                            o.0
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                    v @ (AttributeValue::DebugStrRef(_)
                    | AttributeValue::DebugStrRefSup(_)
                    | AttributeValue::DebugStrOffsetsIndex(_)
                    | AttributeValue::DebugLineStrRef(_)) => {
                        if let Err(e) = unit.attr_string(v.clone()) {
                            c.add(problem(Severity::Error, "string", format!("{name} ({v:?}): {e}")));
                        }
                    }
                    AttributeValue::DebugAddrIndex(i) => {
                        if let Err(e) = unit.address(i) {
                            c.add(problem(
                                Severity::Error,
                                "address",
                                format!("{name}: address index {}: {e}", i.0),
                            ));
                        }
                    }
                    v @ (AttributeValue::RangeListsRef(_) | AttributeValue::DebugRngListsIndex(_)) => {
                        match unit.attr_ranges(v) {
                            Ok(Some(mut iter)) => {
                                let mut backwards = 0;
                                let mut n = 0;
                                loop {
                                    match iter.next() {
                                        Ok(Some(r)) => {
                                            n += 1;
                                            if r.end < r.begin {
                                                backwards += 1;
                                            }
                                            if n > 100_000 {
                                                break;
                                            }
                                        }
                                        Ok(None) => break,
                                        Err(e) => {
                                            c.add(problem(Severity::Error, "ranges", format!("{name}: {e}")));
                                            break;
                                        }
                                    }
                                }
                                if backwards > 0 {
                                    c.add(problem(
                                        Severity::Warning,
                                        "ranges",
                                        format!("{name}: {backwards} range(s) end before they start"),
                                    ));
                                }
                            }
                            Ok(None) => {}
                            Err(e) => c.add(problem(Severity::Error, "ranges", format!("{name}: {e}"))),
                        }
                    }
                    v @ (AttributeValue::LocationListsRef(_) | AttributeValue::DebugLocListsIndex(_)) => {
                        match unit.attr_locations(v) {
                            Ok(Some(mut iter)) => {
                                let mut n = 0;
                                loop {
                                    match iter.next() {
                                        Ok(Some(entry)) => {
                                            n += 1;
                                            if entry.range.end < entry.range.begin {
                                                c.add(problem(
                                                    Severity::Warning,
                                                    "location",
                                                    format!(
                                                        "{name}: an entry ends before it starts ({:#x}..{:#x})",
                                                        entry.range.begin, entry.range.end
                                                    ),
                                                ));
                                            }
                                            if let Err(e) = check_expression(&unit, entry.data) {
                                                c.add(problem(Severity::Warning, "expression", format!("{name}: {e}")));
                                            }
                                            if n > 10_000 {
                                                break;
                                            }
                                        }
                                        Ok(None) => break,
                                        Err(e) => {
                                            c.add(problem(Severity::Error, "location", format!("{name}: {e}")));
                                            break;
                                        }
                                    }
                                }
                            }
                            Ok(None) => {}
                            Err(e) => c.add(problem(Severity::Error, "location", format!("{name}: {e}"))),
                        }
                    }
                    AttributeValue::Exprloc(e) => {
                        if let Err(err) = check_expression(&unit, e) {
                            c.add(problem(Severity::Warning, "expression", format!("{name}: {err}")));
                        }
                    }
                    AttributeValue::FileIndex(i) => {
                        // Before DWARF 5, file 0 means "no file".
                        let valid = (version < 5 && i == 0) || (i as usize) < files;
                        if !valid {
                            c.add(problem(
                                Severity::Warning,
                                "file",
                                format!(
                                    "{name} is file {i}, but the unit's line table defines {} file(s)",
                                    files.saturating_sub(if version < 5 { 1 } else { 0 })
                                ),
                            ));
                        }
                    }
                    _ => {}
                }
            }
            if let (Some(low), Some(high)) = pc_range(&unit, die)
                && high < low
            {
                c.add(problem(
                    Severity::Error,
                    "ranges",
                    format!("DW_AT_high_pc {high:#x} is below DW_AT_low_pc {low:#x}"),
                ));
            }
        }
        count
    }

    fn check_lines(&self, ui: u32, c: &mut Collector) -> u64 {
        let Some(program) = self.units.get(ui as usize).and_then(|u| u.line_program.as_ref()) else {
            return 0;
        };
        let header = program.header();
        let offset = header.offset().0 as u64;
        let first = if header.version() >= 5 { 0 } else { 1 };
        let defined = header.file_names().len() as u64 + first;
        let problem = |severity: Severity, message: String| DwarfProblem {
            severity,
            area: "line program".into(),
            message,
            unit: Some(ui),
            die: None,
            tag: None,
            section: ".debug_line".into(),
            offset: Some(offset),
        };
        let mut rows = program.clone().rows();
        let mut count = 0u64;
        let mut bad_files = 0u64;
        let mut first_bad_file = 0u64;
        let mut backwards = 0u64;
        let mut last: Option<u64> = None;
        loop {
            match rows.next_row() {
                Ok(Some((_, row))) => {
                    count += 1;
                    let f = row.file_index();
                    if (f < first || f >= defined) && !row.end_sequence() {
                        if bad_files == 0 {
                            first_bad_file = f;
                        }
                        bad_files += 1;
                    }
                    if let Some(prev) = last
                        && row.address() < prev
                    {
                        backwards += 1;
                    }
                    last = if row.end_sequence() { None } else { Some(row.address()) };
                }
                Ok(None) => break,
                Err(e) => {
                    c.add(problem(
                        Severity::Error,
                        format!("can't read row {count} of the line program: {e}"),
                    ));
                    break;
                }
            }
        }
        if last.is_some() {
            c.add(problem(
                Severity::Warning,
                "the last sequence isn't ended by DW_LNE_end_sequence".into(),
            ));
        }
        if bad_files > 0 {
            c.add(problem(
                Severity::Warning,
                format!(
                    "{bad_files} row(s) refer to files the header doesn't define (the first: file {first_bad_file}; {} defined)",
                    defined - first
                ),
            ));
        }
        if backwards > 0 {
            c.add(problem(
                Severity::Warning,
                format!("{backwards} row(s) go back in address within a sequence"),
            ));
        }
        count
    }
}

/// Decodes every operation of an expression.
fn check_expression(unit: &gimli::UnitRef<'_, R>, expr: gimli::Expression<R>) -> Result<(), String> {
    let len = expr.0.len();
    let mut ops = expr.operations(unit.encoding());
    let mut n = 0;
    loop {
        match ops.next() {
            Ok(Some(_)) => {
                n += 1;
                if n > 10_000 {
                    return Ok(());
                }
            }
            Ok(None) => return Ok(()),
            Err(e) => {
                return Err(format!(
                    "can't decode operation {} of a {len}-byte expression: {e}",
                    n + 1
                ));
            }
        }
    }
}
