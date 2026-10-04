//! The libraries a program was linked with, from the version strings they
//! carry. Sony's Psy-Q libraries (and plenty of others from the CVS era) keep
//! their RCS `$Id: sys.c,v 1.140 1998/01/12 07:52:27 noda Exp yos $` in the
//! object code, one per source file. Each says a file, its revision and when
//! it was checked in; the newest date says the earliest the library could
//! have been built, and the revisions identify the release once a known build
//! is compared (see [`crate::sigs`], which does that by code).

use serde::Serialize;

use crate::binary::Binary;

/// A source file a library was built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySource {
    /// Where the string is.
    pub address: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    /// `sys.c`.
    pub file: String,
    /// `1.140`.
    pub revision: String,
    /// `1998-01-12`.
    pub date: String,
    /// Who checked it in.
    pub author: String,
}

/// The parts of an RCS `$Id: file,v rev yyyy/mm/dd hh:mm:ss author state … $`.
pub fn parse_id(text: &str) -> Option<LibrarySource> {
    let start = text.find("$Id: ")?;
    let tail = &text[start + 5..];
    let body = &tail[..tail.find('$')?];
    let mut words = body.split_whitespace();
    let file = words.next()?.strip_suffix(",v")?;
    let revision = words.next()?;
    let date = words.next()?;
    let time = words.next()?;
    let author = words.next()?;
    let ok = revision
        .split('.')
        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    let mut ymd = date.split('/');
    let (y, m, d) = (ymd.next()?, ymd.next()?, ymd.next()?);
    let hms: Vec<_> = time.split(':').collect();
    if !ok
        || file.is_empty()
        || y.len() != 4
        || m.len() != 2
        || d.len() != 2
        || ymd.next().is_some()
        || !(y.bytes().chain(m.bytes()).chain(d.bytes())).all(|b| b.is_ascii_digit())
        || !(1..=12).contains(&m.parse::<u32>().ok()?)
        || !(1..=31).contains(&d.parse::<u32>().ok()?)
        || hms.len() != 3
        || hms
            .iter()
            .any(|n| n.len() != 2 || !n.bytes().all(|b| b.is_ascii_digit()))
        || hms[0].parse::<u32>().ok()? > 23
        || hms[1].parse::<u32>().ok()? > 59
        || hms[2].parse::<u32>().ok()? > 60
    {
        return None;
    }
    Some(LibrarySource {
        address: None,
        offset: None,
        file: file.rsplit('/').next().unwrap_or(file).to_string(),
        revision: revision.to_string(),
        date: format!("{y}-{m}-{d}"),
        author: author.to_string(),
    })
}

impl Binary {
    /// The source files the code was built from that say so, in address order.
    pub fn library_sources(&self) -> Vec<LibrarySource> {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for offset in memchr::memmem::find_iter(self.data(), b"$Id: ") {
            let tail = &self.data()[offset..self.data().len().min(offset.saturating_add(512))];
            let Some(end) = tail[5..].iter().position(|b| *b == b'$').map(|n| n + 6) else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(&tail[..end]) else {
                continue;
            };
            if !text.bytes().all(|b| b.is_ascii_graphic() || b == b' ') {
                continue;
            }
            let Some(mut id) = parse_id(text) else { continue };
            if !seen.insert((id.file.clone(), id.revision.clone(), id.date.clone())) {
                continue;
            }
            id.offset = Some(offset as u64);
            id.address = self.offset_to_address(offset as u64);
            out.push(id);
        }
        out.sort_by(|a, b| (a.address, a.offset, &a.file).cmp(&(b.address, b.offset, &b.file)));
        out
    }
}

/// The sources, one to a line, and what their dates say.
pub fn sources_text(sources: &[LibrarySource]) -> String {
    if sources.is_empty() {
        return "No RCS `$Id:` strings: the libraries carry no version strings of their own.\n".into();
    }
    let mut out = format!("{} source files carry their version:\n", sources.len());
    for s in sources {
        let at = s.address.map_or_else(String::new, |a| format!("{a:#x}  "));
        out.push_str(&format!(
            "  {at}{:<14} {:<8} {}  {}\n",
            s.file, s.revision, s.date, s.author
        ));
    }
    let oldest = sources.iter().map(|s| &s.date).min().expect("not empty");
    let newest = sources.iter().map(|s| &s.date).max().expect("not empty");
    out.push_str(&format!(
        "Checked in between {oldest} and {newest}: the libraries were built on or after {newest}.\n"
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_string_is_read() {
        let s = parse_id("$Id: sys.c,v 1.140 1998/01/12 07:52:27 noda Exp yos $").unwrap();
        assert_eq!(
            (s.file.as_str(), s.revision.as_str(), s.date.as_str()),
            ("sys.c", "1.140", "1998-01-12")
        );
        assert_eq!(s.author, "noda");
        let s = parse_id("$Id: /cvs/psx/lib/intr.c,v 1.75 1997/02/07 09:00:36 makoto Exp $").unwrap();
        assert_eq!((s.file.as_str(), s.date.as_str()), ("intr.c", "1997-02-07"));
        assert!(parse_id("$Id$").is_none());
        assert!(parse_id("$Id: not rcs at all").is_none());
        assert!(parse_id("$Id: a.c,v x.y 1998/01/12 07:52:27 me Exp $").is_none());
    }
    #[test]
    fn targeted_scan_keeps_code_embedded_ids_and_refuses_truncation() {
        let id = b"$Id: sys.c,v 1.140 1998/01/12 07:52:27 noda Exp $";
        let mut raw = vec![0u8; 0x100];
        raw.extend(id);
        raw.extend([0, 0xff, 0]);
        raw.extend(id);
        raw.extend(b"$Id: bad.c,v 1.2 1998/01/12 00:00:00 n Exp");
        let bin = Binary::parse_psx_overlay(raw, 0x80010000, None).unwrap();
        let sources = bin.library_sources();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].offset, Some(0x100));
        assert_eq!(sources[0].address, Some(0x80010100));
        assert!(parse_id("$Id: x.c,v 1.2 1998/01/12 00:00:00 n Exp").is_none());
    }
}
