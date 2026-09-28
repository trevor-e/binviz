//! Byte-level decoding of DWARF sections: unit headers, DIEs and their
//! attributes in .debug_info, abbreviation declarations in .debug_abbrev, and
//! line program headers in .debug_line.

use gimli::UnitOffset;

use super::decode::Entry;
use super::fields::FieldValue;
use super::{Ctx, Node};
use crate::dwarf::die::die_name;
use crate::model::RegionKind;
use crate::util::{self, hex};

fn name_of(n: Option<&'static str>) -> &'static str {
    n.unwrap_or("?")
}

/// ULEB128 at `pos`: (value, length).
fn uleb(ctx: &Ctx, mut pos: u64) -> Option<(u64, u64)> {
    let start = pos;
    let mut value = 0u64;
    let mut shift = 0;
    loop {
        let b = ctx.bytes.u8(pos)?;
        pos += 1;
        if shift < 64 {
            value |= u64::from(b & 0x7f) << shift;
        }
        shift += 7;
        if b & 0x80 == 0 {
            return Some((value, pos - start));
        }
    }
}

fn sleb(ctx: &Ctx, mut pos: u64) -> Option<(i64, u64)> {
    let start = pos;
    let mut value = 0i64;
    let mut shift = 0;
    loop {
        let b = ctx.bytes.u8(pos)?;
        pos += 1;
        if shift < 64 {
            value |= i64::from(b & 0x7f) << shift;
        }
        shift += 7;
        if b & 0x80 == 0 {
            if shift < 64 && b & 0x40 != 0 {
                value |= -1i64 << shift;
            }
            return Some((value, pos - start));
        }
    }
}

fn field(start: u64, end: u64, name: &'static str, raw: u64, value: String) -> FieldValue {
    FieldValue {
        start,
        end,
        name,
        raw,
        value,
    }
}

/// Reads an initial length: (length, bytes used, is DWARF64).
fn initial_length(ctx: &Ctx, pos: u64) -> Option<(u64, u64, bool)> {
    let l = ctx.bytes.u32(pos)?;
    if l == 0xffff_ffff {
        Some((ctx.bytes.u64(pos + 4)?, 12, true))
    } else {
        Some((l as u64, 4, false))
    }
}

// --- .debug_info ---------------------------------------------------------------

pub(crate) fn info_entry(ctx: &Ctx, node: &Node, offset: u64) -> Option<Entry> {
    let debug = ctx.dwarf.filter(|d| d.is_embedded())?;
    let sec_off = offset - node.start;
    let unit_idx = debug.unit_for_info_offset(sec_off)?;
    let unit = debug.unit(unit_idx)?;
    let unit_start = unit.header.offset().0 as u64;
    let header_end = unit_start + unit.header.header_size() as u64;
    let base = node.start + unit_start;
    if sec_off < header_end {
        return Some(unit_header(ctx, unit_idx, &unit, base));
    }
    let starts = debug.die_starts(unit_idx);
    let rel = (sec_off - unit_start) as u32;
    let i = starts.partition_point(|&s| s <= rel).checked_sub(1)?;
    let die_off = starts[i] as u64;
    let end = starts
        .get(i + 1)
        .map_or(unit.header.length_including_self() as u64, |&s| s as u64);
    let mut raw = unit.entries_raw(Some(UnitOffset(die_off as usize))).ok()?;
    let code_start = raw.next_offset().0 as u64;
    let abbrev = raw.read_abbreviation().ok()?;
    let code_end = raw.next_offset().0 as u64;
    let Some(abbrev) = abbrev else {
        return Some(Entry {
            start: base + die_off,
            end: base + end,
            name: "Null entry".into(),
            value: Some("ends a list of sibling DIEs".into()),
            fields: vec![field(
                base + code_start,
                base + code_end,
                "abbrev code",
                0,
                "0 (null)".into(),
            )],
            ..Entry::default()
        });
    };
    let mut fields = vec![field(
        base + code_start,
        base + code_end,
        "abbrev code",
        abbrev.code(),
        format!("{} → {}", abbrev.code(), name_of(abbrev.tag().static_string())),
    )];
    let mut low_pc = None;
    for spec in abbrev.attributes() {
        let s = raw.next_offset().0 as u64;
        let Ok(attr) = raw.read_attribute(*spec) else { break };
        let e = raw.next_offset().0 as u64;
        if attr.name() == gimli::DW_AT_low_pc {
            low_pc = unit.attr_address(attr.value()).ok().flatten();
        }
        let info = debug.format_attr(unit_idx, &unit, &attr, low_pc);
        let value = format!(
            "{}  [{}]",
            info.value.lines().next().unwrap_or(""),
            info.form.trim_start_matches("DW_FORM_")
        );
        fields.push(field(
            base + s,
            base + e,
            name_of(attr.name().static_string()),
            0,
            value,
        ));
    }
    let die = unit.entry(UnitOffset(die_off as usize)).ok();
    let name = die.as_ref().and_then(|d| die_name(&unit, d, false));
    let tag = name_of(abbrev.tag().static_string());
    Some(Entry {
        start: base + die_off,
        end: base + end,
        name: format!(
            "DIE <{:#x}> {}{}",
            unit_start + die_off,
            tag.trim_start_matches("DW_TAG_"),
            name.map(|n| format!(" {n}")).unwrap_or_default()
        ),
        value: Some(tag.to_string()),
        note: Some(format!("unit {unit_idx}, {} attributes", abbrev.attributes().len())),
        fields,
        kind: Some(RegionKind::Debug),
    })
}

fn unit_header(ctx: &Ctx, unit_idx: u32, unit: &gimli::UnitRef<'_, crate::dwarf::R>, base: u64) -> Entry {
    let h = &unit.header;
    let mut pos = base;
    let mut fields = Vec::new();
    if let Some((len, used, dwarf64)) = initial_length(ctx, pos) {
        fields.push(field(
            pos,
            pos + used,
            "unit_length",
            len,
            format!("{} ({})", hex(len), if dwarf64 { "DWARF64" } else { "DWARF32" }),
        ));
        pos += used;
    }
    let word = if h.format() == gimli::Format::Dwarf64 { 8 } else { 4 };
    fields.push(field(
        pos,
        pos + 2,
        "version",
        h.version() as u64,
        h.version().to_string(),
    ));
    pos += 2;
    let abbrev = |pos: u64| {
        field(
            pos,
            pos + word,
            "debug_abbrev_offset",
            h.debug_abbrev_offset().0 as u64,
            hex(h.debug_abbrev_offset().0 as u64),
        )
    };
    if h.version() >= 5 {
        let ut = ctx.bytes.u8(pos).unwrap_or(0);
        fields.push(field(
            pos,
            pos + 1,
            "unit_type",
            ut as u64,
            name_of(gimli::DwUt(ut).static_string()).to_string(),
        ));
        fields.push(field(
            pos + 1,
            pos + 2,
            "address_size",
            h.address_size() as u64,
            h.address_size().to_string(),
        ));
        fields.push(abbrev(pos + 2));
        pos += 2 + word;
    } else {
        fields.push(abbrev(pos));
        fields.push(field(
            pos + word,
            pos + word + 1,
            "address_size",
            h.address_size() as u64,
            h.address_size().to_string(),
        ));
        pos += word + 1;
    }
    match h.type_() {
        gimli::UnitType::Type {
            type_signature,
            type_offset,
        }
        | gimli::UnitType::SplitType {
            type_signature,
            type_offset,
        } => {
            fields.push(field(
                pos,
                pos + 8,
                "type_signature",
                type_signature.0,
                format!("{:#018x}", type_signature.0),
            ));
            fields.push(field(
                pos + 8,
                pos + 8 + word,
                "type_offset",
                type_offset.0 as u64,
                hex(type_offset.0 as u64),
            ));
        }
        gimli::UnitType::Skeleton(id) | gimli::UnitType::SplitCompilation(id) => {
            fields.push(field(pos, pos + 8, "dwo_id", id.0, format!("{:#018x}", id.0)));
        }
        _ => {}
    }
    let name = unit
        .name
        .as_ref()
        .and_then(|n| gimli::Reader::to_string_lossy(n).ok().map(|c| c.into_owned()));
    Entry {
        start: base,
        end: base + h.header_size() as u64,
        name: format!("Unit {unit_idx} header"),
        value: name,
        note: Some(format!(
            "DWARF {} unit, {} bytes",
            h.version(),
            h.length_including_self()
        )),
        fields,
        kind: Some(RegionKind::Debug),
    }
}

/// Entry boundaries in `range` (file offsets) for hex view shading.
pub(crate) fn info_bounds(
    ctx: &Ctx,
    node: &Node,
    range: std::ops::Range<u64>,
) -> Vec<(u64, u64, u64, Option<RegionKind>)> {
    let mut out = Vec::new();
    let Some(debug) = ctx.dwarf.filter(|d| d.is_embedded()) else {
        return out;
    };
    let mut sec_off = range.start - node.start;
    let end = range.end - node.start;
    let mut index = 0;
    while sec_off < end && out.len() < 4096 {
        let Some(unit_idx) = debug.unit_for_info_offset(sec_off) else {
            break;
        };
        let Some(unit) = debug.unit(unit_idx) else { break };
        let unit_start = unit.header.offset().0 as u64;
        let unit_end = unit_start + unit.header.length_including_self() as u64;
        let header_end = unit_start + unit.header.header_size() as u64;
        let base = node.start + unit_start;
        if sec_off < header_end {
            out.push((base, base + unit.header.header_size() as u64, index, None));
            index += 1;
        }
        let starts = debug.die_starts(unit_idx);
        let rel = sec_off.saturating_sub(unit_start) as u32;
        let first = starts.partition_point(|&s| s <= rel).saturating_sub(1);
        for (i, &s) in starts.iter().enumerate().skip(first) {
            if unit_start + s as u64 >= end {
                break;
            }
            let e = starts.get(i + 1).map_or(unit_end - unit_start, |&n| n as u64);
            out.push((base + s as u64, base + e, index, None));
            index += 1;
        }
        sec_off = unit_end;
    }
    out
}

// --- .debug_abbrev -------------------------------------------------------------

pub(crate) fn abbrev_entry(ctx: &Ctx, node: &Node, pos: u64) -> Option<Entry> {
    let (code, n) = uleb(ctx, pos)?;
    let mut fields = vec![field(pos, pos + n, "code", code, code.to_string())];
    if code == 0 {
        return Some(Entry {
            start: pos,
            end: pos + n,
            name: "End of abbreviation table".into(),
            fields,
            ..Entry::default()
        });
    }
    let mut p = pos + n;
    let (tag, n) = uleb(ctx, p)?;
    let tag_name = name_of(gimli::DwTag(tag as u16).static_string());
    fields.push(field(p, p + n, "tag", tag, tag_name.to_string()));
    p += n;
    let children = ctx.bytes.u8(p)?;
    fields.push(field(
        p,
        p + 1,
        "children",
        children as u64,
        if children != 0 {
            "DW_CHILDREN_yes"
        } else {
            "DW_CHILDREN_no"
        }
        .into(),
    ));
    p += 1;
    let mut specs = 0;
    while p < node.end {
        let (at, n1) = uleb(ctx, p)?;
        let (form, n2) = uleb(ctx, p + n1)?;
        let mut len = n1 + n2;
        if at == 0 && form == 0 {
            fields.push(field(p, p + len, "end of attributes", 0, "0, 0".into()));
            p += len;
            break;
        }
        let mut value = format!(
            "{} {}",
            name_of(gimli::DwAt(at as u16).static_string()),
            name_of(gimli::DwForm(form as u16).static_string())
        );
        if form == gimli::DW_FORM_implicit_const.0 as u64 {
            let (c, n3) = sleb(ctx, p + len)?;
            value.push_str(&format!(" = {c}"));
            len += n3;
        }
        fields.push(field(p, p + len, "attribute", at, value));
        p += len;
        specs += 1;
    }
    Some(Entry {
        start: pos,
        end: p,
        name: format!("Abbreviation {code}: {}", tag_name.trim_start_matches("DW_TAG_")),
        value: Some(format!(
            "{specs} attributes{}",
            if children != 0 { ", has children" } else { "" }
        )),
        fields,
        ..Entry::default()
    })
}

// --- .debug_line ---------------------------------------------------------------

pub(crate) fn line_entry(ctx: &Ctx, node: &Node, pos: u64) -> Option<Entry> {
    let (len, used, dwarf64) = initial_length(ctx, pos)?;
    let end = (pos + used + len).min(node.end);
    let word = if dwarf64 { 8 } else { 4 };
    let mut fields = vec![field(
        pos,
        pos + used,
        "unit_length",
        len,
        format!("{} ({})", hex(len), if dwarf64 { "DWARF64" } else { "DWARF32" }),
    )];
    let mut p = pos + used;
    let version = ctx.bytes.u16(p)?;
    fields.push(field(p, p + 2, "version", version as u64, version.to_string()));
    p += 2;
    if version >= 5 {
        fields.push(field(
            p,
            p + 1,
            "address_size",
            ctx.bytes.u8(p)? as u64,
            ctx.bytes.u8(p)?.to_string(),
        ));
        fields.push(field(
            p + 1,
            p + 2,
            "segment_selector_size",
            ctx.bytes.u8(p + 1)? as u64,
            ctx.bytes.u8(p + 1)?.to_string(),
        ));
        p += 2;
    }
    let header_length = ctx.bytes.uint(p, word as u32)?;
    fields.push(field(p, p + word, "header_length", header_length, hex(header_length)));
    p += word;
    let header_end = (p + header_length).min(end);
    let u8f = |p: u64, name: &'static str| {
        field(
            p,
            p + 1,
            name,
            ctx.bytes.u8(p).unwrap_or(0) as u64,
            ctx.bytes.u8(p).unwrap_or(0).to_string(),
        )
    };
    fields.push(u8f(p, "minimum_instruction_length"));
    p += 1;
    if version >= 4 {
        fields.push(u8f(p, "maximum_operations_per_instruction"));
        p += 1;
    }
    fields.push(u8f(p, "default_is_stmt"));
    let line_base = ctx.bytes.u8(p + 1)? as i8;
    fields.push(field(
        p + 1,
        p + 2,
        "line_base",
        line_base as i64 as u64,
        line_base.to_string(),
    ));
    fields.push(u8f(p + 2, "line_range"));
    let opcode_base = ctx.bytes.u8(p + 3)?;
    fields.push(u8f(p + 3, "opcode_base"));
    p += 4;
    let lengths = opcode_base.saturating_sub(1) as u64;
    if lengths > 0 {
        let bytes = ctx.bytes.slice(p, lengths).unwrap_or(&[]);
        fields.push(field(
            p,
            p + lengths,
            "standard_opcode_lengths",
            0,
            format!("{bytes:?}"),
        ));
        p += lengths;
    }
    if p < header_end {
        fields.push(field(
            p,
            header_end,
            if version >= 5 {
                "directory/file entry formats and tables"
            } else {
                "include_directories and file_names"
            },
            0,
            format!("{} bytes", header_end - p),
        ));
    }
    if header_end < end {
        fields.push(field(
            header_end,
            end,
            "line number program",
            0,
            format!("{} bytes of opcodes (state machine instructions)", end - header_end),
        ));
    }
    let unit = ctx.dwarf.and_then(|d| {
        d.units().iter().find(|u| {
            let idx = u.index as usize;
            d.units
                .get(idx)
                .and_then(|x| x.line_program.as_ref())
                .is_some_and(|lp| lp.header().offset().0 as u64 == pos - node.start)
        })
    });
    Some(Entry {
        start: pos,
        end,
        name: format!("Line program @{}", hex(pos - node.start)),
        value: unit.and_then(|u| u.name.clone()).map(|n| util::quote(n.as_bytes())),
        note: Some(format!("DWARF {version} line table header + opcodes")),
        fields,
        ..Entry::default()
    })
}
