//! Inspect compiler-side call-contract findings without treating them as proofs.
//!
//! These are imported observations about reconstructed source, complementary to
//! the original binary's call graph. No source hash or native behavior is verified
//! by importing a report, and a missing finding never means a call is safe.
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    pub return_type: String,
    pub parameter_types: Vec<String>,
    #[serde(default)]
    pub variadic: bool,
}

impl Signature {
    fn display(&self) -> String {
        let mut parameters = self.parameter_types.clone();
        if self.variadic {
            parameters.push("...".into());
        }
        format!(
            "{} ({})",
            self.return_type,
            if parameters.is_empty() {
                "void".into()
            } else {
                parameters.join(", ")
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContractKind {
    ArgumentCount,
    VoidResult,
    ArgumentType,
    ReturnType,
    MissingDefinition,
    AmbiguousDefinition,
    UnsupportedContract,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallContract {
    pub caller: String,
    pub callee: String,
    pub source: String,
    pub kind: ContractKind,
    pub definition_abi: Signature,
    pub declared_caller_abi: Signature,
    pub actual_call_counts: Vec<usize>,
    pub all_results_discarded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_sha256: Option<String>,
    #[serde(default)]
    pub old_style: bool,
    /// Keep additional adapter evidence, such as actual typed call shapes or locations.
    #[serde(flatten)]
    pub details: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractReport {
    pub contracts: Vec<CallContract>,
    pub scope: String,
    #[serde(default)]
    pub cache_misses: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distinct_callees: Option<usize>,
    #[serde(flatten)]
    pub metadata: std::collections::BTreeMap<String, serde_json::Value>,
}

impl ContractReport {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if value.get("format").and_then(serde_json::Value::as_str) == Some("binviz-compiler-facts") {
            return crate::compilerfacts::CompilerFacts::parse(bytes).map(|facts| facts.audit());
        }
        let report: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if report.contract_count.is_some_and(|n| n != report.contracts.len()) {
            return Err("contractCount differs from the findings array".into());
        }
        let distinct: std::collections::BTreeSet<_> = report.contracts.iter().map(|r| &r.callee).collect();
        if report.distinct_callees.is_some_and(|n| n != distinct.len()) {
            return Err("distinctCallees differs from the findings array".into());
        }
        for row in &report.contracts {
            if row.caller.is_empty()
                || row.callee.is_empty()
                || row.source.is_empty()
                || row.actual_call_counts.is_empty()
            {
                return Err("finding needs caller, callee, source and actualCallCounts".into());
            }
            let consistent = match row.kind {
                ContractKind::ArgumentCount => row.actual_call_counts.iter().any(|&n| {
                    if row.definition_abi.variadic {
                        n < row.definition_abi.parameter_types.len()
                    } else {
                        n != row.definition_abi.parameter_types.len()
                    }
                }),
                ContractKind::VoidResult => {
                    (row.definition_abi.return_type == "void"
                        || row
                            .details
                            .get("providerReturnType")
                            .and_then(|t| t.get("category"))
                            .and_then(serde_json::Value::as_str)
                            == Some("void"))
                        && !row.all_results_discarded
                }
                _ => {
                    row.details
                        .get("findingId")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|id| !id.is_empty())
                        && row
                            .details
                            .get("reason")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|s| !s.is_empty())
                }
            };
            if !consistent {
                return Err(format!(
                    "inconsistent reported mismatch: {} -> {}",
                    row.caller, row.callee
                ));
            }
        }
        Ok(report)
    }

    /// Exact names, so overlay names and differently cased symbols stay distinct.
    pub fn filtered(mut self, caller: Option<&str>, callee: Option<&str>) -> Self {
        self.contracts
            .retain(|r| caller.is_none_or(|n| r.caller == n) && callee.is_none_or(|n| r.callee == n));
        self.contracts
            .sort_by(|a, b| (&a.callee, &a.caller, &a.source).cmp(&(&b.callee, &b.caller, &b.source)));
        self.contract_count = Some(self.contracts.len());
        self.distinct_callees = Some(
            self.contracts
                .iter()
                .map(|r| &r.callee)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
        );
        self
    }

    pub fn describe(&self, limit: usize) -> String {
        let mut out = format!(
            "{} reported call-contract findings; {} uncached callers.\nImported observations; source hashes and native behavior are not verified here.\nScope: {}\n",
            self.contracts.len(),
            self.cache_misses.len(),
            self.scope
        );
        for row in self.contracts.iter().take(limit) {
            let _ = writeln!(
                out,
                "\n{} -> {}\n  source: {}\n  caller declaration: {}{}\n  actual supplied counts: {:?}\n  definition: {}",
                row.caller,
                row.callee,
                row.source,
                if row.old_style {
                    format!("{} ()", row.declared_caller_abi.return_type)
                } else {
                    row.declared_caller_abi.display()
                },
                if row.old_style {
                    " [unspecified parameter list]"
                } else {
                    ""
                },
                row.actual_call_counts,
                row.definition_abi.display()
            );
            let reason = match row.kind {
                ContractKind::ArgumentCount => "supplied argument count differs from the definition",
                ContractKind::VoidResult => "caller consumes a value from a void definition",
                ContractKind::ArgumentType => "supplied argument representation differs from definition",
                ContractKind::ReturnType => "consumed result representation differs from definition",
                ContractKind::MissingDefinition => "no actual definition available",
                ContractKind::AmbiguousDefinition => "multiple actual definitions require ownership resolution",
                ContractKind::UnsupportedContract => "contract cannot be lowered by the selected profile",
            };
            let _ = writeln!(out, "  finding: {reason}");
            if let Some(reason) = row.details.get("reason").and_then(serde_json::Value::as_str) {
                let _ = writeln!(out, "  reason: {reason}");
            }
            if let Some(id) = row.details.get("findingId").and_then(serde_json::Value::as_str) {
                let _ = writeln!(out, "  finding ID: {id}");
            }
            if let Some(span) = row.details.get("sourceSpan") {
                let _ = writeln!(out, "  compiler source span: {span}");
            }
            if let Some(shapes) = row.details.get("actualCallShapes") {
                let _ = writeln!(out, "  reported typed call shapes: {shapes}");
            }
            if let Some(h) = &row.source_sha256 {
                let _ = writeln!(out, "  reported source SHA256: {h}");
            }
            if let Some(h) = &row.original_sha256 {
                let _ = writeln!(out, "  reported native extent SHA256: {h}");
            }
        }
        if self.contracts.len() > limit {
            let _ = writeln!(
                out,
                "\n{} more findings; use --top N, --caller NAME, --callee NAME or --json.",
                self.contracts.len() - limit
            );
        }
        if !self.cache_misses.is_empty() {
            let _ = writeln!(out, "Uncached callers: {}", self.cache_misses.join(", "));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn specimen() -> serde_json::Value {
        serde_json::json!({"scope":"cached actual compiler observations only","cacheMisses":["uncached"],"contractCount":2,"distinctCallees":2,"contracts":[
            {"caller":"consumer","callee":"find","source":"src/a.c","kind":"void-result","definitionAbi":{"returnType":"void","parameterTypes":["int"]},"declaredCallerAbi":{"returnType":"int","parameterTypes":["int"]},"actualCallCounts":[1],"allResultsDiscarded":false},
            {"caller":"forward","callee":"Find","source":"src/b.c","kind":"argument-count","definitionAbi":{"returnType":"int","parameterTypes":["int","int"]},"declaredCallerAbi":{"returnType":"int","parameterTypes":[]},"actualCallCounts":[1],"allResultsDiscarded":true,"oldStyle":true}
        ]})
    }
    #[test]
    fn imported_evidence_is_visible_and_case_sensitive() {
        let mut input = specimen();
        input["contracts"][0]["actualCallShapes"] = serde_json::json!([["int"]]);
        input["adapterVersion"] = serde_json::json!("fixture-v1");
        let report = ContractReport::parse(&serde_json::to_vec(&input).unwrap())
            .unwrap()
            .filtered(None, Some("find"));
        assert_eq!(report.contracts.len(), 1);
        let text = report.describe(10);
        assert!(text.contains("consumer -> find"));
        assert!(text.contains("caller consumes a value"));
        assert!(text.contains("not verified"));
        assert_eq!(report.cache_misses, vec!["uncached"]);
        assert_eq!(
            report.contracts[0].details["actualCallShapes"],
            serde_json::json!([["int"]])
        );
        assert_eq!(report.metadata["adapterVersion"], serde_json::json!("fixture-v1"));
        assert!(ContractReport::parse(&serde_json::to_vec(&report).unwrap()).is_ok());
    }
    #[test]
    fn rejects_contradictory_counts_and_semantics() {
        for (field, value) in [
            ("contractCount", serde_json::json!(3)),
            ("distinctCallees", serde_json::json!(1)),
        ] {
            let mut v = specimen();
            v[field] = value;
            assert!(ContractReport::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        let mut v = specimen();
        v["contracts"][0]["allResultsDiscarded"] = serde_json::json!(true);
        assert!(ContractReport::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        let mut v = specimen();
        v["contracts"][1]["actualCallCounts"] = serde_json::json!([2]);
        assert!(ContractReport::parse(&serde_json::to_vec(&v).unwrap()).is_err());
    }
}
