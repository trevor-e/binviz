//! Read-only follow-ups derived from workspace findings. These grant no acceptance.
use crate::{evidence::IdentityState, workspace::WorkspaceReport};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextActions {
    pub state: String,
    pub summary: String,
    pub total_actions: usize,
    pub omitted_actions: usize,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub kind: String,
    pub subject: String,
    pub caller: Option<String>,
    pub unit: Option<String>,
    pub reason: String,
    /// JSON pointers into the complete workspace report, never detached verdicts.
    pub evidence: Vec<String>,
    /// Argument arrays, with <manifest> replaced by the caller's workspace path.
    pub cli: Vec<String>,
    pub mcp: FollowUp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowUp {
    pub tool: String,
    pub arguments: Value,
}

impl NextActions {
    pub fn for_caller(&self, caller: &str) -> Self {
        let actions: Vec<_> = self
            .actions
            .iter()
            .filter(|a| a.caller.as_deref().is_none_or(|c| c == caller))
            .cloned()
            .collect();
        Self {
            state: self.state.clone(),
            summary: format!(
                "{} Global prerequisites and selected caller {caller} follow.",
                self.summary
            ),
            total_actions: actions.len(),
            omitted_actions: 0,
            actions,
        }
    }

    /// Presentation limit only; complete findings remain in workspace_report.
    pub fn limited(mut self, limit: usize) -> Self {
        self.actions.truncate(limit);
        self.omitted_actions = self.total_actions.saturating_sub(self.actions.len());
        self
    }
}

fn action(
    kind: &str,
    subject: &str,
    reason: String,
    evidence: Vec<String>,
    view: &str,
    tool: &str,
    arguments: Value,
) -> Action {
    Action {
        kind: kind.into(),
        subject: subject.into(),
        caller: None,
        unit: None,
        reason,
        evidence,
        cli: vec!["workspace".into(), "<manifest>".into(), view.into(), "--json".into()],
        mcp: FollowUp {
            tool: tool.into(),
            arguments,
        },
    }
}

pub fn next_actions(report: &WorkspaceReport) -> NextActions {
    let mut actions = vec![];
    for (i, check) in report.inventory.identity_checks.iter().enumerate() {
        if check.state == IdentityState::Verified {
            continue;
        }
        actions.push(action("verify-identity", &check.id,
            format!("{:?}: {}. Supply current artifact bytes or regenerate the affected producer before preparing a candidate.", check.state, check.reasons.join("; ")),
            vec![format!("/inventory/identityChecks/{i}")], "--preflight", "workspace_preflight", json!({})));
    }
    for (i, collision) in report
        .inventory
        .collisions
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.resolved)
    {
        actions.push(action(
            "resolve-ownership",
            &format!("{} / {}", collision.left, collision.right),
            collision.reason.clone(),
            vec![format!("/inventory/collisions/{i}")],
            "--inventory",
            "unit_inventory",
            json!({}),
        ));
    }
    for (i, unit) in report.inventory.units.iter().enumerate() {
        if unit.state != IdentityState::Verified
            || unit
                .functions
                .iter()
                .any(|f| !f.retired && f.state != IdentityState::Verified)
        {
            let mut a = action(
                "inspect-unit",
                &unit.unit.id,
                "Physical unit or function identity remains unresolved; inspect extents, aliases and native bytes."
                    .into(),
                vec![format!("/inventory/units/{i}")],
                "--inventory",
                "unit_inventory",
                json!({}),
            );
            a.unit = Some(unit.unit.id.clone());
            actions.push(a);
        }
    }
    // Reuse the existing provider-dependency ranking, then retain caller-only gaps.
    let mut callers: Vec<_> = report.callers.iter().enumerate().collect();
    callers.sort_by_key(|(i, caller)| {
        (
            report
                .blockers
                .iter()
                .position(|b| b.callers.contains(&caller.id))
                .unwrap_or(usize::MAX),
            *i,
        )
    });
    for (i, caller) in callers {
        let mut add =
            |kind: &str, subject: &str, reason: String, pointers: Vec<String>, tool: &str, arguments: Value| {
                let mut a = action(kind, subject, reason, pointers, "--caller", tool, arguments);
                a.caller = Some(caller.id.clone());
                a.unit = Some(caller.unit.clone());
                a.cli.insert(3, caller.id.clone());
                if kind == "review-adapters" {
                    a.cli = vec!["adapters".into(), "<manifest>".into()];
                    for policy in a.mcp.arguments["policies"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        a.cli.extend(["--policy".into(), policy.into()]);
                    }
                }
                actions.push(a);
            };
        if caller.rejected {
            add(
                "inspect-build",
                &caller.id,
                "A current producer rejected this caller. Inspect its diagnostics before rebuilding.".into(),
                vec![format!("/callers/{i}/builds")],
                "caller_package",
                json!({"caller":caller.id}),
            );
        }
        if !caller.extraction_gaps.is_empty() {
            add(
                "inspect-extraction",
                &caller.id,
                "Compiler extraction omitted facts. Resolve these gaps before claiming call coverage.".into(),
                vec![format!("/callers/{i}/extractionGaps")],
                "caller_package",
                json!({"caller":caller.id}),
            );
        }
        for (j, call) in caller
            .calls
            .iter()
            .enumerate()
            .filter(|(_, c)| !matches!(c.state.as_str(), "clear" | "verified-and-applied"))
        {
            let pointer = format!("/callers/{i}/calls/{j}");
            let (kind, reason, tool, args) = if call.binding.is_none() {
                ("inspect-correspondence", "No unique native/compiler/linked correspondence is available. Inspect the exact caller and establish the physical provider.".into(), "caller_package", json!({"caller":caller.id}))
            } else if call.state == "eligible-not-applied" {
                ("review-adapters", "A scoped policy is eligible but has not been applied. Inspect the edit plan, then build and verify a separate candidate.".into(), "plan_adapters", json!({"policies":call.policies.iter().filter(|p|p.state=="eligible").map(|p|&p.policy).collect::<Vec<_>>()}))
            } else {
                (
                    "inspect-call",
                    format!(
                        "{}: {}. Inspect the complete caller, policy decisions and paired native witnesses.",
                        call.state,
                        call.reasons.join("; ")
                    ),
                    "caller_package",
                    json!({"caller":caller.id}),
                )
            };
            add(kind, &call.call.id, reason, vec![pointer], tool, args);
        }
    }
    for (field, values, flag, tool) in [
        ("layoutChecks", &report.layout_checks, "--inventory", "workspace_report"),
        (
            "promotionPlans",
            &report.promotion_plans,
            "--promotions",
            "promotion_plan",
        ),
        ("proofClosures", &report.proof_closures, "--closures", "proof_closures"),
        (
            "matchingPublications",
            &report.matching_publications,
            "--publications",
            "matching_publications",
        ),
        (
            "readabilityBatches",
            &report.readability_batches,
            "--readability",
            "readability_batches",
        ),
        ("adoption", &report.adoption, "--adoption", "adoption_evidence"),
    ] {
        for (i, value) in values.iter().enumerate() {
            // These reports have different success states. Only their explicit refusals
            // route here; no guessed success or merged acceptance scope is introduced.
            if value["state"] != "refused" {
                continue;
            }
            let subject = value["id"].as_str().unwrap_or(field);
            let mut a = action(
                "inspect-refusal",
                subject,
                format!(
                    "{field} refused the requested scope. Inspect its reasons and dependencies; this recommendation does not apply or publish it."
                ),
                vec![format!("/{field}/{i}")],
                flag,
                tool,
                json!({}),
            );
            if field == "layoutChecks" {
                a.cli = vec!["workspace".into(), "<manifest>".into(), "--json".into()];
            }
            actions.push(a);
        }
    }
    for (i, proof) in report
        .callee_certificates
        .iter()
        .enumerate()
        .filter(|(_, p)| p.state != "verified")
    {
        actions.push(action(
            "inspect-callee",
            &proof.id,
            proof.reasons.join("; "),
            vec![format!("/calleeCertificates/{i}")],
            "--callees",
            "callee_certificates",
            json!({}),
        ));
    }
    if report.storage["closureComplete"] == false {
        actions.push(action(
            "inspect-storage",
            "storage",
            "Storage/access closure is incomplete. Inspect object extents and unresolved accesses.".into(),
            vec!["/storage".into()],
            "--storage",
            "storage_evidence",
            json!({}),
        ));
    }
    let state = if actions.is_empty() {
        "review"
    } else {
        "needs-attention"
    };
    let summary = if actions.is_empty() {
        "No outstanding findings in this workspace view. Review the requested acceptance scope; this does not establish whole-program equivalence or grant matching credit.".into()
    } else {
        format!(
            "{} follow-ups; {} rejected callers; {} unresolved caller packages. Identity and ownership prerequisites come first. Follow-ups only inspect or plan work.",
            actions.len(),
            report.rejected_callers,
            report.unresolved_callers
        )
    };
    NextActions {
        state: state.into(),
        summary,
        total_actions: actions.len(),
        omitted_actions: 0,
        actions,
    }
}
