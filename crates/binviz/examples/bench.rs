//! Times what the UI asks of a binary, and how much memory each step holds on
//! to, to find what gets slow on large files.
//!
//! ```text
//! cargo run --release -p binviz --example bench -- path/to/binary
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering::Relaxed};
use std::time::Instant;

use binviz::{Annotation, AttributionMode, Binary, HitKind, SymbolQuery, Target};

/// Counts live heap bytes so each step's footprint can be reported.
struct Counting;

static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);

fn grew(by: isize) {
    let now = LIVE.fetch_add(by, Relaxed) + by;
    PEAK.fetch_max(now, Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            grew(layout.size() as isize);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size() as isize, Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            grew(new_size as isize - layout.size() as isize);
        }
        p
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn mb(n: isize) -> String {
    format!("{:+.1} MB", n as f64 / 1_048_576.0)
}

fn time<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let before = LIVE.load(Relaxed);
    let peak_before = PEAK.swap(before, Relaxed);
    let t = Instant::now();
    let r = f();
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    let (after, peak) = (LIVE.load(Relaxed), PEAK.load(Relaxed));
    PEAK.fetch_max(peak_before, Relaxed);
    let flag = if ms >= 1000.0 {
        "  <-- slow"
    } else if ms >= 200.0 {
        "  <-"
    } else {
        ""
    };
    println!(
        "  {label:<40} {ms:>9.1} ms {:>12} kept {:>12} peak{flag}",
        mb(after - before),
        mb(peak - before)
    );
    r
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: bench <file>");
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    println!("{path}: {:.1} MB", data.len() as f64 / 1_048_576.0);
    let base = LIVE.load(Relaxed);

    println!("open");
    let mut bin = time("parse (layout, symbols, DWARF index)", || Binary::parse(data).unwrap());
    let s = bin.summary().clone();
    println!(
        "    {} {} · {} sections · {} symbols · DWARF: {}",
        s.format_name, s.arch, s.section_count, s.symbol_count, s.has_dwarf
    );
    time("composition", || bin.composition().len());
    time("regions (root)", || bin.regions(None).len());

    println!("overview");
    time("file_map(1600)", || bin.file_map(1600).len());
    time("entropy_map(1600)", || bin.entropy_map(1600).len());

    let entry = s.entry.or_else(|| bin.symbols().functions().next().map(|f| f.address));
    if let Some(e) = entry {
        println!("inspector / code at {e:#x}");
        time("inspect (first)", || bin.inspect(Target::Address(e)).frames.len());
        time("inspect (again)", || bin.inspect(Target::Address(e)).frames.len());
        time("disassemble_function", || {
            bin.disassemble_function(e, 20_000).instructions.len()
        });
    }
    let funcs = time("functions list", || bin.symbols().functions().count());
    println!("    {funcs} functions");

    println!("symbols / layout / hex");
    time("symbols page (by address)", || {
        bin.symbols()
            .query(&SymbolQuery {
                limit: 200,
                ..Default::default()
            })
            .total
    });
    time("symbols page (filter 'a', by name)", || {
        bin.symbols()
            .query(&SymbolQuery {
                filter: "a".into(),
                sort: "name".into(),
                limit: 200,
                ..Default::default()
            })
            .total
    });
    // The region with the most entries decoded on demand.
    let mut regions = bin.regions(None);
    for r in bin.regions(None) {
        regions.extend(bin.regions(Some(r.id)));
    }
    if let Some(big) = regions
        .iter()
        .filter(|r| r.entry_count.is_some())
        .max_by_key(|r| r.entry_count)
    {
        let n = big.entry_count.unwrap_or(0);
        println!("    biggest table: {} ({n} entries)", big.name);
        time("region entries (middle, 100)", || {
            bin.region_entries(big.id, n / 2, 100).len()
        });
    }
    let text = bin
        .sections()
        .iter()
        .find(|s| s.kind == binviz::RegionKind::Code && s.file_offset.is_some())
        .cloned();
    if let Some(t) = &text {
        let mid = t.file_offset.unwrap() + t.file_size / 2;
        time("hex spans (4 KB of code)", || bin.spans(mid, mid + 4096).len());
    }
    let debug_info = bin.sections().iter().find(|s| s.name.ends_with("debug_info")).cloned();
    if let Some(d) = &debug_info
        && let Some(off) = d.file_offset
    {
        let mid = off + d.file_size / 2;
        time("hex spans (4 KB of .debug_info)", || bin.spans(mid, mid + 4096).len());
    }

    if let Some(d) = bin.debug_info() {
        println!("DWARF / sources");
        let units = d.units().len();
        println!("    {units} units, {} source files", d.source_files().len());
        time("unit root + children", || d.die_children(0, None).len());
        time("DIE search 'main'", || d.search("main", 50).len());
        time("file_line_counts (line index)", || d.file_line_counts().len());
        let busiest = d
            .file_line_counts()
            .iter()
            .enumerate()
            .max_by_key(|&(_, n)| *n)
            .map(|(i, _)| i as u32);
        if let Some(f) = busiest {
            time("file_lines (busiest file)", || d.file_lines(f).len());
        }
        if let Some(e) = entry {
            time("function_die_at(entry)", || d.function_die_at(e));
        }
    }

    println!("search");
    time("prepare (strings, DWARF names)", || bin.prepare_search());
    for q in ["main", "e", "0x1000", "48 89 5c 24 ??", "\"error\""] {
        time(&format!("search {q:?}"), || bin.search(q, 6, None).hits.len());
    }
    time("search 'e', all symbols", || {
        bin.search("e", 300, Some(HitKind::Symbol)).hits.len()
    });
    time("strings page", || bin.strings("", 0, 200).total);

    println!("map");
    if bin.debug_info().is_some() {
        let a = time("attribution by file", || {
            bin.attribution(AttributionMode::File).unwrap()
        });
        time("attribution by unit", || {
            bin.attribution(AttributionMode::Unit).map(|a| a.contributors.len())
        });
        if let Some(top) = a.contributors.first() {
            time("attributed ranges (top file)", || {
                bin.attributed_ranges(AttributionMode::File, top.id).len()
            });
        }
    }
    let cov = time("coverage", || bin.coverage(100));
    time("coverage map (4000)", || bin.coverage_map(4000).len());
    if let Some(sec) = cov.sections.iter().max_by_key(|s| s.size) {
        time("coverage strip (largest section)", || {
            bin.coverage_strip(sec.section, 1500).len()
        });
    }

    if bin.xrefs_supported() {
        println!("references / call graph");
        time("xref index", || bin.prepare_xrefs());
        let c = bin.xref_counts();
        println!("    {} references: {c:?}", c.total());
        // The most-referenced of a sample of functions.
        let busiest = time("reference counts (2000 functions)", || {
            bin.symbols()
                .functions()
                .step_by((funcs / 2000).max(1))
                .map(|f| {
                    (
                        bin.reference_counts(f.address, f.address + f.size.max(1)).total(),
                        f.address,
                    )
                })
                .max()
                .map(|(_, a)| a)
        });
        if let Some(b) = busiest {
            println!(
                "    busiest: {}",
                bin.symbols()
                    .at(b)
                    .map(|s| s.display_name().into_owned())
                    .unwrap_or_default()
            );
            time("callers (busiest)", || bin.callers(b).len());
            time("references_to (busiest, 200)", || {
                bin.references_to(b, b + 1, 0, 200).refs.len()
            });
            time("function_summary (busiest)", || {
                bin.function_summary(b, 30).map(|f| f.caller_count)
            });
        }
        if let Some(e) = entry {
            time("callees (entry)", || bin.callees(e).len());
            time("call_graph (entry, 2 up, 2 down)", || {
                bin.call_graph(e, 2, 2, 12).nodes.len()
            });
            if let Some(b) = busiest {
                let path = time("call_path (entry -> busiest)", || bin.call_path(e, b, 8));
                println!("    path: {:?}", path.map(|p| p.len()));
            }
        }
    }

    println!("annotations");
    let notes: Vec<Annotation> = bin
        .symbols()
        .functions()
        .step_by((funcs / 1000).max(1))
        .take(1000)
        .map(|f| Annotation {
            address: f.address,
            size: 0,
            name: format!("fn_{:x}", f.address),
            comment: "note".into(),
            reviewed: true,
            kind: None,
        })
        .collect();
    time(&format!("set {} annotations", notes.len()), || {
        bin.set_annotations(notes)
    });
    time("coverage after annotating", || bin.coverage(100).gap_count);
    time("clear annotations", || bin.set_annotations(Vec::new()));
    time("coverage, no annotations again", || bin.coverage(100).gap_count);

    println!(
        "total held {:.1} MB (file {:.1} MB)",
        (LIVE.load(Relaxed) - base) as f64 / 1_048_576.0,
        bin.data().len() as f64 / 1_048_576.0
    );
}
