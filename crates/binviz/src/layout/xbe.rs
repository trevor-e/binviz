//! XBE file layout: the headers the Xbox maps at the base address (the image
//! header, the certificate, the section headers with their names and shared
//! page counters, the library versions, the debug file names, the logo),
//! the kernel thunk table and TLS directory inside the sections, and each
//! section's bytes.

use super::decode::{Decoder, Table, TableKind};
use super::fields::{self, F, Fmt};
use super::{Builder, Ctx};
use crate::model::RegionKind;
use crate::util::{self, hex};
use crate::xbe::{self, Build, Header, IMAGE_HEADER_SIZE, LIBRARY_VERSION_SIZE, SECTION_HEADER_SIZE};

/// The image header, 0x178 bytes (`xbe::Header` notes each field's offset).
const IMAGE_HEADER: [F; 31] = [
    F::str("Magic", 4),
    F::bytes("Digital signature", 256),
    F::u32("Base address", Fmt::Addr),
    F::u32("Size of headers", Fmt::Size),
    F::u32("Size of image", Fmt::Size),
    F::u32("Size of image header", Fmt::Size),
    F::u32("Timestamp", Fmt::Time),
    F::u32("Certificate address", Fmt::Addr),
    F::u32("Number of sections", Fmt::Dec),
    F::u32("Section headers address", Fmt::Addr),
    F::u32("Initialization flags", Fmt::Flags(xbe::init_flags)),
    F::u32("Entry point (encoded)", Fmt::Hex),
    F::u32("TLS directory address", Fmt::Addr),
    F::u32("PE stack commit", Fmt::Size),
    F::u32("PE heap reserve", Fmt::Size),
    F::u32("PE heap commit", Fmt::Size),
    F::u32("PE base address", Fmt::Addr),
    F::u32("PE size of image", Fmt::Size),
    F::u32("PE checksum", Fmt::Hex),
    F::u32("PE timestamp", Fmt::Time),
    F::u32("Debug path name address", Fmt::Addr),
    F::u32("Debug file name address", Fmt::Addr),
    F::u32("Debug Unicode file name address", Fmt::Addr),
    F::u32("Kernel thunk table address (encoded)", Fmt::Hex),
    F::u32("Non-kernel import directory address", Fmt::Addr),
    F::u32("Number of library versions", Fmt::Dec),
    F::u32("Library versions address", Fmt::Addr),
    F::u32("Kernel library version address", Fmt::Addr),
    F::u32("XAPI library version address", Fmt::Addr),
    F::u32("Logo bitmap address", Fmt::Addr),
    F::u32("Logo bitmap size", Fmt::Size),
];

/// What later XDKs write after the logo's size, as far as the image header's
/// size says there is room. Unconfirmed: the names are Cxbx-Reloaded's, which
/// reads the first two as the address and number of library feature entries
/// (shaped like library versions) and the third as a debug-info address.
const LATER_IMAGE_HEADER: [F; 3] = [
    F::u32("Library features address", Fmt::Addr),
    F::u32("Number of library features", Fmt::Dec),
    F::u32("Debug info address", Fmt::Addr),
];

/// The certificate: 0x1D0 bytes in most XBEs (its first field says how many).
const CERTIFICATE: [F; 13] = [
    F::u32("Size", Fmt::Size),
    F::u32("Timestamp", Fmt::Time),
    F::u32("Title ID", Fmt::Hex),
    // 40 UTF-16 characters.
    F::bytes("Title name", 80),
    // 16 title IDs.
    F::bytes("Alternate title IDs", 64),
    F::u32("Allowed media", Fmt::Flags(xbe::media)),
    F::u32("Game regions", Fmt::Flags(xbe::regions)),
    F::u32("Game ratings", Fmt::Hex),
    F::u32("Disk number", Fmt::Dec),
    F::u32("Version", Fmt::Hex),
    F::bytes("LAN key", 16),
    F::bytes("Signature key", 16),
    // 16 keys of 16 bytes, one per alternate title ID.
    F::bytes("Alternate signature keys", 256),
];

/// What some later certificates add, up to 0x1EC. Unconfirmed: the names are
/// Cxbx-Reloaded's.
const LATER_CERTIFICATE: [F; 4] = [
    F::u32("Original certificate size", Fmt::Size),
    F::u32("Online service ID", Fmt::Hex),
    F::u32("Security flags", Fmt::Hex),
    F::bytes("Code encryption key", 16),
];

const SECTION_HEADER: [F; 10] = [
    F::u32("Flags", Fmt::Flags(xbe::section_flags)),
    F::u32("Virtual address", Fmt::Addr),
    F::u32("Virtual size", Fmt::Size),
    F::u32("Raw address", Fmt::Off),
    F::u32("Raw size", Fmt::Size),
    F::u32("Name address", Fmt::Addr),
    // How many times the section is loaded; zero in the file.
    F::u32("Reference count", Fmt::Dec),
    F::u32("Head shared page reference count address", Fmt::Addr),
    F::u32("Tail shared page reference count address", Fmt::Addr),
    F::bytes("Digest (SHA-1)", 20),
];

const LIBRARY_VERSION: [F; 5] = [
    F::str("Name", 8),
    F::u16("Major version", Fmt::Dec),
    F::u16("Minor version", Fmt::Dec),
    F::u16("Build", Fmt::Dec),
    F::u16("Flags", Fmt::Custom(library_flags)),
];

/// IMAGE_TLS_DIRECTORY32, as in a PE.
const TLS_DIRECTORY: [F; 6] = [
    F::u32("StartAddressOfRawData", Fmt::Addr),
    F::u32("EndAddressOfRawData", Fmt::Addr),
    F::u32("AddressOfIndex", Fmt::Addr),
    F::u32("AddressOfCallBacks", Fmt::Addr),
    F::u32("SizeOfZeroFill", Fmt::Size),
    F::u32("Characteristics", Fmt::Hex),
];

/// An entry of the non-kernel import directory, which development builds use
/// to import from other executables (the debug monitor, say) and games leave
/// empty. Unconfirmed: laid out as recalled from the XDK's headers, the
/// thunk table's address, then the executable's name in UTF-16, up to an
/// entry of zeros.
const IMPORT_DESCRIPTOR: [F; 2] = [
    F::u32("Thunk table address", Fmt::Addr),
    F::u32("Image name address", Fmt::Addr),
];

fn library_flags(_: &Ctx, v: u64) -> String {
    xbe::library_flags(v)
}

pub(crate) fn build(b: &mut Builder) {
    let Some(h) = Header::read(b.ctx.bytes.data) else {
        return;
    };
    let headers = b.region(
        0,
        h.size_of_headers.max(IMAGE_HEADER_SIZE),
        RegionKind::Header,
        "Headers",
    );
    b.set_value(headers, format!("mapped at {}", hex(h.base)));
    b.set_note(headers, "Mapped at the base address as they are in the file");
    b.set_fill_gaps(headers);
    image_header(b, &h);
    certificate(b, &h);
    section_headers(b, &h);
    library_versions(b, &h);
    debug_names(b, &h);
    if let Some(off) = h.offset(h.logo)
        && h.logo_size > 0
    {
        let id = b.region(off, h.logo_size, RegionKind::Resources, "Logo bitmap");
        b.set_value(id, format!("{} bytes", h.logo_size));
        b.set_note(id, "The Microsoft logo, run-length encoded");
    }
    import_directory(b, &h);
    sections(b);
    kernel_thunks(b, &h);
    tls(b, &h);
}

/// How many of `specs` fit in `size` bytes.
fn fitting(specs: &[F], size: u64) -> usize {
    let mut total = 0;
    specs
        .iter()
        .take_while(|f| {
            total += f.ty.size();
            total <= size
        })
        .count()
}

/// An address in the headers, with its file offset.
fn in_headers(h: &Header, va: u64) -> String {
    match h.offset(va) {
        Some(off) if va != 0 => format!("{} (file offset {})", hex(va), hex(off)),
        _ => hex(va),
    }
}

/// An address in the image, with the section it is in.
fn in_section(ctx: &Ctx, va: u64) -> String {
    match ctx.sections.iter().find(|s| va >= s.address && va - s.address < s.size) {
        Some(s) if va != 0 => format!("{} (in {})", hex(va), s.name),
        _ => hex(va),
    }
}

/// The file offset of an address in the headers or in a section's bytes.
fn offset_of(ctx: &Ctx, h: &Header, va: u64) -> Option<u64> {
    h.offset(va).or_else(|| xbe::section_offset(ctx.sections, va))
}

/// An address of ASCII text, with the text.
fn ascii_at(ctx: &Ctx, h: &Header, va: u64) -> String {
    match offset_of(ctx, h, va).and_then(|o| ctx.bytes.cstr(o, 1024)) {
        Some(text) if va != 0 => format!("{} → {}", hex(va), util::quote(text)),
        _ => hex(va),
    }
}

/// UTF-16 text at a file offset (512 characters at most), and its length in
/// bytes with the terminating zero.
fn utf16_at(ctx: &Ctx, off: u64) -> (String, u64) {
    let data = ctx.bytes.data;
    let bytes = usize::try_from(off).ok().and_then(|o| data.get(o..)).unwrap_or(&[]);
    let bytes = &bytes[..bytes.len().min(1024)];
    let units = bytes.chunks_exact(2).take_while(|c| c[0] | c[1] != 0).count() as u64;
    (xbe::utf16(bytes), (units + 1) * 2)
}

/// A header field XOR-ed with a key: what it decodes to, with which key.
fn encoded(raw: u64, decoded: Option<(Build, u64, u32)>) -> String {
    match decoded {
        Some((build, address, key)) => format!(
            "{} → {} (XOR {key:#010x}, the {} key)",
            hex(raw),
            hex(address),
            build.name()
        ),
        None => format!("{} (no known key decodes it into the image)", hex(raw)),
    }
}

fn image_header(b: &mut Builder, h: &Header) {
    let ctx = b.ctx;
    let size = h
        .size_of_image_header
        .clamp(IMAGE_HEADER_SIZE, h.size_of_headers.max(IMAGE_HEADER_SIZE));
    let Some(id) = b.region(0, size, RegionKind::Header, "Image header") else {
        return;
    };
    b.set_note(
        Some(id),
        "Where the image loads and starts, the kernel thunk table, and where the rest of the headers are",
    );
    let keys = h.keys();
    let mut values = fields::decode(&ctx, 0, &IMAGE_HEADER);
    for v in &mut values {
        v.value = match v.name {
            "Entry point (encoded)" => encoded(v.raw, keys.map(|(k, entry, _)| (k, entry, k.entry_key()))),
            "Kernel thunk table address (encoded)" => {
                encoded(v.raw, keys.map(|(k, _, thunks)| (k, thunks, k.thunk_key())))
            }
            "Certificate address" | "Section headers address" | "Library versions address" | "Logo bitmap address" => {
                in_headers(h, v.raw)
            }
            "Kernel library version address" | "XAPI library version address" => {
                match h.offset(v.raw).and_then(|o| xbe::Library::read(&ctx.bytes, o)) {
                    Some(lib) if v.raw != 0 => format!("{} → {}", hex(v.raw), lib.describe()),
                    _ => hex(v.raw),
                }
            }
            "Debug path name address" | "Debug file name address" => ascii_at(&ctx, h, v.raw),
            "Debug Unicode file name address" => match h.offset(v.raw) {
                Some(o) if v.raw != 0 => {
                    format!("{} → L{}", hex(v.raw), util::quote(utf16_at(&ctx, o).0.as_bytes()))
                }
                _ => hex(v.raw),
            },
            "TLS directory address" | "Non-kernel import directory address" => in_section(&ctx, v.raw),
            _ => continue,
        };
    }
    b.add_field_nodes(id, &values);
    let fixed = fields::struct_size(&IMAGE_HEADER);
    let n = fitting(&LATER_IMAGE_HEADER, size - fixed);
    let later = b.fields(id, fixed, &LATER_IMAGE_HEADER[..n]);
    let end = fixed + fields::struct_size(&LATER_IMAGE_HEADER[..n]);
    if size > end
        && let Some(rest) = b.child(id, end, size - end, RegionKind::Header, "Further fields")
    {
        b.node_mut(rest).is_field = true;
    }
    // The library features, when the header has room for where they are, and
    // (their reading being unconfirmed) no other structure starts among them.
    let (address, count) = (
        fields::get(&later, "Library features address"),
        fields::get(&later, "Number of library features"),
    );
    let others = [
        h.certificate,
        h.section_headers,
        h.libraries,
        h.debug_path,
        h.debug_unicode_file,
        h.logo,
    ];
    if n >= 2
        && let Some(off) = h.offset(address)
        && count > 0
        && count <= 256
        && off + count * LIBRARY_VERSION_SIZE <= h.size_of_headers
        && !others
            .iter()
            .filter_map(|&a| h.offset(a))
            .any(|o| o > off && o < off + count * LIBRARY_VERSION_SIZE)
    {
        library_table(b, off, count, "Library features");
    }
}

fn certificate(b: &mut Builder, h: &Header) {
    let ctx = b.ctx;
    let Some(off) = h.offset(h.certificate) else { return };
    let fixed = fields::struct_size(&CERTIFICATE);
    // Its size, as its first field says, inside the headers.
    let size = match ctx.bytes.u32(off).map_or(0, u64::from) {
        0 => fixed,
        n => n,
    }
    .min(h.size_of_headers - off);
    let Some(id) = b.region(off, size, RegionKind::Header, "Certificate") else {
        return;
    };
    b.set_note(
        Some(id),
        "The title, the media and regions it may run from, its version and keys",
    );
    let n = fitting(&CERTIFICATE, size);
    let mut values = fields::decode(&ctx, off, &CERTIFICATE[..n]);
    for v in &mut values {
        let bytes = ctx.bytes.slice(v.start, v.end - v.start).unwrap_or(&[]);
        v.value = match v.name {
            "Title ID" => xbe::title_id(v.raw as u32),
            "Title name" => format!("L{}", util::quote(xbe::utf16(bytes).as_bytes())),
            "Alternate title IDs" => {
                let ids: Vec<String> = bytes
                    .chunks_exact(4)
                    .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .filter(|&id| id != 0)
                    .map(xbe::title_id)
                    .collect();
                if ids.is_empty() { "none".into() } else { ids.join(", ") }
            }
            _ => continue,
        };
    }
    if let Some(title) = values.iter().find(|v| v.name == "Title name") {
        b.set_value(Some(id), title.value.clone());
    }
    b.add_field_nodes(id, &values);
    if size > fixed {
        let n = fitting(&LATER_CERTIFICATE, size - fixed);
        b.fields(id, off + fixed, &LATER_CERTIFICATE[..n]);
        let end = fixed + fields::struct_size(&LATER_CERTIFICATE[..n]);
        if size > end
            && let Some(rest) = b.child(id, off + end, size - end, RegionKind::Header, "Further fields")
        {
            b.node_mut(rest).is_field = true;
        }
    }
}

fn section_headers(b: &mut Builder, h: &Header) {
    let ctx = b.ctx;
    let Some(off) = h.offset(h.section_headers) else { return };
    let list = xbe::section_headers(&ctx.bytes, h);
    let table = b.region(
        off,
        list.len() as u64 * SECTION_HEADER_SIZE,
        RegionKind::Metadata,
        "Section headers",
    );
    b.set_value(table, format!("{} sections", list.len()));
    if let Some(table) = table {
        for (i, s) in list.iter().enumerate() {
            let at = off + i as u64 * SECTION_HEADER_SIZE;
            // The section's name, or what stands for it when it can't be read.
            let name = ctx.sections.get(i).map_or(s.name.as_str(), |sec| sec.name.as_str());
            let Some(id) = b.child(
                table,
                at,
                SECTION_HEADER_SIZE,
                RegionKind::Metadata,
                format!("Section header {name}"),
            ) else {
                continue;
            };
            let mut values = fields::decode(&ctx, at, &SECTION_HEADER);
            for v in &mut values {
                v.value = match v.name {
                    "Name address" => ascii_at(&ctx, h, v.raw),
                    "Head shared page reference count address" | "Tail shared page reference count address" => {
                        in_headers(h, v.raw)
                    }
                    _ => continue,
                };
            }
            b.add_field_nodes(id, &values);
            b.set_value(
                Some(id),
                format!(
                    "{}..{}, {}",
                    hex(s.address),
                    hex(s.address + s.size),
                    xbe::section_flags(s.flags as u64)
                ),
            );
        }
    }
    for s in &list {
        if let Some(o) = h.offset(s.name_address) {
            let len = ctx.bytes.cstr(o, 64).map_or(0, |c| c.len() as u64);
            let id = b.region(o, len + 1, RegionKind::Strings, format!("Section name {}", s.name));
            b.set_value(id, util::quote(s.name.as_bytes()));
        }
    }
    // The 16-bit counters of the pages sections share: the tail of one
    // section and the head of the next point at the same one when they share a page.
    let mut counters: Vec<(u64, String)> = Vec::new();
    for s in &list {
        for (address, end) in [(s.head_page, "head"), (s.tail_page, "tail")] {
            let Some(o) = h.offset(address) else { continue };
            match counters.iter_mut().find(|c| c.0 == o) {
                Some(c) => c.1.push_str(&format!(", {end} of {}", s.name)),
                None => counters.push((o, format!("{end} of {}", s.name))),
            }
        }
    }
    counters.sort_by_key(|c| c.0);
    let mut i = 0;
    while i < counters.len() {
        // A run of adjacent counters is one array.
        let mut j = i + 1;
        while j < counters.len() && counters[j].0 == counters[j - 1].0 + 2 {
            j += 1;
        }
        let (start, end) = (counters[i].0, counters[j - 1].0 + 2);
        if let Some(id) = b.region(start, end - start, RegionKind::Metadata, "Shared page reference counts") {
            b.set_note(
                Some(id),
                "Counters the loader keeps for the pages sections share (zero in the file)",
            );
            for (o, what) in &counters[i..j] {
                let mut name = what.clone();
                name[..1].make_ascii_uppercase();
                if let Some(c) = b.child(id, *o, 2, RegionKind::Metadata, name) {
                    b.node_mut(c).is_field = true;
                    b.set_value(Some(c), ctx.bytes.u16(*o).unwrap_or(0).to_string());
                }
            }
        }
        i = j;
    }
}

/// A table of library versions (or features) at `off`.
fn library_table(b: &mut Builder, off: u64, count: u64, name: &str) -> Option<u32> {
    let ctx = b.ctx;
    let table = b.region(off, count * LIBRARY_VERSION_SIZE, RegionKind::Metadata, name)?;
    b.set_value(Some(table), format!("{count} entries"));
    for i in 0..count {
        let at = off + i * LIBRARY_VERSION_SIZE;
        let Some(lib) = xbe::Library::read(&ctx.bytes, at) else {
            break;
        };
        b.struct_child(table, at, RegionKind::Metadata, lib.describe(), &LIBRARY_VERSION);
    }
    Some(table)
}

fn library_versions(b: &mut Builder, h: &Header) {
    let Some(off) = h.offset(h.libraries) else { return };
    let count = h.library_count.min(1024);
    if count == 0 {
        return;
    }
    let id = library_table(b, off, count, "Library versions");
    b.set_note(
        id,
        "The XDK libraries the game linked, each with its build: XAPILIB's is the XDK's",
    );
}

fn debug_names(b: &mut Builder, h: &Header) {
    let ctx = b.ctx;
    let path = h
        .offset(h.debug_path)
        .filter(|_| h.debug_path != 0)
        .and_then(|o| Some((o, ctx.bytes.cstr(o, 1024)?)));
    let file = h
        .offset(h.debug_file)
        .filter(|_| h.debug_file != 0)
        .and_then(|o| Some((o, ctx.bytes.cstr(o, 1024)?)));
    let path_id = path.and_then(|(o, text)| {
        let id = b.region(o, text.len() as u64 + 1, RegionKind::Debug, "Debug path name");
        b.set_value(id, util::quote(text));
        b.set_note(id, "Where the PE the image was made from was built");
        id.map(|id| (id, o, o + text.len() as u64 + 1))
    });
    if let Some((o, text)) = file {
        let size = text.len() as u64 + 1;
        // Usually the path's last part.
        let id = match path_id {
            Some((parent, start, end)) if o >= start && o + size <= end => {
                b.child(parent, o, size, RegionKind::Debug, "Debug file name")
            }
            _ => b.region(o, size, RegionKind::Debug, "Debug file name"),
        };
        b.set_value(id, util::quote(text));
    }
    if h.debug_unicode_file != 0
        && let Some(o) = h.offset(h.debug_unicode_file)
    {
        let (text, size) = utf16_at(&ctx, o);
        let id = b.region(o, size, RegionKind::Debug, "Debug Unicode file name");
        b.set_value(id, format!("L{}", util::quote(text.as_bytes())));
    }
}

fn import_directory(b: &mut Builder, h: &Header) {
    let ctx = b.ctx;
    if h.import_directory == 0 {
        return;
    }
    let Some(off) = offset_of(&ctx, h, h.import_directory) else {
        return;
    };
    let size = fields::struct_size(&IMPORT_DESCRIPTOR);
    let count = (0..64)
        .take_while(|&i| ctx.bytes.u64(off + i * size).is_some_and(|v| v != 0))
        .count() as u64;
    let Some(id) = b.region(
        off,
        (count + 1) * size,
        RegionKind::Linking,
        "Non-kernel import directory",
    ) else {
        return;
    };
    b.set_note(
        Some(id),
        "Imports from executables other than the kernel (layout unconfirmed); games leave it empty",
    );
    for i in 0..count {
        let at = off + i * size;
        let (_, v) = b.struct_child(
            id,
            at,
            RegionKind::Linking,
            format!("Import descriptor {i}"),
            &IMPORT_DESCRIPTOR,
        );
        if let Some(name) = offset_of(&ctx, h, fields::get(&v, "Image name address")) {
            let (text, len) = utf16_at(&ctx, name);
            let nid = b.region(name, len, RegionKind::Linking, "Import name");
            b.set_value(nid, format!("L{}", util::quote(text.as_bytes())));
        }
    }
}

fn sections(b: &mut Builder) {
    let ctx = b.ctx;
    for (i, s) in ctx.sections.iter().enumerate() {
        let Some(off) = s.file_offset.filter(|_| s.file_size > 0) else {
            continue;
        };
        let loaded = s.file_size.min(s.size);
        let id = b.region(off, loaded, s.kind, format!("Section {}", s.name));
        b.set_section(id, i as u32);
        b.set_value(id, format!("VA {}..{}", hex(s.address), hex(s.address + s.size)));
        if matches!(
            s.kind,
            RegionKind::Code | RegionKind::Data | RegionKind::Rodata | RegionKind::Tls
        ) {
            b.set_decoder(id, Decoder::Symbols { address: s.address });
        }
        if s.file_size > loaded {
            let rest = off + loaded;
            let zero = ctx
                .bytes
                .slice(rest, s.file_size - loaded)
                .is_some_and(|d| d.iter().all(|&x| x == 0));
            let id = b.region(
                rest,
                s.file_size - loaded,
                if zero { RegionKind::Padding } else { s.kind },
                format!("Raw data past the virtual size ({})", s.name),
            );
            b.set_note(id, "In the file, but not loaded: the section is smaller in memory");
        }
    }
}

fn kernel_thunks(b: &mut Builder, h: &Header) {
    let ctx = b.ctx;
    let Some((_, _, table)) = h.keys() else { return };
    let Some(off) = xbe::section_offset(ctx.sections, table) else {
        return;
    };
    let thunks = xbe::kernel_thunks(&ctx.bytes, ctx.sections, table);
    let n = thunks.len() as u64;
    let ends = ctx.bytes.u32(off + 4 * n) == Some(0);
    let Some(id) = b.region(
        off,
        4 * (n + u64::from(ends)),
        RegionKind::Linking,
        "Kernel thunk table",
    ) else {
        return;
    };
    b.set_value(Some(id), format!("{n} imports from {}", xbe::KERNEL));
    b.set_note(
        Some(id),
        "Ordinals of the kernel's exports, their high bit set; the loader writes each export's address over its slot",
    );
    for (i, &(slot, ordinal)) in thunks.iter().enumerate() {
        if let Some(c) = b.child(
            id,
            off + 4 * i as u64,
            4,
            RegionKind::Linking,
            xbe::kernel_import_name(ordinal),
        ) {
            b.node_mut(c).is_field = true;
            b.set_value(
                Some(c),
                format!(
                    "ordinal {ordinal} ({:#010x}), slot {}",
                    ordinal | 0x8000_0000,
                    hex(slot)
                ),
            );
        }
    }
    if ends && let Some(c) = b.child(id, off + 4 * n, 4, RegionKind::Linking, "End of table") {
        b.node_mut(c).is_field = true;
        b.set_value(Some(c), "0");
    }
}

fn tls(b: &mut Builder, h: &Header) {
    let ctx = b.ctx;
    if h.tls == 0 {
        return;
    }
    let Some(off) = xbe::section_offset(ctx.sections, h.tls) else {
        return;
    };
    let (id, v) = b.struct_region(off, RegionKind::Tls, "TLS directory", &TLS_DIRECTORY);
    b.set_note(id, "Thread-local storage template and callbacks");
    let callbacks = fields::get(&v, "AddressOfCallBacks");
    if callbacks != 0
        && let Some(coff) = xbe::section_offset(ctx.sections, callbacks)
    {
        let n = (0..256)
            .take_while(|&i| ctx.bytes.u32(coff + 4 * i).is_some_and(|f| f != 0))
            .count() as u64;
        let id = b.region(coff, (n + 1) * 4, RegionKind::Data, "TLS callbacks");
        let mut t = Table::new(TableKind::Pointers, 4);
        t.param = callbacks;
        b.set_decoder(id, Decoder::Table(t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_structures_are_their_documented_sizes() {
        assert_eq!(fields::struct_size(&IMAGE_HEADER), IMAGE_HEADER_SIZE);
        assert_eq!(fields::struct_size(&LATER_IMAGE_HEADER), 0x184 - IMAGE_HEADER_SIZE);
        assert_eq!(fields::struct_size(&CERTIFICATE), 0x1D0);
        assert_eq!(fields::struct_size(&LATER_CERTIFICATE), 0x1EC - 0x1D0);
        assert_eq!(fields::struct_size(&SECTION_HEADER), SECTION_HEADER_SIZE);
        assert_eq!(fields::struct_size(&LIBRARY_VERSION), LIBRARY_VERSION_SIZE);
        assert_eq!(fields::struct_size(&TLS_DIRECTORY), 24);
        // The fields `xbe::Header` reads are where the image header's spec puts them.
        let offset = |name: &str| {
            let at = IMAGE_HEADER.iter().position(|f| f.name == name).unwrap();
            fields::struct_size(&IMAGE_HEADER[..at])
        };
        for (name, at) in [
            ("Base address", 0x104),
            ("Certificate address", 0x118),
            ("Entry point (encoded)", 0x128),
            ("TLS directory address", 0x12C),
            ("Debug path name address", 0x14C),
            ("Kernel thunk table address (encoded)", 0x158),
            ("Library versions address", 0x164),
            ("XAPI library version address", 0x16C),
            ("Logo bitmap size", 0x174),
        ] {
            assert_eq!(offset(name), at, "{name}");
        }
    }
}
