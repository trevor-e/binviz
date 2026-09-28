//! Property lists (`Info.plist`), in both of Apple's formats: binary
//! (`bplist00`, what Xcode ships in app bundles) and XML.

/// A property list value.
#[derive(Debug, Clone, PartialEq)]
pub enum Plist {
    Dict(Vec<(String, Plist)>),
    Array(Vec<Plist>),
    String(String),
    Integer(i64),
    Real(f64),
    Bool(bool),
    Data(Vec<u8>),
    /// Seconds since 2001-01-01.
    Date(f64),
    Uid(u64),
}

impl Plist {
    pub fn parse(bytes: &[u8]) -> Result<Plist, String> {
        if bytes.starts_with(b"bplist00") {
            Binary::new(bytes)?.top()
        } else {
            Xml::new(bytes).document()
        }
    }

    pub fn get(&self, key: &str) -> Option<&Plist> {
        match self {
            Plist::Dict(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Plist::String(s) => Some(s),
            _ => None,
        }
    }

    /// A string value by key.
    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    /// An array of strings by key (a lone string counts as one).
    pub fn strings(&self, key: &str) -> Vec<String> {
        match self.get(key) {
            Some(Plist::Array(items)) => items.iter().filter_map(|i| i.as_str().map(str::to_string)).collect(),
            Some(Plist::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        }
    }
}

// --- Binary ---------------------------------------------------------------------

struct Binary<'a> {
    bytes: &'a [u8],
    ref_size: usize,
    offsets: Vec<usize>,
    top: usize,
}

fn be(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0, |acc, &b| acc << 8 | b as u64)
}

impl<'a> Binary<'a> {
    fn new(bytes: &'a [u8]) -> Result<Binary<'a>, String> {
        if bytes.len() < 40 {
            return Err("binary plist too short".into());
        }
        let t = &bytes[bytes.len() - 32..];
        let offset_size = t[6] as usize;
        let ref_size = t[7] as usize;
        let count = be(&t[8..16]) as usize;
        let top = be(&t[16..24]) as usize;
        let table = be(&t[24..32]) as usize;
        if !(1..=8).contains(&offset_size) || !(1..=8).contains(&ref_size) || count > bytes.len() {
            return Err("bad binary plist trailer".into());
        }
        let end = table
            .checked_add(count * offset_size)
            .filter(|&e| e <= bytes.len())
            .ok_or("binary plist offset table out of range")?;
        let offsets = bytes[table..end]
            .chunks_exact(offset_size)
            .map(|c| be(c) as usize)
            .collect();
        Ok(Binary {
            bytes,
            ref_size,
            offsets,
            top,
        })
    }

    fn top(&self) -> Result<Plist, String> {
        self.object(self.top, 0)
    }

    /// The length of a collection or string, and where its contents start.
    fn length(&self, at: usize, info: u8) -> Result<(usize, usize), String> {
        if info != 0x0F {
            return Ok((info as usize, at + 1));
        }
        let marker = *self.bytes.get(at + 1).ok_or("truncated binary plist")?;
        if marker >> 4 != 0x1 {
            return Err("bad length in binary plist".into());
        }
        let n = 1usize << (marker & 0x0F);
        let v = self.bytes.get(at + 2..at + 2 + n).ok_or("truncated binary plist")?;
        Ok((be(v) as usize, at + 2 + n))
    }

    fn object(&self, index: usize, depth: u32) -> Result<Plist, String> {
        if depth > 64 {
            return Err("binary plist nested too deeply".into());
        }
        let at = *self.offsets.get(index).ok_or("binary plist object out of range")?;
        let marker = *self.bytes.get(at).ok_or("binary plist object out of range")?;
        let info = marker & 0x0F;
        let slice = |from: usize, len: usize| self.bytes.get(from..from + len).ok_or("truncated binary plist");
        Ok(match marker >> 4 {
            0x0 => match info {
                0x8 => Plist::Bool(false),
                0x9 => Plist::Bool(true),
                _ => Plist::String(String::new()),
            },
            0x1 => {
                // Eight-byte integers are signed; sixteen-byte ones keep their low half.
                Plist::Integer(be(slice(at + 1, 1usize << info)?) as i64)
            }
            0x2 => {
                let n = 1usize << info;
                let b = slice(at + 1, n)?;
                Plist::Real(match n {
                    4 => f32::from_be_bytes(b.try_into().map_err(|_| "bad real")?) as f64,
                    8 => f64::from_be_bytes(b.try_into().map_err(|_| "bad real")?),
                    _ => 0.0,
                })
            }
            0x3 => Plist::Date(f64::from_be_bytes(
                slice(at + 1, 8)?.try_into().map_err(|_| "bad date")?,
            )),
            0x4 => {
                let (n, start) = self.length(at, info)?;
                Plist::Data(slice(start, n)?.to_vec())
            }
            0x5 => {
                let (n, start) = self.length(at, info)?;
                Plist::String(String::from_utf8_lossy(slice(start, n)?).into_owned())
            }
            0x6 => {
                let (n, start) = self.length(at, info)?;
                let units: Vec<u16> = slice(start, n * 2)?
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&c| u16::from_be_bytes(c))
                    .collect();
                Plist::String(String::from_utf16_lossy(&units))
            }
            0x8 => Plist::Uid(be(slice(at + 1, info as usize + 1)?)),
            0xA | 0xC => {
                let (n, start) = self.length(at, info)?;
                let refs = slice(start, n * self.ref_size)?;
                let items = refs
                    .chunks_exact(self.ref_size)
                    .map(|r| self.object(be(r) as usize, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                Plist::Array(items)
            }
            0xD => {
                let (n, start) = self.length(at, info)?;
                let refs = slice(start, 2 * n * self.ref_size)?;
                let (keys, values) = refs.split_at(n * self.ref_size);
                let mut entries = Vec::with_capacity(n);
                for (k, v) in keys.chunks_exact(self.ref_size).zip(values.chunks_exact(self.ref_size)) {
                    let key = match self.object(be(k) as usize, depth + 1)? {
                        Plist::String(s) => s,
                        other => format!("{other:?}"),
                    };
                    entries.push((key, self.object(be(v) as usize, depth + 1)?));
                }
                Plist::Dict(entries)
            }
            t => return Err(format!("unknown binary plist object type {t:#x}")),
        })
    }
}

// --- XML ------------------------------------------------------------------------

struct Xml<'a> {
    s: &'a str,
    pos: usize,
}

enum Tag<'a> {
    Open(&'a str),
    Close(&'a str),
    Empty(&'a str),
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else { break };
        let entity = &rest[1..end];
        let ch = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

impl<'a> Xml<'a> {
    fn new(bytes: &'a [u8]) -> Xml<'a> {
        let s = std::str::from_utf8(bytes).unwrap_or("");
        Xml {
            s: s.trim_start_matches('\u{feff}'),
            pos: 0,
        }
    }

    /// The next tag, skipping text, comments, declarations and doctypes.
    fn tag(&mut self) -> Result<Tag<'a>, String> {
        loop {
            let rest = &self.s[self.pos..];
            let i = rest.find('<').ok_or("unexpected end of plist")?;
            self.pos += i;
            let rest = &self.s[self.pos..];
            if rest.starts_with("<!--") {
                self.pos += rest.find("-->").ok_or("unterminated comment")? + 3;
                continue;
            }
            if rest.starts_with("<?") || rest.starts_with("<!") {
                self.pos += rest.find('>').ok_or("unterminated declaration")? + 1;
                continue;
            }
            let end = rest.find('>').ok_or("unterminated tag")?;
            let inner = &rest[1..end];
            self.pos += end + 1;
            let name = |t: &'a str| t.split_whitespace().next().unwrap_or("");
            return Ok(if let Some(n) = inner.strip_prefix('/') {
                Tag::Close(name(n))
            } else if let Some(n) = inner.strip_suffix('/') {
                Tag::Empty(name(n))
            } else {
                Tag::Open(name(inner))
            });
        }
    }

    /// Text up to the closing tag of `name`.
    fn text(&mut self, name: &str) -> Result<String, String> {
        let close = format!("</{name}>");
        let rest = &self.s[self.pos..];
        let end = rest.find(&close).ok_or_else(|| format!("unterminated <{name}>"))?;
        let text = unescape(&rest[..end]);
        self.pos += end + close.len();
        Ok(text)
    }

    fn document(&mut self) -> Result<Plist, String> {
        match self.tag()? {
            Tag::Open("plist") => {
                let v = self.value()?;
                Ok(v)
            }
            // A bare value without the <plist> wrapper.
            Tag::Open(n) => self.value_for(n),
            Tag::Empty(n) => self.empty(n),
            Tag::Close(n) => Err(format!("unexpected </{n}>")),
        }
    }

    fn value(&mut self) -> Result<Plist, String> {
        match self.tag()? {
            Tag::Open(n) => self.value_for(n),
            Tag::Empty(n) => self.empty(n),
            Tag::Close(n) => Err(format!("unexpected </{n}>")),
        }
    }

    fn empty(&mut self, name: &str) -> Result<Plist, String> {
        Ok(match name {
            "true" => Plist::Bool(true),
            "false" => Plist::Bool(false),
            "dict" => Plist::Dict(Vec::new()),
            "array" => Plist::Array(Vec::new()),
            "string" => Plist::String(String::new()),
            "data" => Plist::Data(Vec::new()),
            other => return Err(format!("unexpected <{other}/>")),
        })
    }

    fn value_for(&mut self, name: &str) -> Result<Plist, String> {
        Ok(match name {
            "dict" => {
                let mut entries = Vec::new();
                loop {
                    match self.tag()? {
                        Tag::Close("dict") => break,
                        Tag::Open("key") => {
                            let key = self.text("key")?;
                            entries.push((key, self.value()?));
                        }
                        Tag::Empty("key") => entries.push((String::new(), self.value()?)),
                        _ => return Err("expected <key> in <dict>".into()),
                    }
                }
                Plist::Dict(entries)
            }
            "array" => {
                let mut items = Vec::new();
                loop {
                    let save = self.pos;
                    if let Tag::Close("array") = self.tag()? {
                        break;
                    }
                    self.pos = save;
                    items.push(self.value()?);
                }
                Plist::Array(items)
            }
            "string" => Plist::String(self.text("string")?),
            "integer" => {
                let t = self.text("integer")?;
                let t = t.trim();
                Plist::Integer(match t.strip_prefix("0x") {
                    Some(h) => i64::from_str_radix(h, 16).map_err(|e| e.to_string())?,
                    None => t.parse().map_err(|e| format!("bad <integer> {t:?}: {e}"))?,
                })
            }
            "real" => Plist::Real(self.text("real")?.trim().parse().unwrap_or(0.0)),
            "date" => {
                self.text("date")?;
                Plist::Date(0.0)
            }
            "data" => Plist::Data(base64(&self.text("data")?)),
            "true" => {
                self.text("true")?;
                Plist::Bool(true)
            }
            "false" => {
                self.text("false")?;
                Plist::Bool(false)
            }
            other => return Err(format!("unexpected <{other}> in plist")),
        })
    }
}

fn base64(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for v in text.bytes().filter_map(value) {
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml() {
        let text = br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleExecutable</key>
	<string>My App</string>
	<!-- a comment -->
	<key>CFBundleIdentifier</key>
	<string>com.example.a&amp;b</string>
	<key>UIRequiredDeviceCapabilities</key>
	<array><string>arm64</string></array>
	<key>LSRequiresIPhoneOS</key>
	<true/>
	<key>Size</key>
	<integer>42</integer>
	<key>Blob</key>
	<data>AQID</data>
	<key>Empty</key>
	<dict/>
</dict>
</plist>"#;
        let p = Plist::parse(text).unwrap();
        assert_eq!(p.str("CFBundleExecutable"), Some("My App"));
        assert_eq!(p.str("CFBundleIdentifier"), Some("com.example.a&b"));
        assert_eq!(p.strings("UIRequiredDeviceCapabilities"), ["arm64"]);
        assert_eq!(p.get("LSRequiresIPhoneOS"), Some(&Plist::Bool(true)));
        assert_eq!(p.get("Size"), Some(&Plist::Integer(42)));
        assert_eq!(p.get("Blob"), Some(&Plist::Data(vec![1, 2, 3])));
        assert_eq!(p.get("Empty"), Some(&Plist::Dict(Vec::new())));
    }

    #[test]
    fn binary() {
        // {"CFBundleExecutable": "App", "Numbers": [1, 300], "OK": true}, as plutil writes it.
        let mut b = b"bplist00".to_vec();
        let mut offsets = Vec::new();
        let mut obj = |b: &mut Vec<u8>, bytes: &[u8]| {
            offsets.push(b.len() as u8);
            b.extend_from_slice(bytes);
        };
        obj(&mut b, &[0xD3, 1, 2, 3, 4, 5, 6]); // 0: dict of 3: keys 1,2,3 values 4,5,6
        obj(&mut b, &[&[0x5F, 0x10, 18][..], b"CFBundleExecutable"].concat()); // 1
        obj(&mut b, &[&[0x57][..], b"Numbers"].concat()); // 2
        obj(&mut b, &[&[0x52][..], b"OK"].concat()); // 3
        obj(&mut b, &[&[0x53][..], b"App"].concat()); // 4
        obj(&mut b, &[0xA2, 7, 8]); // 5: array of objects 7, 8
        obj(&mut b, &[0x09]); // 6: true
        obj(&mut b, &[0x10, 1]); // 7: 1
        obj(&mut b, &[0x11, 0x01, 0x2C]); // 8: 300
        let table = b.len();
        b.extend_from_slice(&offsets);
        b.extend_from_slice(&[0, 0, 0, 0, 0, 0, 1, 1]);
        b.extend_from_slice(&(offsets.len() as u64).to_be_bytes());
        b.extend_from_slice(&0u64.to_be_bytes());
        b.extend_from_slice(&(table as u64).to_be_bytes());
        let p = Plist::parse(&b).unwrap();
        assert_eq!(p.str("CFBundleExecutable"), Some("App"));
        assert_eq!(
            p.get("Numbers"),
            Some(&Plist::Array(vec![Plist::Integer(1), Plist::Integer(300)]))
        );
        assert_eq!(p.get("OK"), Some(&Plist::Bool(true)));
    }
}
