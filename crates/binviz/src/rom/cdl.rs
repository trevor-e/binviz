//! Code/data logs: which bytes of a ROM an emulator saw run as code and
//! read as data while the game was played (FCEUX's and Mesen's code/data
//! loggers). They show what following the code statically can't: code only
//! reached through jump tables, data it would take for code, the 65816's
//! register widths as each instruction ran, ARM or Thumb; and, from FCEUX,
//! where each switched bank was mapped when its code ran.
//!
//! Formats:
//! - FCEUX (NES): no header; a flag byte per PRG ROM byte, then per CHR ROM
//!   byte. Bit 0 code, 1 data, 2–3 the 8 KiB CPU window (`$8000`, `$A000`,
//!   `$C000`, `$E000`), 4 reached by `JMP ($nnnn)`, 5 read indirectly, 6 a
//!   DMC sample, 7 seen below `$8000`.
//! - Mesen (NES): the same layout; bit 0 code, 1 data, 4 a jump's target,
//!   6 a DMC sample, 7 a subroutine's start.
//! - Mesen 2 (NES, SNES, Game Boy, Game Boy Advance): `CDLv2`, the ROM's
//!   CRC-32, then a flag byte per ROM byte. Bit 0 code, 1 data, 2 a jump's
//!   target, 3 a subroutine's start; SNES: 4 the index registers 8-bit, 5
//!   the accumulator 8-bit; Game Boy Advance: 5 Thumb; NES: 7 a DMC sample.

use serde::Serialize;

use super::Platform;
use crate::error::{Result, bail};

/// Normalized flags, one set per ROM byte.
pub mod flag {
    /// Ran as (part of) an instruction.
    pub const CODE: u16 = 1;
    /// Read as data.
    pub const DATA: u16 = 2;
    /// A subroutine starts here (a call's or an interrupt's target).
    pub const ENTRY: u16 = 4;
    /// A jump or a branch lands here.
    pub const JUMP: u16 = 8;
    /// Reached through a pointer: an indirect jump's target, or data read through one.
    pub const INDIRECT: u16 = 16;
    /// 65816: the accumulator was 8-bit when this ran.
    pub const M8: u16 = 32;
    /// 65816: the index registers were 8-bit when this ran.
    pub const X8: u16 = 64;
    /// ARM7: this instruction ran in Thumb state.
    pub const THUMB: u16 = 128;
    /// NES: a DMC sample the sound hardware played.
    pub const PCM: u16 = 256;
}

/// Which logger wrote a log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LogFormat {
    Fceux,
    Mesen,
    Mesen2,
}

impl LogFormat {
    pub fn name(self) -> &'static str {
        match self {
            LogFormat::Fceux => "FCEUX",
            LogFormat::Mesen => "Mesen",
            LogFormat::Mesen2 => "Mesen 2",
        }
    }
}

/// A code/data log, its flags normalized.
#[derive(Debug, Clone)]
pub struct CodeDataLog {
    pub format: LogFormat,
    /// The file offset the first flag is for (past an iNES header, or an SNES copier header).
    pub rom_offset: u64,
    /// Flags per ROM byte (for the NES, per PRG ROM byte): [`flag`] bits.
    pub flags: Vec<u16>,
    /// FCEUX: the 8 KiB window (0–3: `$8000`, `$A000`, `$C000`, `$E000`)
    /// each byte was seen in, 0xFF where not seen (or seen below `$8000`).
    pub windows: Option<Vec<u8>>,
    /// NES: CHR ROM bytes the PPU drew (or the CPU read).
    pub chr_seen: u64,
    /// Mesen 2: the CRC-32 of the ROM it was made for.
    pub crc32: Option<u32>,
}

/// What a code/data log covers, to show.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSummary {
    pub format: LogFormat,
    /// Bytes the log has flags for (the PRG ROM, for the NES).
    pub bytes: u64,
    pub code: u64,
    pub data: u64,
    /// Seen as both code and data.
    pub both: u64,
    /// Subroutine starts and jump targets the logger marked.
    pub entries: u64,
    pub jumps: u64,
    /// CHR ROM bytes drawn (NES).
    pub chr_seen: u64,
    /// Mesen 2: whether the log was made for this ROM (its CRC-32 matches).
    pub crc_matches: Option<bool>,
    /// NES: switched banks placed where the log saw them run (8 KiB pages).
    pub pages_placed: u32,
}

/// What reading a log needs to know about the ROM.
pub(crate) struct RomShape {
    pub platform: Platform,
    /// Where the flags start in the file, and how many there are for code.
    pub rom_offset: u64,
    pub rom_size: u64,
    /// NES: CHR ROM bytes after the PRG ROM's.
    pub chr_size: u64,
}

impl CodeDataLog {
    /// Reads a log for a ROM of this shape.
    pub(crate) fn parse(data: &[u8], shape: &RomShape) -> Result<CodeDataLog> {
        let nes = shape.platform == Platform::Nes;
        if let Some(rest) = data.strip_prefix(b"CDLv2") {
            if rest.len() < 4 {
                bail!("the Mesen 2 code/data log is cut short");
            }
            let crc = u32::from_le_bytes(rest[..4].try_into().unwrap());
            let raw = &rest[4..];
            if (raw.len() as u64) < shape.rom_size {
                bail!(
                    "the code/data log covers {} bytes, but the ROM has {}: it is for another game",
                    raw.len(),
                    shape.rom_size
                );
            }
            let (prg, chr) = raw.split_at(shape.rom_size as usize);
            let flags = prg.iter().map(|&b| mesen2(b, shape.platform)).collect();
            return Ok(CodeDataLog {
                format: LogFormat::Mesen2,
                rom_offset: shape.rom_offset,
                flags,
                windows: None,
                chr_seen: if nes {
                    chr.iter().filter(|&&b| b & 1 != 0).count() as u64
                } else {
                    0
                },
                crc32: Some(crc),
            });
        }
        if (data.len() as u64) < shape.rom_size {
            bail!(
                "the code/data log covers {} bytes, but the ROM has {}: it is for another game, or not a log",
                data.len(),
                shape.rom_size
            );
        }
        let (prg, chr) = data.split_at(shape.rom_size as usize);
        if !nes {
            // A Mesen 2 log saved without its header.
            if chr.is_empty() {
                return Ok(CodeDataLog {
                    format: LogFormat::Mesen2,
                    rom_offset: shape.rom_offset,
                    flags: prg.iter().map(|&b| mesen2(b, shape.platform)).collect(),
                    windows: None,
                    chr_seen: 0,
                    crc32: None,
                });
            }
            bail!("not a code/data log binviz reads: for this console, Mesen 2's (they start with CDLv2)");
        }
        if chr.len() as u64 != shape.chr_size && !chr.is_empty() {
            bail!(
                "the code/data log is {} bytes; this ROM's would be {} (PRG ROM) and {} (CHR ROM)",
                data.len(),
                shape.rom_size,
                shape.chr_size
            );
        }
        // FCEUX writes the window each byte was seen in (bits 2 and 3, zero for $8000);
        // Mesen never does, and marks every subroutine's start (bit 7).
        let fceux = prg.iter().any(|&b| b & 0x0C != 0) || !prg.iter().any(|&b| b & 0x80 != 0);
        let chr_seen = chr.iter().filter(|&&b| b & 3 != 0).count() as u64;
        if fceux {
            let mut windows = vec![0xFFu8; prg.len()];
            let flags = prg
                .iter()
                .zip(windows.iter_mut())
                .map(|(&b, w)| {
                    if b & 3 != 0 && b & 0x80 == 0 {
                        *w = (b >> 2) & 3;
                    }
                    fceux_flags(b)
                })
                .collect();
            Ok(CodeDataLog {
                format: LogFormat::Fceux,
                rom_offset: shape.rom_offset,
                flags,
                windows: Some(windows),
                chr_seen,
                crc32: None,
            })
        } else {
            Ok(CodeDataLog {
                format: LogFormat::Mesen,
                rom_offset: shape.rom_offset,
                flags: prg.iter().map(|&b| mesen_flags(b)).collect(),
                windows: None,
                chr_seen,
                crc32: None,
            })
        }
    }

    /// The flags for a file offset (none outside what the log covers).
    pub fn at(&self, offset: u64) -> u16 {
        offset
            .checked_sub(self.rom_offset)
            .and_then(|i| self.flags.get(i as usize))
            .copied()
            .unwrap_or(0)
    }

    pub fn summary(&self, rom_crc: u32, pages_placed: u32) -> LogSummary {
        let count = |f: fn(u16) -> bool| self.flags.iter().filter(|&&b| f(b)).count() as u64;
        LogSummary {
            format: self.format,
            bytes: self.flags.len() as u64,
            code: count(|b| b & flag::CODE != 0),
            data: count(|b| b & flag::DATA != 0),
            both: count(|b| b & (flag::CODE | flag::DATA) == flag::CODE | flag::DATA),
            entries: count(|b| b & flag::ENTRY != 0),
            jumps: count(|b| b & flag::JUMP != 0),
            chr_seen: self.chr_seen,
            crc_matches: self.crc32.map(|c| c == rom_crc),
            pages_placed,
        }
    }

    /// NES (FCEUX): the window each 8 KiB page of PRG ROM ran in most, if the log saw it.
    pub(crate) fn page_windows(&self) -> Option<Vec<Option<u64>>> {
        let windows = self.windows.as_ref()?;
        let pages = self.flags.len().div_ceil(0x2000);
        let mut out = Vec::with_capacity(pages);
        for p in 0..pages {
            let range = p * 0x2000..((p + 1) * 0x2000).min(self.flags.len());
            let mut votes = [0u64; 4];
            for i in range {
                let w = windows[i];
                if w < 4 {
                    // Code says more about where a bank is than data (tables are read from anywhere).
                    votes[w as usize] += if self.flags[i] & flag::CODE != 0 { 4 } else { 1 };
                }
            }
            let (w, &n) = votes.iter().enumerate().max_by_key(|&(_, n)| *n).unwrap();
            out.push((n > 0).then_some(0x8000 + w as u64 * 0x2000));
        }
        Some(out)
    }
}

fn fceux_flags(b: u8) -> u16 {
    let mut f = u16::from(b & 3);
    if b & 0x10 != 0 {
        // Reached by JMP ($nnnn): a jump table's target.
        f |= flag::JUMP | flag::INDIRECT;
    }
    if b & 0x20 != 0 {
        f |= flag::DATA | flag::INDIRECT;
    }
    if b & 0x40 != 0 {
        f |= flag::DATA | flag::PCM;
    }
    f
}

fn mesen_flags(b: u8) -> u16 {
    let mut f = u16::from(b & 3);
    if b & 0x10 != 0 {
        f |= flag::JUMP;
    }
    if b & 0x20 != 0 {
        f |= flag::DATA | flag::INDIRECT;
    }
    if b & 0x40 != 0 {
        f |= flag::DATA | flag::PCM;
    }
    if b & 0x80 != 0 {
        f |= flag::ENTRY;
    }
    f
}

fn mesen2(b: u8, platform: Platform) -> u16 {
    let mut f = u16::from(b & 3);
    if b & 4 != 0 {
        f |= flag::JUMP;
    }
    if b & 8 != 0 {
        f |= flag::ENTRY;
    }
    match platform {
        Platform::Snes => {
            if b & 0x10 != 0 {
                f |= flag::X8;
            }
            if b & 0x20 != 0 {
                f |= flag::M8;
            }
        }
        Platform::GameBoyAdvance if b & 0x20 != 0 => f |= flag::THUMB,
        Platform::Nes if b & 0x80 != 0 => f |= flag::DATA | flag::PCM,
        _ => {}
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(platform: Platform, rom_size: u64, chr_size: u64) -> RomShape {
        RomShape {
            platform,
            rom_offset: 16,
            rom_size,
            chr_size,
        }
    }

    #[test]
    fn fceux_windows_and_flags() {
        let mut log = vec![0u8; 0x4000 + 0x2000];
        log[0x10] = 0x01 | (1 << 2); // code at $A010
        log[0x11] = 0x11 | (1 << 2); // code, reached by JMP ($nnnn)
        log[0x2000] = 0x02 | (2 << 2); // data at $C000
        log[0x2001] = 0x42 | (2 << 2); // a DMC sample
        log[0x4000] = 1; // CHR drawn
        let l = CodeDataLog::parse(&log, &shape(Platform::Nes, 0x4000, 0x2000)).unwrap();
        assert_eq!(l.format, LogFormat::Fceux);
        assert_eq!(l.at(16 + 0x10), flag::CODE);
        assert_eq!(l.at(16 + 0x11), flag::CODE | flag::JUMP | flag::INDIRECT);
        assert_eq!(l.at(16 + 0x2001), flag::DATA | flag::PCM);
        assert_eq!(l.chr_seen, 1);
        assert_eq!(l.page_windows().unwrap(), [Some(0xA000), Some(0xC000)]);
    }

    #[test]
    fn mesen_and_mesen2() {
        let mut log = vec![0u8; 0x4000];
        log[0] = 0x81; // a subroutine's start
        let l = CodeDataLog::parse(&log, &shape(Platform::Nes, 0x4000, 0)).unwrap();
        assert_eq!(l.format, LogFormat::Mesen);
        assert_eq!(l.at(16), flag::CODE | flag::ENTRY);
        assert!(l.windows.is_none());

        let mut log = b"CDLv2".to_vec();
        log.extend_from_slice(&0x1234ABCDu32.to_le_bytes());
        let mut flags = vec![0u8; 0x100];
        flags[0] = 0x09 | 0x30; // SNES: a subroutine's start, 8-bit A and X
        flags[1] = 0x01;
        log.extend_from_slice(&flags);
        let s = RomShape {
            platform: Platform::Snes,
            rom_offset: 0,
            rom_size: 0x100,
            chr_size: 0,
        };
        let l = CodeDataLog::parse(&log, &s).unwrap();
        assert_eq!(l.format, LogFormat::Mesen2);
        assert_eq!(l.crc32, Some(0x1234ABCD));
        assert_eq!(l.at(0), flag::CODE | flag::ENTRY | flag::M8 | flag::X8);
        let summary = l.summary(0x1234ABCD, 0);
        assert_eq!((summary.code, summary.entries, summary.crc_matches), (2, 1, Some(true)));
    }

    #[test]
    fn wrong_sizes_are_refused() {
        assert!(CodeDataLog::parse(&[0; 100], &shape(Platform::Nes, 0x4000, 0)).is_err());
        assert!(CodeDataLog::parse(&[0; 0x4000 + 5], &shape(Platform::Nes, 0x4000, 0x2000)).is_err());
    }
}
