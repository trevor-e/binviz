//! Strict shared audit inputs. No implicit ABI assumptions.
use crate::mipsaudit::AuditPolicy;
use serde_json::Value;
#[derive(Debug, Clone)]
pub struct Request {
    pub id: String,
    pub address: u64,
    pub bytes: usize,
    pub entry: u64,
    pub register: u8,
    pub policy: AuditPolicy,
}
pub fn decode_request(value: &Value) -> Result<Request, String> {
    policy_fields(
        value,
        &["id", "address", "bytes", "entry", "register", "policy"],
        "register request",
    )?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or("request needs nonempty id")?
        .to_owned();
    let text = |key: &str| -> Result<String, String> {
        match value.get(key) {
            Some(Value::String(s)) => Ok(s.clone()),
            Some(n) if n.as_u64().is_some() => Ok(n.as_u64().unwrap().to_string()),
            _ => Err(format!("{key} requires unsigned number/string")),
        }
    };
    let address = number(&text("address")?)?;
    let bytes = usize::try_from(number(&text("bytes")?)?).map_err(|_| "size overflow")?;
    let entry = number(&text("entry")?)?;
    let reg = text("register")?.trim_start_matches('$').to_ascii_lowercase();
    let names = [
        "zero", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7", "s0", "s1",
        "s2", "s3", "s4", "s5", "s6", "s7", "t8", "t9", "k0", "k1", "gp", "sp", "fp", "ra",
    ];
    let register = if reg == "s8" {
        30
    } else if let Some(n) = names.iter().position(|s| *s == reg) {
        n as u8
    } else {
        u8::try_from(number(&reg)?).map_err(|_| "invalid GPR")?
    };
    let end = address.checked_add(bytes as u64).ok_or("extent overflow")?;
    if register == 0
        || register > 31
        || address > u32::MAX as u64
        || end > 0x1_0000_0000
        || bytes == 0
        || address & 3 != 0
        || bytes & 3 != 0
        || entry < address
        || entry >= end
        || entry & 3 != 0
    {
        return Err("invalid aligned exact PS1 extent/GPR".into());
    }
    let policy = value
        .get("policy")
        .cloned()
        .map(decode_policy)
        .transpose()?
        .unwrap_or_default();
    Ok(Request {
        id,
        address,
        bytes,
        entry,
        register,
        policy,
    })
}
pub fn decode_batch(value: &Value) -> Result<Vec<Request>, String> {
    policy_fields(value, &["schemaVersion", "requests"], "register batch")?;
    if value.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err("register batch requires schemaVersion 1".into());
    }
    let items = value
        .get("requests")
        .and_then(Value::as_array)
        .ok_or("requests array required")?;
    if items.is_empty() || items.len() > 4096 {
        return Err("requires 1..4096 requests".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let request = decode_request(item).map_err(|e| format!("request index {i}: {e}"))?;
            if !ids.insert(request.id.clone()) {
                return Err(format!("duplicate request id {}", request.id));
            }
            Ok(request)
        })
        .collect()
}
pub fn run_batch(bin: &crate::Binary, value: &Value) -> Result<Value, String> {
    let requests = decode_batch(value)?;
    let rows: Result<Vec<_>, String> = requests
        .iter()
        .map(|r| {
            let report = bin
                .audit_ps1_register(r.address, r.bytes, r.entry, r.register, &r.policy)
                .map_err(|e| format!("request {}: {e}", r.id))?;
            Ok(serde_json::json!({"id":r.id,"report":report}))
        })
        .collect();
    Ok(serde_json::json!({"schemaVersion":1,"requests":rows?}))
}
pub fn number(s: &str) -> Result<u64, String> {
    if let Some(s) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(s, 16).map_err(|e| e.to_string())
    } else {
        s.parse::<u64>().map_err(|e| e.to_string())
    }
}
fn policy_fields(value: &serde_json::Value, allowed: &[&str], context: &str) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{context} must be an object"))?;
    if let Some(name) = object.keys().find(|name| !allowed.contains(&name.as_str())) {
        return Err(format!("unknown {context} field: {name}"));
    }
    Ok(())
}
fn policy_word(value: &mut serde_json::Value, context: &str) -> Result<(), String> {
    let n = if let Some(s) = value.as_str() {
        number(s)?
    } else {
        value
            .as_u64()
            .ok_or_else(|| format!("{context} must be an unsigned word"))?
    };
    let n = u32::try_from(n).map_err(|_| format!("{context} does not fit 32 bits"))?;
    *value = serde_json::Value::from(n);
    Ok(())
}
pub fn decode_policy(mut value: serde_json::Value) -> Result<crate::mipsaudit::AuditPolicy, String> {
    policy_fields(
        &value,
        &[
            "returnUse",
            "callees",
            "indirectTargets",
            "maxStates",
            "mode",
            "incomingMask",
            "crossRegisterCallees",
            "loopBounds",
        ],
        "register policy",
    )?;
    if let Some(mask) = value.get_mut("incomingMask") {
        policy_word(mask, "incomingMask")?;
        if mask.as_u64() == Some(0) {
            return Err("incomingMask must be nonzero".into());
        }
    }
    if let Some(bounds) = value.get_mut("loopBounds") {
        for b in bounds.as_array_mut().ok_or("loopBounds requires an array")? {
            policy_fields(
                b,
                &["from", "target", "maxTraversals", "evidence", "nativeSha256"],
                "loop bound",
            )?;
            for key in ["from", "target"] {
                policy_word(b.get_mut(key).ok_or("loop bound needs from/target")?, key)?;
            }
        }
    }
    if let Some(map) = value.get_mut("crossRegisterCallees") {
        let object = map
            .as_object_mut()
            .ok_or("crossRegisterCallees requires an address map")?;
        for (address, mut records) in std::mem::take(object) {
            let target = u32::try_from(number(&address)?).map_err(|_| "cross-register address exceeds 32 bits")?;
            if target & 3 != 0 {
                return Err("cross-register target unaligned".into());
            }
            for record in records
                .as_array_mut()
                .ok_or("cross-register target needs summary array")?
            {
                policy_fields(
                    record,
                    &["register", "effect", "evidence", "instructionPath"],
                    "cross-register summary",
                )?;
                if let Some(points) = record.get_mut("instructionPath") {
                    for p in points.as_array_mut().ok_or("instructionPath needs an array")? {
                        policy_fields(p, &["pc", "word"], "reviewed instruction")?;
                        for key in ["pc", "word"] {
                            policy_word(p.get_mut(key).ok_or("instruction needs pc/word")?, key)?;
                        }
                    }
                }
            }
            if object.insert(target.to_string(), records).is_some() {
                return Err("duplicate cross-register target".into());
            }
        }
    }
    for name in ["callees", "indirectTargets"] {
        let Some(map) = value.get_mut(name) else { continue };
        let object = map
            .as_object_mut()
            .ok_or_else(|| format!("{name} must be an address-keyed object"))?;
        let original = std::mem::take(object);
        for (address, mut item) in original {
            let target = u32::try_from(number(&address)?).map_err(|_| format!("{name} address exceeds 32 bits"))?;
            if target & 3 != 0 {
                return Err(format!("unaligned {name} address"));
            }
            if name == "callees" {
                policy_fields(
                    &item,
                    &["register", "effect", "evidence", "instructionPath"],
                    "callee summary",
                )?;
                if let Some(points) = item.get_mut("instructionPath") {
                    for point in points.as_array_mut().ok_or("instructionPath must be an array")? {
                        policy_fields(point, &["pc", "word"], "reviewed instruction")?;
                        for key in ["pc", "word"] {
                            policy_word(point.get_mut(key).ok_or("reviewed instruction requires pc/word")?, key)?;
                        }
                    }
                }
            } else {
                policy_fields(&item, &["targets", "evidence"], "indirect target summary")?;
                for target in item
                    .get_mut("targets")
                    .ok_or("indirect summary needs targets")?
                    .as_array_mut()
                    .ok_or("targets must be an array")?
                {
                    policy_word(target, "indirect target")?;
                }
            }
            if object.insert(target.to_string(), item).is_some() {
                return Err(format!("duplicate normalized {name} address"));
            }
        }
    }
    let policy: crate::mipsaudit::AuditPolicy = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if policy.max_states == 0 {
        return Err("maxStates must be positive".into());
    }
    for summary in policy
        .callees
        .values()
        .chain(policy.cross_register_callees.values().flatten())
    {
        if !(1..32).contains(&summary.register) || summary.evidence.trim().is_empty() {
            return Err("callee summary needs register1..31 and reviewed evidence".into());
        }
        if summary.instruction_path.iter().any(|p| p.pc & 3 != 0) {
            return Err("reviewed instruction pc is unaligned".into());
        }
    }
    for targets in policy.indirect_targets.values() {
        if targets.targets.is_empty()
            || targets.evidence.trim().is_empty()
            || targets.targets.iter().any(|&t| t & 3 != 0)
        {
            return Err("indirect summary needs nonempty aligned targets and reviewed evidence".into());
        }
    }
    Ok(policy)
}
