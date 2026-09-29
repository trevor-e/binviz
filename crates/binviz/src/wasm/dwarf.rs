//! DWARF in a WebAssembly module's `.debug_*` custom sections, read at
//! binviz's addresses (see [`super`]).
//!
//! WebAssembly's DWARF counts a code address from the start of the code
//! section's contents, and a data address (`DW_OP_addr` of a global
//! variable) in linear memory. Both are moved as the sections are read:
//! code addresses by the file offset of the code section's contents, data
//! addresses by [`MEMORY_BASE`]. To tell one from the other, the DWARF is
//! walked once with a reader that records where each address it reads
//! lies: `DW_FORM_addr` attributes, range and location lists, line
//! programs and address ranges hold code addresses; expressions hold data
//! addresses. The copies of the sections holding them are patched, and
//! read as usual; the sections that hold none are shared with the file.
//!
//! Addresses of discarded code are left as they are: `wasm-ld` writes
//! `0xffffffff` there (`0xfffffffe` in `.debug_ranges` and `.debug_loc`),
//! older linkers 0. A pair of a DWARF 4 range or location list counts
//! from the unit's base address, which moves with the unit's
//! `DW_AT_low_pc` (0 in a unit whose code is in several ranges), so it is
//! left as it is too, unless the unit has no `DW_AT_low_pc` to count from.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use gimli::{AttributeValue, EndianSlice, Operation, RawLocListEntry, RawRngListEntry, RunTimeEndian, SectionId};

use super::MEMORY_BASE;
use super::read::{CUSTOM, Module};
use crate::dwarf::{DebugInfo, DwarfSection, R};
use crate::error::Result;
use crate::model::Section;

/// The sections gimli can be asked for, by the names they have as custom sections.
const IDS: [SectionId; 17] = [
    SectionId::DebugAbbrev,
    SectionId::DebugAddr,
    SectionId::DebugAranges,
    SectionId::DebugInfo,
    SectionId::DebugLine,
    SectionId::DebugLineStr,
    SectionId::DebugLoc,
    SectionId::DebugLocLists,
    SectionId::DebugMacinfo,
    SectionId::DebugMacro,
    SectionId::DebugRanges,
    SectionId::DebugRngLists,
    SectionId::DebugStr,
    SectionId::DebugStrOffsets,
    SectionId::DebugTypes,
    SectionId::DebugCuIndex,
    SectionId::DebugTuIndex,
];

type Log = Rc<RefCell<Vec<(SectionId, usize, u64)>>>;

/// Notes, in a log all sections share, the section offset of every address
/// read, and the value there.
#[derive(Clone, Debug)]
struct Recorder {
    id: SectionId,
    log: Log,
}

impl gimli::Relocate<usize> for Recorder {
    fn relocate_address(&self, offset: usize, value: u64) -> gimli::Result<u64> {
        self.log.borrow_mut().push((self.id, offset, value));
        Ok(value)
    }

    fn relocate_offset(&self, _offset: usize, value: usize) -> gimli::Result<usize> {
        Ok(value)
    }
}

type Rd<'a> = gimli::RelocateReader<EndianSlice<'a, RunTimeEndian>, Recorder>;

/// Whether an address is one to move: not a tombstone for discarded code
/// (the largest two values), and not 0 unless 0 is meant (a unit's base).
fn live(value: u64, size: u8, zero: bool) -> bool {
    let max = if size >= 8 { u64::MAX } else { u64::from(u32::MAX) };
    value < max - 1 && (value != 0 || zero)
}

/// Where the addresses to move are: (section, offset) → (address size, amount to add).
#[derive(Default)]
struct Sites {
    at: HashMap<(SectionId, usize), (u8, u64)>,
    /// `.debug_addr` entries: (offset, size, amount, whether 0 is an address there).
    addr: Vec<(usize, u8, u64, bool)>,
}

/// What moving the addresses needs to know.
struct Walk<'a> {
    log: Log,
    sites: Sites,
    /// Where the code section's contents start, for code addresses.
    code: Option<u64>,
    /// Whether linear memory has addresses, for data addresses.
    memory: bool,
    lists: HashSet<(bool, usize)>,
    lines: HashSet<usize>,
    dwarf: &'a gimli::Dwarf<Rd<'a>>,
}

impl Walk<'_> {
    fn take(&self) -> Vec<(SectionId, usize, u64)> {
        std::mem::take(&mut *self.log.borrow_mut())
    }

    fn clear(&self) {
        self.log.borrow_mut().clear();
    }

    /// Code addresses just read.
    fn code(&mut self, read: &[(SectionId, usize, u64)], size: u8, zero: bool) {
        let Some(base) = self.code else { return };
        for &(id, offset, value) in read {
            if live(value, size, zero) {
                self.sites.at.entry((id, offset)).or_insert((size, base));
            }
        }
    }

    /// Data addresses just read.
    fn data(&mut self, read: &[(SectionId, usize, u64)], size: u8) {
        if !self.memory {
            return;
        }
        for &(id, offset, value) in read {
            if live(value, size, false) {
                self.sites.at.entry((id, offset)).or_insert((size, MEMORY_BASE));
            }
        }
    }

    /// An entry of `.debug_addr`, named by index.
    fn index(&mut self, unit: &gimli::Unit<Rd>, index: gimli::DebugAddrIndex<usize>, code: bool, zero: bool) {
        let size = unit.encoding().address_size;
        let add = match (code, self.code) {
            (true, Some(base)) => base,
            (false, _) if self.memory => MEMORY_BASE,
            _ => return,
        };
        let offset = unit.addr_base.0 + index.0 * size as usize;
        self.sites.addr.push((offset, size, add, zero));
    }

    /// The data addresses in an expression, unless it computes a thread-local address.
    fn expression(&mut self, unit: &gimli::Unit<Rd>, expr: gimli::Expression<Rd>) {
        self.clear();
        let mut ops = expr.operations(unit.encoding());
        let mut tls = false;
        let mut indices = Vec::new();
        while let Ok(Some(op)) = ops.next() {
            match op {
                Operation::TLS => tls = true,
                Operation::AddressIndex { index } => indices.push(index),
                _ => {}
            }
        }
        let read = self.take();
        if tls {
            return;
        }
        self.data(&read, unit.encoding().address_size);
        for i in indices {
            self.index(unit, i, false, false);
        }
    }

    fn ranges(&mut self, unit: &gimli::Unit<Rd>, offset: gimli::RangeListsOffset<usize>, based: bool) {
        if !self.lists.insert((false, offset.0)) {
            return;
        }
        let size = unit.encoding().address_size;
        let Ok(mut iter) = self.dwarf.raw_ranges(unit, offset) else {
            return;
        };
        let mut based = based;
        loop {
            self.clear();
            let Ok(Some(entry)) = iter.next() else { break };
            let read = self.take();
            match entry {
                RawRngListEntry::AddressOrOffsetPair { .. } if !based => self.code(&read, size, false),
                RawRngListEntry::AddressOrOffsetPair { .. } | RawRngListEntry::OffsetPair { .. } => {}
                RawRngListEntry::BaseAddress { .. } => {
                    self.code(&read, size, true);
                    based = true;
                }
                RawRngListEntry::StartEnd { .. } | RawRngListEntry::StartLength { .. } => self.code(&read, size, false),
                RawRngListEntry::BaseAddressx { addr } => {
                    self.index(unit, addr, true, true);
                    based = true;
                }
                RawRngListEntry::StartxEndx { begin, end } => {
                    self.index(unit, begin, true, false);
                    self.index(unit, end, true, false);
                }
                RawRngListEntry::StartxLength { begin, .. } => self.index(unit, begin, true, false),
            }
        }
    }

    fn locations(&mut self, unit: &gimli::Unit<Rd>, offset: gimli::LocationListsOffset<usize>, based: bool) {
        if !self.lists.insert((true, offset.0)) {
            return;
        }
        let size = unit.encoding().address_size;
        let Ok(mut iter) = self.dwarf.raw_locations(unit, offset) else {
            return;
        };
        let mut based = based;
        loop {
            self.clear();
            let Ok(Some(entry)) = iter.next() else { break };
            let read = self.take();
            let data = match entry {
                RawLocListEntry::AddressOrOffsetPair { data, .. } => {
                    if !based {
                        self.code(&read, size, false);
                    }
                    Some(data)
                }
                RawLocListEntry::BaseAddress { .. } => {
                    self.code(&read, size, true);
                    based = true;
                    None
                }
                RawLocListEntry::BaseAddressx { addr } => {
                    self.index(unit, addr, true, true);
                    based = true;
                    None
                }
                RawLocListEntry::StartxEndx { begin, end, data } => {
                    self.index(unit, begin, true, false);
                    self.index(unit, end, true, false);
                    Some(data)
                }
                RawLocListEntry::StartxLength { begin, data, .. } => {
                    self.index(unit, begin, true, false);
                    Some(data)
                }
                RawLocListEntry::StartEnd { data, .. } | RawLocListEntry::StartLength { data, .. } => {
                    self.code(&read, size, false);
                    Some(data)
                }
                RawLocListEntry::OffsetPair { data, .. } | RawLocListEntry::DefaultLocation { data } => Some(data),
            };
            if let Some(expr) = data {
                self.expression(unit, expr);
            }
        }
    }

    /// A unit's DIEs, attribute by attribute, and its line program.
    fn unit(&mut self, unit: &gimli::Unit<Rd>) -> gimli::Result<()> {
        let size = unit.encoding().address_size;
        let mut entries = unit.entries_raw(None)?;
        let mut root = true;
        // A DWARF 4 list counts from the unit's DW_AT_low_pc, when it has one.
        let mut based = false;
        while !entries.is_empty() {
            let Some(abbrev) = entries.read_abbreviation()? else {
                continue;
            };
            let mut attrs = Vec::with_capacity(abbrev.attributes().len());
            for spec in abbrev.attributes() {
                self.clear();
                let attr = entries.read_attribute(*spec)?;
                attrs.push((attr, self.take()));
            }
            if root {
                based = attrs.iter().any(|(a, _)| a.name() == gimli::DW_AT_low_pc);
            }
            for (attr, read) in attrs {
                // The unit's base address is an address even when it is 0.
                let base = root && attr.name() == gimli::DW_AT_low_pc;
                self.code(&read, size, base);
                match attr.value() {
                    AttributeValue::DebugAddrIndex(i) => self.index(unit, i, true, base),
                    AttributeValue::Exprloc(expr) => self.expression(unit, expr),
                    v @ (AttributeValue::RangeListsRef(_) | AttributeValue::DebugRngListsIndex(_)) => {
                        if let Ok(Some(offset)) = self.dwarf.attr_ranges_offset(unit, v) {
                            self.ranges(unit, offset, based);
                        }
                    }
                    v @ (AttributeValue::LocationListsRef(_) | AttributeValue::DebugLocListsIndex(_)) => {
                        if let Ok(Some(offset)) = self.dwarf.attr_locations_offset(unit, v) {
                            self.locations(unit, offset, based);
                        }
                    }
                    _ => {}
                }
            }
            root = false;
        }
        if let Some(program) = &unit.line_program {
            let header = program.header();
            if self.lines.insert(header.offset().0) {
                let mut insts = header.instructions();
                loop {
                    self.clear();
                    match insts.next_instruction(header) {
                        Ok(Some(gimli::LineInstruction::SetAddress(_))) => {
                            let read = self.take();
                            self.code(&read, header.address_size(), false);
                        }
                        Ok(Some(_)) => {}
                        _ => break,
                    }
                }
            }
        }
        Ok(())
    }

    /// `.debug_aranges`: each range's start (its length is read as an address too).
    fn aranges(&mut self) {
        let mut headers = self.dwarf.debug_aranges.headers();
        while let Ok(Some(h)) = headers.next() {
            let size = h.encoding().address_size;
            let mut entries = h.entries();
            loop {
                self.clear();
                let Ok(Some(_)) = entries.next() else { break };
                let read = self.take();
                for pair in read.chunks(2) {
                    self.code(&pair[..1], size, false);
                }
            }
        }
    }
}

/// Where each DWARF section is in `data`: (start, end), by section.
fn sections_in(holder: &Module) -> HashMap<SectionId, (usize, usize)> {
    let mut found = HashMap::new();
    for s in holder.sections.iter().filter(|s| s.id == CUSTOM) {
        if let Some(id) = IDS.iter().find(|id| id.name() == s.name) {
            found.entry(*id).or_insert((s.content as usize, s.end as usize));
        }
    }
    found
}

/// Where the addresses to move are.
fn sites(data: &[u8], found: &HashMap<SectionId, (usize, usize)>, target: &Module) -> Result<Sites> {
    let log: Log = Rc::default();
    let dwarf = gimli::Dwarf::load(|id| -> gimli::Result<Rd> {
        let bytes = found.get(&id).map_or(&[][..], |&(s, e)| &data[s..e]);
        let recorder = Recorder { id, log: log.clone() };
        Ok(gimli::RelocateReader::new(
            EndianSlice::new(bytes, RunTimeEndian::Little),
            recorder,
        ))
    })?;
    let mut walk = Walk {
        log: log.clone(),
        sites: Sites::default(),
        code: target.code_base(),
        memory: target.memory_mapped(),
        lists: HashSet::new(),
        lines: HashSet::new(),
        dwarf: &dwarf,
    };
    let mut units = dwarf.units();
    // A unit that can't be read is skipped: what is read of the others still moves.
    while let Ok(Some(header)) = units.next() {
        if let Ok(unit) = dwarf.unit(header) {
            let _ = walk.unit(&unit);
        }
    }
    walk.aranges();
    Ok(walk.sites)
}

/// Adds `add` to the `size`-byte little-endian address at `offset`, if the sum still fits.
fn patch(bytes: &mut [u8], offset: usize, size: u8, add: u64, zero: bool) {
    let Some(slot) = bytes.get_mut(offset..offset + size as usize) else {
        return;
    };
    match size {
        4 => {
            let v = u64::from(u32::from_le_bytes(slot.try_into().expect("4 bytes")));
            if live(v, 4, zero)
                && let Ok(n) = u32::try_from(v + add)
            {
                slot.copy_from_slice(&n.to_le_bytes());
            }
        }
        8 => {
            let v = u64::from_le_bytes(slot.try_into().expect("8 bytes"));
            if live(v, 8, zero)
                && let Some(n) = v.checked_add(add)
            {
                slot.copy_from_slice(&n.to_le_bytes());
            }
        }
        _ => {}
    }
}

/// Reads the DWARF in `holder`'s `.debug_*` sections (in `data`), its
/// addresses moved to `target`'s: the same module, or for a separate debug
/// file, the module it describes. `None` when there is none.
pub(crate) fn load(
    data: &Arc<[u8]>,
    holder: &Module,
    target: &Module,
    sections: &[Section],
    arch: object::Architecture,
    source: &str,
) -> Result<Option<DebugInfo>> {
    let found = sections_in(holder);
    if !found.contains_key(&SectionId::DebugInfo) && !found.contains_key(&SectionId::DebugLine) {
        return Ok(None);
    }
    let sites = sites(data, &found, target)?;
    let mut copies: HashMap<SectionId, Vec<u8>> = HashMap::new();
    let copy = |id: SectionId, copies: &mut HashMap<SectionId, Vec<u8>>| {
        copies
            .remove(&id)
            .or_else(|| found.get(&id).map(|&(s, e)| data[s..e].to_vec()))
    };
    for (&(id, offset), &(size, add)) in &sites.at {
        let Some(mut bytes) = copy(id, &mut copies) else {
            continue;
        };
        patch(&mut bytes, offset, size, add, true);
        copies.insert(id, bytes);
    }
    if !sites.addr.is_empty()
        && let Some(mut bytes) = copy(SectionId::DebugAddr, &mut copies)
    {
        let mut done = HashSet::new();
        for &(offset, size, add, zero) in &sites.addr {
            if done.insert(offset) {
                patch(&mut bytes, offset, size, add, zero);
            }
        }
        copies.insert(SectionId::DebugAddr, bytes);
    }
    let whole = R::new(data.clone(), RunTimeEndian::Little);
    let mut infos = Vec::new();
    let dwarf = gimli::Dwarf::load(|id| -> Result<R> {
        let r = match (copies.remove(&id), found.get(&id)) {
            (Some(bytes), _) => R::new(Arc::from(bytes), RunTimeEndian::Little),
            (None, Some(&(s, e))) => whole.range(s..e),
            _ => R::new(Arc::from(&[][..]), RunTimeEndian::Little),
        };
        if !r.bytes().is_empty() {
            infos.push(DwarfSection {
                name: id.name().to_string(),
                size: r.bytes().len() as u64,
            });
        }
        Ok(r)
    })?;
    DebugInfo::from_dwarf(dwarf, infos, source, sections, arch).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tombstones_stay() {
        assert!(live(0x10, 4, false));
        assert!(!live(0, 4, false));
        assert!(live(0, 4, true));
        assert!(!live(0xffff_ffff, 4, true));
        assert!(!live(0xffff_fffe, 4, false));
        assert!(live(0xffff_fffe, 8, false));
        let mut bytes = [0x10, 0, 0, 0, 0xff, 0xff, 0xff, 0xff];
        patch(&mut bytes, 0, 4, 0x100, false);
        patch(&mut bytes, 4, 4, 0x100, false);
        assert_eq!(bytes, [0x10, 1, 0, 0, 0xff, 0xff, 0xff, 0xff]);
    }
}
