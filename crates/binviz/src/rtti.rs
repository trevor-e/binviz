//! C++ run-time type information in binaries built by MSVC (and clang-cl):
//! what a stripped Windows binary still says about its classes. For each
//! polymorphic class the compiler writes
//!
//! - a type descriptor: a pointer to `type_info`'s vtable, a spare word,
//!   and the class's decorated name (`.?AVSquare@shapes@@`);
//! - a class hierarchy descriptor: the class and its bases, each with where
//!   its sub-object sits in the class (a base class descriptor);
//! - for each vtable, a complete object locator: the offset of the
//!   sub-object the vtable is for, and pointers to the type and hierarchy
//!   descriptors; the word before the vtable points at it.
//!
//! Found, each gets the name MSVC gives it (`??_7Square@shapes@@6B@`, which
//! reads `const shapes::Square::`vftable'`), so a stripped binary's vtables
//! and descriptors are named as its PDB would name them. In 32-bit images
//! these structures hold addresses; in 64-bit ones, offsets from the image
//! base.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::binary::Binary;
use crate::model::{Format, RegionKind};

/// A class RTTI describes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CppClass {
    /// `shapes::Square`.
    pub name: String,
    /// `.?AVSquare@shapes@@`.
    pub decorated: String,
    pub type_descriptor: u64,
    /// Its bases, nearest first, with where each sub-object sits.
    pub bases: Vec<CppBase>,
    pub vtables: Vec<CppVtable>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CppBase {
    pub name: String,
    /// Offset of the base's sub-object in the class.
    pub offset: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CppVtable {
    pub address: u64,
    /// The offset of the sub-object whose vtable it is.
    pub offset: u32,
    /// The base the vtable is for, when the class has more than one.
    pub for_base: Option<String>,
    /// Its entries: the virtual functions.
    pub functions: Vec<u64>,
    pub locator: u64,
}

/// What reading the RTTI found: the classes, and a name for each structure.
#[derive(Default)]
pub(crate) struct Rtti {
    pub classes: Vec<CppClass>,
    /// (decorated name, address, size) of every structure.
    pub symbols: Vec<(String, u64, u64)>,
}

/// A type descriptor found: its address and decorated name.
struct Descriptor {
    address: u64,
    name: String,
}

/// A number the way MSVC's decorated names write it: `A@` for 0, a digit
/// for 1 to 10, hex digits `A`–`P` then `@` otherwise, `?` before negatives.
fn number(n: i64) -> String {
    if n < 0 {
        return format!("?{}", number(-n));
    }
    match n {
        0 => "A@".into(),
        1..=10 => ((b'0' + (n - 1) as u8) as char).to_string(),
        _ => {
            let hex = format!("{n:X}");
            let letters: String = hex
                .chars()
                .map(|c| (b'A' + c.to_digit(16).unwrap_or(0) as u8) as char)
                .collect();
            format!("{letters}@")
        }
    }
}

/// The class name a type descriptor's decorated name stands for: through the
/// demangler, as the vtable's name (`??_7` + the name + `6B@`).
fn class_name(decorated: &str) -> String {
    let id = &decorated[4..];
    if let Some(d) = crate::util::demangle(&format!("??_7{id}6B@")) {
        let d = d.trim_start_matches("const ");
        if let Some(name) = d.strip_suffix("::`vftable'") {
            return name.to_string();
        }
    }
    // `Bar@ns@@` is ns::Bar.
    id.trim_end_matches('@').split('@').rev().collect::<Vec<_>>().join("::")
}

impl Binary {
    /// The C++ classes an MSVC-built binary's RTTI describes.
    pub fn cpp_classes(&self) -> Vec<CppClass> {
        self.read_rtti().classes
    }

    pub(crate) fn read_rtti(&self) -> Rtti {
        let mut out = Rtti::default();
        if self.summary.format != Format::Pe {
            return out;
        }
        let ptr = if self.is64 { 8u64 } else { 4 };
        let data_sections: Vec<&crate::model::Section> = self
            .sections
            .iter()
            .filter(|s| s.loaded && s.kind != RegionKind::Code && s.file_offset.is_some() && !s.compressed)
            .collect();
        let bytes_of = |s: &crate::model::Section| {
            let off = s.file_offset.unwrap_or(0) as usize;
            self.data
                .get(off..off + s.file_size.min(s.size) as usize)
                .unwrap_or(&[])
        };
        let u32_at = |a: u64| -> Option<u32> {
            let o = self.address_to_offset(a)? as usize;
            Some(u32::from_le_bytes(self.data.get(o..o + 4)?.try_into().ok()?))
        };
        // In 64-bit images, the structures point at each other by offsets from the image base.
        let pointer_at = |a: u64| -> Option<u64> {
            if self.is64 {
                u32_at(a).map(|rva| self.image_base + rva as u64)
            } else {
                u32_at(a).map(|v| v as u64)
            }
        };
        let word_at = |a: u64| -> Option<u64> {
            let o = self.address_to_offset(a)? as usize;
            if self.is64 {
                Some(u64::from_le_bytes(self.data.get(o..o + 8)?.try_into().ok()?))
            } else {
                u32_at(a).map(|v| v as u64)
            }
        };
        let in_code = |a: u64| self.section_at(a).is_some_and(|s| s.kind == RegionKind::Code);

        // Type descriptors, by their decorated names.
        let mut descriptors: HashMap<u64, Descriptor> = HashMap::new();
        for s in &data_sections {
            let bytes = bytes_of(s);
            for pattern in [b".?AV".as_slice(), b".?AU".as_slice()] {
                for at in memchr::memmem::find_iter(bytes, pattern) {
                    let Some(td) = (at as u64).checked_sub(2 * ptr).map(|o| s.address + o) else {
                        continue;
                    };
                    if !td.is_multiple_of(ptr) {
                        continue;
                    }
                    let name_bytes: Vec<u8> = bytes[at..]
                        .iter()
                        .take(1024)
                        .take_while(|&&b| b != 0)
                        .copied()
                        .collect();
                    let Ok(name) = String::from_utf8(name_bytes) else {
                        continue;
                    };
                    if !name.ends_with("@@") || name.len() < 7 {
                        continue;
                    }
                    descriptors.insert(td, Descriptor { address: td, name });
                }
            }
        }
        if descriptors.is_empty() {
            return out;
        }
        // Complete object locators: (address, offset, type descriptor, hierarchy descriptor).
        let mut locators: Vec<(u64, u32, u64, u64)> = Vec::new();
        for s in &data_sections {
            let bytes = bytes_of(s);
            let mut at = (4 - s.address % 4) % 4;
            while at + 20 <= bytes.len() as u64 {
                let col = s.address + at;
                let word = |k: u64| {
                    u32::from_le_bytes(
                        bytes[(at + 4 * k) as usize..(at + 4 * k + 4) as usize]
                            .try_into()
                            .unwrap(),
                    )
                };
                let signature = word(0);
                if (signature == 0 && !self.is64) || (signature == 1 && self.is64 && at + 24 <= bytes.len() as u64) {
                    let td = if self.is64 {
                        self.image_base + word(3) as u64
                    } else {
                        word(3) as u64
                    };
                    let chd = if self.is64 {
                        self.image_base + word(4) as u64
                    } else {
                        word(4) as u64
                    };
                    let own = !self.is64 || self.image_base + word(5) as u64 == col;
                    if own && descriptors.contains_key(&td) && self.section_at(chd).is_some() {
                        locators.push((col, word(1), td, chd));
                    }
                }
                at += 4;
            }
        }
        let locator_at: HashMap<u64, usize> = locators.iter().enumerate().map(|(i, l)| (l.0, i)).collect();
        // Vtables: the word before each points at a locator.
        let mut vtables: Vec<(u64, usize, Vec<u64>)> = Vec::new();
        for s in &data_sections {
            let bytes = bytes_of(s);
            let mut at = (ptr - s.address % ptr) % ptr;
            while at + ptr <= bytes.len() as u64 {
                let v = word_at(s.address + at).unwrap_or(0);
                if let Some(&l) = locator_at.get(&v) {
                    let vt = s.address + at + ptr;
                    let mut functions = Vec::new();
                    let mut e = vt;
                    while let Some(f) = word_at(e).filter(|&f| in_code(f) && !locator_at.contains_key(&f)) {
                        functions.push(f);
                        e += ptr;
                        if functions.len() >= 4096 {
                            break;
                        }
                    }
                    if !functions.is_empty() {
                        vtables.push((vt, l, functions));
                    }
                }
                at += ptr;
            }
        }
        // The hierarchy: each base class descriptor's type and where its sub-object sits.
        let hierarchy = |chd: u64| -> Vec<(u64, i32, u32)> {
            let (Some(count), Some(array)) = (u32_at(chd + 8), pointer_at(chd + 12)) else {
                return Vec::new();
            };
            (0..count.min(256) as u64)
                .filter_map(|i| {
                    let bcd = pointer_at(array + 4 * i)?;
                    let td = pointer_at(bcd)?;
                    let mdisp = u32_at(bcd + 8)? as i32;
                    let attributes = u32_at(bcd + 20)?;
                    descriptors.contains_key(&td).then_some((td, mdisp, attributes))
                })
                .collect()
        };
        let mut seen_chd: HashSet<u64> = HashSet::new();
        let mut classes: HashMap<u64, CppClass> = HashMap::new();
        for (_, _, td, chd) in &locators {
            let d = &descriptors[td];
            let id = &d.name[4..];
            let bases = hierarchy(*chd);
            classes.entry(*td).or_insert_with(|| CppClass {
                name: class_name(&d.name),
                decorated: d.name.clone(),
                type_descriptor: *td,
                bases: bases
                    .iter()
                    .skip(1)
                    .map(|&(b, mdisp, _)| CppBase {
                        name: class_name(&descriptors[&b].name),
                        offset: mdisp,
                    })
                    .collect(),
                vtables: Vec::new(),
            });
            // The descriptors each structure's name comes from.
            if seen_chd.insert(*chd) {
                out.symbols.push((format!("??_R3{id}8"), *chd, 16));
                if let Some(array) = pointer_at(chd + 12) {
                    out.symbols
                        .push((format!("??_R2{id}8"), array, 4 * bases.len().max(1) as u64));
                    for (i, &(b, mdisp, attributes)) in bases.iter().enumerate() {
                        let Some(bcd) = pointer_at(array + 4 * i as u64) else {
                            continue;
                        };
                        let (pdisp, vdisp) = (
                            u32_at(bcd + 12).unwrap_or(u32::MAX) as i32,
                            u32_at(bcd + 16).unwrap_or(0) as i32,
                        );
                        let base_id = &descriptors[&b].name[4..];
                        out.symbols.push((
                            format!(
                                "??_R1{}{}{}{}{base_id}8",
                                number(mdisp as i64),
                                number(pdisp as i64),
                                number(vdisp as i64),
                                number(attributes as i64)
                            ),
                            bcd,
                            if attributes & 0x40 != 0 { 7 * 4 } else { 6 * 4 },
                        ));
                    }
                }
            }
        }
        // A class with more than one vtable says which base each is for.
        let mut per_class: HashMap<u64, u32> = HashMap::new();
        for (_, l, _) in &vtables {
            *per_class.entry(locators[*l].2).or_default() += 1;
        }
        for (vt, l, functions) in vtables {
            let (col, offset, td, chd) = locators[l];
            let id = &descriptors[&td].name[4..];
            let for_base = (per_class.get(&td).copied().unwrap_or(0) > 1)
                .then(|| hierarchy(chd).into_iter().skip(1).find(|b| b.1 == offset as i32))
                .flatten()
                .map(|b| &descriptors[&b.0].name[4..]);
            let suffix = match for_base {
                Some(b) => format!("6B{b}@"),
                None => "6B@".into(),
            };
            out.symbols
                .push((format!("??_7{id}{suffix}"), vt, functions.len() as u64 * ptr));
            out.symbols
                .push((format!("??_R4{id}{suffix}"), col, if self.is64 { 24 } else { 20 }));
            if let Some(class) = classes.get_mut(&td) {
                class.vtables.push(CppVtable {
                    address: vt,
                    offset,
                    for_base: for_base.map(|b| class_name(&format!(".?AV{b}"))),
                    functions,
                    locator: col,
                });
            }
        }
        for d in descriptors.values().filter(|d| classes.contains_key(&d.address)) {
            out.symbols.push((
                format!("??_R0{}@8", &d.name[1..]),
                d.address,
                2 * ptr + d.name.len() as u64 + 1,
            ));
        }
        let mut classes: Vec<CppClass> = classes.into_values().collect();
        for c in &mut classes {
            c.vtables.sort_by_key(|v| v.offset);
        }
        classes.sort_by(|a, b| a.name.cmp(&b.name));
        out.classes = classes;
        out.symbols.sort_by_key(|s| s.1);
        out.symbols.dedup_by_key(|s| s.1);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_as_msvc_writes_them() {
        assert_eq!(number(0), "A@");
        assert_eq!(number(1), "0");
        assert_eq!(number(10), "9");
        assert_eq!(number(12), "M@");
        assert_eq!(number(64), "EA@");
        assert_eq!(number(-1), "?0");
        assert_eq!(class_name(".?AVSquare@shapes@@"), "shapes::Square");
        assert_eq!(class_name(".?AU?$Box@H@@"), "Box<int>");
    }
}
