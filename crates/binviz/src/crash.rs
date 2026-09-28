//! Crash reports, symbolicated with the binaries at hand: Apple's text
//! reports (`.crash`) and JSON ones (`.ips`), Android tombstones, and
//! pasted stack traces that give images and offsets.
//!
//! [`parse`] reads a report. The caller picks the binaries whose UUID or
//! build ID is one of the report's images ([`CrashImage::matches`]), and
//! [`symbolicate`] turns each frame into the function it was in, its source
//! line, and the calls inlined there.

use serde::Serialize;

use crate::Binary;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReport {
    /// "Apple crash report", "Apple crash report (.ips)", "Android tombstone" or "Stack trace".
    pub format: String,
    pub process: Option<String>,
    pub identifier: Option<String>,
    pub version: Option<String>,
    pub os: Option<String>,
    pub arch: Option<String>,
    pub exception: Option<String>,
    /// Why it stopped: a termination reason, an abort message, what the app said last.
    pub reason: Option<String>,
    pub images: Vec<CrashImage>,
    /// The crashed thread first.
    pub threads: Vec<CrashThread>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashImage {
    /// As the report names it (the executable's name, or a bundle identifier).
    pub name: String,
    pub path: Option<String>,
    /// Its UUID (Mach-O) or build ID (ELF), as the report writes it.
    pub id: Option<String>,
    pub arch: Option<String>,
    /// Where it was loaded.
    pub load_address: Option<u64>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CrashThread {
    pub name: String,
    pub crashed: bool,
    pub frames: Vec<CrashFrame>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashFrame {
    pub index: u32,
    /// Index into the images.
    pub image: Option<u32>,
    /// The image's name as the frame gives it.
    pub image_name: String,
    /// Where the frame pointed at run time, in the image as loaded.
    pub address: Option<u64>,
    /// Or how far into its image, from the image's first byte.
    pub offset: Option<u64>,
    /// Or the address as the image was linked (an Android tombstone's "relative pc").
    pub linked: Option<u64>,
    /// The name the report gives it, if any.
    pub symbol: Option<String>,
}

/// Whether a binary is one of a report's images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMatch {
    /// The same UUID or build ID (or, lacking those, the same name).
    Yes,
    /// The same name, but another UUID or build ID: another build.
    OtherBuild,
    No,
}

fn id_key(id: &str) -> String {
    id.chars()
        .filter(char::is_ascii_hexdigit)
        .collect::<String>()
        .to_ascii_lowercase()
}

impl CrashImage {
    /// The executable's file name (from the path, when the report gives one).
    pub fn file_name(&self) -> &str {
        self.path
            .as_deref()
            .and_then(|p| p.rsplit(['/', '\\']).next())
            .filter(|n| !n.is_empty())
            .unwrap_or(&self.name)
    }

    /// Whether a binary named `name` with these build IDs (a Mach-O UUID per
    /// slice, an ELF build ID) is this image: by ID when both have one, else by name.
    pub fn matches<'a>(&self, ids: impl IntoIterator<Item = &'a str>, name: &str) -> ImageMatch {
        let named = name == self.file_name() || name == self.name;
        let ids: Vec<String> = ids.into_iter().map(id_key).filter(|k| !k.is_empty()).collect();
        match self.id.as_deref().map(id_key).filter(|k| !k.is_empty()) {
            Some(want) if !ids.is_empty() => {
                if ids.contains(&want) {
                    ImageMatch::Yes
                } else if named {
                    ImageMatch::OtherBuild
                } else {
                    ImageMatch::No
                }
            }
            _ if named => ImageMatch::Yes,
            _ => ImageMatch::No,
        }
    }
}

impl CrashReport {
    /// The images that frames point into, each once.
    pub fn images_in_frames(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self
            .threads
            .iter()
            .flat_map(|t| &t.frames)
            .filter_map(|f| f.image)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// A binary that may be one of a report's images: the caller's index for
/// it, its file name, and its build IDs (a Mach-O UUID per slice, an ELF build ID).
pub struct Candidate<'a> {
    pub binary: u32,
    pub name: &'a str,
    pub ids: Vec<&'a str>,
}

/// Which binary is each image frames point into.
#[derive(Debug, Default)]
pub struct Matches {
    /// (image, binary)
    pub pairs: Vec<(u32, u32)>,
    /// (image, note) for images a candidate has the name but not the build of.
    pub notes: Vec<(u32, String)>,
}

/// Which candidate is each image that frames point into.
pub fn match_images(report: &CrashReport, candidates: &[Candidate<'_>]) -> Matches {
    let mut found = Vec::new();
    let mut notes = Vec::new();
    for image in report.images_in_frames() {
        let img = &report.images[image as usize];
        let mut other = None;
        let mut hit = None;
        for c in candidates {
            match img.matches(c.ids.iter().copied(), c.name) {
                ImageMatch::Yes => {
                    hit = Some(c.binary);
                    break;
                }
                ImageMatch::OtherBuild => other = other.or(Some(c)),
                ImageMatch::No => {}
            }
        }
        match (hit, other) {
            (Some(b), _) => found.push((image, b)),
            (None, Some(c)) => notes.push((
                image,
                format!(
                    "{} here is another build: {}, the report's is {}",
                    c.name,
                    c.ids.first().copied().unwrap_or("no ID"),
                    img.id.as_deref().unwrap_or("?")
                ),
            )),
            (None, None) => {}
        }
    }
    Matches { pairs: found, notes }
}

// --- Symbolication -------------------------------------------------------------------

/// A binary found for one of a report's images.
pub struct Found<'a> {
    pub image: u32,
    /// The caller's name for it (an index into its list of binaries).
    pub binary: u32,
    pub bin: &'a Binary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbolicated {
    pub report: CrashReport,
    /// For each image frames point into: the binary that symbolicated it, or why none did.
    pub images: Vec<ImageStatus>,
    pub threads: Vec<SymbolicatedThread>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStatus {
    pub image: u32,
    pub frames: u32,
    pub binary: Option<u32>,
    /// Why it wasn't symbolicated (another build here, say).
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SymbolicatedThread {
    pub name: String,
    pub crashed: bool,
    pub frames: Vec<SymbolicatedFrame>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolicatedFrame {
    pub index: u32,
    pub image: Option<u32>,
    pub image_name: String,
    /// The address as the report gives it (at run time).
    pub address: Option<u64>,
    /// The address in the binary that symbolicated it.
    pub binary_address: Option<u64>,
    pub binary: Option<u32>,
    /// Innermost first: calls inlined there, then the function the code is in.
    pub lines: Vec<SymbolLine>,
    /// The name the report gave it.
    pub reported: Option<String>,
}

/// One function of a frame, with where in the source it was.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolLine {
    pub function: Option<String>,
    /// Bytes into the function, when only its symbol is known.
    pub offset: Option<u64>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub inlined: bool,
}

fn line_text(l: &SymbolLine) -> String {
    let mut s = l.function.clone().unwrap_or_else(|| "?".into());
    if let Some(o) = l.offset {
        s.push_str(&format!(" + {o}"));
    }
    if let Some(f) = &l.file {
        s.push_str(&format!("  {f}"));
        if let Some(n) = l.line.filter(|&n| n > 0) {
            s.push_str(&format!(":{n}"));
            if let Some(c) = l.column.filter(|&c| c > 0) {
                s.push_str(&format!(":{c}"));
            }
        }
    }
    s
}

impl Symbolicated {
    /// As text: what happened, the images frames are in and what
    /// symbolicated them (`binary` names the caller's binaries), then every
    /// thread frame by frame, the crashed one first.
    pub fn to_text(&self, binary: impl Fn(u32) -> String) -> String {
        use std::fmt::Write as _;
        let r = &self.report;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{}{}",
            r.format,
            r.process.as_deref().map(|p| format!(": {p}")).unwrap_or_default()
        );
        for (k, v) in [
            ("Identifier", &r.identifier),
            ("Version", &r.version),
            ("OS", &r.os),
            ("Exception", &r.exception),
            ("Reason", &r.reason),
        ] {
            if let Some(v) = v {
                let _ = writeln!(out, "{k}: {v}");
            }
        }
        if !self.images.is_empty() {
            let _ = writeln!(out, "\nImages the frames are in:");
            let width = self
                .images
                .iter()
                .map(|s| r.images[s.image as usize].name.len())
                .max()
                .unwrap_or(0)
                .min(40);
            for s in &self.images {
                let status = match (s.binary, &s.note) {
                    (Some(b), _) => format!("symbolicated with {}", binary(b)),
                    (None, Some(n)) => n.clone(),
                    (None, None) => "no binary for it here".into(),
                };
                let _ = writeln!(
                    out,
                    "  {:<width$}  {:>4} frame{}  {status}",
                    r.images[s.image as usize].name,
                    s.frames,
                    if s.frames == 1 { " " } else { "s" }
                );
            }
        }
        for t in &self.threads {
            let _ = writeln!(out, "\n{}{}", t.name, if t.crashed { " — crashed" } else { "" });
            let width = t.frames.iter().map(|f| f.image_name.len()).max().unwrap_or(0).min(32);
            for f in &t.frames {
                let addr = f
                    .address
                    .or(f.binary_address)
                    .map(|a| format!("{a:#x}"))
                    .unwrap_or_default();
                let head = format!("  {:>3}  {:<width$}  {addr:<18}", f.index, f.image_name);
                match f.lines.split_first() {
                    Some((first, rest)) => {
                        let _ = writeln!(out, "{head}  {}", line_text(first));
                        for l in rest {
                            let _ = writeln!(
                                out,
                                "{:pad$}  inlined into {}",
                                "",
                                line_text(l),
                                pad = head.chars().count()
                            );
                        }
                    }
                    None => {
                        let _ = writeln!(out, "{head}  {}", f.reported.as_deref().unwrap_or("?"));
                    }
                }
            }
        }
        out
    }
}

impl Binary {
    /// The address the file's first byte is linked at: what an image's load
    /// address in a crash report corresponds to (a Mach-O file's `__TEXT`,
    /// the lowest ELF `PT_LOAD`, a PE image base).
    pub fn link_base(&self) -> u64 {
        if self.summary.format == crate::Format::Pe {
            return self.image_base;
        }
        self.segments
            .iter()
            .filter(|s| s.mapped && s.file_size > 0 && s.address >= s.file_offset)
            .map(|s| s.address - s.file_offset)
            .min()
            .unwrap_or(0)
    }

    /// What code is at `address` for a stack frame: the calls inlined there
    /// and the function around them (innermost first) with their source
    /// lines when there is DWARF, else the symbol and how far into it.
    pub fn symbolize(&self, address: u64) -> Vec<SymbolLine> {
        let symbol = self.symbols.lookup(address);
        let mut lines: Vec<SymbolLine> = self
            .debug
            .as_ref()
            .map(|d| d.frames(address))
            .unwrap_or_default()
            .into_iter()
            .map(|f| SymbolLine {
                function: f.demangled.or(f.function),
                offset: None,
                file: f.file,
                line: f.line,
                column: f.column,
                inlined: f.inlined,
            })
            .collect();
        if let Some(outer) = lines.last_mut()
            && outer.function.is_none()
        {
            outer.function = symbol
                .as_ref()
                .map(|s| s.demangled.clone().unwrap_or_else(|| s.name.clone()));
        }
        if lines.is_empty()
            && let Some(s) = symbol
        {
            lines.push(SymbolLine {
                function: Some(s.demangled.unwrap_or(s.name)),
                offset: Some(s.offset),
                file: None,
                line: None,
                column: None,
                inlined: false,
            });
        }
        lines
    }
}

/// Symbolicates the frames of the images there are binaries for (`found`);
/// `notes` say why other images weren't (image index, note).
pub fn symbolicate(report: &CrashReport, found: &[Found<'_>], notes: &[(u32, String)]) -> Symbolicated {
    let binary_for = |image: Option<u32>| found.iter().find(|f| Some(f.image) == image);
    let threads = report
        .threads
        .iter()
        .map(|t| SymbolicatedThread {
            name: t.name.clone(),
            crashed: t.crashed,
            frames: t
                .frames
                .iter()
                .map(|f| {
                    let hit = binary_for(f.image);
                    let image = f.image.and_then(|i| report.images.get(i as usize));
                    let binary_address = hit.and_then(|h| {
                        let base = h.bin.link_base();
                        f.linked.or_else(|| f.offset.map(|o| base.wrapping_add(o))).or_else(|| {
                            let (a, load) = (f.address?, image?.load_address?);
                            a.checked_sub(load).map(|o| base.wrapping_add(o))
                        })
                    });
                    // Frames past the first hold return addresses: the call is just
                    // before, which is where to look (but the offset is the frame's).
                    let lines = match (hit, binary_address) {
                        (Some(h), Some(a)) if f.index > 0 && a > 0 => {
                            let mut lines = h.bin.symbolize(a - 1);
                            for l in &mut lines {
                                l.offset = l.offset.map(|o| o + 1);
                            }
                            lines
                        }
                        (Some(h), Some(a)) => h.bin.symbolize(a),
                        _ => Vec::new(),
                    };
                    SymbolicatedFrame {
                        index: f.index,
                        image: f.image,
                        image_name: image.map_or_else(|| f.image_name.clone(), |i| i.name.clone()),
                        address: f.address,
                        binary_address,
                        binary: hit.map(|h| h.binary),
                        lines,
                        reported: f.symbol.clone(),
                    }
                })
                .collect(),
        })
        .collect();
    let images = report
        .images_in_frames()
        .into_iter()
        .map(|image| ImageStatus {
            image,
            frames: report
                .threads
                .iter()
                .flat_map(|t| &t.frames)
                .filter(|f| f.image == Some(image))
                .count() as u32,
            binary: binary_for(Some(image)).map(|f| f.binary),
            note: notes.iter().find(|(i, _)| *i == image).map(|(_, n)| n.clone()),
        })
        .collect();
    Symbolicated {
        report: report.clone(),
        images,
        threads,
    }
}

// --- Parsing ------------------------------------------------------------------------

/// Reads a crash report: Apple's (text or `.ips`), an Android tombstone, or
/// a stack trace with images and offsets. `None` when it is none of those.
pub fn parse(text: &str) -> Option<CrashReport> {
    let text = text.trim_start_matches('\u{feff}');
    let report = if text.trim_start().starts_with('{') {
        parse_ips(text)
    } else if text.lines().any(|l| tombstone_frame(l).is_some()) {
        parse_tombstone(text)
    } else {
        parse_apple(text)
    }?;
    report.threads.iter().any(|t| !t.frames.is_empty()).then_some(report)
}

fn hex(s: &str) -> Option<u64> {
    let s = s.trim();
    u64::from_str_radix(s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s), 16).ok()
}

fn value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.strip_prefix(key).map(str::trim).filter(|v| !v.is_empty())
}

/// An Apple report's frame line: `3   MyApp   0x0000000100f2c7a4 main + 28`.
fn apple_frame(line: &str) -> Option<CrashFrame> {
    let t = line.trim_start();
    let (index, rest) = t.split_once(char::is_whitespace)?;
    let index: u32 = index.parse().ok()?;
    // The address is the first 0x token after the image's name (which may have spaces).
    let at = rest
        .match_indices("0x")
        .map(|(i, _)| i)
        .find(|&i| i > 0 && rest[..i].ends_with(char::is_whitespace))?;
    let image_name = rest[..at].trim().to_string();
    let after = &rest[at..];
    let token_end = after.find(char::is_whitespace).unwrap_or(after.len());
    let address = hex(&after[..token_end])?;
    let tail = after[token_end..].trim();
    let mut frame = CrashFrame {
        index,
        image_name,
        address: Some(address),
        ..CrashFrame::default()
    };
    // "0x100e24000 + 1083300": the load address, and a decimal offset.
    if let Some((load, off)) = tail.split_once(" + ")
        && let (Some(_), Ok(off)) = (
            hex(load).filter(|_| load.trim().starts_with("0x")),
            off.trim().parse::<u64>(),
        )
    {
        frame.offset = Some(off);
    } else if !tail.is_empty() {
        frame.symbol = Some(tail.to_string());
    }
    Some(frame)
}

/// A `Binary Images:` line: `0x100e24000 - 0x101223fff +MyApp arm64  <9a8f…> /path/MyApp`.
fn apple_image(line: &str) -> Option<CrashImage> {
    let t = line.trim();
    let (start, rest) = t.split_once('-')?;
    let load = hex(start.trim()).filter(|_| start.trim().starts_with("0x"))?;
    let rest = rest.trim_start();
    let end_len = rest.find(char::is_whitespace)?;
    let end = hex(&rest[..end_len])?;
    let rest = rest[end_len..].trim();
    let (before, after) = rest.split_once('<')?;
    let (uuid, path) = after.split_once('>')?;
    let mut before = before.trim();
    // A version in parentheses: "com.example.MyApp (1.0 - 1)".
    if before.ends_with(')')
        && let Some(i) = before.rfind(" (")
    {
        before = before[..i].trim_end();
    }
    let is_arch = |w: &str| ["arm", "x86", "i386", "ppc", "???"].iter().any(|a| w.starts_with(a));
    // The name may have spaces ("MyApp Widget Extension"); the architecture is the last word.
    let (name, arch) = match before.rsplit_once(char::is_whitespace) {
        Some((name, arch)) if is_arch(arch) => (name.trim(), Some(arch.to_string())),
        _ => (before, None),
    };
    Some(CrashImage {
        name: name.trim_start_matches('+').trim().to_string(),
        path: Some(path.trim().to_string()).filter(|p| !p.is_empty()),
        id: Some(uuid.trim().to_string()).filter(|u| !u.is_empty()),
        arch,
        load_address: Some(load),
        size: end.checked_sub(load).map(|n| n + 1),
    })
}

fn parse_apple(text: &str) -> Option<CrashReport> {
    let mut r = CrashReport {
        format: "Apple crash report".into(),
        ..CrashReport::default()
    };
    let mut threads: Vec<CrashThread> = Vec::new();
    let mut thread_names: Vec<(u32, String)> = Vec::new();
    let mut current: Option<usize> = None;
    let mut in_images = false;
    let mut asi: Option<String> = None;
    let mut in_asi = false;
    for line in text.lines() {
        let t = line.trim();
        if in_images {
            if let Some(image) = apple_image(line) {
                r.images.push(image);
            }
            continue;
        }
        if in_asi {
            if t.is_empty() {
                in_asi = false;
            } else {
                let a = asi.get_or_insert_with(String::new);
                if !a.is_empty() {
                    a.push('\n');
                }
                a.push_str(t);
            }
            continue;
        }
        if t.starts_with("Binary Images:") {
            in_images = true;
            current = None;
            continue;
        }
        if t.starts_with("Application Specific Information:") {
            in_asi = true;
            continue;
        }
        if let Some(v) = value(t, "Process:") {
            r.process = Some(v.split(" [").next().unwrap_or(v).to_string());
        } else if let Some(v) = value(t, "Identifier:") {
            r.identifier = Some(v.to_string());
        } else if let Some(v) = value(t, "Version:") {
            r.version = Some(v.to_string());
        } else if let Some(v) = value(t, "OS Version:") {
            r.os = Some(v.to_string());
        } else if let Some(v) = value(t, "Code Type:") {
            r.arch = Some(v.to_string());
        } else if let Some(v) = value(t, "Exception Type:") {
            r.exception = Some(v.to_string());
        } else if let Some(v) = value(t, "Termination Reason:") {
            r.reason = Some(v.to_string());
        } else if let Some(rest) = t.strip_prefix("Thread ") {
            // "Thread 0 name:  Dispatch queue: …", "Thread 0 Crashed:", "Thread 3:"
            let (num, what) = rest.split_once([' ', ':']).unwrap_or((rest, ""));
            if let Ok(n) = num.parse::<u32>() {
                let what = what.trim();
                if let Some(name) = what.strip_prefix("name:") {
                    thread_names.push((n, name.trim().to_string()));
                } else if what.starts_with("Crashed") || what.is_empty() || what == ":" {
                    threads.push(CrashThread {
                        name: format!("Thread {n}"),
                        crashed: what.starts_with("Crashed"),
                        frames: Vec::new(),
                    });
                    current = Some(threads.len() - 1);
                } else {
                    current = None;
                }
            } else {
                current = None;
            }
        } else if t.starts_with("Last Exception Backtrace:") {
            threads.push(CrashThread {
                name: "Last Exception Backtrace".into(),
                crashed: true,
                frames: Vec::new(),
            });
            current = Some(threads.len() - 1);
        } else if let Some(frame) = apple_frame(line) {
            let i = *current.get_or_insert_with(|| {
                threads.push(CrashThread {
                    name: "Stack".into(),
                    crashed: true,
                    frames: Vec::new(),
                });
                threads.len() - 1
            });
            threads[i].frames.push(frame);
        } else if t.is_empty() {
            current = None;
        }
    }
    if r.reason.is_none() {
        r.reason = asi;
    }
    for (n, name) in thread_names {
        if let Some(t) = threads.iter_mut().find(|t| t.name == format!("Thread {n}")) {
            t.name = format!("Thread {n}: {name}");
        }
    }
    // Frames point into images by address range, else by name.
    for f in threads.iter_mut().flat_map(|t| &mut t.frames) {
        {
            f.image = r
                .images
                .iter()
                .position(|i| {
                    let (Some(a), Some(load), Some(size)) = (f.address, i.load_address, i.size) else {
                        return false;
                    };
                    a >= load && a - load < size
                })
                .or_else(|| {
                    r.images
                        .iter()
                        .position(|i| i.name == f.image_name || i.file_name() == f.image_name)
                })
                .map(|i| i as u32);
            // A frame given as "load + offset" names its image's load address.
            if let (Some(off), Some(a), Some(i)) = (f.offset, f.address, f.image) {
                let image = &mut r.images[i as usize];
                image.load_address.get_or_insert(a.wrapping_sub(off));
            }
        }
    }
    // Images a pasted trace only names, through its "load + offset" frames.
    for f in threads.iter_mut().flat_map(|t| &mut t.frames) {
        let (None, Some(off)) = (f.image, f.offset) else {
            continue;
        };
        let i = match r.images.iter().position(|img| img.name == f.image_name) {
            Some(i) => i,
            None => {
                r.images.push(CrashImage {
                    name: f.image_name.clone(),
                    load_address: f.address.map(|a| a.wrapping_sub(off)),
                    ..CrashImage::default()
                });
                r.images.len() - 1
            }
        };
        f.image = Some(i as u32);
    }
    if threads.iter().any(|t| t.name == "Stack") && r.images.iter().all(|i| i.id.is_none()) {
        r.format = "Stack trace".into();
    }
    threads.sort_by_key(|t| !t.crashed);
    r.threads = threads;
    Some(r)
}

/// A tombstone's frame line: `#00 pc 000000000004f0a8  /data/app/…/libnative.so (Java_crash+24) (BuildId: 1a2b…)`.
fn tombstone_frame(line: &str) -> Option<(CrashFrame, Option<String>)> {
    let hash = line.find('#')?;
    let rest = &line[hash + 1..];
    let (index, rest) = rest.split_once(char::is_whitespace)?;
    let index: u32 = index.parse().ok()?;
    let rest = rest.trim_start().strip_prefix("pc")?.trim_start();
    let (pc, rest) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let linked = hex(pc)?;
    let mut rest = rest.trim();
    let mut build_id = None;
    if let Some(i) = rest.rfind("(BuildId:")
        && rest.ends_with(')')
    {
        build_id = Some(rest[i + 9..rest.len() - 1].trim().to_string());
        rest = rest[..i].trim_end();
    }
    // An offset into a file mapped at an offset: "(offset 0x1000)".
    if let Some(i) = rest.rfind("(offset ") {
        rest = rest[..i].trim_end();
    }
    let mut symbol = None;
    if rest.ends_with(')') {
        // The symbol's parentheses may nest (C++): find the one that opens it.
        let mut depth = 0;
        for (i, c) in rest.char_indices().rev() {
            match c {
                ')' => depth += 1,
                '(' => {
                    depth -= 1;
                    if depth == 0 {
                        symbol = Some(rest[i + 1..rest.len() - 1].to_string());
                        rest = rest[..i].trim_end();
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    let path = rest.trim();
    if path.is_empty() {
        return None;
    }
    // The frame's image name is its path, until the caller finds the image.
    let frame = CrashFrame {
        index,
        image_name: path.to_string(),
        linked: Some(linked),
        symbol,
        ..CrashFrame::default()
    };
    Some((frame, build_id))
}

fn parse_tombstone(text: &str) -> Option<CrashReport> {
    let mut r = CrashReport {
        format: "Android tombstone".into(),
        ..CrashReport::default()
    };
    let mut threads: Vec<CrashThread> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(v) = value(t, "Build fingerprint:") {
            r.os.get_or_insert_with(|| v.trim_matches('\'').to_string());
        } else if let Some(v) = value(t, "ABI:") {
            r.arch.get_or_insert_with(|| v.trim_matches('\'').to_string());
        } else if t.starts_with("pid:") && t.contains("tid:") {
            // "pid: 1234, tid: 1240, name: RenderThread  >>> com.example <<<"
            let name = t
                .split("name:")
                .nth(1)
                .map(|n| n.split(">>>").next().unwrap_or(n).trim().to_string())
                .unwrap_or_default();
            if let Some(p) = t.split(">>>").nth(1).and_then(|p| p.split("<<<").next()) {
                r.process.get_or_insert_with(|| p.trim().to_string());
            }
            threads.push(CrashThread {
                crashed: threads.is_empty(),
                name,
                frames: Vec::new(),
            });
        } else if t.starts_with("signal ") && r.exception.is_none() {
            r.exception = Some(t.to_string());
        } else if let Some(v) = value(t, "Abort message:") {
            r.reason.get_or_insert_with(|| v.trim_matches('\'').to_string());
        } else if let Some((mut frame, id)) = tombstone_frame(line) {
            if threads.is_empty() {
                threads.push(CrashThread {
                    name: "backtrace".into(),
                    crashed: true,
                    frames: Vec::new(),
                });
            }
            let path = std::mem::take(&mut frame.image_name);
            let image = match r
                .images
                .iter()
                .position(|i| i.path.as_deref() == Some(path.as_str()) && i.id == id)
            {
                Some(i) => i,
                None => {
                    r.images.push(CrashImage {
                        name: path.rsplit('/').next().unwrap_or(&path).to_string(),
                        path: Some(path.clone()),
                        id,
                        ..CrashImage::default()
                    });
                    r.images.len() - 1
                }
            };
            frame.image_name = r.images[image].name.clone();
            frame.image = Some(image as u32);
            threads.last_mut().expect("a thread").frames.push(frame);
        }
    }
    r.threads = threads;
    Some(r)
}

fn parse_ips(text: &str) -> Option<CrashReport> {
    use serde_json::Value;
    let text = text.trim_start();
    // A header line, then the report; or the report alone.
    let (header, body): (Value, Value) = match text.split_once('\n') {
        Some((first, rest)) if !rest.trim().is_empty() => (
            serde_json::from_str(first.trim()).ok()?,
            serde_json::from_str(rest.trim()).ok()?,
        ),
        _ => (Value::Null, serde_json::from_str(text).ok()?),
    };
    let s = |v: &Value| v.as_str().map(str::to_string);
    let used = body.get("usedImages")?.as_array()?;
    let mut r = CrashReport {
        format: "Apple crash report (.ips)".into(),
        process: s(&body["procName"]).or_else(|| s(&header["app_name"])),
        identifier: s(&body["bundleInfo"]["CFBundleIdentifier"]).or_else(|| s(&header["bundleID"])),
        version: match (
            s(&body["bundleInfo"]["CFBundleShortVersionString"]).or_else(|| s(&header["app_version"])),
            s(&body["bundleInfo"]["CFBundleVersion"]).or_else(|| s(&header["build_version"])),
        ) {
            (Some(v), Some(b)) => Some(format!("{v} ({b})")),
            (v, b) => v.or(b),
        },
        os: s(&header["os_version"]).or_else(|| {
            let os = &body["osVersion"];
            Some(format!(
                "{} ({})",
                s(&os["train"])?,
                s(&os["build"]).unwrap_or_default()
            ))
        }),
        arch: s(&body["cpuType"]),
        exception: {
            let e = &body["exception"];
            s(&e["type"]).map(|t| match s(&e["signal"]) {
                Some(sig) => format!("{t} ({sig})"),
                None => t,
            })
        },
        reason: {
            let t = &body["termination"];
            s(&t["indicator"])
                .or_else(|| {
                    t["reasons"]
                        .as_array()
                        .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; "))
                })
                .or_else(|| {
                    let asi = body["asi"].as_object()?;
                    Some(
                        asi.values()
                            .flat_map(|v| v.as_array().into_iter().flatten())
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join("\n"),
                    )
                })
                .filter(|r| !r.is_empty())
        },
        ..CrashReport::default()
    };
    for img in used {
        r.images.push(CrashImage {
            name: s(&img["name"])
                .or_else(|| s(&img["path"]).map(|p| p.rsplit('/').next().unwrap_or(&p).to_string()))
                .unwrap_or_else(|| "???".into()),
            path: s(&img["path"]),
            id: s(&img["uuid"]),
            arch: s(&img["arch"]),
            load_address: img["base"].as_u64(),
            size: img["size"].as_u64(),
        });
    }
    let frames = |list: &Value| -> Vec<CrashFrame> {
        list.as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, f)| {
                let image = f["imageIndex"].as_u64().map(|i| i as u32);
                let img = image.and_then(|i| r.images.get(i as usize));
                let offset = f["imageOffset"].as_u64();
                CrashFrame {
                    index: i as u32,
                    image,
                    image_name: img.map(|i| i.name.clone()).unwrap_or_default(),
                    address: offset
                        .zip(img.and_then(|i| i.load_address))
                        .map(|(o, l)| l.wrapping_add(o)),
                    offset,
                    symbol: s(&f["symbol"]).map(|sym| match f["symbolLocation"].as_u64() {
                        Some(off) => format!("{sym} + {off}"),
                        None => sym,
                    }),
                    ..CrashFrame::default()
                }
            })
            .collect()
    };
    if body["lastExceptionBacktrace"].is_array() {
        r.threads.push(CrashThread {
            name: "Last Exception Backtrace".into(),
            crashed: true,
            frames: frames(&body["lastExceptionBacktrace"]),
        });
    }
    for (i, t) in body["threads"].as_array().into_iter().flatten().enumerate() {
        let mut name = format!("Thread {i}");
        if let Some(n) = s(&t["name"]).or_else(|| s(&t["queue"])) {
            name = format!("{name}: {n}");
        }
        r.threads.push(CrashThread {
            name,
            crashed: t["triggered"].as_bool().unwrap_or(false),
            frames: frames(&t["frames"]),
        });
    }
    r.threads.sort_by_key(|t| !t.crashed);
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPLE: &str = "Incident Identifier: 4E1F0D9A-0000-0000-0000-000000000000
Hardware Model:      iPhone14,2
Process:             MyApp [1234]
Path:                /private/var/containers/Bundle/Application/X/MyApp.app/MyApp
Identifier:          com.example.myapp
Version:             4.2 (1234)
Code Type:           ARM-64 (Native)
OS Version:          iPhone OS 17.2 (21C62)

Exception Type:  EXC_BAD_ACCESS (SIGSEGV)
Termination Reason: SIGNAL 11 Segmentation fault: 11

Thread 0 name:  Dispatch queue: com.apple.main-thread
Thread 0 Crashed:
0   MyApp                         \t0x0000000104f2c7a4 0x104e24000 + 1083300
1   MyApp Widget Extension        \t0x00000001050101a0 main + 16
2   libdyld.dylib                 \t0x00000001b4b4c9e8 start + 4

Thread 1:
0   libsystem_kernel.dylib        \t0x00000001b4b44000 __workq_kernreturn + 8

Binary Images:
       0x104e24000 -        0x104f3ffff MyApp arm64  <9a8f0e3b2c1d4e5f8a9b0c1d2e3f4a5b> /private/var/containers/Bundle/Application/X/MyApp.app/MyApp
       0x105010000 -        0x105013fff MyApp Widget Extension arm64  <aaaabbbbccccddddeeeeffff00001111> /private/var/X/Widget.appex/Widget
       0x1b4b44000 -        0x1b4b7bfff libsystem_kernel.dylib arm64e  <b3a1e0f1c2d3e4f5a6b7c8d9e0f1a2b3> /usr/lib/system/libsystem_kernel.dylib
";

    #[test]
    fn apple_text_reports() {
        let r = parse(APPLE).expect("parsed");
        assert_eq!(r.format, "Apple crash report");
        assert_eq!(r.process.as_deref(), Some("MyApp"));
        assert_eq!(r.version.as_deref(), Some("4.2 (1234)"));
        assert_eq!(r.exception.as_deref(), Some("EXC_BAD_ACCESS (SIGSEGV)"));
        assert_eq!(r.images.len(), 3);
        assert_eq!(r.images[0].id.as_deref(), Some("9a8f0e3b2c1d4e5f8a9b0c1d2e3f4a5b"));
        assert_eq!(r.images[0].size, Some(0x11c000));
        let t = &r.threads[0];
        assert!(t.crashed);
        assert_eq!(t.name, "Thread 0: Dispatch queue: com.apple.main-thread");
        assert_eq!(t.frames[0].offset, Some(1083300));
        assert_eq!(t.frames[0].image, Some(0));
        // An image name with spaces; a frame by address range.
        assert_eq!(t.frames[1].image_name, "MyApp Widget Extension");
        assert_eq!(t.frames[1].image, Some(1));
        assert_eq!(t.frames[1].symbol.as_deref(), Some("main + 16"));
        assert_eq!(r.threads[1].frames.len(), 1);
        // Matching: by UUID (any case, dashes or not), another build by name.
        let img = &r.images[0];
        assert_eq!(
            img.matches(["9A8F0E3B-2C1D-4E5F-8A9B-0C1D2E3F4A5B"], "Whatever"),
            ImageMatch::Yes
        );
        assert_eq!(
            img.matches(["00000000-0000-0000-0000-000000000000"], "MyApp"),
            ImageMatch::OtherBuild
        );
        assert_eq!(
            img.matches(["00000000-0000-0000-0000-000000000000"], "Other"),
            ImageMatch::No
        );
    }

    #[test]
    fn ips_reports() {
        let ips = r#"{"app_name":"MyApp","bug_type":"309","os_version":"iPhone OS 17.2 (21C62)","app_version":"4.2","build_version":"1234"}
{
  "procName" : "MyApp",
  "cpuType" : "ARM-64",
  "exception" : {"type" : "EXC_CRASH", "signal" : "SIGABRT"},
  "termination" : {"indicator" : "Abort trap: 6"},
  "threads" : [
    {"id" : 1, "frames" : [{"imageOffset" : 100, "imageIndex" : 1}]},
    {"triggered" : true, "queue" : "com.apple.main-thread", "frames" : [
      {"imageOffset" : 1083300, "imageIndex" : 0},
      {"imageOffset" : 2048, "symbol" : "abort", "symbolLocation" : 12, "imageIndex" : 1}
    ]}
  ],
  "usedImages" : [
    {"source" : "P", "arch" : "arm64", "base" : 4377952256, "size" : 4194304, "uuid" : "9a8f0e3b-2c1d-4e5f-8a9b-0c1d2e3f4a5b", "path" : "/private/var/X/MyApp.app/MyApp", "name" : "MyApp"},
    {"source" : "P", "arch" : "arm64e", "base" : 7327698944, "size" : 229376, "uuid" : "b3a1e0f1-c2d3-e4f5-a6b7-c8d9e0f1a2b3", "path" : "/usr/lib/system/libsystem_c.dylib", "name" : "libsystem_c.dylib"}
  ]
}"#;
        let r = parse(ips).expect("parsed");
        assert_eq!(r.format, "Apple crash report (.ips)");
        assert_eq!(r.version.as_deref(), Some("4.2 (1234)"));
        assert_eq!(r.exception.as_deref(), Some("EXC_CRASH (SIGABRT)"));
        assert_eq!(r.reason.as_deref(), Some("Abort trap: 6"));
        // The triggered thread comes first.
        assert_eq!(r.threads[0].name, "Thread 1: com.apple.main-thread");
        assert!(r.threads[0].crashed);
        assert_eq!(r.threads[0].frames[0].offset, Some(1083300));
        assert_eq!(r.threads[0].frames[1].symbol.as_deref(), Some("abort + 12"));
        assert_eq!(r.images[0].load_address, Some(4377952256));
    }

    #[test]
    fn tombstones() {
        let t = "*** *** *** *** *** *** *** *** *** *** *** *** *** *** *** ***
Build fingerprint: 'google/raven/raven:14/UQ1A.240205.004/11269751:user/release-keys'
ABI: 'arm64'
pid: 4321, tid: 4321, name: example.app  >>> com.example.app <<<
signal 11 (SIGSEGV), code 1 (SEGV_MAPERR), fault addr 0x0000000000000000
Abort message: 'something broke'
backtrace:
      #00 pc 000000000004f0a8  /data/app/~~x==/com.example.app/lib/arm64/libnative.so (Java_com_example_crash(int)+24) (BuildId: 1a2b3c4d)
      #01 pc 0000000000355830  /apex/com.android.art/lib64/libart.so (art_quick_generic_jni_trampoline+144) (BuildId: 99887766)
      #02 pc 0000000000011111  /data/app/~~x==/com.example.app/lib/arm64/libnative.so (BuildId: 1a2b3c4d)
";
        let r = parse(t).expect("parsed");
        assert_eq!(r.format, "Android tombstone");
        assert_eq!(r.process.as_deref(), Some("com.example.app"));
        assert_eq!(r.arch.as_deref(), Some("arm64"));
        assert_eq!(r.reason.as_deref(), Some("something broke"));
        assert_eq!(r.images.len(), 2, "one image per library");
        assert_eq!(r.images[0].name, "libnative.so");
        assert_eq!(r.images[0].id.as_deref(), Some("1a2b3c4d"));
        let f = &r.threads[0].frames;
        assert_eq!(f[0].linked, Some(0x4f0a8));
        assert_eq!(f[0].symbol.as_deref(), Some("Java_com_example_crash(int)+24"));
        assert_eq!((f[0].image, f[2].image), (Some(0), Some(0)));
        assert!(f[2].symbol.is_none());
    }

    #[test]
    fn not_crash_reports() {
        assert!(parse("hello world").is_none());
        assert!(parse("{\"a\": 1}").is_none());
    }
}
