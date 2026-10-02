//! Command-line front end for binviz.

use std::process::ExitCode;

use binviz::{Binary, Container, SymbolQuery, Target};

const USAGE: &str = "\
binviz — explain every byte and address of ELF, Mach-O, PE, XBE and WebAssembly binaries

USAGE:
    binviz <command> <file> [args]

A folder or a zip (an .ipa, an .app, a build…) works as the <file> too, and
stands for the binaries in it, each paired with its debug file (dSYM, .debug, PDB):
info lists them all and sums their code by owner, search searches them all,
info json describes the folder, and every other command works on the first
binary (an app's own executable) or the one --member names. A CD image opens
the executable the disc boots. An original Xbox game's default.xbe opens like
a 32-bit PE: its kernel imports named, its code followed from the entry point.

COMMANDS:
  The file
    info <file> [json]             Summary, sections and segments (json: as JSON)
    layout <file> [depth]          File layout tree (default depth 2)
    inspect <file> <address|@offset>
                                   Everything known about an address, or the byte at a file offset
    check <file>                   Verify that the layout covers every byte
    files <file>                   The files on a disc image, or the members of a universal
                                   binary or archive: number, sector or offset, size, name
    extract <file> <member> [out]  One of them (its number or name) written out; or for any
                                   file, a stretch of it: @0x9800+0xe800 (offset+length)
    blobs <file> [--psx-exe <exe>] Code inside a file binviz can't read (a game's archive of
                                   overlays): each run of MIPS code with where it loads,
                                   worked out from its own calls and pointers
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
    structs <file> [function|name] Structures joined up across calls (MIPS): what a pointer passed
                                   around is, as every function that reaches it uses it, as C;
                                   for one function, those it hands around or reaches
    blocks <file> [--window N] [--done 10,25,50] [--top N] [--loose]
                                   Runs of N instructions (default 12, registers renamed, numbers
                                   and names left out) found in several functions, and how much of
                                   the bigger functions the smallest ones' runs would reach
                                   (--loose: whichever registers the runs use)
    globals <file> [filter]        The data the code uses, typed by its use and named where
                                   nothing names it: floats, integers, pointers to structures
                                   (the offsets reached through them), tables of functions,
                                   strings or pointers, arrays, function pointers called through
    classes <file> [filter]        C++ classes from an MSVC binary's RTTI: bases with their
                                   offsets, vtables with their virtual functions
    store <file> record <src-root> [project]
                                   Keep the --notes' matched functions, each with its C (read
                                   from the file its note names, under src-root) and what built
                                   it, in your store (BINVIZ_STORE, else
                                   ~/.local/share/binviz/store), for the next project
    store <file> hits              The unmatched functions the store has a match for: the same
                                   code as a function matched in another project
    store <file> <addr|symbol>     One function's matches, with their C
    worklist <file> [n] [k/n]      The unnamed functions to name next: those whose callees
                                   all have names first, then the most called (k/n: one
                                   of n shares, for agents working at once)
    score <file> <names>           The --notes' names (or <file>.binviz-notes.json's) against
                                   the real ones: <names> is its debug file (.dbg, .pdb,
                                   .debug...) or an unstripped build
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
    header <file> [name...]        The debug info's types and functions as a C header (a PDB's
                                   with --debug) that checks its own layout when compiled:
                                   explicit padding, _Static_assert on sizes and offsets;
                                   with names, those and the types they need
    attribution <file> [unit] [id] Code and data per source file (or unit); with an id,
                                   the address ranges of that one
    crash <file> <report>          Symbolicate a crash report (Apple .crash or .ips, Android
                                   tombstone, a stack trace, a browser's or Node's through
                                   WebAssembly) with a binary or a folder's binaries
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
  Decompilation (PlayStation, Nintendo 64; x86 where noted)
    signature <file> <addr|symbol> What a function's code says about its prototype: register
                                   and stack arguments, return, frame, saved registers, the
                                   structures it walks; for x86 the calling convention too
    context <file> <addr|symbol> [n]
                                   Everything needed to write a function's C: its code with
                                   names (n instructions, default 4000), its signature, callers
                                   and callees with theirs, strings, globals, notes (the first
                                   24 of each list; func and refs list them all)
    match <file> <object> [name]   The compiler's object file (ELF .o, or COFF .obj from MSVC
                                   or clang-cl) scored against the original, function by
                                   function (relocations masked), each difference explained;
                                   with a name, that function only; with a folder of objects,
                                   the whole project, unit by unit, worst first; the shape of
                                   the epilogue says which GCC built the original
    match <file> <object> <name> --range <start> <end>
                                   That function against the original's code in a range you
                                   give (start..end or start+length work too), whatever extent
                                   the notes give the function (MIPS)
    match <file> <folder> [--json] [--record [--meta compiler=..,flags=..,sdk=..]] [--no-cache]
                                   [--strict-relocs]
                                   --json: every function's score as JSON (unit, name, address,
                                   percent, exact, distance, clusters, instruction counts, the
                                   kinds of difference, the compilers the epilogues imply);
                                   --strict-relocs: a symbol the original has no name for is
                                   taken at the address its name carries (D_800CB188, sub_…), so
                                   a wrong global or callee no longer scores as exact (no cache);
                                   --record: the outcomes into --notes (or <file>.binviz-notes.json)
                                   through its journal, as the MCP server's record does; the
                                   score cache (BINVIZ_CACHE, else ~/.cache/binviz/scores) gives
                                   objects scored before straight back
    rank <file> <object|folder>... [--function name] [--top N] [--json] [--no-cache]
                                   Variants (objects, or folders of them) scored in one process
                                   and ranked closest first by distance: differing instructions
                                   weighted by kind (registers 1, reordering and constants 2,
                                   another instruction 3, missing or extra 4, frame and slots 6),
                                   so a search over rewrites has a gradient where the percent
                                   stays flat; --function: that function of each object only
    resolve <file> <name|address>  Which function holds a name or address now (a fragment a split
                                   function used to start at is inside the joined one)
    scores <before.json> <after.json> [--top N] [--json]
                                   Two runs' scores (match --json, rank --json) set against each
                                   other: which functions got closer or further (by distance,
                                   then percent), how many stayed, exact counts before and after
    asm <file> <addr|symbol>... [--list <file>] [--all] [--out <dir>] [--bare]
                                   MIPS functions as GNU assembler source, as splat writes it
                                   (labels, calls by name, %hi/%lo pairs, jump tables): m2c's
                                   input. Several at once (names, a file with one per line, or
                                   --all), to stdout or one <name>.s each in --out; --bare leaves
                                   out the offset/address/word comments
    m2c <file> <addr|symbol> [cmd] m2c's first draft of its C (cmd: how to run m2c, \"python3
                                   m2c.py\" say; through sh on Unix, split into words on Windows),
                                   with the prototypes of the function and its callees from the
                                   notes' types or their code, so it doesn't invent arguments
    flags <file> <source|folder> <command> [<flags>...] [--jobs N] [--function name] [--top N] [--json]
                                   The source compiled with each set of flags (the command's
                                   {src}, {out} and {flags} filled in) and matched against the
                                   file, best first: the flags a unit is built with. --jobs N
                                   runs N compiles at once; a folder of sources makes each file
                                   a variant (ranked by distance, --function for one function);
                                   a command with {srcdir} and {outdir} is run once for the
                                   whole folder and must write <name>.o for each <name>.c there;
                                   --record [--meta compiler=..,flags=..] puts each function's
                                   best outcome into the notes, as match --record does
    report <file> <report.json>    objdiff's report placed on the file's functions
    progress <file> [json]         Where the decompilation stands (the notes' statuses): the
                                   counts by state, partial credit (each function's best percent
                                   weighted by its size), then by unit, a range a note merges
                                   counted once; json: as objdiff's report, which decomp.dev shows
    splat <file> <name> [dir] [split...]
                                   A splat config and symbol_addrs.txt (in dir, or printed)
                                   for a PS-X EXE, the code split into units at the splits
    splat <file> import <symbol_addrs.txt>
                                   A splat symbol file as notes, JSON for --notes
    fieldrefs <file> <global> [offset]
                                   What the code does through a pointer kept in a global (MIPS):
                                   each offset it loads, stores or takes the address of; with an
                                   offset, the functions that use that field and how
    libraries <file>               The source files the code was built from that say so (RCS
                                   $Id: strings: Psy-Q's libraries carry them), with their
                                   revisions and dates; sdk with no libraries prints the same
    sdk <file> <lib|folder...> [notes]
                                   The library functions in the file (Psy-Q's SDK, MSVC's C
                                   runtime…), found by the signatures of the libraries' objects:
                                   Psy-Q .LIB/.OBJ, MSVC .lib/.obj (COFF), .a/.o; notes: as
                                   notes JSON
    locate <ram.bin> <file>        Where a file from the disc (an overlay) sits in a PlayStation
                                   memory image; for an archive of files (any format, stored
                                   uncompressed), each stretch of it loaded there
    counterparts <file> <source folder> [n]
                                   The file set against the source it may be built from (C,
                                   C++): functions named here the source doesn't define and
                                   strings the code uses the source doesn't have (another
                                   version), and the source's functions nothing here is named
                                   after; the other way, for two builds: names and diff
    names <file> <candidates.json | named build> [min%]
                                   Names from another build proposed for the file's functions:
                                   its functions with the strings they use and the functions
                                   they call (JSON), or the build itself (its symbols, debug
                                   info or PDB beside it): functions paired by their code, then
                                   strings and calls, data by the instructions using it; lists
                                   its functions with no counterpart here. min%: as notes JSON
                                   for those at or above that confidence

Options: --debug <file>  load debug info from a separate file (dSYM, .debug,
                         PDB; for WebAssembly, a source map or the module with
                         the DWARF, which binviz looks for beside the module
                         when it names one), or for a Mach-O binary linked
                         without dsymutil, from the folder holding the object
                         files its debug map names
         --member <n>    pick a slice/member of a universal binary or archive, or
                         a binary of a folder (its name, path or number)
         --notes <file>  load annotations first (the MCP server's and the web UI's notes
                         file, or a JSON array); a note's type (a function's prototype, a
                         global's type) names the fields its pointers reach: [esi+0x21c]
                         reads as edict_t.enemy
         --examples-from <file> context: worked examples from a sibling decompilation
                         (another game built with the same compiler): its functions its
                         notes mark matched, shaped like this one. Its notes are
                         <file>.binviz-notes.json, or --examples-notes <file>
         --types <file>  take the types those name from this file's debug info (an
                         object compiled from the program's headers with -g
                         -fno-eliminate-unused-debug-types, or a PDB)
         --log <file>    a game ROM: follow its code with an emulator's code/data log
                         (FCEUX's or Mesen's .cdl): the code the game ran, the data it read
         --psx-exe <file> a PlayStation memory image (2 MiB of RAM dumped by an emulator)
                         or overlay: name the functions of this boot executable in it,
                         as its own notes (<file>.binviz-notes.json) and --notes name them
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

/// Rust ignores SIGPIPE, so printing into a closed pipe (`binviz … | head`)
/// panics; the default action ends the process quietly like any other tool.
#[cfg(unix)]
fn restore_sigpipe() {
    unsafe extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
    }
    const SIGPIPE: i32 = 13;
    const SIG_DFL: usize = 0;
    // SAFETY: called once at startup, before any thread exists.
    unsafe {
        signal(SIGPIPE, SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

fn main() -> ExitCode {
    restore_sigpipe();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut take_opt = |name: &str| -> Option<String> {
        let i = args.iter().position(|a| a == name)?;
        args.remove(i);
        (i < args.len()).then(|| args.remove(i))
    };
    let debug = take_opt("--debug");
    let member = take_opt("--member");
    let notes = take_opt("--notes");
    let types = take_opt("--types");
    let examples_from = take_opt("--examples-from");
    let examples_notes = take_opt("--examples-notes");
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
        Notes {
            notes: notes.as_deref(),
            types: types.as_deref(),
            examples_from: examples_from.as_deref(),
            examples_notes: examples_notes.as_deref(),
        },
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
    if debug.is_none() && bin.debug_info().is_none() {
        wasm_debug_beside(&mut bin, path);
    }
    Ok(bin)
}

/// Attaches the debug info a WebAssembly module names (its DWARF module,
/// its source map) when the file is beside the module: the name's last
/// part, a URL's too. Says which on stderr.
fn wasm_debug_beside(bin: &mut Binary, path: &str) {
    let dir = std::path::Path::new(path).parent().unwrap_or(std::path::Path::new(""));
    for named in bin.wasm_debug_files() {
        let file = named.split(['?', '#']).next().unwrap_or(&named);
        let file = dir.join(file.rsplit(['/', '\\']).next().unwrap_or(file));
        let Ok(bytes) = std::fs::read(&file) else { continue };
        match bin.attach_debug_file(&file.to_string_lossy(), bytes) {
            Ok(()) => {
                eprintln!("debug info from {}, which the module names", file.display());
                return;
            }
            Err(e) => eprintln!("{}: {e}", file.display()),
        }
    }
}

/// A PlayStation memory image or overlay, its functions named after the
/// boot executable's.
/// A notes file's annotations: the JSON the web UI and the MCP server keep, or a bare list.
fn read_notes(path: &str) -> Result<Vec<binviz::Annotation>, String> {
    // With its journal: what agents wrote that no rewrite of the file has folded yet.
    if !std::path::Path::new(path).exists() {
        return Err(format!("{path}: no such file"));
    }
    binviz::notes::read(std::path::Path::new(path)).map(|(list, _, _)| list)
}

/// Loads the notes into `bin`; for a ROM, the functions they name become
/// boundaries of its reading first (a note never loses its function to the
/// joining of pieces).
fn apply_notes(bin: &mut Binary, list: Vec<binviz::Annotation>) -> Result<(), String> {
    if let Some(rebuilt) = bin
        .with_function_boundaries(&binviz::notes::function_boundaries(&list))
        .map_err(|e| e.to_string())?
    {
        *bin = rebuilt;
    }
    bin.set_annotations(list);
    Ok(())
}

fn open_psx(path: &str, psx: Psx<'_>, notes: Option<&str>) -> Result<Binary, String> {
    let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let exe = match psx.exe {
        Some(e) => {
            let mut exe = open(e, None, None)?;
            // The notes are the executable's too (the same addresses), and so are
            // its own, kept beside it: the names given there reach every image.
            let mut list = match notes {
                Some(n) => read_notes(n)?,
                None => Vec::new(),
            };
            let beside = format!("{e}.binviz-notes.json");
            if std::path::Path::new(&beside).exists() {
                let own = read_notes(&beside)?;
                eprintln!("{e}: {} notes from {beside}", own.len());
                list.extend(own);
            }
            if !list.is_empty() {
                apply_notes(&mut exe, list)?;
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

/// Loaded after the binary: the user's notes, and a file describing its types.
struct Notes<'a> {
    notes: Option<&'a str>,
    types: Option<&'a str>,
    /// A sibling decompilation (another game, the same compiler) and its notes, for worked examples.
    examples_from: Option<&'a str>,
    examples_notes: Option<&'a str>,
}

fn run(
    args: &[String],
    debug: Option<&str>,
    member: Option<&str>,
    Notes {
        notes,
        types,
        examples_from,
        examples_notes,
    }: Notes<'_>,
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
    if cmd == "files" || cmd == "extract" {
        return files_or_extract(&args[1..], cmd == "files");
    }
    if cmd == "blobs" {
        return blobs(&args[1], psx.exe);
    }
    if cmd == "scores" {
        // Two runs' scores (match --json, rank --json) set against each other.
        let mut rest: Vec<&str> = args[1..].iter().map(String::as_str).collect();
        let json = take_flag(&mut rest, "--json");
        let top = match take_value(&mut rest, "--top") {
            Some(n) => num(&n)? as usize,
            None => 30,
        };
        let (Some(a), Some(b)) = (rest.first(), rest.get(1)) else {
            return Err("binviz scores <before.json> <after.json> [--top N] [--json]".into());
        };
        let read = |p: &str| {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?;
            binviz::matching::scores_from_json(&text).map_err(|e| format!("{p}: {e}"))
        };
        let diff = binviz::matching::compare_scores(&read(a)?, &read(b)?);
        if json {
            println!("{}", serde_json::to_string_pretty(&diff).map_err(|e| e.to_string())?);
        } else {
            print!("{}", diff.to_text(top));
        }
        return Ok(());
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
        // A notes file that --record will create may not exist yet.
        if std::path::Path::new(path).exists() || !args.iter().any(|a| a == "--record") {
            apply_notes(&mut bin, read_notes(path)?)?;
        }
    }
    if let Some(path) = types {
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        bin.attach_types(path, bytes).map_err(|e| e.to_string())?;
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
            if bin.xrefs_supported() {
                bin.prepare_xrefs();
            }
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
            let n = arg(3).map(num).transpose()?.unwrap_or(4000) as usize;
            // The references name the data the code uses (tables, globals, short strings).
            if bin.xrefs_supported() {
                bin.prepare_xrefs();
            }
            // An address inside a function starts there, not at the function's start.
            let d = match bin.symbols().function_containing(addr) {
                Some(f) if f.address != addr && addr < f.address + f.size => {
                    bin.disassemble(addr, f.address + f.size, n)
                }
                _ => bin.disassemble_function(addr, n),
            };
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
                for m in d.marks.iter().filter(|m| m.address == i.address) {
                    println!("  ; {}", m.text);
                }
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
            if d.truncated {
                println!("  … stopped at {n} instructions; pass a larger count for the rest");
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
        "classes" => {
            let filter = arg(2).unwrap_or("").to_ascii_lowercase();
            let classes = bin.cpp_classes();
            if classes.is_empty() {
                return Err("no C++ run-time type information (RTTI) found: it is MSVC's, in PE files".into());
            }
            let word = if bin.summary().bits == 64 { 8 } else { 4 };
            for c in classes.iter().filter(|c| c.name.to_ascii_lowercase().contains(&filter)) {
                let bases: Vec<String> = c
                    .bases
                    .iter()
                    .map(|b| format!("{} at {:#x}", b.name, b.offset))
                    .collect();
                println!(
                    "{}{}",
                    c.name,
                    if bases.is_empty() {
                        String::new()
                    } else {
                        format!(" : {}", bases.join(", "))
                    }
                );
                for v in &c.vtables {
                    println!(
                        "  vtable {:#x}{}",
                        v.address,
                        v.for_base
                            .as_deref()
                            .map_or(String::new(), |b| format!(" for {b} (at {:#x})", v.offset))
                    );
                    for (i, f) in v.functions.iter().enumerate() {
                        let name = bin
                            .symbols()
                            .at(*f)
                            .map_or(format!("{f:#x}"), |s| s.display_name().into_owned());
                        println!("    [{i}] {:#x} {name}  (vtable+{:#x})", f, i * word);
                    }
                }
            }
        }
        "globals" => {
            if !bin.xrefs_supported() {
                return Err(
                    "globals are found through the code's references, which aren't read for this architecture".into(),
                );
            }
            let filter = arg(2).unwrap_or("");
            let page = bin.globals(filter, 0, 100_000);
            println!(
                "{} globals{}",
                page.total,
                if filter.is_empty() {
                    String::new()
                } else {
                    format!(" matching {filter:?}")
                }
            );
            for g in &page.globals {
                let name = bin
                    .symbols()
                    .at(g.address)
                    .map_or(g.name.clone(), |s| s.display_name().into_owned());
                println!("  {:#x} {:>6}  {:<20} {}", g.address, g.size, name, g.description);
            }
        }
        "score" => {
            let with = arg(2).ok_or("missing the file with the real names (a debug file, or an unstripped build)")?;
            let mut bin = bin;
            if bin.annotations().is_empty() {
                let sidecar = format!("{}.binviz-notes.json", args[1]);
                let text = std::fs::read_to_string(&sidecar)
                    .map_err(|_| format!("no notes to score: pass --notes, or keep them in {sidecar}"))?;
                bin.set_annotations(binviz::notes::parse(&text).map_err(|e| format!("{sidecar}: {e}"))?.0);
            }
            // A debug file for this binary, else a build of it with its names.
            let data = std::fs::read(with).map_err(|e| format!("{with}: {e}"))?;
            let mut real = Binary::parse(bin.data().to_vec()).map_err(|e| e.to_string())?;
            let real = match real.attach_debug_file(with, data.clone()) {
                Ok(()) => real,
                // Not its debug file: a build with its names, if it names functions.
                Err(e) => match Binary::parse(data) {
                    Ok(other)
                        if other
                            .symbols()
                            .functions()
                            .any(|f| f.source != binviz::SymbolSource::Discovered) =>
                    {
                        other
                    }
                    _ => return Err(format!("{with}: {e}")),
                },
            };
            let c = bin.compare_names(&real);
            let same = c.pairs.iter().filter(|p| p.same).count();
            let close = c.pairs.iter().filter(|p| p.close).count();
            let pct = |n: usize| n as f64 * 100.0 / f64::from(c.functions.max(1));
            println!(
                "The notes name {} of the {} functions {with} names ({:.0}%): {same} the same, {close} close, {} different.",
                c.pairs.len(),
                c.functions,
                pct(c.pairs.len()),
                c.pairs.len() - same - close
            );
            if !c.extra.is_empty() {
                println!(
                    "{} names in the notes are where no function starts (data, labels, or a wrong start).",
                    c.extra.len()
                );
            }
            println!("\n{:<18}  {:<32}  real name", "address", "notes");
            for p in &c.pairs {
                println!(
                    "{:#018x}  {:<32}  {}{}",
                    p.address,
                    p.noted,
                    p.real,
                    if p.same {
                        "  ="
                    } else if p.close {
                        "  ~"
                    } else {
                        ""
                    }
                );
            }
            if !c.missed.is_empty() {
                println!("\nNot named in the notes ({}):", c.missed.len());
                for (address, name) in c.missed.iter().take(200) {
                    println!("{address:#018x}  {name}");
                }
            }
        }
        "worklist" => {
            let limit = arg(2).and_then(|n| n.parse().ok()).unwrap_or(20);
            let shard = match arg(3).or(arg(2).filter(|a| a.contains('/'))) {
                Some(s) => {
                    let (k, n) = s
                        .split_once('/')
                        .and_then(|(k, n)| Some((k.parse::<u32>().ok()?, n.parse::<u32>().ok()?)))
                        .filter(|&(k, n)| k >= 1 && k <= n)
                        .ok_or("the share is k/n, with 1 <= k <= n")?;
                    Some((k - 1, n))
                }
                None => None,
            };
            let w = bin.worklist(limit, shard, &[]);
            println!(
                "{} of {} functions named; {} to go{}",
                w.named,
                w.functions,
                w.functions - w.named,
                if shard.is_some() {
                    format!(" ({} in this share)", w.remaining)
                } else {
                    String::new()
                }
            );
            for item in &w.items {
                println!(
                    "{:#018x} {:>7}  {:>4} callers  {:>3} callees ({} unnamed)  {}",
                    item.address, item.size, item.callers, item.callees, item.unnamed_callees, item.name
                );
            }
        }
        "blocks" => {
            let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1));
            let mut options = binviz::blocks::BlockOptions {
                loose: args.iter().any(|a| a == "--loose"),
                ..Default::default()
            };
            if let Some(v) = flag("--window") {
                options.window = num(v)? as usize;
            }
            if let Some(v) = flag("--top") {
                options.top = num(v)? as usize;
            }
            if let Some(v) = flag("--done") {
                options.done_shares = v
                    .split(',')
                    .map(|p| p.trim().trim_end_matches('%').parse::<f64>().map(|x| x / 100.0))
                    .collect::<Result<_, _>>()
                    .map_err(|_| "--done takes percentages: 10,25,50".to_string())?;
            }
            let r = bin.repeated_blocks(&options);
            println!(
                "{} functions, {} instructions ({} of them frame setup and teardown, left out); runs of {} instructions: {} ({} more left out as a few instructions repeated)",
                r.functions, r.instructions, r.frame_instructions, r.window, r.windows, r.windows_skipped
            );
            println!(
                "{} distinct runs, {} of them in more than one function",
                r.distinct, r.repeating
            );
            println!("\nIf the smallest functions were done, how much of the rest their runs reach:");
            println!(
                "  done   functions  pending insns  in a run of a done fn  in a run of 3+ done fns  fns half covered"
            );
            let pct = |n: usize, d: usize| if d == 0 { 0.0 } else { n as f64 * 100.0 / d as f64 };
            for t in &r.trials {
                println!(
                    "  {:>3.0}%  {:>9}  {:>13}  {:>20.1}%  {:>22.1}%  {:>9} of {}",
                    t.done_share * 100.0,
                    t.done_functions,
                    t.pending_instructions,
                    pct(t.covered_once, t.pending_instructions),
                    pct(t.covered_thrice, t.pending_instructions),
                    t.half_covered_functions,
                    t.pending_functions,
                );
            }
            println!("\nRuns in the most functions:");
            for (n, b) in r.top.iter().enumerate() {
                println!(
                    "\n#{} in {} functions ({} times), for instance at {}:",
                    n + 1,
                    b.functions,
                    b.occurrences,
                    fmt_addr(b.example)
                );
                for t in &b.text {
                    println!("    {t}");
                }
            }
        }
        "structs" => {
            let list = bin.structures();
            if list.is_empty() {
                return Err(
                    "no structures found: MIPS code only, and only those more than one function reaches".into(),
                );
            }
            match arg(2) {
                Some(which) => {
                    let addr = resolve_address(&bin, which).ok();
                    let found: Vec<String> = match addr {
                        Some(a) if bin.symbols().function_containing(a).is_some() => bin
                            .structures_of(a)
                            .into_iter()
                            .map(|(what, s)| format!("{what} is {}", s.describe(12)))
                            .collect(),
                        _ => list
                            .iter()
                            .filter(|s| s.name == which)
                            .map(|s| s.describe(1000))
                            .collect(),
                    };
                    if found.is_empty() {
                        return Err(format!("no structure named or used by {which}"));
                    }
                    println!("{}", found.join("\n"));
                }
                None => {
                    println!(
                        "{} structures reached by more than one function, the most used first:\n",
                        list.len()
                    );
                    for s in list {
                        println!("{}", s.describe(4));
                    }
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
                    "{name:<20} {size:>9}  match {:>5.1}%  rev {:>5.1}%  ann {:>5.1}%  named {:>5.1}%  struct {:>5.1}%  recov {:>5.1}%  pad {:>5.1}%  unexpl {:>5.1}%",
                    pct(b.matched, size),
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
                "functions: {} named, {} recovered, {} yours, {} agents'; {} annotations ({} reviewed, {} by agents)",
                c.functions.named,
                c.functions.recovered,
                c.functions.user,
                c.functions.agents,
                c.annotations,
                c.reviewed,
                c.agent_notes
            );
            if !c.unexplored_by_kind.is_empty() {
                let kinds: Vec<String> = c
                    .unexplored_by_kind
                    .iter()
                    .map(|(k, n)| format!("{k} {n} bytes"))
                    .collect();
                println!("unexplored, by what it looks like: {}", kinds.join(", "));
            }
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
        "header" => {
            let d = bin
                .debug_info()
                .ok_or("no debug info (DWARF, or a PDB passed with --debug)")?;
            let names: Vec<&str> = args[2..].iter().map(String::as_str).collect();
            let h = d.c_header(&names);
            if !names.is_empty() && h.not_found.len() == names.len() {
                return Err(format!("no type or function named {}", h.not_found.join(", ")));
            }
            print!("{}", h.text);
            eprintln!(
                "{} structures and unions, {} enums, {} typedefs, {} functions",
                h.structs, h.enums, h.typedefs, h.functions
            );
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
            println!("{} at {:#x}, {} bytes", f.name, f.address, f.size);
            let refs = f.referenced_by.describe();
            println!("referenced by: {}", if refs.is_empty() { "nothing" } else { &refs });
            // Each list says how long it is: an empty heading reads as output cut short.
            let heading = |n: u32, one: &str, many: &str| match n {
                0 => println!("no {many}"),
                1 => println!("1 {one}:"),
                n => println!("{n} {many}:"),
            };
            let more = |n: u32, shown: usize| {
                if n as usize > shown {
                    println!("  … and {} more", n as usize - shown);
                }
            };
            heading(f.caller_count, "caller", "callers");
            for e in &f.callers {
                println!("  {:>4}x {}", e.calls, e.name);
            }
            more(f.caller_count, f.callers.len());
            heading(f.callee_count, "callee", "callees");
            for e in &f.callees {
                println!("  {:>4}x {}", e.calls, e.name);
            }
            more(f.callee_count, f.callees.len());
            heading(f.string_count, "string", "strings");
            for s in &f.strings {
                println!("  {:#x} {:?}", s.address, s.text);
            }
            more(f.string_count, f.strings.len());
            heading(f.data_count, "global", "globals");
            for r in &f.data {
                println!(
                    "  {:<8} {:#x} {}",
                    r.kind.as_str(),
                    r.target,
                    r.to.as_deref().unwrap_or("")
                );
            }
            more(f.data_count, f.data.len());
        }
        "store" => {
            let store = binviz::store::Store::open_default().ok_or("no place for the store: set BINVIZ_STORE")?;
            let sub = arg(2).ok_or("store <file> record <src-root> [project] | hits | <addr|symbol>")?;
            match sub {
                "record" => {
                    let root = arg(3).ok_or("store <file> record <src-root> [project]")?;
                    let label = std::path::Path::new(&args[1])
                        .file_name()
                        .map_or_else(|| "binary".to_string(), |n| n.to_string_lossy().into_owned());
                    let project = arg(4).map_or(label, str::to_string);
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs());
                    let rec = bin.store_entries(std::path::Path::new(root), &project, now);
                    for e in &rec.entries {
                        store.add(e).map_err(|e| format!("{}: {e}", store.dir().display()))?;
                    }
                    println!(
                        "Kept {} matched function(s) of {project} in {}.",
                        rec.entries.len(),
                        store.dir().display()
                    );
                    for (address, name, why) in &rec.skipped {
                        println!("  not kept: {name} ({address:#x}): {why}");
                    }
                }
                "hits" => {
                    let hits = bin.store_hits(&store.index());
                    println!(
                        "{} unmatched function(s) have a match in the store at {}:",
                        hits.len(),
                        store.dir().display()
                    );
                    for h in &hits {
                        let e = &h.entries[0];
                        println!(
                            "  {:#x} {} ({} instructions) = {} in {}, {}",
                            h.address,
                            h.name,
                            h.instructions,
                            e.name,
                            e.project,
                            e.build()
                        );
                    }
                }
                at => {
                    let addr = resolve_address(&bin, at)?;
                    let key = bin.function_key(addr).ok_or_else(|| {
                        format!(
                            "not the start of a function of {} instructions or more",
                            binviz::store::MIN_INSTRUCTIONS
                        )
                    })?;
                    let found = store.find(&key.key);
                    if found.is_empty() {
                        println!("nothing in the store matches {addr:#x} (key {})", key.key);
                    }
                    for e in &found {
                        println!("{} in {}, built with {}\n{}\n", e.name, e.project, e.build(), e.c);
                    }
                }
            }
        }
        "signature" => {
            let addr = resolve_address(&bin, arg(2).ok_or("missing address or symbol")?)?;
            let s = bin
                .function_signature(addr)
                .ok_or("not in a function, or not MIPS or x86 code")?;
            print!("{}", s.describe());
        }
        "context" => {
            let addr = resolve_address(&bin, arg(2).ok_or("missing address or symbol")?)?;
            let n = arg(3).map(num).transpose()?.unwrap_or(4000) as usize;
            // A sibling decompilation's done functions as worked examples: its notes beside it, unless given.
            let sibling = match examples_from {
                Some(path) => {
                    let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
                    let mut other = open_with_its_debug_file(path, data)?;
                    let beside = format!("{path}.binviz-notes.json");
                    match examples_notes {
                        Some(notes) => other.set_annotations(read_notes(notes)?),
                        None if std::path::Path::new(&beside).exists() => other.set_annotations(read_notes(&beside)?),
                        None => {}
                    }
                    let label = std::path::Path::new(path)
                        .file_name()
                        .map_or(path.into(), |f| f.to_string_lossy());
                    Some((label.into_owned(), other))
                }
                None => None,
            };
            let siblings: Vec<(&str, &Binary)> = sibling.iter().map(|(l, b)| (l.as_str(), b)).collect();
            let c = bin.decomp_context_with(addr, n, &siblings).ok_or("not in a function")?;
            print!("{}", c.describe());
        }
        "match" => {
            let object = arg(2).ok_or("which object file? binviz match <file> <object.o|folder> [name]")?;
            let folder = std::path::Path::new(object);
            let mut rest: Vec<&str> = args[3..].iter().map(String::as_str).collect();
            let json = take_flag(&mut rest, "--json");
            // Strict relocations change the scores: never through the cache.
            let strict = take_flag(&mut rest, "--strict-relocs");
            binviz::matching::STRICT_RELOCS.store(strict, std::sync::atomic::Ordering::Relaxed);
            let cache = if take_flag(&mut rest, "--no-cache") || strict {
                None
            } else {
                binviz::matching::cache::ScoreCache::open(None)
            };
            let record = take_flag(&mut rest, "--record");
            let meta = take_value(&mut rest, "--meta");
            if folder.is_dir() {
                // A project's build output: every object, unit by unit.
                let objects = objects_in(&[object.to_string()])?;
                if objects.is_empty() {
                    return Err(format!("no object files (.o, .obj) in {object}"));
                }
                let p = bin.match_project_cached(&objects, cache.as_ref());
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&p.to_json()).map_err(|e| e.to_string())?
                    );
                } else {
                    print!("{}", p.to_text(50));
                }
                if record {
                    record_outcomes(&bin, &args[1], notes, meta.as_deref(), &p.progress())?;
                }
                return Ok(());
            }
            if record {
                return Err("--record is for a folder of objects: binviz match <file> <folder> --record".into());
            }
            let bytes = std::fs::read(object).map_err(|e| format!("{object}: {e}"))?;
            // --range start end (or start..end, start+length): the original's code to
            // compare, whatever extent the notes give the function.
            let range = match rest.iter().position(|a| *a == "--range") {
                Some(i) => {
                    let first = rest
                        .get(i + 1)
                        .copied()
                        .ok_or("--range takes <start> <end>, start..end or start+length")?;
                    let second = rest.get(i + 2).copied().filter(|s| !s.starts_with("--"));
                    let range = binviz::matching::parse_range(first, second)
                        .ok_or_else(|| format!("not a range: {first} {}", second.unwrap_or("")))?;
                    rest.drain(i..i + 2 + second.is_some() as usize);
                    Some(range)
                }
                None => None,
            };
            match rest.first() {
                Some(name) => {
                    let funcs = binviz::matching::object_functions(&bytes).map_err(|e| e.to_string())?;
                    let f = binviz::matching::find_function(&funcs, name)
                        .ok_or_else(|| format!("no function {name} in {object}"))?;
                    bin.check_isa(f.isa).map_err(|e| e.to_string())?;
                    let m = match range {
                        Some((start, end)) => bin.match_range(start, end, f).map_err(|e| e.to_string())?,
                        None => {
                            let addr = match bin.object_symbol_address(&f.name) {
                                Some(a) => a,
                                None => resolve_address(&bin, name)?,
                            };
                            bin.match_function(addr, f).ok_or("not in a function")?
                        }
                    };
                    print!("{}", m.to_text());
                }
                None if range.is_some() => {
                    return Err("--range needs the function's name in the object: binviz match <file> <object> <name> --range <start> <end>".into());
                }
                None => {
                    let unit = bin.match_unit(object, &bytes).map_err(|e| e.to_string())?;
                    if unit.functions.is_empty() {
                        return Err("no function of the object has a name the binary knows".into());
                    }
                    if json {
                        let scores: Vec<_> = unit.functions.iter().map(|m| m.score(&unit.unit)).collect();
                        println!("{}", serde_json::to_string_pretty(&scores).map_err(|e| e.to_string())?);
                        return Ok(());
                    }
                    let matched = unit.exact();
                    print!("{} functions compared, {matched} match exactly", unit.functions.len());
                    if !unit.unplaced.is_empty() {
                        print!("; not named in the binary: {}", unit.unplaced.join(", "));
                    }
                    println!("\n");
                    for m in &unit.functions {
                        print!("{}", m.to_text());
                    }
                }
            }
        }
        "rank" => {
            // Variants (objects, or folders of them), ranked closest first by distance.
            let mut rest: Vec<&str> = args[2..].iter().map(String::as_str).collect();
            let json = take_flag(&mut rest, "--json");
            let strict = take_flag(&mut rest, "--strict-relocs");
            binviz::matching::STRICT_RELOCS.store(strict, std::sync::atomic::Ordering::Relaxed);
            let cache = if take_flag(&mut rest, "--no-cache") || strict {
                None
            } else {
                binviz::matching::cache::ScoreCache::open(None)
            };
            let function = take_value(&mut rest, "--function");
            let top = match take_value(&mut rest, "--top") {
                Some(n) => num(&n)? as usize,
                None => 20,
            };
            if rest.is_empty() {
                return Err("binviz rank <file> <object|folder>... [--function name] [--top N] [--json]".into());
            }
            let variants = objects_in(&rest.iter().map(|s| s.to_string()).collect::<Vec<_>>())?;
            if variants.is_empty() {
                return Err("no object files (.o, .obj) given".into());
            }
            let mut ranking = bin.rank_variants(&variants, function.as_deref(), cache.as_ref());
            if json {
                ranking.scores.truncate(top);
                println!("{}", serde_json::to_string_pretty(&ranking).map_err(|e| e.to_string())?);
            } else {
                print!("{}", ranking.to_text(top));
            }
        }
        "resolve" => {
            let what = arg(2).ok_or("binviz resolve <file> <name|address>")?;
            let address = resolve_address(&bin, what)?;
            print!("{}", resolve_text(&bin, what, address));
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
            let second = arg(2)
                .ok_or("binviz splat <file> <name> [dir] [split...], or splat <file> import <symbol_addrs.txt>")?;
            if second == "import" {
                let path = arg(3).ok_or("which symbol file?")?;
                let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
                let notes = binviz::splat::parse_symbol_addrs(&text);
                eprintln!("{} names", notes.len());
                println!("{}", serde_json::to_string_pretty(&notes).map_err(|e| e.to_string())?);
            } else {
                let splits: Vec<u64> = args
                    .get(4..)
                    .unwrap_or(&[])
                    .iter()
                    .map(|a| num(a))
                    .collect::<Result<_, _>>()?;
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
            let mut sigs = binviz::sigs::SignatureSet::default();
            let mut paths: Vec<std::path::PathBuf> = Vec::new();
            let mut as_notes = false;
            for a in args.get(2..).unwrap_or(&[]) {
                if a == "notes" {
                    as_notes = true;
                    continue;
                }
                let path = std::path::Path::new(a);
                if path.is_dir() {
                    // Psy-Q's .LIB and .OBJ, MSVC's .lib and .obj, a GNU toolchain's .a and .o.
                    let mut found: Vec<_> = std::fs::read_dir(path)
                        .map_err(|e| format!("{a}: {e}"))?
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| {
                            p.extension()
                                .and_then(|e| e.to_str())
                                .is_some_and(|e| ["lib", "obj", "a", "o"].iter().any(|x| e.eq_ignore_ascii_case(x)))
                        })
                        .collect();
                    found.sort();
                    paths.extend(found);
                } else {
                    paths.push(path.to_path_buf());
                }
            }
            if paths.is_empty() {
                // Without the libraries, what the code itself says about them.
                print!("{}", binviz::rcs::sources_text(&bin.library_sources()));
                println!(
                    "To name the library functions, give the libraries: binviz sdk <file> <lib|folder...> [notes]"
                );
                return Ok(());
            }
            for path in &paths {
                let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
                match sigs.add_file(&path.to_string_lossy(), &bytes) {
                    Ok(n) => eprintln!("{}: {n} signatures", path.display()),
                    Err(e) => eprintln!("{}: {e}", path.display()),
                }
            }
            for note in &sigs.notes {
                eprintln!("{note}");
            }
            let r = bin.identify_sdk(&sigs);
            if as_notes {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&r.annotations()).map_err(|e| e.to_string())?
                );
            } else {
                print!("{}", r.to_text());
            }
        }
        "fieldrefs" => {
            let global = resolve_address(
                &bin,
                arg(2).ok_or("which global pointer? binviz fieldrefs <file> <global> [offset]")?,
            )?;
            let offset = match arg(3) {
                Some(o) => {
                    let (neg, digits) = match o.strip_prefix('-') {
                        Some(d) => (true, d),
                        None => (false, o.strip_prefix('+').unwrap_or(o)),
                    };
                    let v = num(digits)? as i64;
                    Some(if neg { -v } else { v })
                }
                None => None,
            };
            print!("{}", bin.field_refs_text(global, offset));
        }
        "libraries" => print!("{}", binviz::rcs::sources_text(&bin.library_sources())),
        "locate" => {
            let path = arg(2).ok_or("which file? binviz locate <ram.bin> <file>")?;
            let blob = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            if !bin.is_psx_memory() {
                return Err("locate is for PlayStation memory images (2 MiB of RAM dumped by an emulator)".into());
            }
            match bin.psx_locate(&blob) {
                Some(at) => println!("{path} is loaded at {at:#x} ({} bytes)", blob.len()),
                None => {
                    // An archive of files, whatever its format: the stretches of it loaded here.
                    let pieces = bin.psx_loaded_pieces(&blob);
                    if pieces.is_empty() {
                        println!("{path} is not in this image");
                    }
                    for p in &pieces {
                        println!(
                            "{path} {:#x}..{:#x} ({} bytes) is loaded at {:#x}..{:#x}, {:.1}% the same",
                            p.offset,
                            p.offset + p.size,
                            p.size,
                            p.address,
                            p.address + p.size,
                            p.same * 100.0
                        );
                    }
                }
            }
        }
        "asm" | "m2c" => {
            let mut rest: Vec<&str> = args[2..].iter().map(String::as_str).collect();
            let bare = take_flag(&mut rest, "--bare");
            let all = take_flag(&mut rest, "--all");
            let out_dir = take_value(&mut rest, "--out");
            let list = take_value(&mut rest, "--list");
            if cmd == "asm" {
                // Every function asked for (names or addresses, a file of them, or all), in one
                // process: to stdout one after another, or each to <name>.s in --out.
                let mut wanted: Vec<String> = rest.iter().map(|s| s.to_string()).collect();
                if let Some(list) = &list {
                    let text = std::fs::read_to_string(list).map_err(|e| format!("{list}: {e}"))?;
                    // The first word of each line (a lanes file's address, a plain list's name).
                    wanted.extend(
                        text.lines()
                            .map(str::trim)
                            .filter(|l| !l.is_empty() && !l.starts_with('#'))
                            .filter_map(|l| l.split_whitespace().next().map(str::to_string)),
                    );
                }
                if all {
                    wanted.extend(bin.symbols().functions().map(|f| format!("{:#x}", f.address)));
                }
                if wanted.is_empty() {
                    return Err(
                        "binviz asm <file> <addr|symbol>... [--list <file>] [--all] [--out <dir>] [--bare]".into(),
                    );
                }
                if let Some(dir) = &out_dir {
                    std::fs::create_dir_all(dir).map_err(|e| format!("{dir}: {e}"))?;
                }
                let (mut written, mut failed) = (0, Vec::new());
                let mut done = std::collections::HashSet::new();
                for what in &wanted {
                    let Ok(addr) = resolve_address(&bin, what) else {
                        failed.push(format!("{what}: no such symbol"));
                        continue;
                    };
                    let Some(f) = bin.symbols().function_containing(addr) else {
                        failed.push(format!("{what}: not in a function"));
                        continue;
                    };
                    if !done.insert(f.address) {
                        continue;
                    }
                    let Some(text) = bin.gnu_asm_with(f.address, bare) else {
                        failed.push(format!("{what}: not MIPS code"));
                        continue;
                    };
                    match &out_dir {
                        Some(dir) => {
                            let name = f
                                .display_name()
                                .replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_");
                            let path = std::path::Path::new(dir).join(format!("{name}.s"));
                            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
                            written += 1;
                        }
                        None => print!("{text}{}", if wanted.len() > 1 { "\n" } else { "" }),
                    }
                }
                if let Some(dir) = &out_dir {
                    eprintln!("{written} functions written to {dir}");
                }
                if !failed.is_empty() {
                    if wanted.len() == 1 {
                        return Err(failed.remove(0));
                    }
                    eprintln!("{} not written:\n  {}", failed.len(), failed.join("\n  "));
                }
            } else {
                let addr = resolve_address(&bin, rest.first().copied().ok_or("missing address or symbol")?)?;
                let text = bin
                    .gnu_asm(addr)
                    .ok_or("not in a function, or not MIPS code (GNU assembler source is for MIPS so far)")?;
                // m2c's first draft of the C: the command given (default m2c), run on the source
                // written out, with a context file of prototypes (the function's and its
                // callees', from the notes' types or the code) so it doesn't invent arguments.
                let m2c = rest.get(1).copied().unwrap_or("m2c");
                let stem = std::env::temp_dir().join(format!("binviz-{}-{addr:x}", std::process::id()));
                let path = stem.with_extension("s");
                std::fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
                let mut extra: Vec<String> = Vec::new();
                let context = stem.with_extension("ctx.c");
                if m2c.contains("m2c")
                    && let Some(ctx) = bin.m2c_context(addr)
                    && std::fs::write(&context, ctx).is_ok()
                {
                    extra.push("--context".into());
                    extra.push(context.to_string_lossy().into_owned());
                }
                extra.push(path.to_string_lossy().into_owned());
                let ran = run_line(m2c, &extra);
                let _ = std::fs::remove_file(&path);
                let _ = std::fs::remove_file(&context);
                match ran {
                    Ok(o) if o.status.success() => print!("{}", String::from_utf8_lossy(&o.stdout)),
                    // It ran, and says (in a C comment) why it couldn't.
                    Ok(o) if !o.stdout.is_empty() => {
                        print!("{text}\n{}", String::from_utf8_lossy(&o.stdout));
                        return Err(format!(
                            "{m2c} couldn't decompile it: its reason is above, after the source"
                        ));
                    }
                    Ok(o) => {
                        print!("{text}");
                        return Err(format!(
                            "{m2c} failed (install m2c, or give its command: binviz m2c <file> <fn> \"python3 m2c.py\"):\n{}",
                            String::from_utf8_lossy(&o.stderr).trim()
                        ));
                    }
                    Err(e) => return Err(format!("{m2c}: {e}")),
                }
            }
        }
        "flags" => {
            let usage = "binviz flags <file> <source|folder> \"<command with {src} {out} {flags}>\" [\"<flags>\"...] [--jobs N] [--function name] [--top N] [--json]";
            let mut rest: Vec<&str> = args[2..].iter().map(String::as_str).collect();
            let jobs = match take_value(&mut rest, "--jobs") {
                Some(n) => (num(&n)? as usize).max(1),
                None => 1,
            };
            let json = take_flag(&mut rest, "--json");
            let function = take_value(&mut rest, "--function");
            let record = take_flag(&mut rest, "--record");
            let meta = take_value(&mut rest, "--meta");
            let top = match take_value(&mut rest, "--top") {
                Some(n) => num(&n)? as usize,
                None => 20,
            };
            let (Some(&src), Some(&command)) = (rest.first(), rest.get(1)) else {
                return Err(usage.into());
            };
            let sets: Vec<String> = rest[2..].iter().map(|s| s.to_string()).collect();
            let batch = command.contains("{srcdir}") && command.contains("{outdir}");
            if !batch && !command.contains("{out}") {
                return Err(format!(
                    "{usage}\n(the command compiles {{src}} with {{flags}} into the object {{out}}; or, given a folder\nof sources {{srcdir}}, compiles each into {{outdir}} under the same name with a .o extension)"
                ));
            }
            // The variants: each source (a file, or every .c and .cpp in a folder) with each set of flags.
            let src_path = std::path::Path::new(src);
            let mut sources: Vec<std::path::PathBuf> = if src_path.is_dir() {
                let mut v: Vec<_> = std::fs::read_dir(src_path)
                    .map_err(|e| format!("{src}: {e}"))?
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| {
                        p.extension()
                            .is_some_and(|x| x == "c" || x == "cpp" || x == "cc" || x == "s")
                    })
                    .collect();
                v.sort();
                v
            } else {
                vec![src_path.to_path_buf()]
            };
            if sources.is_empty() {
                return Err(format!("no sources in {src}"));
            }
            if sets.is_empty() && batch {
                sources.sort();
            }
            let sets: Vec<String> = if sets.is_empty() { vec![String::new()] } else { sets };
            if sets.len() > 1 && batch {
                return Err(
                    "a batch command ({srcdir}/{outdir}) takes one set of flags; give variants as files instead".into(),
                );
            }
            let dir = std::env::temp_dir().join(format!("binviz-flags-{}", std::process::id()));
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            // (label, source, flags, object path)
            let mut plan: Vec<(String, std::path::PathBuf, String, std::path::PathBuf)> = Vec::new();
            for (k, source) in sources.iter().enumerate() {
                for (i, flags) in sets.iter().enumerate() {
                    let stem = source
                        .file_stem()
                        .map_or_else(|| format!("src{k}"), |n| n.to_string_lossy().into_owned());
                    let label = match (sources.len() > 1, flags.is_empty()) {
                        (true, true) => stem.clone(),
                        (true, false) => format!("{stem} [{flags}]"),
                        (false, _) => flags.clone(),
                    };
                    let out = if batch {
                        dir.join("out").join(format!("{stem}.o"))
                    } else {
                        dir.join(format!("build-{k}-{i}.o"))
                    };
                    plan.push((label, source.clone(), flags.clone(), out));
                }
            }
            let outcome = |ran: std::io::Result<std::process::Output>, out: &std::path::Path| match ran {
                Ok(o) if o.status.success() => {
                    std::fs::read(out).map_err(|e| binviz::Error::new(format!("no object: {e}")))
                }
                Ok(o) => {
                    let err = String::from_utf8_lossy(&o.stderr);
                    Err(binviz::Error::new(format!(
                        "didn't compile: {}",
                        err.lines().next().unwrap_or("")
                    )))
                }
                Err(e) => Err(binviz::Error::new(e.to_string())),
            };
            let mut builds: Vec<(String, binviz::Result<Vec<u8>>)> = Vec::new();
            if batch {
                // One command for all the sources: the caller fans out inside it (one container call).
                let srcdir = dir.join("src");
                let outdir = dir.join("out");
                std::fs::create_dir_all(&srcdir).map_err(|e| format!("{}: {e}", srcdir.display()))?;
                std::fs::create_dir_all(&outdir).map_err(|e| format!("{}: {e}", outdir.display()))?;
                for (_, source, _, _) in &plan {
                    let to = srcdir.join(source.file_name().unwrap_or_default());
                    std::fs::copy(source, &to).map_err(|e| format!("{}: {e}", source.display()))?;
                }
                let line = command
                    .replace("{srcdir}", &shell_quote(&srcdir.to_string_lossy()))
                    .replace("{outdir}", &shell_quote(&outdir.to_string_lossy()))
                    .replace("{flags}", &sets[0]);
                let ran = run_line(&line, &[]);
                let stderr = ran
                    .as_ref()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stderr).into_owned());
                for (label, _, _, out) in &plan {
                    let object = match &ran {
                        Ok(_) if out.exists() => std::fs::read(out).map_err(|e| binviz::Error::new(e.to_string())),
                        Ok(_) => Err(binviz::Error::new(format!(
                            "no object written: {}",
                            stderr.as_deref().and_then(|e| e.lines().next()).unwrap_or("")
                        ))),
                        Err(e) => Err(binviz::Error::new(e.to_string())),
                    };
                    builds.push((label.clone(), object));
                }
            } else {
                // Each variant its own command, `jobs` at a time.
                let results: std::sync::Mutex<Vec<(usize, binviz::Result<Vec<u8>>)>> =
                    std::sync::Mutex::new(Vec::new());
                let next = std::sync::atomic::AtomicUsize::new(0);
                std::thread::scope(|scope| {
                    for _ in 0..jobs.min(plan.len()) {
                        scope.spawn(|| {
                            loop {
                                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                let Some((label, source, flags, out)) = plan.get(i) else {
                                    break;
                                };
                                let line = command
                                    .replace("{src}", &shell_quote(&source.to_string_lossy()))
                                    .replace("{out}", &shell_quote(&out.to_string_lossy()))
                                    .replace("{flags}", flags);
                                let object = outcome(run_line(&line, &[]), out);
                                eprintln!("{label}: {}", if object.is_ok() { "compiled" } else { "failed" });
                                results.lock().unwrap().push((i, object));
                            }
                        });
                    }
                });
                let mut results = results.into_inner().unwrap();
                results.sort_by_key(|r| r.0);
                for (i, object) in results {
                    builds.push((plan[i].0.clone(), object));
                }
            }
            let _ = std::fs::remove_dir_all(&dir);
            if function.is_some() || sources.len() > 1 {
                // Variants of a function: closest first by distance.
                let (ok, failed): (Vec<_>, Vec<_>) = builds.into_iter().partition(|b| b.1.is_ok());
                let variants: Vec<(String, Vec<u8>)> =
                    ok.into_iter().map(|(l, o)| (l, o.unwrap_or_default())).collect();
                let mut ranking = bin.rank_variants(&variants, function.as_deref(), None);
                ranking.failed.extend(
                    failed
                        .into_iter()
                        .map(|(l, e)| (l, e.err().map_or(String::new(), |e| e.to_string()))),
                );
                if json {
                    ranking.scores.truncate(top);
                    println!("{}", serde_json::to_string_pretty(&ranking).map_err(|e| e.to_string())?);
                } else {
                    print!("{}", ranking.to_text(top));
                }
                if record {
                    // Each function's best score among the variants, into the notes.
                    let mut best: std::collections::BTreeMap<u64, binviz::matching::FunctionProgress> =
                        Default::default();
                    for s in &ranking.scores {
                        let e = best
                            .entry(s.address)
                            .or_insert_with(|| binviz::matching::FunctionProgress {
                                address: s.address,
                                name: s.name.clone(),
                                unit: s.unit.clone(),
                                size: s.original_bytes,
                                percent: s.percent,
                            });
                        if s.percent > e.percent {
                            e.percent = s.percent;
                            e.unit = s.unit.clone();
                        }
                    }
                    let outcomes: Vec<_> = best.into_values().collect();
                    record_outcomes(&bin, &args[1], notes, meta.as_deref(), &outcomes)?;
                }
            } else {
                let scores = bin.rank_builds(&builds);
                if json {
                    println!("{}", serde_json::to_string_pretty(&scores).map_err(|e| e.to_string())?);
                } else {
                    print!("{}", binviz::matching::builds_text(&scores));
                }
            }
        }
        "progress" => {
            let report = bin.progress_report();
            if arg(2) == Some("json") {
                println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
            } else {
                println!("{}", bin.decomp_progress().summary());
                let areas = bin.decomp_progress_by_area();
                if areas.len() > 1 {
                    for (name, p) in &areas {
                        println!("  {name}: {}", p.summary());
                    }
                }
                print!("{}", progress_text(&report));
            }
        }
        "counterparts" => {
            let dir = arg(2).ok_or("which source? binviz counterparts <file> <source folder> [n]")?;
            let files =
                binviz::csource::read_source_tree(std::path::Path::new(dir)).map_err(|e| format!("{dir}: {e}"))?;
            if files.is_empty() {
                return Err(format!("{dir}: no C or C++ files"));
            }
            if bin.xrefs_supported() {
                bin.prepare_xrefs();
            }
            let n = arg(3).map(num).transpose()?.unwrap_or(100) as usize;
            print!("{}", bin.source_counterparts(&files).to_text(n));
        }
        "names" => {
            let path = arg(2).ok_or("which names? binviz names <file> <candidates.json|named build> [min%]")?;
            let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let p = match binviz::names::parse_candidates(&data) {
                Ok(candidates) => bin.propose_names(&candidates),
                // Another build with names: its functions paired with these by their code.
                Err(_) => bin.port_names(&open_with_its_debug_file(path, data)?),
            };
            match arg(3) {
                Some(min) => {
                    let min = num(min.trim_end_matches('%'))? as f32 / 100.0;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&p.annotations(min)).map_err(|e| e.to_string())?
                    );
                }
                None => print!("{}", p.to_text()),
            }
        }
        _ => return Err(format!("unknown command {cmd}\n\n{USAGE}")),
    }
    Ok(())
}

/// `s` as one word of a shell command.
fn shell_quote(s: &str) -> String {
    if cfg!(windows) {
        format!("\"{s}\"")
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Runs a command line the user gave (`m2c`, `python3 m2c.py`, a compiler
/// with its flags) with `args` after it. Through `sh` on Unix, so pipes and
/// quoting work as in a shell; on Windows without `cmd`, which strips the
/// quotes off a line that starts with one (`"C:\\Program Files\\..."`),
/// so the line is split into its words here, quotes respected.
fn run_line(line: &str, args: &[String]) -> std::io::Result<std::process::Output> {
    if cfg!(windows) {
        let words = split_words(line);
        let Some((program, rest)) = words.split_first() else {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "no command"));
        };
        std::process::Command::new(program).args(rest).args(args).output()
    } else {
        let mut full = line.to_string();
        for a in args {
            full.push(' ');
            full.push_str(&shell_quote(a));
        }
        std::process::Command::new("sh").args(["-c", &full]).output()
    }
}

/// A command line's words: split on spaces, with double or single quotes
/// holding a word together (and dropped).
fn split_words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut any = false;
    for c in line.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => word.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                any = true;
            }
            None if c.is_whitespace() => {
                if any || !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                    any = false;
                }
            }
            None => word.push(c),
        }
    }
    if any || !word.is_empty() {
        out.push(word);
    }
    out
}

/// An objdiff report's totals and units, one line each.
fn progress_text(report: &serde_json::Value) -> String {
    let line = |name: &str, m: &serde_json::Value| {
        let n = |k: &str| {
            m[k].as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| m[k].as_u64())
                .unwrap_or(0)
        };
        format!(
            "{name:<40} {:>5} of {:>5} functions, {:>8} of {:>8} bytes ({:>5.1}%), fuzzy {:>5.1}%\n",
            n("matched_functions"),
            n("total_functions"),
            n("matched_code"),
            n("total_code"),
            m["matched_code_percent"].as_f64().unwrap_or(0.0),
            m["fuzzy_match_percent"].as_f64().unwrap_or(0.0)
        )
    };
    let mut out = line("TOTAL", &report["measures"]);
    for c in report["categories"].as_array().into_iter().flatten() {
        out.push_str(&line(
            &format!("  {}", c["name"].as_str().unwrap_or("")),
            &c["measures"],
        ));
    }
    for u in report["units"].as_array().into_iter().flatten() {
        out.push_str(&line(u["name"].as_str().unwrap_or(""), &u["measures"]));
    }
    out
}

/// Another build, with the debug file it names when that is beside it (a PE's PDB).
fn open_with_its_debug_file(path: &str, data: Vec<u8>) -> Result<Binary, String> {
    let mut bin = Binary::parse(data).map_err(|e| format!("{path}: {e}"))?;
    let named = bin.summary().debug_link.clone();
    if let Some(link) = named {
        let name = link
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(&link)
            .split(" (crc ")
            .next()
            .unwrap_or(&link)
            .to_string();
        let beside = std::path::Path::new(path).with_file_name(&name);
        if let Ok(bytes) = std::fs::read(&beside) {
            let _ = bin.attach_debug_file(&beside.to_string_lossy(), bytes);
        }
    }
    Ok(bin)
}

/// Records a batch's outcomes in the notes file (`--notes`, else the one
/// beside the binary), through its journal, as the MCP server's `record`
/// does: functions at 100% matched (with the build `meta` names), the
/// others' best percent kept, those no longer matching sent back to do.
fn record_outcomes(
    bin: &Binary,
    file: &str,
    notes: Option<&str>,
    meta: Option<&str>,
    outcomes: &[binviz::matching::FunctionProgress],
) -> Result<(), String> {
    let path = notes
        .map(str::to_string)
        .unwrap_or_else(|| format!("{file}.binviz-notes.json"));
    let build = match meta {
        Some(m) => binviz::queue::BuildInfo::parse(m)?,
        None => binviz::queue::BuildInfo::default(),
    };
    let (before, _, folded) = binviz::notes::read(std::path::Path::new(&path))?;
    let mut after = before.clone();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (matched, lost) = binviz::queue::record_scores(&mut after, outcomes, &build, now);
    let name = std::path::Path::new(file)
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let lines = binviz::notes::write(
        std::path::Path::new(&path),
        &name,
        &bin.summary().fingerprint,
        &before,
        &after,
        folded,
    )?;
    eprintln!(
        "Recorded in {path}: {matched} newly matched, {} no longer match, {lines} journal lines{}",
        lost.len(),
        if build == binviz::queue::BuildInfo::default() {
            " (no --meta compiler=…: a match with no compiler can't be reused elsewhere)"
        } else {
            ""
        }
    );
    Ok(())
}

/// Removes `name` from the arguments, saying whether it was there.
fn take_flag(rest: &mut Vec<&str>, name: &str) -> bool {
    match rest.iter().position(|a| *a == name) {
        Some(i) => {
            rest.remove(i);
            true
        }
        None => false,
    }
}

/// Removes `name` and its value from the arguments, giving the value.
fn take_value(rest: &mut Vec<&str>, name: &str) -> Option<String> {
    let i = rest.iter().position(|a| *a == name)?;
    let value = rest.get(i + 1).map(|v| v.to_string());
    rest.drain(i..(i + 2).min(rest.len()));
    value
}

/// The object files among `paths` (each a file, or a folder searched with its
/// subfolders), named by their path in the folder given: (unit, bytes).
fn objects_in(paths: &[String]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut objects = Vec::new();
    for p in paths {
        let path = std::path::Path::new(p);
        if path.is_dir() {
            for f in binviz::matching::object_files(path).map_err(|e| format!("{p}: {e}"))? {
                let bytes = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
                let unit = f.strip_prefix(path).unwrap_or(&f).to_string_lossy().replace('\\', "/");
                objects.push((unit, bytes));
            }
        } else {
            let bytes = std::fs::read(path).map_err(|e| format!("{p}: {e}"))?;
            objects.push((p.clone(), bytes));
        }
    }
    Ok(objects)
}

/// Where a name or address is now: the function holding it.
fn resolve_text(bin: &Binary, what: &str, address: u64) -> String {
    let mut out = format!("{what} is {address:#x}");
    match bin.symbols().function_containing(address) {
        Some(f) if f.address == address => {
            out.push_str(&format!(": the start of {} ({} bytes)", f.display_name(), f.size));
        }
        Some(f) => {
            out.push_str(&format!(
                ": inside {} ({:#x}, {} bytes), at offset {:#x}",
                f.display_name(),
                f.address,
                f.size,
                address - f.address
            ));
        }
        None => out.push_str(": in no known function"),
    }
    if let Some(s) = bin.symbols().at(address).filter(|s| s.address == address) {
        out.push_str(&format!("; named {}", s.display_name()));
    }
    let noted: Vec<String> = bin
        .annotations()
        .iter()
        .filter(|a| a.address == address && !a.name.is_empty())
        .map(|a| a.name.clone())
        .collect();
    if !noted.is_empty() {
        out.push_str(&format!("; notes there: {}", noted.join(", ")));
    }
    out.push('\n');
    out
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

/// `files` and `extract`: what a disc, universal binary or archive holds, and one of its members
/// (or, for any file, `@offset+length` of its bytes) written out.
fn files_or_extract(args: &[String], list: bool) -> Result<(), String> {
    let path = &args[0];
    let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let spec = args.get(1).map(String::as_str);
    if let Some(range) = spec.and_then(|s| s.strip_prefix('@')) {
        let (from, len) = range
            .split_once('+')
            .ok_or("a stretch is @offset+length, like @0x9800+0xe800")?;
        let (from, len) = (num(from)? as usize, num(len)? as usize);
        let bytes = data
            .get(from..from.checked_add(len).ok_or("that runs past the end")?)
            .ok_or_else(|| format!("{path} has {} bytes: {from:#x}+{len:#x} runs past the end", data.len()))?;
        let out = args.get(2).cloned().unwrap_or_else(|| format!("{from:#x}.bin"));
        std::fs::write(&out, bytes).map_err(|e| format!("{out}: {e}"))?;
        eprintln!("wrote {out} ({len} bytes)");
        return Ok(());
    }
    if !binviz::Container::is_container(&data) {
        return Err(format!(
            "{path} holds no files: it is not a disc image, universal binary or archive"
        ));
    }
    let c = binviz::Container::parse(data).map_err(|e| e.to_string())?;
    if list {
        print!("{}", c.listing());
        return Ok(());
    }
    let spec = spec.ok_or("which member? binviz extract <file> <number|name> [out]  (binviz files lists them)")?;
    let index = c
        .find(spec)
        .ok_or_else(|| format!("no member {spec:?}; binviz files {path} lists them"))?;
    let bytes = c.member_data(index).map_err(|e| e.to_string())?;
    let name = &c.members()[index as usize].name;
    let out = args.get(2).cloned().unwrap_or_else(|| {
        let last = name.rsplit('/').next().unwrap_or(name);
        last.split(';').next().unwrap_or(last).to_string()
    });
    std::fs::write(&out, &*bytes).map_err(|e| format!("{out}: {e}"))?;
    eprintln!("wrote {out} ({} bytes)", bytes.len());
    Ok(())
}

/// `blobs`: the code in a file binviz has no reader for, and where each run loads.
fn blobs(path: &str, exe: Option<&str>) -> Result<(), String> {
    let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    // The boot executable's calls say nothing about where an overlay loads.
    let skip = match exe {
        Some(e) => {
            let head = std::fs::read(e).map_err(|e2| format!("{e}: {e2}"))?;
            Some(binviz::blobs::exe_range(&head).ok_or_else(|| format!("{e} is not a PS-X EXE"))?)
        }
        None => None,
    };
    print!(
        "{}",
        binviz::blobs::blobs_text(&binviz::blobs::find_code_blobs(&data, skip))
    );
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
    if let Some(g) = &i.global {
        println!(
            "global: {}+{:#x} ({} bytes): {}",
            g.name,
            i.address.unwrap_or(g.address) - g.address,
            g.size,
            g.description
        );
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
    if let Some(s) = &i.string {
        println!(
            "string ({}, {} bytes{}): \"{}\"",
            s.encoding,
            s.size,
            s.address.map(|a| format!(" at {a:#x}")).unwrap_or_default(),
            s.text.replace('\n', "⏎")
        );
    } else if let Some(ins) = &i.instruction {
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
        format!(
            "{report_path}: not a crash report (an Apple .crash or .ips, an Android tombstone, or a stack trace, native or WebAssembly)"
        )
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
