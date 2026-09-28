//! WebAssembly bindings for binviz.
//!
//! A [`Session`] holds one opened file. Results are plain JS objects; every
//! `u64` (addresses, offsets, sizes) arrives as a `BigInt` so large addresses
//! survive intact, while counts and indices are ordinary numbers.

use binviz::{Annotation, AttributionMode, Binary, Container, HitKind, SymbolQuery, Target};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

fn to_js<T: Serialize + ?Sized>(value: &T) -> Result<JsValue, JsError> {
    let serializer = serde_wasm_bindgen::Serializer::new()
        .serialize_large_number_types_as_bigints(true)
        .serialize_maps_as_objects(true);
    value.serialize(&serializer).map_err(|e| JsError::new(&e.to_string()))
}

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn attribution_mode(mode: &str) -> AttributionMode {
    if mode == "unit" {
        AttributionMode::Unit
    } else {
        AttributionMode::File
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Opened<'a> {
    Binary {
        name: &'a str,
        summary: &'a binviz::Summary,
    },
    Container {
        name: &'a str,
        info: &'a binviz::ContainerInfo,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Resolved {
    /// "address" or "offset".
    kind: &'static str,
    value: u64,
    label: String,
}

#[wasm_bindgen]
#[derive(Default)]
pub struct Session {
    name: String,
    container: Option<Container>,
    binary: Option<Binary>,
    /// A buffer being filled by JavaScript (see `beginInput`).
    input: Option<std::sync::Arc<[std::mem::MaybeUninit<u8>]>>,
    /// A folder of binaries being browsed (its current binary is `binary`).
    package: Option<PackageState>,
    /// The build to compare sizes with, reduced to a snapshot.
    baseline: Option<Baseline>,
    /// A binary of the earlier build being read for its snapshot.
    baseline_bin: Option<Binary>,
    /// The open folder's snapshot, being built to compare with a folder baseline.
    current_snapshot: Option<binviz::diff::FolderSnapshot>,
    /// Object files gathered to link a debug map's DWARF, by file name (see `debugMapAdd`).
    debug_objects: std::collections::HashMap<String, Vec<u8>>,
    /// The table file text is read with (see `tableSet`).
    table: Option<binviz::tables::Table>,
    /// A patched version of the open file: from a patch file, bytes edited, or both.
    patched: Option<Patched>,
    /// The earlier binary compared with (see `baselineBinary`), whole: its functions are compared too.
    baseline_full: Option<Binary>,
    /// The last function diff: what it compared (`with`, and both fingerprints), and the result.
    function_diff: Option<(String, binviz::fndiff::FunctionDiff)>,
}

struct Patched {
    /// The file it patches (its fingerprint): another file open means no patch.
    fingerprint: String,
    /// The patched file's bytes.
    target: Vec<u8>,
    /// What the patch file said (its bytes moved to `target`), when there was one.
    applied: Option<binviz::patch::Applied>,
    /// Its name.
    name: Option<String>,
    /// The patched file read, to compare its functions (read again after edits).
    parsed: Option<Binary>,
}

/// A change a patch makes, placed, with its bytes before and after.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PatchRow {
    #[serde(flatten)]
    placed: binviz::patch::Placed,
    before: Vec<u8>,
    after: Vec<u8>,
    /// Both read with the table file, when there is one.
    before_text: Option<String>,
    after_text: Option<String>,
}

/// Where a patch stands: what the patch file said, and the changes.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PatchState<'a> {
    name: Option<&'a str>,
    applied: Option<&'a binviz::patch::Applied>,
    changes: Vec<PatchRow>,
    /// All the runs of changes (only the first `limit` are placed).
    total: u32,
    differ: u64,
    target_size: u64,
}

enum Baseline {
    Binary(binviz::diff::SizeSnapshot),
    Folder(binviz::diff::FolderSnapshot),
}

/// A folder's files as snapshot entries, with their kinds of content.
fn file_bytes(files: JsValue, info: &binviz::package::PackageInfo) -> Result<Vec<binviz::diff::FileBytes>, JsError> {
    let files: Vec<binviz::package::PackageFile> = serde_wasm_bindgen::from_value(files).map_err(err)?;
    let kinds = binviz::package::file_categories(&files, info);
    Ok(files
        .into_iter()
        .zip(kinds)
        .map(|(f, category)| binviz::diff::FileBytes {
            path: f.path,
            bytes: f.size,
            category,
        })
        .collect())
}

struct PackageState {
    info: binviz::package::PackageInfo,
    /// Binaries loaded but not current, by binary index.
    loaded: std::collections::HashMap<u32, Binary>,
    current: Option<u32>,
}

// --- Zip archives (read by the worker from a Blob) ---------------------------

/// The central directory's location, from an archive's last bytes.
#[wasm_bindgen(js_name = zipFindDirectory)]
pub fn zip_find_directory(tail: &[u8]) -> Result<JsValue, JsError> {
    to_js(&binviz::zip::find_directory(tail).map_err(err)?)
}

/// The directory's location from a zip64 end record.
#[wasm_bindgen(js_name = zipZip64Directory)]
pub fn zip_zip64_directory(record: &[u8]) -> Result<JsValue, JsError> {
    to_js(&binviz::zip::zip64_directory(record).map_err(err)?)
}

/// The entries of a central directory.
#[wasm_bindgen(js_name = zipParseDirectory)]
pub fn zip_parse_directory(cd: &[u8]) -> Result<JsValue, JsError> {
    to_js(&binviz::zip::parse_directory(cd).map_err(err)?)
}

/// Inflates up to `max` bytes from a Zstandard-compressed zip entry.
#[wasm_bindgen(js_name = zipDecompressZstandard)]
pub fn zip_decompress_zstandard(data: &[u8], max: u64) -> Result<Vec<u8>, JsError> {
    binviz::zip::decompress_zstandard(data, max).map_err(err)
}

// --- CD images (read by the worker a sector at a time) --------------------------------

/// How a CD image lays its sectors out (`{sector, data}`), from its first
/// bytes (`discPrefix` of them), or null if it isn't one.
#[wasm_bindgen(js_name = discLayout)]
pub fn disc_layout(prefix: &[u8]) -> Result<JsValue, JsError> {
    match binviz::disc::layout(prefix) {
        Some(l) => to_js(&l),
        None => Ok(JsValue::NULL),
    }
}

#[wasm_bindgen(js_name = discPrefix)]
pub fn disc_prefix() -> u32 {
    binviz::disc::PREFIX as u32
}

/// The volume's name and root folder, from logical sector 16: `[name, {path, lba, size, dir}]`.
#[wasm_bindgen(js_name = discRoot)]
pub fn disc_root(pvd: &[u8]) -> Result<JsValue, JsError> {
    match binviz::disc::root(pvd) {
        Some(r) => to_js(&r),
        None => Ok(JsValue::NULL),
    }
}

/// The files and folders in a folder's sectors.
#[wasm_bindgen(js_name = discRecords)]
pub fn disc_records(bytes: &[u8], parent: &str) -> Result<JsValue, JsError> {
    to_js(&binviz::disc::records(bytes, parent))
}

/// What `SYSTEM.CNF` boots.
#[wasm_bindgen(js_name = discBoot)]
pub fn disc_boot(cnf: &[u8]) -> Option<String> {
    binviz::disc::boot_path(cnf)
}

/// Files, what the disc boots first.
#[wasm_bindgen(js_name = discSort)]
pub fn disc_sort(files: JsValue, boot: Option<String>) -> Result<JsValue, JsError> {
    let mut files: Vec<binviz::disc::DiscFile> = serde_wasm_bindgen::from_value(files).map_err(err)?;
    binviz::disc::sort(&mut files, boot.as_deref());
    to_js(&files)
}

// --- Crash reports ----------------------------------------------------------------

/// A crash report read from text (Apple .crash or .ips, an Android
/// tombstone, a stack trace), or null if the text is none of those.
#[wasm_bindgen(js_name = crashParse)]
pub fn crash_parse(text: &str) -> Result<JsValue, JsError> {
    match binviz::crash::parse(text) {
        Some(r) => to_js(&r),
        None => Ok(JsValue::NULL),
    }
}

// --- Folders of binaries: discovery (see `binviz::package`) --------------------

/// What to read of a folder's files (`[{path, size, compressedSize?, crc32?}]`):
/// `{headers, plists}`, file indices.
#[wasm_bindgen(js_name = packagePlan)]
pub fn package_plan(files: JsValue) -> Result<JsValue, JsError> {
    let files: Vec<binviz::package::PackageFile> = serde_wasm_bindgen::from_value(files).map_err(err)?;
    to_js(&binviz::package::plan(&files))
}

/// What a file's first bytes say it is, if it is a binary.
#[wasm_bindgen(js_name = packageHeader)]
pub fn package_header(prefix: &[u8]) -> Result<JsValue, JsError> {
    match binviz::package::header(prefix) {
        Some(h) => to_js(&h),
        None => Ok(JsValue::NULL),
    }
}

/// The bundle facts in an `Info.plist`.
#[wasm_bindgen(js_name = packageBundle)]
pub fn package_bundle(bytes: &[u8]) -> Result<JsValue, JsError> {
    match binviz::package::bundle_info(bytes) {
        Some(b) => to_js(&b),
        None => Ok(JsValue::NULL),
    }
}

/// Finds the binaries among `files`, given `headers` and `bundles` as
/// `[[fileIndex, value], …]`, and pairs them with their debug files.
#[wasm_bindgen(js_name = packageDiscover)]
pub fn package_discover(
    name: String,
    container: String,
    files: JsValue,
    headers: JsValue,
    bundles: JsValue,
) -> Result<JsValue, JsError> {
    let files: Vec<binviz::package::PackageFile> = serde_wasm_bindgen::from_value(files).map_err(err)?;
    let headers: Vec<(u32, binviz::package::Header)> = serde_wasm_bindgen::from_value(headers).map_err(err)?;
    let bundles: Vec<(u32, binviz::package::BundleInfo)> = serde_wasm_bindgen::from_value(bundles).map_err(err)?;
    to_js(&binviz::package::discover(
        &name,
        &container,
        &files,
        &headers.into_iter().collect(),
        &bundles.into_iter().collect(),
    ))
}

/// A folder's binaries as candidates for a crash report's images.
fn crash_candidates(info: &binviz::package::PackageInfo) -> Vec<binviz::crash::Candidate<'_>> {
    info.binaries
        .iter()
        .map(|b| binviz::crash::Candidate {
            binary: b.index,
            name: &b.name,
            ids: b.ids.iter().map(|x| x.id.as_str()).collect(),
        })
        .collect()
}

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Session {
        Session::default()
    }

    /// Opens a file. Returns `{kind: "binary", summary}` or, for universal
    /// binaries and archives, `{kind: "container", info}` (then call `openMember`).
    pub fn open(&mut self, name: String, bytes: Vec<u8>) -> Result<JsValue, JsError> {
        self.open_data(name, bytes.into())
    }

    /// Allocates `len` bytes inside WebAssembly memory for the next file and
    /// returns their address, so JavaScript can copy the file straight in (in
    /// chunks) instead of passing one big array. Drops the open file first, to
    /// make room. Finish with `openInput` or `attachInput`.
    #[wasm_bindgen(js_name = beginInput)]
    pub fn begin_input(&mut self, len: usize) -> *mut u8 {
        self.binary = None;
        self.container = None;
        self.package = None;
        self.input = None;
        let mut buf = std::sync::Arc::<[u8]>::new_uninit_slice(len);
        let ptr = std::sync::Arc::get_mut(&mut buf).expect("new buffer").as_mut_ptr() as *mut u8;
        self.input = Some(buf);
        ptr
    }

    /// Allocates a buffer for a companion debug file, keeping the open binary.
    #[wasm_bindgen(js_name = beginDebugInput)]
    pub fn begin_debug_input(&mut self, len: usize) -> *mut u8 {
        let mut buf = std::sync::Arc::<[u8]>::new_uninit_slice(len);
        let ptr = std::sync::Arc::get_mut(&mut buf).expect("new buffer").as_mut_ptr() as *mut u8;
        self.input = Some(buf);
        ptr
    }

    fn take_input(&mut self) -> Result<std::sync::Arc<[u8]>, JsError> {
        let buf = self.input.take().ok_or_else(|| JsError::new("no input buffer"))?;
        // SAFETY: JavaScript filled every byte of the buffer before calling us.
        Ok(unsafe { buf.assume_init() })
    }

    /// Opens the file copied in after `beginInput`.
    #[wasm_bindgen(js_name = openInput)]
    pub fn open_input(&mut self, name: String) -> Result<JsValue, JsError> {
        let data = self.take_input()?;
        self.open_data(name, data)
    }

    /// Attaches the debug file copied in after `beginDebugInput`.
    #[wasm_bindgen(js_name = attachInput)]
    pub fn attach_input(&mut self, name: String) -> Result<JsValue, JsError> {
        let data = self.take_input()?;
        let b = self.binary.as_mut().ok_or_else(|| JsError::new("no binary open"))?;
        b.attach_debug_file(&name, data).map_err(err)?;
        self.opened()
    }

    fn open_data(&mut self, name: String, data: std::sync::Arc<[u8]>) -> Result<JsValue, JsError> {
        self.binary = None;
        self.container = None;
        self.package = None;
        self.name = name;
        if Container::is_container(&data) {
            let c = Container::parse(data).map_err(err)?;
            self.container = Some(c);
            let info = self.container.as_ref().map(|c| c.info()).expect("just set");
            return to_js(&Opened::Container { name: &self.name, info });
        }
        self.binary = Some(Binary::parse(data).map_err(err)?);
        self.opened()
    }

    // --- Folders of binaries -----------------------------------------------------

    /// Starts browsing a folder of binaries (an info from `packageDiscover`),
    /// dropping what was open. Load its binaries with `packageLoad`.
    #[wasm_bindgen(js_name = packageOpen)]
    pub fn package_open(&mut self, info: JsValue) -> Result<(), JsError> {
        let info: binviz::package::PackageInfo = serde_wasm_bindgen::from_value(info).map_err(err)?;
        self.binary = None;
        self.container = None;
        self.input = None;
        self.package = Some(PackageState {
            info,
            loaded: Default::default(),
            current: None,
        });
        Ok(())
    }

    fn pkg(&mut self) -> Result<&mut PackageState, JsError> {
        self.package.as_mut().ok_or_else(|| JsError::new("no folder open"))
    }

    #[wasm_bindgen(js_name = packageInfo)]
    pub fn package_info(&self) -> Result<JsValue, JsError> {
        match &self.package {
            Some(p) => to_js(&p.info),
            None => Ok(JsValue::NULL),
        }
    }

    /// Parses binary `index` from the bytes copied in after `beginDebugInput`,
    /// and makes it current (or keeps it aside with `current: false`).
    #[wasm_bindgen(js_name = packageLoad)]
    pub fn package_load(&mut self, index: u32, current: bool) -> Result<JsValue, JsError> {
        let data = self.take_input()?;
        let (bin, arch) = binviz::package::load_binary(data.clone()).map_err(err)?;
        let p = self.package.as_mut().ok_or_else(|| JsError::new("no folder open"))?;
        let path = p
            .info
            .binaries
            .get(index as usize)
            .ok_or_else(|| JsError::new("no such binary"))?
            .path
            .clone();
        let label = format!(
            "{} › {path}{}",
            p.info.name,
            arch.map(|a| format!(" [{a}]")).unwrap_or_default()
        );
        // Every slice's UUID, and a debug file the binary's debug link names.
        binviz::package::update_loaded(&mut p.info, index, &data, &bin);
        if !current {
            let summary = to_js(bin.summary());
            p.loaded.insert(index, bin);
            return summary;
        }
        if let (Some(prev), Some(old)) = (p.current, self.binary.take()) {
            p.loaded.insert(prev, old);
        }
        p.current = Some(index);
        self.binary = Some(bin);
        self.name = label;
        self.opened()
    }

    /// Makes an already loaded binary current; `undefined` if it isn't loaded.
    #[wasm_bindgen(js_name = packageSelect)]
    pub fn package_select(&mut self, index: u32) -> Result<JsValue, JsError> {
        let p = self.package.as_mut().ok_or_else(|| JsError::new("no folder open"))?;
        if p.current == Some(index) {
            return self.opened();
        }
        let Some(bin) = p.loaded.remove(&index) else {
            return Ok(JsValue::UNDEFINED);
        };
        if let (Some(prev), Some(old)) = (p.current, self.binary.take()) {
            p.loaded.insert(prev, old);
        }
        p.current = Some(index);
        let path = p
            .info
            .binaries
            .get(index as usize)
            .map(|b| b.path.clone())
            .unwrap_or_default();
        self.name = format!("{} › {path}", p.info.name);
        self.binary = Some(bin);
        self.opened()
    }

    /// Frees a loaded binary that isn't current.
    #[wasm_bindgen(js_name = packageUnload)]
    pub fn package_unload(&mut self, index: u32) -> Result<(), JsError> {
        self.pkg()?.loaded.remove(&index);
        Ok(())
    }

    fn package_binary(&mut self, index: u32) -> Result<&mut Binary, JsError> {
        let p = self.package.as_mut().ok_or_else(|| JsError::new("no folder open"))?;
        if p.current == Some(index) {
            return self.binary.as_mut().ok_or_else(|| JsError::new("no binary open"));
        }
        p.loaded
            .get_mut(&index)
            .ok_or_else(|| JsError::new("that binary isn't loaded"))
    }

    /// Attaches the debug file copied in after `beginDebugInput` to binary `index`.
    #[wasm_bindgen(js_name = packageAttach)]
    pub fn package_attach(&mut self, index: u32, name: String) -> Result<JsValue, JsError> {
        let data = self.take_input()?;
        let b = self.package_binary(index)?;
        b.attach_debug_file(&name, data).map_err(err)?;
        to_js(b.summary())
    }

    /// The object files binary `index`'s debug map names, when it has no
    /// DWARF yet (a Mach-O binary linked without dsymutil); else none.
    #[wasm_bindgen(js_name = packageDebugMapNeeded)]
    pub fn package_debug_map_needed(&mut self, index: u32) -> Result<JsValue, JsError> {
        let b = self.package_binary(index)?;
        if b.debug_info().is_some() {
            return to_js(&Vec::<binviz::dwarf::debugmap::DebugMapObject>::new());
        }
        to_js(&b.debug_map())
    }

    /// Links binary `index`'s debug map from the objects added with `debugMapAdd`.
    #[wasm_bindgen(js_name = packageDebugMapLink)]
    pub fn package_debug_map_link(&mut self, index: u32) -> Result<JsValue, JsError> {
        let objects = std::mem::take(&mut self.debug_objects);
        let b = self.package_binary(index)?;
        let report = b
            .attach_debug_map(&mut |o| Ok(objects.get(o.file_name()).cloned()))
            .map_err(err)?;
        to_js(&report)
    }

    /// The size report of binary `index` (loaded or current).
    #[wasm_bindgen(js_name = packageSizeReport)]
    pub fn package_size_report(&mut self, index: u32, top: u32) -> Result<JsValue, JsError> {
        let b = self.package_binary(index)?;
        to_js(&b.size_report(top as usize))
    }

    /// The folder's binaries (by index) a crash report's frames are in: load
    /// them (with their debug files) before `symbolicate`.
    #[wasm_bindgen(js_name = crashNeeds)]
    pub fn crash_needs(&self, text: &str) -> Result<Vec<u32>, JsError> {
        let report = binviz::crash::parse(text).ok_or_else(|| JsError::new("not a crash report"))?;
        let Some(p) = &self.package else {
            return Ok(Vec::new());
        };
        let binviz::crash::Matches { pairs, .. } = binviz::crash::match_images(&report, &crash_candidates(&p.info));
        let mut need: Vec<u32> = pairs.into_iter().map(|(_, b)| b).collect();
        need.sort_unstable();
        need.dedup();
        Ok(need)
    }

    /// Symbolicates a crash report with the open binary, or the open folder's
    /// loaded binaries (see `crashNeeds`); with neither, it comes back as the
    /// report says it.
    pub fn symbolicate(&self, text: &str) -> Result<JsValue, JsError> {
        use binviz::crash::{Candidate, Found, Matches, match_images, parse, symbolicate};
        let report = parse(text).ok_or_else(|| JsError::new("not a crash report"))?;
        let mut found = Vec::new();
        let mut notes = Vec::new();
        if let Some(p) = &self.package {
            let Matches { pairs, notes: n } = match_images(&report, &crash_candidates(&p.info));
            notes = n;
            for (image, b) in pairs {
                let bin = if p.current == Some(b) {
                    self.binary.as_ref()
                } else {
                    p.loaded.get(&b)
                };
                if let Some(bin) = bin {
                    found.push(Found { image, binary: b, bin });
                }
            }
        } else if let Some(bin) = &self.binary {
            let name = self.name.rsplit(['/', '\\']).next().unwrap_or(&self.name);
            let id = bin.summary().build_id.clone();
            let Matches { pairs, notes: n } = match_images(
                &report,
                &[Candidate {
                    binary: 0,
                    name,
                    ids: id.iter().map(String::as_str).collect(),
                }],
            );
            notes = n;
            found = pairs
                .into_iter()
                .map(|(image, binary)| Found { image, binary, bin })
                .collect();
        }
        to_js(&symbolicate(&report, &found, &notes))
    }

    // --- Comparing sizes with another build ----------------------------------------

    /// Makes the binary copied in after `beginDebugInput` the build to compare with.
    #[wasm_bindgen(js_name = baselineBinary)]
    pub fn baseline_binary(&mut self, name: &str) -> Result<(), JsError> {
        let data = self.take_input()?;
        let (bin, _) = binviz::package::load_binary(data).map_err(err)?;
        self.baseline = Some(Baseline::Binary(bin.size_snapshot(name)));
        self.baseline_full = Some(bin);
        Ok(())
    }

    /// Starts a folder as the build to compare with: its files (`[{path,
    /// size, …}]`) and what `packageDiscover` found in them. Add its binaries
    /// with `baselineLoad`, `baselineAttach` and `baselineAdd`.
    #[wasm_bindgen(js_name = baselineFolder)]
    pub fn baseline_folder(&mut self, files: JsValue, info: JsValue) -> Result<(), JsError> {
        let info: binviz::package::PackageInfo = serde_wasm_bindgen::from_value(info).map_err(err)?;
        self.baseline = Some(Baseline::Folder(binviz::diff::FolderSnapshot {
            name: info.name.clone(),
            files: file_bytes(files, &info)?,
            binaries: Vec::new(),
        }));
        Ok(())
    }

    /// Reads a binary of the earlier folder (copied in after `beginDebugInput`).
    #[wasm_bindgen(js_name = baselineLoad)]
    pub fn baseline_load(&mut self) -> Result<(), JsError> {
        let data = self.take_input()?;
        self.baseline_bin = Some(binviz::package::load_binary(data).map_err(err)?.0);
        Ok(())
    }

    /// Attaches its debug file (copied in after `beginDebugInput`).
    #[wasm_bindgen(js_name = baselineAttach)]
    pub fn baseline_attach(&mut self, name: &str) -> Result<(), JsError> {
        let data = self.take_input()?;
        let bin = self
            .baseline_bin
            .as_mut()
            .ok_or_else(|| JsError::new("no binary read"))?;
        bin.attach_debug_file(name, data).map_err(err)
    }

    /// Adds the binary read to the earlier folder's snapshot, and lets it go.
    #[wasm_bindgen(js_name = baselineAdd)]
    pub fn baseline_add(&mut self, path: String, name: &str) -> Result<(), JsError> {
        let bin = self.baseline_bin.take().ok_or_else(|| JsError::new("no binary read"))?;
        if let Some(Baseline::Folder(f)) = &mut self.baseline {
            f.binaries.push(binviz::diff::BinarySnapshot {
                path,
                snapshot: bin.size_snapshot(name),
            });
        }
        Ok(())
    }

    /// "binary", "folder", or null with no build to compare with.
    #[wasm_bindgen(js_name = baselineKind)]
    pub fn baseline_kind(&self) -> Option<String> {
        self.baseline.as_ref().map(|b| match b {
            Baseline::Binary(_) => "binary".into(),
            Baseline::Folder(_) => "folder".into(),
        })
    }

    #[wasm_bindgen(js_name = baselineClear)]
    pub fn baseline_clear(&mut self) {
        self.baseline = None;
        self.baseline_bin = None;
        self.baseline_full = None;
    }

    // --- Comparing functions -------------------------------------------------------

    /// Reads the patched file, the first time a function diff needs it.
    fn prepare_sides(&mut self, with: &str) -> Result<(), JsError> {
        let fingerprint = self.bin()?.summary().fingerprint.clone();
        if with != "baseline"
            && let Some(p) = self.patched.as_mut().filter(|p| p.fingerprint == fingerprint)
            && p.parsed.is_none()
        {
            let (bin, _) = binviz::package::load_binary(p.target.clone().into()).map_err(err)?;
            p.parsed = Some(bin);
        }
        Ok(())
    }

    /// The two binaries a function diff compares: `baseline` (the earlier
    /// build, and the open binary) or `patch` (the open file, and the patched
    /// one, once `prepare_sides` read it).
    fn sides(&self, with: &str) -> Result<(&Binary, &Binary), JsError> {
        let current = self.bin()?;
        match with {
            "baseline" => Ok((
                self.baseline_full
                    .as_ref()
                    .ok_or_else(|| JsError::new("no earlier binary to compare with"))?,
                current,
            )),
            _ => {
                let p = self
                    .patched
                    .as_ref()
                    .filter(|p| p.fingerprint == current.summary().fingerprint)
                    .and_then(|p| p.parsed.as_ref())
                    .ok_or_else(|| JsError::new("no patch"))?;
                Ok((current, p))
            }
        }
    }

    /// Which functions of the two sides (`baseline` or `patch`) are which:
    /// the counts, the pairs that aren't identical, the added and removed.
    #[wasm_bindgen(js_name = functionDiff)]
    pub fn function_diff(&mut self, with: &str) -> Result<JsValue, JsError> {
        self.prepare_sides(with)?;
        let key = {
            let (old, new) = self.sides(with)?;
            format!(
                "{with}:{}:{}:{}",
                old.summary().fingerprint,
                new.summary().fingerprint,
                new.data().len()
            )
        };
        if !matches!(&self.function_diff, Some((k, _)) if *k == key) {
            let (old, new) = self.sides(with)?;
            let mut d = old.diff_functions(new);
            // Identical ones are counted, not listed.
            d.pairs.retain(|p| p.status != binviz::fndiff::PairStatus::Identical);
            self.function_diff = Some((key, d));
        }
        to_js(&self.function_diff.as_ref().unwrap().1)
    }

    /// Two matched functions' instructions lined up (see `functionDiff`).
    #[wasm_bindgen(js_name = functionCode)]
    pub fn function_code(&mut self, with: &str, old_address: u64, new_address: u64) -> Result<JsValue, JsError> {
        self.prepare_sides(with)?;
        let (old, new) = self.sides(with)?;
        to_js(&old.diff_function_code(old_address, new, new_address))
    }

    /// Starts the open folder's snapshot (its files, `[{path, size, …}]`);
    /// add its binaries with `packageSnapshotAdd` once each is loaded.
    #[wasm_bindgen(js_name = packageSnapshotBegin)]
    pub fn package_snapshot_begin(&mut self, files: JsValue) -> Result<(), JsError> {
        let p = self.package.as_ref().ok_or_else(|| JsError::new("no folder open"))?;
        self.current_snapshot = Some(binviz::diff::FolderSnapshot {
            name: p.info.name.clone(),
            files: file_bytes(files, &p.info)?,
            binaries: Vec::new(),
        });
        Ok(())
    }

    #[wasm_bindgen(js_name = packageSnapshotAdd)]
    pub fn package_snapshot_add(&mut self, index: u32) -> Result<(), JsError> {
        let (path, name) = {
            let p = self.package.as_ref().ok_or_else(|| JsError::new("no folder open"))?;
            let pb = p
                .info
                .binaries
                .get(index as usize)
                .ok_or_else(|| JsError::new("no such binary"))?;
            (pb.path.clone(), pb.name.clone())
        };
        let snapshot = self.package_binary(index)?.size_snapshot(&name);
        if let Some(f) = &mut self.current_snapshot {
            f.binaries.push(binviz::diff::BinarySnapshot { path, snapshot });
        }
        Ok(())
    }

    /// What changed from the earlier folder to the open one (after `packageSnapshotBegin` and `packageSnapshotAdd`).
    #[wasm_bindgen(js_name = sizeDiffFolder)]
    pub fn size_diff_folder(&self, top: u32) -> Result<JsValue, JsError> {
        let Some(Baseline::Folder(old)) = &self.baseline else {
            return Err(JsError::new("no folder to compare with"));
        };
        let new = self
            .current_snapshot
            .as_ref()
            .ok_or_else(|| JsError::new("the open folder isn't read yet"))?;
        to_js(&binviz::diff::diff_folders(old, new, top as usize))
    }

    /// What changed from the earlier binary (or, of an earlier folder, the
    /// binary with the open one's name) to the open binary.
    #[wasm_bindgen(js_name = sizeDiffBinary)]
    pub fn size_diff_binary(&self, top: u32) -> Result<JsValue, JsError> {
        let bin = self.bin()?;
        let name = match &self.package {
            Some(p) => p
                .current
                .and_then(|c| p.info.binaries.get(c as usize))
                .map(|b| b.name.clone())
                .unwrap_or_default(),
            None => self.name.rsplit(['/', '\\']).next().unwrap_or(&self.name).to_string(),
        };
        let old = match &self.baseline {
            Some(Baseline::Binary(s)) => s,
            Some(Baseline::Folder(f)) => f
                .binaries
                .iter()
                .find(|b| b.snapshot.name == name)
                .map(|b| &b.snapshot)
                .ok_or_else(|| JsError::new(&format!("the earlier build has no binary named {name}")))?,
            None => return Err(JsError::new("no build to compare with")),
        };
        to_js(&binviz::diff::diff_binaries(
            old,
            &bin.size_snapshot(&name),
            top as usize,
        ))
    }

    /// Where the open binary's bytes go: sections, owners, largest symbols.
    #[wasm_bindgen(js_name = sizeReport)]
    pub fn size_report(&self, top: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.size_report(top as usize))
    }

    /// Size of the WebAssembly memory, in bytes (it only ever grows).
    #[wasm_bindgen(js_name = memoryBytes)]
    pub fn memory_bytes(&self) -> f64 {
        #[cfg(target_arch = "wasm32")]
        {
            (core::arch::wasm32::memory_size(0) * 65536) as f64
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            0.0
        }
    }

    /// `count` bytes of the open file from `offset` (for small reads).
    pub fn read(&self, offset: u64, count: u32) -> Result<Vec<u8>, JsError> {
        let data = self.bin()?.data();
        let start = (offset as usize).min(data.len());
        let end = start.saturating_add(count as usize).min(data.len());
        Ok(data[start..end].to_vec())
    }

    #[wasm_bindgen(js_name = openMember)]
    pub fn open_member(&mut self, index: u32) -> Result<JsValue, JsError> {
        let c = self
            .container
            .as_ref()
            .ok_or_else(|| JsError::new("no container open"))?;
        let member = c
            .members()
            .get(index as usize)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        self.binary = Some(c.open(index).map_err(err)?);
        self.name = format!("{} [{member}]", self.name.split(" [").next().unwrap_or(""));
        self.opened()
    }

    fn opened(&self) -> Result<JsValue, JsError> {
        let b = self.bin()?;
        to_js(&Opened::Binary {
            name: &self.name,
            summary: b.summary(),
        })
    }

    fn bin(&self) -> Result<&Binary, JsError> {
        self.binary.as_ref().ok_or_else(|| JsError::new("no binary open"))
    }

    fn debug(&self) -> Result<&binviz::DebugInfo, JsError> {
        self.bin()?
            .debug_info()
            .ok_or_else(|| JsError::new("no DWARF debug info"))
    }

    /// Loads debug info from a companion file (dSYM DWARF file, `.debug`, PDB, unstripped copy).
    #[wasm_bindgen(js_name = attachDebug)]
    pub fn attach_debug(&mut self, name: String, bytes: Vec<u8>) -> Result<JsValue, JsError> {
        let b = self.binary.as_mut().ok_or_else(|| JsError::new("no binary open"))?;
        b.attach_debug_file(&name, bytes).map_err(err)?;
        self.opened()
    }

    /// The object files the open binary's debug map names (a Mach-O binary
    /// linked without dsymutil keeps its DWARF there).
    #[wasm_bindgen(js_name = debugMap)]
    pub fn debug_map(&self) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.debug_map())
    }

    /// Adds an object file (or static library), by its file name, for `debugMapLink`.
    #[wasm_bindgen(js_name = debugMapAdd)]
    pub fn debug_map_add(&mut self, name: String, bytes: Vec<u8>) {
        self.debug_objects.insert(name, bytes);
    }

    /// Links the open binary's debug map from the objects added with
    /// `debugMapAdd` (then let go): `{objects, linked, missing, failed, units}`.
    #[wasm_bindgen(js_name = debugMapLink)]
    pub fn debug_map_link(&mut self) -> Result<JsValue, JsError> {
        let objects = std::mem::take(&mut self.debug_objects);
        let b = self.binary.as_mut().ok_or_else(|| JsError::new("no binary open"))?;
        let report = b
            .attach_debug_map(&mut |o| Ok(objects.get(o.file_name()).cloned()))
            .map_err(err)?;
        to_js(&report)
    }

    /// The bytes of the open binary (a member's bytes for containers).
    pub fn bytes(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.bin()?.data().to_vec())
    }

    pub fn summary(&self) -> Result<JsValue, JsError> {
        to_js(self.bin()?.summary())
    }

    pub fn sections(&self) -> Result<JsValue, JsError> {
        to_js(self.bin()?.sections())
    }

    pub fn segments(&self) -> Result<JsValue, JsError> {
        to_js(self.bin()?.segments())
    }

    pub fn imports(&self) -> Result<JsValue, JsError> {
        to_js(self.bin()?.imports())
    }

    pub fn exports(&self) -> Result<JsValue, JsError> {
        to_js(self.bin()?.exports())
    }

    /// Layout regions under `parent` (top level when undefined).
    pub fn regions(&self, parent: Option<u32>) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.regions(parent))
    }

    pub fn region(&self, id: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.region(id))
    }

    #[wasm_bindgen(js_name = regionEntries)]
    pub fn region_entries(&self, id: u32, first: u32, count: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.region_entries(id, first, count))
    }

    /// Coloured spans for the hex view.
    pub fn spans(&self, start: u64, end: u64) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.spans(start, end))
    }

    /// Dominant region kind per bucket, for the file overview map.
    #[wasm_bindgen(js_name = fileMap)]
    pub fn file_map(&self, buckets: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.file_map(buckets))
    }

    /// `[kind, bytes]` pairs covering the whole file.
    pub fn composition(&self) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.composition())
    }

    #[wasm_bindgen(js_name = entropyMap)]
    pub fn entropy_map(&self, buckets: u32) -> Result<Vec<f32>, JsError> {
        Ok(self.bin()?.entropy_map(buckets))
    }

    #[wasm_bindgen(js_name = inspectOffset)]
    pub fn inspect_offset(&self, offset: u64) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.inspect(Target::Offset(offset)))
    }

    #[wasm_bindgen(js_name = inspectAddress)]
    pub fn inspect_address(&self, address: u64) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.inspect(Target::Address(address)))
    }

    #[wasm_bindgen(js_name = addressToOffset)]
    pub fn address_to_offset(&self, address: u64) -> Result<Option<u64>, JsError> {
        Ok(self.bin()?.address_to_offset(address))
    }

    #[wasm_bindgen(js_name = offsetToAddress)]
    pub fn offset_to_address(&self, offset: u64) -> Result<Option<u64>, JsError> {
        Ok(self.bin()?.offset_to_address(offset))
    }

    /// `query` is `{filter, kind, sort, descending, definedOnly, offset, limit}`.
    pub fn symbols(&self, query: JsValue) -> Result<JsValue, JsError> {
        let q: SymbolQuery = if query.is_undefined() || query.is_null() {
            SymbolQuery::default()
        } else {
            serde_wasm_bindgen::from_value(query).map_err(err)?
        };
        to_js(&self.bin()?.symbols().query(&q))
    }

    pub fn symbol(&self, index: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.symbols().get(index).map(|s| s.to_symbol()))
    }

    /// A page of the functions whose name or address contains `filter`, in
    /// address order: `{total, offset, functions: [address, size, name][]}`.
    #[wasm_bindgen(js_name = functionsPage)]
    pub fn functions_page(&self, filter: String, offset: u32, limit: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.symbols().function_page(&filter, offset, limit))
    }

    /// Where the function containing `address` is in that list (undefined if none).
    #[wasm_bindgen(js_name = functionIndex)]
    pub fn function_index(&self, filter: String, address: u64) -> Result<Option<u32>, JsError> {
        Ok(self.bin()?.symbols().function_position(&filter, address))
    }

    /// Function symbols in address order: `[address, size, name]` triples.
    pub fn functions(&self) -> Result<JsValue, JsError> {
        let list: Vec<(u64, u64, String)> = self
            .bin()?
            .symbols()
            .functions()
            .map(|s| (s.address, s.size, s.display_name().into_owned()))
            .collect();
        to_js(&list)
    }

    pub fn disassemble(&self, start: u64, end: u64, limit: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.disassemble(start, end, limit as usize))
    }

    #[wasm_bindgen(js_name = disassembleFunction)]
    pub fn disassemble_function(&self, address: u64, limit: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.disassemble_function(address, limit as usize))
    }

    /// Resolves a "go to" query: `0x401000` (address), `@0x200` (file
    /// offset), a symbol name, or `file.rs:42` (source line).
    pub fn resolve(&self, query: String) -> Result<JsValue, JsError> {
        let b = self.bin()?;
        let q = query.trim();
        let parse = |s: &str| -> Option<u64> {
            let s = s.trim();
            match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                Some(h) => u64::from_str_radix(h, 16).ok(),
                None if s.chars().all(|c| c.is_ascii_hexdigit()) && s.chars().any(|c| c.is_ascii_digit()) => {
                    u64::from_str_radix(s, 16).ok()
                }
                None => None,
            }
        };
        let resolved = if let Some(off) = q.strip_prefix('@').and_then(parse) {
            Resolved {
                kind: "offset",
                value: off,
                label: format!("file offset {off:#x}"),
            }
        } else if let Some(addr) = b.rom_address(q) {
            Resolved {
                kind: "address",
                value: addr,
                label: format!("{q} ({addr:#x})"),
            }
        } else if let Some(addr) = parse(q) {
            if b.segment_at(addr).is_some() || b.section_at(addr).is_some() {
                Resolved {
                    kind: "address",
                    value: addr,
                    label: format!("address {addr:#x}"),
                }
            } else if addr < b.data().len() as u64 {
                Resolved {
                    kind: "offset",
                    value: addr,
                    label: format!("file offset {addr:#x} (not a mapped address)"),
                }
            } else {
                return Err(JsError::new(&format!(
                    "{addr:#x} is neither a mapped address nor a file offset"
                )));
            }
        } else if let Some(sym) = b.symbols().by_name(q) {
            Resolved {
                kind: "address",
                value: sym.address,
                label: sym.display_name().to_string(),
            }
        } else if let Some((file, line)) = q.rsplit_once(':').and_then(|(f, l)| Some((f, l.parse::<u32>().ok()?))) {
            let debug = self.debug()?;
            let needle = file.replace('\\', "/");
            let found = debug
                .source_files()
                .iter()
                .filter(|f| f.path.replace('\\', "/").ends_with(&needle))
                .find_map(|f| {
                    let lines = debug.file_lines(f.id);
                    lines
                        .iter()
                        .filter(|l| l.line >= line)
                        .min_by_key(|l| (l.line, !l.is_stmt, l.start))
                        .map(|l| (f.path.clone(), l.clone()))
                });
            match found {
                Some((path, l)) => Resolved {
                    kind: "address",
                    value: l.start,
                    label: format!("{path}:{}", l.line),
                },
                None => return Err(JsError::new(&format!("no code for {q}"))),
            }
        } else {
            return Err(JsError::new(&format!("nothing matches {q:?}")));
        };
        to_js(&resolved)
    }

    // --- Search ------------------------------------------------------------

    /// Searches addresses, offsets, names, byte patterns, strings and source
    /// lines at once. `only` restricts to one hit kind ("symbol", "string"...).
    pub fn search(&self, query: String, per_kind: u32, only: Option<String>) -> Result<JsValue, JsError> {
        let only = match only.as_deref() {
            None | Some("") => None,
            Some(k) => Some(serde_wasm_bindgen::from_value::<HitKind>(JsValue::from_str(k)).map_err(err)?),
        };
        to_js(&self.bin()?.search(&query, per_kind, only))
    }

    /// Builds the search indexes (strings, DWARF names) ahead of the first query.
    #[wasm_bindgen(js_name = prepareSearch)]
    pub fn prepare_search(&self) -> Result<(), JsError> {
        self.bin()?.prepare_search();
        Ok(())
    }

    /// A page of the printable strings containing `filter`.
    pub fn strings(&self, filter: String, offset: u32, limit: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.strings(&filter, offset, limit))
    }

    // --- Attribution and coverage ------------------------------------------

    /// Code and data per source file (`mode` "file") or compilation unit ("unit").
    pub fn attribution(&self, mode: String) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.attribution(attribution_mode(&mode)))
    }

    #[wasm_bindgen(js_name = attributedRanges)]
    pub fn attributed_ranges(&self, mode: String, id: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.attributed_ranges(attribution_mode(&mode), id))
    }

    /// Reverse-engineering coverage of the code and data sections.
    pub fn coverage(&self, max_gaps: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.coverage(max_gaps))
    }

    #[wasm_bindgen(js_name = coverageStrip)]
    pub fn coverage_strip(&self, section: u32, buckets: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.coverage_strip(section, buckets))
    }

    #[wasm_bindgen(js_name = coverageMap)]
    pub fn coverage_map(&self, buckets: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.coverage_map(buckets))
    }

    /// Replaces the user's annotations (`[{address, size, name, comment, reviewed}]`)
    /// and returns the updated summary.
    #[wasm_bindgen(js_name = setAnnotations)]
    pub fn set_annotations(&mut self, list: JsValue) -> Result<JsValue, JsError> {
        let list: Vec<Annotation> = serde_wasm_bindgen::from_value(list).map_err(err)?;
        let b = self.binary.as_mut().ok_or_else(|| JsError::new("no binary open"))?;
        b.set_annotations(list);
        to_js(b.summary())
    }

    pub fn annotations(&self) -> Result<JsValue, JsError> {
        to_js(self.bin()?.annotations())
    }

    // --- Cross-references and the call graph -------------------------------

    /// Whether references can be found in this binary's code (x86, AArch64).
    #[wasm_bindgen(js_name = xrefsSupported)]
    pub fn xrefs_supported(&self) -> Result<bool, JsError> {
        Ok(self.bin()?.xrefs_supported())
    }

    #[wasm_bindgen(js_name = xrefsReady)]
    pub fn xrefs_ready(&self) -> Result<bool, JsError> {
        Ok(self.bin()?.xrefs_ready())
    }

    /// Builds the reference index; returns the number of references by kind.
    #[wasm_bindgen(js_name = prepareXrefs)]
    pub fn prepare_xrefs(&self) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.xref_counts())
    }

    /// References to `lo..hi`: `{total, offset, counts, refs}`.
    #[wasm_bindgen(js_name = referencesTo)]
    pub fn references_to(&self, lo: u64, hi: u64, offset: u32, limit: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.references_to(lo, hi, offset, limit))
    }

    #[wasm_bindgen(js_name = referenceCounts)]
    pub fn reference_counts(&self, lo: u64, hi: u64) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.reference_counts(lo, hi))
    }

    /// References made by the code or data in `lo..hi`.
    #[wasm_bindgen(js_name = referencesFrom)]
    pub fn references_from(&self, lo: u64, hi: u64) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.references_from(lo, hi))
    }

    pub fn callers(&self, address: u64) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.callers(address))
    }

    pub fn callees(&self, address: u64) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.callees(address))
    }

    #[wasm_bindgen(js_name = callGraph)]
    pub fn call_graph(&self, center: u64, up: u32, down: u32, fanout: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.call_graph(center, up, down, fanout as usize))
    }

    #[wasm_bindgen(js_name = callPath)]
    pub fn call_path(&self, from: u64, to: u64, max_depth: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.call_path(from, to, max_depth))
    }

    #[wasm_bindgen(js_name = functionSummary)]
    pub fn function_summary(&self, address: u64, limit: u32) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.function_summary(address, limit as usize))
    }

    // --- Text in games: relative search and table files -----------------

    // --- Emulators: code/data logs and label files ---------------------------------

    /// Reads the open ROM again with a code/data log (FCEUX's or Mesen's),
    /// keeping the notes: what the log covers. The views then show the ROM
    /// as the log saw it run.
    #[wasm_bindgen(js_name = codeLog)]
    pub fn code_log(&mut self, bytes: &[u8]) -> Result<JsValue, JsError> {
        let b = self.bin()?;
        let notes = b.annotations().to_vec();
        let (mut logged, summary) = b.with_code_log(bytes).map_err(err)?;
        logged.set_annotations(notes);
        self.binary = Some(logged);
        to_js(&summary)
    }

    /// What the code/data log the ROM was read with covers, or null.
    #[wasm_bindgen(js_name = codeLogSummary)]
    pub fn code_log_summary(&self) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.code_log())
    }

    /// The code/data log's flags for `count` bytes from a file offset (1 code, 2 data, 4 a subroutine's start…).
    #[wasm_bindgen(js_name = codeLogFlags)]
    pub fn code_log_flags(&self, offset: u64, count: u32) -> Result<Vec<u16>, JsError> {
        let b = self.bin()?;
        Ok((offset..offset + count as u64).map(|o| b.code_log_at(o)).collect())
    }

    /// Reads a label file (Mesen's .mlb, FCEUX's .nl, RGBDS / WLA DX / no$gba
    /// .sym) into notes: `{format, labels, skipped, directives}`.
    #[wasm_bindgen(js_name = readLabels)]
    pub fn read_labels(&self, name: &str, text: &str) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.read_labels(name, text).map_err(err)?)
    }

    /// The label formats this ROM's emulators read (`mlb`, `nl`, `sym`, `nocash`).
    #[wasm_bindgen(js_name = labelFormats)]
    pub fn label_formats(&self) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.label_formats())
    }

    /// The notes as label files in a format: `[{suffix, text}]`.
    #[wasm_bindgen(js_name = writeLabels)]
    pub fn write_labels(&self, format: &str) -> Result<JsValue, JsError> {
        let format =
            binviz::rom::labels::LabelFormat::from_name(format).ok_or_else(|| JsError::new("unknown label format"))?;
        to_js(&self.bin()?.write_labels(format).map_err(err)?)
    }

    // --- Patches ------------------------------------------------------------------

    fn patched(&self) -> Option<&Patched> {
        let b = self.binary.as_ref()?;
        self.patched
            .as_ref()
            .filter(|p| p.fingerprint == b.summary().fingerprint)
    }

    /// Applies an IPS, UPS or BPS patch (named `name`) to the open file, replacing
    /// any patch or edits before: where it stands (see `patchState`).
    #[wasm_bindgen(js_name = patchApply)]
    pub fn patch_apply(&mut self, name: String, patch: &[u8], limit: u32) -> Result<JsValue, JsError> {
        let b = self.bin()?;
        let mut applied = binviz::patch::apply(patch, b.data()).map_err(err)?;
        let target = std::mem::take(&mut applied.output);
        // Changes are worked out again as the patched file is edited.
        applied.changes = Vec::new();
        self.patched = Some(Patched {
            fingerprint: b.summary().fingerprint.clone(),
            target,
            applied: Some(applied),
            name: Some(name),
            parsed: None,
        });
        self.patch_state(limit)
    }

    /// Writes bytes at a file offset of the patched file (the open file, the
    /// first time): where the patch stands.
    #[wasm_bindgen(js_name = patchEdit)]
    pub fn patch_edit(&mut self, offset: u64, bytes: &[u8], limit: u32) -> Result<JsValue, JsError> {
        let b = self.binary.as_ref().ok_or_else(|| JsError::new("no binary open"))?;
        let fingerprint = b.summary().fingerprint.clone();
        if self.patched.as_ref().is_none_or(|p| p.fingerprint != fingerprint) {
            self.patched = Some(Patched {
                fingerprint,
                target: b.data().to_vec(),
                applied: None,
                name: None,
                parsed: None,
            });
        }
        let p = self.patched.as_mut().unwrap();
        p.parsed = None;
        let (start, end) = (offset as usize, offset as usize + bytes.len());
        if end > p.target.len() {
            return Err(JsError::new("past the end of the file"));
        }
        p.target[start..end].copy_from_slice(bytes);
        self.patch_state(limit)
    }

    /// Where the patch stands: what the patch file said, the changed runs
    /// (the first `limit` placed in banks, functions and regions, with their
    /// bytes before and after), or null with none.
    #[wasm_bindgen(js_name = patchState)]
    pub fn patch_state(&self, limit: u32) -> Result<JsValue, JsError> {
        let b = self.bin()?;
        let Some(p) = self.patched() else {
            return Ok(JsValue::NULL);
        };
        let original = b.data();
        let changes = binviz::patch::changes(original, &p.target);
        let differ = changes.iter().map(|c| c.differ).sum();
        let shown = &changes[..changes.len().min(limit as usize)];
        let slice = |data: &[u8], c: &binviz::patch::Change| -> Vec<u8> {
            let start = (c.offset as usize).min(data.len());
            data[start..(start + c.len.min(32) as usize).min(data.len())].to_vec()
        };
        let read = |bytes: &[u8]| self.table.as_ref().map(|t| t.decode(bytes, false).text);
        let rows = binviz::patch::place(b, shown)
            .into_iter()
            .map(|placed| {
                let before = slice(original, &placed.change);
                let after = slice(&p.target, &placed.change);
                PatchRow {
                    before_text: read(&before),
                    after_text: read(&after),
                    before,
                    after,
                    placed,
                }
            })
            .collect();
        to_js(&PatchState {
            name: p.name.as_deref(),
            applied: p.applied.as_ref(),
            changes: rows,
            total: changes.len() as u32,
            differ,
            target_size: p.target.len() as u64,
        })
    }

    /// Bytes of the patched file.
    #[wasm_bindgen(js_name = patchRead)]
    pub fn patch_read(&self, offset: u64, count: u32) -> Result<Vec<u8>, JsError> {
        let p = self.patched().ok_or_else(|| JsError::new("no patch"))?;
        let start = (offset as usize).min(p.target.len());
        Ok(p.target[start..(start + count as usize).min(p.target.len())].to_vec())
    }

    /// The patched file, whole.
    #[wasm_bindgen(js_name = patchTarget)]
    pub fn patch_target(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.patched().ok_or_else(|| JsError::new("no patch"))?.target.clone())
    }

    /// A patch file (`ips`, `ups` or `bps`) that turns the open file into the patched one.
    #[wasm_bindgen(js_name = patchCreate)]
    pub fn patch_create(&self, format: &str) -> Result<Vec<u8>, JsError> {
        let p = self.patched().ok_or_else(|| JsError::new("no patch"))?;
        let format =
            binviz::patch::PatchFormat::from_name(format).ok_or_else(|| JsError::new("unknown patch format"))?;
        binviz::patch::create(format, self.bin()?.data(), &p.target).map_err(err)
    }

    /// Drops the patch and the edits.
    #[wasm_bindgen(js_name = patchClear)]
    pub fn patch_clear(&mut self) {
        self.patched = None;
    }

    /// Relative search in the open file: `word` in any encoding that keeps its
    /// letters in order (`width` 1, or 2 for 16-bit characters).
    #[wasm_bindgen(js_name = relativeSearch)]
    pub fn relative_search(&self, word: &str, width: u32, limit: u32) -> Result<JsValue, JsError> {
        let found = binviz::tables::relative_search(self.bin()?.data(), word, width, limit as usize)
            .map_err(|e| JsError::new(&e))?;
        to_js(&found)
    }

    /// The table (as `.tbl` lines) of an alphabet starting at `first`.
    #[wasm_bindgen(js_name = tableFromAlphabet)]
    pub fn table_from_alphabet(&self, first: u32, letter: char, width: u32) -> String {
        binviz::tables::Table::from_alphabet(first, letter, width).to_tbl()
    }

    /// Reads text with this table file from now on; returns how many entries it has.
    #[wasm_bindgen(js_name = tableSet)]
    pub fn table_set(&mut self, text: &str) -> Result<u32, JsError> {
        let table = binviz::tables::Table::parse(text).map_err(|e| JsError::new(&e))?;
        let n = table.len() as u32;
        self.table = Some(table);
        Ok(n)
    }

    #[wasm_bindgen(js_name = tableClear)]
    pub fn table_clear(&mut self) {
        self.table = None;
    }

    fn table(&self) -> Result<&binviz::tables::Table, JsError> {
        self.table.as_ref().ok_or_else(|| JsError::new("no table set"))
    }

    /// Per byte value, the text of its one-byte entry ("" for none).
    #[wasm_bindgen(js_name = tableChars)]
    pub fn table_chars(&self) -> Result<Vec<String>, JsError> {
        let t = self.table()?;
        Ok((0..=255u8).map(|b| t.single(b).unwrap_or("").to_string()).collect())
    }

    /// The text at `offset` (up to `len` bytes, or the first end marker).
    #[wasm_bindgen(js_name = tableDecode)]
    pub fn table_decode(&self, offset: u64, len: u32) -> Result<JsValue, JsError> {
        let data = self.bin()?.data();
        let start = (offset as usize).min(data.len());
        let bytes = &data[start..(start + len as usize).min(data.len())];
        let mut text = self.table()?.decode(bytes, true);
        text.offset = offset;
        to_js(&text)
    }

    /// Where `text` is, as the table encodes it, with the text there.
    #[wasm_bindgen(js_name = tableFind)]
    pub fn table_find(&self, text: &str, limit: u32) -> Result<JsValue, JsError> {
        let found = self
            .table()?
            .find_text(self.bin()?.data(), text, limit as usize)
            .map_err(|e| JsError::new(&e))?;
        to_js(&found)
    }

    /// All the text the table reads: strings of `min` entries or more.
    #[wasm_bindgen(js_name = tableStrings)]
    pub fn table_strings(&self, min: u32, limit: u32) -> Result<JsValue, JsError> {
        to_js(&self.table()?.strings(self.bin()?.data(), min as usize, limit as usize))
    }

    // --- Objective-C -------------------------------------------------------

    /// How many classes, categories, protocols, methods and selectors there are.
    #[wasm_bindgen(js_name = objcCounts)]
    pub fn objc_counts(&self) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.objc().counts())
    }

    /// Classes, categories and protocols: `[{kind, name, address, base, methods, swift}]`.
    #[wasm_bindgen(js_name = objcEntries)]
    pub fn objc_entries(&self) -> Result<JsValue, JsError> {
        to_js(&self.bin()?.objc().entries())
    }

    /// A class, category or protocol (`kind`) declared as its header would, or null.
    #[wasm_bindgen(js_name = objcInterface)]
    pub fn objc_interface(&self, kind: &str, name: &str) -> Result<JsValue, JsError> {
        match self.bin()?.objc().interface_of(kind, name) {
            Some(i) => to_js(&i),
            None => Ok(JsValue::NULL),
        }
    }

    /// The methods implementing a selector and the functions sending it, or null.
    #[wasm_bindgen(js_name = objcSelector)]
    pub fn objc_selector(&self, selector: &str) -> Result<JsValue, JsError> {
        match self.bin()?.objc_selector(selector) {
            Some(u) => to_js(&u),
            None => Ok(JsValue::NULL),
        }
    }

    // --- DWARF -------------------------------------------------------------

    #[wasm_bindgen(js_name = dwarfSummary)]
    pub fn dwarf_summary(&self) -> Result<JsValue, JsError> {
        match self.bin()?.debug_info() {
            Some(d) => to_js(&d.summary()),
            None => Ok(JsValue::NULL),
        }
    }

    #[wasm_bindgen(js_name = dwarfUnits)]
    pub fn dwarf_units(&self) -> Result<JsValue, JsError> {
        to_js(self.debug()?.units())
    }

    #[wasm_bindgen(js_name = unitRoot)]
    pub fn unit_root(&self, unit: u32) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.unit_root(unit))
    }

    #[wasm_bindgen(js_name = dieChildren)]
    pub fn die_children(&self, unit: u32, offset: Option<u64>) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.die_children(unit, offset))
    }

    pub fn die(&self, unit: u32, offset: u64) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.die(unit, offset))
    }

    /// Innermost scope DIE (function / inlined call / block) at an address.
    #[wasm_bindgen(js_name = dieAt)]
    pub fn die_at(&self, address: u64) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.die_at(address))
    }

    /// The function's DW_TAG_subprogram at an address (ignoring inlined callees).
    #[wasm_bindgen(js_name = functionDieAt)]
    pub fn function_die_at(&self, address: u64) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.function_die_at(address))
    }

    /// Named DIEs (functions, variables, types, members) by qualified name, best first.
    #[wasm_bindgen(js_name = dieSearch)]
    pub fn die_search(&self, query: String, limit: u32) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.search(&query, limit as usize))
    }

    /// Every DIE whose name contains `query`, locals and parameters included (slower).
    #[wasm_bindgen(js_name = dieSearchAll)]
    pub fn die_search_all(&self, query: String, limit: u32) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.search_all(&query, limit as usize))
    }

    /// A page of a unit's DIEs whose tag matches `filter` and name contains `name`.
    #[wasm_bindgen(js_name = listDies)]
    pub fn list_dies(
        &self,
        unit: u32,
        filter: String,
        name: String,
        offset: u32,
        limit: u32,
    ) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.list_dies(unit, &filter, &name, offset, limit))
    }

    #[wasm_bindgen(js_name = tagCounts)]
    pub fn tag_counts(&self, unit: u32) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.tag_counts(unit))
    }

    /// The DIE at (or containing) a `.debug_info` offset: `[unit, offset]`.
    #[wasm_bindgen(js_name = dieAtOffset)]
    pub fn die_at_offset(&self, offset: u64) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.die_at_offset(offset))
    }

    /// Scopes and variables in scope at an address.
    #[wasm_bindgen(js_name = scopeAt)]
    pub fn scope_at(&self, address: u64) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.scope_at(address))
    }

    /// Everything in the DWARF that can't be read or doesn't add up.
    #[wasm_bindgen(js_name = dwarfCheck)]
    pub fn dwarf_check(&self) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.check())
    }

    /// Units that couldn't be read at all (found while loading).
    #[wasm_bindgen(js_name = dwarfLoadProblems)]
    pub fn dwarf_load_problems(&self) -> Result<JsValue, JsError> {
        to_js(self.debug()?.load_problems())
    }

    #[wasm_bindgen(js_name = lineProgram)]
    pub fn line_program(&self, unit: u32) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.line_program(unit))
    }

    #[wasm_bindgen(js_name = lineRows)]
    pub fn line_rows(&self, unit: u32, first: u32, count: u32) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.line_rows(unit, first, count))
    }

    #[wasm_bindgen(js_name = sourceFiles)]
    pub fn source_files(&self) -> Result<JsValue, JsError> {
        match self.bin()?.debug_info() {
            Some(d) => to_js(d.source_files()),
            None => to_js::<[u8]>(&[]),
        }
    }

    #[wasm_bindgen(js_name = fileLines)]
    pub fn file_lines(&self, file: u32) -> Result<JsValue, JsError> {
        to_js(&self.debug()?.file_lines(file))
    }

    /// Lines-with-code count for every source file (indexed by file id).
    #[wasm_bindgen(js_name = fileLineCounts)]
    pub fn file_line_counts(&self) -> Result<Vec<u32>, JsError> {
        Ok(self.debug()?.file_line_counts())
    }

    #[wasm_bindgen(js_name = embeddedSource)]
    pub fn embedded_source(&self, file: u32) -> Result<Option<String>, JsError> {
        Ok(self.debug()?.embedded_source(file))
    }
}
