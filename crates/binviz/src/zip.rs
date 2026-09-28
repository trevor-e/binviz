//! Reading zip archives (an `.ipa`, a zipped build, a zip of dSYMs): the
//! central directory, and entries' bytes on demand.
//!
//! The pieces are separate so a caller that reads the archive itself (the
//! browser, from a `Blob`) can use them: find the directory from the last
//! bytes of the file, parse it, then read an entry's local header to know
//! where its data starts. [`ZipFile`] puts them together for a file on disk
//! or an archive in memory.

use std::io::{Read, Seek, SeekFrom};

use serde::Serialize;

/// One file in an archive.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipEntry {
    pub name: String,
    /// 0 = stored, 8 = deflated, 93 = Zstandard.
    pub method: u16,
    pub compressed_size: u64,
    pub size: u64,
    pub crc32: u32,
    /// Where the entry's local header is in the archive.
    pub header_offset: u64,
    pub is_dir: bool,
    pub is_symlink: bool,
}

/// Where the central directory is.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    pub offset: u64,
    pub size: u64,
    pub entries: u64,
    /// Zip64: the directory's real location is in a record at this offset,
    /// which has to be read (56 bytes) and passed to [`zip64_directory`].
    pub zip64_record: Option<u64>,
}

/// How many bytes at the end of an archive hold its end-of-directory
/// records, at most (a 64 KiB comment, the record, the zip64 locator).
pub const TAIL: u64 = 65_535 + 22 + 20;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// Whether bytes start like a zip archive.
pub fn is_zip(prefix: &[u8]) -> bool {
    prefix.starts_with(b"PK\x03\x04") || prefix.starts_with(b"PK\x05\x06")
}

/// Finds the central directory from an archive's last bytes (up to [`TAIL`]
/// of them, ending at the end of the file).
pub fn find_directory(tail: &[u8]) -> Result<Directory, String> {
    if tail.len() < 22 {
        return Err("not a zip archive: too short".into());
    }
    // The end record is the last one whose comment runs exactly to the end.
    let at = (0..=tail.len() - 22)
        .rev()
        .find(|&i| {
            u32_at(tail, i) == Some(0x0605_4b50)
                && u16_at(tail, i + 20).is_some_and(|c| i + 22 + c as usize == tail.len())
        })
        .ok_or("not a zip archive: no end of central directory record")?;
    let entries = u16_at(tail, at + 10).unwrap_or(0) as u64;
    let size = u32_at(tail, at + 12).unwrap_or(0) as u64;
    let offset = u32_at(tail, at + 16).unwrap_or(0) as u64;
    let zip64 = entries == 0xFFFF || size == 0xFFFF_FFFF || offset == 0xFFFF_FFFF;
    let locator = at.checked_sub(20).filter(|&l| u32_at(tail, l) == Some(0x0706_4b50));
    if let Some(l) = locator {
        let record = u64_at(tail, l + 8).ok_or("truncated zip64 locator")?;
        return Ok(Directory {
            offset,
            size,
            entries,
            zip64_record: Some(record),
        });
    }
    if zip64 {
        return Err("zip64 archive without a zip64 locator".into());
    }
    Ok(Directory {
        offset,
        size,
        entries,
        zip64_record: None,
    })
}

/// The directory's location from a zip64 end record (at least 56 bytes).
pub fn zip64_directory(record: &[u8]) -> Result<Directory, String> {
    if u32_at(record, 0) != Some(0x0606_4b50) {
        return Err("bad zip64 end of central directory record".into());
    }
    Ok(Directory {
        entries: u64_at(record, 32).ok_or("truncated zip64 record")?,
        size: u64_at(record, 40).ok_or("truncated zip64 record")?,
        offset: u64_at(record, 48).ok_or("truncated zip64 record")?,
        zip64_record: None,
    })
}

/// The entries of a central directory.
pub fn parse_directory(cd: &[u8]) -> Result<Vec<ZipEntry>, String> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 46 <= cd.len() && u32_at(cd, at) == Some(0x0201_4b50) {
        let made_by = u16_at(cd, at + 4).unwrap_or(0);
        let flags = u16_at(cd, at + 8).unwrap_or(0);
        let method = u16_at(cd, at + 10).unwrap_or(0);
        let crc32 = u32_at(cd, at + 16).unwrap_or(0);
        let mut compressed_size = u32_at(cd, at + 20).unwrap_or(0) as u64;
        let mut size = u32_at(cd, at + 24).unwrap_or(0) as u64;
        let name_len = u16_at(cd, at + 28).unwrap_or(0) as usize;
        let extra_len = u16_at(cd, at + 30).unwrap_or(0) as usize;
        let comment_len = u16_at(cd, at + 32).unwrap_or(0) as usize;
        let external = u32_at(cd, at + 38).unwrap_or(0);
        let mut header_offset = u32_at(cd, at + 42).unwrap_or(0) as u64;
        let name_bytes = cd
            .get(at + 46..at + 46 + name_len)
            .ok_or("truncated central directory")?;
        let name = if flags & 0x800 != 0 {
            String::from_utf8_lossy(name_bytes).into_owned()
        } else {
            // Most archivers write UTF-8 even without the flag.
            match std::str::from_utf8(name_bytes) {
                Ok(s) => s.to_string(),
                Err(_) => name_bytes.iter().map(|&b| b as char).collect(),
            }
        };
        // Zip64: the extra field holds the real values of the fields that are all ones.
        let extra = cd
            .get(at + 46 + name_len..at + 46 + name_len + extra_len)
            .unwrap_or(&[]);
        let mut e = 0;
        while e + 4 <= extra.len() {
            let id = u16_at(extra, e).unwrap_or(0);
            let len = u16_at(extra, e + 2).unwrap_or(0) as usize;
            if id == 0x0001 {
                let mut p = e + 4;
                if size == 0xFFFF_FFFF {
                    size = u64_at(extra, p).unwrap_or(size);
                    p += 8;
                }
                if compressed_size == 0xFFFF_FFFF {
                    compressed_size = u64_at(extra, p).unwrap_or(compressed_size);
                    p += 8;
                }
                if header_offset == 0xFFFF_FFFF {
                    header_offset = u64_at(extra, p).unwrap_or(header_offset);
                }
            }
            e += 4 + len;
        }
        // Unix permissions live in the top half of the external attributes.
        let unix = made_by >> 8 == 3;
        let mode = external >> 16;
        out.push(ZipEntry {
            is_dir: name.ends_with('/'),
            is_symlink: unix && mode & 0o170000 == 0o120000,
            name,
            method,
            compressed_size,
            size,
            crc32,
            header_offset,
        });
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

/// Where an entry's data starts, from its local header (at least 30 bytes).
pub fn data_offset(entry: &ZipEntry, local_header: &[u8]) -> Result<u64, String> {
    if u32_at(local_header, 0) != Some(0x0403_4b50) {
        return Err(format!("{}: bad local header", entry.name));
    }
    let name = u16_at(local_header, 26).unwrap_or(0) as u64;
    let extra = u16_at(local_header, 28).unwrap_or(0) as u64;
    Ok(entry.header_offset + 30 + name + extra)
}

/// Inflates up to `max` bytes from a Zstandard-compressed zip entry.
pub fn decompress_zstandard(data: &[u8], max: u64) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    ruzstd::decoding::StreamingDecoder::new(data)
        .map_err(|e| e.to_string())?
        .take(max)
        .read_to_end(&mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

/// A zip archive, read on demand: a file on disk, or one in memory (a zip found inside another).
pub struct ZipFile {
    file: Box<dyn ReadSeek>,
    pub entries: Vec<ZipEntry>,
    pub size: u64,
}

impl ZipFile {
    pub fn open(path: &std::path::Path) -> Result<ZipFile, String> {
        let err = |e: std::io::Error| format!("{}: {e}", path.display());
        let file = std::fs::File::open(path).map_err(err)?;
        let size = file.metadata().map_err(err)?.len();
        ZipFile::from_reader(Box::new(file), size).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// An archive held in memory.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<ZipFile, String> {
        let size = bytes.len() as u64;
        ZipFile::from_reader(Box::new(std::io::Cursor::new(bytes)), size)
    }

    fn from_reader(mut file: Box<dyn ReadSeek>, size: u64) -> Result<ZipFile, String> {
        let err = |e: std::io::Error| e.to_string();
        let tail_offset = size.saturating_sub(TAIL);
        let tail = read_at(&mut *file, tail_offset, (size - tail_offset) as usize).map_err(err)?;
        let mut dir = find_directory(&tail)?;
        if let Some(record) = dir.zip64_record {
            dir = zip64_directory(&read_at(&mut *file, record, 56).map_err(err)?)?;
        }
        if dir.offset.checked_add(dir.size).is_none_or(|end| end > size) {
            return Err("the central directory runs past the end".into());
        }
        let cd = read_at(&mut *file, dir.offset, dir.size as usize).map_err(err)?;
        let entries = parse_directory(&cd)?;
        Ok(ZipFile { file, entries, size })
    }

    fn start(&mut self, entry: &ZipEntry) -> Result<u64, String> {
        let header = read_at(&mut *self.file, entry.header_offset, 30).map_err(|e| e.to_string())?;
        data_offset(entry, &header)
    }

    /// An entry's bytes, inflated.
    pub fn read(&mut self, entry: &ZipEntry) -> Result<Vec<u8>, String> {
        self.read_prefix(entry, u64::MAX)
    }

    /// The first `max` bytes of an entry (all of them if it is shorter).
    pub fn read_prefix(&mut self, entry: &ZipEntry, max: u64) -> Result<Vec<u8>, String> {
        let start = self.start(entry)?;
        let want = entry.size.min(max);
        self.file.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
        let raw = (&mut self.file).take(entry.compressed_size);
        let mut out = Vec::with_capacity(want as usize);
        let res = match entry.method {
            0 => raw.take(want).read_to_end(&mut out),
            8 => flate2::read::DeflateDecoder::new(raw).take(want).read_to_end(&mut out),
            93 => ruzstd::decoding::StreamingDecoder::new(raw)
                .map_err(|e| format!("{}: {e}", entry.name))?
                .take(want)
                .read_to_end(&mut out),
            m => return Err(format!("{}: unsupported compression method {m}", entry.name)),
        };
        res.map_err(|e| format!("{}: {e}", entry.name))?;
        Ok(out)
    }
}

fn read_at(file: &mut dyn ReadSeek, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0; len];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive_entry(name: &str, method: u16, data: &[u8], compressed: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(compressed);

        let cd_offset = out.len() as u32;
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        let cd_size = out.len() as u32 - cd_offset;

        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// A stored-only archive built by hand: two files and a directory.
    fn archive() -> Vec<u8> {
        let files: [(&str, &[u8]); 3] = [
            ("A.app/", b""),
            ("A.app/A", b"\xcf\xfa\xed\xfe"),
            ("A.app/Info.plist", b"<plist/>"),
        ];
        let mut out = Vec::new();
        let mut cd = Vec::new();
        for (name, data) in files {
            let offset = out.len() as u32;
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&0u32.to_le_bytes()); // crc
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            cd.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            cd.extend_from_slice(&[20, 3, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            cd.extend_from_slice(&0u32.to_le_bytes());
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
            cd.extend_from_slice(&0u32.to_le_bytes());
            cd.extend_from_slice(&offset.to_le_bytes());
            cd.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&cd);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0, 3, 0, 3, 0]);
        out.extend_from_slice(&(cd.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn directory_and_entries() {
        let zip = archive();
        let dir = find_directory(&zip).unwrap();
        assert_eq!(dir.entries, 3);
        let entries = parse_directory(&zip[dir.offset as usize..(dir.offset + dir.size) as usize]).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["A.app/", "A.app/A", "A.app/Info.plist"]);
        assert!(entries[0].is_dir && !entries[1].is_dir);
        let e = &entries[2];
        let start = data_offset(e, &zip[e.header_offset as usize..]).unwrap() as usize;
        assert_eq!(&zip[start..start + e.size as usize], b"<plist/>");
    }

    #[test]
    fn reads_zstandard_entry() {
        let data = b"Zstandard-compressed ZIP entry";
        let compressed =
            ruzstd::encoding::compress_to_vec(data.as_slice(), ruzstd::encoding::CompressionLevel::Fastest);
        let zip = archive_entry("file.bin", 93, data, &compressed);
        let mut zip = ZipFile::from_bytes(zip).unwrap();
        let entry = zip.entries[0].clone();

        assert_eq!(zip.read_prefix(&entry, 9).unwrap(), b"Zstandard");
        assert_eq!(zip.read(&entry).unwrap(), data);
        assert_eq!(decompress_zstandard(&compressed, 9).unwrap(), b"Zstandard");
    }
}
