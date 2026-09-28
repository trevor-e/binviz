//! Printable strings in the loaded data sections, like `strings(1)`: ASCII
//! runs and UTF-16LE runs (common in Windows binaries and resources).

use serde::Serialize;

use crate::binary::Binary;
use crate::model::RegionKind;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundString {
    pub offset: u64,
    pub address: Option<u64>,
    /// Bytes the string occupies in the file (UTF-16 counts two per character).
    pub size: u32,
    /// UTF-16LE rather than ASCII.
    pub wide: bool,
    pub text: String,
    pub section: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StringPage {
    pub total: u32,
    pub offset: u32,
    pub strings: Vec<FoundString>,
}

const MIN_CHARS: usize = 4;
const MAX_STRINGS: usize = 500_000;
const MAX_TEXT: usize = 400;

fn printable(b: u8) -> bool {
    (0x20..0x7f).contains(&b) || b == b'\t'
}

impl Binary {
    /// All strings found (computed once).
    pub(crate) fn found_strings(&self) -> &[FoundString] {
        self.strings.get_or_init(|| self.scan_strings())
    }

    /// A page of strings containing `filter` (case-insensitive).
    pub fn strings(&self, filter: &str, offset: u32, limit: u32) -> StringPage {
        let needle = filter.to_lowercase();
        let all = self.found_strings();
        let matches: Vec<&FoundString> = all
            .iter()
            .filter(|s| needle.is_empty() || s.text.to_lowercase().contains(&needle))
            .collect();
        let limit = if limit == 0 { 200 } else { limit };
        StringPage {
            total: matches.len() as u32,
            offset,
            strings: matches
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .cloned()
                .collect(),
        }
    }

    fn scan_strings(&self) -> Vec<FoundString> {
        let mut out = Vec::new();
        for s in &self.sections {
            // Code sections produce mostly noise; tables and debug info are browsed elsewhere.
            let wanted = matches!(
                s.kind,
                RegionKind::Rodata | RegionKind::Data | RegionKind::Tls | RegionKind::Resources | RegionKind::Metadata
            ) || (s.kind == RegionKind::Notes && s.loaded);
            let Some(off) = s.file_offset else { continue };
            if !wanted || s.compressed || s.file_size == 0 {
                continue;
            }
            let end = (off + s.file_size).min(self.data.len() as u64);
            let bytes = &self.data[off as usize..end as usize];
            let address = |i: usize| s.loaded.then_some(s.address + i as u64);
            // ASCII runs.
            let mut i = 0;
            while i < bytes.len() {
                if !printable(bytes[i]) {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < bytes.len() && printable(bytes[i]) {
                    i += 1;
                }
                if i - start >= MIN_CHARS {
                    let text = String::from_utf8_lossy(&bytes[start..i.min(start + MAX_TEXT)]).into_owned();
                    out.push(FoundString {
                        offset: off + start as u64,
                        address: address(start),
                        size: (i - start) as u32,
                        wide: false,
                        text,
                        section: Some(s.index),
                    });
                }
            }
            // UTF-16LE runs: printable low byte, zero high byte.
            for parity in 0..2 {
                let mut i = parity;
                while i + 1 < bytes.len() {
                    if !(printable(bytes[i]) && bytes[i + 1] == 0) {
                        i += 2;
                        continue;
                    }
                    let start = i;
                    let mut text = String::new();
                    while i + 1 < bytes.len() && printable(bytes[i]) && bytes[i + 1] == 0 {
                        if text.len() < MAX_TEXT {
                            text.push(bytes[i] as char);
                        }
                        i += 2;
                    }
                    if (i - start) / 2 >= MIN_CHARS {
                        out.push(FoundString {
                            offset: off + start as u64,
                            address: address(start),
                            size: (i - start) as u32,
                            wide: true,
                            text,
                            section: Some(s.index),
                        });
                    }
                }
            }
            if out.len() >= MAX_STRINGS {
                out.truncate(MAX_STRINGS);
                break;
            }
        }
        out.sort_by_key(|s| s.offset);
        out
    }
}
