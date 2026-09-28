//! Pretty-printing of DWARF expressions (location descriptions).

use gimli::{Operation, Reader};

use super::R;

pub(crate) fn register_name(arch: object::Architecture, reg: gimli::Register) -> String {
    let name = match arch {
        object::Architecture::X86_64 | object::Architecture::X86_64_X32 => gimli::X86_64::register_name(reg),
        object::Architecture::I386 => gimli::X86::register_name(reg),
        object::Architecture::Aarch64 | object::Architecture::Aarch64_Ilp32 => gimli::AArch64::register_name(reg),
        object::Architecture::Arm => gimli::Arm::register_name(reg),
        object::Architecture::Riscv32 | object::Architecture::Riscv64 => gimli::RiscV::register_name(reg),
        object::Architecture::LoongArch64 => gimli::LoongArch::register_name(reg),
        object::Architecture::PowerPc | object::Architecture::PowerPc64 => gimli::PowerPc64::register_name(reg),
        _ => None,
    };
    match name {
        Some(n) => n.to_string(),
        None => format!("r{}", reg.0),
    }
}

/// Formats an expression as `DW_OP_x operand; DW_OP_y ...`.
pub(crate) fn format(expr: &gimli::Expression<R>, encoding: gimli::Encoding, arch: object::Architecture) -> String {
    let mut input = expr.0.clone();
    let mut parts = Vec::new();
    while !input.is_empty() {
        let opcode = match input.clone().read_u8() {
            Ok(b) => b,
            Err(_) => break,
        };
        let name = gimli::DwOp(opcode).static_string().unwrap_or("DW_OP_<unknown>");
        let op = match Operation::parse(&mut input, encoding) {
            Ok(op) => op,
            Err(e) => {
                parts.push(format!("{name} <error: {e}>"));
                break;
            }
        };
        let reg = |r: gimli::Register| register_name(arch, r);
        let operand = match op {
            Operation::Deref { size, space, .. } => {
                if opcode == gimli::DW_OP_deref.0 {
                    String::new()
                } else {
                    format!("{size}{}", if space { " (space)" } else { "" })
                }
            }
            Operation::Pick { index } if opcode == gimli::DW_OP_pick.0 => index.to_string(),
            Operation::PlusConstant { value } => value.to_string(),
            Operation::Bra { target } | Operation::Skip { target } => format!("{target:+}"),
            Operation::UnsignedConstant { value } => {
                if (gimli::DW_OP_lit0.0..=gimli::DW_OP_lit31.0).contains(&opcode) {
                    String::new()
                } else {
                    value.to_string()
                }
            }
            Operation::SignedConstant { value } => value.to_string(),
            Operation::Register { register } => {
                if opcode == gimli::DW_OP_regx.0 {
                    format!("{} ({})", register.0, reg(register))
                } else {
                    format!("({})", reg(register))
                }
            }
            Operation::RegisterOffset { register, offset, .. } => {
                if opcode == gimli::DW_OP_bregx.0 {
                    format!("{} ({}) {offset:+}", register.0, reg(register))
                } else {
                    format!("({}) {offset:+}", reg(register))
                }
            }
            Operation::FrameOffset { offset } => format!("{offset:+}"),
            Operation::Call { offset } => format!("{offset:?}"),
            Operation::VariableValue { offset } => format!("{:#x}", offset.0),
            Operation::Piece {
                size_in_bits,
                bit_offset,
            } => match bit_offset {
                Some(b) => format!("{size_in_bits} bits at {b}"),
                None => format!("{} bytes", size_in_bits / 8),
            },
            Operation::ImplicitValue { data } => {
                let bytes = data.to_slice().map(|c| c.into_owned()).unwrap_or_default();
                if bytes.iter().all(|&b| b == 0) {
                    format!("0 ({} zero bytes)", bytes.len())
                } else if bytes.len() == 8 {
                    let v = u64::from_le_bytes(bytes[..8].try_into().unwrap_or([0; 8]));
                    format!(
                        "[{}] (= {} as double)",
                        crate::util::hex_bytes(&bytes),
                        f64::from_bits(v)
                    )
                } else {
                    format!("[{}]", crate::util::hex_bytes(&bytes[..bytes.len().min(16)]))
                }
            }
            Operation::ImplicitPointer { value, byte_offset } => format!("{:#x} {byte_offset:+}", value.0),
            Operation::EntryValue { expression } => {
                format!("[{}]", self::format(&gimli::Expression(expression), encoding, arch))
            }
            Operation::ParameterRef { offset } => format!("{:#x}", offset.0),
            Operation::Address { address } => format!("{address:#x}"),
            Operation::AddressIndex { index } | Operation::ConstantIndex { index } => format!("index {}", index.0),
            Operation::TypedLiteral { base_type, value } => format!("type {:#x}, {} bytes", base_type.0, value.len()),
            Operation::Convert { base_type } | Operation::Reinterpret { base_type } => {
                format!("type {:#x}", base_type.0)
            }
            Operation::WasmLocal { index } | Operation::WasmGlobal { index } | Operation::WasmStack { index } => {
                index.to_string()
            }
            _ => String::new(),
        };
        parts.push(if operand.is_empty() {
            name.to_string()
        } else {
            format!("{name} {operand}")
        });
    }
    parts.join("; ")
}
