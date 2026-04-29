pub mod agent;

use std::collections::{HashMap, HashSet};

use inference_bridge::ModelCapabilityProbe;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

// ── Investigation templates (#84) ──

/// A declarative investigation template that maps task keywords to tool sets.
#[derive(Debug, Clone, Serialize)]
pub struct InvestigationTemplate {
    pub name: &'static str,
    pub description: &'static str,
    #[serde(skip)]
    pub match_keywords: &'static [&'static str],
    pub tools: &'static [&'static str],
}

/// All built-in investigation templates.
pub fn builtin_investigation_templates() -> &'static [InvestigationTemplate] {
    &BUILTIN_TEMPLATES
}

static BUILTIN_TEMPLATES: [InvestigationTemplate; 9] = [
    InvestigationTemplate {
        name: "broad-host-triage",
        description: "General-purpose host investigation covering persistence, accounts, network, and privilege vectors",
        match_keywords: &[],
        tools: &[
            "audit_account_changes",
            "inspect_persistence_locations",
            "enumerate_scheduled_tasks",
            "analyze_process_tree",
            "read_syslog",
            "scan_network",
            "check_privilege_escalation_vectors",
        ],
    },
    InvestigationTemplate {
        name: "ssh-key-investigation",
        description: "Investigate unauthorized SSH keys and related access",
        match_keywords: &["ssh", "authorized_keys", "key"],
        tools: &[
            "enumerate_ssh_keys",
            "audit_account_changes",
            "inspect_persistence_locations",
            "check_privilege_escalation_vectors",
        ],
    },
    InvestigationTemplate {
        name: "persistence-analysis",
        description: "Analyze persistence mechanisms including autoruns and scheduled tasks (MITRE T1053)",
        match_keywords: &["persistence", "autorun", "startup", "cron", "scheduled", "task", "t1053"],
        tools: &[
            "inspect_persistence_locations",
            "enumerate_scheduled_tasks",
            "audit_account_changes",
            "read_syslog",
        ],
    },
    InvestigationTemplate {
        name: "process-tree-analysis",
        description: "Analyse running process parent-child relationships to surface suspicious spawns (MITRE T1057)",
        match_keywords: &["process", "tree", "parent", "child", "spawn", "t1057", "injection"],
        tools: &[
            "analyze_process_tree",
            "correlate_process_network",
            "audit_account_changes",
        ],
    },
    InvestigationTemplate {
        name: "network-exposure-audit",
        description: "Audit network listeners, exposed services, and lateral movement indicators",
        match_keywords: &["network", "connection", "port", "listen", "listener", "lateral", "beacon", "socket"],
        tools: &[
            "scan_network",
            "correlate_process_network",
            "audit_account_changes",
        ],
    },
    InvestigationTemplate {
        name: "privilege-escalation-check",
        description: "Review local privilege escalation vectors and unauthorized account grants",
        match_keywords: &["privilege", "escalat", "admin", "root", "sudo", "unauthori"],
        tools: &[
            "check_privilege_escalation_vectors",
            "audit_account_changes",
            "inspect_persistence_locations",
        ],
    },
    InvestigationTemplate {
        name: "file-integrity-check",
        description: "Verify file hashes and detect tampering of critical binaries",
        match_keywords: &["hash", "integrity", "checksum", "binary", "tamper"],
        tools: &[
            "hash_binary",
            "inspect_persistence_locations",
        ],
    },
    InvestigationTemplate {
        name: "syslog-analysis",
        description: "Analyze system logs for security-relevant events including failed logins and service anomalies",
        match_keywords: &["log", "syslog", "journal", "event", "audit"],
        tools: &[
            "read_syslog",
            "audit_account_changes",
            "inspect_persistence_locations",
        ],
    },
    InvestigationTemplate {
        name: "malware-triage",
        description: "Triage a suspected malware infection: processes, persistence, network beacons, file hashes",
        match_keywords: &["malware", "infection", "ransomware", "trojan", "backdoor", "c2", "beacon", "implant"],
        tools: &[
            "analyze_process_tree",
            "enumerate_scheduled_tasks",
            "inspect_persistence_locations",
            "correlate_process_network",
            "hash_binary",
        ],
    },
];

// ── Capability tiering thresholds (const, easy to tune) ──

/// Models below this parameter count (billions) are classified as Basic.
/// Lowered from 2.0 → 1.0 so common 1B+ models (Qwen2.5-0.5B overestimates
/// to ~1.4B, Llama-3.2-1B at ~1.12B) reach Moderate tier and use inference (#157).
const PARAM_BASIC_CEILING_B: f32 = 1.0;
/// Models above this parameter count (billions) are classified as Strong.
const PARAM_STRONG_FLOOR_B: f32 = 10.0;
/// Latency above this (ms/tok) demotes to Basic.
const LATENCY_BASIC_FLOOR_MS: u64 = 200;
/// Latency below this (ms/tok) promotes to Strong.
const LATENCY_STRONG_CEILING_MS: u64 = 50;

/// Model capability tier that determines agent behavior in Phase 2.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum ModelCapabilityTier {
    Basic,
    Moderate,
    Strong,
}

impl ModelCapabilityTier {
    pub fn token(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Moderate => "moderate",
            Self::Strong => "strong",
        }
    }
}

impl std::fmt::Display for ModelCapabilityTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.token())
    }
}

/// Classify a model's capability probe signals into a tier.
///
/// Final tier = min(param_tier, latency_tier).
/// A 13B model on a slow CPU is Moderate (latency-constrained).
/// A 1B model on a fast GPU is Basic (param-constrained).
pub fn classify_capability(probe: &ModelCapabilityProbe) -> ModelCapabilityTier {
    let param_tier = if probe.estimated_param_billions < PARAM_BASIC_CEILING_B {
        ModelCapabilityTier::Basic
    } else if probe.estimated_param_billions > PARAM_STRONG_FLOOR_B {
        ModelCapabilityTier::Strong
    } else {
        ModelCapabilityTier::Moderate
    };

    let latency_tier = if probe.smoke_latency_ms > LATENCY_BASIC_FLOOR_MS {
        ModelCapabilityTier::Basic
    } else if probe.smoke_latency_ms < LATENCY_STRONG_CEILING_MS {
        ModelCapabilityTier::Strong
    } else {
        ModelCapabilityTier::Moderate
    };

    // min() works because of the PartialOrd derive: Basic < Moderate < Strong.
    param_tier.min(latency_tier)
}

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
    /// Wall-clock tool execution time in milliseconds (#129).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
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
    // Round at f64 precision to avoid 0.61_f32 → 0.6100000143051147 in JSON output (#195).
    let rounded = ((*value as f64) * 100.0).round() / 100.0;
    serializer.serialize_f64(rounded)
}

/// Single contribution to a finding's overall confidence (#195).
///
/// Each factor names where the score came from (base prior, count signal,
/// corroboration, etc.) so analysts can audit the derivation instead of
/// trusting an opaque float.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfidenceFactor {
    pub factor: String,
    #[serde(serialize_with = "serialize_confidence")]
    pub contribution: f32,
    pub detail: String,
}

impl ConfidenceFactor {
    pub fn new(factor: impl Into<String>, contribution: f32, detail: impl Into<String>) -> Self {
        Self {
            factor: factor.into(),
            contribution,
            detail: detail.into(),
        }
    }
}

/// Discrete confidence label replacing arbitrary float precision (#85).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum FindingConfidence {
    Informational,
    Possible,
    Likely,
    Confirmed,
}

impl FindingConfidence {
    pub fn token(self) -> &'static str {
        match self {
            Self::Informational => "informational",
            Self::Possible => "possible",
            Self::Likely => "likely",
            Self::Confirmed => "confirmed",
        }
    }
}

impl std::fmt::Display for FindingConfidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.token())
    }
}

/// Map a continuous confidence float to a discrete label.
pub fn confidence_to_label(confidence: f32) -> FindingConfidence {
    if confidence >= 0.90 {
        FindingConfidence::Confirmed
    } else if confidence >= 0.72 {
        FindingConfidence::Likely
    } else if confidence >= 0.55 {
        FindingConfidence::Possible
    } else {
        FindingConfidence::Informational
    }
}

/// Relevance of a finding relative to the user's task (#86).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FindingRelevance {
    #[default]
    Primary,
    Supplementary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Finding {
    pub title: String,
    pub severity: FindingSeverity,
    #[serde(serialize_with = "serialize_confidence")]
    pub confidence: f32,
    #[serde(default = "default_confidence_label")]
    pub confidence_label: FindingConfidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confidence_factors: Vec<ConfidenceFactor>,
    #[serde(default)]
    pub relevance: FindingRelevance,
    pub evidence_pointer: EvidencePointer,
    pub recommended_action: String,
}

fn default_confidence_label() -> FindingConfidence {
    FindingConfidence::Possible
}

impl Finding {
    /// Create a new finding, auto-deriving confidence_label from the float value.
    pub fn new(
        title: String,
        severity: FindingSeverity,
        confidence: f32,
        evidence_pointer: EvidencePointer,
        recommended_action: String,
    ) -> Self {
        Self {
            title,
            severity,
            confidence_label: confidence_to_label(confidence),
            confidence_factors: Vec::new(),
            relevance: FindingRelevance::Primary,
            confidence,
            evidence_pointer,
            recommended_action,
        }
    }

    /// Backfill confidence_label from the confidence float.
    pub fn with_derived_label(mut self) -> Self {
        self.confidence_label = confidence_to_label(self.confidence);
        self
    }

    /// Attach derivation factors that explain how `confidence` was computed (#195).
    pub fn with_factors(mut self, factors: Vec<ConfidenceFactor>) -> Self {
        self.confidence_factors = factors;
        self
    }
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
    pub backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_capability: Option<ModelCapabilityReport>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supplementary_findings: Vec<Finding>,
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
            findings.push(
                Finding::new(
                    format!("Tool execution failed for {tool_label}"),
                    FindingSeverity::High,
                    0.95,
                    EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.error".to_string(),
                    },
                    format!(
                        "Review tool arguments and host access policy, then rerun {tool_label}. Error sample: {error}"
                    ),
                )
                .with_factors(fixed_confidence_factor(
                    0.95,
                    "tool execution error is unambiguous",
                )),
            );
            continue;
        }

        if let Some(indicator_count) = observation.get("indicator_count").and_then(Value::as_u64) {
            if indicator_count > 0 {
                // Most Windows desktops report a few privilege escalation
                // indicators from normal admin usage (UAC settings, service
                // permissions).  Reserve High for genuinely elevated counts.
                let severity = if indicator_count >= 8 {
                    FindingSeverity::High
                } else if indicator_count >= 4 {
                    FindingSeverity::Medium
                } else {
                    FindingSeverity::Low
                };

                let (conf, factors) = confidence_from_count_with_factors(
                    0.55,
                    indicator_count,
                    0.05,
                    0.92,
                    "indicator_count",
                );
                findings.push(
                    Finding::new(
                        format!(
                            "Privilege escalation indicators detected ({indicator_count})"
                        ),
                        severity,
                        conf,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.indicator_count".to_string(),
                        },
                        "Review potential_vectors and verify whether elevated rights are expected; revoke or constrain unexpected grants.".to_string(),
                    )
                    .with_factors(factors),
                );
            }
        }

        if let Some(listener_count) = observation.get("listener_count").and_then(Value::as_u64) {
            if listener_count > 0 {
                // A typical Windows desktop has 80-150+ listening sockets from
                // standard services (svchost, lsass, etc.).  Only flag when the
                // count is well above normal baselines.
                let severity = if listener_count >= 250 {
                    FindingSeverity::High
                } else if listener_count >= 150 {
                    FindingSeverity::Medium
                } else if listener_count >= 50 {
                    FindingSeverity::Low
                } else {
                    FindingSeverity::Info
                };

                let (conf, factors) = confidence_from_count_with_factors(
                    0.50,
                    listener_count,
                    0.001,
                    0.80,
                    "listener_count",
                );
                findings.push(
                    Finding::new(
                        format!("Active listening sockets observed ({listener_count})"),
                        severity,
                        conf,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.listener_count".to_string(),
                        },
                        "Correlate listener PIDs and ports with expected services; investigate unknown listeners and expose only required interfaces.".to_string(),
                    )
                    .with_factors(factors),
                );
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

            findings.push(
                Finding::new(
                    format!(
                        "Coverage baseline captured ({baseline_entries_count} persistence entries, {baseline_privileged_account_count} privileged accounts, {baseline_exposed_binding_count} exposed bindings)"
                    ),
                    FindingSeverity::Info,
                    0.9,
                    EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.baseline_version".to_string(),
                    },
                    format!(
                        "Store baseline arrays from this {baseline_version} snapshot and supply them to coverage tools in subsequent runs to detect drift."
                    ),
                )
                .with_factors(fixed_confidence_factor(
                    0.9,
                    "baseline capture is a deterministic snapshot",
                )),
            );
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
            // A few suspicious persistence entries are common on consumer
            // systems (Discord, Steam, etc.).  Only flag as High when many
            // entries are present, suggesting deliberate persistence planting.
            let severity = if persistence_count >= 8 {
                FindingSeverity::High
            } else if persistence_count >= 3 {
                FindingSeverity::Medium
            } else {
                FindingSeverity::Low
            };

            // Extract top entries for detail.
            let detail_field = if actionable_persistence_count > 0 {
                "actionable_suspicious_entries"
            } else {
                "suspicious_entries"
            };
            let detail = extract_persistence_summary(observation, detail_field, 3);

            let (title, evidence_field) = if actionable_persistence_count > 0 {
                (
                    if detail.is_empty() {
                        format!("Actionable suspicious persistence entries detected ({persistence_count})")
                    } else {
                        format!("Actionable suspicious persistence entries detected ({persistence_count}): {detail}")
                    },
                    "observation.actionable_suspicious_count",
                )
            } else {
                (
                    if detail.is_empty() {
                        format!("Suspicious persistence entries detected ({persistence_count})")
                    } else {
                        format!("Suspicious persistence entries detected ({persistence_count}): {detail}")
                    },
                    "observation.suspicious_entry_count",
                )
            };

            let (conf, factors) = confidence_from_count_with_factors(
                0.7,
                persistence_count,
                0.05,
                0.95,
                evidence_field,
            );
            findings.push(
                Finding::new(
                    title,
                    severity,
                    conf,
                    EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: evidence_field.to_string(),
                    },
                    "Review persistence entries for unauthorized startup references, remove unapproved autoruns, and preserve forensic artifacts before cleanup.".to_string(),
                )
                .with_factors(factors),
            );
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

                let (conf, factors) = confidence_from_count_with_factors(
                    0.69,
                    baseline_new_count,
                    0.04,
                    0.94,
                    "baseline_new_count",
                );
                findings.push(
                    Finding::new(
                        format!(
                            "Persistence baseline drift detected ({baseline_new_count} new entries)"
                        ),
                        severity,
                        conf,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.baseline_new_count".to_string(),
                        },
                        "Compare baseline_new_entries against approved software changes and investigate unexpected startup additions for persistence abuse.".to_string(),
                    )
                    .with_factors(factors),
                );
            }
        }

        if let Some(non_default_privileged_account_count) = observation
            .get("non_default_privileged_account_count")
            .and_then(Value::as_u64)
        {
            if non_default_privileged_account_count > 0 {
                // A single non-default admin account on a personal workstation
                // is common and expected — only escalate for multiple accounts
                // or when combined with other indicators.
                let severity = if non_default_privileged_account_count >= 5 {
                    FindingSeverity::High
                } else if non_default_privileged_account_count >= 3 {
                    FindingSeverity::Medium
                } else {
                    FindingSeverity::Low
                };
                // Extract account names for detail.
                let detail =
                    extract_string_list_summary(observation, "non_default_privileged_accounts", 3);
                let title = if detail.is_empty() {
                    format!("Non-default privileged accounts observed ({non_default_privileged_account_count})")
                } else {
                    format!("Non-default privileged accounts observed ({non_default_privileged_account_count}): {detail}")
                };
                let (conf, factors) = confidence_from_count_with_factors(
                    0.55,
                    non_default_privileged_account_count,
                    0.06,
                    0.92,
                    "non_default_privileged_account_count",
                );
                findings.push(
                    Finding::new(
                        title,
                        severity,
                        conf,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.non_default_privileged_account_count".to_string(),
                        },
                        "Validate each non-default privileged account against approved access records; revoke unauthorized role grants and rotate exposed credentials.".to_string(),
                    )
                    .with_factors(factors),
                );
            }
        }

        if let Some(newly_privileged_account_count) = observation
            .get("newly_privileged_account_count")
            .and_then(Value::as_u64)
        {
            if newly_privileged_account_count > 0 {
                let (conf, factors) = confidence_from_count_with_factors(
                    0.78,
                    newly_privileged_account_count,
                    0.04,
                    0.97,
                    "newly_privileged_account_count",
                );
                findings.push(
                    Finding::new(
                        format!(
                            "Privileged account baseline drift detected ({newly_privileged_account_count} new account(s))"
                        ),
                        if newly_privileged_account_count >= 3 {
                            FindingSeverity::Critical
                        } else {
                            FindingSeverity::High
                        },
                        conf,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.newly_privileged_account_count".to_string(),
                        },
                        "Validate each newly privileged account against approved access changes, disable unauthorized grants, and rotate impacted credentials.".to_string(),
                    )
                    .with_factors(factors),
                );
            }
        }

        if let Some(unapproved_privileged_account_count) = observation
            .get("unapproved_privileged_account_count")
            .and_then(Value::as_u64)
        {
            if unapproved_privileged_account_count > 0 {
                let (conf, factors) = confidence_from_count_with_factors(
                    0.8,
                    unapproved_privileged_account_count,
                    0.04,
                    0.98,
                    "unapproved_privileged_account_count",
                );
                findings.push(
                    Finding::new(
                        format!(
                            "Unapproved privileged accounts detected ({unapproved_privileged_account_count})"
                        ),
                        FindingSeverity::Critical,
                        conf,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.unapproved_privileged_account_count".to_string(),
                        },
                        "Escalate immediately: remove unapproved privileged memberships, confirm identity ownership, and collect IAM audit evidence.".to_string(),
                    )
                    .with_factors(factors),
                );
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

                let (conf, factors) = confidence_from_count_with_factors(
                    0.66,
                    externally_exposed_count,
                    0.03,
                    0.93,
                    "externally_exposed_count",
                );
                findings.push(
                    Finding::new(
                        format!(
                            "Externally exposed listening endpoints observed ({externally_exposed_count})"
                        ),
                        severity,
                        conf,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.externally_exposed_count".to_string(),
                        },
                        "Confirm process ownership and necessity of exposed listeners; close or firewall unnecessary bindings and monitor for reappearance.".to_string(),
                    )
                    .with_factors(factors),
                );
            }
        }

        if let Some(high_risk_exposed_count) = observation
            .get("high_risk_exposed_count")
            .and_then(Value::as_u64)
        {
            if high_risk_exposed_count > 0 {
                let (conf, factors) = confidence_from_count_with_factors(
                    0.76,
                    high_risk_exposed_count,
                    0.04,
                    0.97,
                    "high_risk_exposed_count",
                );
                findings.push(
                    Finding::new(
                        format!(
                            "High-risk process listeners exposed externally ({high_risk_exposed_count})"
                        ),
                        if high_risk_exposed_count >= 2 {
                            FindingSeverity::Critical
                        } else {
                            FindingSeverity::High
                        },
                        conf,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.high_risk_exposed_count".to_string(),
                        },
                        "Prioritize containment for high-risk exposed processes, validate command-line lineage, and restrict inbound access immediately.".to_string(),
                    )
                    .with_factors(factors),
                );
            }
        }

        if let Some(unknown_exposed_process_count) = observation
            .get("unknown_exposed_process_count")
            .and_then(Value::as_u64)
        {
            if unknown_exposed_process_count > 0 {
                let (conf, factors) = confidence_from_count_with_factors(
                    0.72,
                    unknown_exposed_process_count,
                    0.04,
                    0.95,
                    "unknown_exposed_process_count",
                );
                findings.push(
                    Finding::new(
                        format!(
                            "Unexpected exposed processes relative to expected allowlist ({unknown_exposed_process_count})"
                        ),
                        FindingSeverity::High,
                        conf,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.unknown_exposed_process_count".to_string(),
                        },
                        "Reconcile unknown exposed processes against approved service inventory and close unapproved listeners through host firewall or service disablement.".to_string(),
                    )
                    .with_factors(factors),
                );
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

                let (conf, factors) = confidence_from_count_with_factors(
                    0.7,
                    network_risk_score,
                    0.002,
                    0.96,
                    "network_risk_score",
                );
                findings.push(
                    Finding::new(
                        format!(
                            "Network exposure risk score exceeded threshold ({network_risk_score})"
                        ),
                        severity,
                        conf,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.network_risk_score".to_string(),
                        },
                        "Escalate to incident triage: prioritize exposed services with highest risk contribution and verify baseline drift across process-network bindings.".to_string(),
                    )
                    .with_factors(factors),
                );
            }
        }

        if let (Some(path), Some(_digest)) = (
            observation.get("path").and_then(Value::as_str),
            observation.get("sha256").and_then(Value::as_str),
        ) {
            findings.push(
                Finding::new(
                    format!("File hash captured for {path}"),
                    FindingSeverity::Info,
                    0.90,
                    EvidencePointer {
                        turn: Some(idx + 1),
                        tool: tool_name.clone(),
                        field: "observation.sha256".to_string(),
                    },
                    "Compare the hash against trusted baseline or threat-intel sources before taking containment action.".to_string(),
                )
                .with_factors(fixed_confidence_factor(0.90, "sha256 capture is deterministic")),
            );
        }

        // SSH key enumeration findings.
        if let Some(total_authorized) = observation
            .get("total_authorized_keys_files")
            .and_then(Value::as_u64)
        {
            let total_private = observation
                .get("total_private_keys")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let dirs_scanned = observation
                .get("ssh_directories_scanned")
                .and_then(Value::as_u64)
                .unwrap_or(0);

            if total_authorized > 0 || total_private > 0 {
                let severity = if total_authorized >= 3 {
                    FindingSeverity::High
                } else if total_authorized >= 1 {
                    FindingSeverity::Medium
                } else {
                    FindingSeverity::Low
                };

                // Build detail string from directories.
                let detail =
                    if let Some(dirs) = observation.get("directories").and_then(Value::as_array) {
                        let summaries: Vec<String> = dirs
                            .iter()
                            .take(3)
                            .map(|d| {
                                let dir = d.get("ssh_dir").and_then(Value::as_str).unwrap_or("?");
                                let has_ak = d
                                    .get("has_authorized_keys")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false);
                                format!("{dir} (authorized_keys: {has_ak})")
                            })
                            .collect();
                        if dirs.len() > 3 {
                            format!("{} and {} more", summaries.join(", "), dirs.len() - 3)
                        } else {
                            summaries.join(", ")
                        }
                    } else {
                        String::new()
                    };

                let (conf, factors) = confidence_from_count_with_factors(
                    0.72,
                    total_authorized + total_private,
                    0.04,
                    0.94,
                    "total_authorized_keys_files",
                );
                findings.push(
                    Finding::new(
                        format!(
                            "SSH keys found: {total_authorized} authorized_keys file(s), {total_private} private key(s) across {dirs_scanned} directory(ies){maybe_detail}",
                            maybe_detail = if detail.is_empty() { String::new() } else { format!(" — {detail}") }
                        ),
                        severity,
                        conf,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.total_authorized_keys_files".to_string(),
                        },
                        "Audit each authorized_keys file for unknown public keys; verify private key ownership and consider rotation for unattended keys.".to_string(),
                    )
                    .with_factors(factors),
                );
            } else {
                findings.push(
                    Finding::new(
                        format!("No SSH keys found across {dirs_scanned} directory(ies) scanned"),
                        FindingSeverity::Info,
                        0.85,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.ssh_directories_scanned".to_string(),
                        },
                        "SSH key access is not configured on this host.".to_string(),
                    )
                    .with_factors(fixed_confidence_factor(
                        0.85,
                        "absence of SSH keys is a deterministic observation",
                    )),
                );
            }
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

                let (conf, factors) = confidence_from_count_with_factors(
                    0.64,
                    suspicious_hits as u64,
                    0.05,
                    0.93,
                    "suspicious_log_lines",
                );
                findings.push(
                    Finding::new(
                        format!(
                            "Suspicious log keywords observed in {suspicious_hits} line(s)"
                        ),
                        severity,
                        conf,
                        EvidencePointer {
                            turn: Some(idx + 1),
                            tool: tool_name.clone(),
                            field: "observation.lines".to_string(),
                        },
                        "Inspect matching log lines for account abuse or execution anomalies, then pivot to host and identity telemetry.".to_string(),
                    )
                    .with_factors(factors),
                );
            }
        }
    }

    if findings.is_empty() {
        findings.push(
            Finding::new(
                "No high-confidence host findings derived from collected evidence".to_string(),
                FindingSeverity::Info,
                0.55,
                EvidencePointer {
                    turn: None,
                    tool: None,
                    field: "final_answer".to_string(),
                },
                if final_answer.trim().is_empty() {
                    "Review raw observations and rerun targeted task templates for deeper coverage."
                        .to_string()
                } else {
                    "Review the final answer and raw observations; rerun targeted task templates if analyst confidence is low.".to_string()
                },
            )
            .with_factors(fixed_confidence_factor(
                0.55,
                "fallback finding when no concrete observations matched derivation rules",
            )),
        );
    }

    // Evidence-derived confidence adjustments (#121):
    // 1. Corroboration: boost confidence when multiple tools report related findings.
    // 2. Count distinct tools that contributed concrete findings (non-error, non-fallback).
    let distinct_tools: HashSet<&str> = findings
        .iter()
        .filter_map(|f| f.evidence_pointer.tool.as_deref())
        .collect();
    let distinct_tools_count = distinct_tools.len();
    let corroboration_bonus: f32 = match distinct_tools_count {
        0 | 1 => 0.0,
        2 => 0.03,
        _ => 0.05,
    };

    for finding in &mut findings {
        // Only boost concrete tool-sourced findings, not error/fallback entries.
        if finding.evidence_pointer.tool.is_some()
            && finding.evidence_pointer.field != "observation.error"
            && corroboration_bonus > 0.0
        {
            finding.confidence =
                ((finding.confidence + corroboration_bonus) * 100.0).round() / 100.0;
            let mut applied_bonus = corroboration_bonus;
            if finding.confidence > 0.99 {
                applied_bonus -= finding.confidence - 0.99;
                finding.confidence = 0.99;
            }
            finding.confidence_factors.push(ConfidenceFactor::new(
                "corroboration",
                applied_bonus,
                format!("{distinct_tools_count} distinct tools contributed to this run"),
            ));
        }
    }

    // Backfill confidence labels from float values (#85).
    findings
        .into_iter()
        .map(|f| f.with_derived_label())
        .collect()
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

    let lower = trimmed.to_lowercase();

    // Detect hallucinated tool calls inside the final answer — the model is
    // role-playing the entire conversation instead of summarising (#137).
    if lower.contains("<call>") || lower.contains("[observation") {
        return true;
    }

    // Detect role-play / menu responses where the model echoes the tool list
    // instead of producing analytical output (#137).
    let menu_indicators = [
        "what would you like",
        "would you like me to",
        "choose one of the following",
        "select an option",
        "here are the available",
        "which tool would you",
        "i can help you with",
        "let me know which",
        "please select",
        "1)",
    ];
    let menu_hits = menu_indicators
        .iter()
        .filter(|indicator| lower.contains(**indicator))
        .count();
    if menu_hits >= 2 {
        return true;
    }

    // Check for numbered menu pattern: "1) item\n2) item\n3) item"
    let numbered_lines = trimmed
        .lines()
        .filter(|line| {
            let l = line.trim();
            l.starts_with("1)")
                || l.starts_with("2)")
                || l.starts_with("3)")
                || l.starts_with("4)")
                || l.starts_with("5)")
        })
        .count();
    if numbered_lines >= 3 && menu_hits >= 1 {
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

/// Build a rich deterministic summary for Basic-tier runs (no LLM).
///
/// Format follows the spec from issue #79 — byte-identical across runs
/// with the same findings.
pub fn basic_tier_summary(findings: &[Finding]) -> String {
    basic_tier_summary_for_task(findings, None)
}

/// Task-aware variant of [`basic_tier_summary`] that generates a contextual
/// executive sentence referencing the investigation task (#122).
pub fn basic_tier_summary_for_task(findings: &[Finding], task: Option<&str>) -> String {
    if findings.is_empty() {
        let prefix = match task {
            Some(t) => {
                format!("SUMMARY: Task \"{t}\" completed with 0 findings. Maximum severity: info.")
            }
            None => "SUMMARY: 0 findings detected. Maximum severity: info.".to_string(),
        };
        return format!("{prefix}\nFINDINGS:\n(none)\nRISK: info\nACTIONS:\n(none)");
    }

    // Sort findings: highest severity first, then by confidence descending (#155).
    let mut sorted: Vec<&Finding> = findings.iter().collect();
    sorted.sort_by(|a, b| {
        b.severity.cmp(&a.severity).then_with(|| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    let max_sev = sorted
        .first()
        .map(|f| f.severity)
        .unwrap_or(FindingSeverity::Info);

    let distinct_tools: HashSet<&str> = findings
        .iter()
        .filter_map(|f| f.evidence_pointer.tool.as_deref())
        .collect();

    // -- Header --
    let mut out = match task {
        Some(t) => format!(
            "INVESTIGATION SUMMARY — \"{t}\"\n\
             {total} findings across {tools} tool(s). Maximum severity: {sev}.\n\n",
            total = findings.len(),
            tools = distinct_tools.len(),
            sev = max_sev.token().to_uppercase()
        ),
        None => format!(
            "INVESTIGATION SUMMARY\n\
             {total} findings detected. Maximum severity: {sev}.\n\n",
            total = findings.len(),
            sev = max_sev.token().to_uppercase()
        ),
    };

    // -- Group by severity (highest first) --
    let severity_order = [
        FindingSeverity::Critical,
        FindingSeverity::High,
        FindingSeverity::Medium,
        FindingSeverity::Low,
        FindingSeverity::Info,
    ];

    for &sev in &severity_order {
        let group: Vec<&&Finding> = sorted.iter().filter(|f| f.severity == sev).collect();
        if group.is_empty() {
            continue;
        }
        let header = match sev {
            FindingSeverity::Critical => "🔴 CRITICAL",
            FindingSeverity::High => "🟠 HIGH",
            FindingSeverity::Medium => "🟡 MEDIUM",
            FindingSeverity::Low => "🔵 LOW",
            FindingSeverity::Info => "ℹ️  INFO",
        };
        out.push_str(&format!("── {} ({}) ──\n", header, group.len()));
        for f in &group {
            let tool_tag = f
                .evidence_pointer
                .tool
                .as_deref()
                .map(|t| format!(" [{}]", t))
                .unwrap_or_default();
            out.push_str(&format!(
                "  • {}{} — {}\n",
                f.title, tool_tag, f.recommended_action
            ));
        }
        out.push('\n');
    }

    // -- Cross-references: find tools whose findings overlap --
    if distinct_tools.len() > 1 {
        // Collect tool→titles mapping for cross-reference hints.
        let mut tool_titles: HashMap<&str, Vec<&str>> = HashMap::new();
        for f in &sorted {
            if let Some(tool) = f.evidence_pointer.tool.as_deref() {
                tool_titles.entry(tool).or_default().push(&f.title);
            }
        }
        if tool_titles.len() > 1 {
            out.push_str("CROSS-REFERENCES:\n");
            let tools_vec: Vec<&&str> = tool_titles.keys().collect();
            out.push_str(&format!(
                "  Data was collected from {} sources ({}). ",
                tools_vec.len(),
                tools_vec.iter().map(|t| **t).collect::<Vec<_>>().join(", ")
            ));
            out.push_str("Correlate findings across tools for a complete picture.\n\n");
        }
    }

    // -- Risk assessment --
    out.push_str(&format!(
        "OVERALL RISK: {}\n\n",
        max_sev.token().to_uppercase()
    ));

    // -- Prioritized actions (deduplicated, urgent first) --
    out.push_str("RECOMMENDED ACTIONS (priority order):\n");
    let mut seen_actions: HashSet<&str> = HashSet::new();
    let mut action_idx = 0usize;
    for f in &sorted {
        let action = f.recommended_action.as_str();
        if seen_actions.insert(action) {
            action_idx += 1;
            out.push_str(&format!("  {}. {}\n", action_idx, action));
        }
    }

    // Remove trailing newline for clean output.
    if out.ends_with('\n') {
        out.truncate(out.len() - 1);
    }

    out
}

/// Model capability report for JSON output (#80).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilityReport {
    pub tier: ModelCapabilityTier,
    pub estimated_params_b: f32,
    pub execution_provider: String,
    pub smoke_latency_ms: u64,
    pub vocab_size: usize,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub r#override: bool,
    /// Warning emitted when the model is likely too small for ReAct (#120).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

impl ModelCapabilityReport {
    pub fn from_probe(probe: &ModelCapabilityProbe, tier: ModelCapabilityTier) -> Self {
        let warning = if tier >= ModelCapabilityTier::Moderate
            && probe.estimated_param_billions > 0.0
            && probe.estimated_param_billions < 3.0
        {
            Some(format!(
                "Model estimated at {:.1}B params — ReAct tool-use may be unreliable below 7B. \
                 Consider --capability-override basic for deterministic mode.",
                probe.estimated_param_billions,
            ))
        } else {
            None
        };
        Self {
            tier,
            estimated_params_b: probe.estimated_param_billions,
            execution_provider: probe.execution_provider.clone(),
            smoke_latency_ms: probe.smoke_latency_ms,
            vocab_size: probe.vocab_size,
            r#override: false,
            warning,
        }
    }
}

/// Compute a confidence score from a count signal and return the derivation factors (#195).
///
/// Splits the score into three transparent factors so downstream consumers
/// can inspect why a finding scored where it did:
///   * `base_rate` — the prior contributed by the rule itself
///   * `count_signal` — additional weight from the observed count
///   * `ceiling_clip` — negative correction when the raw sum was clipped to the ceiling
fn confidence_from_count_with_factors(
    base: f32,
    count: u64,
    slope: f32,
    ceiling: f32,
    field: &str,
) -> (f32, Vec<ConfidenceFactor>) {
    let count_contribution = count as f32 * slope;
    let raw = base + count_contribution;
    let clipped = raw.min(ceiling);
    let final_score = (clipped * 100.0).round() / 100.0;

    let mut factors = vec![
        ConfidenceFactor::new("base_rate", base, format!("rule prior for {field}")),
        ConfidenceFactor::new(
            "count_signal",
            count_contribution,
            format!("count={count}, slope={slope}"),
        ),
    ];
    if raw > ceiling {
        factors.push(ConfidenceFactor::new(
            "ceiling_clip",
            ceiling - raw,
            format!("clipped to ceiling {ceiling}"),
        ));
    }
    (final_score, factors)
}

/// Build the single-factor list for a fixed-confidence finding where the
/// score is hardcoded (e.g. tool errors, baseline captures, hash captures).
fn fixed_confidence_factor(score: f32, reason: &str) -> Vec<ConfidenceFactor> {
    vec![ConfidenceFactor::new(
        "rule_prior",
        score,
        reason.to_string(),
    )]
}

/// Extract top N items from a JSON string array field, returning a summary like
/// "item1, item2, item3 and 5 more".
fn extract_string_list_summary(observation: &Value, field: &str, max: usize) -> String {
    let items: Vec<&str> = observation
        .get(field)
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).take(max + 1).collect())
        .unwrap_or_default();

    if items.is_empty() {
        return String::new();
    }

    let total = observation
        .get(field)
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    let shown: Vec<&str> = items.into_iter().take(max).collect();

    if total > max {
        format!("{} and {} more", shown.join(", "), total - max)
    } else {
        shown.join(", ")
    }
}

/// Extract top N persistence entries, showing the entry string of each.
fn extract_persistence_summary(observation: &Value, field: &str, max: usize) -> String {
    let entries: Vec<&str> = observation
        .get(field)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.get("entry")
                        .and_then(Value::as_str)
                        .or_else(|| v.as_str())
                })
                .take(max + 1)
                .collect()
        })
        .unwrap_or_default();

    if entries.is_empty() {
        return String::new();
    }

    let total = observation
        .get(field)
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    let shown: Vec<&str> = entries.into_iter().take(max).collect();

    if total > max {
        format!("{} and {} more", shown.join("; "), total - max)
    } else {
        shown.join("; ")
    }
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
            elapsed_ms: None,
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
            elapsed_ms: None,
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
            elapsed_ms: None,
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
            elapsed_ms: None,
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
            elapsed_ms: None,
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
            elapsed_ms: None,
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
            elapsed_ms: None,
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
        Finding::new(
            title.to_string(),
            severity,
            confidence,
            EvidencePointer {
                turn: Some(1),
                tool: Some(tool.to_string()),
                field: field.to_string(),
            },
            "Investigate further.".to_string(),
        )
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

    // ── Capability tiering tests (#77) ──

    use super::{
        basic_tier_summary, basic_tier_summary_for_task, classify_capability,
        ModelCapabilityReport, ModelCapabilityTier,
    };
    use inference_bridge::ModelCapabilityProbe;

    #[test]
    fn classify_small_model_as_basic() {
        // With PARAM_BASIC_CEILING_B = 1.0 (#157), a 0.5B model is Basic.
        let probe = ModelCapabilityProbe {
            estimated_param_billions: 0.5,
            execution_provider: "CPUExecutionProvider".to_string(),
            smoke_latency_ms: 80,
            vocab_size: 32000,
        };
        assert_eq!(classify_capability(&probe), ModelCapabilityTier::Basic);
    }

    #[test]
    fn classify_1b_model_as_moderate() {
        // With PARAM_BASIC_CEILING_B = 1.0 (#157), a 1.2B model reaches Moderate.
        let probe = ModelCapabilityProbe {
            estimated_param_billions: 1.2,
            execution_provider: "CPUExecutionProvider".to_string(),
            smoke_latency_ms: 80,
            vocab_size: 32000,
        };
        assert_eq!(classify_capability(&probe), ModelCapabilityTier::Moderate);
    }

    #[test]
    fn classify_medium_model_moderate_latency_as_moderate() {
        let probe = ModelCapabilityProbe {
            estimated_param_billions: 7.0,
            execution_provider: "CPUExecutionProvider".to_string(),
            smoke_latency_ms: 120,
            vocab_size: 32000,
        };
        assert_eq!(classify_capability(&probe), ModelCapabilityTier::Moderate);
    }

    #[test]
    fn classify_large_model_fast_gpu_as_strong() {
        let probe = ModelCapabilityProbe {
            estimated_param_billions: 13.0,
            execution_provider: "CUDAExecutionProvider".to_string(),
            smoke_latency_ms: 30,
            vocab_size: 128256,
        };
        assert_eq!(classify_capability(&probe), ModelCapabilityTier::Strong);
    }

    #[test]
    fn classify_large_model_slow_cpu_as_basic() {
        // 13B model but 250ms/tok latency → latency-constrained → Basic.
        let probe = ModelCapabilityProbe {
            estimated_param_billions: 13.0,
            execution_provider: "CPUExecutionProvider".to_string(),
            smoke_latency_ms: 250,
            vocab_size: 32000,
        };
        assert_eq!(classify_capability(&probe), ModelCapabilityTier::Basic);
    }

    #[test]
    fn classify_small_model_fast_gpu_as_basic() {
        // 1B model on fast GPU → param-constrained → Basic.
        let probe = ModelCapabilityProbe {
            estimated_param_billions: 0.8,
            execution_provider: "CUDAExecutionProvider".to_string(),
            smoke_latency_ms: 10,
            vocab_size: 32000,
        };
        assert_eq!(classify_capability(&probe), ModelCapabilityTier::Basic);
    }

    #[test]
    fn classify_boundary_2b_model_as_moderate() {
        let probe = ModelCapabilityProbe {
            estimated_param_billions: 2.0,
            execution_provider: "CPUExecutionProvider".to_string(),
            smoke_latency_ms: 100,
            vocab_size: 32000,
        };
        assert_eq!(classify_capability(&probe), ModelCapabilityTier::Moderate);
    }

    #[test]
    fn classify_boundary_latency_200ms_as_moderate() {
        let probe = ModelCapabilityProbe {
            estimated_param_billions: 5.0,
            execution_provider: "CPUExecutionProvider".to_string(),
            smoke_latency_ms: 200,
            vocab_size: 32000,
        };
        assert_eq!(classify_capability(&probe), ModelCapabilityTier::Moderate);
    }

    #[test]
    fn tier_ordering() {
        assert!(ModelCapabilityTier::Basic < ModelCapabilityTier::Moderate);
        assert!(ModelCapabilityTier::Moderate < ModelCapabilityTier::Strong);
    }

    #[test]
    fn tier_serializes_lowercase() {
        let json = serde_json::to_string(&ModelCapabilityTier::Basic).unwrap();
        assert_eq!(json, "\"basic\"");
        let json = serde_json::to_string(&ModelCapabilityTier::Strong).unwrap();
        assert_eq!(json, "\"strong\"");
    }

    #[test]
    fn tier_deserializes_from_lowercase() {
        let tier: ModelCapabilityTier = serde_json::from_str("\"moderate\"").unwrap();
        assert_eq!(tier, ModelCapabilityTier::Moderate);
    }

    // ── Basic tier summary tests (#79) ──

    #[test]
    fn basic_tier_summary_empty_findings() {
        let summary = basic_tier_summary(&[]);
        assert!(summary.starts_with("SUMMARY: 0 findings detected."));
        assert!(summary.contains("RISK: info"));
    }

    #[test]
    fn basic_tier_summary_with_findings() {
        let findings = vec![
            make_finding(
                "Active listeners",
                FindingSeverity::High,
                0.80,
                "scan_network",
                "observation.listener_count",
            ),
            make_finding(
                "Suspicious persistence",
                FindingSeverity::Medium,
                0.70,
                "inspect_persistence_locations",
                "observation.suspicious_entry_count",
            ),
        ];

        let summary = basic_tier_summary(&findings);
        // Updated format groups by severity and provides cross-references (#155).
        assert!(summary.contains("INVESTIGATION SUMMARY"));
        assert!(summary.contains("2 findings"));
        assert!(summary.contains("HIGH"));
        assert!(summary.contains("Active listeners"));
        assert!(summary.contains("Suspicious persistence"));
        assert!(summary.contains("OVERALL RISK: HIGH"));
        assert!(summary.contains("RECOMMENDED ACTIONS"));
        assert!(summary.contains("CROSS-REFERENCES"));
    }

    #[test]
    fn basic_tier_summary_is_deterministic() {
        let findings = vec![make_finding(
            "Test",
            FindingSeverity::Low,
            0.50,
            "scan_network",
            "a",
        )];
        let a = basic_tier_summary(&findings);
        let b = basic_tier_summary(&findings);
        assert_eq!(a, b);
    }

    #[test]
    fn basic_tier_summary_for_task_includes_task_name() {
        let findings = vec![make_finding(
            "Priv-esc detected",
            FindingSeverity::High,
            0.85,
            "check_privilege_escalation_vectors",
            "observation.indicator_count",
        )];
        let summary = basic_tier_summary_for_task(&findings, Some("windows-triage"));
        // Updated format includes task name in header (#155).
        assert!(summary.contains("windows-triage"));
        assert!(summary.contains("1 tool(s)"));
        assert!(summary.contains("RECOMMENDED ACTIONS"));
    }

    // ── Discrete confidence label tests (#85) ──

    use super::{confidence_to_label, FindingConfidence, FindingRelevance};

    #[test]
    fn confidence_label_confirmed_threshold() {
        assert_eq!(confidence_to_label(0.90), FindingConfidence::Confirmed);
        assert_eq!(confidence_to_label(0.99), FindingConfidence::Confirmed);
        assert_eq!(confidence_to_label(1.0), FindingConfidence::Confirmed);
    }

    #[test]
    fn confidence_label_likely_threshold() {
        assert_eq!(confidence_to_label(0.72), FindingConfidence::Likely);
        assert_eq!(confidence_to_label(0.89), FindingConfidence::Likely);
    }

    #[test]
    fn confidence_label_possible_threshold() {
        assert_eq!(confidence_to_label(0.55), FindingConfidence::Possible);
        assert_eq!(confidence_to_label(0.71), FindingConfidence::Possible);
    }

    #[test]
    fn confidence_label_informational_threshold() {
        assert_eq!(confidence_to_label(0.54), FindingConfidence::Informational);
        assert_eq!(confidence_to_label(0.10), FindingConfidence::Informational);
        assert_eq!(confidence_to_label(0.0), FindingConfidence::Informational);
    }

    #[test]
    fn finding_new_auto_derives_confidence_label() {
        let finding = make_finding("Test", FindingSeverity::High, 0.92, "scan_network", "a");
        assert_eq!(finding.confidence_label, FindingConfidence::Confirmed);

        let finding = make_finding("Test", FindingSeverity::Low, 0.60, "scan_network", "b");
        assert_eq!(finding.confidence_label, FindingConfidence::Possible);
    }

    #[test]
    fn finding_confidence_label_ordering() {
        assert!(FindingConfidence::Confirmed > FindingConfidence::Likely);
        assert!(FindingConfidence::Likely > FindingConfidence::Possible);
        assert!(FindingConfidence::Possible > FindingConfidence::Informational);
    }

    #[test]
    fn finding_confidence_serializes_lowercase() {
        let json = serde_json::to_string(&FindingConfidence::Confirmed).unwrap();
        assert_eq!(json, "\"confirmed\"");
        let json = serde_json::to_string(&FindingConfidence::Informational).unwrap();
        assert_eq!(json, "\"informational\"");
    }

    #[test]
    fn finding_relevance_default_is_primary() {
        assert_eq!(FindingRelevance::default(), FindingRelevance::Primary);
    }

    #[test]
    fn finding_confidence_label_in_json_output() {
        let finding = make_finding("Test", FindingSeverity::High, 0.95, "scan_network", "a");
        let json = serde_json::to_string(&finding).unwrap();
        assert!(json.contains("\"confidence_label\":\"confirmed\""));
        assert!(json.contains("\"relevance\":\"primary\""));
    }

    #[test]
    fn derive_findings_backfills_confidence_labels() {
        let turns = vec![AgentTurn {
            thought: "<call>{...}</call>".to_string(),
            tool_call: Some(ToolCall {
                tool: "check_privilege_escalation_vectors".to_string(),
                args: json!({}),
            }),
            observation: Some(json!({
                "indicator_count": 2,
            })),
            elapsed_ms: None,
        }];

        let findings = derive_findings(&turns, "");
        for finding in &findings {
            assert_eq!(
                finding.confidence_label,
                confidence_to_label(finding.confidence),
                "confidence_label should match float for '{}'",
                finding.title
            );
        }
    }

    // ── ModelCapabilityReport tests (#80) ──

    #[test]
    fn capability_report_from_probe_roundtrips_json() {
        let probe = ModelCapabilityProbe {
            estimated_param_billions: 1.2,
            execution_provider: "CPUExecutionProvider".to_string(),
            smoke_latency_ms: 350,
            vocab_size: 32000,
        };
        let report = ModelCapabilityReport::from_probe(&probe, ModelCapabilityTier::Basic);
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("\"tier\": \"basic\""));
        assert!(json.contains("\"estimated_params_b\""));
        assert!(json.contains("\"execution_provider\""));
        assert!(json.contains("\"smoke_latency_ms\""));
        assert!(json.contains("\"vocab_size\""));
        // override should be absent when false
        assert!(!json.contains("\"override\""));
    }

    #[test]
    fn capability_report_override_flag_serialized() {
        let probe = ModelCapabilityProbe {
            estimated_param_billions: 1.2,
            execution_provider: "CPUExecutionProvider".to_string(),
            smoke_latency_ms: 350,
            vocab_size: 32000,
        };
        let mut report = ModelCapabilityReport::from_probe(&probe, ModelCapabilityTier::Strong);
        report.r#override = true;
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("\"override\": true"));
        assert!(json.contains("\"tier\": \"strong\""));
    }
}
