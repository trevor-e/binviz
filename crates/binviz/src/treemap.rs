//! Shareable, byte-weighted decompilation progress images, using the same
//! noted function extents as the existing progress report.
use std::collections::BTreeMap;
use std::fmt::Write;

use crate::{Binary, DecompState};

const LEGEND: [(&str, &str, &str); 7] = [
    ("matched", "Matched", "#238b57"),
    ("nonmatching", "Nonmatching C", "#248999"),
    ("in-progress", "In progress", "#ad741c"),
    ("attempted", "Attempted", "#795bb1"),
    ("todo", "To do", "#34455e"),
    ("skipped", "Skipped", "#6a6470"),
    ("library", "Library", "#555f6b"),
];

#[derive(Clone)]
struct Function {
    address: u64,
    size: u64,
    name: String,
    state: &'static str,
    percent: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Deterministic weighted binary partition. No per-tile padding: the allocated
/// area remains proportional to bytes, including very small functions.
fn layout(weights: &[f64], rect: Rect) -> Vec<Rect> {
    fn split(weights: &[f64], rect: Rect, out: &mut Vec<Rect>) {
        if weights.is_empty() {
            return;
        }
        if weights.len() == 1 {
            out.push(rect);
            return;
        }
        let total: f64 = weights.iter().sum();
        let mut left = weights[0];
        let mut at = 1;
        while at < weights.len() - 1 && (left + weights[at] - total / 2.0).abs() < (left - total / 2.0).abs() {
            left += weights[at];
            at += 1;
        }
        let fraction = left / total;
        let (a, b) = if rect.w >= rect.h {
            let w = rect.w * fraction;
            (
                Rect { w, ..rect },
                Rect {
                    x: rect.x + w,
                    w: rect.w - w,
                    ..rect
                },
            )
        } else {
            let h = rect.h * fraction;
            (
                Rect { h, ..rect },
                Rect {
                    y: rect.y + h,
                    h: rect.h - h,
                    ..rect
                },
            )
        };
        split(&weights[..at], a, out);
        split(&weights[at..], b, out);
    }
    let mut out = Vec::with_capacity(weights.len());
    split(weights, rect, &mut out);
    out
}

fn xml(s: &str) -> String {
    s.chars()
        .filter(|c| matches!(c, '\n' | '\r' | '\t') || *c >= ' ')
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&apos;".into(),
            _ => c.to_string(),
        })
        .collect()
}

fn label(s: &str, width: f64) -> String {
    let max = (width / 7.2).floor().max(0.0) as usize;
    if s.chars().count() <= max {
        s.into()
    } else if max > 1 {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    } else {
        String::new()
    }
}

impl Binary {
    /// Self-contained SVG, grouped by recorded source file (otherwise section).
    /// Library code is omitted by default and never credited as game progress.
    /// Zero-sized functions count in the summary but have no drawable area.
    pub fn progress_svg(&self, title: &str, width: u32, height: u32, include_library: bool) -> Result<String, String> {
        if !(640..=8192).contains(&width) || !(360..=8192).contains(&height) {
            return Err("treemap dimensions must be width 640..8192 and height 360..8192".into());
        }
        let mut groups: BTreeMap<String, Vec<Function>> = BTreeMap::new();
        let (mut total, mut matched, mut count, mut matched_count, mut library, mut zero) =
            (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
        for (address, size) in self.functions_as_noted() {
            let note = self.decomp_at(address);
            let state = note.map_or(DecompState::Todo, |d| d.state);
            if state == DecompState::Library {
                library += size;
                if !include_library {
                    continue;
                }
            } else {
                total += size;
                count += 1;
                if state == DecompState::Matched {
                    matched += size;
                    matched_count += 1;
                }
            }
            if size == 0 {
                zero += 1;
                continue;
            }
            let group = note
                .filter(|d| !d.source.is_empty())
                .map(|d| d.source.clone())
                .unwrap_or_else(|| {
                    self.section_at(address)
                        .map_or_else(|| "(unknown unit)".into(), |s| s.name.clone())
                });
            let group = if state == DecompState::Library {
                format!("Library / {group}")
            } else {
                group
            };
            groups.entry(group).or_default().push(Function {
                address,
                size,
                name: self
                    .symbols()
                    .at(address)
                    .map_or_else(|| format!("sub_{address:x}"), |s| s.name().to_owned()),
                state: if state == DecompState::Todo && note.is_some_and(|d| d.attempts > 0) {
                    "attempted"
                } else {
                    state.as_str()
                },
                percent: note.and_then(|d| d.percent),
            });
        }
        let pct = if total == 0 {
            0.0
        } else {
            matched as f64 * 100.0 / total as f64
        };
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-labelledby=\"progress-title progress-description\"><title id=\"progress-title\">{}</title><desc id=\"progress-description\">Function areas are proportional to code bytes. {matched_count} of {count} game functions matched; {matched} of {total} game code bytes ({pct:.1}%). Library code is excluded from progress totals.</desc><rect width=\"100%\" height=\"100%\" fill=\"#111b29\"/><g font-family=\"system-ui, sans-serif\" fill=\"#f3f6fb\"><text x=\"24\" y=\"36\" font-size=\"22\" font-weight=\"600\">{}</text><text x=\"24\" y=\"62\" font-size=\"14\">{pct:.1}% matched by bytes · {matched} / {total} bytes · {matched_count} / {count} game functions</text>",
            xml(title),
            xml(&label(title, f64::from(width) - 48.0))
        );
        for (i, (state, name, color)) in LEGEND.iter().enumerate() {
            let x = 24 + (i % 4) * 150;
            let y = 82 + (i / 4) * 22;
            let _ = write!(
                svg,
                "<rect x=\"{x}\" y=\"{y}\" width=\"12\" height=\"12\" fill=\"{color}\"/><text x=\"{}\" y=\"{}\" font-size=\"12\" data-state=\"{state}\">{name}</text>",
                x + 18,
                y + 11
            );
        }
        let bounds = Rect {
            x: 24.0,
            y: 136.0,
            w: f64::from(width) - 48.0,
            h: f64::from(height) - 181.0,
        };
        let group_weights: Vec<f64> = groups
            .values()
            .map(|fs| fs.iter().map(|f| f.size as f64).sum())
            .collect();
        let group_rects = layout(&group_weights, bounds);
        if groups.is_empty() {
            svg.push_str("<text x=\"24\" y=\"170\" font-size=\"16\">No sized functions to display.</text>");
        }
        for ((group, mut functions), rect) in groups.into_iter().zip(group_rects) {
            functions.sort_by_key(|f| (std::cmp::Reverse(f.size), f.address));
            let weights: Vec<f64> = functions.iter().map(|f| f.size as f64).collect();
            let tiles = layout(&weights, rect);
            for (f, r) in functions.iter().zip(tiles) {
                let (_, status, color) = LEGEND.iter().find(|(s, _, _)| *s == f.state).unwrap();
                let score = f
                    .percent
                    .filter(|p| p.is_finite())
                    .map(|p| format!(" · best match {p:.1}%"))
                    .unwrap_or_default();
                let _ = write!(
                    svg,
                    "<g data-address=\"0x{:x}\" data-state=\"{}\" data-bytes=\"{}\"><title>{} · 0x{:x} · {} bytes · {}{} · {}</title><rect x=\"{:.4}\" y=\"{:.4}\" width=\"{:.4}\" height=\"{:.4}\" fill=\"{color}\" stroke=\"#111b29\" stroke-width=\"0.6\"/>",
                    f.address,
                    f.state,
                    f.size,
                    xml(&f.name),
                    f.address,
                    f.size,
                    status,
                    xml(&score),
                    xml(&group),
                    r.x,
                    r.y,
                    r.w,
                    r.h
                );
                if r.w >= 50.0 && r.h >= 34.0 {
                    let _ = write!(
                        svg,
                        "<text x=\"{:.4}\" y=\"{:.4}\" font-size=\"12\">{}</text>",
                        r.x + 5.0,
                        r.y + 17.0,
                        xml(&label(&f.name, r.w - 10.0))
                    );
                }
                if r.w >= 70.0 && r.h >= 52.0 {
                    let _ = write!(
                        svg,
                        "<text x=\"{:.4}\" y=\"{:.4}\" font-size=\"11\" fill=\"#e1e8f1\">{} bytes</text>",
                        r.x + 5.0,
                        r.y + 32.0,
                        f.size
                    );
                }
                svg.push_str("</g>");
            }
            let _ = write!(
                svg,
                "<rect x=\"{:.4}\" y=\"{:.4}\" width=\"{:.4}\" height=\"{:.4}\" fill=\"none\" stroke=\"#c3cedd\" stroke-width=\"1.2\" pointer-events=\"none\"><title>{}</title></rect>",
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                xml(&group)
            );
        }
        let footer = format!(
            "Area = function bytes · Source/section groups · Library {library} bytes {} · {zero} zero-sized functions omitted",
            if include_library { "shown" } else { "hidden" }
        );
        let _ = write!(
            svg,
            "<text x=\"24\" y=\"{}\" font-size=\"12\" fill=\"#b8c5d6\">{}</text></g></svg>",
            height - 20,
            xml(&label(&footer, f64::from(width) - 48.0))
        );
        Ok(svg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Annotation, Decomp};
    #[test]
    fn layout_preserves_area_and_stays_in_bounds() {
        let weights = [1.0, 20.0, 300.0, 2.0, 1000.0];
        let area = Rect {
            x: 10.0,
            y: 15.0,
            w: 640.0,
            h: 480.0,
        };
        let tiles = layout(&weights, area);
        for (r, w) in tiles.iter().zip(weights) {
            assert!((r.w * r.h / (area.w * area.h) - w / weights.iter().sum::<f64>()).abs() < 1e-10);
            assert!(
                r.x >= area.x
                    && r.y >= area.y
                    && r.x + r.w <= area.x + area.w + 1e-8
                    && r.y + r.h <= area.y + area.h + 1e-8
            );
        }
        for (i, a) in tiles.iter().enumerate() {
            for b in &tiles[i + 1..] {
                assert!(
                    (a.x + a.w).min(b.x + b.w) - a.x.max(b.x) <= 1e-8
                        || (a.y + a.h).min(b.y + b.h) - a.y.max(b.y) <= 1e-8
                );
            }
        }
    }
    #[test]
    fn svg_uses_merged_extents_states_and_excludes_library_credit() {
        let mut b = Binary::parse(include_bytes!("../../../tests/fixtures/bin/tiny-psx.exe").as_slice()).unwrap();
        b.set_annotations(vec![
            Annotation {
                address: 0x80010000,
                size: 32,
                name: "<Matched & function>".into(),
                decomp: Some(Decomp {
                    state: DecompState::Matched,
                    source: "game.c".into(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            Annotation {
                address: 0x80010020,
                size: 32,
                decomp: Some(Decomp {
                    state: DecompState::Library,
                    ..Default::default()
                }),
                ..Default::default()
            },
        ]);
        let svg = b.progress_svg("<Game>", 1000, 600, false).unwrap();
        assert!(svg.contains("&lt;Game&gt;"));
        assert!(svg.contains("&lt;Matched &amp; function&gt;"));
        assert!(svg.contains("data-address=\"0x80010000\" data-state=\"matched\" data-bytes=\"32\""));
        assert!(!svg.contains("data-address=\"0x80010020\""));
        assert!(
            b.progress_svg("Game", 1000, 600, true)
                .unwrap()
                .contains("data-address=\"0x80010020\" data-state=\"library\"")
        );
        assert!(b.progress_svg("Game", 0, 600, true).is_err());
    }

    #[test]
    fn partial_or_nonmatching_c_does_not_count_as_an_exact_match() {
        let mut b = Binary::parse(include_bytes!("../../../tests/fixtures/bin/tiny-psx.exe").as_slice()).unwrap();
        b.set_annotations(vec![
            Annotation {
                address: 0x80010000,
                decomp: Some(Decomp {
                    state: DecompState::Matched,
                    ..Default::default()
                }),
                ..Default::default()
            },
            Annotation {
                address: 0x80010020,
                decomp: Some(Decomp {
                    state: DecompState::Nonmatching,
                    percent: Some(100.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ]);
        let svg = b.progress_svg("Game", 1000, 600, false).unwrap();
        assert!(svg.contains("1 of 2 game functions matched; 32 of 44 game code bytes (72.7%)"));
        assert!(svg.contains("data-state=\"nonmatching\" data-bytes=\"12\""));
        b.set_annotations(vec![Annotation {
            address: 0x80010000,
            size: 44,
            decomp: Some(Decomp {
                state: DecompState::Matched,
                ..Default::default()
            }),
            ..Default::default()
        }]);
        let svg = b.progress_svg("Game", 1000, 600, false).unwrap();
        assert!(svg.contains("1 of 1 game functions matched; 44 of 44 game code bytes (100.0%)"));
        assert_eq!(svg.matches("data-address=").count(), 1);
    }
}
