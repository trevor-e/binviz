//! splat (<https://github.com/ethteck/splat>), the tool decompilation
//! projects split their binary with: a YAML config naming the segments
//! (header, code, data) and the units they split into, and
//! `symbol_addrs.txt` naming what is at each address. binviz writes both
//! from what it found (the code followed, the functions named) so that a
//! project starts from a map rather than from nothing, and reads the symbol
//! file back into its notes so that the project's names show here.
//!
//! PlayStation executables so far.

use std::fmt::Write;

use serde::Serialize;

use crate::binary::Binary;
use crate::error::{Error, Result};
use crate::model::{Annotation, SymbolKind};
use crate::rom::Platform;

/// A splat project's starting files.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplatExport {
    /// The YAML config.
    pub config: String,
    /// `symbol_addrs.txt`.
    pub symbol_addrs: String,
    pub functions: u32,
    pub data_symbols: u32,
}

/// splat's own names for what it hasn't been told about.
fn auto_name(name: &str) -> bool {
    ["func_", "D_", "jtbl_", "L", "B_", "RODATA_"]
        .iter()
        .any(|p| name.strip_prefix(p).is_some_and(|rest| rest.len() == 8 && rest.chars().all(|c| c.is_ascii_hexdigit())))
}

impl Binary {
    /// A splat config and `symbol_addrs.txt` for this PlayStation
    /// executable, the project named `name`. The code segment is split into
    /// units at `splits` (addresses; each unit is named after where it
    /// starts), one unit when there are none; the bytes after the last
    /// function are a data unit.
    pub fn splat_export(&self, name: &str, splits: &[u64]) -> Result<SplatExport> {
        let rom = self
            .rom
            .as_ref()
            .filter(|r| r.platform == Platform::PlayStation)
            .ok_or_else(|| Error::new("splat export is for PlayStation executables"))?;
        let _ = rom;
        if self.summary.format_name != "PS-X EXE" {
            return Err(Error::new("splat export is for PS-X EXE files (the disc's boot executable)"));
        }
        let data = self.data();
        let word = |at: usize| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as u64;
        let (entry, gp, load, size) = (word(0x10), word(0x14), word(0x18), word(0x1C));
        let (bss, bss_size) = (word(0x28), word(0x2C));
        let end = 0x800 + size.min(data.len() as u64 - 0x800);
        // Code runs from the first function to the end of the last; the rest is data.
        let mut functions: Vec<(u64, u64)> = self
            .symbols()
            .functions()
            .filter(|f| f.address >= load && f.address < load + size)
            .map(|f| (f.address, f.size))
            .collect();
        functions.sort_unstable();
        let code_end = functions
            .iter()
            .map(|&(a, s)| a + s.max(4))
            .max()
            .unwrap_or(load);
        let code_end = (code_end + 15) & !15;
        let to_offset = |a: u64| 0x800 + (a - load);
        let mut config = String::new();
        let _ = writeln!(config, "# splat config written by binviz for {name}: the code as followed from the entry");
        let _ = writeln!(config, "# point, one unit per split; refine the units and mark rodata/sdata by hand.");
        let _ = writeln!(config, "name: {name}");
        let _ = writeln!(config, "# sha1: (sha1sum of the executable)");
        let _ = writeln!(config, "options:");
        let _ = writeln!(config, "  platform: psx");
        let _ = writeln!(config, "  compiler: GCC");
        let _ = writeln!(config, "  basename: {name}");
        let _ = writeln!(config, "  base_path: .");
        let _ = writeln!(config, "  build_path: build");
        let _ = writeln!(config, "  target_path: {name}.exe");
        let _ = writeln!(config, "  asm_path: asm");
        let _ = writeln!(config, "  src_path: src");
        let _ = writeln!(config, "  ld_script_path: {name}.ld");
        let _ = writeln!(config, "  symbol_addrs_path: symbol_addrs.txt");
        let _ = writeln!(config, "  undefined_funcs_auto_path: undefined_funcs_auto.txt");
        let _ = writeln!(config, "  undefined_syms_auto_path: undefined_syms_auto.txt");
        let _ = writeln!(config, "  find_file_boundaries: no");
        let _ = writeln!(config, "  gp_value: {gp:#x}");
        let _ = writeln!(config, "  asm_function_macro: glabel");
        let _ = writeln!(config, "  asm_jtbl_label_macro: jlabel");
        let _ = writeln!(config, "  asm_data_macro: dlabel");
        let _ = writeln!(config, "  section_order: [.text, .data, .rodata, .sdata, .sbss, .bss]");
        let _ = writeln!(config, "# entry point: {entry:#x}");
        let _ = writeln!(config, "segments:");
        let _ = writeln!(config, "  - name: header");
        let _ = writeln!(config, "    type: header");
        let _ = writeln!(config, "    start: 0x0");
        let _ = writeln!(config, "  - name: main");
        let _ = writeln!(config, "    type: code");
        let _ = writeln!(config, "    start: 0x800");
        let _ = writeln!(config, "    vram: {load:#x}");
        if bss_size > 0 {
            let _ = writeln!(config, "    bss_size: {bss_size:#x}  # at {bss:#x}");
        }
        let _ = writeln!(config, "    subsegments:");
        let mut splits: Vec<u64> = splits.iter().copied().filter(|&s| s > load && s < code_end).collect();
        splits.sort_unstable();
        splits.dedup();
        let mut unit_start = load;
        let unit_name = |a: u64| if a == load { "main".to_string() } else { format!("unit_{a:08x}") };
        for s in splits.iter().copied().chain(std::iter::once(code_end)) {
            let _ = writeln!(config, "      - [{:#x}, asm, {}]", to_offset(unit_start), unit_name(unit_start));
            unit_start = s;
        }
        if code_end < load + size {
            let _ = writeln!(config, "      - [{:#x}, data, main]", to_offset(code_end));
        }
        let _ = writeln!(config, "  - [{end:#x}]");

        // symbol_addrs.txt: every function, and every other named place.
        let mut symbol_addrs = String::new();
        let mut n_functions = 0;
        let mut n_data = 0;
        let mut seen = std::collections::HashSet::new();
        for s in self.symbols().iter() {
            if !s.defined || !seen.insert(s.address) {
                continue;
            }
            let name = s.display_name();
            let name = name.strip_prefix("sub_").map_or_else(|| name.to_string(), |a| format!("func_{a}"));
            if name.is_empty() {
                continue;
            }
            let (kind, ok) = match s.kind {
                SymbolKind::Function => ("func", true),
                SymbolKind::Data | SymbolKind::Label => ("data", true),
                _ => ("", false),
            };
            if !ok {
                continue;
            }
            let mut line = format!("{name} = {:#x}; // type:{kind}", s.address);
            if s.size > 0 && !s.size_inferred {
                let _ = write!(line, " size:{:#x}", s.size);
            }
            if s.address >= load && s.address < load + size {
                let _ = write!(line, " rom:{:#x}", to_offset(s.address));
            }
            symbol_addrs.push_str(&line);
            symbol_addrs.push('\n');
            if kind == "func" {
                n_functions += 1;
            } else {
                n_data += 1;
            }
        }
        Ok(SplatExport {
            config,
            symbol_addrs,
            functions: n_functions,
            data_symbols: n_data,
        })
    }
}

/// The names in a splat symbol file (`symbol_addrs.txt`,
/// `undefined_funcs_auto.txt`, `undefined_syms_auto.txt`): `name = 0xADDR;
/// // type:func size:0x40`, as notes. splat's own made-up names
/// (`func_80010000`, `D_8001ABCD`) are left out.
pub fn parse_symbol_addrs(text: &str) -> Vec<Annotation> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        let (decl, attrs) = line.split_once("//").map_or((line, ""), |(d, a)| (d, a));
        let Some((name, value)) = decl.split_once('=') else { continue };
        let name = name.trim();
        let value = value.trim().trim_end_matches(';').trim();
        let Ok(address) = u64::from_str_radix(value.trim_start_matches("0x").trim_start_matches("0X"), 16) else {
            continue;
        };
        if name.is_empty() || auto_name(name) {
            continue;
        }
        let mut size = 0;
        let mut kind = None;
        for attr in attrs.split_whitespace() {
            if let Some(v) = attr.strip_prefix("size:") {
                size = u64::from_str_radix(v.trim_start_matches("0x"), 16).unwrap_or(0);
            }
            if let Some(v) = attr.strip_prefix("type:") {
                kind = Some(if v == "func" { "function" } else { "data" }.to_string());
            }
        }
        out.push(Annotation {
            address,
            size,
            name: name.to_string(),
            comment: String::new(),
            reviewed: false,
            kind,
            decomp: None,
            ctype: None,
            author: String::new(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_and_import() {
        let mut data = vec![0u8; 0x800];
        data[..8].copy_from_slice(b"PS-X EXE");
        for (at, v) in [
            (0x10, 0x8001_0000u32),
            (0x14, 0x8001_8000),
            (0x18, 0x8001_0000),
            (0x1C, 0x40),
            (0x28, 0x8002_0000),
            (0x2C, 0x100),
        ] {
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        // entry: jal 0x80010010 / nop / jr $ra / nop; the callee: jr $ra / li $v0, 1; then data.
        let words = [0x0C00_4004u32, 0, 0x03E0_0008, 0, 0x03E0_0008, 0x2402_0001, 0, 0];
        data.extend(words.iter().flat_map(|w| w.to_le_bytes()));
        data.extend([0x11u8; 0x20]);
        let bin = Binary::parse(data).unwrap();
        let e = bin.splat_export("tiny", &[0x8001_0010]).unwrap();
        assert!(e.config.contains("platform: psx"), "{}", e.config);
        assert!(e.config.contains("    vram: 0x80010000"));
        assert!(e.config.contains("    bss_size: 0x100"));
        assert!(e.config.contains("      - [0x800, asm, main]"));
        assert!(e.config.contains("      - [0x810, asm, unit_80010010]"));
        assert!(e.config.contains("      - [0x820, data, main]"), "{}", e.config);
        assert!(e.config.contains("  - [0x840]"));
        assert!(e.symbol_addrs.contains("entry = 0x80010000; // type:func size:0x10 rom:0x800"), "{}", e.symbol_addrs);
        assert!(e.symbol_addrs.contains("func_80010010 = 0x80010010; // type:func size:0x8 rom:0x810"));
        assert!(e.symbol_addrs.contains("GP0 = 0x1f801810; // type:data size:0x4"));
        assert_eq!(e.functions, 2);

        let notes = parse_symbol_addrs(
            "// names\nMainLoop = 0x80010000; // type:func size:0x10\nfunc_80010010 = 0x80010010;\nD_80020000 = 0x80020000;\ngFrame = 0x80020004; // type:data size:0x4\n",
        );
        let got: Vec<(u64, u64, &str)> = notes.iter().map(|a| (a.address, a.size, a.name.as_str())).collect();
        assert_eq!(got, [(0x8001_0000, 0x10, "MainLoop"), (0x8002_0004, 4, "gFrame")]);
    }
}
