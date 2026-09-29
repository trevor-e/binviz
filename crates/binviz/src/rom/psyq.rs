//! Psy-Q, Sony's PlayStation SDK: its libraries (`LIBGPU.LIB`, `LIBSPU.LIB`…)
//! are archives of object files in the SDK's own `LNK` format. Their
//! functions' bytes, with the places the linker patches masked, are
//! signatures: found in a game, they name the SDK's code (`GsSortObject4`,
//! `CdRead`, `SpuSetKey`) so that a decompilation can leave it be, and say
//! which libraries the game was linked with.
//!
//! The `LNK` format is a stream of records: sections, code bytes and zeroes
//! for the current section, exported and local symbols at offsets in
//! sections, imports, and relocations (a type, an offset and an expression
//! tree). A `LIB` is a header then modules, each a name, a date, the offset
//! of its `LNK` data and of the next module, and the names it defines.

use std::collections::HashMap;

use serde::Serialize;

use crate::binary::Binary;
use crate::error::{Error, Result};
use crate::model::Annotation;

/// A function from an object file: its bytes, and which of them the linker
/// fills in (1 bits in `mask` are compared, 0 bits are not).
#[derive(Debug, Clone)]
pub struct ObjFunction {
    pub name: String,
    pub code: Vec<u8>,
    pub mask: Vec<u8>,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn u8(&mut self) -> Result<u8> {
        let v = *self.b.get(self.at).ok_or_else(|| Error::new("LNK: truncated"))?;
        self.at += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes([self.u8()?, self.u8()?]))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes([self.u8()?, self.u8()?, self.u8()?, self.u8()?]))
    }
    fn bytes(&mut self, n: usize) -> Result<&[u8]> {
        let s = self.b.get(self.at..self.at + n).ok_or_else(|| Error::new("LNK: truncated"))?;
        self.at += n;
        Ok(s)
    }
    fn string(&mut self) -> Result<String> {
        let n = self.u8()? as usize;
        Ok(String::from_utf8_lossy(self.bytes(n)?).into_owned())
    }
    /// Skips a relocation expression.
    fn expression(&mut self) -> Result<()> {
        match self.u8()? {
            0 => {
                self.u32()?;
            }
            2 | 4 | 12 | 22 => {
                self.u16()?;
            }
            44 | 46 | 50 => {
                self.expression()?;
                self.expression()?;
            }
            op => return Err(Error::new(format!("LNK: unknown expression opcode {op}"))),
        }
        Ok(())
    }
}

#[derive(Default)]
struct Section {
    code: Vec<u8>,
    mask: Vec<u8>,
    /// (offset, name), exported and local.
    symbols: Vec<(u32, String)>,
}

/// The functions of a Psy-Q object file (`LNK`).
pub fn parse_obj(bytes: &[u8]) -> Result<Vec<ObjFunction>> {
    if bytes.len() < 4 || &bytes[..3] != b"LNK" {
        return Err(Error::new("not a Psy-Q object file (no LNK header)"));
    }
    let mut r = Reader { b: bytes, at: 4 };
    let mut sections: HashMap<u16, Section> = HashMap::new();
    let mut current: u16 = 0;
    loop {
        let op = match r.u8() {
            Ok(op) => op,
            Err(_) => break,
        };
        match op {
            0 => break,
            2 => {
                let n = r.u16()? as usize;
                let data = r.bytes(n)?.to_vec();
                let s = sections.entry(current).or_default();
                s.code.extend_from_slice(&data);
                s.mask.extend(std::iter::repeat_n(0xFF, n));
            }
            6 => current = r.u16()?,
            8 => {
                let n = r.u32()? as usize;
                let s = sections.entry(current).or_default();
                s.code.extend(std::iter::repeat_n(0, n));
                s.mask.extend(std::iter::repeat_n(0, n));
            }
            10 => {
                let kind = r.u8()?;
                let offset = r.u16()? as usize;
                r.expression()?;
                let width = match kind {
                    // Half-word patches: the low 16 bits of the instruction.
                    12 | 30 | 82 | 84 | 96 | 98 | 100 => 2,
                    _ => 4,
                };
                let s = sections.entry(current).or_default();
                for i in offset..(offset + width).min(s.mask.len()) {
                    s.mask[i] = 0;
                }
            }
            12 => {
                r.u16()?;
                let section = r.u16()?;
                let offset = r.u32()?;
                let name = r.string()?;
                sections.entry(section).or_default().symbols.push((offset, name));
            }
            14 => {
                r.u16()?;
                r.string()?;
            }
            16 => {
                r.u16()?;
                r.u16()?;
                r.u8()?;
                r.string()?;
            }
            18 => {
                let section = r.u16()?;
                let offset = r.u32()?;
                let name = r.string()?;
                sections.entry(section).or_default().symbols.push((offset, format!(".{name}")));
            }
            28 => {
                r.u16()?;
                r.string()?;
            }
            46 => {
                r.u8()?;
            }
            48 => {
                r.u16()?;
                r.u16()?;
                r.u32()?;
                r.string()?;
            }
            50 => {
                r.u16()?;
            }
            52 => {
                r.u16()?;
                r.u8()?;
            }
            54 => {
                r.u16()?;
                r.u16()?;
            }
            56 => {
                r.u16()?;
                r.u32()?;
            }
            58 => {
                r.u16()?;
                r.u32()?;
                r.u16()?;
            }
            60 => {
                r.u16()?;
            }
            74 => {
                r.u16()?;
                r.u32()?;
                r.u16()?;
                r.u32()?;
                r.u16()?;
                r.u32()?;
                r.u16()?;
                r.u32()?;
                r.u32()?;
                r.string()?;
            }
            76 => {
                r.u16()?;
                r.u32()?;
                r.u32()?;
            }
            78 | 80 => {
                r.u16()?;
                r.u32()?;
                r.u32()?;
            }
            82 | 84 => {
                r.u16()?;
                r.u32()?;
                r.u16()?;
                r.u16()?;
                r.string()?;
                r.string()?;
            }
            86 => {
                r.u16()?;
                r.u32()?;
                r.u16()?;
                r.u32()?;
                r.u16()?;
                r.u32()?;
                r.u16()?;
                r.u32()?;
                r.u32()?;
                r.u32()?;
                r.string()?;
            }
            op => return Err(Error::new(format!("LNK: unknown record {op} at {}", r.at - 1))),
        }
    }
    let mut out = Vec::new();
    for s in sections.values() {
        let mut symbols = s.symbols.clone();
        symbols.sort();
        for (i, (offset, name)) in symbols.iter().enumerate() {
            if name.starts_with('.') {
                continue;
            }
            let start = *offset as usize;
            let end = symbols[i + 1..]
                .iter()
                .map(|(o, _)| *o as usize)
                .find(|&o| o > start)
                .unwrap_or(s.code.len())
                .min(s.code.len());
            if end <= start {
                continue;
            }
            out.push(ObjFunction {
                name: name.clone(),
                code: s.code[start..end].to_vec(),
                mask: s.mask[start..end].to_vec(),
            });
        }
    }
    Ok(out)
}

/// The modules of a Psy-Q library (`LIB`): (module name, its functions).
pub fn parse_lib(bytes: &[u8]) -> Result<Vec<(String, Vec<ObjFunction>)>> {
    if bytes.len() < 4 || &bytes[..3] != b"LIB" {
        return Err(Error::new("not a Psy-Q library (no LIB header)"));
    }
    let mut out = Vec::new();
    let mut at = 4;
    while at + 20 <= bytes.len() {
        let name = String::from_utf8_lossy(&bytes[at..at + 8]).trim().to_string();
        let word = |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]) as usize;
        let (lnk, next) = (word(at + 12), word(at + 16));
        let lnk_at = if bytes.get(at + lnk..at + lnk + 3) == Some(b"LNK") {
            at + lnk
        } else if bytes.get(lnk..lnk + 3) == Some(b"LNK") {
            lnk
        } else {
            return Err(Error::new(format!("LIB: module {name:?} at {at:#x} has no LNK data")));
        };
        let end = if next == 0 {
            bytes.len()
        } else if at + next <= bytes.len() && at + next > lnk_at {
            at + next
        } else if next <= bytes.len() && next > lnk_at {
            next
        } else {
            bytes.len()
        };
        let functions = parse_obj(&bytes[lnk_at..end]).map_err(|e| Error::new(format!("LIB: module {name}: {e}")))?;
        out.push((name, functions));
        if end >= bytes.len() || next == 0 {
            break;
        }
        at = end;
    }
    Ok(out)
}

/// A function's bytes to look for.
#[derive(Debug, Clone)]
pub struct Signature {
    pub name: String,
    /// `LIBGPU.LIB/gpu` : the library file and the module.
    pub library: String,
    pub code: Vec<u8>,
    pub mask: Vec<u8>,
}

/// Signatures from a set of libraries and objects.
#[derive(Debug, Clone, Default)]
pub struct SignatureSet {
    pub signatures: Vec<Signature>,
    pub files: u32,
}

/// Signatures shorter than this, or with fewer compared bytes, match by chance.
const MIN_BYTES: usize = 16;

impl SignatureSet {
    /// Adds a `.LIB` or `.OBJ` file's functions.
    pub fn add_file(&mut self, name: &str, bytes: &[u8]) -> Result<u32> {
        let modules = if bytes.starts_with(b"LIB") {
            parse_lib(bytes)?
        } else {
            vec![(String::new(), parse_obj(bytes)?)]
        };
        let file = std::path::Path::new(name)
            .file_name()
            .map_or(name.to_string(), |n| n.to_string_lossy().into_owned());
        let mut n = 0;
        for (module, functions) in modules {
            for f in functions {
                let compared = f.mask.iter().filter(|&&m| m != 0).count();
                if f.code.len() < MIN_BYTES || compared * 2 < f.code.len() {
                    continue;
                }
                self.signatures.push(Signature {
                    name: f.name,
                    library: if module.is_empty() { file.clone() } else { format!("{file}/{module}") },
                    code: f.code,
                    mask: f.mask,
                });
                n += 1;
            }
        }
        self.files += 1;
        Ok(n)
    }
}

/// A function of the binary that is one of the SDK's.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdkMatch {
    pub address: u64,
    pub name: String,
    pub library: String,
    pub size: u64,
    /// Other signatures with the same bytes (aliases, or identical functions).
    pub also: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdkReport {
    pub matches: Vec<SdkMatch>,
    /// Functions matched per library file, most first.
    pub libraries: Vec<(String, u32)>,
    pub functions_checked: u32,
    pub signatures: u32,
}

impl Binary {
    /// The SDK functions in this binary: each function's start compared with
    /// every signature (the linker's fields aside).
    pub fn identify_sdk(&self, sigs: &SignatureSet) -> SdkReport {
        let mut matches = Vec::new();
        let mut checked = 0;
        for f in self.symbols().functions() {
            let Some(bytes) = self.code_bytes(f.address) else { continue };
            checked += 1;
            let mut found: Vec<&Signature> = sigs
                .signatures
                .iter()
                .filter(|s| {
                    bytes.len() >= s.code.len()
                        && (f.size == 0 || f.size + 8 >= s.code.len() as u64)
                        && s.code
                            .iter()
                            .zip(&s.mask)
                            .zip(bytes)
                            .all(|((c, m), b)| (c & m) == (b & m))
                })
                .collect();
            if found.is_empty() {
                continue;
            }
            // The longest signature is the surest; the others with the same length are aliases.
            found.sort_by(|a, b| b.code.len().cmp(&a.code.len()).then(a.name.cmp(&b.name)));
            let best = found[0];
            let also = found[1..]
                .iter()
                .filter(|s| s.code.len() == best.code.len() && s.name != best.name)
                .map(|s| s.name.clone())
                .collect();
            matches.push(SdkMatch {
                address: f.address,
                name: best.name.clone(),
                library: best.library.clone(),
                size: best.code.len() as u64,
                also,
            });
        }
        let mut per: HashMap<String, u32> = HashMap::new();
        for m in &matches {
            let file = m.library.split('/').next().unwrap_or(&m.library).to_string();
            *per.entry(file).or_default() += 1;
        }
        let mut libraries: Vec<(String, u32)> = per.into_iter().collect();
        libraries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        SdkReport {
            matches,
            libraries,
            functions_checked: checked,
            signatures: sigs.signatures.len() as u32,
        }
    }
}

impl SdkReport {
    /// The matches as notes: each function named, with an `sdk:` comment
    /// saying which library it is from, so that it can be skipped.
    pub fn annotations(&self) -> Vec<Annotation> {
        self.matches
            .iter()
            .map(|m| Annotation {
                address: m.address,
                size: m.size,
                name: m.name.clone(),
                comment: format!("sdk: {}", m.library),
                reviewed: true,
                kind: Some("function".into()),
            })
            .collect()
    }

    pub fn to_text(&self) -> String {
        let mut out = format!(
            "{} of {} functions are the SDK's ({} signatures from the libraries).\n",
            self.matches.len(),
            self.functions_checked,
            self.signatures
        );
        for (lib, n) in &self.libraries {
            out.push_str(&format!("  {n:>5}  {lib}\n"));
        }
        for m in &self.matches {
            out.push_str(&format!(
                "{:#x}  {}  ({}, {} bytes){}\n",
                m.address,
                m.name,
                m.library,
                m.size,
                if m.also.is_empty() {
                    String::new()
                } else {
                    format!("  also: {}", m.also.join(", "))
                }
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An LNK object with one section holding `code`, symbols at offsets,
    /// and 2-byte relocations at `patches`.
    pub(crate) fn lnk(code: &[u8], symbols: &[(u32, &str)], patches: &[(u16, u8)]) -> Vec<u8> {
        let mut o = b"LNK\x02".to_vec();
        o.push(46);
        o.push(7);
        o.extend([16, 1, 0, 0, 0, 3, 5]);
        o.extend(b".text");
        o.extend([6, 1, 0]);
        o.push(2);
        o.extend((code.len() as u16).to_le_bytes());
        o.extend(code);
        for (at, kind) in patches {
            o.push(10);
            o.push(*kind);
            o.extend(at.to_le_bytes());
            // ($symbol 1) + 0
            o.extend([44, 2, 1, 0, 0, 0, 0, 0, 0]);
        }
        for (i, (offset, name)) in symbols.iter().enumerate() {
            o.push(12);
            o.extend((i as u16 + 1).to_le_bytes());
            o.extend(1u16.to_le_bytes());
            o.extend(offset.to_le_bytes());
            o.push(name.len() as u8);
            o.extend(name.as_bytes());
        }
        o.extend([14, 9, 0, 6]);
        o.extend(b"extern");
        o.push(0);
        o
    }

    fn lib(modules: &[(&str, &[u8])]) -> Vec<u8> {
        let mut o = b"LIB\x01".to_vec();
        for (i, (name, obj)) in modules.iter().enumerate() {
            let start = o.len();
            let mut header = format!("{name:<8}").into_bytes();
            header.extend(0u32.to_le_bytes());
            let names = 20 + 1 + name.len() + 1;
            header.extend((names as u32).to_le_bytes());
            let next = if i + 1 == modules.len() { 0 } else { names + obj.len() };
            header.extend((next as u32).to_le_bytes());
            header.push(name.len() as u8);
            header.extend(name.as_bytes());
            header.push(0);
            assert_eq!(header.len(), names);
            o.extend(header);
            o.extend(*obj);
            assert_eq!(o.len() - start, names + obj.len());
        }
        o
    }

    #[test]
    fn objects_libraries_and_signatures() {
        // Two functions: `GsInit` (28 bytes, a jal patched at 8 and a lo16 at 20) and `GsClear` (16).
        let code: Vec<u8> = (0..44u8).map(|i| i.wrapping_mul(37).wrapping_add(11)).collect();
        let obj = lnk(&code, &[(0, "GsInit"), (28, "GsClear")], &[(8, 74), (20, 84)]);
        let f = parse_obj(&obj).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].name.as_str(), f[0].code.len()), ("GsInit", 28));
        assert_eq!(&f[0].mask[8..12], &[0, 0, 0, 0]);
        assert_eq!(&f[0].mask[20..22], &[0, 0]);
        assert_eq!(f[0].mask[22], 0xFF);
        assert_eq!((f[1].name.as_str(), f[1].code.len()), ("GsClear", 16));

        let lib = lib(&[("gs", &obj), ("gs2", &lnk(&[1; 20], &[(0, "GsTiny")], &[]))]);
        let modules = parse_lib(&lib).unwrap();
        assert_eq!(modules.len(), 2);
        assert_eq!((modules[0].0.as_str(), modules[1].0.as_str()), ("gs", "gs2"));
        assert_eq!(modules[1].1[0].name, "GsTiny");

        // The binary: GsInit's bytes with the patched fields filled in, then GsClear's, at 0x80010000.
        let mut sigs = SignatureSet::default();
        assert_eq!(sigs.add_file("LIBGS.LIB", &lib).unwrap(), 3);
        let mut game = code.clone();
        game[8..12].copy_from_slice(&0x0C00_4000u32.to_le_bytes());
        game[20..22].copy_from_slice(&[0x34, 0x12]);
        let mut data = vec![0u8; 0x800];
        data[..8].copy_from_slice(b"PS-X EXE");
        for (at, v) in [(0x10, 0x8001_0000u32), (0x14, 0x8001_8000), (0x18, 0x8001_0000), (0x1C, 0x40)] {
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        data.extend(&game);
        data.extend([0u8; 20]);
        let mut bin = Binary::parse(data).unwrap();
        // Functions the follower wouldn't find (the bytes are made up): name them so they exist.
        bin.set_annotations(vec![
            Annotation { address: 0x8001_0000, size: 28, name: "f1".into(), comment: String::new(), reviewed: false, kind: Some("function".into()) },
            Annotation { address: 0x8001_001C, size: 16, name: "f2".into(), comment: String::new(), reviewed: false, kind: Some("function".into()) },
        ]);
        let r = bin.identify_sdk(&sigs);
        let names: Vec<(u64, &str)> = r.matches.iter().map(|m| (m.address, m.name.as_str())).collect();
        assert_eq!(names, [(0x8001_0000, "GsInit"), (0x8001_001C, "GsClear")]);
        assert_eq!(r.libraries, [("LIBGS.LIB".to_string(), 2)]);
        let notes = r.annotations();
        assert_eq!(notes[0].comment, "sdk: LIBGS.LIB/gs");
        assert!(r.to_text().contains("2 of"));
    }
}
