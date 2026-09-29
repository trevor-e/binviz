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
    "pdbdemo.exe",
    "x86demo.exe",
    "x86demo-fixed.exe",
    "imports-elf-x64",
    "imports-elf-a64",
    "imports-macho-a64",
    "imports-macho-a64.chained",
    "tiny.nes",
    "tiny.gb",
    "tiny.sfc",
    "tiny.z64",
    "tiny-psx.exe",
    "tiny.gba",
    "tiny.md",
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
fn pdb_debug_info() {
    let mut bin = open("pdbdemo.exe");
    assert!(bin.debug_info().is_none());
    assert_eq!(bin.summary().debug_link.as_deref(), Some("pdbdemo.pdb"));
    bin.attach_debug_file("pdbdemo.pdb", fixture("pdbdemo.pdb")).unwrap();
    assert!(bin.summary().has_dwarf);
    // Its procedures name the functions, with their sizes.
    let total = bin.symbols().by_name("pdbdemo::total_area").unwrap();
    assert!(total.size > 0 && !total.size_inferred);
    let (address, size) = (total.address, total.size);
    // Its names were never mangled: the short name finds it too.
    assert_eq!(bin.symbols().by_name("total_area").unwrap().address, address);
    // Data is named once, by its debug name (not also by the linker's).
    let frames: Vec<&str> = bin
        .symbols()
        .iter()
        .map(|s| s.name())
        .filter(|n| n.ends_with("FRAMES"))
        .collect();
    assert_eq!(frames, ["pdbdemo::FRAMES"]);
    // Line records map the code to its source.
    let debug = bin.debug_info().unwrap();
    assert_eq!(
        debug.location(address).unwrap().line,
        source_line("pdbdemo.rs", "fn total_area")
    );
    assert!(debug.location(address + size - 1).is_some());
    // Each module is a unit, in the language its compiler names.
    let unit = debug
        .units()
        .iter()
        .find(|u| u.name.as_deref().is_some_and(|n| n.ends_with("pdbdemo.rs")))
        .unwrap();
    assert_eq!(unit.language.as_deref(), Some("Rust"));
    assert!(unit.code_size > 0);
    // Its procedures are subprograms.
    let (u, offset) = debug.function_die_at(address + 1).unwrap();
    assert_eq!(
        debug.die(u, offset).unwrap().die.name.as_deref(),
        Some("pdbdemo::total_area")
    );
}

#[test]
fn pdbs_must_match() {
    // Another build's PDB: its GUID differs (here, one byte of the binary's record is changed).
    let mut exe = fixture("pdbdemo.exe");
    let rsds = exe.windows(4).position(|w| w == b"RSDS").unwrap();
    exe[rsds + 4] ^= 0xFF;
    let mut bin = Binary::parse(exe).unwrap();
    let err = bin
        .attach_debug_file("pdbdemo.pdb", fixture("pdbdemo.pdb"))
        .unwrap_err();
    assert!(err.to_string().contains("GUID"), "{err}");
    assert!(bin.debug_info().is_none());
    // A PE that names no PDB, and a binary that isn't PE.
    for name in ["tiny-pe-x64.exe", "tiny-elf-x64.stripped"] {
        assert!(
            open(name)
                .attach_debug_file("pdbdemo.pdb", fixture("pdbdemo.pdb"))
                .is_err(),
            "{name}"
        );
    }
    // On its own a PDB is no binary: opening one says what it is for.
    let Err(err) = Binary::parse(fixture("pdbdemo.pdb")) else {
        panic!("a PDB opened as a binary");
    };
    assert!(err.to_string().contains("PDB"), "{err}");
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

#[test]
fn stripped_32_bit_pe_functions_are_found_by_following_the_code() {
    // What the PDB names in the code: (address, size if it records one, name).
    let mut full = open("x86demo.exe");
    full.attach_debug_file("x86demo.pdb", fixture("x86demo.pdb")).unwrap();
    let in_code = |b: &Binary, a: u64| b.section_at(a).is_some_and(|s| s.kind == RegionKind::Code);
    let named: Vec<(u64, Option<u64>, String)> = full
        .symbols()
        .functions()
        .filter(|s| s.source != SymbolSource::Discovered && in_code(&full, s.address))
        .map(|s| {
            (
                s.address,
                (!s.size_inferred).then_some(s.size),
                s.display_name().into_owned(),
            )
        })
        .collect();
    assert!(named.len() >= 40, "{named:?}");
    let start_of = |f: &str| named.iter().find(|n| n.2 == f).unwrap_or_else(|| panic!("{f}")).0;
    let text = |b: &Binary| {
        let s = b.sections().iter().find(|s| s.name == ".text").unwrap();
        b.data()[s.file_offset.unwrap() as usize..][..s.file_size as usize].to_vec()
    };
    for name in ["x86demo.exe", "x86demo-fixed.exe"] {
        let bin = open(name);
        // The same code at the same addresses, with base relocations or without.
        assert!(text(&bin) == text(&full), "{name}: the code differs");
        // Every function the PDB names is found where it starts, and with its
        // size where the PDB records one (it doesn't for the assembly's)...
        for (address, size, f) in &named {
            let found = bin
                .symbols()
                .at(*address)
                .filter(|s| s.kind == binviz::SymbolKind::Function)
                .unwrap_or_else(|| panic!("{name}: {f} at {address:#x} not found"));
            if let Some(size) = size {
                assert_eq!(found.size, *size, "{name}: {f}");
            }
        }
        // ...and nothing else is taken for a function.
        let extra: Vec<String> = bin
            .symbols()
            .functions()
            .filter(|s| named.iter().all(|n| n.0 != s.address))
            .map(|s| s.name().to_string())
            .collect();
        assert!(extra.is_empty(), "{name}: {extra:?}");
        let size = |f: &str| bin.symbols().at(start_of(f)).unwrap().size;
        // MSVC's switch keeps its jump table and index table in the function,
        assert_eq!(size("_msvc_switch"), 77, "{name}");
        // its __finally block is called in place, and the unwinder's way in is there too,
        assert_eq!(size("_with_finally"), 34, "{name}");
        // and a call that doesn't return ends a function, whatever follows it.
        assert_eq!(size("_checked_index"), 20, "{name}");
        assert_eq!(size("_quit"), 10, "{name}");
        // All of the code is in functions, or alignment filler between them.
        let cov = bin.coverage(5);
        let text = cov.sections.iter().find(|s| s.name == ".text").unwrap();
        assert_eq!(text.bytes.unexplored, 0, "{name}: {:?}", cov.gaps);
    }
    // The tables read as data, each entry pointing at a case of the switch.
    let bin = open("x86demo-fixed.exe");
    let switch = start_of("_msvc_switch");
    let d = bin.disassemble_function(switch, 100);
    let entries: Vec<u64> = d
        .instructions
        .iter()
        .filter(|i| i.mnemonic == "dd")
        .map(|i| i.target.unwrap())
        .collect();
    let indexes: Vec<&str> = d
        .instructions
        .iter()
        .filter(|i| i.mnemonic == "db")
        .map(|i| i.operands.as_str())
        .collect();
    assert_eq!(entries.len(), 4, "{d:?}");
    assert_eq!(indexes, ["0, 1, 1, 2, 3, 3, 0, 2", "1"]);
    for case in entries {
        assert!(case > switch && case < switch + 77);
        let refs = bin.references_to(case, case + 1, 0, 10);
        assert!(refs.refs.iter().any(|r| r.kind == binviz::RefKind::Pointer), "{refs:?}");
    }
}

#[test]
fn decompilation_goes_from_what_is_ready() {
    use binviz::{Decomp, DecompState, NextQuery, Readiness};
    let mut bin = open("x86demo.exe");
    bin.attach_debug_file("x86demo.pdb", fixture("x86demo.pdb")).unwrap();
    let at = |bin: &Binary, name: &str| bin.symbols().by_name(name).unwrap_or_else(|| panic!("{name}")).address;
    let set = |bin: &mut Binary, address: u64, decomp: Decomp| {
        let mut notes: Vec<Annotation> = bin.annotations().to_vec();
        notes.retain(|a| a.address != address);
        notes.push(Annotation {
            address,
            size: 0,
            name: String::new(),
            comment: String::new(),
            reviewed: false,
            kind: None,
            decomp: Some(decomp),
        });
        bin.set_annotations(notes);
    };
    let matched = |since: u64| Decomp {
        state: DecompState::Matched,
        since,
        ..Default::default()
    };
    let q = NextQuery {
        limit: 100,
        now: 1000,
        ..Default::default()
    };
    let find = |list: &binviz::NextList, address: u64| list.functions.iter().find(|f| f.address == address).cloned();

    // The program's own functions, its import thunks left out, all to do.
    let list = bin.next_functions(&q);
    assert_eq!(list.progress.functions as usize, list.functions.len());
    assert!(list.functions.len() >= 40, "{}", list.functions.len());
    assert!(!list.functions.iter().any(|f| f.address == at(&bin, "Sleep")));
    // fatal leads: two instructions, and the one thing both its callers wait on.
    let (fatal, checked_div) = (at(&bin, "fatal"), at(&bin, "checked_div"));
    assert_eq!((list.functions[0].address, list.functions[0].unblocks), (fatal, 2));
    assert_eq!(find(&list, checked_div).unwrap().waiting_on, [fatal]);
    // start calls most of the program: it waits.
    let start = find(&list, bin.summary().entry.unwrap()).unwrap();
    assert_eq!(start.readiness, Readiness::Waiting);
    assert!(start.waiting_on.len() >= 8, "{:?}", start.waiting_on);

    // Once fatal matches, its callers are ready and it isn't listed.
    set(&mut bin, fatal, matched(10));
    let list = bin.next_functions(&q);
    assert!(find(&list, fatal).is_none());
    assert_eq!(find(&list, checked_div).unwrap().readiness, Readiness::Ready);
    let mut ready = bin.callers_now_ready(fatal);
    ready.sort_unstable();
    assert_eq!(ready, [checked_div, at(&bin, "_checked_index")]);
    assert_eq!((list.progress.matched, list.progress.matched_bytes), (1, 10));

    // A claim keeps a function from others for an hour.
    let claimed = Decomp {
        state: DecompState::InProgress,
        by: "agent-1".into(),
        since: 1000,
        ..Default::default()
    };
    set(&mut bin, checked_div, claimed);
    let list = bin.next_functions(&q);
    assert!(find(&list, checked_div).is_none());
    assert_eq!(list.claimed, 1);
    let later = NextQuery {
        now: 1000 + 3600,
        ..q.clone()
    };
    assert!(find(&bin.next_functions(&later), checked_div).is_some());

    // Three tries without a match send a function to the end of the list,
    // until something it calls is done after the last try.
    let (dispatch, sum3) = (at(&bin, "dispatch"), at(&bin, "sum3"));
    let tried = Decomp {
        attempts: 3,
        percent: Some(71.0),
        since: 20,
        ..Default::default()
    };
    set(&mut bin, dispatch, tried);
    let list = bin.next_functions(&q);
    assert_eq!(list.functions.last().unwrap().address, dispatch);
    assert_eq!(find(&list, dispatch).unwrap().readiness, Readiness::Hard);
    set(&mut bin, sum3, matched(30));
    assert_eq!(
        find(&bin.next_functions(&q), dispatch).unwrap().readiness,
        Readiness::Waiting
    );

    // A function copied and edited from one done is a template: first in line.
    let (health, ammo) = (at(&bin, "clamp_health"), at(&bin, "clamp_ammo"));
    let like = bin.similar_functions(ammo, 3);
    assert_eq!(like[0].address, health);
    assert!(like[0].similarity >= 0.9, "{like:?}");
    set(&mut bin, health, matched(40));
    let list = bin.next_functions(&q);
    let first = &list.functions[0];
    assert_eq!((first.address, first.readiness), (ammo, Readiness::LikeDone));
    assert_eq!(first.like.unwrap().address, health);
    // Its C is the worked example the context for writing the copy offers.
    assert_eq!(bin.worked_examples(ammo, 3).first().map(|e| e.address), Some(health));
    let context = bin.decomp_context(ammo, 100).unwrap();
    assert_eq!(context.examples[0].name, "clamp_health");
    assert!(context.describe().contains("Worked examples"), "{}", context.describe());

    // Only what starts in a range, when asked.
    let within = NextQuery {
        within: Some((ammo, ammo + 1)),
        ..q.clone()
    };
    let list = bin.next_functions(&within);
    assert_eq!(list.functions.iter().map(|f| f.address).collect::<Vec<_>>(), [ammo]);
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
            kind: None, decomp: None,
        },
        Annotation {
            address: main.address + 0x10,
            size: 0,
            name: String::new(),
            comment: "calls __main".into(),
            reviewed: false,
            kind: None, decomp: None,
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

#[test]
fn pdbs_pair_through_the_name_their_binary_records() {
    use binviz::package::{BinaryKind, DiskPackage, header, load_binary, update_loaded};
    let pdb = fixture("pdbdemo.pdb");
    let h = header(&pdb[..64 * 1024]).unwrap();
    assert_eq!((h.format.as_str(), h.kind), ("PDB", BinaryKind::Debug));
    let dir = std::env::temp_dir().join(format!("binviz-pdb-folder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::create_dir_all(dir.join("symbols")).unwrap();
    std::fs::write(dir.join("bin/pdbdemo.exe"), fixture("pdbdemo.exe")).unwrap();
    // Windows ignores case in file names, and so does the pairing.
    std::fs::write(dir.join("symbols/PDBDEMO.PDB"), &pdb).unwrap();
    let mut pkg = DiskPackage::open(&dir).unwrap();
    assert_eq!(pkg.info.binaries.len(), 1);
    assert_eq!(pkg.info.debug_files.len(), 1);
    // Its GUID is past the header: it pairs once the binary is read and names it.
    assert!(pkg.info.binaries[0].debug.is_none());
    let data = pkg.read_shared(pkg.info.binaries[0].file).unwrap();
    let (mut bin, _) = load_binary(data.clone()).unwrap();
    let mut info = pkg.info.clone();
    update_loaded(&mut info, 0, &data, &bin);
    assert_eq!(info.binaries[0].debug, Some(0));
    let debug = &info.debug_files[0];
    bin.attach_debug_file(&debug.path, pkg.read_shared(debug.file).unwrap())
        .unwrap();
    assert!(bin.summary().has_dwarf);
    // With no binary beside it, a PDB is what there is (to pair with what is open).
    std::fs::remove_file(dir.join("bin/pdbdemo.exe")).unwrap();
    let pkg = DiskPackage::open(&dir).unwrap();
    let b = &pkg.info.binaries[0];
    assert_eq!((b.format.as_str(), b.kind), ("PDB", BinaryKind::Debug));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn crash_reports_symbolicate() {
    use binviz::crash::{Found, ImageMatch, parse, symbolicate};
    // An Android tombstone frame inside tiny::run, where dot() is inlined.
    let bin = open("tiny-elf-x64");
    let run = bin.symbols().by_name("tiny::run").expect("run");
    let inlined = (run.address..run.address + run.size)
        .find(|&a| bin.symbolize(a).first().and_then(|l| l.function.as_deref()) == Some("tiny::dot"))
        .expect("an address in the inlined dot()");
    let tombstone = format!(
        "pid: 7, tid: 7, name: tiny  >>> tiny <<<\nsignal 11 (SIGSEGV)\nbacktrace:\n      #00 pc {inlined:016x}  /system/bin/tiny-elf-x64\n"
    );
    let report = parse(&tombstone).expect("a tombstone");
    // No build ID in this fixture: it matches by name.
    assert_eq!(report.images[0].matches([], "tiny-elf-x64"), ImageMatch::Yes);
    let out = symbolicate(
        &report,
        &[Found {
            image: 0,
            binary: 0,
            bin: &bin,
        }],
        &[],
    );
    let lines = &out.threads[0].frames[0].lines;
    assert_eq!(lines[0].function.as_deref(), Some("tiny::dot"), "{lines:?}");
    assert!(lines[0].inlined && lines[0].file.as_deref().is_some_and(|f| f.ends_with("tiny.rs")));
    assert_eq!(lines.last().unwrap().function.as_deref(), Some("tiny::run"));

    // An Apple report for the stripped app, loaded 0x4000000 higher than linked,
    // symbolicated through its "dSYM" (the unstripped copy).
    let mut app = open("imports-macho-a64.chained.stripped");
    let main = open("imports-macho-a64.chained")
        .symbols()
        .by_name("_main")
        .expect("main")
        .address;
    let uuid = app.summary().build_id.clone().expect("a UUID");
    let slide = 0x400_0000u64;
    let apple = format!(
        "Process: ShopApp [1]\nThread 0 Crashed:\n0   ShopApp  0x{:016x} 0x{:x} + {}\n\nBinary Images:\n0x{:x} - 0x{:x} ShopApp arm64  <{}> /var/ShopApp.app/ShopApp\n",
        main + slide + 8,
        0x1_0000_0000u64 + slide,
        main - 0x1_0000_0000 + 8,
        0x1_0000_0000u64 + slide,
        0x1_0000_ffffu64 + slide,
        uuid.replace('-', "").to_lowercase()
    );
    let report = parse(&apple).expect("an Apple report");
    assert_eq!(report.images[0].matches([uuid.as_str()], "ShopApp"), ImageMatch::Yes);
    assert_eq!(
        report.images[0].matches(["00000000-0000-0000-0000-000000000000"], "ShopApp"),
        ImageMatch::OtherBuild
    );
    let mut dsym = fixture("imports-macho-a64.chained");
    dsym[12..16].copy_from_slice(&10u32.to_le_bytes());
    app.attach_debug_file("ShopApp.dSYM", dsym).unwrap();
    let out = symbolicate(
        &report,
        &[Found {
            image: 0,
            binary: 3,
            bin: &app,
        }],
        &[],
    );
    let frame = &out.threads[0].frames[0];
    assert_eq!(frame.binary, Some(3));
    assert_eq!(frame.binary_address, Some(main + 8));
    assert_eq!(frame.lines[0].function.as_deref(), Some("_main"), "{frame:?}");
    assert_eq!(frame.lines[0].offset, Some(8));
}

#[test]
fn names_demangled_elsewhere_are_used() {
    // binviz can't demangle Swift; names from `swift-demangle` (or anything
    // else) replace the raw ones wherever a symbol is shown.
    let mut bin = open("tiny-macho-a64");
    assert_eq!(bin.symbols().by_name("_main").unwrap().display_name(), "_main");
    bin.add_demangled_names([("_main".to_string(), "the entry point".to_string())]);
    assert_eq!(
        bin.symbols().by_name("_main").unwrap().display_name(),
        "the entry point"
    );
    let found = bin.search("entry point", 5, None);
    assert!(
        found.hits.iter().any(|h| h.label.contains("the entry point")),
        "{:?}",
        found.hits
    );
}

#[test]
fn objective_c_metadata_names_what_stripping_removed() {
    use binviz::{SymbolKind, SymbolSource};
    let bin = open("objc-macho-a64.chained.stripped");
    let objc = bin.objc();
    // The class; its superclass is bound to libobjc's NSObject.
    assert_eq!(objc.classes.len(), 1);
    let greeter = &objc.classes[0];
    assert_eq!(greeter.name, "Greeter");
    assert_eq!(greeter.superclass.as_deref(), Some("NSObject"));
    assert_eq!(greeter.instance_size, 16);
    let ivar = &greeter.ivars[0];
    assert_eq!(
        (ivar.name.as_str(), ivar.types.as_str(), ivar.offset, ivar.size),
        ("_name", "@\"NSString\"", Some(8), 8)
    );
    assert_eq!(greeter.properties[0].name, "name");
    // A category on a class from elsewhere, with a relative method list.
    let extras = &objc.categories[0];
    assert_eq!((extras.name.as_str(), extras.class.as_str()), ("Extras", "NSObject"));

    // The methods get their names back, sized by the function starts.
    let named = |n: &str| bin.symbols().by_name(n).unwrap_or_else(|| panic!("{n}"));
    for method in [
        "-[Greeter hello]",
        "-[Greeter greetWith:times:]",
        "+[Greeter make]",
        "-[NSObject(Extras) wave]",
    ] {
        let s = named(method);
        assert_eq!(
            (s.source, s.kind, s.size),
            (SymbolSource::Objc, SymbolKind::Function, 8),
            "{method}"
        );
    }
    // So do the metadata, and the references code loads.
    for name in [
        "_OBJC_CLASS_$_Greeter",
        "_OBJC_METACLASS_$_Greeter",
        "__OBJC_CLASS_RO_$_Greeter",
        "__OBJC_$_INSTANCE_METHODS_Greeter",
        "__OBJC_$_CLASS_METHODS_Greeter",
        "__OBJC_$_CATEGORY_NSObject_$_Extras",
        "_OBJC_IVAR_$_Greeter._name",
        "@selector(hello)",
        "_objc_msgSend$wave",
    ] {
        assert_eq!(named(name).source, SymbolSource::Objc, "{name}");
    }
    assert_eq!(named("__OBJC_$_INSTANCE_METHODS_Greeter").size, 8 + 2 * 24);

    // main sends each selector: through objc_msgSend, or the objc_msgSend$wave stub.
    let main = bin.summary().entry.unwrap();
    for selector in ["hello", "make", "wave"] {
        let uses = bin.objc_selector(selector).unwrap();
        assert_eq!(uses.implementations.len(), 1, "{selector}");
        let senders: Vec<u64> = uses.senders.iter().map(|s| s.address).collect();
        assert_eq!(senders, [main], "{selector}");
    }
    assert_eq!(bin.objc_selector("wave").unwrap().stubs.len(), 1);
    assert!(bin.objc_selector("nothing").is_none());

    // The class as its header would declare it.
    let text = objc.interface("Greeter").unwrap().to_text();
    for line in [
        "@interface Greeter : NSObject",
        "    NSString *_name;",
        "@property (nonatomic, strong) NSString *name;",
        "+ (id)make;",
        "- (void)greetWith:(id)arg1 times:(int)arg2;",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
    let text = objc.interface("NSObject (Extras)").unwrap().to_text();
    assert!(text.contains("- (void)wave;"), "{text}");
}

#[test]
fn objective_c_classes_bound_by_dyld_info() {
    use binviz::SymbolSource;
    // Without chained fixups the loader binds the superclass by opcode.
    let bin = open("objc-macho-a64");
    let objc = bin.objc();
    assert_eq!(objc.classes[0].superclass.as_deref(), Some("NSObject"));
    assert_eq!(objc.categories[0].class, "NSObject");
    // The implementations are where the symbol table says, and its names stay.
    for (selector, symbol) in [
        ("hello", "_Greeter_hello"),
        ("greetWith:times:", "_Greeter_greetWith_times"),
        ("make", "_Greeter_make"),
        ("wave", "_NSObject_Extras_wave"),
    ] {
        let address = bin.symbols().by_name(symbol).unwrap().address;
        assert_eq!(objc.implementations(selector)[0].address, address, "{selector}");
        assert_eq!(bin.symbols().at(address).unwrap().name(), symbol);
    }
    // The metadata names what the symbol table doesn't.
    assert_eq!(
        bin.symbols().by_name("@selector(hello)").unwrap().source,
        SymbolSource::Objc
    );
    // Images without Objective-C have nothing to show.
    assert!(open("imports-macho-a64.chained").objc().is_empty());
    assert!(open("tiny-elf-x64").objc().is_empty());
}

/// A debug map's object, read from the fixtures by its file name (the path it
/// records is where it was built).
fn object_from_fixtures(o: &binviz::dwarf::debugmap::DebugMapObject) -> Result<Option<Vec<u8>>, String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bin");
    Ok(std::fs::read(dir.join(o.file_name())).ok())
}

#[test]
fn debug_maps_bring_the_objects_dwarf() {
    let obj = open("tiny-macho-a64.o");
    let od = obj.debug_info().unwrap();
    // As linked, and with the functions reordered and main dead-stripped.
    for name in ["tiny-macho-a64", "tiny-macho-a64.reordered"] {
        let mut bin = open(name);
        assert!(bin.debug_info().is_none(), "{name}");
        let map = bin.debug_map();
        assert_eq!(map.len(), 1, "{name}");
        assert_eq!(map[0].file_name(), "tiny-macho-a64.o");
        let report = bin.attach_debug_map(&mut object_from_fixtures).unwrap();
        assert_eq!((report.linked, report.units), (1, 1), "{name}: {report:?}");
        let debug = bin.debug_info().unwrap();
        // Every instruction has the object's line for it, wherever its function went.
        for func in ["checksum", "tiny::fib", "tiny::run"] {
            let (os, es) = (
                obj.symbols().by_name(func).unwrap(),
                bin.symbols().by_name(func).unwrap(),
            );
            assert_eq!(os.size, es.size, "{name}: {func}");
            for delta in (0..os.size).step_by(4) {
                assert_eq!(
                    od.location(os.address + delta).map(|l| (l.line, l.column)),
                    debug.location(es.address + delta).map(|l| (l.line, l.column)),
                    "{name}: {func}+{delta:#x}"
                );
            }
            let frames = debug.frames(es.address + 8);
            let outer = frames.last().unwrap_or_else(|| panic!("{name}: no frames in {func}"));
            let function = outer.demangled.as_deref().or(outer.function.as_deref()).unwrap_or("");
            assert!(
                function.ends_with(func.trim_start_matches("tiny::")),
                "{name}: {function}"
            );
        }
        // The DIEs follow their code.
        let fib = debug
            .search("fib", 10)
            .into_iter()
            .find(|d| d.tag == "DW_TAG_subprogram")
            .unwrap();
        assert_eq!(
            fib.low_pc,
            Some(bin.symbols().by_name("tiny::fib").unwrap().address),
            "{name}"
        );
        let dead = debug
            .search("main", 10)
            .into_iter()
            .any(|d| d.tag == "DW_TAG_subprogram" && d.name.as_deref() == Some("main"));
        assert_eq!(dead, name == "tiny-macho-a64", "{name}: main");
    }
    // Objects that can't be found are reported, not guessed at.
    let mut bin = open("tiny-macho-a64");
    let err = bin.attach_debug_map(&mut |_| Ok(None)).unwrap_err();
    assert!(err.message().contains("was not found"), "{err}");
    // lld records no modification time; with one, a rebuilt object is refused.
    assert_eq!(bin.debug_map()[0].modified, 0);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bin");
    let report = bin.attach_debug_map_from_disk(&[dir]).unwrap();
    assert_eq!(report.linked, 1, "{report:?}");
}

fn functions(bin: &Binary) -> Vec<(u64, String)> {
    bin.symbols()
        .iter()
        .filter(|s| s.kind == binviz::SymbolKind::Function && s.defined)
        .map(|s| (s.address, s.display_name().into_owned()))
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_iter()
        .collect()
}

#[test]
fn nes_roms_are_followed_from_their_vectors() {
    let bin = open("tiny.nes");
    let s = bin.summary();
    assert_eq!((s.format, s.arch.as_str()), (binviz::Format::Rom, "6502"));
    // Banks get addresses of their own: the fixed last one is bank 3's $C000.
    assert_eq!(s.entry, Some(0x3_C000));
    let banks: Vec<(&str, u64)> = bin
        .sections()
        .iter()
        .filter(|s| s.name.starts_with("PRG bank"))
        .map(|s| (s.name.as_str(), s.address))
        .collect();
    assert_eq!(
        banks,
        [
            ("PRG bank 0", 0x8000),
            ("PRG bank 1", 0x1_8000),
            ("PRG bank 2", 0x2_8000),
            ("PRG bank 3 (fixed)", 0x3_C000)
        ]
    );
    // The handlers, named after their vectors, and what they call.
    let found = functions(&bin);
    let names: Vec<&str> = found.iter().map(|f| f.1.as_str()).collect();
    assert_eq!(names, ["reset", "sub_3c023", "sub_3c029", "nmi", "irq"]);
    // The registers are named, and who reads and writes them is known.
    bin.prepare_xrefs();
    let ppuctrl = bin.symbols().by_name("PPUCTRL").unwrap().address;
    assert_eq!(bin.references_to(ppuctrl, ppuctrl + 1, 0, 10).counts.write, 2);
    let status = bin.symbols().by_name("PPUSTATUS").unwrap().address;
    let readers = bin.references_to(status, status + 1, 0, 10);
    assert_eq!(readers.refs[0].function, Some(0x3_C023));
    let code = bin.disassemble_function(0x3_C000, 100);
    let text: Vec<String> = code
        .instructions
        .iter()
        .map(|i| {
            format!(
                "{} {}{}",
                i.mnemonic,
                i.operands,
                i.target_symbol
                    .as_deref()
                    .map(|t| format!(" <{t}>"))
                    .unwrap_or_default()
            )
        })
        .collect();
    assert_eq!(text[5], "sta $2000 <PPUCTRL>");
    // A call into the switched window can't be placed from the fixed bank.
    let into_bank = code
        .instructions
        .iter()
        .find(|i| i.operands == "$8000" && i.mnemonic == "jsr")
        .unwrap();
    assert_eq!(into_bank.target, None);
    // The palette table the loop reads.
    let palette = bin.disassemble_function(0x3_C029, 100);
    let read = palette
        .instructions
        .iter()
        .find(|i| i.mnemonic == "lda" && i.operands.ends_with(",x"))
        .unwrap();
    assert_eq!(read.target, Some(0x3_C041));
}

#[test]
fn a_code_data_log_finds_what_the_game_ran() {
    let bin = open("tiny.nes");
    let data = bin.data();
    // FCEUX's log for tiny.nes: a flag byte per PRG ROM byte, then per CHR ROM byte.
    let mut log = vec![0u8; 0x10000 + 0x2000];
    // Bank 1's code, run at $8000 (window 0) after the fixed bank switched it in.
    log[0x4000..0x4007].fill(0x01);
    // The fixed bank's reset code, at $C000 (window 2), and the palette read as data.
    log[0xC000..0xC010].fill(0x01 | 2 << 2);
    log[0xC041..0xC045].fill(0x02 | 2 << 2);
    // Bank 2's text, read as data.
    log[0x8000..0x8040].fill(0x02);
    let (logged, summary) = bin.with_code_log(&log).unwrap();
    assert_eq!(summary.format, binviz::rom::cdl::LogFormat::Fceux);
    assert_eq!(
        (summary.code, summary.data, summary.pages_placed),
        (7 + 16, 4 + 0x40, 0)
    );
    // Bank 1's code, which nothing static reaches (`jsr $8000` could be any bank), and what it calls.
    let found: Vec<u64> = functions(&logged).into_iter().map(|f| f.0).collect();
    assert!(found.contains(&0x1_8000) && found.contains(&0x1_8006), "{found:x?}");
    assert!(!functions(&bin).iter().any(|f| f.0 == 0x1_8000));
    // The log's flags, by file offset.
    assert_eq!(logged.code_log_at(16 + 0xC041), binviz::rom::cdl::flag::DATA);
    assert!(logged.summary().properties.iter().any(|p| p.key == "Code/data log"));
    // The same bytes: notes and places still match.
    assert_eq!(logged.summary().fingerprint, bin.summary().fingerprint);
    assert_eq!(logged.data(), data);
}

#[test]
fn a_code_data_log_places_switched_pages_where_they_ran() {
    // An MMC3 game: 64 KiB of PRG ROM in 8 KiB pages, the last fixed at $E000.
    let mut rom = b"NES\x1a".to_vec();
    rom.extend_from_slice(&[4, 0, 0x40, 0]);
    rom.extend_from_slice(&[0; 8]);
    let mut prg = vec![0u8; 0x10000];
    // Page 3, run at $8000: lda #1 / jsr $8006 / rts, and $8006: rts.
    prg[0x6000..0x6007].copy_from_slice(&[0xA9, 0x01, 0x20, 0x06, 0x80, 0x60, 0x60]);
    // The last page: reset at $E000 loops forever.
    prg[0xE000..0xE003].copy_from_slice(&[0x4C, 0x00, 0xE0]);
    prg[0xFFFA..].copy_from_slice(&[0x00, 0xE0, 0x00, 0xE0, 0x00, 0xE0]);
    rom.extend_from_slice(&prg);
    let bin = Binary::parse(rom).unwrap();
    // Without a log, page 3 is the second half of 16 KiB bank 1: at $A000.
    assert!(
        bin.sections()
            .iter()
            .any(|s| s.name == "PRG bank 1" && s.address == 0x1_8000)
    );
    let mut log = vec![0u8; 0x10000];
    log[0x6000..0x6007].fill(0x01);
    log[0xE000..0xE003].fill(0x01 | 3 << 2);
    let (logged, summary) = bin.with_code_log(&log).unwrap();
    assert_eq!(summary.pages_placed, 2);
    let page = logged
        .sections()
        .iter()
        .find(|s| s.name == "PRG page 3 (at $8000)")
        .unwrap();
    assert_eq!((page.address, page.file_offset), (0x3_8000, Some(16 + 0x6000)));
    let found: Vec<u64> = functions(&logged).into_iter().map(|f| f.0).collect();
    assert!(found.contains(&0x3_8000) && found.contains(&0x3_8006), "{found:x?}");
    assert_eq!(logged.summary().entry, Some(0x7_E000));
}

#[test]
fn label_files_are_read_and_written() {
    use binviz::rom::labels::LabelFormat;
    // Mesen: PRG ROM offsets, RAM offsets, registers; FCEUX: CPU addresses in bank files.
    let mut nes = open("tiny.nes");
    let mlb = "P:C000:Reset:Starts here\nP:4000:BankOne\nR:0010:FrameCount:bumped each NMI\\nby one\nG:2000:PpuCtrl\nP:C041-C044:Palette\nSpcRam:0100:NotNes\n";
    let read = nes.read_labels("game.mlb", mlb).unwrap();
    assert_eq!(read.format, LabelFormat::Mlb);
    assert_eq!(read.skipped, 1);
    let at: Vec<(u64, u64, &str)> = read
        .labels
        .iter()
        .map(|a| (a.address, a.size, a.name.as_str()))
        .collect();
    assert_eq!(
        at,
        [
            (0x10, 0, "FrameCount"),
            (0x2000, 0, "PpuCtrl"),
            (0x1_8000, 0, "BankOne"),
            (0x3_C000, 0, "Reset"),
            (0x3_C041, 4, "Palette")
        ]
    );
    assert_eq!(read.labels[0].comment, "bumped each NMI\nby one");
    // A bank file's addresses are in that 16 KiB of PRG ROM, wherever it is mapped; RAM's anywhere.
    let nl = nes
        .read_labels(
            "tiny.nes.3.nl",
            "$C000#Reset#entry\n\\more\n$C041/4#Palette#\n$0010#Frames#\n",
        )
        .unwrap();
    assert_eq!((nl.format, nl.labels.len(), nl.skipped), (LabelFormat::Nl, 3, 0));
    assert_eq!(
        (nl.labels[1].address, nl.labels[1].comment.as_str()),
        (0x3_C000, "entry\nmore")
    );
    assert_eq!((nl.labels[2].address, nl.labels[2].size), (0x3_C041, 4));
    // What binviz writes, the emulators read back to the same places.
    nes.set_annotations(read.labels.clone());
    for format in nes.label_formats() {
        let files = nes.write_labels(format).unwrap();
        let mut back = Vec::new();
        for f in &files {
            back.extend(
                nes.read_labels(&format!("tiny.nes.{}", f.suffix), &f.text)
                    .unwrap()
                    .labels,
            );
        }
        back.sort_by_key(|a| (a.address, a.size));
        let names: Vec<(u64, &str)> = back.iter().map(|a| (a.address, a.name.as_str())).collect();
        assert_eq!(
            names,
            [
                (0x10, "FrameCount"),
                (0x2000, "PpuCtrl"),
                (0x1_8000, "BankOne"),
                (0x3_C000, "Reset"),
                (0x3_C041, "Palette")
            ],
            "{format:?}: {:?}",
            files
        );
    }
    let nl_files = nes.write_labels(LabelFormat::Nl).unwrap();
    let suffixes: Vec<&str> = nl_files.iter().map(|f| f.suffix.as_str()).collect();
    assert_eq!(suffixes, ["ram.nl", "1.nl", "3.nl"]);

    // RGBDS: bank:address, lowercase; bare constants are skipped.
    let mut gb = open("tiny.gb");
    let sym = "; File generated by rgblink\n00:0150 Main\n00:0150 Main.start\n01:4000 BankedFn\n00:c000 wCounter\n00:ff80 hFrames\n0042 SOME_CONSTANT\n";
    let read = gb.read_labels("tiny.sym", sym).unwrap();
    assert_eq!(read.format, LabelFormat::Sym);
    let at: Vec<(u64, &str)> = read.labels.iter().map(|a| (a.address, a.name.as_str())).collect();
    assert_eq!(
        at,
        [
            (0x150, "Main"),
            (0x150, "Main.start"),
            (0xC000, "wCounter"),
            (0xFF80, "hFrames"),
            (0x1_4000, "BankedFn")
        ]
    );
    gb.set_annotations(read.labels[2..].to_vec());
    let text = &gb.write_labels(LabelFormat::Sym).unwrap()[0].text;
    assert!(
        text.contains("01:4000 BankedFn") && text.contains("00:c000 wCounter"),
        "{text}"
    );

    // no$gba: addresses, and directives that aren't names.
    let gba = open("tiny.gba");
    let read = gba
        .read_labels(
            "tiny.sym",
            "08000000 .arm\n080000C0 start\n03000000 iwram_var\n08000100 .dbl:0010\n",
        )
        .unwrap();
    assert_eq!((read.format, read.directives), (LabelFormat::NoCash, 2));
    let at: Vec<(u64, &str)> = read.labels.iter().map(|a| (a.address, a.name.as_str())).collect();
    assert_eq!(at, [(0x0300_0000, "iwram_var"), (0x0800_00C0, "start")]);

    // WLA DX: 24-bit SNES addresses, mirrors folded.
    let snes = open("tiny.sfc");
    let read = snes
        .read_labels(
            "tiny.sym",
            "[information]\nversion 3\n\n[labels]\n00:8000 Reset\n80:8010 FastMirror\n7e:0010 wram_var\n",
        )
        .unwrap();
    let at: Vec<(u64, &str)> = read.labels.iter().map(|a| (a.address, a.name.as_str())).collect();
    assert_eq!(at, [(0x8000, "Reset"), (0x8010, "FastMirror"), (0x7E_0010, "wram_var")]);
    // Consoles without emulator label files say so.
    assert!(open("tiny.md").write_labels(LabelFormat::Mlb).is_err());
}

#[test]
fn game_boy_and_snes_roms() {
    let gb = open("tiny.gb");
    assert_eq!(gb.summary().arch, "SM83");
    assert!(
        gb.summary()
            .properties
            .iter()
            .any(|p| p.key == "Header checksum" && p.value == "matches")
    );
    let names: Vec<String> = functions(&gb).into_iter().map(|f| f.1).collect();
    // The entry jumps over the header: the code there is a function of its own.
    for want in ["entry", "vblank", "sub_150", "sub_16f", "sub_14000"] {
        assert!(names.iter().any(|n| n == want), "{want}: {names:?}");
    }
    let entry = gb.symbols().by_name("entry").unwrap();
    assert_eq!(entry.size, 4);
    gb.prepare_xrefs();
    let lcdc = gb.symbols().by_name("LCDC").unwrap().address;
    assert_eq!(gb.references_to(lcdc, lcdc + 1, 0, 10).counts.write, 2);

    let snes = open("tiny.sfc");
    let s = snes.summary();
    assert_eq!((s.arch.as_str(), s.entry), ("65816", Some(0x8000)));
    assert!(
        s.properties
            .iter()
            .any(|p| p.key == "Title" && p.value == "BINVIZ TEST")
    );
    assert!(s.properties.iter().any(|p| p.key == "Checksum" && p.value == "matches"));
    // `jsl` reaches bank $01; after `rep #$20` the accumulator's immediates are 16 bits.
    let reset = snes.disassemble_function(0x8000, 100);
    let lines: Vec<String> = reset
        .instructions
        .iter()
        .map(|i| format!("{} {}", i.mnemonic, i.operands))
        .collect();
    assert!(lines.contains(&"lda #$1234".to_string()), "{lines:?}");
    assert!(
        reset
            .instructions
            .iter()
            .any(|i| i.mnemonic == "jsl" && i.target == Some(0x1_8000))
    );
    let far = snes.symbols().at(0x1_8000).unwrap();
    assert_eq!(far.size, 10);
    // $7E0010 is WRAM; INIDISP is written in bank $00 and in bank $01.
    snes.prepare_xrefs();
    let inidisp = snes.symbols().by_name("INIDISP").unwrap().address;
    assert_eq!(snes.references_to(inidisp, inidisp + 1, 0, 10).counts.write, 2);
}

#[test]
fn nintendo_64_and_playstation_code() {
    // The same ROM in the three byte orders reads the same.
    let z64 = fixture("tiny.z64");
    let v64: Vec<u8> = z64.chunks(2).flat_map(|p| [p[1], p[0]]).collect();
    let n64: Vec<u8> = z64.chunks(4).flat_map(|w| [w[3], w[2], w[1], w[0]]).collect();
    for (name, data) in [("z64", z64), ("v64", v64), ("n64", n64)] {
        let bin = Binary::parse(data).unwrap();
        assert_eq!(
            (bin.summary().arch.as_str(), bin.summary().entry),
            ("MIPS R4300i", Some(0x8000_0400)),
            "{name}"
        );
        let entry = bin.disassemble_function(0x8000_0400, 100);
        let text: Vec<String> = entry
            .instructions
            .iter()
            .map(|i| {
                format!(
                    "{} {}{}",
                    i.mnemonic,
                    i.operands,
                    i.target_symbol
                        .as_deref()
                        .map(|t| format!(" <{t}>"))
                        .unwrap_or_default()
                )
            })
            .collect();
        assert_eq!(text[3], "sw $zero, 0x0($t0) <VI_STATUS>", "{name}");
        // The call's delay slot belongs to the caller; the callee is a function of its own.
        assert!(text[5].starts_with("jal 0x80000424"), "{name}: {text:?}");
        assert_eq!(bin.symbols().at(0x8000_0424).unwrap().size, 24, "{name}");
        // lui + lw: the counter the callee reads and writes.
        bin.prepare_xrefs();
        let counter = bin.references_to(0x8000_0600, 0x8000_0601, 0, 10);
        assert_eq!((counter.counts.read, counter.counts.write), (1, 1), "{name}");
    }

    let psx = open("tiny-psx.exe");
    assert_eq!(psx.summary().arch, "MIPS R3000A");
    let text: Vec<String> = psx
        .disassemble_function(0x8001_0000, 100)
        .instructions
        .iter()
        .map(|i| {
            format!(
                "{} {}{}",
                i.mnemonic,
                i.operands,
                i.target_symbol
                    .as_deref()
                    .map(|t| format!(" <{t}>"))
                    .unwrap_or_default()
            )
        })
        .collect();
    assert_eq!(text[1], "sw $zero, 0x1074($t0) <I_MASK>");
    assert_eq!(text[2], "lw $v0, 0x1814($t0) <GP1>");
    assert_eq!(text[5], "li $t2, 0xa0 <bios_a>");
    // $gp comes from the header: -0x8000($gp) is the start of the code.
    let func = psx.disassemble_function(0x8001_0020, 10);
    assert_eq!(func.instructions[0].target, Some(0x8001_0000));
}

#[test]
fn game_boy_advance_arm_and_thumb() {
    let bin = open("tiny.gba");
    assert_eq!(
        (bin.summary().arch.as_str(), bin.summary().entry),
        ("ARM7TDMI", Some(0x0800_0000))
    );
    assert!(
        bin.summary()
            .properties
            .iter()
            .any(|p| p.key == "Header checksum" && p.value == "matches")
    );
    // The branch over the header, the ARM start-up code, and through `bx` the Thumb main and what it calls.
    let starts: Vec<u64> = functions(&bin).into_iter().map(|f| f.0).collect();
    assert_eq!(starts, [0x0800_0000, 0x0800_00C0, 0x0800_0100, 0x0800_0110]);
    let arm = bin.disassemble_function(0x0800_00C0, 100);
    let bx = arm.instructions.iter().find(|i| i.mnemonic == "bx").unwrap();
    assert_eq!(bx.target, Some(0x0800_0100));
    let thumb = bin.disassemble_function(0x0800_0100, 100);
    let lens: Vec<u32> = thumb.instructions.iter().map(|i| i.len).collect();
    assert_eq!(lens, [2, 2, 2, 4, 2]);
    // Registers named through the literal pool: r0 = 0x04000130.
    let load = &thumb.instructions[2];
    assert_eq!(
        (load.mnemonic.as_str(), load.target_symbol.as_deref()),
        ("ldrh", Some("KEYINPUT"))
    );
    bin.prepare_xrefs();
    let dispcnt = bin.symbols().by_name("DISPCNT").unwrap().address;
    assert_eq!(bin.references_to(dispcnt, dispcnt + 1, 0, 10).counts.write, 1);
}

#[test]
fn mega_drive_68000() {
    let bin = open("tiny.md");
    let s = bin.summary();
    assert_eq!((s.arch.as_str(), s.entry), ("68000", Some(0x200)));
    assert!(s.properties.iter().any(|p| p.key == "Checksum" && p.value == "matches"));
    let starts: Vec<(u64, String)> = functions(&bin);
    let names: Vec<&str> = starts.iter().map(|f| f.1.as_str()).collect();
    // The reset, what it calls, and the interrupt handlers (the shared error loop named by its first vector).
    assert_eq!(names, ["reset", "sub_21e", "hblank", "vblank"]);
    // lea into a4, then (a4): the VDP's control port.
    let reset = bin.disassemble_function(0x200, 100);
    let writes: Vec<Option<&str>> = reset
        .instructions
        .iter()
        .filter(|i| i.operands.ends_with("(a4)"))
        .map(|i| i.target_symbol.as_deref())
        .collect();
    assert_eq!(writes, [Some("VDP_CTRL"), Some("VDP_CTRL")]);
    bin.prepare_xrefs();
    let frames = bin.references_to(0xFF_0010, 0xFF_0011, 0, 10);
    assert_eq!((frames.counts.write, frames.refs[0].function), (1, Some(0x250)));
    // A .smd copier dump: interleaved blocks behind a 512-byte header, read the same.
    let md = fixture("tiny.md");
    let mut block = vec![0u8; 0x4000];
    block[..md.len()].copy_from_slice(&md);
    let mut smd = vec![0u8; 512];
    smd[8] = 0xAA;
    smd[9] = 0xBB;
    smd.extend((0..0x2000).map(|i| block[2 * i + 1]));
    smd.extend((0..0x2000).map(|i| block[2 * i]));
    let from_smd = Binary::parse(smd).unwrap();
    assert_eq!(functions(&from_smd).len(), 4);
}

#[test]
fn functions_are_matched_across_builds() {
    use binviz::fndiff::{MatchKind, PairStatus};
    // A ROM and a patched copy: one function changed, the rest the same.
    let old = open("tiny.nes");
    let mut bytes = old.data().to_vec();
    bytes[16 + 0xC006] = 0x01; // reset's `lda #$00` becomes `lda #$01`
    let new = Binary::parse(bytes).unwrap();
    let d = old.diff_functions(&new);
    assert_eq!((d.changed, d.added.len(), d.removed.len()), (1, 0, 0), "{d:#?}");
    let changed = &d.pairs[0];
    assert_eq!(
        (changed.old.name.as_str(), changed.status, changed.how),
        ("reset", PairStatus::Changed, MatchKind::Name)
    );
    let lines = old.diff_function_code(changed.old.address, &new, changed.new.address);
    let differ: Vec<_> = lines
        .iter()
        .filter(|l| l.kind != binviz::fndiff::LineKind::Same)
        .map(|l| {
            (
                l.old.as_ref().map(|i| i.operands.clone()),
                l.new.as_ref().map(|i| i.operands.clone()),
            )
        })
        .collect();
    assert_eq!(differ, [(Some("#$00".to_string()), Some("#$01".to_string()))]);

    // A build and its stripped copy: the same code, found by its bytes.
    let full = open("shapes-pe.exe");
    let stripped = open("shapes-pe.stripped.exe");
    let d = full.diff_functions(&stripped);
    let by_bytes = d.pairs.iter().filter(|p| p.how == MatchKind::Bytes).count();
    assert!(by_bytes > 20, "{} pairs by bytes of {}", by_bytes, d.pairs.len());
    assert_eq!(
        d.changed,
        0,
        "{:?}",
        d.pairs
            .iter()
            .filter(|p| p.status == PairStatus::Changed)
            .map(|p| &p.old.name)
            .collect::<Vec<_>>()
    );
}

#[test]
fn jump_tables_are_followed() {
    // NROM: a dispatcher that pushes a table's entry and returns into it (the 6502's return trick).
    let mut prg = vec![0u8; 0x8000];
    prg[0..11].copy_from_slice(&[0xA2, 0x00, 0xBD, 0x20, 0x80, 0x48, 0xBD, 0x10, 0x80, 0x48, 0x60]);
    // lo at $8010, hi at $8020: $8030 and $8040, minus one.
    prg[0x10..0x12].copy_from_slice(&[0x2F, 0x3F]);
    prg[0x20..0x22].copy_from_slice(&[0x80, 0x80]);
    prg[0x30..0x33].copy_from_slice(&[0xA9, 0x01, 0x60]);
    prg[0x40..0x43].copy_from_slice(&[0xA9, 0x02, 0x60]);
    prg[0x7FFA..].copy_from_slice(&[0x00, 0x80, 0x00, 0x80, 0x00, 0x80]);
    let mut rom = b"NES\x1a".to_vec();
    rom.extend_from_slice(&[2, 0, 0, 0]);
    rom.extend_from_slice(&[0; 8]);
    rom.extend_from_slice(&prg);
    let bin = Binary::parse(rom).unwrap();
    let found: Vec<u64> = functions(&bin).into_iter().map(|f| f.0).collect();
    assert!(found.contains(&0x8030) && found.contains(&0x8040), "{found:x?}");
    // The table itself isn't code.
    assert!(!found.iter().any(|&a| (0x8010..0x8030).contains(&a)), "{found:x?}");
}

#[test]
fn playstation_discs_open_their_executable() {
    // A raw disc (2352-byte sectors, Mode 2): SYSTEM.CNF boots the executable in DATA.
    let exe = fixture("tiny-psx.exe");
    let mut img = vec![0u8; 2352 * (40 + exe.len() / 2048)];
    let put = |img: &mut Vec<u8>, lba: usize, bytes: &[u8]| {
        for (i, chunk) in bytes.chunks(2048).enumerate() {
            let at = (lba + i) * 2352 + 24;
            img[at..at + chunk.len()].copy_from_slice(chunk);
        }
    };
    let rec = |lba: u32, size: u32, dir: bool, name: &[u8]| {
        let mut r = vec![0u8; 33 + name.len() + (name.len() + 1) % 2];
        r[0] = r.len() as u8;
        r[2..6].copy_from_slice(&lba.to_le_bytes());
        r[10..14].copy_from_slice(&size.to_le_bytes());
        r[25] = if dir { 2 } else { 0 };
        r[32] = name.len() as u8;
        r[33..33 + name.len()].copy_from_slice(name);
        r
    };
    let mut pvd = vec![0u8; 2048];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[40..44].copy_from_slice(b"GAME");
    let root = rec(20, 2048, true, &[0]);
    pvd[156..156 + root.len()].copy_from_slice(&root);
    put(&mut img, 16, &pvd);
    let mut dir = rec(20, 2048, true, &[0]);
    dir.extend(rec(20, 2048, true, &[1]));
    dir.extend(rec(22, 36, false, b"SYSTEM.CNF;1"));
    dir.extend(rec(24, exe.len() as u32, false, b"SLUS_999.99;1"));
    put(&mut img, 20, &dir);
    put(&mut img, 22, b"BOOT = cdrom:\\SLUS_999.99;1\r\n");
    put(&mut img, 24, &exe);
    assert!(binviz::Container::is_container(&img));
    let c = binviz::Container::parse(img).unwrap();
    assert!(c.info().kind.starts_with("CD image (GAME"), "{}", c.info().kind);
    let first = &c.members()[0];
    assert_eq!(
        (first.name.as_str(), first.arch.as_deref(), first.contiguous),
        ("SLUS_999.99", Some("boots first"), false)
    );
    let bin = c.open(0).unwrap();
    assert_eq!(bin.platform(), Some(binviz::rom::Platform::PlayStation));
    assert_eq!(bin.data(), &exe[..]);
}
