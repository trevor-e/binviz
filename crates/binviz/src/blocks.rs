//! Runs of instructions that recur across a binary's functions.
//!
//! Every function is read as its instructions with the numbers and names
//! taken out and the registers renamed in order of first use, so
//! `lw $v0, 8($a0)` and `lw $t3, 8($s1)` read alike when the code around them
//! uses its registers alike. Each run of `window` instructions in a row is
//! hashed. Two things come out of that:
//!
//! - the runs that occur in the most functions, which are what the
//!   compiler or the programmer keeps writing (an inlined macro, a loop
//!   over an array, a call sequence);
//! - a trial of the idea that already-decompiled functions help with the
//!   rest: treat the smallest functions as done, and count how much of the
//!   others is made of runs also found in them.

use std::collections::HashMap;

use serde::Serialize;

use crate::binary::Binary;

/// What to look for.
#[derive(Debug, Clone)]
pub struct BlockOptions {
    /// Instructions in a run.
    pub window: usize,
    /// The share of functions, smallest first, taken as done, for each row
    /// of the coverage trial.
    pub done_shares: Vec<f64>,
    /// Repeated runs to list.
    pub top: usize,
    /// At most this many instructions of a function are read.
    pub max_instructions: usize,
    /// Leave the registers out altogether: two runs are alike when they do
    /// the same operations, whichever registers they use.
    pub loose: bool,
}

impl Default for BlockOptions {
    fn default() -> Self {
        BlockOptions {
            window: 12,
            done_shares: vec![0.1, 0.25, 0.5],
            top: 10,
            max_instructions: 4000,
            loose: false,
        }
    }
}

/// A run of instructions found in more than one function.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    /// Functions it occurs in.
    pub functions: usize,
    /// Times it occurs, in all of them.
    pub occurrences: usize,
    /// One place it occurs: the address of its first instruction.
    pub example: u64,
    /// The instructions there, as written (not renamed).
    pub text: Vec<String>,
}

/// The coverage trial for one share of functions taken as done.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trial {
    pub done_share: f64,
    pub done_functions: usize,
    /// Instructions of the functions not done, and the runs' reach in them.
    pub pending_instructions: usize,
    /// In a run that occurs in at least one done function.
    pub covered_once: usize,
    /// In a run that occurs in at least three done functions.
    pub covered_thrice: usize,
    /// Functions not done that have at least half their instructions covered once.
    pub half_covered_functions: usize,
    pub pending_functions: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockReport {
    pub window: usize,
    pub functions: usize,
    pub instructions: usize,
    /// Runs left out because they are a few instructions repeated (register
    /// saves, zeroing a block), which every function has.
    pub windows_skipped: usize,
    pub windows: usize,
    /// Instructions of frame setup and teardown, left out of everything below.
    pub frame_instructions: usize,
    /// Distinct runs, and how many of them occur in more than one function.
    pub distinct: usize,
    pub repeating: usize,
    pub top: Vec<Block>,
    pub trials: Vec<Trial>,
}

/// One instruction with its registers taken out: text with `\u{1}` where a
/// register goes, and the registers in order.
struct Shape {
    template: String,
    regs: Vec<String>,
}

/// Frame setup and teardown the compiler writes by itself: saving and
/// restoring registers on the stack, the return.
fn is_frame(template: &str) -> bool {
    const STARTS: &[&str] = &[
        "addiu $sp, $sp,",
        "jr $ra",
        "nop",
        "push ",
        "pop ",
        "endbr64",
        "ret",
        "leave",
        "mov rbp, rsp",
        "sub rsp,",
        "add rsp,",
        "stp ",
        "ldp ",
        "sub sp,",
        "add sp,",
    ];
    STARTS.iter().any(|p| template.starts_with(p))
        || ((template.starts_with("sw ") || template.starts_with("lw ")) && template.ends_with("($sp)"))
}

const KEPT: &[&str] = &[
    "$zero", "$sp", "$gp", "$ra", "$fp", "rsp", "rbp", "rip", "esp", "ebp", "sp", "x29", "x30", "xzr", "wzr",
];

const X86_REGS: &[&str] = &[
    "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "eax", "ebx", "ecx", "edx", "esi", "edi", "ax", "bx", "cx", "dx", "si",
    "di", "al", "bl", "cl", "dl", "ah", "bh", "ch", "dh", "sil", "dil", "bpl", "spl",
];

fn is_register(word: &str) -> bool {
    if word.starts_with('$') || X86_REGS.contains(&word) {
        return true;
    }
    // A prefix and a number: r8–r15 (with d/w/b), xmm, ymm, st, AArch64's x, w, v, s, d, q.
    let digits = word
        .trim_end_matches(['d', 'w', 'b'])
        .trim_start_matches(|c: char| c.is_ascii_alphabetic());
    let prefix = &word[..word.len() - word.trim_start_matches(|c: char| c.is_ascii_alphabetic()).len()];
    let ok = |n: &str| !n.is_empty() && n.len() <= 2 && n.bytes().all(|b| b.is_ascii_digit());
    match prefix {
        "r" => ok(digits),
        "xmm" | "ymm" | "st" | "x" | "w" | "v" | "s" | "d" | "q" => {
            ok(word[prefix.len()..].trim_end_matches(char::is_alphabetic))
        }
        _ => false,
    }
}

fn shape(mnemonic: &str, operands: &str) -> Shape {
    let mut template = String::with_capacity(mnemonic.len() + operands.len());
    template.push_str(mnemonic);
    template.push(' ');
    let mut regs = Vec::new();
    let bytes = operands.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_alphanumeric() || c == '$' || c == '_' || c == '.' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'$' || bytes[i] == b'_' || bytes[i] == b'.')
            {
                i += 1;
            }
            let word = &operands[start..i];
            let lower = word.to_ascii_lowercase();
            if word.as_bytes()[0].is_ascii_digit() {
                template.push('N');
            } else if KEPT.contains(&lower.as_str()) {
                template.push_str(&lower);
            } else if is_register(&lower) {
                template.push('\u{1}');
                regs.push(lower);
            } else {
                template.push('S');
            }
        } else {
            template.push(c);
            i += 1;
        }
    }
    Shape { template, regs }
}

fn fnv(h: u64, x: u64) -> u64 {
    (h ^ x).wrapping_mul(0x0000_0100_0000_01b3)
}

fn hash_str(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| fnv(h, b as u64))
}

impl Binary {
    /// See the module's description.
    pub fn repeated_blocks(&self, options: &BlockOptions) -> BlockReport {
        let w = options.window.max(2);
        let mut seen = std::collections::HashSet::new();
        // Per function: start, instruction shapes, and the addresses.
        let mut funcs: Vec<(u64, Vec<Shape>, Vec<(u64, String)>)> = Vec::new();
        for f in self.symbols().functions() {
            if f.size == 0 || !f.defined || !seen.insert(f.address) {
                continue;
            }
            let d = self.disassemble_function(f.address, options.max_instructions);
            if !d.supported || d.instructions.is_empty() {
                continue;
            }
            let shapes = d.instructions.iter().map(|i| shape(&i.mnemonic, &i.operands)).collect();
            let text = d
                .instructions
                .iter()
                .map(|i| (i.address, format!("{} {}", i.mnemonic, i.operands)))
                .collect();
            funcs.push((f.address, shapes, text));
        }
        funcs.sort_by_key(|f| f.0);
        // The instructions between the frame setup at the top and the teardown at the bottom.
        let bodies: Vec<(usize, usize)> = funcs
            .iter()
            .map(|(_, shapes, _)| {
                let head = shapes.iter().take_while(|s| is_frame(&s.template)).count();
                let tail = shapes[head..]
                    .iter()
                    .rev()
                    .take_while(|s| is_frame(&s.template))
                    .count();
                (head, shapes.len() - tail)
            })
            .collect();

        // Each function's runs, hashed with registers renamed inside the run.
        let mut windows_skipped = 0;
        let mut windows = 0;
        let mut hashes: Vec<Vec<Option<u64>>> = Vec::with_capacity(funcs.len());
        for ((_, shapes, _), &(lo, hi)) in funcs.iter().zip(&bodies) {
            let n = shapes.len();
            let mut row = vec![None; n.saturating_sub(w - 1)];
            for (start, slot) in row.iter_mut().enumerate() {
                if start < lo || start + w > hi {
                    continue;
                }
                windows += 1;
                let mut names: Vec<&str> = Vec::new();
                let mut h = 0xcbf2_9ce4_8422_2325u64;
                let mut distinct: Vec<u64> = Vec::with_capacity(w);
                for s in &shapes[start..start + w] {
                    let mut ih = hash_str(&s.template);
                    for r in s.regs.iter().filter(|_| !options.loose) {
                        let idx = match names.iter().position(|n| n == r) {
                            Some(p) => p,
                            None => {
                                names.push(r);
                                names.len() - 1
                            }
                        };
                        ih = fnv(ih, idx as u64 + 1);
                    }
                    if !distinct.contains(&ih) {
                        distinct.push(ih);
                    }
                    h = fnv(h, ih);
                }
                // A few instructions said over and over say nothing.
                if distinct.len() * 3 < w {
                    windows_skipped += 1;
                } else {
                    *slot = Some(h);
                }
            }
            hashes.push(row);
        }

        // Where each run occurs.
        let mut by_run: HashMap<u64, (usize, usize, usize, u32)> = HashMap::new(); // (functions, occurrences, first function, its position)
        for (fi, row) in hashes.iter().enumerate() {
            let mut in_this = std::collections::HashSet::new();
            for (pos, h) in row.iter().enumerate() {
                let Some(h) = h else { continue };
                let e = by_run.entry(*h).or_insert((0, 0, fi, pos as u32));
                e.1 += 1;
                if in_this.insert(*h) {
                    e.0 += 1;
                }
            }
        }
        let repeating = by_run.values().filter(|v| v.0 > 1).count();

        // The runs in most functions, one per stretch of code.
        let mut ranked: Vec<(&u64, &(usize, usize, usize, u32))> = by_run.iter().filter(|(_, v)| v.0 > 1).collect();
        ranked.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(b.1.1.cmp(&a.1.1)).then(a.0.cmp(b.0)));
        let mut top: Vec<Block> = Vec::new();
        let mut listed: Vec<(usize, u32)> = Vec::new();
        for (_, &(functions, occurrences, fi, pos)) in ranked {
            if top.len() >= options.top {
                break;
            }
            // The same stretch shifted by a few instructions is the same block.
            if listed
                .iter()
                .any(|&(f, p)| f == fi && (p as i64 - pos as i64).unsigned_abs() < w as u64)
            {
                continue;
            }
            listed.push((fi, pos));
            let text = &funcs[fi].2[pos as usize..pos as usize + w];
            top.push(Block {
                functions,
                occurrences,
                example: text[0].0,
                text: text.iter().map(|t| t.1.clone()).collect(),
            });
        }

        // The trial: the smallest functions done, how much of the rest their runs reach.
        let mut order: Vec<usize> = (0..funcs.len()).collect();
        order.sort_by_key(|&i| (funcs[i].1.len(), funcs[i].0));
        let trials = options
            .done_shares
            .iter()
            .map(|&share| {
                let done_n = ((funcs.len() as f64) * share).round() as usize;
                let mut done_count: HashMap<u64, u32> = HashMap::new();
                for &fi in &order[..done_n.min(order.len())] {
                    let mut once = std::collections::HashSet::new();
                    for h in hashes[fi].iter().flatten() {
                        if once.insert(*h) {
                            *done_count.entry(*h).or_insert(0) += 1;
                        }
                    }
                }
                let (mut pending, mut once, mut thrice, mut half, mut pending_fns) = (0, 0, 0, 0, 0);
                for &fi in &order[done_n.min(order.len())..] {
                    let n = funcs[fi].1.len();
                    let body = bodies[fi].1 - bodies[fi].0;
                    pending += body;
                    if body > 0 {
                        pending_fns += 1;
                    }
                    let cover = |min: u32| -> usize {
                        let mut mark = vec![false; n];
                        for (pos, h) in hashes[fi].iter().enumerate() {
                            if h.is_some_and(|h| done_count.get(&h).copied().unwrap_or(0) >= min) {
                                mark[pos..pos + w].iter_mut().for_each(|m| *m = true);
                            }
                        }
                        mark.iter().filter(|&&m| m).count()
                    };
                    let c1 = cover(1);
                    once += c1;
                    thrice += cover(3);
                    if body > 0 && c1 * 2 >= body {
                        half += 1;
                    }
                }
                Trial {
                    done_share: share,
                    done_functions: done_n,
                    pending_instructions: pending,
                    covered_once: once,
                    covered_thrice: thrice,
                    half_covered_functions: half,
                    pending_functions: pending_fns,
                }
            })
            .collect();

        BlockReport {
            window: w,
            functions: funcs.len(),
            instructions: funcs.iter().map(|f| f.1.len()).sum(),
            windows_skipped,
            windows,
            frame_instructions: funcs.iter().zip(&bodies).map(|(f, b)| f.1.len() - (b.1 - b.0)).sum(),
            distinct: by_run.len(),
            repeating,
            top,
            trials,
        }
    }
}
