//! Inspect actual linked WASM signatures and allocations, reusing the module reader.
use crate::evidence::{hex, sha256};
use crate::wasm::{
    code,
    read::{ImportDesc, Kind, Mode, Module},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Region {
    pub name: String,
    pub start: String,
    pub bytes: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataAllocation {
    pub name: String,
    pub export: String,
    pub address: String,
    pub bytes: String,
    pub alias_of: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutCertificate {
    pub module: String,
    pub guest_ram: Region,
    pub reserved: Vec<Region>,
    pub stack_pointer_export: String,
    pub stack_pointer: String,
    pub data_end_export: String,
    pub data_end: String,
    pub heap_base_export: String,
    pub heap_base: String,
    pub memory_import: Option<String>,
    pub table_import: Option<String>,
    pub shared_memory: bool,
    pub required_imports: Vec<String>,
    pub data: Vec<DataAllocation>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedFunction {
    pub index: u32,
    pub names: Vec<String>,
    pub exports: Vec<String>,
    pub parameters: Vec<String>,
    pub results: Vec<String>,
    pub imported: bool,
    pub calls: Vec<LinkedInstruction>,
    pub body_start: Option<String>,
    pub body_end: Option<String>,
    pub stack_pointer_operations: Vec<serde_json::Value>,
    pub indirect_calls: Vec<String>,
    pub import_identity: Option<ImportIdentity>,
    pub indirect_signatures: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportIdentity {
    pub module: String,
    pub field: String,
    pub parameters: Vec<String>,
    pub results: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializationWrite {
    pub segment: usize,
    pub memory: u32,
    pub address: Option<String>,
    pub bytes: String,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedInstruction {
    pub offset: String,
    pub target: u32,
    pub tail: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleReport {
    pub sha256: String,
    pub functions: Vec<LinkedFunction>,
    pub globals: BTreeMap<String, String>,
    pub imports: Vec<String>,
    pub memory_imports: Vec<String>,
    pub table_imports: Vec<String>,
    pub memory_min_bytes: Vec<String>,
    pub shared_memory: Vec<bool>,
    pub problems: Vec<String>,
    pub memory_max_bytes: Vec<Option<String>>,
    pub initialization_writes: Vec<InitializationWrite>,
    pub start_function: Option<u32>,
    pub table_initializers: Vec<serde_json::Value>,
    pub data_symbols: Vec<serde_json::Value>,
    pub import_inventory: Vec<serde_json::Value>,
}
pub fn inspect(bytes: &[u8]) -> Result<ModuleReport, String> {
    let m = Module::parse(bytes).map_err(|e| e.to_string())?;
    let import_inventory=m.imports.iter().enumerate().map(|(ordinal,i)|{
        let type_index=if let ImportDesc::Func(n)=i.desc{Some(n)}else{None};let ty=type_index.and_then(|n|m.types.get(n as usize)).and_then(Option::as_ref);
        let index=m.imports[..ordinal].iter().filter(|p|p.desc.kind()==i.desc.kind()).count();
        serde_json::json!({"ordinal":ordinal,"module":i.module,"field":i.field,"kind":i.desc.kind().name(),"index":index,"typeIndex":type_index,"parameters":ty.map(|t|t.params.iter().map(ToString::to_string).collect::<Vec<_>>()),"results":ty.map(|t|t.results.iter().map(ToString::to_string).collect::<Vec<_>>())})
    }).collect();
    let mut functions = vec![];
    let mut problems = m.problems.clone();
    let stack_global = m
        .exports
        .iter()
        .find(|e| e.kind == Kind::Global && e.name == "__stack_pointer")
        .map(|e| e.index);
    for (index, f) in m.funcs.iter().enumerate() {
        let ty = m
            .func_type(index as u32)
            .ok_or("linked function has no supported type")?;
        let mut names: Vec<_> = m
            .exports
            .iter()
            .filter(|e| e.kind == Kind::Func && e.index == index as u32)
            .map(|e| e.name.clone())
            .collect();
        let exports = names.clone();
        names.extend(m.names.functions.get(&(index as u32)).cloned());
        names.extend(
            m.symbols
                .iter()
                .filter(|s| s.kind == 0 && s.index == index as u32)
                .filter_map(|s| s.name.clone()),
        );
        names.sort();
        names.dedup();
        let mut calls = vec![];
        let mut stack_pointer_operations = vec![];
        let mut indirect_calls = vec![];
        let mut indirect_signatures = BTreeMap::new();
        if let Some(body) = &f.body {
            let decoded = code::decode_body(bytes, body.clone());
            if let Some(offset) = decoded.stopped {
                problems.push(format!("function {index} decoding stopped at 0x{offset:x}"));
            }
            for (position, (offset, insn)) in decoded.insns.iter().enumerate() {
                if matches!(insn.op, code::CALL | code::RETURN_CALL) {
                    if let code::Imm::Func(target) = insn.imm {
                        calls.push(LinkedInstruction {
                            offset: format!("0x{offset:x}"),
                            target,
                            tail: insn.op == code::RETURN_CALL,
                        });
                    }
                }
                if matches!(
                    insn.op,
                    code::CALL_INDIRECT | code::RETURN_CALL_INDIRECT | code::CALL_REF | code::RETURN_CALL_REF
                ) {
                    indirect_calls.push(format!("0x{offset:x}"));
                    if let code::Imm::Indirect { ty, table } = insn.imm {
                        if let Some(Some(t)) = m.types.get(ty as usize) {
                            indirect_signatures.insert(format!("0x{offset:x}"),serde_json::json!({"table":table,"parameters":t.params.iter().map(ToString::to_string).collect::<Vec<_>>(),"results":t.results.iter().map(ToString::to_string).collect::<Vec<_>>()}));
                        }
                    }
                }
                if matches!(insn.op, code::GLOBAL_GET | code::GLOBAL_SET)
                    && matches!(insn.imm,code::Imm::Global(index) if Some(index)==stack_global)
                {
                    let context:Vec<_>=decoded.insns[position.saturating_sub(6)..(position+3).min(decoded.insns.len())].iter().map(|(at,i)|serde_json::json!({"offset":format!("0x{at:x}"),"operation":i.name,"immediate":format!("{:?}",i.imm)})).collect();
                    let local_writes: Vec<_> = decoded
                        .insns
                        .iter()
                        .filter(|(_, i)| matches!(i.op, code::LOCAL_SET | code::LOCAL_TEE))
                        .map(|(at, i)| serde_json::json!({"offset":format!("0x{at:x}"),"local":format!("{:?}",i.imm)}))
                        .collect();
                    let exits: Vec<_> = decoded
                        .insns
                        .iter()
                        .filter(|(_, i)| {
                            matches!(
                                i.op,
                                code::END
                                    | code::RETURN
                                    | code::RETURN_CALL
                                    | code::RETURN_CALL_INDIRECT
                                    | code::RETURN_CALL_REF
                            )
                        })
                        .map(|(at, _)| format!("0x{at:x}"))
                        .collect();
                    stack_pointer_operations.push(
                        serde_json::json!({"offset":format!("0x{offset:x}"),"operation":insn.name,"context":context,"controlBefore":decoded.insns[..position].iter().any(|(_,i)|matches!(i.name,"block"|"loop"|"if"|"br"|"br_if"|"br_table")),"hasControl":decoded.insns.iter().any(|(_,i)|matches!(i.name,"block"|"loop"|"if"|"br"|"br_if"|"br_table"|"try"|"try_table")),"localWrites":local_writes,"exits":exits}),
                    );
                }
            }
        }
        functions.push(LinkedFunction {
            index: index as u32,
            names,
            exports,
            parameters: ty.params.iter().map(ToString::to_string).collect(),
            results: ty.results.iter().map(ToString::to_string).collect(),
            imported: f.import.is_some(),
            calls,
            body_start: f.body.as_ref().map(|b| format!("0x{:x}", b.code)),
            body_end: f.body.as_ref().map(|b| format!("0x{:x}", b.end)),
            stack_pointer_operations,
            indirect_calls,
            import_identity: f
                .import
                .and_then(|n| m.imports.get(n as usize))
                .map(|i| ImportIdentity {
                    module: i.module.clone(),
                    field: i.field.clone(),
                    parameters: ty.params.iter().map(ToString::to_string).collect(),
                    results: ty.results.iter().map(ToString::to_string).collect(),
                }),
            indirect_signatures,
        });
    }
    let globals = m
        .exports
        .iter()
        .filter(|e| e.kind == Kind::Global)
        .filter_map(|e| {
            m.globals
                .get(e.index as usize)
                .and_then(|g| g.init)
                .and_then(|v| v.address())
                .map(|v| (e.name.clone(), format!("0x{v:x}")))
        })
        .collect();
    let import_name = |index: u32| {
        m.imports
            .get(index as usize)
            .map(|i| format!("{}.{}", i.module, i.field))
    };
    Ok(ModuleReport {
        sha256: sha256(bytes),
        import_inventory,
        functions,
        globals,
        imports: m
            .imports
            .iter()
            .map(|i| format!("{}.{}:{}", i.module, i.field, i.desc.kind().name()))
            .collect(),
        memory_imports: m
            .memories
            .iter()
            .filter_map(|m| m.import.and_then(import_name))
            .collect(),
        table_imports: m.tables.iter().filter_map(|t| t.import.and_then(import_name)).collect(),
        memory_min_bytes: m
            .memories
            .iter()
            .map(|m| m.limits.min.checked_mul(m.limits.page_size()).map(|n|format!("0x{n:x}")).ok_or_else(||"memory minimum byte size overflows u64".to_owned()))
            .collect::<Result<_,_>>()?,
        shared_memory: m.memories.iter().map(|m| m.limits.shared).collect(),
        problems,
        memory_max_bytes: m.memories.iter().map(|m| m.limits.max.map(|n|n.checked_mul(m.limits.page_size()).map(|n|format!("0x{n:x}")).ok_or_else(||"memory maximum byte size overflows u64".to_owned())).transpose()).collect::<Result<_,_>>()?,
        initialization_writes: m.data.iter().enumerate().filter_map(|(segment,s)| {
            if let Mode::Active { index, offset } = s.mode {
                Some(InitializationWrite { segment, memory:index, address:offset.address().map(|n|format!("0x{n:x}")),
                    bytes:format!("0x{:x}",s.bytes.end-s.bytes.start),
                    sha256:sha256(&bytes[s.bytes.start as usize..s.bytes.end as usize]) })
            } else { None }
        }).collect(),
        start_function: m.start,
        table_initializers: m.elements.iter().enumerate().filter_map(|(segment,s)| {
            if let Mode::Active { index, offset } = s.mode {
                Some(serde_json::json!({"segment":segment,"table":index,"offset":offset.address().map(|n|format!("0x{n:x}")),"functions":s.items.iter().map(|i|i.0).collect::<Vec<_>>()}))
            } else { None }
        }).collect(),
        data_symbols:m.symbols.iter().filter(|s|s.kind==1).map(|s|serde_json::json!({"name":s.name,"defined":s.data.is_some(),"storage":s.data.map(|(segment,offset,bytes)|serde_json::json!({"segment":segment,"offset":format!("0x{offset:x}"),"bytes":format!("0x{bytes:x}")}))})).collect(),
    })
}

impl LayoutCertificate {
    /// All addresses are checked against actual exported constant globals. A
    /// function-import list alone never proves tentative data was allocated.
    pub fn verify(&self, actual: &ModuleReport) -> Result<Vec<String>, String> {
        let mut reasons = actual.problems.clone();
        let end = |r: &Region| -> Result<u64, String> {
            hex(&r.start)?
                .checked_add(hex(&r.bytes)?)
                .ok_or_else(|| "layout region overflow".into())
        };
        let ram_start = hex(&self.guest_ram.start)?;
        let ram_end = end(&self.guest_ram)?;
        if ram_start == ram_end {
            return Err("guest RAM region must be nonempty".into());
        }
        let mut ranges = vec![&self.guest_ram];
        ranges.extend(&self.reserved);
        for (i, a) in ranges.iter().enumerate() {
            if hex(&a.bytes)? == 0 {
                return Err("reserved region must be nonempty".into());
            }
            for b in &ranges[i + 1..] {
                if hex(&a.start)? < end(b)? && hex(&b.start)? < end(a)? {
                    reasons.push(format!("reserved regions {} and {} overlap", a.name, b.name));
                }
            }
        }
        for (export, want) in [
            (&self.stack_pointer_export, &self.stack_pointer),
            (&self.data_end_export, &self.data_end),
            (&self.heap_base_export, &self.heap_base),
        ] {
            hex(want)?;
            if actual.globals.get(export).is_none_or(|v| hex(v).ok() != hex(want).ok()) {
                reasons.push(format!("actual linked global {export} missing or shifted from {want}"));
            }
        }
        let stack = hex(&self.stack_pointer)?;
        if !self
            .reserved
            .iter()
            .any(|r| r.name == "stack" && hex(&r.start).is_ok_and(|s| s < stack) && end(r).ok() == Some(stack))
        {
            reasons.push("stack pointer does not equal the configured reserved stack end".into());
        }
        if actual.memory_imports != self.memory_import.iter().cloned().collect::<Vec<_>>() {
            reasons.push("actual memory import identity differs".into());
        }
        if actual.table_imports != self.table_import.iter().cloned().collect::<Vec<_>>() {
            reasons.push("actual table import identity differs".into());
        }
        if actual.shared_memory != [self.shared_memory] {
            reasons.push("module requires exactly one memory with the declared sharing mode".into());
        }
        if actual.imports.iter().any(|i| !self.required_imports.contains(i))
            || self.required_imports.iter().any(|i| !actual.imports.contains(i))
        {
            reasons.push("actual required imports differ from the checked layout certificate".into());
        }
        let capacity = actual.memory_min_bytes.first().and_then(|s| hex(s).ok()).unwrap_or(0);
        if ranges.iter().any(|r| end(r).is_ok_and(|e| e > capacity)) {
            reasons.push("reserved range exceeds actual memory minimum".into());
        }
        let mut symbols = BTreeMap::new();
        for d in &self.data {
            if d.name.is_empty() || symbols.insert(&d.name, d).is_some() {
                return Err("duplicate/empty allocation symbol".into());
            }
            let start = hex(&d.address)?;
            let size = hex(&d.bytes)?;
            let limit = start.checked_add(size).ok_or("allocation overflow")?;
            if size == 0 || start < ram_start || limit > ram_end {
                reasons.push(format!("{} unresolved/unallocated or outside guest RAM", d.name));
            }
            if actual.globals.get(&d.export).is_none_or(|v| hex(v).ok() != Some(start)) {
                reasons.push(format!("{} lacks the actual linked data address/export", d.name));
            }
            if limit > hex(&self.data_end)? {
                reasons.push(format!("{} exceeds the actual data end", d.name));
            }
        }
        for d in &self.data {
            if let Some(alias) = &d.alias_of {
                if symbols
                    .get(alias)
                    .is_none_or(|other| other.address != d.address || other.bytes != d.bytes)
                {
                    reasons.push(format!("alias {} has no matching allocation lineage", d.name));
                }
            }
        }
        for (i, a) in self.data.iter().enumerate() {
            for b in &self.data[i + 1..] {
                if hex(&a.address)? < hex(&b.address)? + hex(&b.bytes)?
                    && hex(&b.address)? < hex(&a.address)? + hex(&a.bytes)?
                    && a.alias_of.as_ref() != Some(&b.name)
                    && b.alias_of.as_ref() != Some(&a.name)
                    && !(a.alias_of.is_some() && a.alias_of == b.alias_of)
                {
                    reasons.push(format!(
                        "allocations {} and {} overlap without declared alias lineage",
                        a.name, b.name
                    ));
                }
            }
        }
        Ok(reasons)
    }
}
