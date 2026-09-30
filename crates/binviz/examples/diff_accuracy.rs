//! How many of `diff_functions`' pairs are right: an old build with names,
//! a stripped new build, and the new build before it was stripped (the same
//! addresses, with names) to check the pairs against.
//!
//!     cargo run --release -p binviz --example diff_accuracy -- \
//!         old new.stripped new [old.notes.json new.notes.json]
//!
//! Notes files name the functions of builds with no symbols (a PS-X EXE).
//! GCC's clones (`f.isra.0`, `f.constprop.1`) count as `f`. With SHOW set,
//! every pair is listed, right or wrong.

use std::collections::{HashMap, HashSet};

fn load(p: &str) -> binviz::Binary {
    let data = binviz::read_file(std::path::Path::new(p)).unwrap_or_else(|e| panic!("{p}: {e}"));
    binviz::package::load_binary(data)
        .unwrap_or_else(|e| panic!("{p}: {e}"))
        .0
}

fn names(b: &binviz::Binary) -> HashMap<u64, String> {
    b.symbols()
        .functions()
        .filter(|f| f.size > 0)
        .map(|f| {
            let n = f.display_name();
            (f.address, n.split('.').next().unwrap_or(&n).to_string())
        })
        .collect()
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 3 {
        eprintln!("usage: diff_accuracy <old> <new.stripped> <new> [old.notes.json new.notes.json]");
        std::process::exit(2);
    }
    let (mut old, new, mut truth) = (load(&a[0]), load(&a[1]), load(&a[2]));
    if let (Some(n_old), Some(n_truth)) = (a.get(3), a.get(4)) {
        let notes = |p: &str| binviz::notes::parse(&std::fs::read_to_string(p).unwrap()).unwrap().0;
        old.set_annotations(notes(n_old));
        truth.set_annotations(notes(n_truth));
    }
    let (old_names, truth_names) = (names(&old), names(&truth));
    let t = std::time::Instant::now();
    let d = old.diff_functions(&new);
    let took = t.elapsed();
    let show = std::env::var("SHOW").is_ok();
    let mut by: HashMap<String, (u32, u32)> = HashMap::new();
    let (mut right, mut wrong) = (0, 0);
    for p in &d.pairs {
        let (Some(x), Some(y)) = (old_names.get(&p.old.address), truth_names.get(&p.new.address)) else {
            continue;
        };
        let e = by.entry(format!("{:?}", p.how)).or_default();
        if x == y {
            right += 1;
            e.0 += 1;
        } else {
            wrong += 1;
            e.1 += 1;
        }
        if show {
            let verdict = if x == y { "right" } else { "WRONG" };
            println!("{verdict} {:?} {x} -> {y} ({:.2})", p.how, p.similarity);
        }
    }
    // The new build's functions with a namesake in the old one: what could be paired.
    let old_set: HashSet<&String> = old_names.values().collect();
    let possible = truth_names.values().filter(|n| old_set.contains(n)).count();
    let mut how: Vec<_> = by.into_iter().collect();
    how.sort();
    println!(
        "{} pairs: {right} right, {wrong} wrong; {possible} could be paired ({:.1}% found right) in {} ms",
        d.pairs.len(),
        100.0 * right as f64 / possible.max(1) as f64,
        took.as_millis()
    );
    for (k, (r, w)) in how {
        println!("  {k:<12} {r:>5} right {w:>4} wrong");
    }
}
