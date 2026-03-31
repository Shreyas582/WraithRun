pub mod agent;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    pub thought: String,
    pub tool_call: Option<ToolCall>,
    pub observation: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidencePointer {
    pub turn: Option<usize>,
    pub tool: Option<String>,
    pub field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Finding {
    pub title: String,
    pub severity: FindingSeverity,
    pub confidence: f32,
    pub evidence_pointer: EvidencePointer,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub task: String,
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

        if let Some(suspicious_entry_count) = observation
            .get("suspicious_entry_count")
            .and_then(Value::as_u64)
        {
            if suspicious_entry_count > 0 {
                let severity = if suspicious_entry_count >= 5 {
                    FindingSeverity::High
                } else {
                    FindingSeverity::Medium
                };

                findings.push(Finding {
                    title: format!(
                        "Suspicious persistence entries detected ({suspicious_entry_count})"
                    ),
                    severity,
                    confidence: confidence_from_count(0.7, suspicious_entry_count, 0.05, 0.95),
                    evidence_pointer: EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.suspicious_entry_count".to_string(),
                    },
                    recommended_action: "Review persistence entries for unsigned or user-profile startup references, then remove unauthorized autoruns and collect triage artifacts.".to_string(),
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

pub fn format_system_prompt(tool_manifest_json: &str) -> String {
    format!(
        "You are a local-first cyber operations agent.\n\
         Reason carefully, use tools when needed, and keep outputs machine-parseable.\n\n\
         Available tools JSON:\n\
         {tool_manifest_json}\n\n\
         Output contract:\n\
         - Tool call format: <call>{{\"tool\":\"tool_name\",\"args\":{{...}}}}</call>\n\
         - Final answer format: <final>your conclusion</final>\n\
         - Never emit both <call> and <final> in the same response."
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

    if let Ok(call) = serde_json::from_str::<ToolCall>(&body) {
        return Some(call);
    }

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(ToolCall {
        tool: trimmed.to_string(),
        args: Value::Object(serde_json::Map::new()),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        derive_findings, extract_tag, parse_tool_call, AgentTurn, FindingSeverity, ToolCall,
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
                "suspicious_entry_count": 2
            })),
        }];

        let findings = derive_findings(&turns, "final");
        assert!(findings.iter().any(|finding| {
            finding
                .title
                .contains("Suspicious persistence entries detected")
                && finding.evidence_pointer.field == "observation.suspicious_entry_count"
        }));
    }
}
