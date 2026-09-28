//! Command-line front end for binviz.

use std::process::ExitCode;

use binviz::{Binary, Container, SymbolQuery, Target};

const USAGE: &str = "\
binviz — explain every byte and address of ELF, Mach-O and PE binaries

USAGE:
    binviz <command> <file> [args]

A folder or a zip (an .ipa, an .app, a build…) works as the <file> too, and
stands for the binaries in it, each paired with its debug file (dSYM, .debug):
info lists them all and sums their code by owner, search searches them all,
json describes the folder, and every other command works on the first binary
(an app's own executable) or the one --member names.

COMMANDS:
    info <file>                    Summary, sections and segments
    layout <file> [depth]          File layout tree (default depth 2)
    at <file> <offset>             What the byte at a file offset is
    inspect <file> <address>       Everything known about a virtual address
    symbols <file> [filter]        Symbols, optionally filtered by name
    disasm <file> <addr|symbol> [n]
                                   Disassemble a function with source lines
    dwarf <file>                   DWARF summary and compilation units
    die <file> <unit> [offset]     Children (or details) of a DIE
    lines <file> <unit> [first] [count]
                                   Line table rows of a unit
    sources <file>                 Source files referenced by the line tables
    dwarf-check <file>             Everything in the DWARF that can't be read or doesn't add up
    dwarf-list <file> <unit> [tags] [name]
                                   A unit's DIEs by tag (functions, variables, types, DW_TAG_…)
    dwarf-find <file> <query> [all]
                                   DIEs by name (all: locals and parameters too)
    dwarf-offset <file> <offset>   The DIE at a .debug_info offset
    dwarf-at <file> <addr|symbol>  Scopes and variables in scope at an address
    file-lines <file> <file-id>    Address ranges for each line of a source file
    search <file> <query> [kind]   Search addresses, offsets (@0x..), names, byte
                                   patterns (48 8b ?? 08), \"text\", file:line
    strings <file> [filter]        Printable strings in the data sections
    coverage <file>                How much of the code and data is mapped out
    xrefs <file> <addr|symbol>     References to an address, symbol or string
    refs-from <file> <addr|symbol> References made by a function (or data)
    callers <file> <addr|symbol>   Functions that call a function
    callees <file> <addr|symbol>   Functions a function calls
    callgraph <file> <addr|symbol> [up] [down]
                                   Call graph around a function
    callpath <file> <from> <to>    A shortest chain of calls between two functions
    func <file> <addr|symbol>      Callers, callees, strings and data of a function
    attribution <file> [unit]      Code and data per source file (or unit)
    attributed <file> <id> [unit]  Address ranges of one source file (or unit)
    json <file>                    Summary as JSON
    check <file>                   Verify that the layout covers every byte

Options: --debug <file>  load DWARF from a separate file (dSYM, .debug)
         --member <n>    pick a slice/member of a universal binary or archive, or
                         a binary of a folder (its name, path or number)
         --notes <file>  load annotations (a JSON array) first
Numbers accept decimal or 0x-prefixed hex.";

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
    if args.len() < 2 {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    match run(&args, debug.as_deref(), member.as_deref(), notes.as_deref()) {
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
        let index = match member {
            Some(m) => num(m)? as u32,
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
    if let Some(d) = debug {
        let bytes = std::fs::read(d).map_err(|e| format!("{d}: {e}"))?;
        bin.attach_debug_file(d, bytes).map_err(|e| e.to_string())?;
    }
    Ok(bin)
}

fn run(args: &[String], debug: Option<&str>, member: Option<&str>, notes: Option<&str>) -> Result<(), String> {
    let cmd = args[0].as_str();
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
    } else {
        open(&args[1], debug, member)?
    };
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
        _ => return Err(format!("unknown command {cmd}\n\n{USAGE}")),
    }
    Ok(())
}

fn resolve_address(bin: &Binary, s: &str) -> Result<u64, String> {
    if let Ok(n) = num(s) {
        return Ok(n);
    }
    bin.symbols()
        .by_name(s)
        .map(|s| s.address)
        .ok_or_else(|| format!("no symbol named {s}"))
}

fn fmt_addr(a: u64) -> String {
    format!("{a:#x}")
}

fn info(bin: &Binary) {
    let s = bin.summary();
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
