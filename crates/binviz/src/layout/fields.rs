//! A tiny declarative description of on-disk structs, so that every header byte
//! can be labelled with the field it belongs to and a human-readable value.

use crate::layout::Ctx;
use crate::util::{self, Bytes};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Ty {
    U8,
    U16,
    U32,
    U64,
    I16,
    I32,
    I64,
    /// Raw bytes shown as hex.
    Bytes(u32),
    /// Fixed-size NUL-padded string.
    Str(u32),
}

impl Ty {
    pub fn size(self) -> u64 {
        match self {
            Ty::U8 => 1,
            Ty::U16 | Ty::I16 => 2,
            Ty::U32 | Ty::I32 => 4,
            Ty::U64 | Ty::I64 => 8,
            Ty::Bytes(n) | Ty::Str(n) => n as u64,
        }
    }
}

/// How to render a field's value.
#[derive(Clone, Copy)]
pub(crate) enum Fmt {
    Dec,
    Hex,
    /// Virtual address.
    Addr,
    /// File offset.
    Off,
    /// Byte count.
    Size,
    /// Constant with a symbolic name.
    Name(fn(u64) -> Option<&'static str>),
    /// Constant whose name depends on context (e.g. relocation types per arch).
    CtxName(fn(&Ctx, u64) -> Option<String>),
    /// Bit flags, rendered by the given function.
    Flags(fn(u64) -> String),
    /// Anything else that needs context.
    Custom(fn(&Ctx, u64) -> String),
    /// Index into the current string table (`Ctx::strtab`).
    StrIndex,
    /// Unix timestamp.
    Time,
    /// Mach-O packed version.
    MachVersion,
    /// RVA (PE): shown with the VA it maps to.
    Rva,
    /// Whatever the type suggests (strings quoted, bytes as hex).
    Auto,
}

#[derive(Clone, Copy)]
pub(crate) struct F {
    pub name: &'static str,
    pub ty: Ty,
    pub fmt: Fmt,
}

impl F {
    pub const fn new(name: &'static str, ty: Ty, fmt: Fmt) -> F {
        F { name, ty, fmt }
    }
    pub const fn u8(name: &'static str, fmt: Fmt) -> F {
        F::new(name, Ty::U8, fmt)
    }
    pub const fn u16(name: &'static str, fmt: Fmt) -> F {
        F::new(name, Ty::U16, fmt)
    }
    pub const fn u32(name: &'static str, fmt: Fmt) -> F {
        F::new(name, Ty::U32, fmt)
    }
    pub const fn u64(name: &'static str, fmt: Fmt) -> F {
        F::new(name, Ty::U64, fmt)
    }
    pub const fn i16(name: &'static str) -> F {
        F::new(name, Ty::I16, Fmt::Dec)
    }
    pub const fn i32(name: &'static str) -> F {
        F::new(name, Ty::I32, Fmt::Dec)
    }
    pub const fn i64(name: &'static str) -> F {
        F::new(name, Ty::I64, Fmt::Dec)
    }
    pub const fn bytes(name: &'static str, n: u32) -> F {
        F::new(name, Ty::Bytes(n), Fmt::Auto)
    }
    pub const fn str(name: &'static str, n: u32) -> F {
        F::new(name, Ty::Str(n), Fmt::Auto)
    }
}

/// A decoded field.
#[derive(Clone, Debug)]
pub(crate) struct FieldValue {
    pub start: u64,
    pub end: u64,
    pub name: &'static str,
    pub raw: u64,
    pub value: String,
}

pub(crate) fn struct_size(fields: &[F]) -> u64 {
    fields.iter().map(|f| f.ty.size()).sum()
}

/// Reads a single field value as an integer (strings/bytes read as 0).
pub(crate) fn read_raw(bytes: &Bytes, offset: u64, ty: Ty) -> Option<u64> {
    Some(match ty {
        Ty::U8 => bytes.u8(offset)? as u64,
        Ty::U16 => bytes.u16(offset)? as u64,
        Ty::U32 => bytes.u32(offset)? as u64,
        Ty::U64 => bytes.u64(offset)?,
        Ty::I16 => bytes.u16(offset)? as i16 as i64 as u64,
        Ty::I32 => bytes.u32(offset)? as i32 as i64 as u64,
        Ty::I64 => bytes.u64(offset)?,
        Ty::Bytes(n) | Ty::Str(n) => {
            bytes.slice(offset, n as u64)?;
            0
        }
    })
}

/// Decodes consecutive fields starting at `offset`. Stops at the first field that
/// runs past the end of the data.
pub(crate) fn decode(ctx: &Ctx, offset: u64, fields: &[F]) -> Vec<FieldValue> {
    let mut out = Vec::with_capacity(fields.len());
    let mut pos = offset;
    for f in fields {
        let size = f.ty.size();
        let Some(raw) = read_raw(&ctx.bytes, pos, f.ty) else {
            break;
        };
        let value = format_value(ctx, pos, f, raw);
        out.push(FieldValue {
            start: pos,
            end: pos + size,
            name: f.name,
            raw,
            value,
        });
        pos += size;
    }
    out
}

/// Looks up a field value by name in an already decoded list.
pub(crate) fn get(values: &[FieldValue], name: &str) -> u64 {
    values.iter().find(|v| v.name == name).map_or(0, |v| v.raw)
}

fn format_value(ctx: &Ctx, pos: u64, f: &F, raw: u64) -> String {
    match (f.fmt, f.ty) {
        (Fmt::Auto, Ty::Str(n)) => {
            util::quote(&util::fixed_str(ctx.bytes.slice(pos, n as u64).unwrap_or(&[])).into_bytes())
        }
        (Fmt::Auto, Ty::Bytes(n)) => {
            let b = ctx.bytes.slice(pos, n as u64).unwrap_or(&[]);
            if n == 16 && f.name.contains("uuid") {
                util::uuid(b)
            } else if b.len() <= 32 {
                util::hex_bytes(b)
            } else {
                format!("{} …", util::hex_bytes(&b[..32]))
            }
        }
        (Fmt::Auto, Ty::I16 | Ty::I32 | Ty::I64) | (Fmt::Dec, Ty::I16 | Ty::I32 | Ty::I64) => (raw as i64).to_string(),
        (Fmt::Auto | Fmt::Dec, _) => raw.to_string(),
        (Fmt::Hex, _) => util::hex(raw),
        (Fmt::Addr, _) => util::hex(raw),
        (Fmt::Off, _) => util::hex(raw),
        (Fmt::Size, _) => {
            if raw < 10 {
                raw.to_string()
            } else {
                format!("{} ({raw})", util::hex(raw))
            }
        }
        (Fmt::Name(name), _) => named(name(raw).map(str::to_string), raw),
        (Fmt::CtxName(name), _) => named(name(ctx, raw), raw),
        (Fmt::Flags(flags), _) => {
            if raw == 0 {
                "0".into()
            } else {
                format!("{} ({})", flags(raw), util::hex(raw))
            }
        }
        (Fmt::Custom(custom), _) => custom(ctx, raw),
        (Fmt::StrIndex, _) => match ctx.string_at(raw) {
            Some(s) => format!("{} → {}", util::hex(raw), util::quote(s)),
            None => util::hex(raw),
        },
        (Fmt::Time, _) => format!("{} ({})", util::unix_time(raw), util::hex(raw)),
        (Fmt::MachVersion, _) => format!("{} ({})", util::macho_version(raw as u32), util::hex(raw)),
        (Fmt::Rva, _) => {
            if raw == 0 {
                "0".into()
            } else {
                format!(
                    "{} (VA {})",
                    util::hex(raw),
                    util::hex(ctx.image_base.wrapping_add(raw))
                )
            }
        }
    }
}

fn named(name: Option<String>, raw: u64) -> String {
    match name {
        Some(n) => format!("{n} ({})", util::hex(raw)),
        None => util::hex(raw),
    }
}
