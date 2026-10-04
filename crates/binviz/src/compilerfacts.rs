//! Versioned compiler observations and all-call contract audit. Clang owns C parsing.
use crate::contracts::{CallContract, ContractKind, ContractReport, Signature};
use crate::evidence::{EvidenceManifest, sha256};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceSpan {
    pub artifact: String,
    pub start: u64,
    pub end: u64,
    pub editable: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompilerType {
    pub spelling: String,
    pub canonical: String,
    /// void/integer/pointer/unsupported; no native ABI inferred here.
    pub category: String,
    pub bits: Option<u32>,
    pub signed: Option<bool>,
}

impl CompilerType {
    fn supported(&self, result: bool) -> bool {
        (result && self.category == "void")
            || (matches!(self.category.as_str(), "integer" | "pointer") && self.bits.is_some_and(|b| b > 0 && b <= 32))
    }
    fn same_wire(&self, other: &Self) -> bool {
        self.category == other.category && self.bits == other.bits && self.signed == other.signed
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definition {
    pub id: String,
    pub unit: String,
    pub name: String,
    pub linkage: String,
    pub has_body: bool,
    pub old_style: bool,
    pub variadic: bool,
    pub return_type: CompilerType,
    pub parameters: Vec<CompilerType>,
    /// Clang callable contract, distinct from K&R narrowed local parameter types.
    pub abi_parameters: Vec<CompilerType>,
    pub span: SourceSpan,
    /// Shared start identifies a multi-declarator statement; individual spans need not end at ';'.
    pub declaration_group: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_span: Option<SourceSpan>,
}
impl Definition {
    fn signature(&self) -> Signature {
        Signature {
            return_type: self.return_type.spelling.clone(),
            parameter_types: self.abi_parameters.iter().map(|t| t.spelling.clone()).collect(),
            variadic: self.variadic,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Argument {
    pub supplied: CompilerType,
    pub promoted: CompilerType,
    pub span: SourceSpan,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectCall {
    pub id: String,
    pub unit: String,
    pub caller: String,
    pub callee: String,
    pub declaration: String,
    pub span: SourceSpan,
    pub arguments: Vec<Argument>,
    pub result_consumed: bool,
    pub result_type: CompilerType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callee_span: Option<SourceSpan>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddressReference {
    pub unit: String,
    pub caller: String,
    pub declaration: String,
    pub span: SourceSpan,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtractionGap {
    pub unit: String,
    pub stage: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranslationUnit {
    pub id: String,
    pub source: String,
    pub prepared: Option<String>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompilerInfo {
    pub binary: String,
    pub version: String,
    pub target: String,
    pub flags: Vec<String>,
    pub profile: String,
    pub target_macros: BTreeMap<String, String>,
    pub adapter: String,
    pub adapter_sha256: String,
    pub working_directory: String,
    pub data_layout: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompilerFacts {
    pub format: String,
    pub schema_version: u32,
    pub compiler: CompilerInfo,
    pub evidence: EvidenceManifest,
    pub units: Vec<TranslationUnit>,
    pub definitions: Vec<Definition>,
    pub calls: Vec<DirectCall>,
    pub address_references: Vec<AddressReference>,
    pub gaps: Vec<ExtractionGap>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compiler_storage: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub intrinsics: Vec<Intrinsic>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Intrinsic {
    pub id: String,
    pub unit: String,
    pub caller: String,
    pub name: String,
    pub effect: String,
    pub span: SourceSpan,
    pub arguments: Vec<Argument>,
    pub result_type: CompilerType,
}
impl CompilerFacts {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let facts: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if facts.schema_version != 1 || facts.format != "binviz-compiler-facts" {
            return Err("requires binviz-compiler-facts schemaVersion 1".into());
        }
        facts.evidence.validate()?;
        let artifacts: BTreeMap<_, _> = facts.evidence.artifacts.iter().map(|a| (a.id.as_str(), a)).collect();
        if !artifacts.contains_key(facts.compiler.binary.as_str()) {
            return Err("compiler binary artifact missing".into());
        }
        if artifacts
            .get("adapter")
            .is_none_or(|a| a.sha256 != facts.compiler.adapter_sha256)
        {
            return Err("adapter artifact identity missing or differs from recipe".into());
        }
        let recipe = artifacts.get("recipe").ok_or("compiler recipe artifact missing")?;
        if sha256(&serde_json::to_vec(&json!(facts.compiler)).map_err(|e| e.to_string())?) != recipe.sha256 {
            return Err("compiler metadata differs from recipe identity".into());
        }
        let mut units = BTreeSet::new();
        for u in &facts.units {
            if u.id.trim().is_empty() || !units.insert(u.id.as_str()) || !artifacts.contains_key(u.source.as_str()) {
                return Err("invalid/duplicate translation unit or source artifact".into());
            }
            if let Some(prepared) = &u.prepared {
                if !artifacts.contains_key(prepared.as_str()) {
                    return Err("unknown prepared artifact".into());
                }
            }
            if u.status != "success" && !facts.gaps.iter().any(|g| g.unit == u.id) {
                return Err(format!("failed unit {} needs an extraction gap", u.id));
            }
        }
        let span = |s: &SourceSpan| -> Result<(), String> {
            if !artifacts.contains_key(s.artifact.as_str()) || s.end <= s.start || s.end > usize::MAX as u64 {
                return Err("invalid source artifact/span".into());
            }
            Ok(())
        };
        let mut definitions = BTreeMap::new();
        for d in &facts.definitions {
            if d.id.trim().is_empty()
                || d.name.is_empty()
                || !units.contains(d.unit.as_str())
                || definitions.insert(d.id.as_str(), d).is_some()
            {
                return Err("invalid/duplicate compiler declaration identity".into());
            }
            span(&d.span)?;
            if let Some(name) = &d.name_span {
                span(name)?;
                if name.artifact != d.span.artifact || name.start < d.span.start || name.end > d.span.end {
                    return Err("declaration name span differs from compiler buffer".into());
                }
            }
            if !matches!(d.linkage.as_str(), "internal" | "external") {
                return Err("unsupported declaration linkage".into());
            }
        }
        let mut ids = BTreeSet::new();
        for c in &facts.calls {
            if c.id.is_empty() || !ids.insert(&c.id) || !units.contains(c.unit.as_str()) {
                return Err("invalid/duplicate call identity".into());
            }
            let declaration = definitions
                .get(c.declaration.as_str())
                .ok_or("call references missing declaration")?;
            let caller = definitions
                .get(c.caller.as_str())
                .ok_or("call references missing caller")?;
            if !caller.has_body || caller.unit != c.unit || declaration.unit != c.unit || declaration.name != c.callee {
                return Err("call/caller/declaration identities conflict".into());
            }
            span(&c.span)?;
            if let Some(name) = &c.callee_span {
                span(name)?;
                if name.artifact != c.span.artifact || name.start < c.span.start || name.end > c.span.end {
                    return Err("call designator span differs from compiler buffer".into());
                }
            }
            for arg in &c.arguments {
                span(&arg.span)?;
            }
        }
        for r in &facts.address_references {
            if !units.contains(r.unit.as_str()) || !definitions.contains_key(r.declaration.as_str()) {
                return Err("address reference identity missing".into());
            }
            span(&r.span)?;
        }
        for g in &facts.gaps {
            if !units.contains(g.unit.as_str()) || g.reason.trim().is_empty() {
                return Err("invalid extraction gap".into());
            }
        }
        // Bind imported records to their content-addressed extracted payloads.
        // A report edited independently of its fact artifacts cannot verify.
        for i in &facts.intrinsics {
            let effect = match i.name.as_str() {
                "__builtin_trap" => "trap",
                "__builtin_debugtrap" => "debug-trap",
                "__builtin_unreachable" => "unreachable",
                _ => "unsupported",
            };
            if i.id.is_empty()
                || !i.name.starts_with("__builtin_")
                || !units.contains(i.unit.as_str())
                || !definitions
                    .get(i.caller.as_str())
                    .is_some_and(|d| d.unit == i.unit && d.has_body)
                || !ids.insert(&i.id)
                || i.effect != effect
            {
                return Err("invalid compiler intrinsic identity/effect".into());
            }
            span(&i.span)?;
            for argument in &i.arguments {
                span(&argument.span)?;
            }
        }
        for observation in &facts.compiler_storage {
            let unit = observation["unit"].as_str().ok_or("compiler storage unit missing")?;
            let ir = format!("{unit}:llvm-ir");
            if !units.contains(unit)
                || observation["irArtifact"] != ir
                || !artifacts.contains_key(ir.as_str())
                || observation["name"].as_str().is_none_or(str::is_empty)
                || !observation["allocations"].is_array()
                || !observation["lifetimes"].is_array()
            {
                return Err("compiler storage identity/IR artifact missing".into());
            }
            for record in observation["allocations"]
                .as_array()
                .unwrap()
                .iter()
                .chain(observation["lifetimes"].as_array().unwrap())
            {
                if record["line"].as_u64().is_none_or(|n| n == 0)
                    || record["instruction"].as_str().is_none_or(str::is_empty)
                {
                    return Err("compiler storage instruction location missing".into());
                }
            }
        }
        for unit in &facts.units {
            if unit.status != "success" {
                continue;
            }
            let mut payload = json!({
                "definitions": facts.definitions.iter().filter(|d| d.unit == unit.id).collect::<Vec<_>>(),
                "calls": facts.calls.iter().filter(|c| c.unit == unit.id).collect::<Vec<_>>(),
                "addressReferences": facts.address_references.iter().filter(|r| r.unit == unit.id).collect::<Vec<_>>(),
                "gaps": facts.gaps.iter().filter(|g| g.unit == unit.id).collect::<Vec<_>>(),
            });
            if artifacts.contains_key(format!("{}:llvm-ir", unit.id).as_str()) {
                payload["compilerStorage"] = json!(
                    facts
                        .compiler_storage
                        .iter()
                        .filter(|f| f["unit"] == unit.id)
                        .collect::<Vec<_>>()
                );
            }
            let intrinsics: Vec<_> = facts.intrinsics.iter().filter(|i| i.unit == unit.id).collect();
            if !intrinsics.is_empty() {
                payload["intrinsics"] = json!(intrinsics);
            }
            let id = format!("{}:facts", unit.id);
            let artifact = artifacts
                .get(id.as_str())
                .ok_or("successful unit needs its extracted fact artifact")?;
            if sha256(&serde_json::to_vec(&payload).map_err(|e| e.to_string())?) != artifact.sha256 {
                return Err(format!("extracted records differ from fact artifact {id}"));
            }
        }
        Ok(facts)
    }

    pub fn audit(&self) -> ContractReport {
        self.audit_selected(&BTreeMap::new())
    }

    /// Explicit linked selections override available-body discovery. The
    /// workspace validates the native/compiler/link correspondence separately.
    pub fn audit_selected(&self, selected: &BTreeMap<String, String>) -> ContractReport {
        let declarations: BTreeMap<_, _> = self.definitions.iter().map(|d| (&d.id, d)).collect();
        let mut findings = vec![];
        for call in &self.calls {
            let declared = declarations[&call.declaration];
            let caller = declarations[&call.caller];
            // Internal linkage is TU-scoped. Same-TU bodies take precedence.
            let local: Vec<_> = self
                .definitions
                .iter()
                .filter(|d| d.has_body && d.name == call.callee && d.unit == call.unit)
                .collect();
            let providers: Vec<_> = if let Some(id) = selected.get(&call.id) {
                self.definitions.iter().filter(|d| &d.id == id && d.has_body).collect()
            } else if !local.is_empty() {
                local
            } else if declared.linkage == "internal" {
                vec![]
            } else {
                self.definitions
                    .iter()
                    .filter(|d| d.has_body && d.name == call.callee && d.linkage == "external")
                    .collect()
            };
            let source = &self
                .evidence
                .artifacts
                .iter()
                .find(|a| a.id == call.span.artifact)
                .unwrap()
                .location;
            let mut emit = |kind: ContractKind, provider: Option<&Definition>, reason: String| {
                let kind_text = serde_json::to_string(&kind).unwrap();
                let finding_id = sha256(format!("{}:{kind_text}:{reason}", call.id).as_bytes());
                let details = BTreeMap::from([
                    ("findingId".into(), json!(finding_id)),
                    ("callSiteId".into(), json!(call.id)),
                    ("reason".into(), json!(reason)),
                    ("unit".into(), json!(call.unit)),
                    ("sourceSpan".into(), json!(call.span)),
                    ("callerId".into(), json!(call.caller)),
                    ("declarationId".into(), json!(call.declaration)),
                    ("providerId".into(), json!(provider.map(|d| &d.id))),
                    ("providerReturnType".into(), json!(provider.map(|d| &d.return_type))),
                    ("providerLocalParameters".into(), json!(provider.map(|d| &d.parameters))),
                    ("actualCallShapes".into(), json!(call.arguments)),
                    ("resultType".into(), json!(call.result_type)),
                    ("identityState".into(), json!("unverified")),
                ]);
                findings.push(CallContract {
                    caller: caller.name.clone(),
                    callee: call.callee.clone(),
                    source: source.clone(),
                    kind,
                    definition_abi: provider.map(Definition::signature).unwrap_or_else(|| Signature {
                        return_type: "<unresolved provider>".into(),
                        parameter_types: vec![],
                        variadic: false,
                    }),
                    declared_caller_abi: declared.signature(),
                    actual_call_counts: vec![call.arguments.len()],
                    all_results_discarded: !call.result_consumed,
                    source_sha256: self
                        .evidence
                        .artifacts
                        .iter()
                        .find(|a| a.id == call.span.artifact)
                        .map(|a| a.sha256.clone()),
                    original_sha256: None,
                    old_style: declared.old_style,
                    details,
                });
            };
            if providers.is_empty() {
                emit(
                    ContractKind::MissingDefinition,
                    None,
                    "declarations are not definition evidence".into(),
                );
                continue;
            }
            if providers.len() != 1 {
                emit(
                    ContractKind::AmbiguousDefinition,
                    None,
                    format!(
                        "{} candidate bodies; explicit provider ownership required",
                        providers.len()
                    ),
                );
                continue;
            }
            let provider = providers[0];
            if call.arguments.len() < provider.abi_parameters.len()
                || (call.arguments.len() != provider.abi_parameters.len() && !provider.variadic)
            {
                emit(
                    ContractKind::ArgumentCount,
                    Some(provider),
                    format!(
                        "supplied {}, definition requires {}",
                        call.arguments.len(),
                        provider.abi_parameters.len()
                    ),
                );
            }
            if provider.return_type.category == "void" && call.result_consumed {
                emit(
                    ContractKind::VoidResult,
                    Some(provider),
                    "caller consumes a void result".into(),
                );
            }
            let unsupported = self.compiler.profile != "c32-scalar"
                || provider.variadic
                || !provider.return_type.supported(true)
                || provider.abi_parameters.iter().any(|t| !t.supported(false))
                || call.arguments.iter().any(|a| !a.promoted.supported(false));
            if unsupported {
                emit(ContractKind::UnsupportedContract, Some(provider), "profile supports only reviewed <=32-bit integers/pointers; varargs, aggregates and floating point require another profile".into());
            }
            for (i, (actual, formal)) in call.arguments.iter().zip(&provider.abi_parameters).enumerate() {
                if actual.promoted.supported(false) && formal.supported(false) && !actual.promoted.same_wire(formal) {
                    emit(
                        ContractKind::ArgumentType,
                        Some(provider),
                        format!("argument {i}: {} -> {}", actual.promoted.canonical, formal.canonical),
                    );
                }
            }
            if call.result_consumed
                && provider.return_type.category != "void"
                && provider.return_type.supported(true)
                && call.result_type.supported(true)
                && !provider.return_type.same_wire(&call.result_type)
            {
                emit(
                    ContractKind::ReturnType,
                    Some(provider),
                    format!(
                        "result {} -> {}",
                        provider.return_type.canonical, call.result_type.canonical
                    ),
                );
            }
        }
        let report = ContractReport {
            contracts: findings,
            scope: "Every extracted direct call; identity verification and native policy eligibility are separate"
                .into(),
            cache_misses: self
                .gaps
                .iter()
                .map(|g| format!("{} [{}]: {}", g.unit, g.stage, g.reason))
                .collect(),
            contract_count: None,
            distinct_callees: None,
            metadata: BTreeMap::from([
                ("schemaVersion".into(), json!(1)),
                ("compiler".into(), json!(self.compiler)),
                ("evidence".into(), json!(self.evidence)),
                ("identityChecks".into(), json!(self.evidence.verify(None).unwrap())),
                ("extractionGaps".into(), json!(self.gaps)),
                ("callsExamined".into(), json!(self.calls.len())),
                ("units".into(), json!(self.units)),
                ("addressReferences".into(), json!(self.address_references)),
                ("compilerStorage".into(), json!(self.compiler_storage)),
                (
                    "intrinsics".into(),
                    json!({"compiler":self.compiler,"observations":self.intrinsics}),
                ),
            ]),
        };
        report.filtered(None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> CompilerFacts {
        CompilerFacts::parse(include_bytes!("../../../tests/fixtures/contracts/facts.json")).unwrap()
    }
    #[test]
    fn all_calls_have_stable_findings_and_prototypes_are_not_bodies() {
        let facts = fixture();
        let report = facts.audit();
        assert_eq!(facts.calls.len(), 11);
        assert_eq!(
            report
                .contracts
                .iter()
                .filter(|f| f.kind == ContractKind::VoidResult)
                .count(),
            2
        );
        assert!(
            report
                .contracts
                .iter()
                .any(|f| f.callee == "count" && f.kind == ContractKind::ArgumentCount)
        );
        assert!(
            report
                .contracts
                .iter()
                .any(|f| f.callee == "missing" && f.kind == ContractKind::MissingDefinition)
        );
        assert!(
            report
                .contracts
                .iter()
                .any(|f| f.callee == "narrow" && f.kind == ContractKind::ArgumentType)
        );
        assert!(
            report
                .contracts
                .iter()
                .any(|f| f.callee == "narrow" && f.kind == ContractKind::ReturnType)
        );
        let ids: BTreeSet<_> = report
            .contracts
            .iter()
            .map(|f| &f.details["findingId"])
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(ids.len(), report.contracts.len());
        assert_eq!(
            serde_json::to_value(facts.audit()).unwrap(),
            serde_json::to_value(report.clone()).unwrap()
        );
        assert!(ContractReport::parse(&serde_json::to_vec(&report).unwrap()).is_ok());
        assert_eq!(facts.address_references.len(), 1);
        let pointers = facts.definitions.iter().find(|d| d.name == "pointers").unwrap();
        assert_eq!(pointers.return_type.category, "pointer");
        assert_eq!(pointers.return_type.canonical, "int **");
        assert_eq!(pointers.return_type.spelling, "pointer *");
    }
    #[test]
    fn modified_records_cannot_reuse_the_extracted_artifact_identity() {
        let mut facts = fixture();
        facts.calls[0].arguments.clear();
        let error = CompilerFacts::parse(&serde_json::to_vec(&facts).unwrap()).unwrap_err();
        assert!(error.contains("records differ"));
    }
    #[test]
    fn grouped_declarations_keep_per_symbol_identity() {
        let facts = fixture();
        let decls: Vec<_> = facts
            .definitions
            .iter()
            .filter(|d| d.unit == "caller" && matches!(d.name.as_str(), "a" | "b" | "c"))
            .collect();
        assert_eq!(decls.len(), 3);
        assert_eq!(decls[0].declaration_group, decls[1].declaration_group);
        assert_eq!(decls[1].declaration_group, decls[2].declaration_group);
        assert_ne!(decls[0].id, decls[1].id);
        assert!(decls[0].span.end < decls[1].span.end);
        assert!(decls.iter().all(|d| !d.has_body));
    }

    #[test]
    fn raw_void_typedefs_and_variadic_minimum_counts_round_trip() {
        let mut facts = fixture();
        facts
            .definitions
            .iter_mut()
            .find(|d| d.name == "no_result" && d.has_body)
            .unwrap()
            .return_type
            .spelling = "no_value".into();
        let provider = facts
            .definitions
            .iter_mut()
            .find(|d| d.name == "count" && d.has_body)
            .unwrap();
        provider.variadic = true;
        let report = facts.audit();
        assert!(
            report
                .contracts
                .iter()
                .any(|f| f.callee == "count" && f.kind == ContractKind::ArgumentCount)
        );
        assert!(ContractReport::parse(&serde_json::to_vec(&report).unwrap()).is_ok());
        let missing = report
            .contracts
            .iter()
            .find(|f| f.kind == ContractKind::MissingDefinition)
            .unwrap();
        assert_eq!(missing.definition_abi.return_type, "<unresolved provider>");
    }

    #[test]
    fn knr_incoming_promotion_is_not_a_local_short_wire_contract() {
        let mut facts = fixture();
        let provider = facts.definitions.iter().find(|d| d.name == "old_style").unwrap();
        assert_eq!(provider.parameters[0].bits, Some(16));
        assert_eq!(provider.abi_parameters[0].bits, Some(32));
        let mut declaration = provider.clone();
        declaration.id = "caller:decl:old-style".into();
        declaration.unit = "caller".into();
        declaration.has_body = false;
        declaration.parameters.clear();
        declaration.abi_parameters.clear();
        declaration.old_style = true;
        let mut call = facts.calls.iter().find(|c| c.callee == "count").unwrap().clone();
        call.id = "caller:call:old-style".into();
        call.callee = "old_style".into();
        call.declaration = declaration.id.clone();
        facts.definitions.push(declaration);
        facts.calls.push(call);
        assert!(facts.audit().filtered(None, Some("old_style")).contracts.is_empty());
    }
    #[test]
    fn ambiguous_providers_and_unsupported_profiles_remain_findings() {
        let mut facts = fixture();
        let mut extra = facts
            .definitions
            .iter()
            .find(|d| d.name == "count" && d.has_body)
            .unwrap()
            .clone();
        extra.id.push_str(":other");
        extra.unit = "other-unit".into();
        facts.definitions.push(extra);
        facts.compiler.profile = "unsupported".into();
        let report = facts.audit();
        assert!(
            report
                .contracts
                .iter()
                .any(|f| f.callee == "count" && f.kind == ContractKind::AmbiguousDefinition)
        );
        assert!(
            report
                .contracts
                .iter()
                .any(|f| f.kind == ContractKind::UnsupportedContract)
        );
    }
}
