//! Patches, the way ROM hacks and translations are shared: IPS, UPS and
//! BPS. Applying one gives the patched file and what it changes, runs of
//! changed bytes that [`place`] puts in banks, functions and regions;
//! creating one saves edits to share without the game itself.

use serde::Serialize;

use crate::binary::Binary;
use crate::error::{Error, Result, bail};

/// A patch format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PatchFormat {
    /// International Patching System: bytes to write at offsets below 16
    /// MiB, runs of one value compressed; no checksums.
    Ips,
    /// Universal Patching System: the XOR of the two files, with their sizes
    /// and CRC-32s. It works both ways.
    Ups,
    /// Beat Patching System: the new file built from copies of the old one,
    /// of itself and new bytes, with CRC-32s.
    Bps,
}

impl PatchFormat {
    /// The format of `data`, from its first bytes.
    pub fn detect(data: &[u8]) -> Option<PatchFormat> {
        if data.starts_with(b"PATCH") {
            Some(PatchFormat::Ips)
        } else if data.starts_with(b"UPS1") {
            Some(PatchFormat::Ups)
        } else if data.starts_with(b"BPS1") {
            Some(PatchFormat::Bps)
        } else {
            None
        }
    }

    /// A format by name or file extension (`ips`, `.bps`...).
    pub fn from_name(name: &str) -> Option<PatchFormat> {
        match name.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "ips" => Some(PatchFormat::Ips),
            "ups" => Some(PatchFormat::Ups),
            "bps" => Some(PatchFormat::Bps),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PatchFormat::Ips => "IPS",
            PatchFormat::Ups => "UPS",
            PatchFormat::Bps => "BPS",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            PatchFormat::Ips => "ips",
            PatchFormat::Ups => "ups",
            PatchFormat::Bps => "bps",
        }
    }
}

/// What a patch says about itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchInfo {
    pub format: PatchFormat,
    /// IPS records, UPS runs or BPS actions.
    pub records: u64,
    /// UPS, BPS: the size and CRC-32 of the file the patch is for...
    pub source_size: Option<u64>,
    pub source_crc32: Option<u32>,
    /// ...and of the file it makes.
    pub target_size: u64,
    pub target_crc32: Option<u32>,
    /// BPS: whatever the author put there (often XML naming the hack).
    pub metadata: Option<String>,
    /// IPS: the size the file is cut to (a Lunar IPS extension).
    pub truncate: Option<u64>,
}

/// How a run of bytes changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    /// Bytes the patched file has in place of the original's.
    Changed,
    /// Bytes past the original's end.
    Added,
    /// The original's bytes past the patched file's end.
    Removed,
}

/// A run of changed bytes, at the same offset in both files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub kind: ChangeKind,
    pub offset: u64,
    pub len: u64,
    /// How many of its bytes differ: changes a few equal bytes apart are one run.
    pub differ: u64,
}

/// A patch applied to a file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub info: PatchInfo,
    pub changes: Vec<Change>,
    /// Bytes that differ in all (added and removed ones included).
    pub differ: u64,
    /// UPS, BPS: whether the file patched is the one the patch was made for.
    pub source_matches: Option<bool>,
    /// UPS, BPS: whether the result is the file the patch promises.
    pub target_matches: Option<bool>,
    /// A copier header the patch was made without (512 bytes before an SNES
    /// ROM): skipped when patching, kept in the result.
    pub skipped_header: u64,
    /// UPS: the file was the patch's result, and was turned back into its original.
    pub reversed: bool,
    /// What to know before trusting the result.
    pub warnings: Vec<String>,
    /// The patched file.
    #[serde(skip)]
    pub output: Vec<u8>,
}

/// The largest file a patch may make.
const MAX_OUTPUT: u64 = 1 << 30;

/// Changes closer than this are one run.
const GAP: usize = 8;

/// Applies an IPS, UPS or BPS patch to `source`.
pub fn apply(patch: &[u8], source: &[u8]) -> Result<Applied> {
    let format = PatchFormat::detect(patch).ok_or_else(|| Error::new("not an IPS, UPS or BPS patch"))?;
    if format == PatchFormat::Ips {
        return finish(apply_ips(patch, source)?, source, 0);
    }
    let footer = Footer::read(patch, format)?;
    let mut applied = apply_checked(patch, format, &footer, source)?;
    // A patch made for the ROM without the 512-byte header copiers put before SNES games.
    if applied.source_matches == Some(false) && source.len() % 1024 == 512 && crc32(&source[512..]) == footer.source_crc
    {
        let mut inner = apply_checked(patch, format, &footer, &source[512..])?;
        let mut output = source[..512].to_vec();
        output.append(&mut inner.output);
        inner.output = output;
        return finish(inner, source, 512);
    }
    if !footer.patch_ok {
        applied
            .warnings
            .push("The patch is damaged: its own CRC-32 doesn't match its bytes.".into());
    }
    finish(applied, source, 0)
}

/// A UPS or BPS patch's checksums.
struct Footer {
    source_crc: u32,
    target_crc: u32,
    patch_ok: bool,
}

impl Footer {
    fn read(patch: &[u8], format: PatchFormat) -> Result<Footer> {
        if patch.len() < 4 + 12 {
            bail!("the {} patch is cut short", format.name());
        }
        let at = patch.len() - 12;
        let le = |i: usize| u32::from_le_bytes(patch[at + i..at + i + 4].try_into().unwrap());
        Ok(Footer {
            source_crc: le(0),
            target_crc: le(4),
            patch_ok: crc32(&patch[..patch.len() - 4]) == le(8),
        })
    }
}

/// Applies a UPS or BPS patch, checking the CRC-32s it carries.
fn apply_checked(patch: &[u8], format: PatchFormat, footer: &Footer, source: &[u8]) -> Result<Applied> {
    let mut applied = match format {
        PatchFormat::Ups => apply_ups(patch, footer, source)?,
        _ => apply_bps(patch, source)?,
    };
    let (want_source, want_target) = if applied.reversed {
        (footer.target_crc, footer.source_crc)
    } else {
        (footer.source_crc, footer.target_crc)
    };
    let have = crc32(source);
    applied.source_matches = Some(have == want_source);
    applied.target_matches = Some(crc32(&applied.output) == want_target);
    if have != want_source {
        applied.warnings.push(format!(
            "This patch is for another file: it wants {} bytes with CRC-32 {want_source:08X}; this file is {} bytes, CRC-32 {have:08X}. The result is likely wrong: check the game's version and region.",
            applied.info.source_size.unwrap_or(0),
            source.len(),
        ));
    } else if applied.target_matches == Some(false) {
        applied.warnings.push(format!(
            "The patched file's CRC-32 isn't the {want_target:08X} the patch promises."
        ));
    }
    Ok(applied)
}

fn finish(mut applied: Applied, source: &[u8], header: usize) -> Result<Applied> {
    applied.skipped_header = header as u64;
    if header > 0 {
        applied.warnings.insert(
            0,
            format!("The patch was made for the ROM without its {header}-byte copier header: it was applied after it."),
        );
    }
    applied.changes = changes(source, &applied.output);
    applied.differ = applied.changes.iter().map(|c| c.differ).sum();
    Ok(applied)
}

fn applied(info: PatchInfo, output: Vec<u8>) -> Applied {
    Applied {
        info,
        changes: Vec::new(),
        differ: 0,
        source_matches: None,
        target_matches: None,
        skipped_header: 0,
        reversed: false,
        warnings: Vec::new(),
        output,
    }
}

fn apply_ips(patch: &[u8], source: &[u8]) -> Result<Applied> {
    let mut r = Reader::new(patch, 5);
    let mut out = source.to_vec();
    let mut records = 0;
    let mut truncate = None;
    let mut warnings = Vec::new();
    loop {
        let left = patch.len() - r.pos;
        if left == 0 {
            warnings.push("The IPS patch has no end marker: it may be cut short.".into());
            break;
        }
        // "EOF" ends the records, unless a record starts at $454F46.
        if patch[r.pos..].starts_with(b"EOF") && (left == 3 || left == 6) {
            if left == 6 {
                truncate = Some(u64::from(be24(&patch[r.pos + 3..])));
            }
            break;
        }
        let offset = r.be24()? as usize;
        let size = r.be16()? as usize;
        let (len, run) = if size == 0 {
            (r.be16()? as usize, Some(r.u8()?))
        } else {
            (size, None)
        };
        if out.len() < offset + len {
            out.resize(offset + len, 0);
        }
        match run {
            Some(value) => out[offset..offset + len].fill(value),
            None => out[offset..offset + len].copy_from_slice(r.bytes(len as u64)?),
        }
        records += 1;
    }
    if let Some(t) = truncate {
        out.truncate(t as usize);
    }
    let info = PatchInfo {
        format: PatchFormat::Ips,
        records,
        source_size: None,
        source_crc32: None,
        target_size: out.len() as u64,
        target_crc32: None,
        metadata: None,
        truncate,
    };
    let mut a = applied(info, out);
    a.warnings = warnings;
    Ok(a)
}

fn apply_ups(patch: &[u8], footer: &Footer, source: &[u8]) -> Result<Applied> {
    let body = &patch[..patch.len() - 12];
    let mut r = Reader::new(body, 4);
    let size_a = r.varint()?;
    let size_b = r.varint()?;
    // UPS works both ways: a file that is the patch's result goes back to its original.
    let crc = crc32(source);
    let reversed = source.len() as u64 == size_b && crc == footer.target_crc && crc != footer.source_crc;
    let (from, to) = if reversed { (size_b, size_a) } else { (size_a, size_b) };
    check_size(to)?;
    let mut out = source[..source.len().min(to as usize)].to_vec();
    out.resize(to as usize, 0);
    let mut pos = 0u64;
    let mut records = 0;
    while r.pos < body.len() {
        pos = pos
            .checked_add(r.varint()?)
            .ok_or_else(|| Error::new("an offset in the UPS patch is too large"))?;
        loop {
            let x = r.u8()?;
            if let Some(b) = out.get_mut(pos as usize) {
                *b ^= x;
            }
            pos += 1;
            if x == 0 {
                break;
            }
        }
        records += 1;
    }
    let info = PatchInfo {
        format: PatchFormat::Ups,
        records,
        source_size: Some(from),
        source_crc32: Some(if reversed { footer.target_crc } else { footer.source_crc }),
        target_size: to,
        target_crc32: Some(if reversed { footer.source_crc } else { footer.target_crc }),
        metadata: None,
        truncate: None,
    };
    let mut a = applied(info, out);
    a.reversed = reversed;
    if reversed {
        a.warnings
            .push("This file is the patch's result: patching it gave back the original.".into());
    }
    Ok(a)
}

fn apply_bps(patch: &[u8], source: &[u8]) -> Result<Applied> {
    let footer = Footer::read(patch, PatchFormat::Bps)?;
    let body = &patch[..patch.len() - 12];
    let mut r = Reader::new(body, 4);
    let source_size = r.varint()?;
    let target_size = r.varint()?;
    let meta_size = r.varint()?;
    let metadata = r.bytes(meta_size)?;
    check_size(target_size)?;
    let target = target_size as usize;
    let mut out: Vec<u8> = Vec::with_capacity(target);
    let (mut source_at, mut target_at) = (0i64, 0i64);
    let mut records = 0;
    while r.pos < body.len() {
        let data = r.varint()?;
        let length = (data >> 2) + 1;
        if out.len() as u64 + length > target_size {
            bail!("the BPS patch writes past the end of the file it makes");
        }
        let length = length as usize;
        match data & 3 {
            // Bytes at the same place in the original.
            0 => {
                let at = out.len();
                let have = source.len().saturating_sub(at).min(length);
                out.extend_from_slice(&source[at.min(source.len())..at.min(source.len()) + have]);
                out.resize(at + length, 0);
            }
            // New bytes.
            1 => out.extend_from_slice(r.bytes(length as u64)?),
            // Bytes from elsewhere in the original.
            2 => {
                source_at += signed(r.varint()?);
                let at = usize::try_from(source_at)
                    .ok()
                    .filter(|&at| at + length <= source.len())
                    .ok_or_else(|| Error::new("the BPS patch copies from outside the original file"))?;
                out.extend_from_slice(&source[at..at + length]);
                source_at += length as i64;
            }
            // Bytes from what was made so far (overlapping, byte by byte).
            _ => {
                target_at += signed(r.varint()?);
                let at = usize::try_from(target_at)
                    .ok()
                    .filter(|&at| at < out.len())
                    .ok_or_else(|| Error::new("the BPS patch copies from outside the file it makes"))?;
                for i in at..at + length {
                    let b = out[i];
                    out.push(b);
                }
                target_at += length as i64;
            }
        }
        records += 1;
    }
    let short = out.len() < target;
    out.resize(target, 0);
    let info = PatchInfo {
        format: PatchFormat::Bps,
        records,
        source_size: Some(source_size),
        source_crc32: Some(footer.source_crc),
        target_size,
        target_crc32: Some(footer.target_crc),
        metadata: (!metadata.is_empty()).then(|| String::from_utf8_lossy(metadata).trim().to_string()),
        truncate: None,
    };
    let mut a = applied(info, out);
    if short {
        a.warnings
            .push("The BPS patch stops before the end of the file it makes: the rest is zeros.".into());
    }
    Ok(a)
}

fn signed(v: u64) -> i64 {
    let n = (v >> 1) as i64;
    if v & 1 != 0 { -n } else { n }
}

fn check_size(size: u64) -> Result<()> {
    if size > MAX_OUTPUT {
        bail!(
            "the patch makes a file of {size} bytes, more than the {} MiB binviz handles",
            MAX_OUTPUT >> 20
        );
    }
    Ok(())
}

/// The runs of bytes that differ between two files, at the same offsets.
pub fn changes(a: &[u8], b: &[u8]) -> Vec<Change> {
    let common = a.len().min(b.len());
    let mut out = Vec::new();
    let mut i = 0;
    let mut run: Option<(usize, usize, u64)> = None; // start, end, differing bytes
    while i < common {
        // Skip equal stretches a block at a time.
        if i % 64 == 0 && i + 64 <= common && a[i..i + 64] == b[i..i + 64] {
            i += 64;
            continue;
        }
        if a[i] != b[i] {
            run = match run {
                Some((start, end, n)) if i - end < GAP => Some((start, i + 1, n + 1)),
                Some((start, end, n)) => {
                    out.push(changed(start, end, n));
                    Some((i, i + 1, 1))
                }
                None => Some((i, i + 1, 1)),
            };
        }
        i += 1;
    }
    if let Some((start, end, n)) = run {
        out.push(changed(start, end, n));
    }
    if b.len() > common {
        out.push(Change {
            kind: ChangeKind::Added,
            offset: common as u64,
            len: (b.len() - common) as u64,
            differ: (b.len() - common) as u64,
        });
    } else if a.len() > common {
        out.push(Change {
            kind: ChangeKind::Removed,
            offset: common as u64,
            len: (a.len() - common) as u64,
            differ: (a.len() - common) as u64,
        });
    }
    out
}

fn changed(start: usize, end: usize, differ: u64) -> Change {
    Change {
        kind: ChangeKind::Changed,
        offset: start as u64,
        len: (end - start) as u64,
        differ,
    }
}

/// A patch that turns `source` into `target`.
pub fn create(format: PatchFormat, source: &[u8], target: &[u8]) -> Result<Vec<u8>> {
    match format {
        PatchFormat::Ips => create_ips(source, target),
        PatchFormat::Ups => Ok(create_ups(source, target)),
        PatchFormat::Bps => Ok(create_bps(source, target)),
    }
}

fn create_ips(source: &[u8], target: &[u8]) -> Result<Vec<u8>> {
    const LIMIT: usize = 1 << 24;
    let mut out = b"PATCH".to_vec();
    // The runs to write: those that differ, and all of what the target adds.
    let common = source.len().min(target.len());
    let mut runs: Vec<(usize, usize)> = changes(&source[..common], &target[..common])
        .iter()
        .map(|c| (c.offset as usize, (c.offset + c.len) as usize))
        .collect();
    if target.len() > common {
        runs.push((common, target.len()));
    }
    for (start, end) in runs {
        if end > LIMIT {
            bail!("IPS patches can't change bytes past 16 MiB: use BPS");
        }
        let mut at = start;
        while at < end {
            // A record at $454F46 would read as the end marker: start a byte early.
            if at == 0x454F46 {
                at -= 1;
            }
            let len = (end - at).min(0xFFFF);
            let chunk = &target[at..at + len];
            out.extend_from_slice(&(at as u32).to_be_bytes()[1..]);
            if len > 3 && chunk.iter().all(|&b| b == chunk[0]) {
                out.extend_from_slice(&[0, 0]);
                out.extend_from_slice(&(len as u16).to_be_bytes());
                out.push(chunk[0]);
            } else {
                out.extend_from_slice(&(len as u16).to_be_bytes());
                out.extend_from_slice(chunk);
            }
            at += len;
        }
    }
    out.extend_from_slice(b"EOF");
    if target.len() < source.len() {
        if target.len() >= LIMIT {
            bail!("IPS patches can't cut a file past 16 MiB: use BPS");
        }
        out.extend_from_slice(&(target.len() as u32).to_be_bytes()[1..]);
    }
    Ok(out)
}

fn create_ups(source: &[u8], target: &[u8]) -> Vec<u8> {
    let mut out = b"UPS1".to_vec();
    write_varint(&mut out, source.len() as u64);
    write_varint(&mut out, target.len() as u64);
    let len = source.len().max(target.len());
    let xor = |i: usize| source.get(i).copied().unwrap_or(0) ^ target.get(i).copied().unwrap_or(0);
    let (mut i, mut pos) = (0, 0);
    while i < len {
        if xor(i) == 0 {
            i += 1;
            continue;
        }
        write_varint(&mut out, (i - pos) as u64);
        while i < len && xor(i) != 0 {
            out.push(xor(i));
            i += 1;
        }
        out.push(0);
        i += 1;
        pos = i;
    }
    footer(out, source, target)
}

/// A BPS patch of the plainest kind: the original's bytes where they are
/// the same, new bytes where not.
fn create_bps(source: &[u8], target: &[u8]) -> Vec<u8> {
    let mut out = b"BPS1".to_vec();
    write_varint(&mut out, source.len() as u64);
    write_varint(&mut out, target.len() as u64);
    write_varint(&mut out, 0);
    let same = |i: usize| source.get(i) == target.get(i);
    let mut i = 0;
    while i < target.len() {
        let start = i;
        let keep = same(i);
        // Short equal stretches cost more as actions than as bytes.
        while i < target.len() && (same(i) == keep || (!keep && (i..(i + 4).min(target.len())).any(|j| !same(j)))) {
            i += 1;
        }
        let length = (i - start) as u64;
        if keep {
            write_varint(&mut out, (length - 1) << 2);
        } else {
            write_varint(&mut out, ((length - 1) << 2) | 1);
            out.extend_from_slice(&target[start..i]);
        }
    }
    footer(out, source, target)
}

fn footer(mut out: Vec<u8>, source: &[u8], target: &[u8]) -> Vec<u8> {
    out.extend_from_slice(&crc32(source).to_le_bytes());
    out.extend_from_slice(&crc32(target).to_le_bytes());
    let crc = crc32(&out);
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let x = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(0x80 | x);
            return;
        }
        out.push(x);
        v -= 1;
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(bytes);
    crc.sum()
}

fn be24(b: &[u8]) -> u32 {
    u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2])
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8], pos: usize) -> Reader<'a> {
        Reader { data, pos }
    }

    fn bytes(&mut self, n: u64) -> Result<&'a [u8]> {
        let n = usize::try_from(n).map_err(|_| Error::new("the patch is cut short"))?;
        if self.data.len() - self.pos < n {
            bail!("the patch is cut short");
        }
        self.pos += n;
        Ok(&self.data[self.pos - n..self.pos])
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn be16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn be24(&mut self) -> Result<u32> {
        Ok(be24(self.bytes(3)?))
    }

    /// A number as UPS and BPS write them: seven bits a byte, the last
    /// flagged, each byte after the first also adding one of its unit.
    fn varint(&mut self) -> Result<u64> {
        let (mut v, mut unit) = (0u64, 1u64);
        loop {
            let x = self.u8()?;
            v = u64::from(x & 0x7F)
                .checked_mul(unit)
                .and_then(|d| v.checked_add(d))
                .ok_or_else(|| Error::new("a number in the patch is too large"))?;
            if x & 0x80 != 0 {
                return Ok(v);
            }
            unit = unit
                .checked_mul(128)
                .ok_or_else(|| Error::new("a number in the patch is too large"))?;
            v = v
                .checked_add(unit)
                .ok_or_else(|| Error::new("a number in the patch is too large"))?;
        }
    }
}

/// Where a change is: its bank or section, its address, the function or
/// region it is in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Placed {
    #[serde(flatten)]
    pub change: Change,
    pub section: Option<String>,
    pub address: Option<u64>,
    /// The function the run starts in, `name+offset`.
    pub function: Option<String>,
    /// The innermost regions the run starts in (header fields, tables...), outermost first.
    pub region: Vec<String>,
}

/// Places changes in `bin` (the original or the patched file).
pub fn place(bin: &Binary, changes: &[Change]) -> Vec<Placed> {
    changes
        .iter()
        .map(|&change| {
            let offset = change.offset;
            let section = bin.section_at_offset(offset).map(|s| s.name.clone());
            let address = bin.offset_to_address(offset);
            let function = address.and_then(|a| {
                let f = bin.symbols().function_containing(a)?;
                let name = f.demangled().map_or_else(|| f.name().to_string(), |d| d.into_owned());
                Some(if a == f.address {
                    name
                } else {
                    format!("{name}+{:#x}", a - f.address)
                })
            });
            let path = bin.describe_offset(offset);
            let region = path
                .iter()
                .skip(1)
                .rev()
                .take(2)
                .rev()
                .map(|p| p.name.clone())
                .collect();
            Placed {
                change,
                section,
                address,
                function,
                region,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Vec<u8> {
        (0..4096u32).map(|i| (i * 7 + i / 256) as u8).collect()
    }

    fn edited() -> Vec<u8> {
        let mut t = game();
        t[0x10] = 0xEA;
        t[0x11] = 0xEA;
        t[0x100..0x140].fill(0x55);
        t[0x800..0x808].copy_from_slice(b"BINVIZ!!");
        t
    }

    #[test]
    fn varints_round_trip() {
        for v in [0u64, 1, 127, 128, 129, 16511, 16512, 1 << 20, u32::MAX as u64, 1 << 40] {
            let mut out = Vec::new();
            write_varint(&mut out, v);
            assert_eq!(Reader::new(&out, 0).varint().unwrap(), v, "{v}");
        }
        // The spec's own example: 128 is written 00 80.
        let mut out = Vec::new();
        write_varint(&mut out, 128);
        assert_eq!(out, [0x00, 0x80]);
    }

    #[test]
    fn each_format_round_trips() {
        let (a, b) = (game(), edited());
        for format in [PatchFormat::Ips, PatchFormat::Ups, PatchFormat::Bps] {
            let patch = create(format, &a, &b).unwrap();
            assert_eq!(PatchFormat::detect(&patch), Some(format));
            let applied = apply(&patch, &a).unwrap();
            assert_eq!(applied.output, b, "{format:?}");
            assert!(applied.warnings.is_empty(), "{format:?}: {:?}", applied.warnings);
            assert_eq!(
                applied.changes.iter().map(|c| (c.offset, c.len)).collect::<Vec<_>>(),
                [(0x10, 2), (0x100, 0x40), (0x800, 8)]
            );
            if format != PatchFormat::Ips {
                assert_eq!(applied.source_matches, Some(true));
                assert_eq!(applied.target_matches, Some(true));
            }
        }
    }

    #[test]
    fn sizes_change() {
        let a = game();
        let mut longer = a.clone();
        longer.extend_from_slice(&[0; 300]);
        longer.extend_from_slice(b"end");
        let shorter = a[..3000].to_vec();
        for format in [PatchFormat::Ips, PatchFormat::Ups, PatchFormat::Bps] {
            for b in [&longer, &shorter] {
                let patch = create(format, &a, b).unwrap();
                let applied = apply(&patch, &a).unwrap();
                assert_eq!(&applied.output, b, "{format:?} {}", b.len());
                let last = applied.changes.last().unwrap();
                assert_eq!(last.offset, a.len().min(b.len()) as u64);
            }
        }
    }

    #[test]
    fn ips_records_and_markers() {
        // A plain record, a run of one value, and a cut to 0x30 bytes.
        let mut patch = b"PATCH".to_vec();
        patch.extend_from_slice(&[0, 0, 2, 0, 3, 1, 2, 3]);
        patch.extend_from_slice(&[0, 0, 0x20, 0, 0, 0, 4, 0xAA]);
        patch.extend_from_slice(b"EOF");
        patch.extend_from_slice(&[0, 0, 0x30]);
        let a = applied_to(&patch, &[0; 0x40]);
        assert_eq!(a.output.len(), 0x30);
        assert_eq!(&a.output[..6], &[0, 0, 1, 2, 3, 0]);
        assert_eq!(&a.output[0x20..0x25], &[0xAA, 0xAA, 0xAA, 0xAA, 0]);
        assert_eq!(a.info.records, 2);
        assert_eq!(a.info.truncate, Some(0x30));
        assert_eq!(a.changes.last().unwrap().kind, ChangeKind::Removed);
    }

    #[test]
    fn ips_avoids_the_end_marker() {
        let a = vec![0u8; 0x454F50];
        let mut b = a.clone();
        b[0x454F46] = 1;
        let patch = create(PatchFormat::Ips, &a, &b).unwrap();
        assert!(!patch[5..].starts_with(b"EOF"));
        assert_eq!(apply(&patch, &a).unwrap().output, b);
    }

    #[test]
    fn ups_works_backwards() {
        let (a, b) = (game(), edited());
        let patch = create(PatchFormat::Ups, &a, &b).unwrap();
        let back = apply(&patch, &b).unwrap();
        assert!(back.reversed);
        assert_eq!(back.output, a);
        assert_eq!(back.source_matches, Some(true));
    }

    #[test]
    fn another_version_is_flagged() {
        let (a, b) = (game(), edited());
        let patch = create(PatchFormat::Bps, &a, &b).unwrap();
        let mut other = a.clone();
        other[5] ^= 1;
        let applied = apply(&patch, &other).unwrap();
        assert_eq!(applied.source_matches, Some(false));
        assert!(applied.warnings[0].contains("another file"));
        // A damaged patch says so.
        let mut damaged = patch.clone();
        damaged[8] ^= 1;
        let applied = apply(&damaged, &a);
        assert!(applied.map_or(true, |a| a.warnings.iter().any(|w| w.contains("damaged"))));
    }

    #[test]
    fn copier_headers_are_skipped() {
        let (a, b) = (game(), edited());
        let patch = create(PatchFormat::Bps, &a, &b).unwrap();
        let mut headered = vec![0u8; 512];
        headered.extend_from_slice(&a);
        // 512 + 4096 isn't 512 more than a multiple of 1024: pad the game to one.
        let applied = apply(&patch, &headered).unwrap();
        assert_eq!(applied.skipped_header, 512);
        assert_eq!(&applied.output[512..], &b[..]);
        assert_eq!(applied.changes[0].offset, 512 + 0x10);
    }

    #[test]
    fn bps_copies() {
        // SourceCopy and TargetCopy, written by hand: the target is the
        // source's second half, then "ab" repeated from itself.
        let source = b"0123456789".to_vec();
        let mut p = b"BPS1".to_vec();
        write_varint(&mut p, 10);
        write_varint(&mut p, 11);
        write_varint(&mut p, 0);
        write_varint(&mut p, ((5 - 1) << 2) | 2); // SourceCopy 5 from +5
        write_varint(&mut p, 5 << 1);
        write_varint(&mut p, ((2 - 1) << 2) | 1); // TargetRead "ab"
        p.extend_from_slice(b"ab");
        write_varint(&mut p, ((4 - 1) << 2) | 3); // TargetCopy 4 from +5
        write_varint(&mut p, 5 << 1);
        let target = b"56789ababab".to_vec();
        let p = footer(p, &source, &target);
        let a = apply(&p, &source).unwrap();
        assert_eq!(a.output, target);
        assert_eq!(a.info.records, 3);
        assert_eq!(a.target_matches, Some(true));
    }

    fn applied_to(patch: &[u8], source: &[u8]) -> Applied {
        apply(patch, source).unwrap()
    }
}
