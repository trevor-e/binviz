//! Command-line front end for binviz.

use std::process::ExitCode;

use binviz::{Binary, Container, SymbolQuery, Target};

const USAGE: &str = "\
binviz — explain every byte and address of ELF, Mach-O and PE binaries

USAGE:
    binviz <command> <file> [args]

A folder or a zip (an .ipa, an .app, a build…) works as the <file> too, and
stands for the binaries in it, each paired with its debug file (dSYM, .debug, PDB):
info lists them all and sums their code by owner, search searches them all,
info json describes the folder, and every other command works on the first
binary (an app's own executable) or the one --member names. A CD image opens
the executable the disc boots.

COMMANDS:
  The file
    info <file> [json]             Summary, sections and segments (json: as JSON)
    layout <file> [depth]          File layout tree (default depth 2)
    inspect <file> <address|@offset>
                                   Everything known about an address, or the byte at a file offset
    check <file>                   Verify that the layout covers every byte
  Names and code
    symbols <file> [filter]        Symbols, optionally filtered by name
    strings <file> [filter]        Printable strings in the data sections
    search <file> <query> [kind]   Search addresses, offsets (@0x..), names, byte
                                   patterns (48 8b ?? 08), \"text\", file:line
    disasm <file> <addr|symbol> [n]
                                   Disassemble a function with source lines
    func <file> <addr|symbol>      Callers, callees, strings and data of a function
    refs <file> <addr|symbol> [from]
                                   References to an address, symbol or string (from: the
                                   references a function or data makes)
    calls <file> <addr|symbol> [up] [down]
                                   The call graph around a function; `callers` or `callees`
                                   in place of up and down lists just those
    calls <file> <from> to <to>    A shortest chain of calls between two functions
    coverage <file>                How much of the code and data is mapped out
    objc <file> [name]             Objective-C classes, categories and protocols; with a name,
                                   one declared as its header would, or a selector's
                                   implementations and the functions that send it
  Debug info
    dwarf <file>                   DWARF summary and compilation units
    dwarf <file> check             Everything in the DWARF that can't be read or doesn't add up
    dwarf <file> find <query> [all]
                                   DIEs by name (all: locals and parameters too)
    dwarf <file> die <unit> [offset] | offset <offset>
                                   A DIE's children or details; the DIE at a .debug_info offset
    dwarf <file> list <unit> [tags] [name]
                                   A unit's DIEs by tag (functions, variables, types, DW_TAG_…)
    dwarf <file> at <addr|symbol>  Scopes and variables in scope at an address
    dwarf <file> lines <unit> [first] [count] | sources | file <id>
                                   A unit's line table; the source files; one file's lines
    attribution <file> [unit] [id] Code and data per source file (or unit); with an id,
                                   the address ranges of that one
    crash <file> <report>          Symbolicate a crash report (Apple .crash or .ips, Android
                                   tombstone, a stack trace) with a binary or a folder's binaries
  Two versions
    diff <old> <new>               What changed in size between two builds: two binaries, or
                                   two folders or zips (files, owners, symbols)
    diff <old> <new> functions [name]
                                   Which functions of two builds (or ROM revisions) are which:
                                   identical, relocated, changed, added, removed; with a
                                   function, its instructions and its match's lined up
    patch <file> <patch> [out]     What an IPS, UPS or BPS patch changes, placed in banks,
                                   functions and regions; out: the patched file
    patch <old> <new> <out.ips|ups|bps>
                                   Write the patch that turns old into new
  Games
    relsearch <file> <word> [16] [tbl]
                                   Relative search: a word in an encoding of the file's own
                                   (A = $80, say), found by the spacing of its letters; 16 for
                                   2-byte characters; tbl prints the table it implies
    text <file> <table.tbl> [offset [length] | text]
                                   Text read with a table file: all of it, at an offset, or
                                   where some text is
    labels <rom> <file | format>   An emulator's label file (Mesen .mlb, FCEUX .nl, a .sym:
                                   RGBDS, WLA DX, no$gba) as notes, JSON for --notes; or with a
                                   format (mlb, nl, sym, nocash), the --notes as that label file
  Decompilation (MIPS: PlayStation, Nintendo 64)
    signature <file> <addr|symbol> What a function's code says about its prototype: register
                                   and stack arguments, return, frame, saved registers, the
                                   structures it walks
    context <file> <addr|symbol> [n]
                                   Everything needed to write a function's C: its code with
                                   names, its signature, callers and callees with theirs,
                                   strings, globals, notes
    match <file> <object.o> [name] The compiler's object file scored against the original,
                                   function by function (relocations masked), each difference
                                   explained; with a name, that function only
    report <file> <report.json>    objdiff's report placed on the file's functions
    splat <file> <name> [dir] [split...]
                                   A splat config and symbol_addrs.txt (in dir, or printed)
                                   for a PS-X EXE, the code split into units at the splits
    splat <file> import <symbol_addrs.txt>
                                   A splat symbol file as notes, JSON for --notes
    sdk <file> <lib|folder...> [notes]
                                   The Psy-Q SDK's functions in the file, found by the
                                   signatures of its .LIB/.OBJ files; notes: as notes JSON
    locate <ram.bin> <file>        Where a file from the disc (an overlay) sits in a PlayStation
                                   memory image
    names <file> <candidates.json> [min%]
                                   Names from another build (each function with the strings
                                   it uses and the functions it calls) proposed for the
                                   file's functions; min%: as notes JSON for those at or
                                   above that confidence

Options: --debug <file>  load debug info from a separate file (dSYM, .debug,
                         PDB), or for a Mach-O binary linked without dsymutil,
                         from the folder holding the object files its debug
                         map names
         --member <n>    pick a slice/member of a universal binary or archive, or
                         a binary of a folder (its name, path or number)
         --notes <file>  load annotations (a JSON array) first
         --log <file>    a game ROM: follow its code with an emulator's code/data log
                         (FCEUX's or Mesen's .cdl): the code the game ran, the data it read
         --psx-exe <file> a PlayStation memory image (2 MiB of RAM dumped by an emulator)
                         or overlay: name the functions of this boot executable in it
         --overlay-at <addr> open the file as a PlayStation overlay loaded at this address
         --trace <file>  a PlayStation image: follow the code an emulator's trace saw run
                         (any text with an address per line: a CPU trace, a list of PCs)
Numbers accept decimal or 0x-prefixed hex; banked ROMs' addresses bank:address (03:C000).";

fn num(s: &str) -> Result<u64, String> {
    let r = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16),
        None => s.parse(),
    };
    r.map_err(|_| format!("not a number: {s}"))
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut take_opt = |name: &str| -> Option<String> {
        let i = args.iter().position(|a| a == name)?;
        args.remove(i);
        (i < args.len()).then(|| args.remove(i))
    };
    let debug = take_opt("--debug");
    let member = take_opt("--member");
    let notes = take_opt("--notes");
    let log = take_opt("--log");
    let psx_exe = take_opt("--psx-exe");
    let overlay_at = take_opt("--overlay-at");
    let trace = take_opt("--trace");
    if args.len() < 2 {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    match run(
        &args,
        debug.as_deref(),
        member.as_deref(),
        notes.as_deref(),
        log.as_deref(),
        Psx {
            exe: psx_exe.as_deref(),
            overlay_at: overlay_at.as_deref(),
            trace: trace.as_deref(),
        },
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn open(path: &str, debug: Option<&str>, member: Option<&str>) -> Result<Binary, String> {
    let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let mut bin = if Container::is_container(&data) {
        let c = Container::parse(data).map_err(|e| e.to_string())?;
        // A disc opens the executable it boots, unless another file is asked for.
        let boots = c
            .members()
            .iter()
            .find(|m| m.arch.as_deref() == Some("boots first"))
            .map(|m| m.index);
        let index = match member {
            Some(m) => num(m)? as u32,
            None if boots.is_some() => {
                let i = boots.unwrap();
                eprintln!(
                    "{}: opening {}, which it boots (--member picks another)",
                    c.info().kind,
                    c.members()[i as usize].name
                );
                i
            }
            None => {
                eprintln!("{} with {} members:", c.info().kind, c.members().len());
                for m in c.members() {
                    eprintln!("  [{}] {} ({} bytes at {:#x})", m.index, m.name, m.size, m.offset);
                }
                return Err("pick one with --member <n>".into());
            }
        };
        c.open(index).map_err(|e| e.to_string())?
    } else {
        Binary::parse(data).map_err(|e| e.to_string())?
    };
    let mut folders = Vec::new();
    if let Some(d) = debug {
        if std::path::Path::new(d).is_dir() {
            folders.push(std::path::PathBuf::from(d));
        } else {
            let bytes = std::fs::read(d).map_err(|e| format!("{d}: {e}"))?;
            bin.attach_debug_file(d, bytes).map_err(|e| e.to_string())?;
        }
    }
    // Built without dsymutil, a Mach-O binary's DWARF is in the objects its debug map names.
    if bin.debug_info().is_none() && !bin.debug_map().is_empty() {
        if let Some(dir) = std::path::Path::new(path).parent() {
            folders.push(dir.to_path_buf());
        }
        debug_map_note(bin.attach_debug_map_from_disk(&folders));
    }
    Ok(bin)
}

/// A PlayStation memory image or overlay, its functions named after the
/// boot executable's.
fn open_psx(path: &str, psx: Psx<'_>, notes: Option<&str>) -> Result<Binary, String> {
    let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let exe = match psx.exe {
        Some(e) => {
            let mut exe = open(e, None, None)?;
            // The notes are the executable's too: the same addresses.
            if let Some(n) = notes {
                let text = std::fs::read_to_string(n).map_err(|e| format!("{n}: {e}"))?;
                let list: Vec<binviz::Annotation> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                exe.set_annotations(list);
            }
            Some(exe)
        }
        None => None,
    };
    let bin = match psx.overlay_at {
        Some(at) => Binary::parse_psx_overlay(data, num(at)?, exe.as_ref()),
        None => Binary::parse_psx_memory(data, exe.as_ref()),
    }
    .map_err(|e| format!("{path}: {e}"))?;
    if let Some(e) = &exe {
        eprintln!(
            "{}: {} functions named after {}",
            bin.summary().format_name,
            e.symbols().functions().count(),
            psx.exe.unwrap_or("")
        );
    }
    Ok(bin)
}

/// Says how linking a debug map's DWARF went.
fn debug_map_note(result: binviz::Result<binviz::dwarf::debugmap::DebugMapReport>) {
    match result {
        Ok(r) => eprintln!("{}", r.to_text()),
        Err(e) => eprintln!("debug map: {e} (--debug <folder> says where the objects are)"),
    }
}

/// The commands as written (`dwarf <file> find main`) as their handlers are
/// named (`dwarf-find`); the older names still work.
fn internal(args: &[String]) -> Vec<String> {
    let mut a = args.to_vec();
    let at = |a: &[String], i: usize| a.get(i).cloned();
    match (at(&a, 0).as_deref(), at(&a, 2).as_deref(), at(&a, 3).as_deref()) {
        (Some("inspect"), Some(t), _) if t.starts_with('@') => {
            a[0] = "at".into();
            a[2] = t[1..].to_string();
        }
        (Some("refs"), _, Some("from")) => {
            a[0] = "refs-from".into();
            a.remove(3);
        }
        (Some("refs"), _, _) => a[0] = "xrefs".into(),
        (Some("calls"), _, Some("to")) => {
            a[0] = "callpath".into();
            a.remove(3);
        }
        (Some("calls"), _, Some(w @ ("callers" | "callees"))) => {
            a[0] = w.to_string();
            a.remove(3);
        }
        (Some("calls"), _, _) => a[0] = "callgraph".into(),
        (Some("dwarf"), Some(sub), _) => {
            let handler = match sub {
                "check" => "dwarf-check",
                "find" => "dwarf-find",
                "die" => "die",
                "at" => "dwarf-at",
                "lines" => "lines",
                "list" => "dwarf-list",
                "offset" => "dwarf-offset",
                "sources" => "sources",
                "file" => "file-lines",
                _ => return a,
            };
            a[0] = handler.into();
            a.remove(2);
        }
        // attribution <file> [unit] [id]: an id names one source file or unit.
        (Some("attribution"), _, _) => {
            if let Some(i) = a.iter().skip(2).position(|x| num(x).is_ok()) {
                let id = a.remove(i + 2);
                a[0] = "attributed".into();
                a.insert(2, id);
            }
        }
        (Some("info"), Some("json"), _) => {
            a[0] = "json".into();
            a.truncate(2);
        }
        _ => {}
    }
    a
}

/// The PlayStation options: a memory image or overlay named after a boot
/// executable, a file opened as an overlay, an emulator's trace.
#[derive(Clone, Copy)]
struct Psx<'a> {
    exe: Option<&'a str>,
    overlay_at: Option<&'a str>,
    trace: Option<&'a str>,
}

fn run(
    args: &[String],
    debug: Option<&str>,
    member: Option<&str>,
    notes: Option<&str>,
    log: Option<&str>,
    psx: Psx<'_>,
) -> Result<(), String> {
    let args = &internal(args);
    let cmd = args[0].as_str();
    if cmd == "diff" {
        let new = args.get(2).ok_or("compare with what? binviz diff <old> <new>")?;
        if args.get(3).map(String::as_str) == Some("functions") {
            return diff_functions(&args[1], new, args.get(4).map(String::as_str));
        }
        return diff(&args[1], new);
    }
    if cmd == "crash" {
        let report = args.get(2).ok_or("which crash report? binviz crash <file> <report>")?;
        return crash(&args[1], report, debug, member);
    }
    if cmd == "patch" {
        let second = args
            .get(2)
            .ok_or("which patch? binviz patch <file> <patch> [out], or binviz patch <old> <new> <out.ips>")?;
        return patch(&args[1], second, args.get(3).map(String::as_str));
    }
    // These read any file's bytes, whatever it is.
    if cmd == "relsearch" {
        let word = args
            .get(2)
            .ok_or("which word? binviz relsearch <file> <word> [16] [tbl]")?;
        return relsearch(&args[1], word, &args[3..]);
    }
    if cmd == "text" {
        let table = args
            .get(2)
            .ok_or("which table? binviz text <file> <table.tbl> [offset [length] | text]")?;
        return table_text(&args[1], table, &args[3..]);
    }
    let path = std::path::Path::new(&args[1]);
    let mut bin = if binviz::package::is_package_path(path) {
        let mut pkg = binviz::package::DiskPackage::open(path)?;
        if pkg.info.binaries.is_empty() {
            return Err(format!("no binaries in {} ({} files)", args[1], pkg.info.files));
        }
        // The folder as a whole, unless one binary was asked for.
        if member.is_none() {
            match cmd {
                "info" => return folder_info(&mut pkg),
                "search" => return folder_search(&mut pkg, &args[2..]),
                "json" => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&pkg.info).map_err(|e| e.to_string())?
                    );
                    return Ok(());
                }
                _ => {}
            }
        }
        folder_binary(&mut pkg, member, debug)?
    } else if psx.exe.is_some() || psx.overlay_at.is_some() {
        open_psx(&args[1], psx, notes)?
    } else {
        open(&args[1], debug, member)?
    };
    if let Some(path) = psx.trace {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let (traced, s) = bin.with_psx_trace(&text).map_err(|e| format!("{path}: {e}"))?;
        eprintln!(
            "trace: {} lines, {} addresses, {} in this image, {} new runs of code followed",
            s.lines, s.addresses, s.placed, s.new_runs
        );
        bin = traced;
    }
    // Swift names read better through `swift-demangle`, where it is installed.
    bin.demangle_swift_with_tool();
    if let Some(path) = log {
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        let (logged, s) = bin.with_code_log(&bytes).map_err(|e| format!("{path}: {e}"))?;
        eprintln!(
            "{} code/data log: {} bytes of code, {} of data{}{}",
            s.format.name(),
            s.code,
            s.data,
            if s.pages_placed > 0 {
                format!(", {} PRG pages placed where they ran", s.pages_placed)
            } else {
                String::new()
            },
            if s.crc_matches == Some(false) {
                " (made for another version of the ROM)"
            } else {
                ""
            }
        );
        bin = logged;
    }
    if let Some(path) = notes {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let list: Vec<binviz::Annotation> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        bin.set_annotations(list);
    }
    let bin = bin;
    let arg = |i: usize| args.get(i).map(String::as_str);
    match cmd {
        "info" => info(&bin),
        "layout" => {
            let depth = arg(2).map(num).transpose()?.unwrap_or(2) as usize;
            layout(&bin, None, 0, depth);
        }
        "at" => {
            let off = num(arg(2).ok_or("missing offset")?)?;
            print_inspection(&bin, &bin.inspect(Target::Offset(off)));
        }
        "inspect" => {
            let addr = resolve_address(&bin, arg(2).ok_or("missing address")?)?;
            print_inspection(&bin, &bin.inspect(Target::Address(addr)));
        }
        "symbols" => {
            let page = bin.symbols().query(&SymbolQuery {
                filter: arg(2).unwrap_or("").into(),
                limit: 100_000,
                ..Default::default()
            });
            for s in &page.symbols {
                println!(
                    "{:#018x} {:>8} {:<9} {:<8} {:<7} {}",
                    s.address,
                    s.size,
                    format!("{:?}", s.kind).to_lowercase(),
                    s.binding,
                    format!("{:?}", s.source).to_lowercase(),
                    s.display_name()
                );
            }
            eprintln!("{} of {} symbols", page.symbols.len(), bin.symbols().len());
        }
        "disasm" => {
            let addr = resolve_address(&bin, arg(2).ok_or("missing address or symbol")?)?;
            let n = arg(3).map(num).transpose()?.unwrap_or(400) as usize;
            let d = bin.disassemble_function(addr, n);
            if !d.supported {
                return Err(format!("no disassembler for {}", bin.summary().arch));
            }
            if let Some(f) = &d.function {
                println!(
                    "{} <{}>:",
                    fmt_addr(f.address),
                    f.demangled.as_deref().unwrap_or(&f.name)
                );
            }
            let mut last_line = None;
            for i in &d.instructions {
                if let Some(s) = &i.source
                    && last_line != Some((s.file, s.line))
                {
                    println!("    ; {}:{}", s.path, s.line);
                    last_line = Some((s.file, s.line));
                }
                let target = i.target_symbol.as_ref().map(|t| format!("  <{t}>")).unwrap_or_default();
                println!(
                    "  {:>12x}:  {:<30} {:<8} {}{target}",
                    i.address, i.bytes, i.mnemonic, i.operands
                );
            }
        }
        "dwarf" => {
            let d = bin.debug_info().ok_or("no DWARF")?;
            let s = d.summary();
            println!(
                "source: {}\nversions: {:?}\nunits: {}",
                s.source, s.versions, s.unit_count
            );
            for sec in &s.sections {
                println!("  {:<18} {:>10}", sec.name, sec.size);
            }
            for p in &s.producers {
                println!("producer: {p}");
            }
            for u in d.units().iter().take(200) {
                println!(
                    "[{}] {} v{} {:#x} {} {} ({} bytes of code, {} ranges)",
                    u.index,
                    u.kind,
                    u.version,
                    u.offset,
                    u.language.as_deref().unwrap_or("?"),
                    u.name.as_deref().unwrap_or("?"),
                    u.code_size,
                    u.ranges.len()
                );
            }
        }
        "die" => {
            let d = bin.debug_info().ok_or("no DWARF")?;
            let unit = num(arg(2).ok_or("missing unit")?)? as u32;
            match arg(3).map(num).transpose()? {
                None => {
                    if let Some(root) = d.unit_root(unit) {
                        println!(
                            "{} {} {}",
                            root.tag,
                            root.name.unwrap_or_default(),
                            root.detail.unwrap_or_default()
                        );
                    }
                    for c in d.die_children(unit, None) {
                        println!(
                            "  <{:#x}> {} {} {}",
                            c.offset,
                            c.tag,
                            c.name.unwrap_or_default(),
                            c.detail.unwrap_or_default()
                        );
                    }
                }
                Some(off) => {
                    let det = d.die(unit, off).ok_or("no such DIE")?;
                    for p in &det.parents {
                        println!("in {} {}", p.tag, p.name.clone().unwrap_or_default());
                    }
                    println!(
                        "<{:#x}> {} {}",
                        det.die.offset,
                        det.die.tag,
                        det.die.name.clone().unwrap_or_default()
                    );
                    for a in &det.attributes {
                        println!(
                            "  {:<26} {:<20} {}   @{:#x}",
                            a.name,
                            a.form,
                            a.value.replace('\n', "\n  "),
                            a.byte_start
                        );
                    }
                    if let Some(t) = &det.type_name {
                        println!("  type: {t}");
                    }
                    if let Some(l) = &det.decl {
                        println!("  declared at {}:{}", l.path, l.line);
                    }
                    if let Some(l) = &det.call_site {
                        println!("  inlined at {}:{}:{}", l.path, l.line, l.column);
                    }
                    for l in &det.code_lines {
                        println!(
                            "  code from {}:{} ({} bytes in {} rows, first {:#x})",
                            l.path, l.line, l.bytes, l.rows, l.first
                        );
                    }
                    for m in &det.layout {
                        println!(
                            "  {:>6} {:>5} {:<6} {:<24} {}{}",
                            m.offset.map_or("-".into(), |o| format!("{o:#x}")),
                            m.size
                                .map_or_else(|| m.bit_size.map_or("?".into(), |b| format!("{b}b")), |s| s.to_string()),
                            m.kind,
                            m.name.as_deref().unwrap_or(""),
                            m.type_name,
                            if m.hole > 0 {
                                format!("   <- {} byte hole before", m.hole)
                            } else {
                                String::new()
                            }
                        );
                    }
                    if let Some(t) = det.tail_padding.filter(|&t| t > 0) {
                        println!("  {t} bytes of padding at the end");
                    }
                    println!("  {} children", det.child_count);
                    for c in d.die_children(unit, Some(off)) {
                        println!(
                            "    <{:#x}> {} {} {}",
                            c.offset,
                            c.tag,
                            c.name.unwrap_or_default(),
                            c.detail.unwrap_or_default()
                        );
                    }
                }
            }
        }
        "lines" => {
            let d = bin.debug_info().ok_or("no DWARF")?;
            let unit = num(arg(2).ok_or("missing unit")?)? as u32;
            let first = arg(3).map(num).transpose()?.unwrap_or(0) as u32;
            let count = arg(4).map(num).transpose()?.unwrap_or(50) as u32;
            if let Some(p) = d.line_program(unit) {
                println!(
                    "line program v{} at {:#x}, {} rows, {} files",
                    p.version,
                    p.offset,
                    p.row_count,
                    p.files.len()
                );
            }
            for r in d.line_rows(unit, first, count) {
                let file = r
                    .file
                    .and_then(|f| d.source_files().get(f as usize))
                    .map_or("?", |f| f.name.as_str());
                println!("{:#x} {file}:{}:{} flags={:#x}", r.address, r.line, r.column, r.flags);
            }
        }
        "sources" => {
            let d = bin.debug_info().ok_or("no DWARF")?;
            for f in d.source_files() {
                println!("[{}] {}", f.id, f.path);
            }
        }
        "file-lines" => {
            let d = bin.debug_info().ok_or("no DWARF")?;
            let id = num(arg(2).ok_or("missing file id")?)? as u32;
            for l in d.file_lines(id) {
                println!(
                    "{:>5}:{:<3} {:#x}..{:#x}{}",
                    l.line,
                    l.column,
                    l.start,
                    l.end,
                    if l.is_stmt { "" } else { " (not stmt)" }
                );
            }
        }
        "dwarf-check" => {
            let d = bin.debug_info().ok_or("no DWARF")?;
            let t = std::time::Instant::now();
            let c = d.check();
            println!(
                "{} units, {} DIEs, {} line rows checked in {:.0?}: {} errors, {} warnings",
                c.units,
                c.dies,
                c.line_rows,
                t.elapsed(),
                c.errors,
                c.warnings
            );
            for p in &c.problems {
                println!(
                    "  {:?} [{}] unit {} DIE {} @ {} {}: {}",
                    p.severity,
                    p.area,
                    p.unit.map_or("-".into(), |u| u.to_string()),
                    p.die.map_or("-".into(), |o| format!("{o:#x}")),
                    p.section,
                    p.offset.map_or("-".into(), |o| format!("{o:#x}")),
                    p.message
                );
            }
        }
        "dwarf-list" => {
            let d = bin.debug_info().ok_or("no DWARF")?;
            let unit = num(arg(2).ok_or("missing unit")?)? as u32;
            let page = d.list_dies(unit, arg(3).unwrap_or(""), arg(4).unwrap_or(""), 0, 100);
            println!("{} DIEs", page.total);
            for x in &page.dies {
                println!(
                    "  <{:#x}> {:<28} {}{} {}",
                    x.section_offset,
                    x.tag,
                    x.scope.as_deref().map(|s| format!("{s}::")).unwrap_or_default(),
                    x.name.as_deref().unwrap_or(""),
                    x.detail.as_deref().unwrap_or("")
                );
            }
            for t in d.tag_counts(unit).iter().take(12) {
                println!("  {:>7} {}", t.count, t.tag);
            }
        }
        "dwarf-find" => {
            let d = bin.debug_info().ok_or("no DWARF")?;
            let q = arg(2).ok_or("missing query")?;
            let t = std::time::Instant::now();
            let hits = if arg(3) == Some("all") {
                d.search_all(q, 50)
            } else {
                d.search(q, 50)
            };
            eprintln!("{} hits in {:.0?}", hits.len(), t.elapsed());
            for x in &hits {
                println!(
                    "  [{}] <{:#x}> {:<24} {}{} {}",
                    x.unit,
                    x.section_offset,
                    x.tag,
                    x.scope.as_deref().map(|s| format!("{s}::")).unwrap_or_default(),
                    x.name.as_deref().unwrap_or(""),
                    x.detail.as_deref().unwrap_or("")
                );
            }
        }
        "dwarf-offset" => {
            let d = bin.debug_info().ok_or("no DWARF")?;
            let off = num(arg(2).ok_or("missing offset")?)?;
            let (unit, die) = d.die_at_offset(off).ok_or("no unit covers that offset")?;
            let det = d.die(unit, die).ok_or("unreadable DIE")?;
            println!(
                "unit {unit}, DIE <{:#x}> (unit offset {die:#x}) {} {}",
                det.die.section_offset,
                det.die.tag,
                det.die.name.clone().unwrap_or_default()
            );
            for m in &det.layout {
                println!(
                    "  {:>6} {:>5} {:<6} {:<24} {}{}",
                    m.offset.map_or("-".into(), |o| format!("{o:#x}")),
                    m.size
                        .map_or_else(|| m.bit_size.map_or("?".into(), |b| format!("{b}b")), |s| s.to_string()),
                    m.kind,
                    m.name.as_deref().unwrap_or(""),
                    m.type_name,
                    if m.hole > 0 {
                        format!("   <- {} byte hole before", m.hole)
                    } else {
                        String::new()
                    }
                );
            }
            if let Some(t) = det.tail_padding.filter(|&t| t > 0) {
                println!("  {t} bytes of padding at the end");
            }
        }
        "dwarf-at" => {
            let d = bin.debug_info().ok_or("no DWARF")?;
            let addr = resolve_address(&bin, arg(2).ok_or("missing address")?)?;
            let s = d.scope_at(addr).ok_or("no DWARF scope covers that address")?;
            for (i, sc) in s.scopes.iter().enumerate() {
                println!(
                    "{:indent$}{} {} {}",
                    "",
                    sc.tag,
                    sc.name.as_deref().unwrap_or(""),
                    sc.detail.as_deref().unwrap_or(""),
                    indent = i * 2
                );
                for v in s.variables.iter().filter(|v| v.scope == i as u32) {
                    println!(
                        "{:indent$}  {} {}: {} = {}",
                        "",
                        v.kind,
                        v.name,
                        v.type_name.as_deref().unwrap_or("?"),
                        v.location,
                        indent = i * 2
                    );
                }
            }
        }
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(bin.summary()).map_err(|e| e.to_string())?
        ),
        "check" => check(&bin)?,
        "search" => print_hits(&search(&bin, &args[2..])?),
        "strings" => {
            let page = bin.strings(arg(2).unwrap_or(""), 0, 200);
            println!("{} strings", page.total);
            for s in page.strings {
                let at = s
                    .address
                    .map_or_else(|| format!("@{:#x}", s.offset), |a| format!("{a:#x}"));
                println!("{at:>12} {}{:?}", if s.wide { "L" } else { " " }, s.text);
            }
        }
        "labels" => {
            use binviz::rom::labels::LabelFormat;
            let what = arg(2)
                .ok_or("which label file, or which format? binviz labels <rom> <file | mlb | nl | sym | nocash>")?;
            match LabelFormat::from_name(what).filter(|_| !std::path::Path::new(what).exists()) {
                Some(format) => {
                    if bin.annotations().is_empty() {
                        eprintln!("no notes to write: give them with --notes <file>");
                    }
                    let files = bin.write_labels(format).map_err(|e| e.to_string())?;
                    if format == LabelFormat::Nl {
                        // FCEUX reads its name lists from next to the ROM, one per bank.
                        for f in &files {
                            let out = format!("{}.{}", args[1], f.suffix);
                            std::fs::write(&out, &f.text).map_err(|e| format!("{out}: {e}"))?;
                            eprintln!("wrote {out}");
                        }
                    } else {
                        for f in &files {
                            print!("{}", f.text);
                        }
                    }
                }
                None => {
                    let text = std::fs::read_to_string(what).map_err(|e| format!("{what}: {e}"))?;
                    let read = bin.read_labels(what, &text).map_err(|e| e.to_string())?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&read.labels).map_err(|e| e.to_string())?
                    );
                    eprintln!(
                        "{} labels from a {} file{}{}",
                        read.labels.len(),
                        read.format.name(),
                        if read.skipped > 0 {
                            format!("; {} skipped (places binviz can't place)", read.skipped)
                        } else {
                            String::new()
                        },
                        if read.directives > 0 {
                            format!("; {} directives", read.directives)
                        } else {
                            String::new()
                        }
                    );
                }
            }
        }
        "coverage" => {
            let c = bin.coverage(15);
            let pct = |n: u64, total: u64| {
                if total == 0 {
                    0.0
                } else {
                    n as f64 * 100.0 / total as f64
                }
            };
            let row = |name: &str, b: &binviz::coverage::StatusBytes, size: u64| {
                println!(
                    "{name:<20} {size:>9}  rev {:>5.1}%  ann {:>5.1}%  named {:>5.1}%  struct {:>5.1}%  recov {:>5.1}%  pad {:>5.1}%  unexpl {:>5.1}%",
                    pct(b.reviewed, size),
                    pct(b.annotated, size),
                    pct(b.named, size),
                    pct(b.structure, size),
                    pct(b.recovered, size),
                    pct(b.padding, size),
                    pct(b.unexplored, size)
                );
            };
            for s in &c.sections {
                row(&s.name, &s.bytes, s.size);
            }
            let total: u64 = c.sections.iter().map(|s| s.size).sum();
            row("TOTAL", &c.totals, total);
            println!(
                "functions: {} named, {} recovered, {} yours; {} annotations ({} reviewed)",
                c.functions.named, c.functions.recovered, c.functions.user, c.annotations, c.reviewed
            );
            println!("{} unexplored gaps; largest:", c.gap_count);
            for g in &c.gaps {
                println!(
                    "  {:#x}..{:#x} {:>8} bytes  {:<14} after {:<40} {}",
                    g.start,
                    g.end,
                    g.end - g.start,
                    g.hint,
                    g.after.as_deref().unwrap_or("-"),
                    g.preview
                );
            }
        }
        "attribution" => {
            let mode = if arg(2) == Some("unit") {
                binviz::AttributionMode::Unit
            } else {
                binviz::AttributionMode::File
            };
            let a = bin.attribution(mode).ok_or("no DWARF")?;
            let name = |i: u32| bin.sections().get(i as usize).map_or("?", |s| s.name.as_str());
            for c in a.contributors.iter().take(40) {
                let shares: Vec<String> = c
                    .sections
                    .iter()
                    .map(|s| format!("{} {}+{}", name(s.section), s.code, s.data))
                    .collect();
                println!(
                    "[{}] {:<40} code {:>8} data {:>7} fns {:>4} vars {:>4}  {}",
                    c.id,
                    c.name,
                    c.code,
                    c.data,
                    c.functions,
                    c.variables,
                    shares.join(", ")
                );
            }
            println!("{} contributors", a.contributors.len());
            for s in &a.sections {
                println!("  {} attributed {}", name(s.section), s.attributed);
            }
        }
        "attributed" => {
            let id = num(arg(2).ok_or("missing id")?)? as u32;
            let mode = if arg(3) == Some("unit") {
                binviz::AttributionMode::Unit
            } else {
                binviz::AttributionMode::File
            };
            for r in bin.attributed_ranges(mode, id).iter().take(200) {
                println!(
                    "{:#x}..{:#x} {} line {} {}",
                    r.start,
                    r.end,
                    if r.data { "data" } else { "code" },
                    r.line,
                    r.label.as_deref().unwrap_or("")
                );
            }
        }
        "xrefs" => {
            let addr = resolve_address(&bin, arg(2).ok_or("missing address or symbol")?)?;
            let (lo, hi) = match bin.symbols().at(addr) {
                Some(s) if s.size > 0 => (addr, addr + s.size),
                _ => (addr, addr + 1),
            };
            let t = std::time::Instant::now();
            bin.prepare_xrefs();
            eprintln!("index: {:?} in {:.0?}", bin.xref_counts(), t.elapsed());
            let page = bin.references_to(lo, hi, 0, 200);
            println!("{} references to {lo:#x}..{hi:#x}: {:?}", page.total, page.counts);
            for r in &page.refs {
                println!(
                    "  {:<8} {:#x} {:<40} -> {:#x} {}",
                    r.kind.as_str(),
                    r.source,
                    r.from.as_deref().unwrap_or("?"),
                    r.target,
                    r.to.as_deref().unwrap_or("")
                );
            }
        }
        "refs-from" => {
            let addr = resolve_address(&bin, arg(2).ok_or("missing address or symbol")?)?;
            let (lo, hi) = match bin.symbols().function_containing(addr) {
                Some(f) => (f.address, f.address + f.size.max(1)),
                None => (addr, addr + 64),
            };
            for r in bin.references_from(lo, hi) {
                println!(
                    "  {:#x} {:<8} {:#x} {}",
                    r.source,
                    r.kind.as_str(),
                    r.target,
                    r.to.as_deref().unwrap_or("")
                );
            }
        }
        "objc" => {
            let objc = bin.objc();
            match arg(2) {
                None => print!("{}", objc.to_text(2000)),
                Some(name) => {
                    if let Some(i) = objc.interface(name) {
                        print!("{}", i.to_text());
                    } else if let Some(uses) = bin.objc_selector(name) {
                        print!("{}", uses.to_text());
                    } else {
                        return Err(format!("no class, category, protocol or selector named {name}"));
                    }
                }
            }
        }
        "callers" | "callees" => {
            let addr = resolve_address(&bin, arg(2).ok_or("missing address or symbol")?)?;
            let list = if cmd == "callers" {
                bin.callers(addr)
            } else {
                bin.callees(addr)
            };
            for e in &list {
                println!(
                    "  {:>4}x {:#x} {:<9} {} (first at {:#x})",
                    e.calls,
                    e.address,
                    format!("{:?}", e.kind).to_lowercase(),
                    e.name,
                    e.site
                );
            }
            eprintln!("{} {cmd}", list.len());
        }
        "callgraph" => {
            let addr = resolve_address(&bin, arg(2).ok_or("missing address or symbol")?)?;
            let up = arg(3).map(num).transpose()?.unwrap_or(1) as u32;
            let down = arg(4).map(num).transpose()?.unwrap_or(2) as u32;
            let g = bin.call_graph(addr, up, down, 12);
            for n in &g.nodes {
                println!(
                    "{:>3} {:#x} {:<9} {}",
                    n.depth,
                    n.address,
                    format!("{:?}", n.kind).to_lowercase(),
                    n.name
                );
            }
            for e in &g.edges {
                println!("  {:#x} -> {:#x} ({}x)", e.from, e.to, e.calls);
            }
            println!("{} hidden", g.hidden);
        }
        "callpath" => {
            let from = resolve_address(&bin, arg(2).ok_or("missing from")?)?;
            let to = resolve_address(&bin, arg(3).ok_or("missing to")?)?;
            match bin.call_path(from, to, 12) {
                Some(steps) => {
                    for s in steps {
                        let site = s.site.map(|a| format!(" (called at {a:#x})")).unwrap_or_default();
                        println!("  {:#x} {}{site}", s.address, s.name);
                    }
                }
                None => println!("no path within 12 calls"),
            }
        }
        "func" => {
            let addr = resolve_address(&bin, arg(2).ok_or("missing address or symbol")?)?;
            let f = bin.function_summary(addr, 30).ok_or("not in a function")?;
            println!(
                "{} at {:#x}, {} bytes; referenced by {:?}",
                f.name, f.address, f.size, f.referenced_by
            );
            println!("{} callers:", f.caller_count);
            for e in &f.callers {
                println!("  {:>4}x {}", e.calls, e.name);
            }
            println!("{} callees:", f.callee_count);
            for e in &f.callees {
                println!("  {:>4}x {}", e.calls, e.name);
            }
            println!("strings:");
            for s in &f.strings {
                println!("  {:#x} {:?}", s.address, s.text);
            }
            println!("data:");
            for r in &f.data {
                println!(
                    "  {:<8} {:#x} {}",
                    r.kind.as_str(),
                    r.target,
                    r.to.as_deref().unwrap_or("")
                );
            }
        }
        "signature" => {
            let addr = resolve_address(&bin, arg(2).ok_or("missing address or symbol")?)?;
            let s = bin
                .function_signature(addr)
                .ok_or("not in a function, or not MIPS code")?;
            print!("{}", s.describe());
        }
        "context" => {
            let addr = resolve_address(&bin, arg(2).ok_or("missing address or symbol")?)?;
            let n = arg(3).map(num).transpose()?.unwrap_or(400) as usize;
            let c = bin.decomp_context(addr, n).ok_or("not in a function")?;
            print!("{}", c.describe());
        }
        "match" => {
            let object = arg(2).ok_or("which object file? binviz match <file> <object.o> [name]")?;
            let bytes = std::fs::read(object).map_err(|e| format!("{object}: {e}"))?;
            match arg(3) {
                Some(name) => {
                    let funcs = binviz::matching::object_functions(&bytes).map_err(|e| e.to_string())?;
                    let f = funcs
                        .iter()
                        .find(|f| f.name == name)
                        .ok_or_else(|| format!("no function {name} in {object}"))?;
                    let addr = resolve_address(&bin, name)?;
                    let m = bin.match_function(addr, f).ok_or("not in a function")?;
                    print!("{}", m.to_text());
                }
                None => {
                    let results = bin.match_object(&bytes).map_err(|e| e.to_string())?;
                    if results.is_empty() {
                        return Err("no function of the object has a name the binary knows".into());
                    }
                    let matched = results.iter().filter(|m| m.percent >= 100.0).count();
                    println!("{} functions compared, {matched} match exactly\n", results.len());
                    for m in &results {
                        print!("{}", m.to_text());
                    }
                }
            }
        }
        "report" => {
            let path = arg(2).ok_or("which report? binviz report <file> <report.json>")?;
            let json = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let report = binviz::matching::ObjdiffReport::parse(&json).map_err(|e| e.to_string())?;
            let p = bin.place_report(&report);
            println!(
                "{:.2}% matched: {} of {} bytes of code, {} of {} functions; {} placed on this file, {} not found",
                p.fuzzy_match_percent,
                p.matched_code,
                p.total_code,
                p.matched_functions,
                p.total_functions,
                p.functions.len(),
                p.unplaced.len()
            );
            for f in &p.functions {
                println!("  {:#x} {:>6.1}%  {}  ({})", f.address, f.percent, f.name, f.unit);
            }
            for u in p.unplaced.iter().take(50) {
                println!("  not placed: {u}");
            }
        }
        "splat" => {
            let second = arg(2).ok_or("binviz splat <file> <name> [dir] [split...], or splat <file> import <symbol_addrs.txt>")?;
            if second == "import" {
                let path = arg(3).ok_or("which symbol file?")?;
                let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
                let notes = binviz::splat::parse_symbol_addrs(&text);
                eprintln!("{} names", notes.len());
                println!("{}", serde_json::to_string_pretty(&notes).map_err(|e| e.to_string())?);
            } else {
                let splits: Vec<u64> = args.get(4..).unwrap_or(&[]).iter().map(|a| num(a)).collect::<Result<_, _>>()?;
                let e = bin.splat_export(second, &splits).map_err(|e| e.to_string())?;
                match arg(3) {
                    Some(dir) => {
                        let dir = std::path::Path::new(dir);
                        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                        let yaml = dir.join(format!("{second}.yaml"));
                        std::fs::write(&yaml, &e.config).map_err(|e| e.to_string())?;
                        std::fs::write(dir.join("symbol_addrs.txt"), &e.symbol_addrs).map_err(|e| e.to_string())?;
                        println!(
                            "wrote {} and symbol_addrs.txt ({} functions, {} data symbols)",
                            yaml.display(),
                            e.functions,
                            e.data_symbols
                        );
                    }
                    None => {
                        print!("{}", e.config);
                        println!("--- symbol_addrs.txt ---");
                        print!("{}", e.symbol_addrs);
                    }
                }
            }
        }
        "sdk" => {
            let mut sigs = binviz::rom::psyq::SignatureSet::default();
            let mut paths: Vec<std::path::PathBuf> = Vec::new();
            let mut as_notes = false;
            for a in args.get(2..).unwrap_or(&[]) {
                if a == "notes" {
                    as_notes = true;
                    continue;
                }
                let path = std::path::Path::new(a);
                if path.is_dir() {
                    let mut found: Vec<_> = std::fs::read_dir(path)
                        .map_err(|e| format!("{a}: {e}"))?
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| {
                            p.extension()
                                .and_then(|e| e.to_str())
                                .is_some_and(|e| e.eq_ignore_ascii_case("lib") || e.eq_ignore_ascii_case("obj"))
                        })
                        .collect();
                    found.sort();
                    paths.extend(found);
                } else {
                    paths.push(path.to_path_buf());
                }
            }
            if paths.is_empty() {
                return Err("which libraries? binviz sdk <file> <lib|folder...> [notes]".into());
            }
            for path in &paths {
                let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
                match sigs.add_file(&path.to_string_lossy(), &bytes) {
                    Ok(n) => eprintln!("{}: {n} signatures", path.display()),
                    Err(e) => eprintln!("{}: {e}", path.display()),
                }
            }
            let r = bin.identify_sdk(&sigs);
            if as_notes {
                println!("{}", serde_json::to_string_pretty(&r.annotations()).map_err(|e| e.to_string())?);
            } else {
                print!("{}", r.to_text());
            }
        }
        "locate" => {
            let path = arg(2).ok_or("which file? binviz locate <ram.bin> <file>")?;
            let blob = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            if !bin.is_psx_memory() {
                return Err("locate is for PlayStation memory images (2 MiB of RAM dumped by an emulator)".into());
            }
            match bin.psx_locate(&blob) {
                Some(at) => println!("{path} is loaded at {at:#x} ({} bytes)", blob.len()),
                None => println!("{path} is not in this image"),
            }
        }
        "names" => {
            let path = arg(2).ok_or("which candidates? binviz names <file> <candidates.json> [min%]")?;
            let json = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let candidates = binviz::names::parse_candidates(&json).map_err(|e| e.to_string())?;
            let p = bin.propose_names(&candidates);
            match arg(3) {
                Some(min) => {
                    let min = num(min.trim_end_matches('%'))? as f32 / 100.0;
                    println!("{}", serde_json::to_string_pretty(&p.annotations(min)).map_err(|e| e.to_string())?);
                }
                None => print!("{}", p.to_text()),
            }
        }
        _ => return Err(format!("unknown command {cmd}\n\n{USAGE}")),
    }
    Ok(())
}

fn resolve_address(bin: &Binary, s: &str) -> Result<u64, String> {
    if let Ok(n) = num(s) {
        return Ok(n);
    }
    // A ROM's bank:address (03:C000).
    if let Some(a) = bin.rom_address(s) {
        return Ok(a);
    }
    bin.symbols()
        .by_name(s)
        .map(|s| s.address)
        .ok_or_else(|| format!("no symbol named {s}"))
}

fn fmt_addr(a: u64) -> String {
    format!("{a:#x}")
}

/// Applies a patch and says what it changes, or makes one from two files.
fn patch(file: &str, second: &str, out: Option<&str>) -> Result<(), String> {
    use binviz::patch::{ChangeKind, PatchFormat};
    let read = |p: &str| std::fs::read(p).map_err(|e| format!("{p}: {e}"));
    let data = read(file)?;
    let other = read(second)?;
    if PatchFormat::detect(&other).is_none() {
        // Two files: the patch from the first to the second.
        let out = out.ok_or("where to write the patch? binviz patch <old> <new> <out.ips|ups|bps>")?;
        let format = std::path::Path::new(out)
            .extension()
            .and_then(|e| PatchFormat::from_name(&e.to_string_lossy()))
            .ok_or("name the patch .ips, .ups or .bps")?;
        let bytes = binviz::patch::create(format, &data, &other).map_err(|e| e.to_string())?;
        std::fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
        let changes = binviz::patch::changes(&data, &other);
        eprintln!(
            "wrote {out}: {} patch, {} bytes, {} changes",
            format.name(),
            bytes.len(),
            changes.len()
        );
        return Ok(());
    }
    let applied = binviz::patch::apply(&other, &data).map_err(|e| format!("{second}: {e}"))?;
    let i = &applied.info;
    println!(
        "{} patch: {} records, makes {} bytes{}",
        i.format.name(),
        i.records,
        i.target_size,
        match (applied.source_matches, applied.target_matches) {
            (Some(true), Some(true)) => " (made for this file; the result is as promised)",
            (Some(false), _) => " (made for another file)",
            (Some(true), Some(false)) => " (the result isn't what it promises)",
            _ => "",
        }
    );
    if let Some(m) = &i.metadata {
        println!("metadata: {m}");
    }
    for w in &applied.warnings {
        println!("warning: {w}");
    }
    let bin = Binary::parse(data.clone()).unwrap_or_else(|_| Binary::raw(data.clone()));
    let placed = binviz::patch::place(&bin, &applied.changes);
    println!("{} changes, {} bytes differ:", applied.changes.len(), applied.differ);
    for p in placed.iter().take(2000) {
        let c = &p.change;
        let at = p.address.map(|a| format!(" {a:#x}")).unwrap_or_default();
        let kind = match c.kind {
            ChangeKind::Changed => String::new(),
            ChangeKind::Added => " added".into(),
            ChangeKind::Removed => " removed".into(),
        };
        println!(
            "  {:#010x} {:>6} bytes{kind}  {}{at}{}{}",
            c.offset,
            c.len,
            p.section.as_deref().unwrap_or("-"),
            p.function.as_ref().map(|f| format!(" in {f}")).unwrap_or_default(),
            if p.region.is_empty() {
                String::new()
            } else {
                format!("  [{}]", p.region.join(" > "))
            }
        );
    }
    if placed.len() > 2000 {
        println!("  … {} more", placed.len() - 2000);
    }
    if let Some(out) = out {
        std::fs::write(out, &applied.output).map_err(|e| format!("{out}: {e}"))?;
        eprintln!("wrote the patched file to {out}");
    }
    Ok(())
}

/// `relsearch`: a word in an encoding of the file's own.
fn relsearch(path: &str, word: &str, rest: &[String]) -> Result<(), String> {
    let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let width = if rest.iter().any(|a| a == "16") { 2 } else { 1 };
    let found = binviz::tables::relative_search(&data, word, width, 50)?;
    if rest.iter().any(|a| a == "tbl") {
        let e = found
            .encodings
            .first()
            .ok_or_else(|| format!("{word:?} is nowhere in {path}, in any encoding"))?;
        print!(
            "{}",
            binviz::tables::Table::from_alphabet(e.first, e.letter, width).to_tbl()
        );
        return Ok(());
    }
    print!("{}", found.to_text());
    if !found.encodings.is_empty() {
        eprintln!(
            "
As a table file: binviz relsearch {path} {word}{} tbl > game.tbl",
            if width == 2 { " 16" } else { "" }
        );
    }
    Ok(())
}

/// `text`: the file's text, read with a table file.
fn table_text(path: &str, table: &str, rest: &[String]) -> Result<(), String> {
    let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let tbl = std::fs::read_to_string(table).map_err(|e| format!("{table}: {e}"))?;
    let table = binviz::tables::Table::parse(&tbl).map_err(|e| format!("{table}: {e}"))?;
    let show = |s: &str| s.replace('\n', "⏎");
    match rest.first() {
        None => {
            let found = table.strings(&data, 4, 5000);
            for s in &found {
                println!("{:#08x}  {}", s.offset, show(&s.text));
            }
            eprintln!("{} strings of 4 characters or more", found.len());
        }
        Some(a) if num(a).is_ok() => {
            let offset = num(a)? as usize;
            let len = rest.get(1).map(|l| num(l)).transpose()?.unwrap_or(256) as usize;
            let bytes = data
                .get(offset..(offset + len).min(data.len()))
                .ok_or_else(|| format!("{offset:#x} is past the end of {path}"))?;
            println!("{}", table.decode(bytes, false).text);
        }
        Some(text) => {
            let hits = table.find(&data, text, 1000)?;
            for &at in &hits {
                let bytes = &data[at as usize..(at as usize + 96).min(data.len())];
                println!("{at:#08x}  {}", show(&table.decode(bytes, true).text));
            }
            eprintln!("{} places", hits.len());
        }
    }
    Ok(())
}

fn info(bin: &Binary) {
    let s = bin.summary();
    if s.format == binviz::Format::Unknown {
        println!("{} bytes of raw data: not a format binviz recognizes", s.file_size);
        return;
    }
    println!(
        "{} {} {} ({}-bit, {} endian), {} bytes",
        s.format_name,
        s.kind,
        s.arch,
        s.bits,
        if s.little_endian { "little" } else { "big" },
        s.file_size
    );
    if let Some(e) = s.entry {
        println!("entry: {e:#x}");
    }
    if let Some(b) = s.image_base {
        println!("image base: {b:#x}");
    }
    if let Some(id) = &s.build_id {
        println!("build id: {id}");
    }
    if let Some(l) = &s.debug_link {
        println!("debug link: {l}");
    }
    println!("dwarf: {}  symbols: {}", s.has_dwarf, s.symbol_count);
    for p in &s.properties {
        println!("{}: {}", p.key, p.value);
    }
    let objc = bin.objc();
    if !objc.is_empty() {
        println!("Objective-C: {}", objc.counts().to_text());
    }
    println!("\nSegments:");
    for g in bin.segments() {
        println!(
            "  [{:>2}] {:<14} {:<14} {} vaddr {:#012x}..{:#012x} file {:#x}..{:#x}",
            g.index,
            g.name,
            g.kind,
            g.perms,
            g.address,
            g.address + g.mem_size,
            g.file_offset,
            g.file_offset + g.file_size
        );
    }
    println!("\nSections:");
    for x in bin.sections() {
        let file = x
            .file_offset
            .map_or("-".to_string(), |o| format!("{:#x}..{:#x}", o, o + x.file_size));
        println!(
            "  [{:>2}] {:<24} {:<10} {} {:#012x} size {:#8x} file {:<18} {}",
            x.index,
            match &x.segment_name {
                Some(sg) => format!("{sg},{}", x.name),
                None => x.name.clone(),
            },
            format!("{:?}", x.kind).to_lowercase(),
            x.perms,
            x.address,
            x.size,
            file,
            if x.compressed { "(compressed)" } else { "" }
        );
    }
}

fn layout(bin: &Binary, parent: Option<u32>, depth: usize, max: usize) {
    for r in bin.regions(parent) {
        println!(
            "{:indent$}{:#08x}..{:#08x} {:<10} {}{}{}",
            "",
            r.start,
            r.end,
            format!("{:?}", r.kind).to_lowercase(),
            r.name,
            r.value.map(|v| format!(" = {v}")).unwrap_or_default(),
            r.entry_count.map(|n| format!(" [{n} entries]")).unwrap_or_default(),
            indent = depth * 2
        );
        if depth + 1 < max && r.child_count > 0 {
            layout(bin, Some(r.id), depth + 1, max);
        }
    }
}

fn print_inspection(bin: &Binary, i: &binviz::Inspection) {
    match (i.offset, i.address) {
        (Some(o), Some(a)) => println!("file offset {o:#x}, address {a:#x}"),
        (Some(o), None) => println!("file offset {o:#x} (not mapped)"),
        (None, Some(a)) => println!("address {a:#x} (no file bytes)"),
        (None, None) => println!("nothing here"),
    }
    for (d, p) in i.path.iter().enumerate() {
        println!(
            "{:indent$}{} [{:#x}..{:#x}]{}",
            "",
            p.name,
            p.start,
            p.end,
            p.value.as_ref().map(|v| format!(" = {v}")).unwrap_or_default(),
            indent = d * 2
        );
    }
    if let Some(s) = i.segment.and_then(|s| bin.segments().get(s as usize)) {
        println!("segment: {} {}", s.name, s.perms);
    }
    if let Some(s) = i.section.and_then(|s| bin.sections().get(s as usize)) {
        println!("section: {}", s.name);
    }
    if let Some(s) = &i.symbol {
        println!("symbol: {}+{:#x}", s.demangled.as_deref().unwrap_or(&s.name), s.offset);
    }
    if let Some(l) = &i.source {
        println!("source: {}:{}:{}", l.path, l.line, l.column);
    }
    for f in &i.frames {
        println!(
            "  {} {} at {}:{}",
            if f.inlined { "inlined" } else { "in" },
            f.demangled.as_deref().or(f.function.as_deref()).unwrap_or("?"),
            f.file.as_deref().unwrap_or("?"),
            f.line.unwrap_or(0)
        );
    }
    if let Some(ins) = &i.instruction {
        println!("instruction: {:#x} {} {}", ins.address, ins.mnemonic, ins.operands);
    }
}

/// Walks the whole file checking that spans are contiguous and cover every byte.
fn check(bin: &Binary) -> Result<(), String> {
    let size = bin.data().len() as u64;
    let mut unknown = 0u64;
    let mut pos = 0;
    let chunk = 1 << 16;
    let mut kinds = std::collections::BTreeMap::new();
    while pos < size {
        let end = (pos + chunk).min(size);
        let spans = bin.spans(pos, end);
        let mut cursor = pos;
        for s in &spans {
            if s.start != cursor {
                return Err(format!(
                    "gap or overlap at {cursor:#x} (next span starts {:#x})",
                    s.start
                ));
            }
            cursor = s.end;
            *kinds.entry(format!("{:?}", s.kind)).or_insert(0u64) += s.end - s.start;
            if s.kind == binviz::RegionKind::Unknown {
                unknown += s.end - s.start;
            }
        }
        if cursor != end {
            return Err(format!("spans end at {cursor:#x}, expected {end:#x}"));
        }
        pos = end;
    }
    for (k, n) in kinds {
        println!("{k:<12} {n:>10} bytes ({:.1}%)", n as f64 * 100.0 / size as f64);
    }
    // Spot-check describe() at a few offsets.
    for o in [0, size / 3, size / 2, size - 1] {
        let path = bin.describe_offset(o);
        println!(
            "{o:#x}: {}",
            path.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(" › ")
        );
    }
    println!("ok: {size} bytes covered, {unknown} unclaimed");
    Ok(())
}

fn human(n: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut x = n as f64 / 1024.0;
    let mut u = 0;
    while x >= 1024.0 && u < UNITS.len() - 1 {
        x /= 1024.0;
        u += 1;
    }
    format!("{x:.1} {}", UNITS[u])
}

/// What changed in size between two builds: two binaries, or two folders or zips.
/// Which functions of two builds are which, or one function's code next to its match's.
fn diff_functions(old: &str, new: &str, function: Option<&str>) -> Result<(), String> {
    let load = |path: &str| -> Result<Binary, String> {
        let data = binviz::read_file(std::path::Path::new(path)).map_err(|e| format!("{path}: {e}"))?;
        let (mut bin, _) = binviz::package::load_binary(data).map_err(|e| format!("{path}: {e}"))?;
        bin.demangle_swift_with_tool();
        Ok(bin)
    };
    let (a, b) = (load(old)?, load(new)?);
    let d = a.diff_functions(&b);
    let Some(want) = function else {
        print!("{}", d.to_text(60));
        return Ok(());
    };
    let address = resolve_address(&a, want)?;
    let pair = d
        .pairs
        .iter()
        .find(|p| address >= p.old.address && address < p.old.address + p.old.size.max(1))
        .ok_or_else(|| format!("{want} has no match in {new} (removed, or not a function)"))?;
    println!(
        "{} {:#x} → {} {:#x}: {:?}, {:.0}% similar, matched by {:?}
",
        pair.old.name,
        pair.old.address,
        pair.new.name,
        pair.new.address,
        pair.status,
        pair.similarity * 100.0,
        pair.how
    );
    print!(
        "{}",
        binviz::fndiff::code_text(&a.diff_function_code(pair.old.address, &b, pair.new.address))
    );
    Ok(())
}

fn diff(old: &str, new: &str) -> Result<(), String> {
    use binviz::package::{DiskPackage, is_package_path, load_binary};
    let (op, np) = (std::path::Path::new(old), std::path::Path::new(new));
    match (is_package_path(op), is_package_path(np)) {
        (true, true) => {
            let demangle = |b: &mut Binary| {
                b.demangle_swift_with_tool();
            };
            let before = DiskPackage::open(op)?.snapshot(FOLDER_BINARIES, demangle);
            let after = DiskPackage::open(np)?.snapshot(FOLDER_BINARIES, demangle);
            print!("{}", binviz::diff::diff_folders(&before, &after, 40).to_text());
        }
        (false, false) => {
            let snapshot = |path: &str| -> Result<binviz::diff::SizeSnapshot, String> {
                let data = binviz::read_file(std::path::Path::new(path)).map_err(|e| format!("{path}: {e}"))?;
                let (mut bin, _) = load_binary(data).map_err(|e| format!("{path}: {e}"))?;
                bin.demangle_swift_with_tool();
                let name = std::path::Path::new(path)
                    .file_name()
                    .map_or(path.to_string(), |n| n.to_string_lossy().into_owned());
                Ok(bin.size_snapshot(&name))
            };
            print!(
                "{}",
                binviz::diff::diff_binaries(&snapshot(old)?, &snapshot(new)?, 40).to_text()
            );
        }
        _ => return Err("compare like with like: two binaries, or two folders or zips".into()),
    }
    Ok(())
}

/// Symbolicates a crash report with a binary, or with a folder's binaries
/// (each image of the report found by UUID or build ID).
fn crash(path: &str, report_path: &str, debug: Option<&str>, member: Option<&str>) -> Result<(), String> {
    use binviz::crash::{Candidate, Found, Matches, match_images, parse, symbolicate};
    let text = std::fs::read_to_string(report_path).map_err(|e| format!("{report_path}: {e}"))?;
    let report = parse(&text).ok_or_else(|| {
        format!("{report_path}: not a crash report (an Apple .crash or .ips, an Android tombstone, or a stack trace)")
    })?;
    let p = std::path::Path::new(path);
    // Binaries by the index the matching knows them by, and their names.
    let mut bins: Vec<(u32, String, Binary)> = Vec::new();
    let (pairs, notes) = if binviz::package::is_package_path(p) && member.is_none() {
        let mut pkg = binviz::package::DiskPackage::open(p)?;
        let Matches { pairs, notes } = {
            let candidates: Vec<Candidate> = pkg
                .info
                .binaries
                .iter()
                .map(|b| Candidate {
                    binary: b.index,
                    name: &b.name,
                    ids: b.ids.iter().map(|x| x.id.as_str()).collect(),
                })
                .collect();
            match_images(&report, &candidates)
        };
        let mut wanted: Vec<u32> = pairs.iter().map(|&(_, b)| b).collect();
        wanted.sort_unstable();
        wanted.dedup();
        for b in wanted {
            let mut bin = folder_load(&mut pkg, b as usize)?;
            folder_attach(&mut pkg, b as usize, &mut bin);
            bin.demangle_swift_with_tool();
            bins.push((b, pkg.info.binaries[b as usize].name.clone(), bin));
        }
        (pairs, notes)
    } else {
        let mut bin = if binviz::package::is_package_path(p) {
            let mut pkg = binviz::package::DiskPackage::open(p)?;
            folder_binary(&mut pkg, member, debug)?
        } else {
            open(path, debug, member)?
        };
        bin.demangle_swift_with_tool();
        let name = member.map(str::to_string).unwrap_or_else(|| {
            p.file_name()
                .map_or(path.to_string(), |n| n.to_string_lossy().into_owned())
        });
        let id = bin.summary().build_id.clone();
        let Matches { pairs, notes } = match_images(
            &report,
            &[Candidate {
                binary: 0,
                name: &name,
                ids: id.iter().map(String::as_str).collect(),
            }],
        );
        bins.push((0, name, bin));
        (pairs, notes)
    };
    let found: Vec<Found> = pairs
        .iter()
        .filter_map(|&(image, b)| {
            let (_, _, bin) = bins.iter().find(|(i, _, _)| *i == b)?;
            Some(Found { image, binary: b, bin })
        })
        .collect();
    let out = symbolicate(&report, &found, &notes);
    print!(
        "{}",
        out.to_text(|b| bins
            .iter()
            .find(|(i, _, _)| *i == b)
            .map_or_else(|| format!("#{b}"), |(_, n, _)| n.clone()))
    );
    Ok(())
}

/// Searches a binary (`args`: the query, and optionally a kind of result).
fn search(bin: &Binary, args: &[String]) -> Result<binviz::SearchResults, String> {
    let query = args.first().ok_or("missing query")?;
    let only = match args.get(1) {
        Some(k) => Some(
            serde_json::from_value::<binviz::HitKind>(serde_json::Value::String(k.to_string()))
                .map_err(|_| format!("unknown kind {k}"))?,
        ),
        None => None,
    };
    Ok(bin.search(query, if only.is_some() { 50 } else { 6 }, only))
}

fn print_hits(res: &binviz::SearchResults) {
    let mut last = None;
    for h in &res.hits {
        if last != Some(h.kind) {
            let count = res.counts.iter().find(|c| c.kind == h.kind).map_or(0, |c| c.count);
            println!("{:?} ({count})", h.kind);
            last = Some(h.kind);
        }
        let at = match (h.address, h.offset) {
            (Some(a), _) => format!("{a:#x}"),
            (None, Some(o)) => format!("@{o:#x}"),
            _ => String::new(),
        };
        println!("  {:>5} {:<18} {}  - {}", h.score, at, h.label, h.detail);
    }
}

/// At most this many binaries of a folder are read for `info` (to sum their
/// code) and `search`: executables and libraries come first, objects last.
const FOLDER_BINARIES: usize = 300;

/// Reads binary `i` of a folder, and pairs it with a debug file its debug link names.
fn folder_load(pkg: &mut binviz::package::DiskPackage, i: usize) -> Result<Binary, String> {
    let b = pkg.info.binaries[i].clone();
    let data = pkg.read_shared(b.file)?;
    let (bin, _) = binviz::package::load_binary(data.clone()).map_err(|e| format!("{}: {e}", b.path))?;
    binviz::package::update_loaded(&mut pkg.info, i as u32, &data, &bin);
    Ok(bin)
}

/// Attaches binary `i`'s debug file from the folder, if it has one; says which.
fn folder_attach(pkg: &mut binviz::package::DiskPackage, i: usize, bin: &mut Binary) -> Option<String> {
    let d = pkg.info.binaries[i].debug?;
    let debug = pkg.info.debug_files[d as usize].clone();
    Some(
        match pkg
            .read_shared(debug.file)
            .and_then(|data| bin.attach_debug_file(&debug.path, data).map_err(|e| e.to_string()))
        {
            Ok(()) => debug.path,
            Err(e) => format!("{} (not attached: {e})", debug.path),
        },
    )
}

/// The binary of a folder a command works on: the first, or the one
/// `member` names (by name, path or number), with its debug file attached.
fn folder_binary(
    pkg: &mut binviz::package::DiskPackage,
    member: Option<&str>,
    debug: Option<&str>,
) -> Result<Binary, String> {
    let list = &pkg.info.binaries;
    let i = match member {
        None => 0,
        Some(m) => list
            .iter()
            .position(|b| b.path == m)
            .or_else(|| list.iter().position(|b| b.name == m))
            .or_else(|| list.iter().position(|b| b.path.ends_with(&format!("/{m}"))))
            .or_else(|| m.parse::<usize>().ok().filter(|&n| n < list.len()))
            .ok_or_else(|| {
                let names: Vec<String> = list
                    .iter()
                    .map(|b| format!("  [{}] {}  {}", b.index, b.name, b.path))
                    .collect();
                format!("no binary {m:?} in {}; it has:\n{}", pkg.info.name, names.join("\n"))
            })?,
    };
    let mut bin = folder_load(pkg, i)?;
    let with = match debug {
        Some(d) => {
            let bytes = std::fs::read(d).map_err(|e| format!("{d}: {e}"))?;
            bin.attach_debug_file(d, bytes).map_err(|e| e.to_string())?;
            Some(d.to_string())
        }
        None => folder_attach(pkg, i, &mut bin),
    };
    if let Some(result) = pkg.attach_debug_map(&mut bin) {
        debug_map_note(result);
    }
    eprintln!(
        "{} › {}{}",
        pkg.info.name,
        pkg.info.binaries[i].path,
        with.map(|d| format!(" (debug file {d})")).unwrap_or_default()
    );
    Ok(bin)
}

/// `search` over every binary of a folder.
fn folder_search(pkg: &mut binviz::package::DiskPackage, args: &[String]) -> Result<(), String> {
    let mut none = Vec::new();
    for i in 0..pkg.info.binaries.len().min(FOLDER_BINARIES) {
        let mut bin = match folder_load(pkg, i) {
            Ok(bin) => bin,
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        };
        folder_attach(pkg, i, &mut bin);
        bin.demangle_swift_with_tool();
        let res = search(&bin, args)?;
        let b = &pkg.info.binaries[i];
        if res.hits.is_empty() {
            none.push(b.name.clone());
            continue;
        }
        println!("## {} ({})", b.name, b.path);
        print_hits(&res);
    }
    if !none.is_empty() {
        println!("\nNo matches in: {}", none.join(", "));
    }
    if pkg.info.binaries.len() > FOLDER_BINARIES {
        println!(
            "(searched the first {FOLDER_BINARIES} of {} binaries)",
            pkg.info.binaries.len()
        );
    }
    Ok(())
}

/// What `info` says about a folder: its binaries paired with their debug
/// files, sizes by kind of content, the largest and duplicated files, and
/// (for up to [`FOLDER_BINARIES`] binaries) their code summed by owner.
fn folder_info(pkg: &mut binviz::package::DiskPackage) -> Result<(), String> {
    use binviz::package::{BinaryKind, combine_owners};
    let t = std::time::Instant::now();
    // Every binary is read (up to a limit): for its code owners, all its
    // slices' UUIDs, and a debug file its debug link names.
    let mut reports = Vec::new();
    for i in 0..pkg.info.binaries.len().min(FOLDER_BINARIES) {
        let mut bin = match folder_load(pkg, i) {
            Ok(bin) => bin,
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        };
        if let Some(note) = folder_attach(pkg, i, &mut bin).filter(|n| n.contains("(not attached")) {
            eprintln!("{note}");
        }
        reports.push((pkg.info.binaries[i].name.clone(), bin.size_report(500)));
    }
    let info = &pkg.info;
    println!(
        "{} ({}): {} files, {}{}{} — read in {:.1?}",
        info.name,
        info.kind,
        info.files,
        human(info.size),
        info.compressed_size
            .map(|c| format!(", {} compressed", human(c)))
            .unwrap_or_default(),
        if info.debug_size > 0 {
            format!(", {} of debug files", human(info.debug_size))
        } else {
            String::new()
        },
        t.elapsed()
    );
    if let Some(a) = info.binaries.first().and_then(|b| b.bundle.as_ref()) {
        println!(
            "{}: {} {} {} ({}) min OS {}",
            a.path,
            a.name.as_deref().unwrap_or("?"),
            a.bundle_id.as_deref().unwrap_or("?"),
            a.version.as_deref().unwrap_or("?"),
            a.build.as_deref().unwrap_or("?"),
            a.min_os.as_deref().unwrap_or("?")
        );
    }
    println!(
        "\n{} binaries (--member <name> picks one for the other commands):",
        info.binaries.len()
    );
    for b in &info.binaries {
        let ids: Vec<String> = b
            .ids
            .iter()
            .map(|x| {
                if x.id.is_empty() {
                    x.arch.clone()
                } else {
                    format!("{} {}", x.arch, x.id)
                }
            })
            .collect();
        let debug = match b.debug {
            Some(d) => format!("debug file {}", info.debug_files[d as usize].path),
            None if b.kind == BinaryKind::Debug => String::new(),
            None => "no debug file".into(),
        };
        println!(
            "  {:<11} {:>10}  {}  [{} {}]  {}",
            b.kind.label(),
            human(b.size),
            b.path,
            b.format,
            ids.join(", "),
            debug
        );
    }
    for d in info.debug_files.iter().filter(|d| d.binary.is_none()) {
        println!("  unpaired debug file {}", d.path);
    }
    println!("\nby kind:");
    for c in &info.categories {
        println!(
            "  {:<20} {:>10} {:>6} files",
            c.category.label(),
            human(c.size),
            c.files
        );
    }
    println!("\nlargest files:");
    for f in info.largest.iter().take(15) {
        println!("  {:>10}  {}", human(f.size), f.path);
    }
    if !info.duplicates.is_empty() {
        println!("\nduplicates: {} wasted", human(info.duplicate_bytes));
        for d in info.duplicates.iter().take(10) {
            println!("  {} x {}: {}", d.paths.len(), human(d.size), d.paths.join(", "));
        }
    }
    let refs: Vec<(String, &binviz::SizeReport)> = reports.iter().map(|(n, r)| (n.clone(), r)).collect();
    println!(
        "\nowners across {} binaries{}:",
        reports.len(),
        if info.binaries.len() > reports.len() {
            format!(" (of {})", info.binaries.len())
        } else {
            String::new()
        }
    );
    for o in combine_owners(&refs, 30) {
        let spread: Vec<String> = o
            .binaries
            .iter()
            .take(4)
            .map(|(b, n)| format!("{b} {}", human(*n)))
            .collect();
        println!(
            "  {:>10}  {:<40} {:<24} {}",
            human(o.bytes),
            o.name,
            o.kind.label(),
            spread.join(", ")
        );
    }
    Ok(())
}
