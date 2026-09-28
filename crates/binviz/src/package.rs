//! Folders and zips of binaries: every file whose first bytes say it is a
//! binary (Mach-O, universal binaries included, ELF or PE), each paired with
//! its separate debug file (a dSYM, an ELF `.debug` file) by UUID or build ID,
//! and every other file by kind of content. An `.ipa`, an `.xcarchive`, an
//! `.app`, an unpacked Android or Linux package, a build folder: all just
//! folders here, and zips inside them open like folders too.
//!
//! Discovery asks for as little as it can ([`plan`]): the first bytes of files
//! that might be binaries, for [`header`], and `Info.plist` files, for
//! [`bundle_info`] (the name and version of the app a binary belongs to). The
//! caller reads them however it reads the files (a folder, a zip directory, a
//! browser `Blob`) and hands the results to [`discover`].

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// A file in a folder or zip, as a walk or a zip directory lists it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageFile {
    /// Relative to the root, `/`-separated.
    pub path: String,
    pub size: u64,
    /// In a zip: bytes in the archive.
    #[serde(default)]
    pub compressed_size: Option<u64>,
    /// In a zip: the CRC-32 (duplicates show without reading anything).
    #[serde(default)]
    pub crc32: Option<u32>,
}

/// What to read before discovery.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Files that may be binaries (or zips to open): their first
    /// [`HEADER_BYTES`], for [`header`].
    pub headers: Vec<u32>,
    /// `Info.plist` files, whole, for [`bundle_info`].
    pub plists: Vec<u32>,
}

/// How much of a file's start to read to recognise a binary and its build IDs.
pub const HEADER_BYTES: u64 = 64 * 1024;

/// What a binary is, from its header. The order is the order binaries are listed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BinaryKind {
    Executable,
    Library,
    /// A loadable bundle or plug-in (Mach-O `MH_BUNDLE`, a kernel extension).
    Plugin,
    /// Anything else that loads or runs: a dynamic linker, a core file…
    Other,
    /// A relocatable object file (`.o`).
    Object,
    /// Separate debug info: a dSYM's DWARF file, an ELF file with only debug sections.
    Debug,
}

impl BinaryKind {
    pub fn label(self) -> &'static str {
        match self {
            BinaryKind::Executable => "executable",
            BinaryKind::Library => "library",
            BinaryKind::Plugin => "plug-in",
            BinaryKind::Other => "binary",
            BinaryKind::Object => "object file",
            BinaryKind::Debug => "debug file",
        }
    }
}

/// An architecture and its build ID: a Mach-O slice's UUID or an ELF build
/// ID (empty when there is none, or the header read didn't reach it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildId {
    pub arch: String,
    pub id: String,
}

/// What a binary's first bytes say.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    /// "Mach-O", "ELF" or "PE".
    pub format: String,
    pub kind: BinaryKind,
    /// One per architecture (a universal binary has several).
    pub ids: Vec<BuildId>,
}

/// An app, extension or framework bundle's `Info.plist` facts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleInfo {
    /// The folder holding the `Info.plist` (filled in by [`discover`]).
    pub path: String,
    pub name: Option<String>,
    pub bundle_id: Option<String>,
    pub version: Option<String>,
    pub build: Option<String>,
    pub min_os: Option<String>,
    pub platforms: Vec<String>,
    pub executable: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageBinary {
    pub index: u32,
    /// Index into the files.
    pub file: u32,
    pub path: String,
    /// The file name.
    pub name: String,
    /// "Mach-O", "ELF" or "PE".
    pub format: String,
    pub kind: BinaryKind,
    /// The bundle it is the executable of, from an `Info.plist` beside it.
    pub bundle: Option<BundleInfo>,
    pub size: u64,
    pub compressed_size: Option<u64>,
    /// Architectures and build IDs, from the header (every slice's, once the binary is read).
    pub ids: Vec<BuildId>,
    /// Index into the debug files: one with its build ID, or the one its debug link names.
    pub debug: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugFile {
    pub index: u32,
    pub file: u32,
    pub path: String,
    pub size: u64,
    pub compressed_size: Option<u64>,
    pub ids: Vec<BuildId>,
    /// The binary it belongs to.
    pub binary: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileCategory {
    Binaries,
    AssetCatalogs,
    Images,
    Interface,
    Localization,
    Fonts,
    Media,
    MlModels,
    Web,
    Data,
    /// Headers, Swift modules, static libraries: build products that don't need to ship.
    DeveloperFiles,
    CodeSignature,
    /// Debug files, dSYMs and symbol maps: not part of what users install.
    DebugSymbols,
    Other,
}

impl FileCategory {
    pub fn label(self) -> &'static str {
        match self {
            FileCategory::Binaries => "Binaries",
            FileCategory::AssetCatalogs => "Asset catalogs",
            FileCategory::Images => "Images",
            FileCategory::Interface => "Nibs & storyboards",
            FileCategory::Localization => "Localization",
            FileCategory::Fonts => "Fonts",
            FileCategory::Media => "Audio & video",
            FileCategory::MlModels => "ML models",
            FileCategory::Web => "Web content",
            FileCategory::Data => "Data & config",
            FileCategory::DeveloperFiles => "Headers & modules",
            FileCategory::CodeSignature => "Code signature",
            FileCategory::DebugSymbols => "Debug symbols",
            FileCategory::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySize {
    pub category: FileCategory,
    pub files: u32,
    pub size: u64,
    pub compressed_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    pub file: u32,
    pub path: String,
    pub size: u64,
    pub compressed_size: Option<u64>,
    pub category: FileCategory,
}

/// Files with identical contents (same CRC-32 and size).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub size: u64,
    pub paths: Vec<String>,
    /// Bytes that one copy would save.
    pub wasted: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    /// "folder" or "zip".
    pub kind: String,
    pub name: String,
    /// Executables first (a bundle's, then the shallowest, then the largest:
    /// an app's own executable leads), then libraries, plug-ins and objects.
    pub binaries: Vec<PackageBinary>,
    pub debug_files: Vec<DebugFile>,
    pub files: u32,
    /// Every file but debug files and symbols.
    pub size: u64,
    /// When every file is zipped: the compressed bytes.
    pub compressed_size: Option<u64>,
    pub debug_size: u64,
    pub categories: Vec<CategorySize>,
    pub largest: Vec<FileRef>,
    pub duplicates: Vec<DuplicateGroup>,
    pub duplicate_bytes: u64,
}

// --- Headers -------------------------------------------------------------------------

/// Reads integers of either byte order, `None` past the end.
struct Bytes<'a> {
    b: &'a [u8],
    le: bool,
}

impl Bytes<'_> {
    fn array<const N: usize>(&self, at: usize) -> Option<[u8; N]> {
        self.b.get(at..at.checked_add(N)?)?.try_into().ok()
    }
    fn u16(&self, at: usize) -> Option<u16> {
        let a = self.array(at)?;
        Some(if self.le {
            u16::from_le_bytes(a)
        } else {
            u16::from_be_bytes(a)
        })
    }
    fn u32(&self, at: usize) -> Option<u32> {
        let a = self.array(at)?;
        Some(if self.le {
            u32::from_le_bytes(a)
        } else {
            u32::from_be_bytes(a)
        })
    }
    fn u64(&self, at: usize) -> Option<u64> {
        let a = self.array(at)?;
        Some(if self.le {
            u64::from_le_bytes(a)
        } else {
            u64::from_be_bytes(a)
        })
    }
}

/// Recognises a binary from its first bytes: Mach-O (thin or universal), ELF or PE.
pub fn header(prefix: &[u8]) -> Option<Header> {
    mach_o(prefix).or_else(|| elf(prefix)).or_else(|| pe(prefix))
}

fn mach_arch(cputype: u32, subtype: u32) -> String {
    match (cputype, subtype & 0x00FF_FFFF) {
        (0x0100_000C, 2) => "arm64e".into(),
        (0x0100_000C, _) => "arm64".into(),
        (0x0200_000C, _) => "arm64_32".into(),
        (0x0100_0007, _) => "x86_64".into(),
        (7, _) => "i386".into(),
        (12, 11) => "armv7s".into(),
        (12, 12) => "armv7k".into(),
        (12, _) => "armv7".into(),
        (t, s) => format!("cpu {t:#x}/{s:#x}"),
    }
}

fn mach_kind(filetype: u32) -> BinaryKind {
    match filetype {
        1 => BinaryKind::Object,
        2 => BinaryKind::Executable,
        6 | 9 => BinaryKind::Library,
        8 | 11 => BinaryKind::Plugin,
        10 => BinaryKind::Debug,
        _ => BinaryKind::Other,
    }
}

/// A thin Mach-O header at `at`: (filetype, arch, UUID if its load commands are within `b`).
fn mach_thin(b: &[u8], at: usize) -> Option<(u32, String, Option<String>)> {
    let r = Bytes { b, le: true };
    let wide = match r.u32(at)? {
        0xFEED_FACF => true,
        0xFEED_FACE => false,
        _ => return None,
    };
    let (cputype, subtype, filetype, ncmds) = (r.u32(at + 4)?, r.u32(at + 8)?, r.u32(at + 12)?, r.u32(at + 16)?);
    let mut off = at + if wide { 32 } else { 28 };
    let mut uuid = None;
    for _ in 0..ncmds.min(4096) {
        let (Some(cmd), Some(size)) = (r.u32(off), r.u32(off + 4)) else {
            break;
        };
        if cmd == 0x1B {
            uuid = b.get(off + 8..off + 24).map(crate::util::uuid);
            break;
        }
        if size < 8 {
            break;
        }
        off += size as usize;
    }
    Some((filetype, mach_arch(cputype, subtype), uuid))
}

fn mach_o(b: &[u8]) -> Option<Header> {
    let r = Bytes { b, le: false };
    let (kind, ids) = match r.u32(0)? {
        magic @ (0xCAFE_BABE | 0xCAFE_BABF) => {
            let n = r.u32(4)? as usize;
            // Java class files share the magic; their "count" is a large version number.
            if n == 0 || n > 32 {
                return None;
            }
            let wide = magic == 0xCAFE_BABF;
            let mut kind = None;
            let mut ids = Vec::new();
            for i in 0..n {
                let e = 8 + i * if wide { 32 } else { 20 };
                let (cputype, subtype) = (r.u32(e)?, r.u32(e + 4)?);
                let offset = if wide { r.u64(e + 8)? } else { r.u32(e + 8)? as u64 };
                // A slice past the bytes read keeps an empty UUID until the file is read in full.
                let slice = usize::try_from(offset).ok().and_then(|o| mach_thin(b, o));
                if kind.is_none() {
                    kind = slice.as_ref().map(|s| mach_kind(s.0));
                }
                ids.push(BuildId {
                    arch: mach_arch(cputype, subtype),
                    id: slice.and_then(|s| s.2).unwrap_or_default(),
                });
            }
            (kind.unwrap_or(BinaryKind::Other), ids)
        }
        _ => {
            let (filetype, arch, uuid) = mach_thin(b, 0)?;
            (
                mach_kind(filetype),
                vec![BuildId {
                    arch,
                    id: uuid.unwrap_or_default(),
                }],
            )
        }
    };
    Some(Header {
        format: "Mach-O".into(),
        kind,
        ids,
    })
}

fn elf_arch(machine: u16, wide: bool) -> String {
    match machine {
        3 => "x86".into(),
        62 => "x86_64".into(),
        40 => "arm".into(),
        183 => "arm64".into(),
        243 if wide => "riscv64".into(),
        243 => "riscv32".into(),
        8 => "mips".into(),
        20 => "ppc".into(),
        21 => "ppc64".into(),
        22 => "s390x".into(),
        258 => "loongarch64".into(),
        m => format!("machine {m}"),
    }
}

/// The GNU build ID among the notes at `b[start..start + len]`.
fn gnu_build_id(r: &Bytes, start: usize, len: usize, align: usize) -> Option<String> {
    let end = start.checked_add(len)?.min(r.b.len());
    let aligned = |at: usize| start + (at - start).next_multiple_of(align);
    let mut at = start;
    while at + 12 <= end {
        let (namesz, descsz, kind) = (r.u32(at)? as usize, r.u32(at + 4)? as usize, r.u32(at + 8)?);
        let desc = aligned(at.checked_add(12)?.checked_add(namesz)?);
        let next = desc.checked_add(descsz)?;
        if kind == 3 && r.b.get(at + 12..at + 12 + namesz)?.starts_with(b"GNU") {
            return r.b.get(desc..next).map(crate::util::hex_compact);
        }
        at = aligned(next);
    }
    None
}

/// Whether the dynamic entries at `b[start..start + len]` mark a position-independent executable.
fn elf_pie(r: &Bytes, start: usize, len: usize, wide: bool) -> bool {
    let size = if wide { 16 } else { 8 };
    let word = |at: usize| if wide { r.u64(at) } else { r.u32(at).map(u64::from) };
    let end = start.saturating_add(len).min(r.b.len());
    let mut at = start;
    while at + size <= end {
        match (word(at), word(at + size / 2)) {
            (Some(0), _) | (None, _) | (_, None) => break,
            // DT_FLAGS_1 with DF_1_PIE.
            (Some(0x6fff_fffb), Some(flags)) => return flags & 0x0800_0000 != 0,
            _ => at += size,
        }
    }
    false
}

fn elf(b: &[u8]) -> Option<Header> {
    if !b.starts_with(b"\x7fELF") {
        return None;
    }
    let wide = match b.get(4)? {
        1 => false,
        2 => true,
        _ => return None,
    };
    let le = match b.get(5)? {
        1 => true,
        2 => false,
        _ => return None,
    };
    let r = Bytes { b, le };
    let (e_type, machine) = (r.u16(16)?, r.u16(18)?);
    let (phoff, phentsize, phnum) = if wide {
        (r.u64(32)?, r.u16(54)?, r.u16(56)?)
    } else {
        (r.u32(28)? as u64, r.u16(42)?, r.u16(44)?)
    };
    let (mut interp, mut pie, mut code_dropped, mut build_id) = (false, false, false, None);
    for i in 0..u64::from(phnum.min(256)) {
        let Some(ph) = usize::try_from(phoff + i * u64::from(phentsize)).ok() else {
            break;
        };
        let Some(p_type) = r.u32(ph) else { break };
        let fields = if wide {
            (
                r.u32(ph + 4),
                r.u64(ph + 8),
                r.u64(ph + 32),
                r.u64(ph + 40),
                r.u64(ph + 48),
            )
        } else {
            let w = |o: usize| r.u32(ph + o).map(u64::from);
            (r.u32(ph + 24), w(4), w(16), w(20), w(28))
        };
        let (Some(flags), Some(offset), Some(filesz), Some(memsz), Some(align)) = fields else {
            break;
        };
        let (offset, filesz) = (offset as usize, filesz as usize);
        match p_type {
            // An executable segment with no bytes in the file: the code was
            // taken out, leaving the debug sections (`objcopy --only-keep-debug`).
            1 if flags & 1 != 0 && filesz == 0 && memsz > 0 => code_dropped = true,
            2 => pie |= elf_pie(&r, offset, filesz, wide),
            3 => interp = true,
            4 if build_id.is_none() => build_id = gnu_build_id(&r, offset, filesz, if align == 8 { 8 } else { 4 }),
            _ => {}
        }
    }
    let kind = match e_type {
        _ if code_dropped => BinaryKind::Debug,
        1 => BinaryKind::Object,
        2 => BinaryKind::Executable,
        3 if interp || pie => BinaryKind::Executable,
        3 => BinaryKind::Library,
        _ => BinaryKind::Other,
    };
    Some(Header {
        format: "ELF".into(),
        kind,
        ids: vec![BuildId {
            arch: elf_arch(machine, wide),
            id: build_id.unwrap_or_default(),
        }],
    })
}

fn pe(b: &[u8]) -> Option<Header> {
    if !b.starts_with(b"MZ") {
        return None;
    }
    let r = Bytes { b, le: true };
    let at = r.u32(0x3C)? as usize;
    if b.get(at..at.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let (machine, characteristics) = (r.u16(at + 4)?, r.u16(at + 22)?);
    let arch = match machine {
        0x14C => "x86".into(),
        0x8664 => "x86_64".into(),
        0xAA64 => "arm64".into(),
        0x1C0 | 0x1C4 => "arm".into(),
        m => format!("machine {m:#x}"),
    };
    Some(Header {
        format: "PE".into(),
        kind: if characteristics & 0x2000 != 0 {
            BinaryKind::Library
        } else {
            BinaryKind::Executable
        },
        ids: vec![BuildId {
            arch,
            id: String::new(),
        }],
    })
}

/// The facts in an `Info.plist` (binary or XML), if it describes a bundle.
pub fn bundle_info(bytes: &[u8]) -> Option<BundleInfo> {
    let p = crate::plist::Plist::parse(bytes).ok()?;
    let get = |k: &str| p.str(k).map(str::to_string);
    let info = BundleInfo {
        path: String::new(),
        name: get("CFBundleDisplayName").or_else(|| get("CFBundleName")),
        bundle_id: get("CFBundleIdentifier"),
        version: get("CFBundleShortVersionString"),
        build: get("CFBundleVersion"),
        min_os: get("MinimumOSVersion").or_else(|| get("LSMinimumSystemVersion")),
        platforms: p.strings("CFBundleSupportedPlatforms"),
        executable: get("CFBundleExecutable"),
    };
    info.executable.is_some().then_some(info)
}

// --- Discovery ----------------------------------------------------------------------

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(p, _)| p)
}

fn ext(path: &str) -> &str {
    let name = file_name(path);
    name.rfind('.').map_or("", |i| &name[i + 1..])
}

fn is_dsym_path(path: &str) -> bool {
    path.contains(".dSYM/")
}

/// Extensions of files that are never binaries: their headers aren't read.
#[rustfmt::skip]
const NOT_BINARY: &[&str] = &[
    // Images, media and fonts.
    "png", "jpg", "jpeg", "gif", "heic", "heif", "webp", "svg", "pdf", "tiff", "tif", "bmp", "ico", "icns", "astc",
    "ktx", "pvr", "mp3", "m4a", "aac", "wav", "caf", "aif", "aiff", "ogg", "flac", "mp4", "mov", "m4v", "avi",
    "webm", "mkv", "ttf", "otf", "ttc", "woff", "woff2",
    // Text, data and interface files.
    "txt", "md", "json", "xml", "yaml", "yml", "csv", "html", "htm", "css", "js", "mjs", "map", "plist", "strings",
    "stringsdict", "xcstrings", "nib", "storyboard", "xib", "car", "mom", "omo", "sqlite", "db", "realm", "pb",
    // Source code.
    "c", "cc", "cpp", "cxx", "h", "hh", "hpp", "m", "mm", "swift", "rs", "go", "py", "rb", "java", "kt", "cs", "ts",
    "sh",
    // Archives that aren't zips.
    "a", "gz", "bz2", "xz", "zst", "lz4", "7z", "rar", "tar",
];

/// Which files to read for [`discover`].
pub fn plan(files: &[PackageFile]) -> Plan {
    let mut plan = Plan::default();
    for (i, f) in files.iter().enumerate() {
        if file_name(&f.path) == "Info.plist" {
            if f.size < 4 * 1024 * 1024 {
                plan.plists.push(i as u32);
            }
            continue;
        }
        // Smaller than any binary's headers.
        if f.size < 64 || plan.headers.len() >= 500_000 {
            continue;
        }
        if !NOT_BINARY.contains(&ext(&f.path).to_ascii_lowercase().as_str()) {
            plan.headers.push(i as u32);
        }
    }
    plan
}

fn category(path: &str) -> FileCategory {
    let lower = path.to_ascii_lowercase();
    let e = ext(&lower);
    if is_dsym_path(path) || lower.contains("bcsymbolmaps/") || e == "bcsymbolmap" {
        return FileCategory::DebugSymbols;
    }
    if lower.contains("/_codesignature/")
        || lower.ends_with("embedded.mobileprovision")
        || lower.ends_with("embedded.provisionprofile")
        || lower.contains("/sc_info/")
        || lower.ends_with("/coderesources")
    {
        return FileCategory::CodeSignature;
    }
    if lower.ends_with(".car") {
        return FileCategory::AssetCatalogs;
    }
    if lower.contains(".nib/") || lower.contains(".storyboardc/") || matches!(e, "nib" | "storyboard" | "xib") {
        return FileCategory::Interface;
    }
    if lower.contains(".mlmodelc/") || matches!(e, "mlmodel" | "mlpackage" | "tflite" | "onnx") {
        return FileCategory::MlModels;
    }
    if lower.contains(".lproj/") || matches!(e, "strings" | "stringsdict" | "xcstrings") {
        return FileCategory::Localization;
    }
    if lower.contains("/headers/")
        || lower.contains("/privateheaders/")
        || lower.contains(".swiftmodule/")
        || lower.contains("/modules/")
        || matches!(
            e,
            "h" | "hpp" | "swiftmodule" | "swiftinterface" | "swiftdoc" | "swiftsourceinfo" | "modulemap" | "a" | "o"
        )
    {
        return FileCategory::DeveloperFiles;
    }
    match e {
        "png" | "jpg" | "jpeg" | "gif" | "heic" | "heif" | "webp" | "pdf" | "svg" | "tiff" | "tif" | "bmp" | "ico"
        | "icns" | "astc" | "ktx" | "pvr" => FileCategory::Images,
        "ttf" | "otf" | "ttc" | "woff" | "woff2" => FileCategory::Fonts,
        "mp3" | "m4a" | "aac" | "wav" | "caf" | "aif" | "aiff" | "ogg" | "flac" | "mp4" | "mov" | "m4v" | "avi"
        | "webm" | "mkv" => FileCategory::Media,
        "html" | "htm" | "js" | "mjs" | "css" | "wasm" => FileCategory::Web,
        "plist" | "json" | "xml" | "yaml" | "yml" | "txt" | "csv" | "db" | "sqlite" | "sqlite3" | "realm" | "bin"
        | "dat" | "pb" | "zip" | "gz" | "lz4" | "zst" | "der" | "cer" | "p12" | "pem" | "cfg" | "conf" | "ini"
        | "momd" | "mom" | "omo" | "car_" | "bundle" | "lottie" | "riv" | "scnassets" | "reality" | "usdz" | "pak" => {
            FileCategory::Data
        }
        _ if lower.contains(".momd/") || lower.contains(".scnassets/") => FileCategory::Data,
        _ => FileCategory::Other,
    }
}

/// Finds the binaries among `files` (those `headers` recognised), pairs them
/// with their debug files, and sizes up everything else by kind of content.
/// `bundles` are the `Info.plist` files read; `container` is "folder" or
/// "zip", `name` the folder's or zip's name.
pub fn discover(
    name: &str,
    container: &str,
    files: &[PackageFile],
    headers: &HashMap<u32, Header>,
    bundles: &HashMap<u32, BundleInfo>,
) -> PackageInfo {
    let index: HashMap<&str, u32> = files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.path.as_str(), i as u32))
        .collect();
    // The bundle a binary is the executable of: an Info.plist beside it, one
    // folder up (a macOS app's Contents/MacOS), or in Resources beside it (a
    // macOS framework's Versions/A).
    let bundle_of = |path: &str| -> Option<BundleInfo> {
        let dir = parent(path);
        let join = |d: &str, rest: &str| {
            if d.is_empty() {
                rest.to_string()
            } else {
                format!("{d}/{rest}")
            }
        };
        [
            join(dir, "Info.plist"),
            join(parent(dir), "Info.plist"),
            join(dir, "Resources/Info.plist"),
        ]
        .iter()
        .find_map(|p| {
            let b = bundles.get(index.get(p.as_str())?)?;
            (b.executable.as_deref() == Some(file_name(path))).then(|| BundleInfo {
                path: parent(p).to_string(),
                ..b.clone()
            })
        })
    };

    let mut binaries = Vec::new();
    let mut debug_files = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let i = i as u32;
        let Some(h) = headers.get(&i) else { continue };
        if h.kind == BinaryKind::Debug {
            debug_files.push(DebugFile {
                index: 0,
                file: i,
                path: f.path.clone(),
                size: f.size,
                compressed_size: f.compressed_size,
                ids: h.ids.clone(),
                binary: None,
            });
        } else {
            binaries.push(PackageBinary {
                index: 0,
                file: i,
                path: f.path.clone(),
                name: file_name(&f.path).to_string(),
                format: h.format.clone(),
                kind: h.kind,
                bundle: bundle_of(&f.path),
                size: f.size,
                compressed_size: f.compressed_size,
                ids: h.ids.clone(),
                debug: None,
            });
        }
    }
    // Nothing but debug files (a zip of dSYMs, say): they are what there is to explore.
    if binaries.is_empty() {
        binaries = debug_files
            .drain(..)
            .map(|d| PackageBinary {
                index: 0,
                file: d.file,
                name: file_name(&d.path).to_string(),
                format: headers[&d.file].format.clone(),
                kind: BinaryKind::Debug,
                bundle: None,
                path: d.path,
                size: d.size,
                compressed_size: d.compressed_size,
                ids: d.ids,
                debug: None,
            })
            .collect();
    }
    // By kind; then a bundle's executable (an app's own) before loose files,
    // the shallowest first, the largest at a depth.
    let depth = |p: &str| p.matches('/').count();
    let key = |b: &PackageBinary| (b.kind, b.bundle.is_none(), depth(&b.path), Reverse(b.size));
    binaries.sort_by(|a, b| key(a).cmp(&key(b)).then_with(|| a.path.cmp(&b.path)));
    for (i, b) in binaries.iter_mut().enumerate() {
        b.index = i as u32;
    }
    for (i, d) in debug_files.iter_mut().enumerate() {
        d.index = i as u32;
    }
    let mut info = PackageInfo {
        kind: container.to_string(),
        name: name.to_string(),
        binaries,
        debug_files,
        files: files.len() as u32,
        ..PackageInfo::default()
    };
    match_debug(&mut info);

    // Sizes by kind of content.
    let kinds = file_categories(files, &info);
    let mut by: HashMap<FileCategory, CategorySize> = HashMap::new();
    let mut refs = Vec::new();
    // Compressed sizes mean something when everything is zipped (not for a folder with a zip in it).
    let zipped = !files.is_empty() && files.iter().all(|f| f.compressed_size.is_some());
    for (i, (f, &cat)) in files.iter().zip(&kinds).enumerate() {
        let i = i as u32;
        let c = by.entry(cat).or_insert(CategorySize {
            category: cat,
            files: 0,
            size: 0,
            compressed_size: zipped.then_some(0),
        });
        c.files += 1;
        c.size += f.size;
        if let (Some(total), Some(cs)) = (&mut c.compressed_size, f.compressed_size) {
            *total += cs;
        }
        if cat == FileCategory::DebugSymbols {
            info.debug_size += f.size;
        } else {
            info.size += f.size;
        }
        refs.push(FileRef {
            file: i,
            path: f.path.clone(),
            size: f.size,
            compressed_size: f.compressed_size,
            category: cat,
        });
    }
    info.compressed_size = zipped.then(|| files.iter().filter_map(|f| f.compressed_size).sum());
    let mut categories: Vec<CategorySize> = by.into_values().collect();
    categories.sort_by(|a, b| b.size.cmp(&a.size).then(a.category.cmp(&b.category)));
    info.categories = categories;
    refs.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
    info.largest = refs
        .into_iter()
        .filter(|r| r.category != FileCategory::DebugSymbols)
        .take(50)
        .collect();

    // Duplicates, among the files a zip directory gives checksums for.
    let mut groups: HashMap<(u32, u64), Vec<&str>> = HashMap::new();
    for (f, &cat) in files.iter().zip(&kinds) {
        if let Some(crc) = f.crc32
            && f.size >= 512
            && cat != FileCategory::DebugSymbols
            && cat != FileCategory::CodeSignature
        {
            groups.entry((crc, f.size)).or_default().push(&f.path);
        }
    }
    let mut dups: Vec<DuplicateGroup> = groups
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|((_, size), mut paths)| {
            paths.sort_unstable();
            DuplicateGroup {
                size,
                wasted: size * (paths.len() as u64 - 1),
                paths: paths.into_iter().map(str::to_string).collect(),
            }
        })
        .collect();
    dups.sort_by(|a, b| b.wasted.cmp(&a.wasted).then_with(|| a.paths.cmp(&b.paths)));
    info.duplicate_bytes = dups.iter().map(|d| d.wasted).sum();
    dups.truncate(50);
    info.duplicates = dups;
    info
}

/// Each file's kind of content, with binaries and debug files as [`discover`] found them.
pub fn file_categories(files: &[PackageFile], info: &PackageInfo) -> Vec<FileCategory> {
    let binaries: HashSet<u32> = info
        .binaries
        .iter()
        .filter(|b| b.kind != BinaryKind::Debug)
        .map(|b| b.file)
        .collect();
    let debug: HashSet<u32> = info
        .debug_files
        .iter()
        .map(|d| d.file)
        .chain(
            info.binaries
                .iter()
                .filter(|b| b.kind == BinaryKind::Debug)
                .map(|b| b.file),
        )
        .collect();
    files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let i = i as u32;
            if debug.contains(&i) || is_dsym_path(&f.path) {
                FileCategory::DebugSymbols
            } else if binaries.contains(&i) {
                FileCategory::Binaries
            } else {
                category(&f.path)
            }
        })
        .collect()
}

/// Pairs binaries with debug files by build ID (UUID), where neither has a pair yet.
pub fn match_debug(info: &mut PackageInfo) {
    let key = |id: &str| id.replace('-', "").to_ascii_lowercase();
    let mut by_id: HashMap<String, u32> = HashMap::new();
    for d in info.debug_files.iter().filter(|d| d.binary.is_none()) {
        for x in d.ids.iter().filter(|x| !x.id.is_empty()) {
            by_id.entry(key(&x.id)).or_insert(d.index);
        }
    }
    for b in 0..info.binaries.len() {
        if info.binaries[b].debug.is_some() {
            continue;
        }
        let found = info.binaries[b]
            .ids
            .iter()
            .filter(|x| !x.id.is_empty())
            .find_map(|x| by_id.get(&key(&x.id)).copied());
        if let Some(d) = found
            && info.debug_files[d as usize].binary.is_none()
        {
            info.binaries[b].debug = Some(d);
            info.debug_files[d as usize].binary = Some(b as u32);
        }
    }
}

/// After binary `index` has been read in full (`bytes`, parsed into `bin`):
/// the build IDs of all its slices (a universal binary's later ones lie past
/// the header read at discovery), and, if nothing pairs with it by build ID,
/// the debug file its `.gnu_debuglink` names.
pub fn update_loaded(info: &mut PackageInfo, index: u32, bytes: &[u8], bin: &crate::Binary) {
    let Some(b) = info.binaries.get_mut(index as usize) else {
        return;
    };
    if let Some(h) = header(bytes)
        && h.ids.iter().any(|x| !x.id.is_empty())
    {
        b.ids = h.ids;
    }
    match_debug(info);
    let b = &info.binaries[index as usize];
    if b.debug.is_some() {
        return;
    }
    // "name (crc 0x…)" for ELF; a PE's PDB path names no file here.
    let Some(link) = bin.summary().debug_link.as_deref() else {
        return;
    };
    let wanted = link.split(" (crc ").next().unwrap_or(link);
    let dir = parent(&b.path);
    let found = info
        .debug_files
        .iter()
        .filter(|d| d.binary.is_none() && file_name(&d.path) == wanted)
        .min_by_key(|d| parent(&d.path) != dir)
        .map(|d| d.index);
    if let Some(d) = found {
        info.binaries[index as usize].debug = Some(d);
        info.debug_files[d as usize].binary = Some(index);
    }
}

/// Parses a binary from a folder: of a universal file, the arm64 slice (else
/// arm64e, else the first). Returns the slice's architecture when it chose one.
pub fn load_binary(data: std::sync::Arc<[u8]>) -> crate::Result<(crate::Binary, Option<String>)> {
    if !crate::Container::is_container(&data) {
        return Ok((crate::Binary::parse(data)?, None));
    }
    let c = crate::Container::parse(data)?;
    let members = c.members();
    let pick = ["arm64", "arm64e"]
        .iter()
        .find_map(|a| members.iter().find(|m| m.arch.as_deref() == Some(a)))
        .or(members.first())
        .ok_or_else(|| crate::Error::new("an empty universal binary"))?;
    let arch = pick.arch.clone();
    Ok((c.open(pick.index)?, arch))
}

/// An owner of code and data (a Swift module, an Objective-C class, a C++
/// namespace…) across the binaries of a folder.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CombinedOwner {
    pub name: String,
    pub kind: crate::GroupKind,
    pub bytes: u64,
    /// (binary, bytes) for each binary it has code or data in, largest first.
    pub binaries: Vec<(String, u64)>,
}

/// Sums owners across binaries' size reports, largest first.
pub fn combine_owners(reports: &[(String, &crate::SizeReport)], top: usize) -> Vec<CombinedOwner> {
    let mut by: HashMap<(crate::GroupKind, &str), CombinedOwner> = HashMap::new();
    for (binary, report) in reports {
        for g in &report.groups {
            let bytes = g.code_bytes + g.data_bytes;
            if bytes == 0 {
                continue;
            }
            let o = by.entry((g.kind, g.name.as_str())).or_insert_with(|| CombinedOwner {
                name: g.name.clone(),
                kind: g.kind,
                bytes: 0,
                binaries: Vec::new(),
            });
            o.bytes += bytes;
            o.binaries.push((binary.clone(), bytes));
        }
    }
    let mut out: Vec<CombinedOwner> = by.into_values().collect();
    for o in &mut out {
        o.binaries.sort_by_key(|b| Reverse(b.1));
    }
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
    out.truncate(top);
    out
}

// --- Folders and zips on disk ------------------------------------------------------

/// Where a file's bytes are.
enum Loc {
    Disk(std::path::PathBuf),
    Zip { zip: usize, entry: usize },
}

/// A folder or zip on disk, looked through for binaries: what's in it, and
/// where to read each file.
pub struct DiskPackage {
    pub info: PackageInfo,
    pub files: Vec<PackageFile>,
    locs: Vec<Loc>,
    zips: Vec<crate::zip::ZipFile>,
}

/// Whether a path is a folder or a zip to look through for binaries, rather than one binary.
pub fn is_package_path(path: &std::path::Path) -> bool {
    if path.is_dir() {
        return true;
    }
    let mut head = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head))
        .is_ok_and(|_| crate::zip::is_zip(&head))
}

/// Folders and files a walk leaves out: version control and Finder litter.
fn ignored(path: &str) -> bool {
    path.split('/')
        .any(|c| c == "__MACOSX" || c == ".git" || c == ".DS_Store")
}

/// Zips inside zips (and inside those) are opened up to this deep.
const NESTED_ZIPS: usize = 2;

impl DiskPackage {
    /// Opens a folder or a zip (an `.ipa`, a zipped build…), and the zips inside it.
    pub fn open(path: &std::path::Path) -> Result<DiskPackage, String> {
        let name = path
            .file_name()
            .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        let mut pkg = DiskPackage {
            info: PackageInfo::default(),
            files: Vec::new(),
            locs: Vec::new(),
            zips: Vec::new(),
        };
        let container = if path.is_dir() {
            pkg.add_folder(path, path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            "folder"
        } else {
            pkg.add_zip(crate::zip::ZipFile::open(path)?, "");
            "zip"
        };
        let mut headers_by_path: HashMap<String, Header> = HashMap::new();
        let mut not_zips: HashSet<String> = HashSet::new();
        for depth in 0..=NESTED_ZIPS {
            let mut zips = Vec::new();
            for i in plan(&pkg.files).headers {
                let path = pkg.files[i as usize].path.clone();
                if headers_by_path.contains_key(&path) || not_zips.contains(&path) {
                    continue;
                }
                let Ok(bytes) = pkg.read_prefix(i, HEADER_BYTES) else {
                    continue;
                };
                if crate::zip::is_zip(&bytes) {
                    if depth < NESTED_ZIPS {
                        zips.push(i);
                    }
                    not_zips.insert(path);
                } else if let Some(h) = header(&bytes) {
                    headers_by_path.insert(path, h);
                }
            }
            if zips.is_empty() {
                break;
            }
            pkg.expand(&zips);
        }
        let headers: HashMap<u32, Header> = pkg
            .files
            .iter()
            .enumerate()
            .filter_map(|(i, f)| Some((i as u32, headers_by_path.get(&f.path)?.clone())))
            .collect();
        let mut bundles = HashMap::new();
        for i in plan(&pkg.files).plists {
            if let Ok(bytes) = pkg.read(i)
                && let Some(b) = bundle_info(&bytes)
            {
                bundles.insert(i, b);
            }
        }
        pkg.info = discover(&name, container, &pkg.files, &headers, &bundles);
        Ok(pkg)
    }

    fn add_folder(&mut self, root: &std::path::Path, dir: &std::path::Path) -> std::io::Result<()> {
        let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let kind = entry.file_type()?;
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if kind.is_symlink() || ignored(&rel) || self.files.len() >= 2_000_000 {
                continue;
            }
            if kind.is_dir() {
                self.add_folder(root, &path)?;
            } else if kind.is_file() {
                self.files.push(PackageFile {
                    path: rel,
                    size: entry.metadata()?.len(),
                    compressed_size: None,
                    crc32: None,
                });
                self.locs.push(Loc::Disk(path));
            }
        }
        Ok(())
    }

    fn add_zip(&mut self, zip: crate::zip::ZipFile, prefix: &str) {
        let z = self.zips.len();
        for (e, entry) in zip.entries.iter().enumerate() {
            if entry.is_dir || entry.is_symlink || ignored(&entry.name) {
                continue;
            }
            self.files.push(PackageFile {
                path: format!("{prefix}{}", entry.name),
                size: entry.size,
                compressed_size: Some(entry.compressed_size),
                crc32: Some(entry.crc32),
            });
            self.locs.push(Loc::Zip { zip: z, entry: e });
        }
        self.zips.push(zip);
    }

    /// Replaces zip files by their contents (`<zip path>/<entry>`); a zip
    /// that can't be read stays a file.
    fn expand(&mut self, zips: &[u32]) {
        let mut zips = zips.to_vec();
        zips.sort_unstable_by(|a, b| b.cmp(a));
        for i in zips {
            let opened = match &self.locs[i as usize] {
                Loc::Disk(p) => crate::zip::ZipFile::open(p),
                Loc::Zip { .. } if self.files[i as usize].size > 2 << 30 => Err("too big to open in memory".into()),
                Loc::Zip { .. } => self.read(i).and_then(crate::zip::ZipFile::from_bytes),
            };
            let Ok(zip) = opened else { continue };
            let prefix = format!("{}/", self.files[i as usize].path);
            self.files.remove(i as usize);
            self.locs.remove(i as usize);
            self.add_zip(zip, &prefix);
        }
    }

    /// The folder's sizes, to compare with another build: every file, and the
    /// first `max` binaries (each read with its debug file, then handed to
    /// `prepare`: to demangle its Swift names, say).
    pub fn snapshot(&mut self, max: usize, mut prepare: impl FnMut(&mut crate::Binary)) -> crate::diff::FolderSnapshot {
        let files = self
            .files
            .iter()
            .zip(file_categories(&self.files, &self.info))
            .map(|(f, category)| crate::diff::FileBytes {
                path: f.path.clone(),
                bytes: f.size,
                category,
            })
            .collect();
        let mut binaries = Vec::new();
        for i in 0..self.info.binaries.len().min(max) {
            let b = self.info.binaries[i].clone();
            let Ok(data) = self.read_shared(b.file) else { continue };
            let Ok((mut bin, _)) = load_binary(data.clone()) else {
                continue;
            };
            update_loaded(&mut self.info, i as u32, &data, &bin);
            if let Some(d) = self.info.binaries[i].debug {
                let debug = self.info.debug_files[d as usize].clone();
                if let Ok(bytes) = self.read_shared(debug.file) {
                    let _ = bin.attach_debug_file(&debug.path, bytes);
                }
            }
            prepare(&mut bin);
            binaries.push(crate::diff::BinarySnapshot {
                path: b.path.clone(),
                snapshot: bin.size_snapshot(&b.name),
            });
        }
        crate::diff::FolderSnapshot {
            name: self.info.name.clone(),
            files,
            binaries,
        }
    }

    /// A file's bytes.
    pub fn read(&mut self, file: u32) -> Result<Vec<u8>, String> {
        self.read_prefix(file, u64::MAX)
    }

    pub fn read_prefix(&mut self, file: u32, max: u64) -> Result<Vec<u8>, String> {
        match self.locs.get(file as usize).ok_or("no such file")? {
            Loc::Disk(path) => {
                let mut out = Vec::new();
                let f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
                std::io::Read::read_to_end(&mut std::io::Read::take(f, max), &mut out)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                Ok(out)
            }
            &Loc::Zip { zip, entry } => {
                let e = self.zips[zip].entries[entry].clone();
                self.zips[zip].read_prefix(&e, max)
            }
        }
    }

    /// A file's bytes as shared storage for [`crate::Binary::parse`]: files
    /// on disk are read straight into it.
    /// Links the DWARF a Mach-O binary's debug map names (when it has no
    /// DWARF yet) from the object files and static libraries in this folder,
    /// or where they were when it was linked. `None` when there is nothing
    /// to link.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn attach_debug_map(
        &self,
        bin: &mut crate::Binary,
    ) -> Option<crate::Result<crate::dwarf::debugmap::DebugMapReport>> {
        if bin.debug_info().is_some() || bin.debug_map().is_empty() {
            return None;
        }
        let mut folders: Vec<std::path::PathBuf> = self
            .locs
            .iter()
            .filter_map(|l| match l {
                Loc::Disk(p) if p.extension().is_some_and(|e| e == "o" || e == "a") => {
                    p.parent().map(std::path::Path::to_path_buf)
                }
                _ => None,
            })
            .collect();
        folders.sort();
        folders.dedup();
        Some(bin.attach_debug_map_from_disk(&folders))
    }

    pub fn read_shared(&mut self, file: u32) -> Result<std::sync::Arc<[u8]>, String> {
        if let Some(Loc::Disk(path)) = self.locs.get(file as usize) {
            return crate::read_file(path).map_err(|e| format!("{}: {e}", path.display()));
        }
        Ok(self.read(file)?.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, size: u64) -> PackageFile {
        PackageFile {
            path: path.into(),
            size,
            compressed_size: None,
            crc32: None,
        }
    }

    fn head(format: &str, kind: BinaryKind, id: &str) -> Header {
        Header {
            format: format.into(),
            kind,
            ids: vec![BuildId {
                arch: "arm64".into(),
                id: id.into(),
            }],
        }
    }

    #[test]
    fn binaries_are_found_by_their_headers() {
        let files = vec![
            file("Payload/Shop.app/Info.plist", 2000),
            file("Payload/Shop.app/ShopApp", 50_000_000),
            file("Payload/Shop.app/Assets.car", 9_000_000),
            file("Payload/Shop.app/en.lproj/Localizable.strings", 30_000),
            file("Payload/Shop.app/Frameworks/Kit.framework/Kit", 8_000_000),
            file("Payload/Shop.app/Frameworks/Kit.framework/Info.plist", 900),
            file("Payload/Shop.app/Frameworks/Kit.framework/Headers/Kit.h", 4000),
            file("Payload/Shop.app/Frameworks/libswiftCore.dylib", 6_000_000),
            file("Payload/Shop.app/PlugIns/Widget.appex/Widget", 1_000_000),
            file("Payload/Shop.app/Bundle.bundle/image.png", 50_000),
            file("Payload/Shop.app/_CodeSignature/CodeResources", 30_000),
            file("Shop.app.dSYM/Contents/Resources/DWARF/ShopApp", 400_000_000),
            file("lib/arm64-v8a/libgame.so", 3_000_000),
            file("tools/helper.exe", 200_000),
        ];
        let p = plan(&files);
        assert_eq!(p.plists, [0, 5]);
        for (i, f) in files.iter().enumerate() {
            let probed = p.headers.contains(&(i as u32));
            let never = [".car", ".strings", ".h", ".png", ".plist"]
                .iter()
                .any(|e| f.path.ends_with(e));
            assert_eq!(probed, !never, "{}", f.path);
        }
        let mut headers = HashMap::new();
        headers.insert(1, head("Mach-O", BinaryKind::Executable, "AAAA"));
        headers.insert(4, head("Mach-O", BinaryKind::Library, "BBBB"));
        headers.insert(7, head("Mach-O", BinaryKind::Library, "CCCC"));
        headers.insert(8, head("Mach-O", BinaryKind::Executable, "DDDD"));
        headers.insert(11, head("Mach-O", BinaryKind::Debug, "aaaa"));
        headers.insert(12, head("ELF", BinaryKind::Library, "0123"));
        headers.insert(13, head("PE", BinaryKind::Executable, ""));
        let mut bundles = HashMap::new();
        bundles.insert(
            0,
            BundleInfo {
                bundle_id: Some("com.example.shop".into()),
                executable: Some("ShopApp".into()),
                ..BundleInfo::default()
            },
        );
        let info = discover("Shop.ipa", "zip", &files, &headers, &bundles);
        assert_eq!(info.kind, "zip");
        let found: Vec<(&str, BinaryKind)> = info.binaries.iter().map(|b| (b.name.as_str(), b.kind)).collect();
        // Executables first, the app's own (it has a bundle) leading, then the
        // shallowest; then libraries.
        assert_eq!(
            found,
            [
                ("ShopApp", BinaryKind::Executable),
                ("helper.exe", BinaryKind::Executable),
                ("Widget", BinaryKind::Executable),
                ("libgame.so", BinaryKind::Library),
                ("libswiftCore.dylib", BinaryKind::Library),
                ("Kit", BinaryKind::Library),
            ]
        );
        let shop = &info.binaries[0];
        assert_eq!(
            shop.bundle.as_ref().and_then(|b| b.bundle_id.as_deref()),
            Some("com.example.shop")
        );
        assert_eq!(shop.bundle.as_ref().map(|b| b.path.as_str()), Some("Payload/Shop.app"));
        // The Info.plist beside Kit names no executable Kit: no bundle.
        assert!(info.binaries[5].bundle.is_none());
        // Paired by UUID, whatever the case and the names.
        assert_eq!(shop.debug, Some(0));
        assert_eq!(info.debug_files[0].binary, Some(0));
        assert!(info.binaries[1..].iter().all(|b| b.debug.is_none()));
        let cat = |c: FileCategory| info.categories.iter().find(|x| x.category == c).map(|x| x.size);
        assert_eq!(cat(FileCategory::Binaries), Some(68_200_000));
        assert_eq!(cat(FileCategory::AssetCatalogs), Some(9_000_000));
        assert_eq!(cat(FileCategory::DeveloperFiles), Some(4000));
        assert_eq!(info.debug_size, 400_000_000);
        assert_eq!(info.largest[0].path, "Payload/Shop.app/ShopApp");
    }

    #[test]
    fn debug_files_alone_are_the_binaries() {
        let files = vec![file("A.dSYM/Contents/Resources/DWARF/A", 5000), file("B.debug", 7000)];
        let mut headers = HashMap::new();
        headers.insert(0, head("Mach-O", BinaryKind::Debug, "AAAA"));
        headers.insert(1, head("ELF", BinaryKind::Debug, "bbbb"));
        let info = discover("dSYMs.zip", "zip", &files, &headers, &HashMap::new());
        assert_eq!(info.binaries.len(), 2);
        assert!(info.binaries.iter().all(|b| b.kind == BinaryKind::Debug));
        assert!(info.debug_files.is_empty());
        assert_eq!((info.size, info.debug_size), (0, 12_000));
    }
}
