//! Small helpers: endian-aware reads, formatting, demangling.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Endian {
    Little,
    Big,
}

/// Bounds-checked, endian-aware reads from a byte slice. Out-of-range reads
/// return `None` so malformed files never panic.
#[derive(Clone, Copy)]
pub(crate) struct Bytes<'a> {
    pub data: &'a [u8],
    pub endian: Endian,
}

impl<'a> Bytes<'a> {
    pub fn new(data: &'a [u8], endian: Endian) -> Self {
        Bytes { data, endian }
    }

    pub fn slice(&self, offset: u64, len: u64) -> Option<&'a [u8]> {
        let start = usize::try_from(offset).ok()?;
        let end = start.checked_add(usize::try_from(len).ok()?)?;
        self.data.get(start..end)
    }

    fn array<const N: usize>(&self, offset: u64) -> Option<[u8; N]> {
        self.slice(offset, N as u64)?.try_into().ok()
    }

    pub fn u8(&self, offset: u64) -> Option<u8> {
        self.data.get(usize::try_from(offset).ok()?).copied()
    }

    pub fn u16(&self, offset: u64) -> Option<u16> {
        let b = self.array::<2>(offset)?;
        Some(match self.endian {
            Endian::Little => u16::from_le_bytes(b),
            Endian::Big => u16::from_be_bytes(b),
        })
    }

    pub fn u32(&self, offset: u64) -> Option<u32> {
        let b = self.array::<4>(offset)?;
        Some(match self.endian {
            Endian::Little => u32::from_le_bytes(b),
            Endian::Big => u32::from_be_bytes(b),
        })
    }

    pub fn u64(&self, offset: u64) -> Option<u64> {
        let b = self.array::<8>(offset)?;
        Some(match self.endian {
            Endian::Little => u64::from_le_bytes(b),
            Endian::Big => u64::from_be_bytes(b),
        })
    }

    /// Reads a word of the given size (1, 2, 4 or 8 bytes).
    pub fn uint(&self, offset: u64, size: u32) -> Option<u64> {
        match size {
            1 => self.u8(offset).map(u64::from),
            2 => self.u16(offset).map(u64::from),
            4 => self.u32(offset).map(u64::from),
            8 => self.u64(offset),
            _ => None,
        }
    }

    /// NUL-terminated string starting at `offset`, limited to `max` bytes.
    pub fn cstr(&self, offset: u64, max: u64) -> Option<&'a [u8]> {
        let start = usize::try_from(offset).ok()?;
        let rest = self.data.get(start..)?;
        let limit = rest.len().min(usize::try_from(max).unwrap_or(usize::MAX));
        let rest = &rest[..limit];
        let len = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        Some(&rest[..len])
    }
}

pub(crate) fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Fixed-size, NUL-padded name field (e.g. Mach-O segname, PE section name).
pub(crate) fn fixed_str(bytes: &[u8]) -> String {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    lossy(&bytes[..len])
}

pub(crate) fn hex(v: u64) -> String {
    format!("{v:#x}")
}

pub(crate) fn hex_bytes(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub(crate) fn hex_compact(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Quoted string with non-printable bytes escaped.
pub(crate) fn quote(bytes: &[u8]) -> String {
    let mut s = String::from("\"");
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            match c {
                '"' => s.push_str("\\\""),
                '\\' => s.push_str("\\\\"),
                '\n' => s.push_str("\\n"),
                '\r' => s.push_str("\\r"),
                '\t' => s.push_str("\\t"),
                c if c.is_control() => s.push_str(&format!("\\u{{{:x}}}", c as u32)),
                c => s.push(c),
            }
        }
        for b in chunk.invalid() {
            s.push_str(&format!("\\x{b:02x}"));
        }
    }
    s.push('"');
    s
}

/// Formats a Unix timestamp as a UTC date without pulling in a date crate.
pub(crate) fn unix_time(secs: u64) -> String {
    if secs == 0 {
        return "0 (not set)".into();
    }
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

pub(crate) fn uuid(bytes: &[u8]) -> String {
    if bytes.len() != 16 {
        return hex_compact(bytes);
    }
    let h = hex_compact(bytes).to_uppercase();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// Microsoft GUID layout: first three groups are little-endian.
pub(crate) fn guid(b: &[u8; 16]) -> String {
    format!(
        "{:08X}-{:04X}-{:04X}-{}-{}",
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        u16::from_le_bytes([b[4], b[5]]),
        u16::from_le_bytes([b[6], b[7]]),
        hex_compact(&b[8..10]).to_uppercase(),
        hex_compact(&b[10..16]).to_uppercase()
    )
}

/// Mach-O packed version "X.Y.Z" (xxxx.yy.zz nibbles).
pub(crate) fn macho_version(v: u32) -> String {
    let (major, minor, patch) = (v >> 16, (v >> 8) & 0xff, v & 0xff);
    if patch == 0 {
        format!("{major}.{minor}")
    } else {
        format!("{major}.{minor}.{patch}")
    }
}

pub(crate) fn align_up(v: u64, align: u64) -> u64 {
    if align <= 1 {
        v
    } else {
        v.div_ceil(align).saturating_mul(align)
    }
}

/// Best-effort demangling of Rust (legacy + v0), Itanium C++ and MSVC names.
pub fn demangle(name: &str) -> Option<String> {
    // Mach-O and 32-bit Windows prefix C symbols with an underscore.
    let stripped = name.strip_prefix('_').unwrap_or(name);
    for candidate in [name, stripped] {
        if let Ok(d) = rustc_demangle::try_demangle(candidate) {
            let s = format!("{d:#}");
            if s != candidate {
                return Some(s);
            }
        }
    }
    if name.starts_with("_Z") || name.starts_with("__Z") || name.starts_with("___Z") {
        let mut n = name;
        while n.starts_with("__Z") {
            n = &n[1..];
        }
        if let Ok(sym) = cpp_demangle::Symbol::new(n.as_bytes()) {
            let options = cpp_demangle::DemangleOptions::new().no_return_type();
            if let Ok(s) = sym.demangle_with_options(&options) {
                return Some(s);
            }
        }
    }
    if name.starts_with('?') {
        let flags = msvc_demangler::DemangleFlags::llvm();
        if let Ok(s) = msvc_demangler::demangle(name, flags) {
            return Some(s);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(unix_time(1_700_000_000), "2023-11-14 22:13:20 UTC");
        assert_eq!(unix_time(951_782_400), "2000-02-29 00:00:00 UTC");
    }

    #[test]
    fn demangling() {
        assert_eq!(
            demangle("_ZN4core3fmt5write17h0123456789abcdefE").as_deref(),
            Some("core::fmt::write")
        );
        assert_eq!(
            demangle("_RNvCs28LAwZlMSxg_4tiny5STATE").as_deref(),
            Some("tiny::STATE")
        );
        assert_eq!(
            demangle("_ZN3geo6CircleC2ENS_5PointEd").as_deref(),
            Some("geo::Circle::Circle(geo::Point, double)")
        );
        assert_eq!(demangle("main"), None);
    }

    #[test]
    fn strings() {
        assert_eq!(quote(b"a\"b\n\xff"), "\"a\\\"b\\n\\xff\"");
        assert_eq!(fixed_str(b"__TEXT\0\0\0"), "__TEXT");
    }
}
