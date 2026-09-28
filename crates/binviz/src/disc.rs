//! CD images with an ISO 9660 file system, as PlayStation games come: raw
//! (`.bin`, 2352-byte sectors, the data 24 bytes in for Mode 2 and 16 for
//! Mode 1) or cooked (`.iso`, 2048-byte sectors). The files on the disc,
//! and the executable the console boots, which `SYSTEM.CNF` names
//! (`BOOT = cdrom:\SLUS_012.34;1`).
//!
//! Reading is done a sector at a time, so that a caller holding the image
//! elsewhere (a browser's file) reads only what it needs.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result, bail};

/// How a disc image lays its sectors out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    /// Bytes per sector in the image: 2048 (cooked) or 2352 (raw).
    pub sector: u64,
    /// Where a sector's 2048 bytes of data start in it.
    pub data: u64,
}

impl Layout {
    /// Where logical sector `lba`'s data is in the image.
    pub fn offset(self, lba: u64) -> u64 {
        lba * self.sector + self.data
    }
}

/// A file (or folder) on the disc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscFile {
    /// Its path, folders separated by `/`, without the `;1` version.
    pub path: String,
    /// The logical sector it starts at.
    pub lba: u64,
    pub size: u64,
    pub dir: bool,
}

/// What is on a disc.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disc {
    pub layout: Layout,
    /// The volume's name.
    pub volume: String,
    /// Files, the executable it boots first.
    pub files: Vec<DiscFile>,
    /// The path of the executable it boots, as `SYSTEM.CNF` says.
    pub boot: Option<String>,
}

/// Bytes of the image a caller needs before [`layout`] can say: up to past sector 16.
pub const PREFIX: usize = 17 * 2352;

/// How the image lays its sectors out, from its first [`PREFIX`] bytes, if it is a disc.
pub fn layout(prefix: &[u8]) -> Option<Layout> {
    let pvd = |l: Layout| {
        let at = l.offset(16) as usize;
        prefix
            .get(at..at + 6)
            .is_some_and(|b| b[0] == 1 && &b[1..6] == b"CD001")
    };
    [
        Layout { sector: 2048, data: 0 },
        Layout { sector: 2352, data: 24 },
        Layout { sector: 2352, data: 16 },
    ]
    .into_iter()
    .find(|&l| pvd(l))
}

/// Walks the disc's file system, reading `count` sectors from `lba` with `read`.
pub fn list(layout: Layout, read: &mut dyn FnMut(u64, u64) -> Result<Vec<u8>>) -> Result<Disc> {
    let pvd = read(16, 1)?;
    let (volume, root) = root(&pvd).ok_or_else(|| Error::new("no ISO 9660 volume on the disc"))?;
    let mut files = Vec::new();
    let mut folders = vec![root];
    let mut seen = std::collections::HashSet::new();
    while let Some(dir) = folders.pop() {
        if !seen.insert(dir.lba) || files.len() > 20_000 {
            continue;
        }
        let sectors = dir.size.div_ceil(2048).clamp(1, 64);
        let bytes = read(dir.lba, sectors)?;
        for f in records(&bytes, &dir.path) {
            if f.dir {
                folders.push(f.clone());
            }
            files.push(f);
        }
    }
    // What SYSTEM.CNF boots, first.
    let cnf = files
        .iter()
        .find(|f| !f.dir && f.path.eq_ignore_ascii_case("SYSTEM.CNF"))
        .cloned();
    let boot = match cnf {
        Some(c) if c.size < 4096 => read(c.lba, 2)
            .ok()
            .and_then(|b| boot_path(&b[..(c.size as usize).min(b.len())])),
        _ => None,
    };
    sort(&mut files, boot.as_deref());
    Ok(Disc {
        layout,
        volume,
        files,
        boot,
    })
}

/// The volume's name and root folder, from its primary volume descriptor (logical sector 16).
pub fn root(pvd: &[u8]) -> Option<(String, DiscFile)> {
    if pvd.len() < 190 || &pvd[1..6] != b"CD001" {
        return None;
    }
    let volume = String::from_utf8_lossy(&pvd[40..72])
        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .to_string();
    Some((volume, record(&pvd[156..190], "", true)?))
}

/// Files first: what the disc boots, then executables, other files, and folders last.
pub fn sort(files: &mut [DiscFile], boot: Option<&str>) {
    files.sort_by(|a, b| {
        let rank = |f: &DiscFile| {
            if boot.is_some_and(|p| p.eq_ignore_ascii_case(&f.path)) {
                0
            } else if f.dir {
                3
            } else if looks_executable(&f.path) {
                1
            } else {
                2
            }
        };
        rank(a).cmp(&rank(b)).then_with(|| a.path.cmp(&b.path))
    });
}

/// Names that executables go by on PlayStation discs (SLUS_012.34, PSX.EXE, MAIN.EXE…).
fn looks_executable(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_uppercase();
    name.ends_with(".EXE")
        || name.ends_with(".ELF")
        || [
            "SLUS", "SCUS", "SLES", "SCES", "SLPS", "SCPS", "SLPM", "SIPS", "SLKA", "PAPX",
        ]
        .iter()
        .any(|p| name.starts_with(p))
}

/// The files and folders in a folder's sectors (its path `parent`).
pub fn records(bytes: &[u8], parent: &str) -> Vec<DiscFile> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let len = bytes[at] as usize;
        if len == 0 {
            // Records don't cross sectors: the rest of this one is padding.
            at = (at / 2048 + 1) * 2048;
            continue;
        }
        if let Some(f) = bytes.get(at..at + len).and_then(|r| record(r, parent, false)) {
            out.push(f);
        }
        at += len;
    }
    out
}

/// One directory record (`root`: the volume's root folder's); None for `.` and `..`.
fn record(r: &[u8], parent: &str, root: bool) -> Option<DiscFile> {
    if r.len() < 34 {
        return None;
    }
    let lba = u32::from_le_bytes(r[2..6].try_into().unwrap()) as u64;
    let size = u32::from_le_bytes(r[10..14].try_into().unwrap()) as u64;
    let dir = r[25] & 2 != 0;
    let name_len = r[32] as usize;
    let raw = r.get(33..33 + name_len)?;
    if !root && (raw == [0] || raw == [1]) {
        return None;
    }
    let name = String::from_utf8_lossy(raw);
    let name = name.split(';').next().unwrap_or("").trim_end_matches('.');
    let path = if root {
        String::new()
    } else if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    };
    Some(DiscFile { path, lba, size, dir })
}

/// The executable `SYSTEM.CNF` boots: `BOOT = cdrom:\SLUS_012.34;1`.
pub fn boot_path(cnf: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(cnf);
    let line = text
        .lines()
        .find(|l| l.trim_start().to_ascii_uppercase().starts_with("BOOT"))?;
    let value = line.split_once('=')?.1.trim();
    let path = value.split_once(':').map_or(value, |(_, p)| p);
    let path = path.trim_start_matches(['\\', '/']).split(';').next()?.trim();
    Some(path.replace('\\', "/"))
}

/// A file's bytes from the image, sector by sector.
pub fn extract(image: &[u8], layout: Layout, file: &DiscFile) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(file.size as usize);
    let mut lba = file.lba;
    while (out.len() as u64) < file.size {
        let at = layout.offset(lba) as usize;
        let take = (file.size - out.len() as u64).min(2048) as usize;
        let chunk = image
            .get(at..at + take)
            .ok_or_else(|| Error::new(format!("{} runs past the end of the image", file.path)))?;
        out.extend_from_slice(chunk);
        lba += 1;
    }
    Ok(out)
}

/// Reads sectors from a whole image in memory.
pub fn sectors(image: &[u8], layout: Layout) -> impl FnMut(u64, u64) -> Result<Vec<u8>> + '_ {
    move |lba, count| {
        let mut out = Vec::with_capacity(count as usize * 2048);
        for n in lba..lba + count {
            let at = layout.offset(n) as usize;
            match image.get(at..at + 2048) {
                Some(s) => out.extend_from_slice(s),
                None if n > lba => break,
                None => bail!("sector {n} is past the end of the image"),
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small disc: SYSTEM.CNF booting SLUS_000.01 in the root, and DATA/LEVEL1.BIN.
    pub(crate) fn image(raw: bool) -> Vec<u8> {
        let sector = if raw { 2352 } else { 2048 };
        let data = if raw { 24 } else { 0 };
        let mut img = vec![0u8; sector * 40];
        let mut put = |lba: usize, bytes: &[u8]| {
            let at = lba * sector + data;
            img[at..at + bytes.len()].copy_from_slice(bytes);
        };
        let rec = |lba: u32, size: u32, dir: bool, name: &[u8]| {
            let mut r = vec![0u8; 33 + name.len() + (name.len() + 1) % 2];
            r[0] = r.len() as u8;
            r[2..6].copy_from_slice(&lba.to_le_bytes());
            r[6..10].copy_from_slice(&lba.to_be_bytes());
            r[10..14].copy_from_slice(&size.to_le_bytes());
            r[14..18].copy_from_slice(&size.to_be_bytes());
            r[25] = if dir { 2 } else { 0 };
            r[32] = name.len() as u8;
            r[33..33 + name.len()].copy_from_slice(name);
            r
        };
        let mut pvd = vec![0u8; 2048];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[40..48].copy_from_slice(b"TESTDISC");
        let root = rec(20, 2048, true, &[0]);
        pvd[156..156 + root.len()].copy_from_slice(&root);
        put(16, &pvd);
        let mut dir = Vec::new();
        dir.extend(rec(20, 2048, true, &[0]));
        dir.extend(rec(20, 2048, true, &[1]));
        dir.extend(rec(22, 40, false, b"SYSTEM.CNF;1"));
        dir.extend(rec(23, 3000, false, b"SLUS_000.01;1"));
        dir.extend(rec(21, 2048, true, b"DATA"));
        put(20, &dir);
        let mut sub = Vec::new();
        sub.extend(rec(21, 2048, true, &[0]));
        sub.extend(rec(20, 2048, true, &[1]));
        sub.extend(rec(26, 10, false, b"LEVEL1.BIN;1"));
        put(21, &sub);
        put(22, b"BOOT = cdrom:\\SLUS_000.01;1\r\nTCB = 4\r\n");
        // The executable spans two sectors: its second starts with "2nd".
        let mut exe = b"PS-X EXE".to_vec();
        exe.resize(2048, 0x11);
        put(23, &exe);
        put(24, b"2nd");
        put(26, b"level data");
        img
    }

    #[test]
    fn cooked_and_raw_discs() {
        for raw in [false, true] {
            let img = image(raw);
            let l = layout(&img[..PREFIX.min(img.len())]).unwrap();
            assert_eq!(
                l,
                if raw {
                    Layout { sector: 2352, data: 24 }
                } else {
                    Layout { sector: 2048, data: 0 }
                }
            );
            let disc = list(l, &mut sectors(&img, l)).unwrap();
            assert_eq!(disc.volume, "TESTDISC");
            assert_eq!(disc.boot.as_deref(), Some("SLUS_000.01"));
            let paths: Vec<&str> = disc.files.iter().map(|f| f.path.as_str()).collect();
            assert_eq!(paths, ["SLUS_000.01", "DATA/LEVEL1.BIN", "SYSTEM.CNF", "DATA"]);
            let exe = extract(&img, l, &disc.files[0]).unwrap();
            assert_eq!(
                (exe.len(), &exe[..8], &exe[2048..2051]),
                (3000, &b"PS-X EXE"[..], &b"2nd"[..])
            );
        }
        assert!(layout(&[0u8; PREFIX]).is_none());
    }
}
