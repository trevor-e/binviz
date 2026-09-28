//! Crash reports symbolicated with every open binary, each of the report's
//! images found by UUID or build ID.

use binviz::crash::{Candidate, Found, Matches, match_images, parse, symbolicate};
use serde_json::Value;

use crate::tools::{Server, string};

impl Server {
    pub(crate) fn symbolicate(&mut self, args: &Value) -> Result<String, String> {
        let text = match (string(args, "report"), string(args, "report_file")) {
            (Some(t), _) => t.to_string(),
            (None, Some(p)) => std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?,
            _ => return Err("pass the report's text as report, or its path as report_file".into()),
        };
        let report = parse(&text).ok_or(
            "not a crash report: an Apple .crash or .ips, an Android tombstone, or a stack trace with images and offsets",
        )?;
        if self.open.is_empty() {
            return Err(
                "no binary is open: open_binary the app first (a folder or zip holding it and its dSYMs, or the binary)"
                    .into(),
            );
        }
        let names: Vec<String> = self
            .open
            .iter()
            .map(|o| {
                o.path
                    .file_name()
                    .map_or_else(|| o.id.clone(), |n| n.to_string_lossy().into_owned())
            })
            .collect();
        let ids: Vec<Option<String>> = self.open.iter().map(|o| o.bin.summary().build_id.clone()).collect();
        let Matches { pairs, notes } = {
            let candidates: Vec<Candidate> = (0..self.open.len())
                .map(|i| Candidate {
                    binary: i as u32,
                    name: &names[i],
                    ids: ids[i].iter().map(String::as_str).collect(),
                })
                .collect();
            match_images(&report, &candidates)
        };
        // Stripped binaries need their debug files for names and lines.
        for &(_, b) in &pairs {
            self.attach_pending(b as usize);
        }
        let found: Vec<Found> = pairs
            .iter()
            .map(|&(image, b)| Found {
                image,
                binary: b,
                bin: &self.open[b as usize].bin,
            })
            .collect();
        let out = symbolicate(&report, &found, &notes);
        let mut text = out.to_text(|b| format!("`{}`", self.open[b as usize].id));
        if pairs.is_empty() {
            text.push_str(
                "\nNo open binary is one of the report's images (by UUID or build ID). Open the app's binaries with their dSYMs — a folder or zip of them works — and try again.\n",
            );
        }
        Ok(text)
    }
}
