pub mod agent;

use std::collections::HashSet;

use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoverageBaseline {
    #[serde(default)]
    pub baseline_entries: Vec<String>,
    #[serde(default)]
    pub baseline_privileged_accounts: Vec<String>,
    #[serde(default)]
    pub approved_privileged_accounts: Vec<String>,
    #[serde(default)]
    pub baseline_exposed_bindings: Vec<String>,
    #[serde(default)]
    pub expected_processes: Vec<String>,
}

impl CoverageBaseline {
    pub fn is_empty(&self) -> bool {
        self.baseline_entries.is_empty()
            && self.baseline_privileged_accounts.is_empty()
            && self.approved_privileged_accounts.is_empty()
            && self.baseline_exposed_bindings.is_empty()
            && self.expected_processes.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    pub thought: String,
    pub tool_call: Option<ToolCall>,
    pub observation: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl FindingSeverity {
    pub fn token(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidencePointer {
    pub turn: Option<usize>,
    pub tool: Option<String>,
    pub field: String,
}

fn serialize_confidence<S: Serializer>(value: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    let rounded = (*value * 100.0).round() / 100.0;
    serializer.serialize_f32(rounded)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Finding {
    pub title: String,
    pub severity: FindingSeverity,
    #[serde(serialize_with = "serialize_confidence")]
    pub confidence: f32,
    pub evidence_pointer: EvidencePointer,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveFallbackDecision {
    pub policy: String,
    pub reason: String,
    pub reason_code: String,
    pub live_error: String,
    pub fallback_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunTimingMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_token_latency_ms: Option<u64>,
    pub total_run_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveFailureReasonCount {
    pub reason_code: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveRunMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_token_latency_ms: Option<u64>,
    pub total_run_duration_ms: u64,
    pub live_attempt_duration_ms: u64,
    pub live_attempt_count: usize,
    pub live_success_count: usize,
    pub fallback_count: usize,
    pub live_success_rate: f64,
    pub fallback_rate: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_failure_reasons: Vec<LiveFailureReasonCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_severity: Option<FindingSeverity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_fallback_decision: Option<LiveFallbackDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_timing: Option<RunTimingMetrics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_run_metrics: Option<LiveRunMetrics>,
    pub turns: Vec<AgentTurn>,
    pub final_answer: String,
    #[serde(default)]
    pub findings: Vec<Finding>,
}

pub fn derive_findings(turns: &[AgentTurn], final_answer: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    for (idx, turn) in turns.iter().enumerate() {
        let Some(observation) = turn.observation.as_ref() else {
            continue;
        };

        let tool_name = turn.tool_call.as_ref().map(|call| call.tool.clone());

        if let Some(error) = observation.get("error").and_then(Value::as_str) {
            let tool_label = tool_name.as_deref().unwrap_or("unknown_tool");
            findings.push(Finding {
                title: format!("Tool execution failed for {tool_label}"),
                severity: FindingSeverity::High,
                confidence: 0.95,
                evidence_pointer: EvidencePointer {
                    turn: Some(idx + 1),
                    tool: tool_name.clone(),
                    field: "observation.error".to_string(),
                },
                recommended_action: format!(
                    "Review tool arguments and host access policy, then rerun {tool_label}. Error sample: {error}"
                ),
            });
            continue;
        }

        if let Some(indicator_count) = observation.get("indicator_count").and_then(Value::as_u64) {
            if indicator_count > 0 {
                let severity = if indicator_count >= 4 {
                    FindingSeverity::High
                } else {
                    FindingSeverity::Medium
                };

                findings.push(Finding {
                    title: format!(
                        "Privilege escalation indicators detected ({indicator_count})"
                    ),
                    severity,
                    confidence: confidence_from_count(0.68, indicator_count, 0.06, 0.96),
                    evidence_pointer: EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.indicator_count".to_string(),
                    },
                    recommended_action: "Review potential_vectors and verify whether elevated rights are expected; revoke or constrain unexpected grants.".to_string(),
                });
            }
        }

        if let Some(listener_count) = observation.get("listener_count").and_then(Value::as_u64) {
            if listener_count > 0 {
                let severity = if listener_count >= 25 {
                    FindingSeverity::High
                } else if listener_count >= 8 {
                    FindingSeverity::Medium
                } else {
                    FindingSeverity::Low
                };

                findings.push(Finding {
                    title: format!("Active listening sockets observed ({listener_count})"),
                    severity,
                    confidence: confidence_from_count(0.62, listener_count, 0.02, 0.92),
                    evidence_pointer: EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.listener_count".to_string(),
                    },
                    recommended_action: "Correlate listener PIDs and ports with expected services; investigate unknown listeners and expose only required interfaces.".to_string(),
                });
            }
        }

        if let Some(baseline_version) = observation.get("baseline_version").and_then(Value::as_str)
        {
            let baseline_entries_count = observation
                .get("baseline_entries_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let baseline_privileged_account_count = observation
                .get("baseline_privileged_account_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let baseline_exposed_binding_count = observation
                .get("baseline_exposed_binding_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);

            findings.push(Finding {
                title: format!(
                    "Coverage baseline captured ({baseline_entries_count} persistence entries, {baseline_privileged_account_count} privileged accounts, {baseline_exposed_binding_count} exposed bindings)"
                ),
                severity: FindingSeverity::Info,
                confidence: 0.9,
                evidence_pointer: EvidencePointer {
                    turn: Some(idx + 1),
                    tool: tool_name.clone(),
                    field: "observation.baseline_version".to_string(),
                },
                recommended_action: format!(
                    "Store baseline arrays from this {baseline_version} snapshot and supply them to coverage tools in subsequent runs to detect drift."
                ),
            });
        }

        let actionable_persistence_count = observation
            .get("actionable_suspicious_count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let suspicious_entry_count = observation
            .get("suspicious_entry_count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let persistence_count = if actionable_persistence_count > 0 {
            Some(actionable_persistence_count)
        } else if suspicious_entry_count > 0 {
            Some(suspicious_entry_count)
        } else {
            None
        };

        if let Some(persistence_count) = persistence_count {
            let severity = if persistence_count >= 5 {
                FindingSeverity::High
            } else {
                FindingSeverity::Medium
            };

            let (title, evidence_field) = if actionable_persistence_count > 0 {
                (
                    format!(
                        "Actionable suspicious persistence entries detected ({persistence_count})"
                    ),
                    "observation.actionable_suspicious_count",
                )
            } else {
                (
                    format!("Suspicious persistence entries detected ({persistence_count})"),
                    "observation.suspicious_entry_count",
                )
            };

            findings.push(Finding {
                title,
                severity,
                confidence: confidence_from_count(0.7, persistence_count, 0.05, 0.95),
                evidence_pointer: EvidencePointer {
                    turn: Some(idx + 1),
                    tool: tool_name.clone(),
                    field: evidence_field.to_string(),
                },
                recommended_action: "Review persistence entries for unauthorized startup references, remove unapproved autoruns, and preserve forensic artifacts before cleanup.".to_string(),
            });
        }

        if let Some(baseline_new_count) = observation
            .get("baseline_new_count")
            .and_then(Value::as_u64)
        {
            if baseline_new_count > 0 {
                let severity = if baseline_new_count >= 8 {
                    FindingSeverity::High
                } else {
                    FindingSeverity::Medium
                };

                findings.push(Finding {
                    title: format!(
                        "Persistence baseline drift detected ({baseline_new_count} new entries)"
                    ),
                    severity,
                    confidence: confidence_from_count(0.69, baseline_new_count, 0.04, 0.94),
                    evidence_pointer: EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.baseline_new_count".to_string(),
                    },
                    recommended_action: "Compare baseline_new_entries against approved software changes and investigate unexpected startup additions for persistence abuse.".to_string(),
                });
            }
        }

        if let Some(non_default_privileged_account_count) = observation
            .get("non_default_privileged_account_count")
            .and_then(Value::as_u64)
        {
            if non_default_privileged_account_count > 0 {
                findings.push(Finding {
                    title: format!(
                        "Non-default privileged accounts observed ({non_default_privileged_account_count})"
                    ),
                    severity: FindingSeverity::High,
                    confidence: confidence_from_count(
                        0.74,
                        non_default_privileged_account_count,
                        0.04,
                        0.96,
                    ),
                    evidence_pointer: EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.non_default_privileged_account_count".to_string(),
                    },
                    recommended_action: "Validate each non-default privileged account against approved access records; revoke unauthorized role grants and rotate exposed credentials.".to_string(),
                });
            }
        }

        if let Some(newly_privileged_account_count) = observation
            .get("newly_privileged_account_count")
            .and_then(Value::as_u64)
        {
            if newly_privileged_account_count > 0 {
                findings.push(Finding {
                    title: format!(
                        "Privileged account baseline drift detected ({newly_privileged_account_count} new account(s))"
                    ),
                    severity: if newly_privileged_account_count >= 3 {
                        FindingSeverity::Critical
                    } else {
                        FindingSeverity::High
                    },
                    confidence: confidence_from_count(
                        0.78,
                        newly_privileged_account_count,
                        0.04,
                        0.97,
                    ),
                    evidence_pointer: EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.newly_privileged_account_count".to_string(),
                    },
                    recommended_action: "Validate each newly privileged account against approved access changes, disable unauthorized grants, and rotate impacted credentials.".to_string(),
                });
            }
        }

        if let Some(unapproved_privileged_account_count) = observation
            .get("unapproved_privileged_account_count")
            .and_then(Value::as_u64)
        {
            if unapproved_privileged_account_count > 0 {
                findings.push(Finding {
                    title: format!(
                        "Unapproved privileged accounts detected ({unapproved_privileged_account_count})"
                    ),
                    severity: FindingSeverity::Critical,
                    confidence: confidence_from_count(
                        0.8,
                        unapproved_privileged_account_count,
                        0.04,
                        0.98,
                    ),
                    evidence_pointer: EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.unapproved_privileged_account_count".to_string(),
                    },
                    recommended_action: "Escalate immediately: remove unapproved privileged memberships, confirm identity ownership, and collect IAM audit evidence.".to_string(),
                });
            }
        }

        if let Some(externally_exposed_count) = observation
            .get("externally_exposed_count")
            .and_then(Value::as_u64)
        {
            if externally_exposed_count > 0 {
                let severity = if externally_exposed_count >= 10 {
                    FindingSeverity::High
                } else {
                    FindingSeverity::Medium
                };

                findings.push(Finding {
                    title: format!(
                        "Externally exposed listening endpoints observed ({externally_exposed_count})"
                    ),
                    severity,
                    confidence: confidence_from_count(0.66, externally_exposed_count, 0.03, 0.93),
                    evidence_pointer: EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.externally_exposed_count".to_string(),
                    },
                    recommended_action: "Confirm process ownership and necessity of exposed listeners; close or firewall unnecessary bindings and monitor for reappearance.".to_string(),
                });
            }
        }

        if let Some(high_risk_exposed_count) = observation
            .get("high_risk_exposed_count")
            .and_then(Value::as_u64)
        {
            if high_risk_exposed_count > 0 {
                findings.push(Finding {
                    title: format!(
                        "High-risk process listeners exposed externally ({high_risk_exposed_count})"
                    ),
                    severity: if high_risk_exposed_count >= 2 {
                        FindingSeverity::Critical
                    } else {
                        FindingSeverity::High
                    },
                    confidence: confidence_from_count(0.76, high_risk_exposed_count, 0.04, 0.97),
                    evidence_pointer: EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.high_risk_exposed_count".to_string(),
                    },
                    recommended_action: "Prioritize containment for high-risk exposed processes, validate command-line lineage, and restrict inbound access immediately.".to_string(),
                });
            }
        }

        if let Some(unknown_exposed_process_count) = observation
            .get("unknown_exposed_process_count")
            .and_then(Value::as_u64)
        {
            if unknown_exposed_process_count > 0 {
                findings.push(Finding {
                    title: format!(
                        "Unexpected exposed processes relative to expected allowlist ({unknown_exposed_process_count})"
                    ),
                    severity: FindingSeverity::High,
                    confidence: confidence_from_count(
                        0.72,
                        unknown_exposed_process_count,
                        0.04,
                        0.95,
                    ),
                    evidence_pointer: EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.unknown_exposed_process_count".to_string(),
                    },
                    recommended_action: "Reconcile unknown exposed processes against approved service inventory and close unapproved listeners through host firewall or service disablement.".to_string(),
                });
            }
        }

        if let Some(network_risk_score) = observation
            .get("network_risk_score")
            .and_then(Value::as_u64)
        {
            if network_risk_score >= 40 {
                let severity = if network_risk_score >= 70 {
                    FindingSeverity::Critical
                } else {
                    FindingSeverity::High
                };

                findings.push(Finding {
                    title: format!(
                        "Network exposure risk score exceeded threshold ({network_risk_score})"
                    ),
                    severity,
                    confidence: confidence_from_count(0.7, network_risk_score, 0.002, 0.96),
                    evidence_pointer: EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.network_risk_score".to_string(),
                    },
                    recommended_action: "Escalate to incident triage: prioritize exposed services with highest risk contribution and verify baseline drift across process-network bindings.".to_string(),
                });
            }
        }

        if let (Some(path), Some(_digest)) = (
            observation.get("path").and_then(Value::as_str),
            observation.get("sha256").and_then(Value::as_str),
        ) {
            findings.push(Finding {
                title: format!("File hash captured for {path}"),
                severity: FindingSeverity::Info,
                confidence: 0.90,
                evidence_pointer: EvidencePointer {
                    turn: Some(idx + 1),
                    tool: tool_name.clone(),
                    field: "observation.sha256".to_string(),
                },
                recommended_action: "Compare the hash against trusted baseline or threat-intel sources before taking containment action.".to_string(),
            });
        }

        if let Some(lines) = observation.get("lines").and_then(Value::as_array) {
            let suspicious_hits = lines
                .iter()
                .filter_map(Value::as_str)
                .filter(|line| is_suspicious_log_line(line))
                .count();

            if suspicious_hits > 0 {
                let severity = if suspicious_hits >= 5 {
                    FindingSeverity::High
                } else {
                    FindingSeverity::Medium
                };

                findings.push(Finding {
                    title: format!(
                        "Suspicious log keywords observed in {suspicious_hits} line(s)"
                    ),
                    severity,
                    confidence: confidence_from_count(0.64, suspicious_hits as u64, 0.05, 0.93),
                    evidence_pointer: EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.lines".to_string(),
                    },
                    recommended_action: "Inspect matching log lines for account abuse or execution anomalies, then pivot to host and identity telemetry.".to_string(),
                });
            }
        }
    }

    if findings.is_empty() {
        findings.push(Finding {
            title: "No high-confidence host findings derived from collected evidence".to_string(),
            severity: FindingSeverity::Info,
            confidence: 0.55,
            evidence_pointer: EvidencePointer {
                turn: None,
                tool: None,
                field: "final_answer".to_string(),
            },
            recommended_action: if final_answer.trim().is_empty() {
                "Review raw observations and rerun targeted task templates for deeper coverage."
                    .to_string()
            } else {
                "Review the final answer and raw observations; rerun targeted task templates if analyst confidence is low.".to_string()
            },
        });
    }

    findings
}

/// Tool authority ranking — higher means this tool's findings should be preferred
/// when two tools produce findings for the same observation field.
fn tool_authority(tool: Option<&str>) -> u8 {
    match tool {
        Some("correlate_process_network") => 3,
        Some("audit_account_changes") => 3,
        Some("inspect_persistence_locations") => 2,
        Some("scan_network") => 1,
        Some("check_privilege_escalation_vectors") => 1,
        _ => 0,
    }
}

/// Deduplicate findings that share the same observation field key.
/// When duplicates exist, keep the finding from the more authoritative tool,
/// falling back to highest confidence as tiebreaker.
pub fn deduplicate_findings(findings: Vec<Finding>) -> Vec<Finding> {
    let mut seen_fields: HashSet<String> = HashSet::new();
    let mut deduped: Vec<Finding> = Vec::new();

    // Group by observation field; these are the dedup keys.
    for finding in findings {
        let field = &finding.evidence_pointer.field;

        // Fields like "final_answer" or "observation.error" are never deduped.
        if field == "final_answer" || field == "observation.error" {
            deduped.push(finding);
            continue;
        }

        if seen_fields.contains(field) {
            // Replace existing if the new one is from a more authoritative tool.
            if let Some(existing) = deduped
                .iter_mut()
                .find(|f| f.evidence_pointer.field == *field)
            {
                let new_authority = tool_authority(finding.evidence_pointer.tool.as_deref());
                let existing_authority = tool_authority(existing.evidence_pointer.tool.as_deref());
                if new_authority > existing_authority
                    || (new_authority == existing_authority
                        && finding.confidence > existing.confidence)
                {
                    *existing = finding;
                }
            }
        } else {
            seen_fields.insert(field.clone());
            deduped.push(finding);
        }
    }

    deduped
}

/// Sort findings by severity descending, then confidence descending as tiebreaker.
pub fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        b.severity.cmp(&a.severity).then(
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
}

/// Compute the maximum severity across all findings.
pub fn max_severity(findings: &[Finding]) -> Option<FindingSeverity> {
    findings.iter().map(|f| f.severity).max()
}

/// Check whether LLM output is low quality and return a deterministic summary if so.
pub fn quality_checked_final_answer(raw_llm_output: &str, findings: &[Finding]) -> String {
    if is_low_quality(raw_llm_output) {
        deterministic_summary(findings)
    } else {
        raw_llm_output.to_string()
    }
}

fn is_low_quality(text: &str) -> bool {
    let trimmed = text.trim();

    // Length check.
    if trimmed.len() < 20 || trimmed.len() > 5000 {
        return true;
    }

    // Repetition detection: if any sentence appears 3+ times, flag.
    let sentences: Vec<&str> = trimmed
        .split(['.', '!', '?', '\n'])
        .map(|s| s.trim())
        .filter(|s| s.len() > 10)
        .collect();
    let mut seen = std::collections::HashMap::new();
    for sentence in &sentences {
        let lower = sentence.to_lowercase();
        let count = seen.entry(lower).or_insert(0u32);
        *count += 1;
        if *count >= 3 {
            return true;
        }
    }

    false
}

fn deterministic_summary(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "SUMMARY: No findings derived from collected evidence.".to_string();
    }

    let max_sev = findings
        .iter()
        .map(|f| f.severity)
        .max()
        .unwrap_or(FindingSeverity::Info);

    let top = &findings[0]; // findings are already severity-sorted

    format!(
        "SUMMARY: {} finding(s) ({} max severity). Top finding: {}. Recommended: {}",
        findings.len(),
        max_sev.token(),
        top.title,
        top.recommended_action
    )
}

fn confidence_from_count(base: f32, count: u64, slope: f32, ceiling: f32) -> f32 {
    let raw = (base + (count as f32 * slope)).min(ceiling);
    (raw * 100.0).round() / 100.0
}

fn is_suspicious_log_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    ["error", "failed", "denied", "unauthorized", "suspicious"]
        .iter()
        .any(|needle| lower.contains(needle))
}

pub fn format_system_prompt(tool_manifest: &str) -> String {
    format!(
        "You are an autonomous security investigation agent. Act independently.\n\
         TOOLS:\n\
         {tool_manifest}\n\
         FORMAT: Respond with ONLY one XML tag per turn.\n\
         To call a tool: <call>{{\"tool\":\"TOOL_NAME\",\"args\":{{}}}}</call>\n\
         To finish: <final>SUMMARY: ...\nFINDINGS: ...\nRISK: ...\nACTIONS: ...</final>\n\
         RULES: Your first response MUST be <call>. Call 2+ tools before <final>."
    )
}

pub fn extract_tag(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");

    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;

    Some(text[start..end].trim().to_string())
}

pub fn parse_tool_call(text: &str) -> Option<ToolCall> {
    let body = extract_tag(text, "call")?;
    serde_json::from_str::<ToolCall>(&body).ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        deduplicate_findings, derive_findings, extract_tag, is_low_quality, max_severity,
        parse_tool_call, quality_checked_final_answer, sort_findings, AgentTurn, EvidencePointer,
        Finding, FindingSeverity, ToolCall,
    };

    #[test]
    fn parses_json_tool_call() {
        let text = r#"<call>{"tool":"scan_network","args":{"limit":5}}</call>"#;
        let call = parse_tool_call(text).expect("tool call should parse");

        assert_eq!(call.tool, "scan_network");
        assert_eq!(call.args["limit"], 5);
    }

    #[test]
    fn extracts_final_tag() {
        let text = "noise <final>done</final> trailing";
        let final_text = extract_tag(text, "final").expect("final tag should parse");
        assert_eq!(final_text, "done");
    }

    #[test]
    fn derives_privilege_indicator_finding() {
        let turns = vec![AgentTurn {
            thought: "<call>{...}</call>".to_string(),
            tool_call: Some(ToolCall {
                tool: "check_privilege_escalation_vectors".to_string(),
                args: json!({}),
            }),
            observation: Some(json!({
                "indicator_count": 2,
                "potential_vectors": ["SeDebugPrivilege"]
            })),
        }];

        let findings = derive_findings(&turns, "final");
        assert!(findings
            .iter()
            .any(|finding| finding.title.contains("Privilege escalation indicators")));
        assert!(findings
            .iter()
            .any(|finding| finding.evidence_pointer.field == "observation.indicator_count"));
    }

    #[test]
    fn derives_error_finding_for_failed_tool_observation() {
        let turns = vec![AgentTurn {
            thought: "<call>{...}</call>".to_string(),
            tool_call: Some(ToolCall {
                tool: "scan_network".to_string(),
                args: json!({}),
            }),
            observation: Some(json!({
                "error": "socket inventory command failed"
            })),
        }];

        let findings = derive_findings(&turns, "final");
        assert!(findings
            .iter()
            .any(|finding| finding.severity == FindingSeverity::High));
        assert!(findings.iter().any(|finding| {
            finding.evidence_pointer.field == "observation.error"
                && finding.evidence_pointer.turn == Some(1)
        }));
    }

    #[test]
    fn emits_fallback_finding_when_no_signals_exist() {
        let turns = vec![AgentTurn {
            thought: "No-op".to_string(),
            tool_call: None,
            observation: None,
        }];

        let findings = derive_findings(&turns, "No significant anomalies detected.");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::Info);
        assert_eq!(findings[0].evidence_pointer.field, "final_answer");
    }

    #[test]
    fn derives_persistence_finding_from_suspicious_entries() {
        let turns = vec![AgentTurn {
            thought: "<call>{...}</call>".to_string(),
            tool_call: Some(ToolCall {
                tool: "inspect_persistence_locations".to_string(),
                args: json!({}),
            }),
            observation: Some(json!({
                "entry_count": 12,
                "suspicious_entry_count": 2,
                "actionable_suspicious_count": 2
            })),
        }];

        let findings = derive_findings(&turns, "final");
        assert!(findings.iter().any(|finding| {
            finding
                .title
                .contains("Actionable suspicious persistence entries")
                && finding.evidence_pointer.field == "observation.actionable_suspicious_count"
        }));
    }

    #[test]
    fn derives_privileged_account_drift_finding() {
        let turns = vec![AgentTurn {
            thought: "<call>{...}</call>".to_string(),
            tool_call: Some(ToolCall {
                tool: "audit_account_changes".to_string(),
                args: json!({}),
            }),
            observation: Some(json!({
                "newly_privileged_account_count": 1,
                "newly_privileged_accounts": ["svc-backup"]
            })),
        }];

        let findings = derive_findings(&turns, "final");
        assert!(findings.iter().any(|finding| {
            finding
                .title
                .contains("Privileged account baseline drift detected")
                && finding.evidence_pointer.field == "observation.newly_privileged_account_count"
        }));
    }

    #[test]
    fn derives_critical_network_risk_finding() {
        let turns = vec![AgentTurn {
            thought: "<call>{...}</call>".to_string(),
            tool_call: Some(ToolCall {
                tool: "correlate_process_network".to_string(),
                args: json!({}),
            }),
            observation: Some(json!({
                "externally_exposed_count": 4,
                "high_risk_exposed_count": 2,
                "network_risk_score": 78
            })),
        }];

        let findings = derive_findings(&turns, "final");
        assert!(findings.iter().any(|finding| {
            finding.title.contains("Network exposure risk score")
                && finding.severity == FindingSeverity::Critical
                && finding.evidence_pointer.field == "observation.network_risk_score"
        }));
    }

    #[test]
    fn derives_baseline_capture_finding() {
        let turns = vec![AgentTurn {
            thought: "<call>{...}</call>".to_string(),
            tool_call: Some(ToolCall {
                tool: "capture_coverage_baseline".to_string(),
                args: json!({}),
            }),
            observation: Some(json!({
                "baseline_version": "coverage-v1",
                "baseline_entries_count": 24,
                "baseline_privileged_account_count": 3,
                "baseline_exposed_binding_count": 5
            })),
        }];

        let findings = derive_findings(&turns, "final");
        assert!(findings.iter().any(|finding| {
            finding.title.contains("Coverage baseline captured")
                && finding.severity == FindingSeverity::Info
                && finding.evidence_pointer.field == "observation.baseline_version"
        }));
    }

    fn make_finding(
        title: &str,
        severity: FindingSeverity,
        confidence: f32,
        tool: &str,
        field: &str,
    ) -> Finding {
        Finding {
            title: title.to_string(),
            severity,
            confidence,
            evidence_pointer: EvidencePointer {
                turn: Some(1),
                tool: Some(tool.to_string()),
                field: field.to_string(),
            },
            recommended_action: "Investigate further.".to_string(),
        }
    }

    #[test]
    fn deduplicates_findings_by_field_keeps_higher_authority() {
        let findings = vec![
            make_finding(
                "Listeners via scan_network",
                FindingSeverity::Medium,
                0.70,
                "scan_network",
                "observation.listener_count",
            ),
            make_finding(
                "Listeners via correlate",
                FindingSeverity::Medium,
                0.65,
                "correlate_process_network",
                "observation.listener_count",
            ),
        ];

        let deduped = deduplicate_findings(findings);
        assert_eq!(deduped.len(), 1);
        assert!(deduped[0].title.contains("correlate"));
    }

    #[test]
    fn dedup_preserves_error_and_final_answer_fields() {
        let findings = vec![
            make_finding(
                "Error 1",
                FindingSeverity::High,
                0.90,
                "scan_network",
                "observation.error",
            ),
            make_finding(
                "Error 2",
                FindingSeverity::High,
                0.90,
                "scan_network",
                "observation.error",
            ),
            make_finding("Fallback", FindingSeverity::Info, 0.50, "", "final_answer"),
        ];

        let deduped = deduplicate_findings(findings);
        assert_eq!(deduped.len(), 3);
    }

    #[test]
    fn sort_findings_by_severity_then_confidence() {
        let mut findings = vec![
            make_finding("A", FindingSeverity::Low, 0.90, "scan_network", "a"),
            make_finding("B", FindingSeverity::Critical, 0.50, "scan_network", "b"),
            make_finding("C", FindingSeverity::High, 0.80, "scan_network", "c"),
            make_finding("D", FindingSeverity::High, 0.95, "scan_network", "d"),
        ];

        sort_findings(&mut findings);

        assert_eq!(findings[0].severity, FindingSeverity::Critical);
        assert_eq!(findings[1].severity, FindingSeverity::High);
        assert!(findings[1].confidence > findings[2].confidence);
        assert_eq!(findings[3].severity, FindingSeverity::Low);
    }

    #[test]
    fn max_severity_returns_highest() {
        let findings = vec![
            make_finding("A", FindingSeverity::Info, 0.50, "scan_network", "a"),
            make_finding("B", FindingSeverity::High, 0.50, "scan_network", "b"),
            make_finding("C", FindingSeverity::Medium, 0.50, "scan_network", "c"),
        ];

        assert_eq!(max_severity(&findings), Some(FindingSeverity::High));
    }

    #[test]
    fn max_severity_returns_none_for_empty() {
        let findings: Vec<Finding> = vec![];
        assert_eq!(max_severity(&findings), None);
    }

    #[test]
    fn low_quality_detects_short_text() {
        assert!(is_low_quality("too short"));
    }

    #[test]
    fn low_quality_detects_repetitive_text() {
        let repeated = "The system is compromised and needs attention. ".repeat(5);
        assert!(is_low_quality(&repeated));
    }

    #[test]
    fn low_quality_accepts_normal_text() {
        let normal = "The investigation found 3 suspicious network listeners on non-standard ports. One process (PID 4321) is associated with an unknown binary.";
        assert!(!is_low_quality(normal));
    }

    #[test]
    fn quality_check_replaces_bad_output_with_deterministic_summary() {
        let findings = vec![make_finding(
            "Active listeners",
            FindingSeverity::Medium,
            0.70,
            "scan_network",
            "observation.listener_count",
        )];

        let result = quality_checked_final_answer("bad", &findings);
        assert!(result.starts_with("SUMMARY:"));
        assert!(result.contains("1 finding(s)"));
        assert!(result.contains("medium"));
    }

    #[test]
    fn quality_check_preserves_good_output() {
        let findings = vec![make_finding(
            "Active listeners",
            FindingSeverity::Medium,
            0.70,
            "scan_network",
            "observation.listener_count",
        )];

        let good = "The investigation revealed 3 suspicious listener ports that warrant further analysis and correlation with expected services.";
        let result = quality_checked_final_answer(good, &findings);
        assert_eq!(result, good);
    }

    #[test]
    fn confidence_serializes_to_two_decimals() {
        let finding = make_finding("A", FindingSeverity::Info, 0.6789, "scan_network", "a");
        let json = serde_json::to_string(&finding).expect("serialize");
        assert!(json.contains("0.68"));
        assert!(!json.contains("0.6789"));
    }

    #[test]
    fn finding_severity_ordering() {
        assert!(FindingSeverity::Critical > FindingSeverity::High);
        assert!(FindingSeverity::High > FindingSeverity::Medium);
        assert!(FindingSeverity::Medium > FindingSeverity::Low);
        assert!(FindingSeverity::Low > FindingSeverity::Info);
    }
}
