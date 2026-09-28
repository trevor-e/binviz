//! Text in old games. Few store it as ASCII: most have an encoding of their
//! own, the letters usually in order but starting anywhere (`A` = `$0A`, or
//! `$80`...). Relative search finds such text without knowing the encoding:
//! the differences between a known word's letters are the same in every
//! encoding that keeps the alphabet in order. A table file (`.tbl`, one
//! `hex=text` line per entry) then says which bytes stand for what, to read,
//! search and dump the text.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

/// Where a word turned up, read with the encoding it implies.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelativeHit {
    pub offset: u64,
    /// The bytes around the hit, read with that encoding: letters (and
    /// digits, when the word has some), `·` for anything else.
    pub preview: String,
    /// Where in `preview` the word starts.
    pub at: u32,
}

/// An encoding relative search found: where the alphabet starts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Encoding {
    /// The value standing for `A` (or `a`, for a lowercase word).
    pub first: u32,
    /// `A` or `a`.
    pub letter: char,
    pub hits: Vec<RelativeHit>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelativeSearch {
    pub word: String,
    /// Bytes per character (1, or 2 little-endian).
    pub width: u32,
    /// Hit counts decide the order: the most common encoding first.
    pub encodings: Vec<Encoding>,
    pub total: u32,
}

impl RelativeSearch {
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "{} for {:?} ({}-byte characters) in {} encodings\n",
            count(self.total as usize, "hit"),
            self.word,
            self.width,
            self.encodings.len()
        );
        for e in &self.encodings {
            out.push_str(&format!(
                "\n{} = ${:0w$X}: {}\n",
                e.letter,
                e.first,
                count(e.hits.len(), "hit"),
                w = self.width as usize * 2
            ));
            for h in e.hits.iter().take(20) {
                out.push_str(&format!("  {:#08x}  {}\n", h.offset, h.preview));
            }
            if e.hits.len() > 20 {
                out.push_str(&format!("  … {} more\n", e.hits.len() - 20));
            }
        }
        out
    }
}

fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// What a character is to relative search: its place in its alphabet, if any.
fn class(c: char) -> Option<(char, u32)> {
    match c {
        'A'..='Z' => Some(('A', c as u32 - 'A' as u32)),
        'a'..='z' => Some(('a', c as u32 - 'a' as u32)),
        '0'..='9' => Some(('0', c as u32 - '0' as u32)),
        _ => None,
    }
}

/// Finds `word` in `data` in any encoding that keeps its letters in order:
/// every place where consecutive values differ as the word's letters do.
/// `width` is 1, or 2 for 16-bit (little-endian) characters. Words need
/// three letters or more, of one case (spaces and punctuation don't count:
/// their codes are the encoding's own).
pub fn relative_search(data: &[u8], word: &str, width: u32, limit: usize) -> Result<RelativeSearch, String> {
    let letters: Vec<(char, u32)> = word.chars().filter_map(class).collect();
    if letters.len() < 3 || letters.len() != word.chars().count() {
        return Err("a word of three letters or more, without spaces or punctuation (their codes vary)".into());
    }
    let kind = letters[0].0;
    if letters.iter().any(|l| l.0 != kind) {
        return Err("one case at a time: upper and lower case letters may be far apart in the encoding".into());
    }
    let width = if width == 2 { 2usize } else { 1 };
    let value = |i: usize| -> Option<u32> {
        match width {
            1 => data.get(i).map(|&b| b as u32),
            _ => data.get(i..i + 2).map(|w| u16::from_le_bytes([w[0], w[1]]) as u32),
        }
    };
    let modulus: u64 = if width == 1 { 0x100 } else { 0x1_0000 };
    let deltas: Vec<u32> = letters.windows(2).map(|w| w[1].1.wrapping_sub(w[0].1)).collect();
    let span = letters.len() * width;
    let mut found: BTreeMap<u32, Vec<RelativeHit>> = BTreeMap::new();
    let mut total = 0u32;
    let mut i = 0usize;
    while i + span <= data.len() {
        let Some(v0) = value(i) else { break };
        let mut ok = true;
        let mut prev = v0;
        for (k, d) in deltas.iter().enumerate() {
            let Some(v) = value(i + (k + 1) * width) else {
                ok = false;
                break;
            };
            if (v as u64 + modulus - prev as u64) % modulus != *d as u64 % modulus {
                ok = false;
                break;
            }
            prev = v;
        }
        if ok {
            let first = ((v0 as u64 + modulus - letters[0].1 as u64) % modulus) as u32;
            let size = if kind == '0' { 10 } else { 26 };
            // A run of one value (all deltas zero) matches nothing useful.
            if deltas.iter().any(|&d| d != 0) {
                total += 1;
                let hits = found.entry(first).or_default();
                if hits.len() < limit {
                    // Around the hit: letters as that encoding reads them.
                    let before = 8 * width;
                    let start = i.saturating_sub(before) / width * width + i % width;
                    let start = start.min(i);
                    let mut preview = String::new();
                    let mut at = 0;
                    let mut j = start;
                    while j < (i + span + 16 * width).min(data.len()) {
                        if j == i {
                            at = preview.chars().count() as u32;
                        }
                        let v = value(j).unwrap_or(0);
                        let n = (v as u64 + modulus - first as u64) % modulus;
                        preview.push(if n < size {
                            char::from_u32(kind as u32 + n as u32).unwrap_or('·')
                        } else {
                            '·'
                        });
                        j += width;
                    }
                    hits.push(RelativeHit {
                        offset: i as u64,
                        preview,
                        at,
                    });
                }
            }
        }
        i += 1;
    }
    let mut encodings: Vec<Encoding> = found
        .into_iter()
        .map(|(first, hits)| Encoding {
            first,
            letter: kind,
            hits,
        })
        .collect();
    encodings.sort_by(|a, b| b.hits.len().cmp(&a.hits.len()).then(a.first.cmp(&b.first)));
    Ok(RelativeSearch {
        word: word.to_string(),
        width: width as u32,
        encodings,
        total,
    })
}

/// A table file: which bytes stand for which text.
#[derive(Debug, Clone, Default)]
pub struct Table {
    /// Byte sequences and their text.
    entries: HashMap<Vec<u8>, String>,
    /// Text and the bytes for it (the first entry given, for each).
    reverse: HashMap<String, Vec<u8>>,
    /// Entries that end a string (`/FF=<end>`).
    ends: std::collections::HashSet<Vec<u8>>,
    longest: usize,
}

/// A string a table reads in the data.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableText {
    pub offset: u64,
    /// Bytes read (the end marker included).
    pub len: u32,
    pub text: String,
}

fn parse_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.is_empty() || !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

impl Table {
    /// Reads a table file: `hex=text` lines (`41=A`, `8A20=the `), `/hex=text`
    /// for the entries that end a string, `*hex` for a line break; `#` and
    /// `;` start comments; other lines are skipped.
    pub fn parse(text: &str) -> Result<Table, String> {
        let mut t = Table::default();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.trim_end_matches(['\r', '\n']);
            if line.trim().is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let (end, line) = match line.strip_prefix('/') {
                Some(rest) => (true, rest),
                None => (false, line),
            };
            if let Some(rest) = line.strip_prefix('*') {
                if let Some(key) = parse_hex(rest.split('=').next().unwrap_or(rest)) {
                    t.add(key, "\n".into(), false);
                }
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                if end && let Some(key) = parse_hex(line) {
                    t.add(key, String::new(), true);
                }
                continue;
            };
            let Some(key) = parse_hex(key) else {
                return Err(format!("line {}: {key:?} is not hex", n + 1));
            };
            t.add(key, value.to_string(), end);
        }
        if t.entries.is_empty() {
            return Err("no entries (lines like 41=A)".into());
        }
        Ok(t)
    }

    fn add(&mut self, key: Vec<u8>, value: String, end: bool) {
        self.longest = self.longest.max(key.len());
        if end {
            self.ends.insert(key.clone());
        }
        if !value.is_empty() {
            self.reverse.entry(value.clone()).or_insert_with(|| key.clone());
        }
        self.entries.insert(key, value);
    }

    /// The table relative search implies: the 26 letters (or the ten digits)
    /// from `first`, in bytes of `width`.
    pub fn from_alphabet(first: u32, letter: char, width: u32) -> Table {
        let mut t = Table::default();
        let n = if letter == '0' { 10 } else { 26 };
        for i in 0..n {
            let v = first + i;
            let key = if width == 2 {
                (v as u16).to_le_bytes().to_vec()
            } else {
                vec![v as u8]
            };
            t.add(key, char::from_u32(letter as u32 + i).unwrap_or('?').to_string(), false);
        }
        t
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The text of the one-byte entry for `b`, if there is one.
    pub fn single(&self, b: u8) -> Option<&str> {
        self.entries.get([b].as_slice()).map(String::as_str)
    }

    /// Where `text` is in `data`, each place with the text read from there on.
    pub fn find_text(&self, data: &[u8], text: &str, limit: usize) -> Result<Vec<TableText>, String> {
        Ok(self
            .find(data, text, limit)?
            .into_iter()
            .map(|at| {
                let bytes = &data[at as usize..(at as usize + 96).min(data.len())];
                TableText {
                    offset: at,
                    ..self.decode(bytes, true)
                }
            })
            .collect())
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The table as a `.tbl` file.
    pub fn to_tbl(&self) -> String {
        let mut lines: Vec<(Vec<u8>, String)> = self.entries.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        lines.sort();
        lines
            .into_iter()
            .map(|(k, v)| {
                let hex: String = k.iter().map(|b| format!("{b:02X}")).collect();
                let end = if self.ends.contains(&k) { "/" } else { "" };
                if v == "\n" {
                    format!("*{hex}\n")
                } else {
                    format!("{end}{hex}={v}\n")
                }
            })
            .collect()
    }

    /// The entry at the start of `bytes`: the longest that matches.
    fn entry<'a>(&'a self, bytes: &[u8]) -> Option<(usize, &'a str)> {
        (1..=self.longest.min(bytes.len()))
            .rev()
            .find_map(|n| self.entries.get(&bytes[..n]).map(|v| (n, v.as_str())))
    }

    /// Reads `bytes` as text: bytes no entry covers are written `[XX]`.
    /// Stops after an end marker when `stop_at_end`.
    pub fn decode(&self, bytes: &[u8], stop_at_end: bool) -> TableText {
        let mut text = String::new();
        let mut i = 0;
        while i < bytes.len() {
            match self.entry(&bytes[i..]) {
                Some((n, v)) => {
                    let end = self.ends.contains(&bytes[i..i + n]);
                    text.push_str(v);
                    i += n;
                    if end && stop_at_end {
                        break;
                    }
                }
                None => {
                    text.push_str(&format!("[{:02X}]", bytes[i]));
                    i += 1;
                }
            }
        }
        TableText {
            offset: 0,
            len: i as u32,
            text,
        }
    }

    /// The bytes for `text`, entry by entry (longest first); `[XX]` stands for a byte.
    pub fn encode(&self, text: &str) -> Result<Vec<u8>, String> {
        let longest = self.reverse.keys().map(|k| k.chars().count()).max().unwrap_or(1);
        let chars: Vec<char> = text.chars().collect();
        let mut out = Vec::new();
        let mut i = 0;
        'outer: while i < chars.len() {
            if chars[i] == '['
                && let Some(close) = chars[i..].iter().position(|&c| c == ']')
                && let Some(b) = parse_hex(&chars[i + 1..i + close].iter().collect::<String>())
            {
                out.extend(b);
                i += close + 1;
                continue;
            }
            for n in (1..=longest.min(chars.len() - i)).rev() {
                let piece: String = chars[i..i + n].iter().collect();
                if let Some(key) = self.reverse.get(&piece) {
                    out.extend(key);
                    i += n;
                    continue 'outer;
                }
            }
            return Err(format!("the table has no entry for {:?}", chars[i]));
        }
        Ok(out)
    }

    /// Where `text` is in `data`, as this table encodes it.
    pub fn find(&self, data: &[u8], text: &str, limit: usize) -> Result<Vec<u64>, String> {
        let needle = self.encode(text)?;
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        Ok(data
            .windows(needle.len())
            .enumerate()
            .filter(|(_, w)| *w == needle.as_slice())
            .map(|(i, _)| i as u64)
            .take(limit)
            .collect())
    }

    /// The text in `data`: runs of `min` entries or more that the table reads
    /// without a gap (ended by an end marker, or by a byte it doesn't cover).
    pub fn strings(&self, data: &[u8], min: usize, limit: usize) -> Vec<TableText> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < data.len() && out.len() < limit {
            let start = i;
            let mut text = String::new();
            let mut chars = 0;
            let mut ended = false;
            while let Some((n, v)) = self.entry(&data[i..]) {
                let end = self.ends.contains(&data[i..i + n]);
                text.push_str(v);
                // Entries count, not their text: an end marker alone isn't text.
                if !end {
                    chars += 1;
                }
                i += n;
                if end {
                    ended = true;
                    break;
                }
            }
            if chars >= min {
                out.push(TableText {
                    offset: start as u64,
                    len: (i - start) as u32,
                    text,
                });
            }
            if i == start || (!ended && i < data.len()) {
                i = i.max(start + 1);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Text as a game might store it: A = $80, a space = $00, end = $FF.
    fn game_text(s: &str) -> Vec<u8> {
        s.chars()
            .map(|c| match c {
                'A'..='Z' => 0x80 + (c as u8 - b'A'),
                ' ' => 0x00,
                _ => 0xFE,
            })
            .chain([0xFF])
            .collect()
    }

    #[test]
    fn relative_search_finds_text_in_any_encoding() {
        let mut data = vec![0x13u8; 40];
        data.extend(game_text("THE HERO FINDS THE SWORD"));
        data.extend([0x42; 10]);
        let found = relative_search(&data, "SWORD", 1, 10).unwrap();
        assert_eq!(found.encodings.len(), 1);
        let e = &found.encodings[0];
        assert_eq!((e.first, e.letter), (0x80, 'A'));
        assert_eq!(e.hits[0].offset, 40 + 19);
        assert!(e.hits[0].preview.contains("THE·SWORD"), "{}", e.hits[0].preview);
        assert!(relative_search(&data, "SW", 1, 10).is_err());
        assert!(relative_search(&data, "Sword", 1, 10).is_err());
    }

    #[test]
    fn tables_read_search_and_dump() {
        let mut tbl = Table::from_alphabet(0x80, 'A', 1).to_tbl();
        tbl.push_str("00= \n/FF=<end>\n*FE\n");
        let t = Table::parse(&tbl).unwrap();
        let data = [vec![0x01, 0x02], game_text("HELLO THERE"), game_text("BYE")].concat();
        let text = t.decode(&data[2..], true);
        assert_eq!(text.text, "HELLO THERE<end>");
        assert_eq!(t.find(&data, "THERE", 10).unwrap(), vec![2 + 6]);
        let dumped = t.strings(&data, 3, 10);
        let texts: Vec<&str> = dumped.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, ["HELLO THERE<end>", "BYE<end>"]);
        assert_eq!(dumped[0].offset, 2);
        assert_eq!(t.encode("HI[FF]").unwrap(), vec![0x87, 0x88, 0xFF]);
        assert!(t.encode("hi").is_err());
        // Multi-byte entries win over their prefixes.
        let t = Table::parse("41=A\n4142=AB!\n").unwrap();
        assert_eq!(t.decode(b"ABA", false).text, "AB!A");
    }
}
