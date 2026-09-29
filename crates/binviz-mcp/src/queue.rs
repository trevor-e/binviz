//! The tools for working through a matching decompilation: which function
//! to take next, where each stands, and which done functions are shaped
//! like one (see `binviz::queue`).

use std::fmt::Write as _;

use binviz::{Annotation, Binary, Decomp, DecompState, NextQuery, Readiness};
use serde_json::Value;

use crate::tools::{Open, address_of, count, human, int, pct, save_notes, string};

/// Seconds since 1970.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// How long a claim holds before others may take the function.
const CLAIM_TTL: u64 = 3600;

/// The start of the function holding `at`.
fn function_start(bin: &Binary, at: &str) -> Result<u64, String> {
    let address = address_of(bin, at)?;
    bin.symbols()
        .function_containing(address)
        .map(|f| f.address)
        .ok_or_else(|| format!("{address:#x} is not inside a known function"))
}

fn name(bin: &Binary, address: u64) -> String {
    bin.symbols()
        .at(address)
        .map_or_else(|| format!("{address:#x}"), |s| s.display_name().into_owned())
}

/// Where a function stands, as in `matched in src/battle.c` or `todo, tried 2×, best 87.5%`.
pub(crate) fn state_text(d: &Decomp) -> String {
    let mut out = d.state.as_str().to_string();
    if d.state == DecompState::InProgress && !d.by.is_empty() {
        let _ = write!(out, " by {}", d.by);
    }
    if !d.source.is_empty() {
        let _ = write!(out, " in {}", d.source);
    }
    if d.attempts > 0 {
        let _ = write!(out, ", tried {}×", d.attempts);
    }
    if let Some(p) = d.percent.filter(|&p| p < 100.0) {
        let _ = write!(out, ", best {p:.1}%");
    }
    out
}

/// Changes where the function starting at `start` stands, in the note at its start.
fn update(o: &mut Open, start: u64, change: impl FnOnce(&mut Decomp)) -> Decomp {
    let mut list: Vec<Annotation> = o.bin.annotations().to_vec();
    let i = list
        .iter()
        .position(|a| a.address == start && a.decomp.is_some())
        .or_else(|| list.iter().position(|a| a.address == start && a.size == 0));
    let mut a = match i {
        Some(i) => list.remove(i),
        None => Annotation {
            address: start,
            size: 0,
            name: String::new(),
            comment: String::new(),
            reviewed: false,
            kind: None,
            decomp: None,
        },
    };
    let mut d = a.decomp.take().unwrap_or_default();
    change(&mut d);
    d.since = now();
    let cleared = d.state == DecompState::Todo && d.attempts == 0 && d.percent.is_none() && d.source.is_empty();
    if !cleared {
        a.decomp = Some(d.clone());
    }
    if a.is_note() || a.decomp.is_some() {
        list.push(a);
    }
    o.bin.set_annotations(list);
    d
}

/// Claims a function for `agent`, saying so if someone else had it.
fn claim(o: &mut Open, start: u64, agent: &str) -> String {
    let taken = o
        .bin
        .decomp_at(start)
        .filter(|d| d.state == DecompState::InProgress && d.by != agent && now() < d.since + CLAIM_TTL)
        .map(|d| format!(" (taking it over from {})", d.by))
        .unwrap_or_default();
    update(o, start, |d| {
        d.state = DecompState::InProgress;
        d.by = agent.to_string();
    });
    taken
}

/// Records objdiff's verdicts in the notes: a function at 100 % is matched
/// (its unit for its source, when it has none), the others keep their best
/// percent, and a function matched before that no longer is goes back to be
/// done. Returns the functions newly matched, and those no longer matching.
pub(crate) fn record_report(o: &mut Open, functions: &[binviz::matching::FunctionProgress]) -> (u32, Vec<u64>) {
    let now = now();
    let mut list: Vec<Annotation> = o.bin.annotations().to_vec();
    let (mut matched, mut lost) = (0, Vec::new());
    for f in functions {
        let i = list
            .iter()
            .position(|a| a.address == f.address && a.decomp.is_some())
            .or_else(|| list.iter().position(|a| a.address == f.address && a.size == 0))
            .unwrap_or_else(|| {
                list.push(Annotation {
                    address: f.address,
                    size: 0,
                    name: String::new(),
                    comment: String::new(),
                    reviewed: false,
                    kind: None,
                    decomp: None,
                });
                list.len() - 1
            });
        let d = list[i].decomp.get_or_insert_with(Decomp::default);
        let before = d.clone();
        if f.percent >= 100.0 {
            if d.state != DecompState::Matched {
                matched += 1;
            }
            d.state = DecompState::Matched;
            d.by.clear();
            d.percent = Some(100.0);
        } else if d.state == DecompState::Matched {
            lost.push(f.address);
            d.state = DecompState::Todo;
            d.percent = Some(f.percent);
        } else {
            d.percent = Some(d.percent.map_or(f.percent, |p| p.max(f.percent)));
        }
        if d.source.is_empty() {
            d.source = f.unit.clone();
        }
        if *d != before {
            d.since = now;
        }
    }
    o.bin.set_annotations(list);
    (matched, lost)
}

pub(crate) fn next_functions(o: &mut Open, args: &Value) -> Result<String, String> {
    let within = match string(args, "within") {
        Some(range) => {
            let (lo, hi) = range.split_once("..").ok_or("within is a range of addresses: lo..hi")?;
            Some((address_of(&o.bin, lo)?, address_of(&o.bin, hi)?))
        }
        None => None,
    };
    let include = string(args, "include").unwrap_or("");
    let q = NextQuery {
        limit: int(args, "count", 10, 200) as usize,
        within,
        include_claimed: matches!(include, "claimed" | "all"),
        include_skipped: matches!(include, "skipped" | "all"),
        now: now(),
        claim_ttl: CLAIM_TTL,
    };
    let list = o.bin.next_functions(&q);
    let p = &list.progress;
    let bin = &o.bin;
    let mut out = String::new();
    if p.functions == 0 {
        return Err("no functions to decompile: this binary's code has no known functions".into());
    }
    let _ = writeln!(
        out,
        "Decompilation: {} of {} functions matched ({} of {} of code, {}), {} nonmatching, {} library, {} in progress, {} set aside.",
        count(p.matched),
        count(p.functions),
        human(p.matched_bytes),
        human(p.bytes),
        pct(p.matched_bytes, p.bytes.saturating_sub(p.library_bytes)),
        count(p.nonmatching),
        count(p.library),
        count(p.in_progress),
        count(p.skipped),
    );
    let _ = writeln!(
        out,
        "Left{}: {} like a done function, {} ready, {} waiting on callees, {} tried without matching; {} claimed.",
        if within.is_some() { " in that range" } else { "" },
        count(list.like_done),
        count(list.ready),
        count(list.waiting),
        count(list.hard),
        count(list.claimed),
    );
    if list.functions.is_empty() {
        let _ = writeln!(out, "Nothing left to list.");
        return Ok(out);
    }
    let _ = writeln!(out, "\nNext, best first:");
    for (i, f) in list.functions.iter().enumerate() {
        let why = match f.readiness {
            Readiness::LikeDone => "like a done one".to_string(),
            Readiness::Ready => "ready".to_string(),
            Readiness::Waiting => {
                let names: Vec<String> = f.waiting_on.iter().take(4).map(|&a| name(bin, a)).collect();
                format!(
                    "waiting on {}: {}{}",
                    f.waiting_on.len(),
                    names.join(", "),
                    if f.waiting_on.len() > 4 { ", …" } else { "" }
                )
            }
            Readiness::Hard => "tried without matching".to_string(),
        };
        let callers = match f.callers {
            0 => "no direct callers".to_string(),
            1 => "1 caller".to_string(),
            n => format!("{n} callers"),
        };
        let mut line = format!(
            "  {:>2}. {} ({:#x}) · {} instructions, {} · {why} · {callers}",
            i + 1,
            name(bin, f.address),
            f.address,
            f.instructions,
            human(f.size),
        );
        if f.unblocks > 0 {
            let _ = write!(line, ", the last callee of {}", f.unblocks);
        }
        if let Some(l) = f.like {
            let _ = write!(
                line,
                " · {:.0}% like {} ({:#x}, {})",
                l.similarity * 100.0,
                name(bin, l.address),
                l.address,
                bin.decomp_at(l.address).map_or_else(|| "todo".into(), state_text),
            );
        }
        if f.attempts > 0 || f.state != DecompState::Todo {
            let d = bin.decomp_at(f.address).cloned().unwrap_or_default();
            let _ = write!(line, " · {}", state_text(&d));
        }
        let _ = writeln!(out, "{line}");
    }
    if args.get("claim").and_then(Value::as_bool).unwrap_or(false) {
        let first = list.functions[0].address;
        let agent = string(args, "agent").unwrap_or("agent").to_string();
        let taken = claim(o, first, &agent);
        let _ = writeln!(
            out,
            "\nClaimed {} ({first:#x}) for {agent}{taken}; mark it matched, nonmatching or attempted when done. {}.",
            name(&o.bin, first),
            save_notes(o)
        );
    } else {
        let _ = writeln!(
            out,
            "\nclaim: true takes the first; disassemble and function_info show it, similar_functions finds worked examples, mark records how it went."
        );
    }
    Ok(out)
}

pub(crate) fn mark(o: &mut Open, args: &Value) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let start = function_start(&o.bin, at)?;
    let state = string(args, "state").ok_or("state is required")?;
    let percent = args
        .get("percent")
        .and_then(Value::as_f64)
        .map(|p| p.clamp(0.0, 100.0) as f32);
    let source = string(args, "source").map(|s| s.trim().to_string());
    let agent = string(args, "agent").unwrap_or("agent").to_string();
    let best = |old: Option<f32>| match (old, percent) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    let mut note = String::new();
    let d = match state.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "attempted" | "attempt" | "tried" => update(o, start, |d| {
            d.attempts += 1;
            d.percent = best(d.percent);
            d.state = DecompState::Todo;
            d.by.clear();
        }),
        "in-progress" | "claim" | "claimed" => {
            note = claim(o, start, &agent);
            o.bin.decomp_at(start).cloned().unwrap_or_default()
        }
        s => {
            let state = DecompState::parse(s).ok_or_else(|| {
                format!("unknown state {s:?}: matched, nonmatching, attempted, in-progress, skipped, library or todo")
            })?;
            update(o, start, |d| {
                d.state = state;
                d.by.clear();
                d.percent = match state {
                    DecompState::Matched => Some(100.0),
                    DecompState::Todo => None,
                    _ => best(d.percent),
                };
                if state == DecompState::Todo {
                    d.attempts = 0;
                }
            })
        }
    };
    if let Some(source) = source {
        update(o, start, |d| d.source = source);
    }
    let d = o.bin.decomp_at(start).cloned().unwrap_or(d);
    let mut out = format!("{} ({start:#x}): {}{note}.", name(&o.bin, start), state_text(&d));
    if d.state.is_done() {
        let ready: Vec<String> = o
            .bin
            .callers_now_ready(start)
            .into_iter()
            .map(|a| format!("{} ({a:#x})", name(&o.bin, a)))
            .collect();
        if !ready.is_empty() {
            let _ = write!(out, " Now ready, everything they call done: {}.", ready.join(", "));
        }
        let like: Vec<String> = o
            .bin
            .similar_functions(start, 8)
            .into_iter()
            .filter(|s| {
                s.similarity >= binviz::queue::TEMPLATE
                    && s.instructions >= binviz::queue::TEMPLATE_INSNS
                    && !o.bin.decomp_at(s.address).is_some_and(|d| d.state.is_done())
            })
            .map(|s| {
                format!(
                    "{} ({:#x}, {:.0}%)",
                    name(&o.bin, s.address),
                    s.address,
                    s.similarity * 100.0
                )
            })
            .collect();
        if !like.is_empty() {
            let _ = write!(out, " Shaped like it, likely the same C: {}.", like.join(", "));
        }
    }
    let p = o.bin.decomp_progress();
    let _ = write!(
        out,
        " {} of {} functions matched. {}.",
        count(p.matched),
        count(p.functions),
        save_notes(o)
    );
    Ok(out)
}

pub(crate) fn similar_functions(o: &Open, args: &Value) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let start = function_start(&o.bin, at)?;
    let n = int(args, "count", 8, 50) as usize;
    let done_only = args.get("done_only").and_then(Value::as_bool).unwrap_or(false);
    let bin = &o.bin;
    let list: Vec<_> = bin
        .similar_functions(start, if done_only { 200 } else { n })
        .into_iter()
        .filter(|s| !done_only || bin.decomp_at(s.address).is_some_and(|d| d.state.is_done()))
        .take(n)
        .collect();
    let mut out = format!(
        "Functions shaped like {} ({start:#x}), most alike first",
        name(bin, start)
    );
    if list.is_empty() {
        out.push_str(": none shares much of its code with it.");
        return Ok(out);
    }
    out.push_str(":\n");
    for s in list {
        let _ = writeln!(
            out,
            "  {:>3.0}%  {} ({:#x}, {} instructions) · {}",
            s.similarity * 100.0,
            name(bin, s.address),
            s.address,
            s.instructions,
            bin.decomp_at(s.address).map_or_else(|| "todo".into(), state_text)
        );
    }
    out.push_str(
        "Alike means the same operations on the same kinds of operands, whatever addresses and numbers they hold.",
    );
    Ok(out)
}
