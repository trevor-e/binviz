//! Complete caller work packages, selected linked providers and scoped decisions.
//! All conclusions are computed from supplied bytes; imported status labels have
//! no authority. Native/C correspondences are explicit reviewed artifacts.
use crate::{
    compilerfacts::{CompilerFacts, DirectCall, SourceSpan},
    contracts::{CallContract, ContractKind, ContractReport},
    evidence::{EvidenceManifest, IdentityState, hex},
    inventory::{Inventory, InventoryReport, OwnedFunction, verified},
    linkevidence::{self, LayoutCertificate, ModuleReport},
    mipsaudit::{self, AuditPolicy, AuditReport, ExactExtent, Outcome},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Workspace {
    pub format: String,
    pub schema_version: u32,
    pub evidence: EvidenceManifest,
    pub inventory: Inventory,
    pub compiler_facts: CompilerFacts,
    #[serde(default)]
    pub bindings: Vec<CallBinding>,
    #[serde(default)]
    pub policies: Vec<ScopedPolicy>,
    #[serde(default)]
    pub applications: Vec<PolicyApplication>,
    #[serde(default)]
    pub builds: Vec<BuildRecord>,
    #[serde(default)]
    pub layouts: Vec<LayoutCertificate>,
    #[serde(default)]
    pub storage: Vec<crate::storage::StorageDescription>,
    #[serde(default)]
    pub promotions: Vec<crate::promotion::Request>,
    #[serde(default)]
    pub callee_certificates: Vec<crate::calleeproof::Request>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_closures: Vec<crate::proofclosure::Request>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matching_publications: Vec<crate::publication::Request>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub readability_batches: Vec<crate::readability::Request>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adoption_requests: Vec<crate::adoption::Request>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallBinding {
    pub call: String,
    pub caller: String,
    pub provider: String,
    pub definition: String,
    pub provider_kind: String,
    pub original_pc: String,
    pub original_word: String,
    pub delay_word: String,
    pub module: String,
    pub object: String,
    pub caller_export: String,
    pub provider_export: String,
    pub linked_call_offset: String,
    pub correspondence_artifact: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_call_owner: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopedPolicy {
    pub id: String,
    pub kind: String,
    pub allowed_callers: Vec<String>,
    pub allowed_sites: Vec<String>,
    pub provider: String,
    pub definition: String,
    pub expected_counts: BTreeMap<String, usize>,
    pub registers: Vec<u8>,
    pub dependencies: Vec<String>,
    pub review_artifact: String,
    pub guard: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callee_certificates: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub closure_certificates: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicyApplication {
    pub policy: String,
    pub call: String,
    pub stage: String,
    pub transformed_artifact: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildRecord {
    pub unit: String,
    pub stage: String,
    pub phase: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyDecision {
    pub policy: String,
    pub state: String,
    pub reasons: Vec<String>,
    pub equivalent_to: Option<String>,
    pub covered_findings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedAudit {
    pub register: u8,
    pub provider: Option<AuditReport>,
    pub continuation: Option<AuditReport>,
    pub reasons: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallPackage {
    pub call: DirectCall,
    pub binding: Option<CallBinding>,
    pub state: String,
    pub reasons: Vec<String>,
    pub findings: Vec<CallContract>,
    pub policies: Vec<PolicyDecision>,
    pub audits: Vec<PairedAudit>,
    pub original_instructions: Vec<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallerPackage {
    pub id: String,
    pub name: String,
    pub unit: String,
    pub source_span: SourceSpan,
    pub calls: Vec<CallPackage>,
    pub address_references: Vec<Value>,
    pub extraction_gaps: Vec<Value>,
    pub intrinsics: Vec<Value>,
    pub builds: Vec<Value>,
    pub rejected: bool,
}
pub use crate::queue::ContractBlocker as Blocker;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReport {
    pub format: String,
    pub schema_version: u32,
    pub inventory: InventoryReport,
    pub callers: Vec<CallerPackage>,
    pub modules: BTreeMap<String, ModuleReport>,
    pub layout_checks: Vec<Value>,
    pub blockers: Vec<Blocker>,
    pub raw_observations: usize,
    pub rejected_callers: usize,
    pub unresolved_callers: usize,
    pub contract_report: ContractReport,
    pub storage: Value,
    pub promotion_plans: Vec<Value>,
    pub callee_certificates: Vec<crate::calleeproof::Report>,
    pub proof_closures: Vec<Value>,
    pub matching_publications: Vec<Value>,
    pub readability_batches: Vec<Value>,
    pub call_coverage: Value,
    pub adoption: Vec<Value>,
}

fn artifact_equal<T: Serialize>(files: &BTreeMap<String, Vec<u8>>, id: &str, record: &T) -> bool {
    files
        .get(id)
        .and_then(|b| serde_json::from_slice::<Value>(b).ok())
        .is_some_and(|v| v == serde_json::to_value(record).unwrap())
}
impl Workspace {
    /// Cheap identity-only gate. No compiler, analysis runner or provider executes.
    pub fn preflight(&self, files: &BTreeMap<String, Vec<u8>>) -> Result<Value, String> {
        self.validate()?;
        let checks = self.evidence.verify(Some(files))?;
        let mut problems = vec![];
        for check in checks.iter().filter(|c| c.state != IdentityState::Verified) {
            let artifact = self.evidence.artifacts.iter().find(|a| a.id == check.id);
            let owners: Vec<_> = self
                .evidence
                .stages
                .iter()
                .filter(|s| s.parents.contains(&check.id) || s.outputs.contains(&check.id) || s.recipe == check.id)
                .map(|s| &s.id)
                .collect();
            problems.push(json!({"id":check.id,"state":check.state,"expectedSha256":artifact.map(|a|&a.sha256),"currentSha256":files.get(&check.id).map(|b|crate::evidence::sha256(b)),"owners":owners,"reasons":check.reasons}));
        }
        for r in &self.adoption_requests {
            for (id, pin) in &r.dependencies {
                if files.get(id).is_none_or(|b| crate::evidence::sha256(b) != *pin) {
                    problems.push(json!({"id":id,"owner":r.id,"expectedSha256":pin,"currentSha256":files.get(id).map(|b|crate::evidence::sha256(b)),"reason":"reviewed adoption dependency drift"}));
                }
            }
        }
        for r in &self.proof_closures {
            for (id, pin) in &r.dependencies {
                if files.get(id).is_none_or(|b| crate::evidence::sha256(b) != *pin) {
                    problems.push(json!({"id":id,"owner":r.id,"expectedSha256":pin,"currentSha256":files.get(id).map(|b|crate::evidence::sha256(b)),"reason":"reviewed proof dependency drift"}));
                }
            }
        }
        Ok(
            json!({"format":"binviz-preflight","schemaVersion":1,"state":if problems.is_empty(){"verified"}else{"refused"},"checked":checks.len(),"mismatches":problems,"checks":checks,"artifacts":self.evidence.artifacts}),
        )
    }
    pub fn matches_loaded_unit(&self, binary: &crate::Binary, unit: &str) -> Result<bool, String> {
        self.validate()?;
        let digest = crate::evidence::sha256(binary.data());
        let matches = |u: &crate::inventory::Unit| -> bool {
            let base = hex(&u.identity.load_address).unwrap();
            let asset = self
                .evidence
                .artifacts
                .iter()
                .find(|a| a.id == u.identity.asset)
                .unwrap();
            let member = self
                .evidence
                .artifacts
                .iter()
                .find(|a| a.id == u.member_artifact)
                .unwrap();
            (asset.sha256 == digest && binary.address_to_offset(base) == hex(&u.identity.member_offset).ok())
                || (member.sha256 == digest && binary.address_to_offset(base) == Some(0))
        };
        let matching: Vec<_> = self.inventory.units.iter().filter(|u| matches(u)).collect();
        Ok(matching.len() == 1 && matching[0].identity.id == unit)
    }
    /// Build adapters emit the same artifact/stage graph. Existing compiler and
    /// native identities cannot be silently replaced by a later build report.
    pub fn merge_build_batch(&mut self, bytes: &[u8]) -> Result<(), String> {
        let report: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if report["format"] != "binviz-build-batch" || report["schemaVersion"] != 1 {
            return Err("requires binviz-build-batch schemaVersion 1".into());
        }
        let evidence: EvidenceManifest =
            serde_json::from_value(report["evidence"].clone()).map_err(|e| e.to_string())?;
        evidence.validate()?;
        let mut candidate = self.clone();
        for a in evidence.artifacts {
            if let Some(existing) = candidate.evidence.artifacts.iter().find(|x| x.id == a.id) {
                if existing.sha256 != a.sha256 {
                    return Err(format!(
                        "build attempts to replace existing identity {}; regenerate the workspace",
                        a.id
                    ));
                }
            } else {
                candidate.evidence.artifacts.push(a);
            }
        }
        for s in evidence.stages {
            if let Some(existing) = candidate.evidence.stages.iter().find(|x| x.id == s.id) {
                if serde_json::to_value(existing).ok() != serde_json::to_value(&s).ok() {
                    return Err("build stage id conflicts with retained evidence".into());
                }
            } else {
                candidate.evidence.stages.push(s);
            }
        }
        let builds: Vec<BuildRecord> = serde_json::from_value(report["builds"].clone()).map_err(|e| e.to_string())?;
        for b in builds {
            if !candidate
                .builds
                .iter()
                .any(|r| r.unit == b.unit && r.stage == b.stage && r.phase == b.phase)
            {
                candidate.builds.push(b);
            }
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let w: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        w.validate()?;
        Ok(w)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format != "binviz-workspace" || self.schema_version != 1 {
            return Err("requires binviz-workspace schemaVersion 1".into());
        }
        self.inventory.validate(&self.evidence)?;
        CompilerFacts::parse(&serde_json::to_vec(&self.compiler_facts).map_err(|e| e.to_string())?)?;
        let artifacts: BTreeMap<_, _> = self.evidence.artifacts.iter().map(|a| (&a.id, a)).collect();
        for a in &self.compiler_facts.evidence.artifacts {
            if artifacts
                .get(&a.id)
                .is_none_or(|b| b.sha256 != a.sha256 || b.location != a.location)
            {
                return Err(format!(
                    "compiler artifact {} is absent or inconsistent with workspace",
                    a.id
                ));
            }
        }
        for s in &self.compiler_facts.evidence.stages {
            if !self
                .evidence
                .stages
                .iter()
                .any(|p| serde_json::to_value(p).ok() == serde_json::to_value(s).ok())
            {
                return Err("workspace must retain compiler stage lineage".into());
            }
        }
        let functions: BTreeMap<_, _> = self
            .inventory
            .units
            .iter()
            .flat_map(|u| &u.functions)
            .map(|f| (&f.id, f))
            .collect();
        let definitions: BTreeMap<_, _> = self.compiler_facts.definitions.iter().map(|d| (&d.id, d)).collect();
        let calls: BTreeMap<_, _> = self.compiler_facts.calls.iter().map(|c| (&c.id, c)).collect();
        let mut bindings = BTreeSet::new();
        for b in &self.bindings {
            if !calls.contains_key(&b.call)
                || !bindings.insert(&b.call)
                || !functions.contains_key(&b.caller)
                || !functions.contains_key(&b.provider)
                || !definitions.contains_key(&b.definition)
            {
                return Err("unknown or duplicate physical/compiler call binding".into());
            }
            if !matches!(
                b.provider_kind.as_str(),
                "reconstructed-c" | "retained-assembly" | "compiler-runtime" | "external-host" | "generated-bridge"
            ) {
                return Err("unknown linked provider kind".into());
            }
            for id in [&b.module, &b.object, &b.correspondence_artifact] {
                if !artifacts.contains_key(id) {
                    return Err("call binding references unknown artifact".into());
                }
            }
            if let Some(owner) = &b.physical_call_owner {
                if functions[&b.caller].owner.as_ref() != Some(owner) || !functions.contains_key(owner) {
                    return Err("shared-tail call binding must select its actual physical owner".into());
                }
            }
            for v in [&b.original_pc, &b.original_word, &b.delay_word, &b.linked_call_offset] {
                hex(v)?;
            }
            if hex(&b.original_pc)? > u32::MAX as u64 - 8
                || hex(&b.original_pc)? & 3 != 0
                || hex(&b.original_word)? > u32::MAX as u64
                || hex(&b.delay_word)? > u32::MAX as u64
            {
                return Err("invalid exact PS1 call/slot identity".into());
            }
        }
        let mut ids = BTreeSet::new();
        for p in &self.policies {
            if p.id.is_empty()
                || !ids.insert(&p.id)
                || p.allowed_callers.is_empty()
                || p.allowed_sites.is_empty()
                || !functions.contains_key(&p.provider)
                || !definitions.contains_key(&p.definition)
                || !artifacts.contains_key(&p.review_artifact)
                || p.dependencies.is_empty()
                || p.dependencies.iter().any(|id| !artifacts.contains_key(id))
                || p.allowed_callers.iter().any(|id| !functions.contains_key(id))
                || p.allowed_sites.iter().any(|id| !calls.contains_key(id))
            {
                return Err("invalid scoped policy identity/scope/dependencies".into());
            }
            if p.allowed_sites.iter().any(|id| !p.expected_counts.contains_key(id))
                || p.expected_counts.keys().any(|id| !p.allowed_sites.contains(id))
            {
                return Err("policy needs exact call counts for each allowed site".into());
            }
            if p.registers.iter().any(|r| !(1..32).contains(r)) {
                return Err("invalid policy register".into());
            }
        }
        for a in &self.applications {
            if !ids.contains(&a.policy)
                || !calls.contains_key(&a.call)
                || !artifacts.contains_key(&a.transformed_artifact)
                || !self.evidence.stages.iter().any(|s| s.id == a.stage)
            {
                return Err("invalid applied policy stage/artifact".into());
            }
        }
        for b in &self.builds {
            if !self.compiler_facts.units.iter().any(|u| u.id == b.unit)
                || !self.evidence.stages.iter().any(|s| s.id == b.stage)
                || !matches!(b.phase.as_str(), "prepare" | "compile" | "link" | "validate")
            {
                return Err("invalid build unit/stage/phase".into());
            }
        }
        crate::storage::validate(self)?;
        crate::promotion::validate(&self.evidence, &self.promotions)?;
        crate::calleeproof::validate(self)?;
        if self
            .policies
            .iter()
            .flat_map(|p| &p.callee_certificates)
            .any(|id| !self.callee_certificates.iter().any(|c| &c.id == id))
        {
            return Err("policy references unknown callee certificate".into());
        }
        Ok(())
    }

    pub fn analyze(&self, supplied: Option<&BTreeMap<String, Vec<u8>>>) -> Result<WorkspaceReport, String> {
        self.validate()?;
        let files = supplied
            .map(|f| self.inventory.materialize(&self.evidence, f))
            .transpose()?
            .unwrap_or_default();
        let inventory = self.inventory.analyze(&self.evidence, supplied.map(|_| &files))?;
        let checks = &inventory.identity_checks;
        let callee_certificates = crate::calleeproof::inspect(self, &inventory, &files)?;
        let functions: BTreeMap<_, _> = inventory
            .units
            .iter()
            .flat_map(|u| &u.functions)
            .map(|f| (&f.function.id, f))
            .collect();
        let mut modules = BTreeMap::new();
        let mut module_errors = BTreeMap::new();
        for id in self
            .bindings
            .iter()
            .flat_map(|b| [&b.module, &b.object])
            .chain(self.layouts.iter().map(|l| &l.module))
            .chain(
                self.evidence
                    .artifacts
                    .iter()
                    .filter(|a| matches!(a.role.as_str(), "linked-module" | "wasm-module" | "wasm-object"))
                    .map(|a| &a.id),
            )
        {
            if modules.contains_key(id) || module_errors.contains_key(id) {
                continue;
            }
            if let Some(bytes) = files.get(id) {
                match linkevidence::inspect(bytes) {
                    Ok(m) => {
                        modules.insert(id.clone(), m);
                    }
                    Err(e) => {
                        module_errors.insert(id.clone(), e);
                    }
                }
            }
        }
        let mut layout_checks = vec![];
        for layout in &self.layouts {
            let reasons = if !verified(checks, &layout.module) {
                vec!["module identity is not current".into()]
            } else if let Some(actual) = modules.get(&layout.module) {
                layout.verify(actual)?
            } else {
                vec!["linked module bytes unavailable/unsupported".into()]
            };
            layout_checks.push(json!({"module":layout.module,"state":if reasons.is_empty(){"verified"}else{"refused"},"reasons":reasons,"certificate":layout}));
        }
        let proof_closures = crate::proofclosure::inspect(self, &files, &modules)?;
        let selected = self
            .bindings
            .iter()
            .map(|b| (b.call.clone(), b.definition.clone()))
            .collect();
        let contract_report = self.compiler_facts.audit_selected(&selected);
        let mut callers = vec![];
        for caller in self.compiler_facts.definitions.iter().filter(|d| d.has_body) {
            let mut calls = vec![];
            for call in self.compiler_facts.calls.iter().filter(|c| c.caller == caller.id) {
                let binding = self.bindings.iter().find(|b| b.call == call.id);
                let mut reasons = vec![];
                let mut instructions = vec![];
                if let Some(b) = binding {
                    let native_caller = functions[b.physical_call_owner.as_ref().unwrap_or(&b.caller)];
                    let provider = functions[&b.provider];
                    if [native_caller, provider].iter().any(|f| {
                        inventory
                            .units
                            .iter()
                            .find(|u| u.unit.id == f.function.identity.unit)
                            .is_none_or(|u| u.architecture != "ps1-mipsel")
                    }) {
                        reasons.push("native call correspondence requires explicit ps1-mipsel units".into());
                    }
                    for f in [native_caller, provider] {
                        if f.state != IdentityState::Verified
                            || f.retired
                            || f.function.identity.role == "shared-fragment"
                        {
                            reasons.push(format!(
                                "{} is not a current independent exact provider: {}",
                                f.function.id,
                                f.reasons.join("; ")
                            ));
                        }
                    }
                    if !verified(checks, &b.correspondence_artifact)
                        || !artifact_equal(&files, &b.correspondence_artifact, b)
                    {
                        reasons.push("reviewed native/compiler correspondence bytes are missing, stale or differ from the binding".into());
                    }
                    for id in [&b.module, &b.object] {
                        if !verified(checks, id) {
                            reasons.push(format!("{id} linked input is not current"));
                        }
                    }
                    let definition = self
                        .compiler_facts
                        .definitions
                        .iter()
                        .find(|d| d.id == b.definition)
                        .unwrap();
                    for id in [format!("{}:extract", call.unit), format!("{}:extract", definition.unit)] {
                        if !verified(checks, &id) {
                            reasons.push(format!("{id} compiler lineage is not current"));
                        }
                    }
                    if !definition.has_body {
                        reasons.push("selected provider is a prototype, not a definition".into());
                    }
                    let mut expected_parameters = definition
                        .abi_parameters
                        .iter()
                        .map(|t| {
                            if matches!(t.category.as_str(), "integer" | "pointer") && t.bits.is_some_and(|b| b <= 32) {
                                "i32"
                            } else {
                                "unsupported"
                            }
                        })
                        .collect::<Vec<_>>();
                    if definition.variadic {
                        expected_parameters.push("i32");
                    }
                    let expected_results = if definition.return_type.category == "void" {
                        vec![]
                    } else if matches!(definition.return_type.category.as_str(), "integer" | "pointer")
                        && definition.return_type.bits.is_some_and(|b| b <= 32)
                    {
                        vec!["i32"]
                    } else {
                        vec!["unsupported"]
                    };
                    if modules.get(&b.object).is_none_or(|m| {
                        !m.problems.is_empty()
                            || !m.functions.iter().any(|f| {
                                !f.imported
                                    && f.names.contains(&definition.name)
                                    && f.parameters == expected_parameters
                                    && f.results == expected_results
                            })
                    }) {
                        reasons.push("actual selected object lacks the named provider body/signature or has unsupported instructions".into());
                    }
                    let prepared = self
                        .compiler_facts
                        .units
                        .iter()
                        .find(|u| u.id == definition.unit)
                        .and_then(|u| u.prepared.as_ref());
                    if !self.evidence.stages.iter().any(|s| {
                        s.result == "success"
                            && s.outputs.contains(&b.object)
                            && prepared.is_some_and(|p| s.parents.contains(p))
                    }) {
                        reasons.push(
                            "selected object lacks a compile stage tied to the true definition's prepared source"
                                .into(),
                        );
                    }
                    if !self.evidence.stages.iter().any(|s| {
                        s.result == "success" && s.outputs.contains(&b.module) && s.parents.contains(&b.object)
                    }) {
                        reasons.push("actual linked module lacks a stage selecting the recorded object".into());
                    }
                    for check in layout_checks.iter().filter(|c| c["module"] == b.module) {
                        if check["state"] != "verified" {
                            reasons.push("actual linked module layout certificate was refused".into());
                        }
                    }
                    let pc = hex(&b.original_pc)?;
                    match native_word(native_caller, &files, pc).zip(native_word(native_caller, &files, pc + 4)) {
                        Some((word, slot)) => {
                            instructions = vec![
                                json!({"unit":native_caller.function.identity.unit,"pc":b.original_pc,"word":format!("0x{word:08x}"),"delaySlot":false}),
                                json!({"unit":native_caller.function.identity.unit,"pc":format!("0x{:x}",pc+4),"word":format!("0x{slot:08x}"),"delaySlot":true}),
                            ];
                            let target = ((pc + 4) & 0xf0000000) | u64::from((word & 0x03ffffff) << 2);
                            if word >> 26 != 3
                                || hex(&b.original_word)? != u64::from(word)
                                || hex(&b.delay_word)? != u64::from(slot)
                                || target != hex(&provider.function.identity.entry)?
                            {
                                reasons.push("original call/slot/target differs; only explicit direct JAL correspondences are supported".into());
                            }
                        }
                        None => reasons.push("original call or delay slot is outside the exact caller extent".into()),
                    }
                    if let Some(m) = modules.get(&b.module) {
                        if !m.problems.is_empty() {
                            reasons.extend(m.problems.clone());
                        }
                        let target = m.functions.iter().find(|f| f.exports.contains(&b.provider_export));
                        let linked_caller = m.functions.iter().find(|f| f.exports.contains(&b.caller_export));
                        if let Some(target) = target {
                            let mut expected_parameters = definition
                                .abi_parameters
                                .iter()
                                .map(|t| {
                                    if matches!(t.category.as_str(), "integer" | "pointer")
                                        && t.bits.is_some_and(|b| b <= 32)
                                    {
                                        "i32"
                                    } else {
                                        "unsupported"
                                    }
                                })
                                .collect::<Vec<_>>();
                            if definition.variadic {
                                expected_parameters.push("i32");
                            }
                            let expected_results = if definition.return_type.category == "void" {
                                vec![]
                            } else if matches!(definition.return_type.category.as_str(), "integer" | "pointer")
                                && definition.return_type.bits.is_some_and(|b| b <= 32)
                            {
                                vec!["i32"]
                            } else {
                                vec!["unsupported"]
                            };
                            if target.parameters != expected_parameters
                                || target.results != expected_results
                                || (target.imported && b.provider_kind != "external-host")
                            {
                                reasons.push(
                                    "actual linked provider signature/import differs from the selected true definition"
                                        .into(),
                                );
                            }
                            if linked_caller.is_none_or(|f| {
                                !f.calls
                                    .iter()
                                    .any(|c| c.offset == b.linked_call_offset && c.target == target.index)
                            }) {
                                reasons.push(
                                    "actual linked caller instruction does not select the configured provider index"
                                        .into(),
                                );
                            }
                        } else {
                            reasons.push("selected provider is absent from actual linked module exports".into());
                        }
                    } else {
                        reasons.push(format!(
                            "linked module unavailable: {}",
                            module_errors.get(&b.module).cloned().unwrap_or_default()
                        ));
                    }
                } else {
                    reasons.push("native/compiler/linked-provider correspondence has not been established".into());
                }
                let findings: Vec<_> = contract_report
                    .contracts
                    .iter()
                    .filter(|f| f.details.get("callSiteId").and_then(Value::as_str) == Some(&call.id))
                    .cloned()
                    .collect();
                let mut decisions = vec![];
                let mut audits = vec![];
                for policy in self
                    .policies
                    .iter()
                    .filter(|p| binding.is_some_and(|b| p.provider == b.provider) || p.allowed_sites.contains(&call.id))
                {
                    let mut refused = reasons.clone();
                    if let Some(b) = binding {
                        if !policy.allowed_callers.contains(&b.caller) {
                            refused.push("caller is not in allowedCallers".into());
                        }
                        if !policy.allowed_sites.contains(&call.id) {
                            refused.push("call site is not in allowedSites".into());
                        }
                        if policy.provider != b.provider || policy.definition != b.definition {
                            refused.push("provider/definition scope differs".into());
                        }
                        if policy.expected_counts.get(&call.id) != Some(&call.arguments.len()) {
                            refused.push("actual supplied call count differs from the reviewed count".into());
                        }
                        if !verified(checks, &policy.review_artifact)
                            || !artifact_equal(&files, &policy.review_artifact, policy)
                        {
                            refused.push("policy review artifact is missing, stale or differs".into());
                        }
                        for id in &policy.dependencies {
                            if !verified(checks, id) {
                                refused.push(format!("policy dependency {id} is not verified"));
                            }
                        }
                        for id in &policy.callee_certificates {
                            let proof = self.callee_certificates.iter().find(|c| &c.id == id).unwrap();
                            if !policy.dependencies.contains(&proof.review_artifact)
                                || proof.dependencies.iter().any(|d| !policy.dependencies.contains(d))
                            {
                                refused.push(format!("policy omits callee certificate {id} dependency closure"));
                            }
                        }
                        let required: [Option<&String>; 6] = [
                            functions[b.physical_call_owner.as_ref().unwrap_or(&b.caller)]
                                .function
                                .analysis_artifact
                                .as_ref(),
                            functions[&b.provider].function.analysis_artifact.as_ref(),
                            Some(&call.span.artifact),
                            Some(&b.module),
                            Some(&b.object),
                            Some(&b.correspondence_artifact),
                        ];
                        if required.iter().flatten().any(|id| !policy.dependencies.contains(id)) {
                            refused
                                .push("policy omits a caller/provider/source/module/correspondence dependency".into());
                        }
                        if policy.guard.is_some() {
                            refused.push(
                                "runtime guard has a declared unsupported frontier; no unconditional allowance".into(),
                            );
                        }
                        if refused.is_empty() && policy.kind == "extra-word-nonuse" {
                            let definition = self
                                .compiler_facts
                                .definitions
                                .iter()
                                .find(|d| d.id == b.definition)
                                .unwrap();
                            if definition.variadic
                                || call.arguments.len() <= definition.abi_parameters.len()
                                || call.arguments.len() > 4
                                || self.compiler_facts.compiler.profile != "c32-scalar"
                                || definition.abi_parameters.iter().any(|t| {
                                    !matches!(t.category.as_str(), "integer" | "pointer")
                                        || t.bits.is_none_or(|n| n > 32)
                                })
                                || call.arguments.iter().any(|a| {
                                    !matches!(a.promoted.category.as_str(), "integer" | "pointer")
                                        || a.promoted.bits.is_none_or(|n| n > 32)
                                })
                            {
                                refused.push("extra-word policy requires reviewed scalar O32 register arguments; stack/varargs need another profile".into());
                            }
                            let expected: Vec<_> = (definition.abi_parameters.len()..call.arguments.len())
                                .map(|n| (n + 4) as u8)
                                .collect();
                            if policy.registers != expected {
                                refused.push("policy word mapping differs from supplied trailing O32 registers".into());
                            }
                            if refused.is_empty() {
                                for &register in &policy.registers {
                                    let pair = pair_audit(
                                        functions[b.physical_call_owner.as_ref().unwrap_or(&b.caller)],
                                        functions[&b.provider],
                                        &files,
                                        hex(&b.original_pc)?,
                                        register,
                                        crate::calleeproof::select(
                                            self,
                                            &callee_certificates,
                                            &policy.callee_certificates,
                                            register,
                                        ),
                                    );
                                    if !pair.reasons.is_empty()
                                        || pair.provider.as_ref().is_none_or(|a| a.outcome != Outcome::Dead)
                                        || pair.continuation.as_ref().is_none_or(|a| a.outcome != Outcome::Dead)
                                    {
                                        refused.push(format!(
                                            "incoming/post-call register {register} remains consumed or unresolved"
                                        ));
                                    }
                                    audits.push(pair);
                                }
                            }
                        } else if policy.kind == "discarded-result" {
                            if call.result_consumed || policy.registers != [2] {
                                refused.push("discarded-result requires an actually discarded compiler result and independent V0 mapping".into());
                            }
                            if refused.is_empty() {
                                let mut pair = pair_audit(
                                    functions[b.physical_call_owner.as_ref().unwrap_or(&b.caller)],
                                    functions[&b.provider],
                                    &files,
                                    hex(&b.original_pc)?,
                                    2,
                                    crate::calleeproof::select(
                                        self,
                                        &callee_certificates,
                                        &policy.callee_certificates,
                                        2,
                                    ),
                                );
                                // The provider is supposed to produce V0 here. Only the
                                // independent continuation audit establishes non-use.
                                pair.provider = None;
                                if !pair.reasons.is_empty()
                                    || pair.continuation.as_ref().is_none_or(|a| a.outcome != Outcome::Dead)
                                {
                                    refused.push("post-call V0 is consumed or unresolved".into());
                                }
                                audits.push(pair);
                            }
                        } else if matches!(policy.kind.as_str(), "scalar-varargs" | "missing-inputs") {
                            let covered = policy.closure_certificates.iter().any(|id| {
                                proof_closures.iter().any(|p| {
                                    p["id"] == *id
                                        && p["state"] == "verified"
                                        && p["request"]["claim"]["kind"] == policy.kind
                                        && p["request"]["claim"]["call"] == call.id
                                })
                            });
                            if !covered {
                                refused.push(
                                    "representation needs a current, exact call-scoped lowering certificate".into(),
                                );
                            }
                            for id in &policy.closure_certificates {
                                if let Some(p) = self.proof_closures.iter().find(|p| p.id == *id) {
                                    if !policy.dependencies.contains(&p.review_artifact)
                                        || p.dependencies.keys().any(|id| !policy.dependencies.contains(id))
                                    {
                                        refused.push("policy omits lowering certificate dependency closure".into());
                                    }
                                } else {
                                    refused.push("unknown lowering certificate".into());
                                }
                            }
                        } else if policy.kind == "direct-binding" {
                            let d = self
                                .compiler_facts
                                .definitions
                                .iter()
                                .find(|d| d.id == b.definition)
                                .unwrap();
                            if !policy.registers.is_empty()
                                || d.variadic
                                || call.arguments.len() != d.abi_parameters.len()
                                || d.return_type != call.result_type
                                || call
                                    .arguments
                                    .iter()
                                    .zip(&d.abi_parameters)
                                    .any(|(a, t)| a.promoted.category != t.category || a.promoted.bits != t.bits)
                            {
                                refused.push(
                                    "direct binding cannot introduce argument/result representation exceptions".into(),
                                );
                            }
                            let target = modules
                                .get(&b.module)
                                .and_then(|m| m.functions.iter().find(|f| f.exports.contains(&b.provider_export)));
                            let selected = policy.closure_certificates.iter().any(|id| {
                                proof_closures.iter().any(|p| {
                                    p["id"] == *id
                                        && p["state"] == "verified"
                                        && p["request"]["claim"]["kind"] == "service-chain"
                                        && p["request"]["claim"]["module"] == b.module
                                        && target.is_some_and(|t| p["request"]["claim"]["root"] == t.index)
                                })
                            });
                            if !selected {
                                refused.push("direct binding requires the selected body's current service-chain execution certificate".into());
                            }
                            for id in &policy.closure_certificates {
                                if let Some(p) = self.proof_closures.iter().find(|p| p.id == *id) {
                                    if !policy.dependencies.contains(&p.review_artifact)
                                        || p.dependencies.keys().any(|k| !policy.dependencies.contains(k))
                                    {
                                        refused.push("direct binding omits certificate dependency closure".into());
                                    }
                                }
                            }
                        } else if policy.kind != "extra-word-nonuse" {
                            refused.push(
                                "policy representation requires an explicit reviewed lowering/closure certificate"
                                    .into(),
                            );
                        }
                    }
                    let mut state = if refused.is_empty() {
                        "eligible"
                    } else if policy.guard.is_some() {
                        "guarded"
                    } else {
                        "refused"
                    };
                    if refused.is_empty() {
                        for a in self
                            .applications
                            .iter()
                            .filter(|a| a.policy == policy.id && a.call == call.id)
                        {
                            let stage = self.evidence.stages.iter().find(|s| s.id == a.stage).unwrap();
                            let compiled = self.evidence.stages.iter().find(|s| {
                                s.result == "success"
                                    && verified(checks, &s.id)
                                    && s.parents.contains(&a.transformed_artifact)
                                    && self
                                        .builds
                                        .iter()
                                        .any(|r| r.unit == call.unit && r.stage == s.id && r.phase == "compile")
                                    && s.outputs.iter().any(|id| {
                                        files
                                            .get(id)
                                            .and_then(|bytes| linkevidence::inspect(bytes).ok())
                                            .is_some_and(|m| {
                                                m.problems.is_empty()
                                                    && m.functions
                                                        .iter()
                                                        .any(|f| !f.imported && f.names.contains(&caller.name))
                                            })
                                    })
                            });
                            let linked = compiled.is_some_and(|compile| {
                                self.evidence.stages.iter().any(|s| {
                                    s.result == "success"
                                        && verified(checks, &s.id)
                                        && binding.is_some_and(|b| s.outputs.contains(&b.module))
                                        && s.parents.iter().any(|p| compile.outputs.contains(p))
                                        && self
                                            .builds
                                            .iter()
                                            .any(|r| r.unit == call.unit && r.stage == s.id && r.phase == "link")
                                })
                            });
                            if verified(checks, &a.stage)
                                && verified(checks, &a.transformed_artifact)
                                && stage.outputs.contains(&a.transformed_artifact)
                                && stage.parents.contains(&policy.review_artifact)
                                && stage.parents.contains(&call.span.artifact)
                                && linked
                            {
                                state = "applied";
                            } else {
                                refused.push(
                                    "application edit, successful compile/link, inputs or output is not current".into(),
                                );
                                state = "refused";
                            }
                        }
                    }
                    let covered_findings = if matches!(state, "eligible" | "applied") {
                        findings
                            .iter()
                            .filter(|f| {
                                matches!(
                                    (&*policy.kind, &f.kind),
                                    ("extra-word-nonuse", ContractKind::ArgumentCount)
                                        | ("missing-inputs", ContractKind::ArgumentCount)
                                        | ("scalar-varargs", ContractKind::UnsupportedContract)
                                        | ("discarded-result", ContractKind::ReturnType)
                                        | ("discarded-result", ContractKind::VoidResult)
                                )
                            })
                            .filter_map(|f| f.details.get("findingId").and_then(Value::as_str).map(str::to_string))
                            .collect()
                    } else {
                        vec![]
                    };
                    decisions.push(PolicyDecision {
                        policy: policy.id.clone(),
                        state: state.into(),
                        reasons: refused,
                        equivalent_to: None,
                        covered_findings,
                    });
                }
                // Equivalent claims do not union scopes. Conflicting eligible
                // kinds/word maps at a site refuse both claims.
                let eligible: Vec<_> = decisions
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| matches!(d.state.as_str(), "eligible" | "applied"))
                    .map(|(i, d)| (i, self.policies.iter().find(|p| p.id == d.policy).unwrap()))
                    .collect();
                for (n, (i, p)) in eligible.iter().enumerate() {
                    for (j, q) in &eligible[n + 1..] {
                        if p.kind == q.kind
                            && p.registers == q.registers
                            && p.provider == q.provider
                            && p.definition == q.definition
                        {
                            decisions[*j].equivalent_to = Some(p.id.clone());
                        } else {
                            for k in [*i, *j] {
                                decisions[k].state = "refused".into();
                                decisions[k].covered_findings.clear();
                                decisions[k]
                                    .reasons
                                    .push("conflicting policy claims at this exact site".into());
                            }
                        }
                    }
                }
                let all_applied = findings.iter().all(|f| {
                    f.details.get("findingId").and_then(Value::as_str).is_some_and(|id| {
                        decisions
                            .iter()
                            .any(|d| d.state == "applied" && d.covered_findings.iter().any(|s| s == id))
                    })
                });
                let state = if !reasons.is_empty() {
                    "unresolved"
                } else if findings.is_empty() {
                    "clear"
                } else if all_applied {
                    "verified-and-applied"
                } else if decisions.iter().any(|d| d.state == "eligible") {
                    "eligible-not-applied"
                } else {
                    "unresolved"
                };
                calls.push(CallPackage {
                    call: call.clone(),
                    binding: binding.cloned(),
                    state: state.into(),
                    reasons,
                    findings,
                    policies: decisions,
                    audits,
                    original_instructions: instructions,
                });
            }
            if calls.is_empty()
                && !self.compiler_facts.gaps.iter().any(|g| g.unit == caller.unit)
                && !self
                    .compiler_facts
                    .address_references
                    .iter()
                    .any(|r| r.caller == caller.id)
                && !self.storage.iter().any(|s| s.caller == caller.id)
                && !self.compiler_facts.intrinsics.iter().any(|i| i.caller == caller.id)
            {
                continue;
            }
            let builds:Vec<_>=self.builds.iter().filter(|b|b.unit==caller.unit).map(|b|{let stage=self.evidence.stages.iter().find(|s|s.id==b.stage).unwrap();json!({"phase":b.phase,"stage":stage,"identityState":checks.iter().find(|c|c.id==b.stage).map(|c|&c.state)})}).collect();
            let rejected = self.builds.iter().filter(|b| b.unit == caller.unit).any(|b| {
                self.evidence.stages.iter().find(|s| s.id == b.stage).is_some_and(|s| {
                    s.result != "success"
                        && s.parents
                            .iter()
                            .chain(std::iter::once(&s.recipe))
                            .all(|p| verified(checks, p))
                        && s.outputs.iter().all(|o| {
                            checks
                                .iter()
                                .any(|c| c.id == *o && c.content_state == Some(IdentityState::Verified))
                        })
                })
            });
            callers.push(CallerPackage {
                id: caller.id.clone(),
                name: caller.name.clone(),
                unit: caller.unit.clone(),
                source_span: caller.span.clone(),
                calls,
                address_references: self
                    .compiler_facts
                    .address_references
                    .iter()
                    .filter(|r| r.caller == caller.id)
                    .map(|r| json!(r))
                    .collect(),
                extraction_gaps: self
                    .compiler_facts
                    .gaps
                    .iter()
                    .filter(|g| g.unit == caller.unit)
                    .map(|g| json!(g))
                    .collect(),
                builds,
                rejected,
                intrinsics: self
                    .compiler_facts
                    .intrinsics
                    .iter()
                    .filter(|i| i.caller == caller.id)
                    .map(|i| json!({"intrinsic":i,"compiler":self.compiler_facts.compiler}))
                    .collect(),
            });
        }
        let blockers = crate::queue::contract_blockers(&callers);
        let promotion_plans = crate::promotion::plans(&self.evidence, checks, &self.promotions)?;
        let call_coverage = json!({"sourceCallSites":self.compiler_facts.calls.len(),"boundSourceCallSites":self.bindings.len(),"uniquePhysicalNativeCalls":self.bindings.iter().map(|b|(&functions[&b.caller].function.identity.unit,&b.original_pc)).collect::<BTreeSet<_>>().len(),"sharedTailBindings":self.bindings.iter().filter(|b|b.physical_call_owner.is_some()).count()});
        let mut report = WorkspaceReport {
            format: "binviz-workspace-report".into(),
            schema_version: 1,
            raw_observations: contract_report.contracts.len(),
            rejected_callers: callers.iter().filter(|c| c.rejected).count(),
            unresolved_callers: callers
                .iter()
                .filter(|c| {
                    c.calls
                        .iter()
                        .any(|c| !matches!(c.state.as_str(), "clear" | "verified-and-applied"))
                        || !c.extraction_gaps.is_empty()
                })
                .count(),
            inventory,
            callers,
            modules,
            layout_checks,
            blockers,
            contract_report,
            storage: Value::Null,
            promotion_plans,
            callee_certificates,
            proof_closures,
            matching_publications: crate::publication::plans(self, &files),
            readability_batches: crate::readability::plans(self, &files),
            call_coverage,
            adoption: vec![],
        };
        report.storage = crate::storage::describe(self, &files, &report)?;
        report.adoption = crate::adoption::inspect(self, &report, &files)?;
        Ok(report)
    }
}

fn native_word(f: &OwnedFunction, files: &BTreeMap<String, Vec<u8>>, pc: u64) -> Option<u32> {
    let span = f.function.identity.analysis_extent.as_ref()?;
    let bytes = files.get(f.function.analysis_artifact.as_ref()?)?;
    let at = usize::try_from(pc.checked_sub(hex(&span.start).ok()?)?).ok()?;
    let word = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(word.try_into().ok()?))
}
fn pair_audit(
    caller: &OwnedFunction,
    provider: &OwnedFunction,
    files: &BTreeMap<String, Vec<u8>>,
    pc: u64,
    register: u8,
    callees: Result<BTreeMap<u32, mipsaudit::CalleeSummary>, String>,
) -> PairedAudit {
    let mut reasons = vec![];
    let callees = callees.map_err(|e| reasons.push(e)).unwrap_or_default();
    // A pending delayed load cannot be represented by a fresh audit entry.
    // Refuse that boundary instead of silently treating its destination as live.
    for (at, label) in [(pc.checked_sub(4), "before call"), (Some(pc + 4), "delay slot")] {
        if at
            .and_then(|at| native_word(caller, files, at))
            .is_some_and(|w| matches!(w >> 26, 0x20..=0x26) && ((w >> 16) & 31) == u32::from(register))
        {
            reasons.push(format!(
                "pending load to register {register} {label} needs an explicit load-delay entry contract"
            ));
        }
    }
    let run = |f: &OwnedFunction, entry: u64, return_use: mipsaudit::ReturnUse| -> Result<AuditReport, String> {
        let span = f
            .function
            .identity
            .analysis_extent
            .as_ref()
            .ok_or("exact extent missing")?;
        let start = u32::try_from(hex(&span.start)?).map_err(|_| "PS1 extent exceeds 32 bits")?;
        let bytes = files
            .get(f.function.analysis_artifact.as_ref().ok_or("native artifact missing")?)
            .ok_or("native bytes missing")?;
        if bytes.len() % 4 != 0 {
            return Err("native extent is not word aligned".into());
        }
        let words: Vec<_> = bytes
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        mipsaudit::audit(
            ExactExtent { start, words: &words },
            u32::try_from(entry).map_err(|_| "entry exceeds 32 bits")?,
            register,
            &AuditPolicy {
                return_use,
                callees: callees.clone(),
                ..AuditPolicy::default()
            },
        )
        .map_err(|e| e.to_string())
    };
    // No ABI clobbers or discarded-return assumptions are added.
    let b = run(caller, pc + 8, mipsaudit::ReturnUse::Unresolved);
    // A preserved extra word may cross the return only when the independent
    // caller continuation proves it dead. Keep the discarded endpoint label;
    // this result must never be exported as a provider kill summary.
    let return_use = if (4..8).contains(&register) && b.as_ref().is_ok_and(|a| a.outcome == Outcome::Dead) {
        mipsaudit::ReturnUse::Discarded
    } else {
        mipsaudit::ReturnUse::Unresolved
    };
    let a = run(provider, hex(&provider.function.identity.entry).unwrap(), return_use);
    let provider = a.map_err(|e| reasons.push(format!("provider: {e}"))).ok();
    let continuation = b.map_err(|e| reasons.push(format!("continuation: {e}"))).ok();
    PairedAudit {
        register,
        provider,
        continuation,
        reasons,
    }
}
