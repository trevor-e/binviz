//! Library code found by its bytes. A toolchain's static libraries hold the
//! functions a program was linked with: Psy-Q's `.LIB` and `.OBJ` files for
//! the PlayStation (see [`crate::rom::psyq`]), and MSVC's `.lib`, an archive
//! of COFF objects, for a Windows program's C runtime. Each function's code,
//! with the bytes the linker fills in masked, is a signature: found at the
//! start of a function of a binary, it names the function (`CdRead`,
//! `strlen`, `GsSortObject4`) and marks it library code, which a
//! decompilation leaves be, and says which libraries the program used.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use object::read::archive::ArchiveFile;
use object::{Object, ObjectSection};
use serde::Serialize;

use crate::binary::Binary;
use crate::error::{Error, Result};
use crate::model::{Annotation, Decomp, DecompState};
use crate::rom::psyq;

/// A function's bytes to look for.
#[derive(Debug, Clone)]
pub struct Signature {
    pub name: String,
    /// `LIBGPU.LIB/gpu`, `libcmt.lib/strlen.obj`: the library file and the module.
    pub library: String,
    pub code: Vec<u8>,
    /// Which bits of `code` are compared: 0 where the linker fills them in.
    pub mask: Vec<u8>,
}

/// Signatures from a set of libraries and objects.
#[derive(Debug, Clone, Default)]
pub struct SignatureSet {
    pub signatures: Vec<Signature>,
    pub files: u32,
    /// What was read and not used, per file: import stubs, objects without
    /// machine code (compiled for link-time code generation).
    pub notes: Vec<String>,
    /// The signatures already taken (name and bytes), to keep one of each.
    seen: HashSet<u64>,
}

/// Signatures shorter than this, or with fewer compared bytes, match by chance.
const MIN_BYTES: usize = 16;

/// A function of a library, to take a signature from.
struct Candidate {
    name: String,
    code: Vec<u8>,
    mask: Vec<u8>,
}

impl SignatureSet {
    /// Adds the functions of a library or an object file: Psy-Q's `.LIB` or
    /// `.OBJ`; an archive of COFF or ELF objects (MSVC's `.lib`, a `.a`);
    /// or one such object (`.obj`, `.o`). Returns how many signatures it gave.
    pub fn add_file(&mut self, name: &str, bytes: &[u8]) -> Result<u32> {
        let file = std::path::Path::new(name)
            .file_name()
            .map_or(name.to_string(), |n| n.to_string_lossy().into_owned());
        let from_psyq = |functions: Vec<psyq::ObjFunction>| {
            functions
                .into_iter()
                .map(|f| Candidate {
                    name: f.name,
                    code: f.code,
                    mask: f.mask,
                })
                .collect::<Vec<_>>()
        };
        let modules = if bytes.starts_with(b"LIB") {
            psyq::parse_lib(bytes)?
                .into_iter()
                .map(|(module, functions)| (module, from_psyq(functions)))
                .collect()
        } else if bytes.starts_with(b"LNK") {
            vec![(String::new(), from_psyq(psyq::parse_obj(bytes)?))]
        } else if bytes.starts_with(b"!<arch>\n") {
            self.archive(&file, bytes)?
        } else {
            vec![(String::new(), object_candidates(bytes)?)]
        };
        let mut n = 0;
        for (module, functions) in modules {
            for f in functions {
                let compared = f.mask.iter().filter(|&&m| m != 0).count();
                if f.code.len() < MIN_BYTES || compared * 2 < f.code.len() {
                    continue;
                }
                let mut h = DefaultHasher::new();
                (&f.name, &f.code, &f.mask).hash(&mut h);
                if !self.seen.insert(h.finish()) {
                    continue;
                }
                self.signatures.push(Signature {
                    name: f.name,
                    library: if module.is_empty() {
                        file.clone()
                    } else {
                        format!("{file}/{module}")
                    },
                    code: f.code,
                    mask: f.mask,
                });
                n += 1;
            }
        }
        self.files += 1;
        Ok(n)
    }

    /// The objects of an `ar` archive (MSVC's `.lib`), by member name, with
    /// their functions; import stubs and objects without machine code are
    /// counted in the notes.
    fn archive(&mut self, file: &str, bytes: &[u8]) -> Result<Vec<(String, Vec<Candidate>)>> {
        let archive = ArchiveFile::parse(bytes).map_err(|e| Error::new(format!("not a library: {e}")))?;
        let mut out = Vec::new();
        let (mut imports, mut il, mut unread) = (0, 0, Vec::new());
        for member in archive.members() {
            let member = member.map_err(|e| Error::new(format!("{file}: {e}")))?;
            let data = member.data(bytes).map_err(|e| Error::new(format!("{file}: {e}")))?;
            // MSVC names members by the path the object was built at.
            let raw = String::from_utf8_lossy(member.name()).into_owned();
            let name = raw.rsplit(['/', '\\']).next().unwrap_or(&raw).to_string();
            match member_kind(data) {
                Member::Import => imports += 1,
                Member::NoCode => il += 1,
                Member::Object => match object_candidates(data) {
                    Ok(functions) => out.push((name, functions)),
                    Err(e) => unread.push(format!("{name} ({e})")),
                },
            }
        }
        let n = |count: usize, one: &str, many: &str| format!("{count} {}", if count == 1 { one } else { many });
        let mut skipped = Vec::new();
        if imports > 0 {
            skipped.push(n(imports, "import library member", "import library members"));
        }
        if il > 0 {
            skipped.push(format!(
                "{} (compiled for link-time code generation: MSVC's /GL, clang's -flto)",
                n(il, "object without machine code", "objects without machine code")
            ));
        }
        if !unread.is_empty() {
            let what = n(unread.len(), "object not read", "objects not read");
            skipped.push(format!("{what}: {}", unread.join(", ")));
        }
        if !skipped.is_empty() {
            self.notes.push(format!("{file}: skipped {}", skipped.join("; ")));
        }
        Ok(out)
    }
}

/// What an archive member holds.
enum Member {
    /// A short import stub, or the import descriptors an import library has.
    Import,
    /// An object with no machine code in it: MSVC's intermediate language
    /// (`/GL`) or LLVM bitcode (`-flto`).
    NoCode,
    Object,
}

fn member_kind(data: &[u8]) -> Member {
    match object::FileKind::parse(data) {
        Ok(object::FileKind::CoffImport) => Member::Import,
        Ok(_) => {
            // An import library's descriptors: objects holding only `.idata$` sections.
            let idata = object::File::parse(data).is_ok_and(|f| {
                let mut sections = f.sections().filter(|s| s.size() > 0).peekable();
                sections.peek().is_some() && sections.all(|s| s.name().is_ok_and(|n| n.starts_with(".idata$")))
            });
            if idata { Member::Import } else { Member::Object }
        }
        // An anonymous object header that isn't an import or a big object, or bitcode.
        Err(_) if data.starts_with(&[0, 0, 0xFF, 0xFF]) => Member::NoCode,
        Err(_) if data.starts_with(b"BC\xC0\xDE") || data.starts_with(&[0xDE, 0xC0, 0x17, 0x0B]) => Member::NoCode,
        Err(_) => Member::Object,
    }
}

/// The functions of a COFF or ELF object, with every relocated field masked;
/// 32-bit MSVC C names undecorated (`_strlen` is `strlen`).
fn object_candidates(bytes: &[u8]) -> Result<Vec<Candidate>> {
    let file = object::File::parse(bytes).map_err(|e| Error::new(format!("not an object file: {e}")))?;
    let decorated = file.format() == object::BinaryFormat::Coff && file.architecture() == object::Architecture::I386;
    let functions = crate::matching::file_functions(&file)?;
    Ok(functions
        .into_iter()
        .map(|f| Candidate {
            name: if decorated && !f.name.starts_with('?') {
                crate::matching::undecorated(&f.name).to_string()
            } else {
                f.name.clone()
            },
            mask: f.mask(),
            code: f.code,
        })
        .collect())
}

/// A function of the binary that is one of a library's.
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
    /// The library functions in this binary: each function's start compared
    /// with every signature (the linker's fields aside). An x86 function
    /// matches only a signature as long as its code: what follows the
    /// signature in the function must be padding. Elsewhere (a console's
    /// code) a function may be up to 8 bytes shorter than the signature, its
    /// size leaving out a delay slot or a `nop` after the return.
    pub fn identify_sdk(&self, sigs: &SignatureSet) -> SdkReport {
        let x86 = match self.arch {
            object::Architecture::I386 => Some(32),
            object::Architecture::X86_64 | object::Architecture::X86_64_X32 => Some(64),
            _ => None,
        };
        // Signatures by their first four bytes, when those are compared.
        let mut by_start: HashMap<[u8; 4], Vec<&Signature>> = HashMap::new();
        let mut anywhere = Vec::new();
        for s in &sigs.signatures {
            match s.code.get(..4) {
                Some(start) if s.mask[..4] == [0xFF; 4] => {
                    by_start.entry(start.try_into().expect("4 bytes")).or_default().push(s)
                }
                _ => anywhere.push(s),
            }
        }
        let mut matches = Vec::new();
        let mut checked = 0;
        for f in self.symbols().functions() {
            let Some(bytes) = self.code_bytes(f.address) else {
                continue;
            };
            checked += 1;
            let size = f.size as usize;
            let fits = |len: usize| match x86 {
                Some(bits) => {
                    size == 0
                        || (size >= len && crate::matching::padding_only(&bytes[len..size.min(bytes.len())], bits))
                }
                None => size == 0 || size + 8 >= len,
            };
            let start: Option<[u8; 4]> = bytes.get(..4).and_then(|b| b.try_into().ok());
            let mut found: Vec<&Signature> = start
                .and_then(|s| by_start.get(&s))
                .into_iter()
                .flatten()
                .chain(&anywhere)
                .copied()
                .filter(|s| {
                    bytes.len() >= s.code.len()
                        && fits(s.code.len())
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
    /// saying which library it is from, and marked library code, so that a
    /// decompilation skips it (and its callers don't wait on it).
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
                decomp: Some(Decomp {
                    state: DecompState::Library,
                    source: m.library.clone(),
                    ..Default::default()
                }),
            })
            .collect()
    }

    pub fn to_text(&self) -> String {
        let mut out = format!(
            "{} of {} functions are library code ({} signatures from the libraries).\n",
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
    use crate::matching::x86_tests::{image, object};

    /// A System V `ar` archive (MSVC's `.lib` is one, with linker members first).
    fn ar(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut out = b"!<arch>\n".to_vec();
        for (name, data) in members {
            let header = format!(
                "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
                format!("{name}/"),
                0,
                0,
                0,
                644,
                data.len()
            );
            out.extend(header.as_bytes());
            out.extend(data);
            if data.len() % 2 == 1 {
                out.push(b'\n');
            }
        }
        out
    }

    /// A library function: two arguments added, `helper` called, `g` added, 24 bytes.
    const SUM: [u8; 24] = [
        0x8B, 0x44, 0x24, 0x04, // mov eax, [esp+4]
        0x03, 0x44, 0x24, 0x08, // add eax, [esp+8]
        0x50, // push eax
        0xE8, 0xFC, 0xFF, 0xFF, 0xFF, // call helper
        0x83, 0xC4, 0x04, // add esp, 4
        0x03, 0x05, 0x00, 0x00, 0x00, 0x00, // add eax, [g]
        0xC3, // ret
    ];

    #[test]
    fn signatures_from_a_library_of_objects() {
        // A short import stub, an object compiled for link-time code generation, LLVM bitcode.
        let mut stub = vec![
            0, 0, 0xFF, 0xFF, 0, 0, 0x4C, 0x01, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0x08, 0,
        ];
        stub.extend(b"_Sleep@4\0KERNEL32.dll\0");
        let mut il = vec![0, 0, 0xFF, 0xFF, 1, 0, 0x4C, 0x01];
        il.resize(64, 0);
        let mut bitcode = b"BC\xC0\xDE".to_vec();
        bitcode.resize(64, 0);
        let lib = ar(&[
            ("sum.o", object("lib_sum", &SUM, &[(10, 2, "helper"), (19, 1, "g")])),
            // Too short to tell apart from other code by its bytes.
            ("tiny.o", object("lib_tiny", &[0x8D, 0x41, 0x01, 0xC3], &[])),
            ("KERNEL32.dll", stub),
            ("ltcg.obj", il),
            ("lto.o", bitcode),
        ]);
        let mut sigs = SignatureSet::default();
        assert_eq!(sigs.add_file("dir/x86lib.lib", &lib).unwrap(), 1);
        assert_eq!(sigs.signatures[0].library, "x86lib.lib/sum.o");
        assert_eq!(&sigs.signatures[0].mask[9..14], &[0xFF, 0, 0, 0, 0]);
        assert_eq!(
            sigs.notes,
            [
                "x86lib.lib: skipped 1 import library member; 2 objects without machine code \
              (compiled for link-time code generation: MSVC's /GL, clang's -flto)"
            ]
        );
        // The same file again gives nothing new.
        assert_eq!(sigs.add_file("x86lib.lib", &lib).unwrap(), 0);

        // The binary: the function linked (its fields filled in, padding after
        // it), a function starting with the same bytes that goes on, and one
        // shorter than the signature, whatever follows it.
        let mut linked = SUM.to_vec();
        linked[10..14].copy_from_slice(&0x6Bu32.to_le_bytes());
        linked[19..23].copy_from_slice(&0x401800u32.to_le_bytes());
        let mut code = linked.clone();
        code.resize(0x20, 0xCC);
        code.extend(&linked);
        code.extend([0xB8, 1, 0, 0, 0, 0xC3]);
        code.resize(0x40, 0xCC);
        code.extend(&linked);
        let functions = [
            ("f1", 0x401100, 0x20),
            ("f2", 0x401120, 30),
            ("f3", 0x401140, 16),
            ("f4", 0x401150, 8),
        ];
        let bin = image(&code, &functions);
        let r = bin.identify_sdk(&sigs);
        let found: Vec<(u64, &str)> = r.matches.iter().map(|m| (m.address, m.name.as_str())).collect();
        assert_eq!(found, [(0x401100, "lib_sum")], "{}", r.to_text());
        assert_eq!(r.libraries, [("x86lib.lib".to_string(), 1)]);
        assert_eq!(r.annotations()[0].decomp.as_ref().unwrap().state, DecompState::Library);
    }
}
