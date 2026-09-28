//! Tests against the binaries in tests/fixtures/bin (see scripts/build-fixtures.sh).

use std::path::PathBuf;

use binviz::{Binary, Container, RegionKind, Target};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/bin")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn open(name: &str) -> Binary {
    Binary::parse(fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Line number (1-based) of the first line of a fixture source containing `needle`.
fn source_line(file: &str, needle: &str) -> u32 {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/src")
        .join(file);
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .position(|l| l.contains(needle))
        .map(|i| i as u32 + 1)
        .unwrap()
}

const ALL: &[&str] = &[
    "tiny-elf-x64",
    "tiny-elf-x64.o",
    "tiny-elf-a64",
    "tiny-elf-a64.o",
    "tiny-elf-x64.debug",
    "tiny-elf-x64.stripped",
    "tiny-macho-a64",
    "tiny-macho-a64.o",
    "libtiny.dylib",
    "tiny-pe-x64.exe",
    "shapes-pe.exe",
    "shapes-pe.stripped.exe",
    "imports-elf-x64",
    "imports-elf-a64",
    "imports-macho-a64",
    "imports-macho-a64.chained",
];

#[test]
fn layout_covers_every_byte() {
    for name in ALL {
        let bin = open(name);
        let size = bin.data().len() as u64;
        let mut cursor = 0;
        for chunk in (0..size).step_by(4096) {
            for span in bin.spans(chunk, (chunk + 4096).min(size)) {
                assert_eq!(span.start, cursor, "{name}: gap or overlap at {cursor:#x}");
                assert!(span.end > span.start, "{name}: empty span at {cursor:#x}");
                cursor = span.end;
            }
        }
        assert_eq!(cursor, size, "{name}: spans stop early");
        let unknown: u64 = bin
            .composition()
            .iter()
            .filter(|(k, _)| *k == RegionKind::Unknown)
            .map(|(_, n)| n)
            .sum();
        assert_eq!(unknown, 0, "{name}: {unknown} bytes not explained");
        for offset in [0, size / 3, size / 2, size - 1] {
            assert!(
                !bin.describe_offset(offset).is_empty(),
                "{name}: nothing describes {offset:#x}"
            );
        }
    }
}

#[test]
fn header_fields_are_named() {
    let bin = open("tiny-elf-x64");
    let path = bin.describe_offset(0x12);
    let field = path.last().unwrap();
    assert_eq!(field.name, "e_machine");
    assert!(field.value.as_deref().unwrap().starts_with("EM_X86_64"));

    let bin = open("tiny-macho-a64");
    let path = bin.describe_offset(4);
    assert_eq!(path.last().unwrap().name, "cputype");
    assert!(
        path.last()
            .unwrap()
            .value
            .as_deref()
            .unwrap()
            .contains("CPU_TYPE_ARM64")
    );

    let bin = open("shapes-pe.exe");
    let path = bin.describe_offset(0x3c);
    assert_eq!(path.last().unwrap().name, "e_lfanew");
}

#[test]
fn table_entries_decode_on_demand() {
    let bin = open("tiny-elf-x64");
    let symtab = bin.sections().iter().find(|s| s.name == ".symtab").unwrap();
    // Entry 2 of the 24-byte Elf64_Sym table, at its st_value field.
    let offset = symtab.file_offset.unwrap() + 2 * 24 + 8;
    let path = bin.describe_offset(offset);
    let entry = &path[path.len() - 2];
    assert!(entry.name.starts_with("Symbol 2: tiny::"), "{}", entry.name);
    assert_eq!(path.last().unwrap().name, "st_value");
}

#[test]
fn dwarf_sections_decode_to_attributes() {
    for name in ["tiny-elf-x64", "tiny-macho-a64.o", "shapes-pe.exe"] {
        let bin = open(name);
        let info = bin
            .sections()
            .iter()
            .find(|s| s.name.trim_start_matches('.').trim_start_matches("__") == "debug_info")
            .unwrap();
        let start = info.file_offset.unwrap();
        let path = bin.describe_offset(start);
        assert_eq!(path[path.len() - 2].name, "Unit 0 header", "{name}");
        assert_eq!(path.last().unwrap().name, "unit_length", "{name}");
        // The first byte after the header is the root DIE's abbreviation code.
        let header = path[path.len() - 2].end;
        let path = bin.describe_offset(header);
        assert!(
            path[path.len() - 2].name.contains("compile_unit"),
            "{name}: {:?}",
            path[path.len() - 2].name
        );
        assert_eq!(path.last().unwrap().name, "abbrev code", "{name}");
        // Every byte of .debug_info belongs to a unit header or a DIE.
        for off in (start..start + info.file_size).step_by(7) {
            let p = bin.describe_offset(off);
            let entry = &p[p.len() - 2];
            assert!(
                entry.name.starts_with("Unit ") || entry.name.starts_with("DIE ") || entry.name == "Null entry",
                "{name} {off:#x}: {}",
                entry.name
            );
        }
    }
}

#[test]
fn elf_basics() {
    let bin = open("tiny-elf-x64");
    let s = bin.summary();
    assert_eq!(s.format_name, "ELF64");
    assert_eq!(s.arch, "x86-64");
    assert!(s.has_dwarf);
    let entry = s.entry.unwrap();
    let start = bin.symbols().by_name("_start").unwrap();
    assert_eq!(start.address, entry);
    let run = bin.symbols().by_name("run").expect("suffix lookup");
    assert_eq!(run.demangled().as_deref(), Some("tiny::run"));
    let hit = bin.symbols().lookup(run.address + 4).unwrap();
    assert_eq!(hit.index, run.index);
    assert_eq!(hit.offset, 4);
    assert_eq!(
        bin.address_to_offset(entry).and_then(|o| bin.offset_to_address(o)),
        Some(entry)
    );
}

/// Source span of a function: from its `fn` line to the next line that is just `}`.
fn function_span(needle: &str) -> (u32, u32) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/src/tiny.rs");
    let text = std::fs::read_to_string(path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| l.contains(needle)).unwrap();
    let end = start + lines[start..].iter().position(|l| *l == "}").unwrap();
    (start as u32 + 1, end as u32 + 1)
}

#[test]
fn dwarf_maps_every_instruction_to_its_function() {
    for name in ["tiny-elf-x64", "tiny-elf-a64", "tiny-macho-a64.o", "tiny-pe-x64.exe"] {
        let bin = open(name);
        let debug = bin.debug_info().unwrap_or_else(|| panic!("{name}: no DWARF"));
        for (func, needle) in [
            ("checksum", "fn checksum"),
            ("tiny::fib", "fn fib"),
            ("tiny::run", "fn run"),
        ] {
            let sym = bin
                .symbols()
                .by_name(func)
                .unwrap_or_else(|| panic!("{name}: no {func}"));
            let (first, last) = function_span(needle);
            let code = bin.disassemble_function(sym.address, 10_000);
            assert!(!code.instructions.is_empty(), "{name}: {func} has no code");
            for ins in &code.instructions {
                // The outermost frame is the function itself, positioned inside its body
                // (inlined callees appear as inner frames).
                let frames = debug.frames(ins.address);
                let outer = frames
                    .last()
                    .unwrap_or_else(|| panic!("{name}: no frames at {:#x}", ins.address));
                let fname = outer.demangled.as_deref().or(outer.function.as_deref()).unwrap_or("");
                assert!(
                    fname.ends_with(func.trim_start_matches("tiny::")),
                    "{name}: {fname} at {:#x}",
                    ins.address
                );
                let line = outer.line.unwrap_or(0);
                assert!(
                    line == 0 || (first..=last).contains(&line),
                    "{name}: {func} line {line} outside {first}..={last}"
                );
            }
        }
        let fib = debug
            .search("fib", 10)
            .into_iter()
            .find(|d| d.tag == "DW_TAG_subprogram")
            .unwrap();
        let decl = debug.die(fib.unit, fib.offset).unwrap().decl.unwrap();
        assert_eq!(decl.line, source_line("tiny.rs", "fn fib"), "{name}: decl_line");
    }
}

#[test]
fn relocatable_objects_get_relocated_dwarf() {
    let obj = open("tiny-elf-x64.o");
    let exe = open("tiny-elf-x64");
    assert!(obj.summary().synthetic_addresses);
    let (od, ed) = (obj.debug_info().unwrap(), exe.debug_info().unwrap());
    let mut addresses = Vec::new();
    for func in ["checksum", "tiny::fib", "tiny::run"] {
        let (os, es) = (
            obj.symbols().by_name(func).unwrap(),
            exe.symbols().by_name(func).unwrap(),
        );
        addresses.push(os.address);
        // Same code, so the object's relocated line table must agree with the executable's.
        for delta in (0..os.size.min(es.size)).step_by(3) {
            let (ol, el) = (od.location(os.address + delta), ed.location(es.address + delta));
            assert_eq!(
                ol.map(|l| (l.line, l.column)),
                el.map(|l| (l.line, l.column)),
                "{func}+{delta:#x}"
            );
        }
    }
    addresses.dedup();
    assert_eq!(addresses.len(), 3, "sections must get distinct synthetic addresses");
}

#[test]
fn inlined_frames() {
    let bin = open("tiny-elf-x64");
    let debug = bin.debug_info().unwrap();
    let run = bin.symbols().by_name("tiny::run").unwrap();
    let dot_line = source_line("tiny.rs", "a.x * b.x");
    let found = (run.address..run.address + run.size).find_map(|a| {
        let frames = debug.frames(a);
        (frames.len() >= 2 && frames[0].demangled.as_deref() == Some("tiny::dot")).then_some(frames)
    });
    let frames = found.expect("dot() is inlined into run()");
    assert!(frames[0].inlined);
    assert_eq!(frames[0].line, Some(dot_line));
    assert_eq!(frames.last().unwrap().demangled.as_deref(), Some("tiny::run"));
}

#[test]
fn source_to_address_and_back() {
    let bin = open("shapes-pe.exe");
    let debug = bin.debug_info().unwrap();
    let file = debug
        .source_files()
        .iter()
        .find(|f| f.path.ends_with("shapes.cpp"))
        .unwrap();
    let line = source_line("shapes.cpp", "++g_counter");
    let ranges: Vec<_> = debug
        .file_lines(file.id)
        .into_iter()
        .filter(|l| l.line == line)
        .collect();
    assert!(!ranges.is_empty());
    for r in ranges {
        let back = debug.location(r.start).unwrap();
        assert_eq!((back.file, back.line), (file.id, line));
        let ins = bin.inspect(Target::Address(r.start));
        assert_eq!(ins.source.as_ref().map(|s| s.line), Some(line));
        assert!(ins.symbol.unwrap().demangled.unwrap().starts_with("total_area"));
    }
}

#[test]
fn dies_and_types() {
    let bin = open("shapes-pe.exe");
    let debug = bin.debug_info().unwrap();
    let found = debug.search("total_area", 10);
    let die = found
        .iter()
        .find(|d| d.tag == "DW_TAG_subprogram")
        .expect("subprogram DIE");
    let details = debug.die(die.unit, die.offset).unwrap();
    assert_eq!(details.type_name.as_deref(), Some("double"));
    let children = debug.die_children(die.unit, Some(die.offset));
    let param = children.iter().find(|c| c.name.as_deref() == Some("shapes")).unwrap();
    assert_eq!(
        param.detail.as_deref(),
        Some("const vector<geo::Shape*, std::allocator<geo::Shape*> > &")
    );
}

#[test]
fn split_debug_file() {
    let mut bin = open("tiny-elf-x64.stripped");
    assert!(bin.debug_info().is_none());
    assert!(
        bin.summary()
            .debug_link
            .as_deref()
            .unwrap()
            .starts_with("tiny-elf-x64.debug")
    );
    bin.attach_debug_file("tiny-elf-x64.debug", fixture("tiny-elf-x64.debug"))
        .unwrap();
    let run = bin.symbols().by_name("tiny::run").unwrap();
    assert_eq!(
        bin.debug_info().unwrap().location(run.address).unwrap().line,
        source_line("tiny.rs", "fn run")
    );

    let mut other = open("tiny-elf-x64.stripped");
    assert!(
        other
            .attach_debug_file("tiny-elf-a64", fixture("tiny-elf-a64"))
            .is_err(),
        "architecture mismatch"
    );
}

#[test]
fn disassembly() {
    let bin = open("tiny-elf-x64");
    let run = bin.symbols().by_name("tiny::run").unwrap();
    let d = bin.disassemble_function(run.address, 1000);
    assert!(d.supported);
    assert_eq!(d.instructions.first().unwrap().mnemonic, "push");
    assert_eq!(d.instructions.last().unwrap().mnemonic, "ret");
    let call = d
        .instructions
        .iter()
        .find(|i| i.target_symbol.as_deref() == Some("tiny::fib"))
        .expect("call to fib");
    assert_eq!(call.flow, binviz::FlowKind::Call);

    let bin = open("tiny-macho-a64");
    let run = bin.symbols().by_name("tiny::run").unwrap();
    let d = bin.disassemble_function(run.address, 1000);
    assert!(d.instructions.first().unwrap().mnemonic.starts_with("stp"));
    assert!(
        d.instructions
            .iter()
            .any(|i| i.target_symbol.as_deref() == Some("tiny::fib"))
    );

    let bin = open("tiny-macho-a64");
    let entry = bin.summary().entry.unwrap();
    assert_eq!(bin.symbols().lookup(entry).unwrap().name, "__start");
}

#[test]
fn pe_imports_have_iat_slots() {
    let bin = open("shapes-pe.exe");
    let imp = bin.imports().iter().find(|i| i.name == "GetLastError").unwrap();
    let slot = imp.address.unwrap();
    let sym = bin.symbols().lookup(slot).unwrap();
    assert_eq!(sym.name, "__imp_GetLastError");
    let path = bin.describe_offset(bin.address_to_offset(slot).unwrap());
    assert!(
        path.iter().any(|p| p.name.starts_with("Import address table")),
        "{path:?}"
    );
}

/// Builds a minimal System V `ar` archive.
fn ar(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"!<arch>\n".to_vec();
    for (name, data) in members {
        let header = format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            format!("{name}/"),
            0,
            0,
            0,
            644,
            data.len()
        );
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(b'\n');
        }
    }
    out
}

#[test]
fn archives_are_containers() {
    let data = ar(&[("a.o", fixture("tiny-elf-x64.o")), ("b.o", fixture("tiny-elf-a64.o"))]);
    assert!(Binary::parse(data.clone()).is_err());
    let c = Container::parse(data).unwrap();
    assert_eq!(c.members().len(), 2);
    let b = c.open(1).unwrap();
    assert_eq!(b.summary().arch, "AArch64");
    assert!(b.debug_info().is_some());
}

// --- Search, attribution, reverse-engineering coverage ---------------------------

use binviz::coverage::StatusBytes;
use binviz::{Annotation, AttributionMode, HitKind, SymbolSource};

fn hits(bin: &Binary, query: &str, kind: HitKind) -> Vec<binviz::SearchHit> {
    bin.search(query, 20, None)
        .hits
        .into_iter()
        .filter(|h| h.kind == kind)
        .collect()
}

#[test]
fn search_finds_every_kind_of_thing() {
    let bin = open("shapes-pe.exe");
    let main = bin.symbols().by_name("main").unwrap().address;

    // Names rank exact / qualified matches first.
    let area = hits(&bin, "area", HitKind::Symbol);
    assert!(
        area[0].label.starts_with("geo::") && area[0].label.contains("::area()"),
        "{:?}",
        area[0].label
    );
    assert!(area.iter().any(|h| h.label.starts_with("total_area")));
    assert!(!hits(&bin, "area", HitKind::Dwarf).is_empty());

    // Numbers are addresses (and offsets when they fit in the file).
    let at = hits(&bin, &format!("{main:#x}"), HitKind::Address);
    assert_eq!(at[0].address, Some(main));
    assert!(at[0].detail.contains("main"), "{}", at[0].detail);
    assert_eq!(hits(&bin, "@0x40", HitKind::Offset)[0].offset, Some(0x40));
    assert_eq!(hits(&bin, "main+0x10", HitKind::Address)[0].address, Some(main + 0x10));

    // Byte patterns, with wildcards, and exact text.
    let prologues = bin.search("55 48 89 e5", 5, Some(HitKind::Bytes));
    assert!(prologues.counts[0].count > 10);
    let wild = bin.search("55 48 ?? e5", 5, Some(HitKind::Bytes));
    assert!(wild.counts[0].count >= prologues.counts[0].count);
    assert!(!hits(&bin, "\"Circle\"", HitKind::Bytes).is_empty());

    // Source lines, strings, globals.
    let line = source_line("shapes.cpp", "int main");
    let src = hits(&bin, &format!("shapes.cpp:{line}"), HitKind::Source);
    assert!(src[0].address.is_some() && src[0].line.is_some(), "{src:?}");
    assert!(
        hits(&bin, "circle", HitKind::String)
            .iter()
            .any(|h| h.label == "circle")
    );
    assert!(!hits(&bin, "g_counter", HitKind::Symbol).is_empty());
    assert!(hits(&bin, "g_counter", HitKind::Dwarf)[0].address.is_some());
}

#[test]
fn stripped_binaries_recover_functions() {
    let bin = open("shapes-pe.stripped.exe");
    assert!(bin.debug_info().is_none());
    let recovered = bin
        .symbols()
        .functions()
        .filter(|s| s.source == SymbolSource::Discovered)
        .count();
    assert!(recovered > 400, "{recovered} functions from .pdata");
    // Every function the unstripped build knows about is found again.
    let full = open("shapes-pe.exe");
    let main = full.symbols().by_name("main").unwrap();
    let sub = bin.symbols().at(main.address).expect("main recovered");
    assert_eq!(sub.name(), format!("sub_{:x}", main.address));
    let cov = bin.coverage(10);
    let text = cov.sections.iter().find(|s| s.name == ".text").unwrap();
    assert!(text.bytes.recovered * 10 > text.size * 9, "{:?}", text.bytes);
    // The only names left in the code are the import thunks' (`jmp [__imp_…]`).
    let thunks: u64 = bin
        .symbols()
        .functions()
        .filter(|s| s.source == SymbolSource::Import)
        .map(|s| s.size)
        .sum();
    assert!(thunks > 0);
    assert_eq!(text.bytes.named, thunks);
}

fn total(b: &StatusBytes) -> u64 {
    b.reviewed + b.annotated + b.named + b.structure + b.recovered + b.padding + b.unexplored
}

#[test]
fn coverage_accounts_for_every_byte() {
    for name in [
        "shapes-pe.exe",
        "shapes-pe.stripped.exe",
        "tiny-elf-x64",
        "tiny-elf-a64",
        "tiny-macho-a64",
    ] {
        let bin = open(name);
        let cov = bin.coverage(5);
        for s in &cov.sections {
            assert_eq!(total(&s.bytes), s.size, "{name} {}", s.name);
            let strip = bin.coverage_strip(s.section, 64);
            assert_eq!(strip.len() as u64, s.size.min(64), "{name} {}", s.name);
        }
        let map = bin.coverage_map(256);
        assert_eq!(map.len() as u64, (bin.data().len() as u64).min(256));
        assert!(
            cov.gaps
                .windows(2)
                .all(|w| w[0].end - w[0].start >= w[1].end - w[1].start)
        );
    }
    // A binary with symbols is essentially all named or padding.
    let cov = open("shapes-pe.exe").coverage(5);
    assert!(cov.totals.named * 100 > total(&cov.totals) * 95, "{:?}", cov.totals);
}

#[test]
fn annotations_name_functions_and_mark_progress() {
    let mut bin = open("shapes-pe.stripped.exe");
    let full = open("shapes-pe.exe");
    let main = full.symbols().by_name("main").unwrap().to_symbol();
    let before = bin.coverage(0);
    bin.set_annotations(vec![
        Annotation {
            address: main.address,
            size: 0,
            name: "main".into(),
            comment: "builds shapes and prints the total area".into(),
            reviewed: true,
        },
        Annotation {
            address: main.address + 0x10,
            size: 0,
            name: String::new(),
            comment: "calls __main".into(),
            reviewed: false,
        },
    ]);
    // The name becomes a symbol with the recovered function's exact size.
    let sym = bin.symbols().by_name("main").unwrap();
    assert_eq!((sym.address, sym.source), (main.address, SymbolSource::User));
    let recovered = bin
        .symbols()
        .iter()
        .find(|s| s.name() == format!("sub_{:x}", main.address))
        .unwrap();
    assert_eq!(sym.size, recovered.size);
    assert!(!sym.size_inferred);
    let lookup = bin.symbols().lookup(main.address + 4).unwrap();
    assert_eq!(lookup.name, "main");

    let after = bin.coverage(0);
    assert_eq!(after.reviewed, 1);
    assert_eq!(after.functions.user, 1);
    assert!(after.totals.reviewed >= sym.size - 16, "{:?}", after.totals);
    assert!(after.totals.recovered < before.totals.recovered);

    // The innermost note wins when inspecting.
    let inner = bin.inspect(Target::Address(main.address + 0x10));
    assert_eq!(inner.annotation.as_ref().unwrap().comment, "calls __main");
    let outer = bin.inspect(Target::Address(main.address + 0x40));
    assert!(outer.annotation.unwrap().reviewed);
    // Notes are searchable by comment, names by name.
    assert!(!hits(&bin, "shapes and prints", HitKind::Note).is_empty());
    assert_eq!(hits(&bin, "main", HitKind::Symbol)[0].address, Some(main.address));
}

#[test]
fn attribution_places_files_in_sections() {
    let bin = open("shapes-pe.exe");
    let attr = bin.attribution(AttributionMode::File).unwrap();
    let shapes = attr.contributors.iter().find(|c| c.name == "shapes.cpp").unwrap();
    let section = |i: u32| bin.sections()[i as usize].name.as_str();
    assert!(shapes.code > 0 && shapes.functions >= 5, "{shapes:?}");
    // g_counter lives in .bss.
    assert!(
        shapes
            .sections
            .iter()
            .any(|s| section(s.section) == ".bss" && s.data >= 4)
    );
    let ranges = bin.attributed_ranges(AttributionMode::File, shapes.id);
    let main = bin.symbols().by_name("main").unwrap();
    // Code is split at function boundaries, so main's own code starts at main.
    let first = ranges.iter().find(|r| r.label.as_deref() == Some("main")).unwrap();
    assert_eq!(first.start, main.address);
    for r in ranges.iter().filter(|r| !r.data) {
        let f = bin.symbols().lookup(r.start).unwrap();
        assert!(r.end <= f.address + f.size.max(1), "{r:?} spills out of {}", f.name);
    }
    assert!(ranges.iter().any(|r| r.data && r.label.as_deref() == Some("g_counter")));
    let units = bin.attribution(AttributionMode::Unit).unwrap();
    assert!(units.contributors.iter().any(|c| c.path.ends_with("shapes.cpp")));
}

#[test]
fn strings_in_data_sections() {
    let bin = open("shapes-pe.exe");
    let page = bin.strings("circle", 0, 50);
    assert!(page.strings.iter().any(|s| s.text == "circle" && s.address.is_some()));
    let all = bin.strings("", 0, 10);
    assert!(all.total > 100 && all.strings.len() == 10);
}

#[test]
fn callers_callees_and_the_data_a_function_uses() {
    let bin = open("shapes-pe.exe");
    let main = bin.symbols().by_name("main").unwrap().address;
    let total_area = bin.symbols().by_name("total_area").unwrap().address;
    let callers = bin.callers(total_area);
    assert!(
        callers.iter().any(|c| c.address == main && c.name == "main"),
        "{callers:?}"
    );
    assert!(bin.callees(main).iter().any(|c| c.address == total_area));

    let f = bin.function_summary(main, 50).unwrap();
    let format = f
        .strings
        .iter()
        .find(|s| s.text.starts_with("%s: %d shapes"))
        .unwrap_or_else(|| panic!("{:?}", f.strings));
    assert!(
        f.data.iter().any(|r| r.to.as_deref() == Some("g_counter")),
        "{:?}",
        f.data
    );
    let refs = bin.references_to(format.address, format.address + 1, 0, 10);
    assert_eq!((refs.total, refs.refs[0].function), (1, Some(main)));
    assert_eq!(refs.refs[0].source, format.site);

    // From the entry point down to total_area, through main.
    let entry = bin.summary().entry.unwrap();
    let path = bin.call_path(entry, total_area, 8).expect("a path");
    assert_eq!(path.first().map(|s| s.address), Some(entry));
    assert!(path.iter().any(|s| s.address == main), "{path:?}");
    let last = path.last().unwrap();
    assert_eq!(last.address, total_area);
    assert!(
        bin.references_to(total_area, total_area + 1, 0, 10)
            .refs
            .iter()
            .any(|r| Some(r.source) == last.site)
    );

    let g = bin.call_graph(main, 1, 1, 20);
    assert!(g.nodes.iter().any(|n| n.address == total_area && n.depth == 1));
    assert!(g.nodes.iter().any(|n| n.depth == -1));
    assert!(g.edges.iter().any(|e| e.from == main && e.to == total_area));

    // Stripped, the same edges connect recovered functions, and calls into
    // the C runtime go through named import thunks.
    let stripped = open("shapes-pe.stripped.exe");
    assert!(stripped.callers(total_area).iter().any(|c| c.address == main));
    let imports: Vec<String> = stripped
        .callees(main)
        .iter()
        .flat_map(|c| stripped.callees(c.address))
        .filter(|c| c.kind == binviz::NodeKind::Import)
        .map(|c| c.name.clone())
        .collect();
    assert!(!imports.is_empty(), "no calls into imports below main");
}

#[test]
fn aarch64_calls() {
    for name in ["tiny-elf-a64", "tiny-macho-a64", "libtiny.dylib"] {
        let bin = open(name);
        let run = bin.symbols().by_name("tiny::run").unwrap().address;
        let callees: Vec<String> = bin.callees(run).into_iter().map(|c| c.name).collect();
        assert!(callees.iter().any(|c| c == "tiny::fib"), "{name}: {callees:?}");
        assert!(callees.iter().any(|c| c.ends_with("checksum")), "{name}: {callees:?}");
        let fib = bin.symbols().by_name("tiny::fib").unwrap().address;
        assert!(bin.callers(fib).iter().any(|c| c.address == run), "{name}");
        // adr of a static: an address reference.
        let points = bin.symbols().by_name("tiny::POINTS").unwrap();
        let refs = bin.references_to(points.address, points.address + points.size, 0, 10);
        assert!(refs.refs.iter().any(|r| r.function == Some(run)), "{name}: {refs:?}");
    }
}

#[test]
fn imports_and_pointers_in_data() {
    for name in [
        "imports-elf-x64",
        "imports-elf-a64",
        "imports-macho-a64",
        "imports-macho-a64.chained",
    ] {
        let bin = open(name);
        let main = bin.symbols().by_name("main").unwrap().address;
        // PLT entries, Mach-O stubs, or (x86-64 without a PLT) the GOT slot itself.
        let callees = bin.callees(main);
        assert!(
            callees
                .iter()
                .any(|c| c.kind == binviz::NodeKind::Import && c.name.contains("puts")),
            "{name}: {callees:?}"
        );
        // The loader relocates the pointers in MESSAGES: RELA, RELR, dyld info, chained fixups.
        let messages = bin.symbols().by_name("imports::MESSAGES").unwrap();
        let pointers: Vec<_> = bin
            .references_from(messages.address, messages.address + messages.size)
            .into_iter()
            .filter(|r| r.kind == binviz::RefKind::Pointer)
            .collect();
        let texts: Vec<&str> = pointers.iter().map(|r| r.to.as_deref().unwrap_or("")).collect();
        assert_eq!(
            texts,
            ["\"hello from binviz\"", "\"goodbye\"", "\"unused message\""],
            "{name}"
        );
        let hello = pointers[0].target;
        let to = bin.references_to(hello, hello + 1, 0, 10);
        assert_eq!(to.counts.pointer, 1, "{name}: {:?}", to.counts);
        assert!(to.counts.address >= 1, "{name}: {:?}", to.counts);
        assert_eq!(bin.pointer_at(messages.address), Some(hello), "{name}");
    }
}

#[test]
fn dwarf_problems_are_found_and_placed() {
    let clean = open("shapes-pe.exe");
    let d = clean.debug_info().unwrap();
    let c = d.check();
    assert_eq!((c.errors, c.warnings), (0, 0), "{:?}", c.problems);
    assert!(c.dies > 2000 && c.line_rows > 500);

    // Pick a type reference and a string in unit 0 to break.
    let mut type_ref = None;
    let mut name = None;
    for s in d.list_dies(0, "", "", 0, 100_000).dies {
        let det = d.die(0, s.offset).unwrap();
        for a in &det.attributes {
            if type_ref.is_none() && a.name == "DW_AT_type" && a.form == "DW_FORM_ref4" {
                type_ref = Some((s.offset, a.byte_start));
            } else if name.is_none()
                && type_ref.is_some_and(|(o, _)| o != s.offset)
                && a.name == "DW_AT_name"
                && a.form == "DW_FORM_strp"
            {
                name = Some((s.offset, a.byte_start));
            }
        }
    }
    let (type_die, type_at) = type_ref.expect("a DW_FORM_ref4 type");
    let (name_die, name_at) = name.expect("a DW_FORM_strp name");
    let last = d.units().last().unwrap().clone();
    assert_eq!(last.version, 5);

    let info = clean
        .sections()
        .iter()
        .find(|s| s.name == ".debug_info")
        .and_then(|s| s.file_offset)
        .unwrap();
    let mut data = fixture("shapes-pe.exe");
    let mut put = |at: u64, v: u32| {
        let i = (info + at) as usize;
        data[i..i + 4].copy_from_slice(&v.to_le_bytes());
    };
    put(type_at, 0x00ff_fff0);
    put(name_at, 0xffff_fff0);
    // DWARF 5 unit header: length (4), version (2), unit type (1), address size (1), abbreviation offset.
    put(last.offset + 8, 0xffff_ff00);

    let bin = Binary::parse(data).unwrap();
    let d = bin.debug_info().unwrap();
    assert_eq!(d.units().len(), 4, "the unit with no abbreviations can't be read");
    assert_eq!(d.load_problems().len(), 1);
    let c = d.check();
    let found = |area: &str, die: Option<u64>| {
        c.problems
            .iter()
            .any(|p| p.area == area && (die.is_none() || (p.unit == Some(0) && p.die == die)))
    };
    assert!(found("reference", Some(type_die)), "{:#?}", c.problems);
    assert!(found("string", Some(name_die)), "{:#?}", c.problems);
    let unit = c
        .problems
        .iter()
        .find(|p| p.area == "unit")
        .expect("the unreadable unit");
    assert_eq!(unit.offset, Some(last.offset));
    assert!(c.errors >= 3);
    // The rest still reads: DIEs, the layout of every byte, and the other units.
    assert!(d.die(0, name_die).is_some());
    let size = bin.data().len() as u64;
    assert_eq!(bin.spans(0, size).last().map(|s| s.end), Some(size));
}

#[test]
fn dies_by_kind_offset_scope_and_layout() {
    let bin = open("shapes-pe.exe");
    let d = bin.debug_info().unwrap();
    // A unit's classes, with their namespaces.
    let rect = d.list_dies(0, "types", "Rect", 0, 10);
    let class = rect
        .dies
        .iter()
        .find(|x| x.tag == "DW_TAG_class_type")
        .expect("class Rect");
    assert_eq!(class.scope.as_deref(), Some("geo"));
    // Its layout: the vtable pointer (from the base class), then two points.
    let det = d.die(0, class.offset).unwrap();
    let layout: Vec<(String, Option<u64>)> = det
        .layout
        .iter()
        .map(|m| (m.name.clone().unwrap_or_else(|| m.type_name.clone()), m.offset))
        .collect();
    assert_eq!(
        layout,
        [
            ("Shape".to_string(), Some(0)),
            ("min".into(), Some(8)),
            ("max".into(), Some(24))
        ]
    );
    assert_eq!((det.byte_size, det.tail_padding), (Some(40), Some(0)));
    // From a .debug_info offset (as llvm-dwarfdump prints them) back to the DIE.
    assert_eq!(d.die_at_offset(class.section_offset + 1), Some((0, class.offset)));
    // Search ranks the functions named "area" first, with their classes.
    let hits = d.search("area", 10);
    assert_eq!(hits[0].name.as_deref(), Some("area"));
    assert_eq!(hits[0].scope.as_deref(), Some("geo::Rect"));
    assert!(hits.iter().any(|h| h.name.as_deref() == Some("total_area")));
    // An inlined constructor knows where it was inlined, and which lines its code came from.
    let inlined = d.list_dies(0, "inlined_subroutine", "Circle", 0, 10);
    let circle = inlined.dies.first().expect("an inlined Circle");
    assert_eq!(circle.scope.as_deref(), Some("main"));
    let det = d.die(0, circle.offset).unwrap();
    let main_line = source_line("shapes.cpp", "geo::Circle c(");
    assert_eq!(det.call_site.as_ref().map(|c| c.line), Some(main_line));
    assert!(
        det.code_lines
            .iter()
            .any(|l| l.path.ends_with("shapes.cpp") && l.bytes > 0)
    );
    // At total_area's entry, its parameter is in a register.
    let total = bin.symbols().by_name("total_area").unwrap().address;
    let scope = d.scope_at(total).unwrap();
    assert_eq!(scope.scopes[0].name.as_deref(), Some("total_area"));
    let shapes = scope.variables.iter().find(|v| v.name == "shapes").unwrap();
    assert_eq!(shapes.kind, "parameter");
    assert!(shapes.location.contains("DW_OP_reg"), "{}", shapes.location);
    assert!(shapes.type_name.as_deref().unwrap_or("").contains("vector"));
}

#[test]
fn stripped_binaries_take_names_from_their_debug_file() {
    let mut bin = open("tiny-elf-x64.stripped-all");
    assert!(bin.symbols().by_name("tiny::fib").is_none());
    bin.attach_debug_file("tiny-elf-x64.debug", fixture("tiny-elf-x64.debug"))
        .unwrap();
    // Functions and data, from the debug file's symbol table.
    let fib = bin.symbols().by_name("tiny::fib").expect("tiny::fib named");
    assert_eq!(fib.source, SymbolSource::DebugFile);
    let points = bin.symbols().by_name("tiny::POINTS").expect("data named too");
    assert_eq!(points.kind, binviz::SymbolKind::Data);
}

/// A small app from the fixtures, as a folder: an app, a framework, an extension,
/// resources (one image twice), and a "dSYM" whose UUID matches the app.
fn package_files() -> Vec<(String, Vec<u8>)> {
    let plist = |exe: &str, id: &str| {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
             <key>CFBundleExecutable</key><string>{exe}</string>\
             <key>CFBundleIdentifier</key><string>{id}</string>\
             <key>CFBundleShortVersionString</key><string>4.2</string>\
             <key>MinimumOSVersion</key><string>15.0</string></dict></plist>"
        )
        .into_bytes()
    };
    // A stripped app; its unstripped copy, marked MH_DSYM, stands in for its dSYM (same UUID).
    let main = fixture("imports-macho-a64.chained.stripped");
    let mut dsym = fixture("imports-macho-a64.chained");
    dsym[12..16].copy_from_slice(&10u32.to_le_bytes()); // MH_DSYM
    let icon: Vec<u8> = (0..3000u32).map(|i| (i * 7 % 251) as u8).collect();
    vec![
        ("Shop.app/ShopApp".into(), main),
        ("Shop.app/Info.plist".into(), plist("ShopApp", "com.example.shop")),
        (
            "Shop.app/Frameworks/Tiny.framework/Tiny".into(),
            fixture("libtiny.dylib"),
        ),
        (
            "Shop.app/Frameworks/Tiny.framework/Info.plist".into(),
            plist("Tiny", "com.example.tiny"),
        ),
        ("Shop.app/PlugIns/Widget.appex/Widget".into(), fixture("tiny-macho-a64")),
        (
            "Shop.app/PlugIns/Widget.appex/Info.plist".into(),
            plist("Widget", "com.example.widget"),
        ),
        ("Shop.app/Assets.car".into(), vec![7; 20_000]),
        ("Shop.app/icon.png".into(), icon.clone()),
        ("Shop.app/Bundle.bundle/icon-copy.png".into(), icon),
        (
            "Shop.app/en.lproj/Localizable.strings".into(),
            b"\"a\" = \"b\";\n".repeat(80),
        ),
        ("Shop.app.dSYM/Contents/Resources/DWARF/ShopApp".into(), dsym),
    ]
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A stored (uncompressed) zip of `files`, each under `prefix`.
fn zip_of(files: &[(String, Vec<u8>)], prefix: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut cd = Vec::new();
    for (name, data) in files {
        let name = format!("{prefix}{name}");
        let (offset, crc, len) = (out.len() as u32, crc32(data), data.len() as u32);
        let header = |sig: u32, central: bool, buf: &mut Vec<u8>| {
            buf.extend_from_slice(&sig.to_le_bytes());
            if central {
                buf.extend_from_slice(&[20, 3]);
            }
            buf.extend_from_slice(&[20, 0, 0, 8, 0, 0, 0, 0, 0, 0]);
            buf.extend_from_slice(&crc.to_le_bytes());
            buf.extend_from_slice(&len.to_le_bytes());
            buf.extend_from_slice(&len.to_le_bytes());
            buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            if central {
                buf.extend_from_slice(&[0; 6]);
                buf.extend_from_slice(&(0o100644u32 << 16).to_le_bytes());
                buf.extend_from_slice(&offset.to_le_bytes());
            }
            buf.extend_from_slice(name.as_bytes());
        };
        header(0x0403_4b50, false, &mut out);
        out.extend_from_slice(data);
        header(0x0201_4b50, true, &mut cd);
    }
    let cd_offset = out.len() as u32;
    let n = files.len() as u16;
    out.extend_from_slice(&cd);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&(cd.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn check_package(info: &binviz::package::PackageInfo, zipped: bool) {
    use binviz::package::{BinaryKind, FileCategory};
    let found: Vec<(&str, BinaryKind)> = info.binaries.iter().map(|b| (b.name.as_str(), b.kind)).collect();
    // Executables first, the app's own leading; its extension is an executable too.
    assert_eq!(
        found,
        [
            ("ShopApp", BinaryKind::Executable),
            ("Widget", BinaryKind::Executable),
            ("Tiny", BinaryKind::Library)
        ],
        "{}",
        info.kind
    );
    let app = info.binaries[0].bundle.as_ref().expect("the app's Info.plist");
    assert_eq!(app.bundle_id.as_deref(), Some("com.example.shop"));
    assert_eq!(app.min_os.as_deref(), Some("15.0"));
    // The dSYM pairs with the app by UUID, not by name.
    assert_eq!(info.binaries[0].debug, Some(0));
    assert_eq!(info.debug_files[0].binary, Some(0));
    assert!(info.binaries[1].debug.is_none() && info.binaries[2].debug.is_none());
    let size = |c: FileCategory| info.categories.iter().find(|x| x.category == c).map(|x| x.size);
    assert_eq!(size(FileCategory::AssetCatalogs), Some(20_000));
    assert_eq!(size(FileCategory::Images), Some(6000));
    assert!(size(FileCategory::Localization).is_some());
    assert_eq!(info.debug_size, info.debug_files[0].size);
    // Only a zip's directory has checksums to find duplicates by.
    if zipped {
        assert_eq!(info.duplicate_bytes, 3000);
        assert_eq!(info.duplicates[0].paths.len(), 2);
    }
}

#[test]
fn binaries_in_folders_and_zips() {
    use binviz::package::{BinaryKind, DiskPackage, load_binary};
    let files = package_files();
    let dir = std::env::temp_dir().join(format!("binviz-package-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let write = |name: &str, data: &[u8]| {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, data).unwrap();
        path
    };
    for (name, data) in &files {
        write(&format!("app/{name}"), data);
    }
    // A folder holding the .app and its dSYM.
    let mut pkg = DiskPackage::open(&dir.join("app")).unwrap();
    assert_eq!(pkg.info.kind, "folder");
    check_package(&pkg.info, false);
    // The app binary loads, and its dSYM attaches (this one has symbols, no
    // DWARF): the names stripped from the app come back.
    let main = pkg.info.binaries[0].clone();
    let (mut bin, _) = load_binary(pkg.read_shared(main.file).unwrap()).unwrap();
    assert!(bin.symbols().by_name("imports::MESSAGES").is_none());
    let dsym = pkg.info.debug_files[0].clone();
    bin.attach_debug_file(&dsym.path, pkg.read_shared(dsym.file).unwrap())
        .unwrap();
    let messages = bin.symbols().by_name("imports::MESSAGES").expect("named by the dSYM");
    assert_eq!(messages.source, SymbolSource::DebugFile);

    // The same files zipped.
    let pkg = DiskPackage::open(&write("Shop.zip", &zip_of(&files, ""))).unwrap();
    assert_eq!(pkg.info.kind, "zip");
    check_package(&pkg.info, true);
    assert!(pkg.info.compressed_size.is_some());

    // An .ipa is a zip with the app under Payload/, and no dSYM.
    let app: Vec<(String, Vec<u8>)> = files
        .iter()
        .filter(|(n, _)| n.starts_with("Shop.app/"))
        .cloned()
        .collect();
    let ipa = zip_of(&app, "Payload/");
    let pkg = DiskPackage::open(&write("Shop.ipa", &ipa)).unwrap();
    assert_eq!(pkg.info.binaries.len(), 3);
    assert_eq!(pkg.info.binaries[0].path, "Payload/Shop.app/ShopApp");
    assert!(pkg.info.debug_files.is_empty());

    // Zips inside a folder, and inside a zip, open like folders: an .ipa next
    // to a zip of its dSYMs pairs up.
    let dsyms: Vec<(String, Vec<u8>)> = files.iter().filter(|(n, _)| n.contains(".dSYM/")).cloned().collect();
    let dsyms_zip = zip_of(&dsyms, "");
    write("upload/Shop.ipa", &ipa);
    write("upload/dSYMs.zip", &dsyms_zip);
    let upload = zip_of(
        &[
            ("Shop.ipa".into(), ipa.clone()),
            ("dSYMs.zip".into(), dsyms_zip.clone()),
        ],
        "",
    );
    for pkg in [
        DiskPackage::open(&dir.join("upload")).unwrap(),
        DiskPackage::open(&write("upload.zip", &upload)).unwrap(),
    ] {
        assert_eq!(pkg.info.binaries.len(), 3, "{}", pkg.info.kind);
        assert_eq!(pkg.info.binaries[0].path, "Shop.ipa/Payload/Shop.app/ShopApp");
        assert_eq!(pkg.info.binaries[0].debug, Some(0));
        assert!(pkg.info.debug_files[0].path.starts_with("dSYMs.zip/Shop.app.dSYM/"));
    }

    // A zip of nothing but dSYMs: they are the binaries to explore.
    let pkg = DiskPackage::open(&write("dSYMs.zip", &dsyms_zip)).unwrap();
    assert_eq!(pkg.info.binaries.len(), 1);
    assert_eq!(pkg.info.binaries[0].kind, BinaryKind::Debug);
    assert_eq!(pkg.info.binaries[0].name, "ShopApp");
    assert!(pkg.info.debug_files.is_empty());
    assert_eq!(pkg.info.size, 0, "debug files aren't counted in the size");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headers_say_what_each_fixture_is() {
    use binviz::package::{BinaryKind, header};
    let cases = [
        ("tiny-elf-x64", "ELF", BinaryKind::Executable, "x86_64"),
        ("tiny-elf-x64.stripped", "ELF", BinaryKind::Executable, "x86_64"),
        ("tiny-elf-x64.debug", "ELF", BinaryKind::Debug, "x86_64"),
        ("tiny-elf-x64.o", "ELF", BinaryKind::Object, "x86_64"),
        ("tiny-elf-a64", "ELF", BinaryKind::Executable, "arm64"),
        // Position-independent, with no interpreter: DF_1_PIE says it's an executable.
        ("imports-elf-x64", "ELF", BinaryKind::Executable, "x86_64"),
        ("tiny-macho-a64", "Mach-O", BinaryKind::Executable, "arm64"),
        ("tiny-macho-a64.o", "Mach-O", BinaryKind::Object, "arm64"),
        ("libtiny.dylib", "Mach-O", BinaryKind::Library, "arm64"),
        ("tiny-pe-x64.exe", "PE", BinaryKind::Executable, "x86_64"),
        ("shapes-pe.exe", "PE", BinaryKind::Executable, "x86_64"),
    ];
    for (name, format, kind, arch) in cases {
        let bytes = fixture(name);
        let h = header(&bytes[..bytes.len().min(64 * 1024)]).unwrap_or_else(|| panic!("{name}: not recognised"));
        assert_eq!(
            (h.format.as_str(), h.kind, h.ids[0].arch.as_str()),
            (format, kind, arch),
            "{name}"
        );
    }
    // Mach-O files carry their UUID in the header.
    let bin = open("tiny-macho-a64");
    let h = header(&fixture("tiny-macho-a64")).unwrap();
    assert_eq!(Some(&h.ids[0].id), bin.summary().build_id.as_ref());
    // Not binaries: text, and a Java class (the universal binary magic, with a version for a count).
    assert!(header(b"hello, this is text and long enough to be anything else").is_none());
    assert!(header(&[0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 0x34, 0, 0, 0, 0]).is_none());
}

#[test]
fn elf_debug_files_pair_through_the_debug_link() {
    use binviz::package::{DiskPackage, load_binary, update_loaded};
    let dir = std::env::temp_dir().join(format!("binviz-elf-folder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("usr/bin")).unwrap();
    std::fs::create_dir_all(dir.join("usr/lib/debug")).unwrap();
    std::fs::write(dir.join("usr/bin/tiny"), fixture("tiny-elf-x64.stripped")).unwrap();
    std::fs::write(
        dir.join("usr/lib/debug/tiny-elf-x64.debug"),
        fixture("tiny-elf-x64.debug"),
    )
    .unwrap();
    std::fs::write(dir.join("usr/bin/tiny.exe"), fixture("tiny-pe-x64.exe")).unwrap();
    let mut pkg = DiskPackage::open(&dir).unwrap();
    // Executables at one depth: the largest first.
    let names: Vec<&str> = pkg.info.binaries.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["tiny.exe", "tiny"]);
    assert_eq!(pkg.info.debug_files.len(), 1);
    // No build IDs: nothing pairs until the binary is read and its .gnu_debuglink named.
    assert!(pkg.info.binaries[1].debug.is_none());
    let data = pkg.read_shared(pkg.info.binaries[1].file).unwrap();
    let (mut bin, _) = load_binary(data.clone()).unwrap();
    let mut info = pkg.info.clone();
    update_loaded(&mut info, 1, &data, &bin);
    assert_eq!(info.binaries[1].debug, Some(0));
    assert_eq!(info.debug_files[0].binary, Some(1));
    assert!(!bin.summary().has_dwarf);
    let debug = &info.debug_files[0];
    bin.attach_debug_file(&debug.path, pkg.read_shared(debug.file).unwrap())
        .unwrap();
    assert!(bin.summary().has_dwarf);
    let _ = std::fs::remove_dir_all(&dir);
}
