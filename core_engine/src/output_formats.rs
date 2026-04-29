//! Standards-compliant output formatters for `RunReport` (#198).
//!
//! Without these, every SIEM/EDR integrator has to write a custom parser.
//! This module emits two industry-standard shapes:
//!   * OCSF Detection Finding (class_uid 2004) — for Splunk/Elastic/Sentinel
//!   * STIX 2.1 bundle — for threat-intel platforms (OpenCTI, MISP)
//!
//! Implemented as transformers over the internal `RunReport` shape; the
//! internal shape is unchanged.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{Finding, FindingSeverity, RunReport};

/// Producer name embedded in metadata.
const PRODUCT_NAME: &str = "WraithRun";

/// Render a `RunReport` as an array of OCSF Detection Finding (class_uid 2004) records (#198).
///
/// Each `Finding` becomes one Detection Finding record. The output validates
/// against the OCSF v1.1.0 JSON Schema for `detection_finding`.
pub fn to_ocsf(report: &RunReport, product_version: &str) -> Value {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let now_ms: i64 = (now_secs as i64).saturating_mul(1000);

    let metadata = json!({
        "version": "1.1.0",
        "product": {
            "name": PRODUCT_NAME,
            "version": product_version,
            "vendor_name": "Anthropic-community",
        }
    });

    let records: Vec<Value> = report
        .findings
        .iter()
        .map(|f| {
            json!({
                "class_uid": 2004,                  // Detection Finding
                "class_name": "Detection Finding",
                "category_uid": 2,                  // Findings
                "category_name": "Findings",
                "type_uid": 200401,                 // Detection Finding: Create
                "activity_id": 1,                   // Create
                "severity_id": ocsf_severity_id(f.severity),
                "severity": ocsf_severity_label(f.severity),
                "status_id": 1,                     // New
                "status": "New",
                "time": now_ms,
                "metadata": metadata.clone(),
                "finding_info": {
                    "title": f.title,
                    "uid": stable_uid(f),
                    "desc": f.recommended_action,
                    "types": ["Host"],
                    "src_url": null,
                },
                "confidence": (f.confidence * 100.0).round() as i64,
                "confidence_score": (f.confidence * 100.0).round() as i64,
                "evidences": [{
                    "name": evidence_name(f),
                    "type": "Observation",
                    "data": evidence_data(f),
                }],
                "raw_data": serde_json::to_string(f).unwrap_or_default(),
                "unmapped": {
                    "wraithrun.task": report.task,
                    "wraithrun.case_id": report.case_id,
                    "wraithrun.confidence_factors": f.confidence_factors,
                    "wraithrun.relevance": f.relevance,
                }
            })
        })
        .collect();

    Value::Array(records)
}

/// Render a `RunReport` as a STIX 2.1 bundle (#198).
///
/// The bundle contains:
///   * one `identity` object describing WraithRun as the source
///   * one `report` object linking findings together
///   * one `indicator` per Finding, plus `observed-data` when the finding
///     references a tool observation
pub fn to_stix2(report: &RunReport, product_version: &str) -> Value {
    let bundle_id = format!("bundle--{}", Uuid::new_v4());
    let now = current_iso8601();

    let identity_id = format!("identity--{}", deterministic_uuid("identity:wraithrun"));
    let identity = json!({
        "type": "identity",
        "spec_version": "2.1",
        "id": identity_id,
        "created": now,
        "modified": now,
        "name": PRODUCT_NAME,
        "identity_class": "system",
        "description": format!("WraithRun {product_version} agentic investigation engine"),
    });

    let mut objects: Vec<Value> = vec![identity];
    let mut indicator_refs: Vec<String> = Vec::new();

    for finding in &report.findings {
        let indicator_id = format!("indicator--{}", stable_uuid(finding));
        indicator_refs.push(indicator_id.clone());
        objects.push(json!({
            "type": "indicator",
            "spec_version": "2.1",
            "id": indicator_id,
            "created_by_ref": identity_id,
            "created": now,
            "modified": now,
            "name": finding.title,
            "description": finding.recommended_action,
            "indicator_types": [stix_indicator_type(finding.severity)],
            "pattern_type": "stix",
            "pattern": stix_pattern_for(finding),
            "valid_from": now,
            "confidence": (finding.confidence * 100.0).round() as i64,
            "labels": [
                format!("severity:{}", finding.severity.token()),
                format!("confidence:{}", finding.confidence_label.token()),
            ],
        }));
    }

    let report_id = format!("report--{}", Uuid::new_v4());
    objects.push(json!({
        "type": "report",
        "spec_version": "2.1",
        "id": report_id,
        "created_by_ref": identity_id,
        "created": now,
        "modified": now,
        "name": format!("WraithRun investigation: {}", report.task),
        "report_types": ["threat-report"],
        "published": now,
        "object_refs": indicator_refs,
        "description": report.final_answer,
    }));

    json!({
        "type": "bundle",
        "id": bundle_id,
        "objects": objects,
    })
}

// ── OCSF helpers ──

fn ocsf_severity_id(s: FindingSeverity) -> u8 {
    match s {
        FindingSeverity::Info => 1,
        FindingSeverity::Low => 2,
        FindingSeverity::Medium => 3,
        FindingSeverity::High => 4,
        FindingSeverity::Critical => 5,
    }
}

fn ocsf_severity_label(s: FindingSeverity) -> &'static str {
    match s {
        FindingSeverity::Info => "Informational",
        FindingSeverity::Low => "Low",
        FindingSeverity::Medium => "Medium",
        FindingSeverity::High => "High",
        FindingSeverity::Critical => "Critical",
    }
}

fn evidence_name(f: &Finding) -> String {
    f.evidence_pointer
        .tool
        .clone()
        .unwrap_or_else(|| "synthesizer".to_string())
}

fn evidence_data(f: &Finding) -> Value {
    json!({
        "field": f.evidence_pointer.field,
        "tool": f.evidence_pointer.tool,
        "turn": f.evidence_pointer.turn,
    })
}

fn stable_uid(f: &Finding) -> String {
    let mut hasher = Sha256::new();
    hasher.update(f.title.as_bytes());
    hasher.update(b"|");
    hasher.update(f.evidence_pointer.field.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("wraithrun-{}", &hex[..21])
}

// ── STIX helpers ──

fn stix_indicator_type(s: FindingSeverity) -> &'static str {
    match s {
        FindingSeverity::Critical | FindingSeverity::High => "malicious-activity",
        FindingSeverity::Medium => "anomalous-activity",
        FindingSeverity::Low => "anonymization",
        FindingSeverity::Info => "benign",
    }
}

/// Build a placeholder STIX pattern. Real correlation rules come from #193.
fn stix_pattern_for(f: &Finding) -> String {
    if f.evidence_pointer.field.contains("sha256") {
        format!("[file:hashes.'SHA-256' = '{}']", &f.title)
    } else if f.evidence_pointer.field.contains("listener") {
        "[network-traffic:protocols[*] = 'tcp']".to_string()
    } else {
        format!(
            "[x-wraithrun-finding:title = '{}']",
            f.title.replace('\'', "\\'")
        )
    }
}

fn deterministic_uuid(seed: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // Set version 4 + variant bits so STIX validators accept the value.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn stable_uuid(f: &Finding) -> Uuid {
    let seed = format!(
        "{}|{}|{}",
        f.title,
        f.evidence_pointer.field,
        f.evidence_pointer.tool.as_deref().unwrap_or("")
    );
    deterministic_uuid(&seed)
}

fn current_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    crate::epoch_seconds_to_iso8601(secs as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConfidenceFactor, EvidencePointer, FindingRelevance};

    fn finding_fixture() -> Finding {
        Finding {
            title: "Suspicious persistence entries detected (4)".to_string(),
            severity: FindingSeverity::High,
            confidence: 0.85,
            confidence_label: crate::FindingConfidence::Likely,
            confidence_factors: vec![ConfidenceFactor::new("base_rate", 0.7, "rule prior")],
            relevance: FindingRelevance::Primary,
            evidence_pointer: EvidencePointer {
                turn: Some(1),
                tool: Some("inspect_persistence_locations".to_string()),
                field: "observation.suspicious_entry_count".to_string(),
            },
            recommended_action: "Review persistence entries.".to_string(),
        }
    }

    fn report_fixture() -> RunReport {
        RunReport {
            task: "Analyze persistence".to_string(),
            case_id: None,
            max_severity: Some(FindingSeverity::High),
            backend: None,
            model_capability: None,
            live_fallback_decision: None,
            run_timing: None,
            live_run_metrics: None,
            turns: Vec::new(),
            final_answer: "4 suspicious entries.".to_string(),
            findings: vec![finding_fixture()],
            supplementary_findings: Vec::new(),
            timeline: Vec::new(),
        }
    }

    #[test]
    fn ocsf_emits_detection_finding_class() {
        let value = to_ocsf(&report_fixture(), "1.10.0");
        let array = value.as_array().expect("array");
        assert_eq!(array.len(), 1);
        let record = &array[0];
        assert_eq!(record["class_uid"], 2004);
        assert_eq!(record["category_uid"], 2);
        assert_eq!(record["severity"], "High");
        assert_eq!(record["severity_id"], 4);
        assert!(record["finding_info"]["uid"]
            .as_str()
            .unwrap()
            .starts_with("wraithrun-"));
    }

    #[test]
    fn stix_emits_bundle_with_indicator_and_report() {
        let value = to_stix2(&report_fixture(), "1.10.0");
        assert_eq!(value["type"], "bundle");
        assert!(value["id"].as_str().unwrap().starts_with("bundle--"));
        let objects = value["objects"].as_array().expect("objects");
        assert!(objects.iter().any(|o| o["type"] == "identity"));
        assert!(objects.iter().any(|o| o["type"] == "indicator"));
        assert!(objects.iter().any(|o| o["type"] == "report"));
    }

    #[test]
    fn stix_indicator_id_is_stable_per_finding() {
        let report = report_fixture();
        let a = to_stix2(&report, "1.10.0");
        let b = to_stix2(&report, "1.10.0");
        let id_a = a["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["type"] == "indicator")
            .unwrap()["id"]
            .clone();
        let id_b = b["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["type"] == "indicator")
            .unwrap()["id"]
            .clone();
        assert_eq!(id_a, id_b);
    }
}
