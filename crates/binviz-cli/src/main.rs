//! Command-line front end for binviz.

use std::process::ExitCode;

use binviz::{Binary, Container, SymbolQuery, Target};

const USAGE: &str = "\
binviz — explain every byte and address of ELF, Mach-O and PE binaries

USAGE:
    binviz <command> <file> [args]

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
    file-lines <file> <file-id>    Address ranges for each line of a source file
    search <file> <query> [kind]   Search addresses, offsets (@0x..), names, byte
                                   patterns (48 8b ?? 08), \"text\", file:line
    strings <file> [filter]        Printable strings in the data sections
    coverage <file>                How much of the code and data is mapped out
    attribution <file> [unit]      Code and data per source file (or unit)
    attributed <file> <id> [unit]  Address ranges of one source file (or unit)
    json <file>                    Summary as JSON
    check <file>                   Verify that the layout covers every byte

Options: --debug <file>  load DWARF from a separate file (dSYM, .debug)
         --member <n>    pick a slice/member of a universal binary or archive
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
    let mut bin = open(&args[1], debug, member)?;
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
                        println!("  {:<26} {:<20} {}", a.name, a.form, a.value.replace('\n', "\n  "));
                    }
                    if let Some(t) = &det.type_name {
                        println!("  type: {t}");
                    }
                    if let Some(l) = &det.decl {
                        println!("  declared at {}:{}", l.path, l.line);
                    }
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
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(bin.summary()).map_err(|e| e.to_string())?
        ),
        "check" => check(&bin)?,
        "search" => {
            let query = arg(2).ok_or("missing query")?;
            let only = match arg(3) {
                Some(k) => Some(
                    serde_json::from_value::<binviz::HitKind>(serde_json::Value::String(k.to_string()))
                        .map_err(|_| format!("unknown kind {k}"))?,
                ),
                None => None,
            };
            let res = bin.search(query, if only.is_some() { 50 } else { 6 }, only);
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
