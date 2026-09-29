//! The file layout: a tree of labelled byte ranges covering the whole file.
//!
//! Format-specific builders (ELF, Mach-O, PE) describe what they know: headers
//! with their fields, tables, sections and so on. Regions are nested by
//! containment, and bytes nobody claimed are classified as padding or unknown.
//! Large tables (symbols, relocations, strings) are not expanded eagerly; they
//! carry a [`Decoder`] that produces entries on demand for the byte being looked at.

pub(crate) mod decode;
pub(crate) mod dwarf;
pub(crate) mod elf;
pub(crate) mod fields;
pub(crate) mod macho;
pub(crate) mod pe;
pub(crate) mod rich;
pub(crate) mod xbe;

use std::cmp::Reverse;
use std::ops::Range;

use crate::model::{PathEntry, RegionInfo, RegionKind, Section, Span};
use crate::symbols::SymbolTable;
use crate::util::Bytes;
pub(crate) use decode::Decoder;
use fields::{F, FieldValue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Machine {
    Elf(u16),
    MachO(u32),
    Pe(u16),
    Other,
}

/// Context needed to decode and pretty-print structures.
#[derive(Clone, Copy)]
pub(crate) struct Ctx<'a> {
    pub bytes: Bytes<'a>,
    pub machine: Machine,
    pub is64: bool,
    /// String table used by `Fmt::StrIndex` fields: (file offset, size).
    pub strtab: Option<(u64, u64)>,
    pub image_base: u64,
    pub sections: &'a [Section],
    pub symbols: Option<&'a SymbolTable>,
    /// DWARF, for byte-level decoding of the debug sections.
    pub dwarf: Option<&'a crate::dwarf::DebugInfo>,
}

impl<'a> Ctx<'a> {
    pub fn with_strtab(&self, strtab: Option<(u64, u64)>) -> Ctx<'a> {
        Ctx { strtab, ..*self }
    }

    pub fn string_at(&self, index: u64) -> Option<&'a [u8]> {
        let (offset, size) = self.strtab?;
        if index >= size {
            return None;
        }
        self.bytes.cstr(offset + index, size - index)
    }

    pub fn addr_size(&self) -> u32 {
        if self.is64 { 8 } else { 4 }
    }

    /// File offset of a virtual address, using the section list.
    pub fn va_to_offset(&self, va: u64) -> Option<u64> {
        for s in self.sections {
            if let Some(off) = s.file_offset
                && va >= s.address
                && va - s.address < s.file_size
                && s.loaded
            {
                return Some(off + (va - s.address));
            }
        }
        None
    }

    /// File offset of a PE relative virtual address. Addresses below the first
    /// section map to the headers, which are loaded at the image base unchanged.
    pub fn rva_to_offset(&self, rva: u64) -> Option<u64> {
        if let Some(off) = self.va_to_offset(self.image_base.wrapping_add(rva)) {
            return Some(off);
        }
        let first = self
            .sections
            .iter()
            .filter(|s| s.loaded && s.address >= self.image_base)
            .map(|s| s.address - self.image_base)
            .min()?;
        (rva < first && rva < self.bytes.data.len() as u64).then_some(rva)
    }

    /// NUL-terminated string at an RVA.
    pub fn cstr_at_rva(&self, rva: u64) -> Option<&'a [u8]> {
        let off = self.rva_to_offset(rva)?;
        self.bytes.cstr(off, 4096)
    }

    /// Symbol name (+offset) for an address, for annotating pointers.
    pub fn symbolize(&self, address: u64) -> Option<String> {
        let table = self.symbols?;
        let sym = table.lookup(address)?;
        let s = table.get(sym.index)?;
        let name = s.display_name();
        Some(if sym.offset == 0 {
            name.to_string()
        } else {
            format!("{name}+{:#x}", sym.offset)
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub start: u64,
    pub end: u64,
    pub kind: RegionKind,
    pub name: String,
    pub value: Option<String>,
    pub note: Option<String>,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub decoder: Option<Decoder>,
    pub section: Option<u32>,
    /// Field nodes never adopt other regions.
    pub is_field: bool,
    /// Uncovered bytes inside this node are labelled as padding/unknown.
    pub fill_gaps: bool,
    /// A filler node for bytes no structure covers.
    pub gap: bool,
    /// Start offsets (relative to `start`) of the entries of a variable-size
    /// decoder, built the first time something looks past the first entry.
    pub starts: std::sync::OnceLock<Vec<u32>>,
    floating: bool,
}

impl Node {
    fn new(start: u64, end: u64, kind: RegionKind, name: String) -> Node {
        Node {
            start,
            end,
            kind,
            name,
            value: None,
            note: None,
            parent: None,
            children: Vec::new(),
            decoder: None,
            section: None,
            is_field: false,
            fill_gaps: false,
            gap: false,
            starts: std::sync::OnceLock::new(),
            floating: false,
        }
    }

    fn contains(&self, other: &Node) -> bool {
        self.start <= other.start && other.end <= self.end
    }
}

pub(crate) struct Layout {
    nodes: Vec<Node>,
    roots: Vec<u32>,
}

pub(crate) struct Builder<'a> {
    pub ctx: Ctx<'a>,
    file_size: u64,
    nodes: Vec<Node>,
}

impl<'a> Builder<'a> {
    pub fn new(ctx: Ctx<'a>) -> Self {
        Builder {
            file_size: ctx.bytes.data.len() as u64,
            ctx,
            nodes: Vec::new(),
        }
    }

    fn push(&mut self, mut node: Node) -> Option<u32> {
        node.end = node.end.min(self.file_size);
        if node.start >= node.end {
            return None;
        }
        let id = self.nodes.len() as u32;
        self.nodes.push(node);
        Some(id)
    }

    /// Adds a region that will be nested wherever it fits by containment.
    pub fn region(&mut self, start: u64, size: u64, kind: RegionKind, name: impl Into<String>) -> Option<u32> {
        let mut node = Node::new(start, start.saturating_add(size), kind, name.into());
        node.floating = true;
        self.push(node)
    }

    /// Adds a region as an explicit child of `parent`.
    pub fn child(
        &mut self,
        parent: u32,
        start: u64,
        size: u64,
        kind: RegionKind,
        name: impl Into<String>,
    ) -> Option<u32> {
        let mut node = Node::new(start, start.saturating_add(size), kind, name.into());
        node.parent = Some(parent);
        let id = self.push(node)?;
        self.nodes[parent as usize].children.push(id);
        Some(id)
    }

    /// Decodes a struct at `offset` and adds each field as a child of `parent`.
    pub fn fields(&mut self, parent: u32, offset: u64, specs: &[F]) -> Vec<FieldValue> {
        let values = fields::decode(&self.ctx, offset, specs);
        self.add_field_nodes(parent, &values);
        values
    }

    pub fn add_field_nodes(&mut self, parent: u32, values: &[FieldValue]) {
        let kind = self.nodes[parent as usize].kind;
        for v in values {
            if let Some(id) = self.child(parent, v.start, v.end - v.start, kind, v.name) {
                let node = &mut self.nodes[id as usize];
                node.value = Some(v.value.clone());
                node.is_field = true;
            }
        }
    }

    /// Adds a struct as a child region with its fields below it.
    pub fn struct_child(
        &mut self,
        parent: u32,
        offset: u64,
        kind: RegionKind,
        name: impl Into<String>,
        specs: &[F],
    ) -> (Option<u32>, Vec<FieldValue>) {
        let size = fields::struct_size(specs);
        match self.child(parent, offset, size, kind, name) {
            Some(id) => {
                let values = self.fields(id, offset, specs);
                (Some(id), values)
            }
            None => (None, Vec::new()),
        }
    }

    /// Adds a floating struct region with its fields.
    pub fn struct_region(
        &mut self,
        offset: u64,
        kind: RegionKind,
        name: impl Into<String>,
        specs: &[F],
    ) -> (Option<u32>, Vec<FieldValue>) {
        let size = fields::struct_size(specs);
        match self.region(offset, size, kind, name) {
            Some(id) => {
                let values = self.fields(id, offset, specs);
                (Some(id), values)
            }
            None => (None, Vec::new()),
        }
    }

    pub fn node(&self, id: u32) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn node_mut(&mut self, id: u32) -> &mut Node {
        &mut self.nodes[id as usize]
    }

    pub fn set_value(&mut self, id: Option<u32>, value: impl Into<String>) {
        if let Some(id) = id {
            self.nodes[id as usize].value = Some(value.into());
        }
    }

    pub fn set_note(&mut self, id: Option<u32>, note: impl Into<String>) {
        if let Some(id) = id {
            self.nodes[id as usize].note = Some(note.into());
        }
    }

    pub fn set_decoder(&mut self, id: Option<u32>, decoder: Decoder) {
        if let Some(id) = id {
            self.nodes[id as usize].decoder = Some(decoder);
        }
    }

    pub fn set_section(&mut self, id: Option<u32>, section: u32) {
        if let Some(id) = id {
            self.nodes[id as usize].section = Some(section);
        }
    }

    pub fn set_fill_gaps(&mut self, id: Option<u32>) {
        if let Some(id) = id {
            self.nodes[id as usize].fill_gaps = true;
        }
    }

    pub fn finish(mut self) -> Layout {
        // Nest floating regions by containment. Sorting by (start, -end, insertion)
        // guarantees that a container is visited before anything it contains.
        let mut floating: Vec<u32> = (0..self.nodes.len() as u32)
            .filter(|&i| self.nodes[i as usize].floating)
            .collect();
        floating.sort_by_key(|&i| {
            let n = &self.nodes[i as usize];
            (n.start, Reverse(n.end), i)
        });
        let mut roots = Vec::new();
        let mut stack: Vec<u32> = Vec::new();
        for &id in &floating {
            while let Some(&top) = stack.last() {
                if self.nodes[top as usize].contains(&self.nodes[id as usize]) {
                    break;
                }
                stack.pop();
            }
            match stack.last() {
                Some(&parent) => {
                    self.nodes[id as usize].parent = Some(parent);
                    self.nodes[parent as usize].children.push(id);
                }
                None => roots.push(id),
            }
            stack.push(id);
        }

        let file_size = self.file_size;
        let mut layout = Layout {
            nodes: self.nodes,
            roots,
        };
        let data = self.ctx.bytes.data;

        // Sort children, then label uncovered ranges.
        for i in 0..layout.nodes.len() {
            let mut children = std::mem::take(&mut layout.nodes[i].children);
            children.sort_by_key(|&c| (layout.nodes[c as usize].start, Reverse(layout.nodes[c as usize].end)));
            layout.nodes[i].children = children;
        }
        layout
            .roots
            .sort_by_key(|&c| (layout.nodes[c as usize].start, Reverse(layout.nodes[c as usize].end)));

        let root_gaps = gaps(&layout.nodes, &layout.roots, 0..file_size);
        for gap in root_gaps {
            let id = layout.add_gap(None, gap, data);
            layout.roots.push(id);
        }
        layout.roots.sort_by_key(|&c| layout.nodes[c as usize].start);

        for i in 0..layout.nodes.len() {
            let node = &layout.nodes[i];
            if !node.fill_gaps {
                continue;
            }
            let range = node.start..node.end;
            let found = gaps(&layout.nodes, &node.children, range);
            if found.is_empty() {
                continue;
            }
            for gap in found {
                let id = layout.add_gap(Some(i as u32), gap, data);
                layout.nodes[i].children.push(id);
            }
            let mut children = std::mem::take(&mut layout.nodes[i].children);
            children.sort_by_key(|&c| layout.nodes[c as usize].start);
            layout.nodes[i].children = children;
        }
        layout
    }
}

/// Ranges of `range` not covered by any of `children` (which are sorted by start).
fn gaps(nodes: &[Node], children: &[u32], range: Range<u64>) -> Vec<Range<u64>> {
    let mut out = Vec::new();
    let mut cursor = range.start;
    for &c in children {
        let n = &nodes[c as usize];
        if n.start > cursor {
            out.push(cursor..n.start.min(range.end));
        }
        cursor = cursor.max(n.end);
        if cursor >= range.end {
            break;
        }
    }
    if cursor < range.end {
        out.push(cursor..range.end);
    }
    out.retain(|r| r.start < r.end);
    out
}

impl Layout {
    fn add_gap(&mut self, parent: Option<u32>, range: Range<u64>, data: &[u8]) -> u32 {
        let bytes = &data[range.start as usize..range.end as usize];
        let zero = bytes.iter().all(|&b| b == 0);
        let (kind, name) = if zero {
            (RegionKind::Padding, "Padding")
        } else {
            (RegionKind::Unknown, "Unclaimed bytes")
        };
        let mut node = Node::new(range.start, range.end, kind, name.to_string());
        node.parent = parent;
        node.gap = true;
        node.note = Some(if zero {
            "Zero bytes not covered by any structure, usually alignment".to_string()
        } else {
            "Bytes not covered by any structure binviz knows about".to_string()
        });
        let id = self.nodes.len() as u32;
        self.nodes.push(node);
        id
    }

    pub fn children(&self, id: Option<u32>) -> &[u32] {
        match id {
            Some(id) => self.nodes.get(id as usize).map_or(&[], |n| &n.children),
            None => &self.roots,
        }
    }

    pub fn info(&self, ctx: &Ctx, id: u32) -> Option<RegionInfo> {
        let n = self.nodes.get(id as usize)?;
        Some(RegionInfo {
            id,
            parent: n.parent,
            start: n.start,
            end: n.end,
            kind: n.kind,
            name: n.name.clone(),
            value: n.value.clone(),
            note: n.note.clone(),
            child_count: n.children.len() as u32,
            entry_count: n.decoder.as_ref().and_then(|d| d.count(ctx, n)),
            decoded: n.decoder.as_ref().is_some_and(|d| d.count(ctx, n) != Some(0)),
            section: n.section,
        })
    }

    /// Finds the child of `children` containing `offset`, preferring the last
    /// (innermost-added) one when siblings overlap.
    fn child_at(&self, children: &[u32], offset: u64) -> Option<u32> {
        let idx = children.partition_point(|&c| self.nodes[c as usize].start <= offset);
        children[..idx].iter().rev().take(4).copied().find(|&c| {
            let n = &self.nodes[c as usize];
            n.start <= offset && offset < n.end
        })
    }

    /// Static node ids from the root down to the innermost region containing `offset`.
    pub fn node_path(&self, offset: u64) -> Vec<u32> {
        let mut path = Vec::new();
        let mut children: &[u32] = &self.roots;
        while let Some(id) = self.child_at(children, offset) {
            path.push(id);
            children = &self.nodes[id as usize].children;
        }
        path
    }

    /// The full "what is this byte" path, including entries decoded on demand.
    pub fn describe(&self, ctx: &Ctx, offset: u64) -> Vec<PathEntry> {
        let ids = self.node_path(offset);
        let mut path: Vec<PathEntry> = ids
            .iter()
            .map(|&id| {
                let n = &self.nodes[id as usize];
                PathEntry {
                    id: Some(id),
                    start: n.start,
                    end: n.end,
                    kind: n.kind,
                    name: n.name.clone(),
                    value: n.value.clone(),
                    note: n.note.clone(),
                }
            })
            .collect();
        if let Some(&last) = ids.last() {
            let node = &self.nodes[last as usize];
            if let Some(decoder) = &node.decoder
                && let Some(entry) = decoder.entry_at(ctx, node, offset)
            {
                let kind = entry.kind.unwrap_or(node.kind);
                path.push(PathEntry {
                    id: None,
                    start: entry.start,
                    end: entry.end,
                    kind,
                    name: entry.name,
                    value: entry.value,
                    note: entry.note,
                });
                if let Some(f) = entry.fields.iter().find(|f| f.start <= offset && offset < f.end) {
                    path.push(PathEntry {
                        id: None,
                        start: f.start,
                        end: f.end,
                        kind,
                        name: f.name.to_string(),
                        value: Some(f.value.clone()),
                        note: None,
                    });
                }
            }
        }
        path
    }

    /// Entries of a decoded region, for browsing tables in the layout tree.
    pub fn entries(&self, ctx: &Ctx, id: u32, first: u32, count: u32) -> Vec<PathEntry> {
        let Some(node) = self.nodes.get(id as usize) else {
            return Vec::new();
        };
        let Some(decoder) = &node.decoder else {
            return Vec::new();
        };
        decoder
            .entries(ctx, node, first, count)
            .into_iter()
            .map(|e| PathEntry {
                id: None,
                start: e.start,
                end: e.end,
                kind: e.kind.unwrap_or(node.kind),
                name: e.name,
                value: e.value,
                note: e.note,
            })
            .collect()
    }

    /// Coloured spans covering `range`, innermost region wins.
    pub fn spans(&self, ctx: &Ctx, range: Range<u64>) -> Vec<Span> {
        let mut out = Vec::new();
        for (i, &id) in self.roots.iter().enumerate() {
            let n = &self.nodes[id as usize];
            if n.end <= range.start || n.start >= range.end {
                continue;
            }
            self.emit_spans(ctx, id, 0, (i % 2) as u8, &range, &mut out);
        }
        merge_spans(out)
    }

    fn emit_spans(&self, ctx: &Ctx, id: u32, depth: u32, shade: u8, range: &Range<u64>, out: &mut Vec<Span>) {
        let node = &self.nodes[id as usize];
        let lo = node.start.max(range.start);
        let hi = node.end.min(range.end);
        if lo >= hi {
            return;
        }
        let first = node.children.partition_point(|&c| self.nodes[c as usize].end <= lo);
        let mut cursor = lo;
        for (i, &c) in node.children.iter().enumerate().skip(first) {
            let child = &self.nodes[c as usize];
            if child.start >= hi {
                break;
            }
            emit_own(ctx, node, depth, shade, cursor, child.start.min(hi), out);
            self.emit_spans(ctx, c, depth + 1, (i % 2) as u8, range, out);
            cursor = cursor.max(child.end.min(hi));
        }
        emit_own(ctx, node, depth, shade, cursor, hi, out);
    }

    /// Dominant region kind for each of `buckets` equal slices of the file.
    pub fn kind_map(&self, file_size: u64, buckets: u32) -> Vec<RegionKind> {
        let buckets = buckets.max(1) as usize;
        let mut weights: Vec<Vec<(RegionKind, u64)>> = vec![Vec::new(); buckets];
        let per = (file_size as f64 / buckets as f64).max(1.0);
        let mut leaves = Vec::new();
        for &r in &self.roots {
            self.collect_leaves(r, 0, &mut leaves);
        }
        for (start, end, kind) in leaves {
            let mut s = start;
            while s < end {
                let b = ((s as f64 / per) as usize).min(buckets - 1);
                let bucket_end = (((b + 1) as f64 * per) as u64).max(s + 1);
                let e = end.min(bucket_end);
                let w = &mut weights[b];
                match w.iter_mut().find(|(k, _)| *k == kind) {
                    Some(entry) => entry.1 += e - s,
                    None => w.push((kind, e - s)),
                }
                s = e;
            }
        }
        weights
            .into_iter()
            .map(|w| {
                w.into_iter()
                    .max_by_key(|&(_, n)| n)
                    .map_or(RegionKind::Unknown, |(k, _)| k)
            })
            .collect()
    }

    /// Leaf regions covering the whole file, in offset order.
    pub fn leaves(&self) -> Vec<(u64, u64, RegionKind)> {
        let mut leaves = Vec::new();
        for &r in &self.roots {
            self.collect_leaves(r, 0, &mut leaves);
        }
        leaves
    }

    /// File ranges of the structures decoded inside a section: the children of
    /// its region, other than filler for uncovered bytes.
    pub fn section_structures(&self, section: u32) -> Vec<(u64, u64)> {
        let mut out: Vec<(u64, u64)> = self
            .nodes
            .iter()
            .filter(|n| n.section == Some(section))
            .flat_map(|n| n.children.iter().map(|&c| &self.nodes[c as usize]))
            .filter(|c| !c.gap)
            .map(|c| (c.start, c.end))
            .collect();
        out.sort_unstable();
        out
    }

    /// Exact number of bytes of each region kind.
    pub fn composition(&self) -> Vec<(RegionKind, u64)> {
        let mut leaves = Vec::new();
        for &r in &self.roots {
            self.collect_leaves(r, 0, &mut leaves);
        }
        let mut totals: Vec<(RegionKind, u64)> = Vec::new();
        for (start, end, kind) in leaves {
            match totals.iter_mut().find(|(k, _)| *k == kind) {
                Some(t) => t.1 += end - start,
                None => totals.push((kind, end - start)),
            }
        }
        totals.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
        totals
    }

    fn collect_leaves(&self, id: u32, depth: u32, out: &mut Vec<(u64, u64, RegionKind)>) {
        let node = &self.nodes[id as usize];
        // Fields and entries don't change the kind, so stop at two levels of structure.
        let structural: Vec<u32> = node
            .children
            .iter()
            .copied()
            .filter(|&c| !self.nodes[c as usize].is_field)
            .collect();
        if structural.is_empty() || depth > 3 {
            out.push((node.start, node.end, node.kind));
            return;
        }
        let mut cursor = node.start;
        for c in structural {
            let child = &self.nodes[c as usize];
            if child.start > cursor {
                out.push((cursor, child.start, node.kind));
            }
            self.collect_leaves(c, depth + 1, out);
            cursor = cursor.max(child.end);
        }
        if cursor < node.end {
            out.push((cursor, node.end, node.kind));
        }
    }
}

/// Spans for the part of `node` not covered by children: decoded entries if the
/// node has a decoder, otherwise the node itself.
fn emit_own(ctx: &Ctx, node: &Node, depth: u32, shade: u8, lo: u64, hi: u64, out: &mut Vec<Span>) {
    if lo >= hi {
        return;
    }
    let plain = |start: u64, end: u64, out: &mut Vec<Span>| {
        if start < end {
            out.push(Span {
                start,
                end,
                kind: node.kind,
                depth,
                shade,
            });
        }
    };
    let Some(decoder) = &node.decoder else {
        plain(lo, hi, out);
        return;
    };
    let mut cursor = lo;
    for (start, end, index, kind) in decoder.bounds(ctx, node, lo..hi) {
        let (start, end) = (start.max(cursor), end.min(hi));
        if start >= end {
            continue;
        }
        plain(cursor, start, out);
        out.push(Span {
            start,
            end,
            kind: kind.unwrap_or(node.kind),
            depth: depth + 1,
            shade: (index % 2) as u8,
        });
        cursor = end;
    }
    plain(cursor, hi, out);
}

fn merge_spans(spans: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for s in spans {
        if let Some(last) = out.last_mut()
            && last.end == s.start
            && last.kind == s.kind
            && last.depth == s.depth
            && last.shade == s.shade
        {
            last.end = s.end;
            continue;
        }
        out.push(s);
    }
    out
}
