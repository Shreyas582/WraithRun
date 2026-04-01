use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use serde_json::Value;
use toml::Value as TomlValue;

fn run_with_stdin(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_wraithrun"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn wraithrun binary");

    {
        let stdin = child
            .stdin
            .as_mut()
            .expect("child stdin should be available");
        stdin
            .write_all(input.as_bytes())
            .expect("failed to write stdin input");
    }

    child
        .wait_with_output()
        .expect("failed waiting for process output")
}

fn run_capture(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wraithrun"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to execute wraithrun")
}

fn parse_stdout_json(output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).expect("stdout should be valid JSON")
}

#[test]
fn accepts_task_from_stdin() {
    let output = run_with_stdin(
        &["--task-stdin", "--format", "summary"],
        "Check suspicious listener ports and summarize risk",
    );

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Task: Check suspicious listener ports and summarize risk"));
    assert!(stdout.contains("Turns:"));
}

#[test]
fn accepts_task_file_dash_from_stdin() {
    let output = run_with_stdin(
        &["--task-file", "-", "--format", "summary"],
        "Investigate unauthorized SSH keys",
    );

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Task: Investigate unauthorized SSH keys"));
}

#[test]
fn report_json_contract_contains_findings_layer() {
    let output = run_capture(&["--task", "Investigate unauthorized SSH keys"]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    assert_eq!(
        json.get("contract_version").and_then(Value::as_str),
        Some("1.0.0")
    );
    let findings = json
        .get("findings")
        .and_then(Value::as_array)
        .expect("findings should be an array");
    assert!(!findings.is_empty(), "findings should not be empty");

    let first = findings[0]
        .as_object()
        .expect("first finding should be an object");
    assert!(
        first.get("severity").and_then(Value::as_str).is_some(),
        "finding severity should be present"
    );
    assert!(
        first.get("confidence").and_then(Value::as_f64).is_some(),
        "finding confidence should be present"
    );
    assert!(
        first
            .get("recommended_action")
            .and_then(Value::as_str)
            .is_some(),
        "finding recommended_action should be present"
    );

    let evidence = first
        .get("evidence_pointer")
        .and_then(Value::as_object)
        .expect("evidence_pointer should be an object");
    assert!(
        evidence.get("field").and_then(Value::as_str).is_some(),
        "evidence_pointer.field should be present"
    );
}

#[test]
fn automation_adapter_outputs_findings_envelope_json() {
    let output = run_capture(&[
        "--task",
        "Hash ./README.md and report integrity context",
        "--automation-adapter",
        "findings-v1",
    ]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    assert_eq!(
        json.get("contract_version").and_then(Value::as_str),
        Some("1.0.0")
    );
    assert_eq!(
        json.get("adapter").and_then(Value::as_str),
        Some("findings-v1")
    );

    let summary = json
        .get("summary")
        .and_then(Value::as_object)
        .expect("summary should be an object");
    assert!(
        summary
            .get("finding_count")
            .and_then(Value::as_u64)
            .is_some(),
        "summary.finding_count should be present"
    );

    let findings = json
        .get("findings")
        .and_then(Value::as_array)
        .expect("findings should be an array");
    assert!(!findings.is_empty(), "adapter findings should not be empty");
    assert!(findings[0]
        .get("finding_id")
        .and_then(Value::as_str)
        .is_some());
}

#[test]
fn exit_policy_fails_when_threshold_is_met() {
    let output = run_capture(&[
        "--task",
        "Hash ./README.md and report integrity context",
        "--exit-policy",
        "severity-threshold",
        "--exit-threshold",
        "info",
    ]);

    assert!(
        !output.status.success(),
        "process should fail when exit policy threshold is met"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("exit policy triggered"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"contract_version\": \"1.0.0\""));
}

#[test]
fn exit_policy_passes_when_threshold_not_met() {
    let output = run_capture(&[
        "--task",
        "Hash ./README.md and report integrity context",
        "--exit-policy",
        "severity-threshold",
        "--exit-threshold",
        "critical",
    ]);

    assert!(
        output.status.success(),
        "process should pass when threshold is not met: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn live_mode_falls_back_to_dry_run_when_policy_enabled() {
    let missing_model = unique_temp_dir("wraithrun-live-fallback-enabled").join("missing.onnx");
    let missing_model_text = missing_model.to_string_lossy().to_string();

    let args = vec![
        "--task",
        "Investigate unauthorized SSH keys",
        "--live",
        "--model",
        missing_model_text.as_str(),
        "--live-fallback-policy",
        "dry-run-on-error",
    ];
    let output = run_capture(&args);

    assert!(
        output.status.success(),
        "process should recover with fallback: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    let decision = json
        .get("live_fallback_decision")
        .and_then(Value::as_object)
        .expect("live_fallback_decision should be present");
    assert_eq!(
        decision.get("policy").and_then(Value::as_str),
        Some("dry-run-on-error")
    );
    assert_eq!(
        decision.get("fallback_mode").and_then(Value::as_str),
        Some("dry-run")
    );
    let reason_code = decision
        .get("reason_code")
        .and_then(Value::as_str)
        .expect("fallback reason_code should be present");
    assert!(
        [
            "model_path_missing",
            "live_runtime_error",
            "tokenizer_path_missing",
            "tokenizer_json_invalid",
            "permission_denied",
            "unknown_live_error"
        ]
        .contains(&reason_code),
        "unexpected fallback reason code: {reason_code}"
    );

    let findings = json
        .get("findings")
        .and_then(Value::as_array)
        .expect("findings should be present");
    assert!(findings.iter().any(|finding| {
        finding
            .get("evidence_pointer")
            .and_then(Value::as_object)
            .and_then(|pointer| pointer.get("field"))
            .and_then(Value::as_str)
            == Some("live_fallback_decision.live_error")
    }));
}

#[test]
fn doctor_live_fix_auto_discovers_tokenizer_and_sets_fallback_policy() {
    let fixture_dir = unique_temp_dir("wraithrun-doctor-live-fix");
    fs::create_dir_all(&fixture_dir).expect("fixture directory should be created");

    let model_path = fixture_dir.join("sample-model.onnx");
    fs::write(&model_path, b"onnx-fixture").expect("model fixture should be written");

    let tokenizer_path = fixture_dir.join("tokenizer.json");
    fs::write(&tokenizer_path, r#"{"model":{"type":"WordPiece"}}"#)
        .expect("tokenizer fixture should be written");

    let model_path_text = model_path.to_string_lossy().to_string();
    let args = vec![
        "--doctor",
        "--live",
        "--fix",
        "--model",
        model_path_text.as_str(),
        "--introspection-format",
        "json",
    ];
    let output = run_capture(&args);

    assert!(
        output.status.success(),
        "doctor fix run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    let checks = json
        .get("checks")
        .and_then(Value::as_array)
        .expect("checks should be an array");

    assert!(checks.iter().any(|check| {
        check.get("name") == Some(&Value::String("fix-live-tokenizer-path".to_string()))
            && check.get("status") == Some(&Value::String("pass".to_string()))
            && check.get("reason_code")
                == Some(&Value::String("tokenizer_path_auto_discovered".to_string()))
    }));

    assert!(checks.iter().any(|check| {
        check.get("name") == Some(&Value::String("fix-live-fallback-policy".to_string()))
            && check.get("status") == Some(&Value::String("pass".to_string()))
            && check.get("reason_code")
                == Some(&Value::String("fallback_policy_auto_enabled".to_string()))
    }));

    assert!(checks.iter().any(|check| {
        check.get("name") == Some(&Value::String("live-tokenizer-path".to_string()))
            && check.get("status") == Some(&Value::String("pass".to_string()))
    }));

    let _ = fs::remove_dir_all(&fixture_dir);
}

#[test]
fn doctor_live_fix_emits_reason_code_for_explicit_tokenizer_path_failure() {
    let fixture_dir = unique_temp_dir("wraithrun-doctor-live-fix-explicit-tokenizer");
    fs::create_dir_all(&fixture_dir).expect("fixture directory should be created");

    let model_path = fixture_dir.join("sample-model.onnx");
    fs::write(&model_path, b"onnx-fixture").expect("model fixture should be written");

    let missing_tokenizer = fixture_dir.join("missing-tokenizer.json");
    let model_path_text = model_path.to_string_lossy().to_string();
    let missing_tokenizer_text = missing_tokenizer.to_string_lossy().to_string();

    let args = vec![
        "--doctor",
        "--live",
        "--fix",
        "--model",
        model_path_text.as_str(),
        "--tokenizer",
        missing_tokenizer_text.as_str(),
        "--introspection-format",
        "json",
    ];
    let output = run_capture(&args);

    assert!(
        output.status.success(),
        "doctor fix run should complete with warning guidance: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    let checks = json
        .get("checks")
        .and_then(Value::as_array)
        .expect("checks should be an array");

    assert!(checks.iter().any(|check| {
        check.get("name") == Some(&Value::String("fix-live-tokenizer-path".to_string()))
            && check.get("status") == Some(&Value::String("warn".to_string()))
            && check.get("reason_code")
                == Some(&Value::String("tokenizer_path_missing".to_string()))
    }));

    let _ = fs::remove_dir_all(&fixture_dir);
}

#[test]
fn live_mode_without_fallback_policy_propagates_error() {
    let missing_model = unique_temp_dir("wraithrun-live-fallback-disabled").join("missing.onnx");
    let missing_model_text = missing_model.to_string_lossy().to_string();

    let args = vec![
        "--task",
        "Investigate unauthorized SSH keys",
        "--live",
        "--model",
        missing_model_text.as_str(),
        "--live-fallback-policy",
        "none",
    ];
    let output = run_capture(&args);

    assert!(
        !output.status.success(),
        "process should fail when fallback policy is none"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.trim().is_empty(),
        "expected non-empty stderr for live failure"
    );
}

#[test]
fn adapter_output_includes_fallback_decision_when_triggered() {
    let missing_model = unique_temp_dir("wraithrun-live-fallback-adapter").join("missing.onnx");
    let missing_model_text = missing_model.to_string_lossy().to_string();

    let args = vec![
        "--task",
        "Investigate unauthorized SSH keys",
        "--live",
        "--model",
        missing_model_text.as_str(),
        "--live-fallback-policy",
        "dry-run-on-error",
        "--automation-adapter",
        "findings-v1",
    ];
    let output = run_capture(&args);

    assert!(
        output.status.success(),
        "adapter run should recover with fallback: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    let summary = json
        .get("summary")
        .and_then(Value::as_object)
        .expect("summary should be present");
    let decision = summary
        .get("live_fallback_decision")
        .and_then(Value::as_object)
        .expect("summary.live_fallback_decision should be present");
    assert_eq!(
        decision.get("fallback_mode").and_then(Value::as_str),
        Some("dry-run")
    );
}

#[test]
fn report_json_contract_includes_case_id_when_provided() {
    let output = run_capture(&[
        "--task",
        "Investigate unauthorized SSH keys",
        "--case-id",
        "CASE-2026-IR-0001",
    ]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    assert_eq!(
        json.get("case_id").and_then(Value::as_str),
        Some("CASE-2026-IR-0001")
    );
}

#[test]
fn evidence_bundle_export_writes_expected_files() {
    let bundle_dir = unique_temp_dir("wraithrun-evidence-bundle");
    let bundle_dir_text = bundle_dir.to_string_lossy().to_string();
    let args = vec![
        "--task",
        "Investigate unauthorized SSH keys",
        "--case-id",
        "CASE-2026-IR-0002",
        "--evidence-bundle-dir",
        bundle_dir_text.as_str(),
    ];
    let output = run_capture(&args);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report_path = bundle_dir.join("report.json");
    let raw_path = bundle_dir.join("raw_observations.json");
    let sums_path = bundle_dir.join("SHA256SUMS");

    assert!(report_path.is_file(), "report.json should exist");
    assert!(raw_path.is_file(), "raw_observations.json should exist");
    assert!(sums_path.is_file(), "SHA256SUMS should exist");

    let report_json = fs::read_to_string(&report_path).expect("report.json should be readable");
    let report_value: Value =
        serde_json::from_str(&report_json).expect("report.json should be valid JSON");
    assert_eq!(
        report_value.get("case_id").and_then(Value::as_str),
        Some("CASE-2026-IR-0002")
    );

    let checksums = fs::read_to_string(&sums_path).expect("SHA256SUMS should be readable");
    assert!(checksums.contains("report.json"));
    assert!(checksums.contains("raw_observations.json"));

    let _ = fs::remove_dir_all(&bundle_dir);
}

#[test]
fn evidence_bundle_archive_export_writes_expected_artifact() {
    let bundle_dir = unique_temp_dir("wraithrun-evidence-archive");
    fs::create_dir_all(&bundle_dir).expect("archive directory should be created");

    let archive_path = bundle_dir.join("CASE-2026-IR-0003.tar");
    let archive_path_text = archive_path.to_string_lossy().to_string();
    let args = vec![
        "--task",
        "Investigate unauthorized SSH keys",
        "--case-id",
        "CASE-2026-IR-0003",
        "--evidence-bundle-archive",
        archive_path_text.as_str(),
    ];
    let output = run_capture(&args);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(archive_path.is_file(), "archive should be created");

    let archive_file = fs::File::open(&archive_path).expect("archive should be readable");
    let mut archive = tar::Archive::new(archive_file);
    let mut entries = Vec::new();

    for entry in archive.entries().expect("archive entries should load") {
        let entry = entry.expect("archive entry should parse");
        entries.push(
            entry
                .path()
                .expect("entry path should resolve")
                .to_string_lossy()
                .to_string(),
        );
    }

    assert_eq!(
        entries,
        vec![
            "report.json".to_string(),
            "raw_observations.json".to_string(),
            "SHA256SUMS".to_string(),
        ]
    );

    let _ = fs::remove_dir_all(&bundle_dir);
}

#[test]
fn verify_bundle_mode_reports_success_as_json() {
    let bundle_dir = unique_temp_dir("wraithrun-verify-mode-success");
    let bundle_dir_text = bundle_dir.to_string_lossy().to_string();
    let create_args = vec![
        "--task",
        "Investigate unauthorized SSH keys",
        "--evidence-bundle-dir",
        bundle_dir_text.as_str(),
    ];
    let create_output = run_capture(&create_args);

    assert!(
        create_output.status.success(),
        "bundle create process failed: {}",
        String::from_utf8_lossy(&create_output.stderr)
    );

    let verify_args = vec![
        "--verify-bundle",
        bundle_dir_text.as_str(),
        "--introspection-format",
        "json",
    ];
    let verify_output = run_capture(&verify_args);

    assert!(
        verify_output.status.success(),
        "verify process failed: {}",
        String::from_utf8_lossy(&verify_output.stderr)
    );

    let json = parse_stdout_json(&verify_output);
    assert_eq!(
        json.get("contract_version").and_then(Value::as_str),
        Some("1.0.0")
    );
    let summary = json
        .get("summary")
        .and_then(Value::as_object)
        .expect("summary should be an object");
    assert_eq!(summary.get("fail").and_then(Value::as_u64), Some(0));
    assert!(
        summary
            .get("pass")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            >= 2,
        "expected at least two verified files"
    );

    let entries = json
        .get("entries")
        .and_then(Value::as_array)
        .expect("entries should be an array");
    assert!(entries.iter().any(|entry| {
        entry.get("file") == Some(&Value::String("report.json".to_string()))
            && entry.get("status") == Some(&Value::String("pass".to_string()))
    }));

    let _ = fs::remove_dir_all(&bundle_dir);
}

#[test]
fn verify_bundle_mode_fails_when_bundle_is_tampered() {
    let bundle_dir = unique_temp_dir("wraithrun-verify-mode-fail");
    let bundle_dir_text = bundle_dir.to_string_lossy().to_string();
    let create_args = vec![
        "--task",
        "Investigate unauthorized SSH keys",
        "--evidence-bundle-dir",
        bundle_dir_text.as_str(),
    ];
    let create_output = run_capture(&create_args);

    assert!(
        create_output.status.success(),
        "bundle create process failed: {}",
        String::from_utf8_lossy(&create_output.stderr)
    );

    let report_path = bundle_dir.join("report.json");
    fs::write(&report_path, "{\"tampered\":true}\n").expect("tamper write should succeed");

    let verify_args = vec!["--verify-bundle", bundle_dir_text.as_str()];
    let verify_output = run_capture(&verify_args);

    assert!(
        !verify_output.status.success(),
        "verify process should fail for tampered bundle"
    );

    let stdout = String::from_utf8_lossy(&verify_output.stdout);
    assert!(stdout.contains("[FAIL] report.json"));

    let stderr = String::from_utf8_lossy(&verify_output.stderr);
    assert!(stderr.contains("verification failed"));

    let _ = fs::remove_dir_all(&bundle_dir);
}

#[test]
fn verify_bundle_mode_accepts_direct_checksums_path_with_spaces() {
    let bundle_dir = unique_temp_dir("wraithrun verify bundle path edge");
    let bundle_dir_text = bundle_dir.to_string_lossy().to_string();
    let create_args = vec![
        "--task",
        "Investigate unauthorized SSH keys",
        "--evidence-bundle-dir",
        bundle_dir_text.as_str(),
    ];
    let create_output = run_capture(&create_args);

    assert!(
        create_output.status.success(),
        "bundle create process failed: {}",
        String::from_utf8_lossy(&create_output.stderr)
    );

    let checksums_path = bundle_dir.join("SHA256SUMS");
    let checksums_path_text = checksums_path.to_string_lossy().to_string();
    let verify_args = vec![
        "--verify-bundle",
        checksums_path_text.as_str(),
        "--introspection-format",
        "json",
    ];
    let verify_output = run_capture(&verify_args);

    assert!(
        verify_output.status.success(),
        "verify process failed: {}",
        String::from_utf8_lossy(&verify_output.stderr)
    );

    let json = parse_stdout_json(&verify_output);
    assert_eq!(
        json.get("checksums_path").and_then(Value::as_str),
        Some(checksums_path_text.as_str())
    );
    assert_eq!(
        json.get("summary")
            .and_then(Value::as_object)
            .and_then(|summary| summary.get("fail"))
            .and_then(Value::as_u64),
        Some(0)
    );

    let _ = fs::remove_dir_all(&bundle_dir);
}

#[test]
fn verify_bundle_mode_rejects_non_manifest_file_path() {
    let bundle_dir = unique_temp_dir("wraithrun verify bundle invalid file");
    let bundle_dir_text = bundle_dir.to_string_lossy().to_string();
    let create_args = vec![
        "--task",
        "Investigate unauthorized SSH keys",
        "--evidence-bundle-dir",
        bundle_dir_text.as_str(),
    ];
    let create_output = run_capture(&create_args);

    assert!(
        create_output.status.success(),
        "bundle create process failed: {}",
        String::from_utf8_lossy(&create_output.stderr)
    );

    let report_path = bundle_dir.join("report.json");
    let report_path_text = report_path.to_string_lossy().to_string();
    let verify_args = vec!["--verify-bundle", report_path_text.as_str()];
    let verify_output = run_capture(&verify_args);

    assert!(
        !verify_output.status.success(),
        "verify process should fail for non-manifest file path"
    );

    let stderr = String::from_utf8_lossy(&verify_output.stderr);
    assert!(stderr.contains("must point to an evidence bundle directory or a SHA256SUMS file"));

    let _ = fs::remove_dir_all(&bundle_dir);
}

#[test]
fn baseline_bundle_import_populates_drift_tool_arguments() {
    let baseline_dir = unique_temp_dir("wraithrun-baseline-import");
    fs::create_dir_all(&baseline_dir).expect("baseline directory should be created");

    let raw_path = baseline_dir.join("raw_observations.json");
    let raw_content = r#"{
    "task": "Capture baseline",
    "turns": [
        {
            "turn": 1,
            "tool": "capture_coverage_baseline",
            "args": {},
            "observation": {
                "persistence": {"baseline_entries": ["entry-a"]},
                "accounts": {
                    "baseline_privileged_accounts": ["svc-admin"],
                    "approved_privileged_accounts": ["svc-admin"]
                },
                "network": {
                    "baseline_exposed_bindings": ["0.0.0.0:443"],
                    "expected_processes": ["nginx"]
                }
            }
        }
    ]
}"#;
    fs::write(&raw_path, raw_content).expect("baseline fixture should be written");

    let baseline_path_text = baseline_dir.to_string_lossy().to_string();
    let args = vec![
        "--task",
        "Audit account change activity in admin group membership",
        "--baseline-bundle",
        baseline_path_text.as_str(),
    ];
    let output = run_capture(&args);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    let turns = json
        .get("turns")
        .and_then(Value::as_array)
        .expect("turns should be an array");
    let first_args = turns
        .first()
        .and_then(|turn| turn.get("tool_call"))
        .and_then(|call| call.get("args"))
        .and_then(Value::as_object)
        .expect("first tool call args should be an object");

    let baseline_accounts = first_args
        .get("baseline_privileged_accounts")
        .and_then(Value::as_array)
        .expect("baseline_privileged_accounts should be present");
    assert!(baseline_accounts.iter().any(|entry| entry == "svc-admin"));

    let approved_accounts = first_args
        .get("approved_privileged_accounts")
        .and_then(Value::as_array)
        .expect("approved_privileged_accounts should be present");
    assert!(approved_accounts.iter().any(|entry| entry == "svc-admin"));

    let _ = fs::remove_dir_all(&baseline_dir);
}

#[test]
fn baseline_bundle_import_accepts_raw_file_path_with_spaces() {
    let baseline_dir = unique_temp_dir("wraithrun baseline raw file path");
    fs::create_dir_all(&baseline_dir).expect("baseline directory should be created");

    let raw_path = baseline_dir.join("raw_observations.json");
    let raw_content = r#"{
    "task": "Capture baseline",
    "turns": [
        {
            "turn": 1,
            "tool": "capture_coverage_baseline",
            "args": {},
            "observation": {
                "persistence": {"baseline_entries": ["entry-a"]},
                "accounts": {
                    "baseline_privileged_accounts": ["svc-admin"],
                    "approved_privileged_accounts": ["svc-admin"]
                },
                "network": {
                    "baseline_exposed_bindings": ["0.0.0.0:443"],
                    "expected_processes": ["nginx"]
                }
            }
        }
    ]
}"#;
    fs::write(&raw_path, raw_content).expect("baseline fixture should be written");

    let raw_path_text = raw_path.to_string_lossy().to_string();
    let args = vec![
        "--task",
        "Audit account change activity in admin group membership",
        "--baseline-bundle",
        raw_path_text.as_str(),
    ];
    let output = run_capture(&args);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    let turns = json
        .get("turns")
        .and_then(Value::as_array)
        .expect("turns should be an array");
    let first_args = turns
        .first()
        .and_then(|turn| turn.get("tool_call"))
        .and_then(|call| call.get("args"))
        .and_then(Value::as_object)
        .expect("first tool call args should be an object");

    let baseline_accounts = first_args
        .get("baseline_privileged_accounts")
        .and_then(Value::as_array)
        .expect("baseline_privileged_accounts should be present");
    assert!(baseline_accounts.iter().any(|entry| entry == "svc-admin"));

    let _ = fs::remove_dir_all(&baseline_dir);
}

#[test]
fn rejects_empty_stdin_task() {
    let output = Command::new(env!("CARGO_BIN_EXE_wraithrun"))
        .args(["--task-stdin", "--format", "summary"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed running wraithrun with empty stdin");

    assert!(
        !output.status.success(),
        "process should fail for empty stdin"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Stdin task input is empty"));
}

#[test]
fn doctor_json_contract_contains_summary_and_checks() {
    let output = run_capture(&["--doctor", "--introspection-format", "json"]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    assert_eq!(
        json.get("contract_version").and_then(Value::as_str),
        Some("1.0.0")
    );
    let summary = json
        .get("summary")
        .and_then(Value::as_object)
        .expect("summary should be an object");
    assert!(summary.get("pass").and_then(Value::as_u64).is_some());
    assert!(summary.get("warn").and_then(Value::as_u64).is_some());
    assert!(summary.get("fail").and_then(Value::as_u64).is_some());

    let checks = json
        .get("checks")
        .and_then(Value::as_array)
        .expect("checks should be an array");
    assert!(
        !checks.is_empty(),
        "checks should include at least one entry"
    );
}

#[test]
fn doctor_json_contract_includes_model_pack_checks_for_live_mode() {
    let model_pack_dir = unique_temp_dir("wraithrun-doctor-model-pack");
    fs::create_dir_all(&model_pack_dir).expect("model pack dir should be created");

    let model_path = model_pack_dir.join("llm.onnx");
    let tokenizer_path = model_pack_dir.join("tokenizer.json");
    fs::write(&model_path, b"onnx-model-bytes").expect("model fixture should be written");
    fs::write(
        &tokenizer_path,
        r#"{"model":{"type":"WordPiece"},"version":"1.0"}"#,
    )
    .expect("tokenizer fixture should be written");

    let model_path_text = model_path.to_string_lossy().to_string();
    let tokenizer_path_text = tokenizer_path.to_string_lossy().to_string();

    let args = vec![
        "--doctor",
        "--introspection-format",
        "json",
        "--live",
        "--model",
        model_path_text.as_str(),
        "--tokenizer",
        tokenizer_path_text.as_str(),
    ];
    let output = run_capture(&args);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    let checks = json
        .get("checks")
        .and_then(Value::as_array)
        .expect("checks should be an array");

    assert!(checks.iter().any(|check| {
        check.get("name").and_then(Value::as_str) == Some("live-model-format")
            && check.get("status").and_then(Value::as_str) == Some("pass")
    }));
    assert!(checks.iter().any(|check| {
        check.get("name").and_then(Value::as_str) == Some("live-tokenizer-json")
            && check.get("status").and_then(Value::as_str) == Some("pass")
    }));

    let _ = fs::remove_dir_all(&model_pack_dir);
}

#[test]
fn live_setup_command_writes_live_profile_to_config() {
    let temp_dir = unique_temp_dir("wraithrun-live-setup-success");
    fs::create_dir_all(&temp_dir).expect("temp directory should be created");

    let model_path = temp_dir.join("llm.onnx");
    let tokenizer_path = temp_dir.join("tokenizer.json");
    let config_path = temp_dir.join("wraithrun.toml");

    fs::write(&model_path, b"onnx-model-bytes").expect("model fixture should be written");
    fs::write(
        &tokenizer_path,
        r#"{"model":{"type":"WordPiece"},"version":"1.0"}"#,
    )
    .expect("tokenizer fixture should be written");

    let model_path_text = model_path.to_string_lossy().to_string();
    let tokenizer_path_text = tokenizer_path.to_string_lossy().to_string();
    let config_path_text = config_path.to_string_lossy().to_string();

    let args = vec![
        "live",
        "setup",
        "--model",
        model_path_text.as_str(),
        "--tokenizer",
        tokenizer_path_text.as_str(),
        "--config",
        config_path_text.as_str(),
    ];
    let output = run_capture(&args);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Live setup complete"));
    assert!(stdout.contains("profile: live-model-local"));

    let config_text = fs::read_to_string(&config_path).expect("config file should be written");
    let parsed: TomlValue = toml::from_str(&config_text).expect("config should parse as toml");
    let profile = parsed
        .get("profiles")
        .and_then(|profiles| profiles.get("live-model-local"))
        .and_then(TomlValue::as_table)
        .expect("live-model-local profile should exist");

    assert_eq!(profile.get("live").and_then(TomlValue::as_bool), Some(true));
    assert_eq!(
        profile.get("model").and_then(TomlValue::as_str),
        Some(model_path_text.as_str())
    );
    assert_eq!(
        profile.get("tokenizer").and_then(TomlValue::as_str),
        Some(tokenizer_path_text.as_str())
    );
    assert_eq!(
        profile
            .get("live_fallback_policy")
            .and_then(TomlValue::as_str),
        Some("dry-run-on-error")
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn live_setup_command_fails_for_missing_explicit_model() {
    let temp_dir = unique_temp_dir("wraithrun-live-setup-missing-model");
    fs::create_dir_all(&temp_dir).expect("temp directory should be created");

    let missing_model = temp_dir.join("missing.onnx");
    let missing_model_text = missing_model.to_string_lossy().to_string();

    let args = vec!["live", "setup", "--model", missing_model_text.as_str()];
    let output = run_capture(&args);

    assert!(
        !output.status.success(),
        "process should fail when explicit model path is missing"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Live setup validation failed"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn task_templates_json_contract_contains_expected_fields() {
    let output = run_capture(&["--list-task-templates", "--introspection-format", "json"]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    assert_eq!(
        json.get("contract_version").and_then(Value::as_str),
        Some("1.0.0")
    );
    let templates = json
        .get("templates")
        .and_then(Value::as_array)
        .expect("templates should be an array");
    assert!(!templates.is_empty(), "templates should not be empty");

    let syslog_template = templates
        .iter()
        .find(|template| {
            template
                .get("name")
                .and_then(Value::as_str)
                .map(|name| name == "syslog-summary")
                .unwrap_or(false)
        })
        .expect("syslog-summary template should exist");

    assert_eq!(
        syslog_template
            .get("supports_template_target")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        syslog_template
            .get("supports_template_lines")
            .and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn profiles_json_contract_contains_expected_fields() {
    let output = run_capture(&["--list-profiles", "--introspection-format", "json"]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    assert_eq!(
        json.get("contract_version").and_then(Value::as_str),
        Some("1.0.0")
    );
    assert!(
        json.get("built_in_profiles")
            .and_then(Value::as_array)
            .is_some(),
        "built_in_profiles should be an array"
    );
    assert!(
        json.get("config_profiles")
            .and_then(Value::as_array)
            .is_some(),
        "config_profiles should be an array"
    );
    assert!(
        json.get("selected_profile").is_some(),
        "selected_profile key should always be present"
    );
}

#[test]
fn tools_json_contract_contains_expected_fields() {
    let output = run_capture(&["--list-tools", "--introspection-format", "json"]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    assert_eq!(
        json.get("contract_version").and_then(Value::as_str),
        Some("1.0.0")
    );
    let tools = json
        .get("tools")
        .and_then(Value::as_array)
        .expect("tools should be an array");
    assert!(!tools.is_empty(), "tools should not be empty");

    let hash_binary = tools
        .iter()
        .find(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .map(|name| name == "hash_binary")
                .unwrap_or(false)
        })
        .expect("hash_binary tool should exist");

    assert!(hash_binary
        .get("description")
        .and_then(Value::as_str)
        .is_some());
    assert!(
        hash_binary
            .get("args_schema")
            .and_then(Value::as_object)
            .is_some(),
        "args_schema should be an object"
    );
}

#[test]
fn tools_json_contract_includes_coverage_expansion_tools() {
    let output = run_capture(&["--list-tools", "--introspection-format", "json"]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    let tools = json
        .get("tools")
        .and_then(Value::as_array)
        .expect("tools should be an array");

    for expected in [
        "inspect_persistence_locations",
        "audit_account_changes",
        "correlate_process_network",
        "capture_coverage_baseline",
    ] {
        assert!(
            tools.iter().any(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .map(|name| name == expected)
                    .unwrap_or(false)
            }),
            "expected tool '{expected}' to be present"
        );
    }
}

#[test]
fn describe_tool_json_contract_includes_baseline_and_allowlist_args() {
    let cases: [(&str, &[&str]); 4] = [
        (
            "inspect_persistence_locations",
            &["baseline_entries", "allowlist_terms"],
        ),
        (
            "audit_account_changes",
            &[
                "baseline_privileged_accounts",
                "approved_privileged_accounts",
            ],
        ),
        (
            "correlate_process_network",
            &["baseline_exposed_bindings", "expected_processes"],
        ),
        (
            "capture_coverage_baseline",
            &["persistence_limit", "listener_limit"],
        ),
    ];

    for (tool_name, expected_fields) in cases {
        let output = run_capture(&[
            "--describe-tool",
            tool_name,
            "--introspection-format",
            "json",
        ]);

        assert!(
            output.status.success(),
            "process failed for {tool_name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let json = parse_stdout_json(&output);
        let properties = json
            .get("tool")
            .and_then(Value::as_object)
            .and_then(|tool| tool.get("args_schema"))
            .and_then(Value::as_object)
            .and_then(|schema| schema.get("properties"))
            .and_then(Value::as_object)
            .expect("args_schema.properties should be present");

        for field in expected_fields {
            assert!(
                properties.contains_key(*field),
                "expected field '{field}' in args schema for tool '{tool_name}'"
            );
        }
    }
}

#[test]
fn describe_tool_json_contract_contains_expected_fields() {
    let output = run_capture(&[
        "--describe-tool",
        "hash_binary",
        "--introspection-format",
        "json",
    ]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    assert_eq!(
        json.get("contract_version").and_then(Value::as_str),
        Some("1.0.0")
    );
    let tool = json
        .get("tool")
        .and_then(Value::as_object)
        .expect("tool should be an object");
    assert_eq!(
        tool.get("name").and_then(Value::as_str),
        Some("hash_binary")
    );
    assert!(
        tool.get("description").and_then(Value::as_str).is_some(),
        "description should be present"
    );
}

#[test]
fn describe_tool_json_contract_accepts_hyphenated_alias() {
    let output = run_capture(&[
        "--describe-tool",
        "hash-binary",
        "--introspection-format",
        "json",
    ]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    let tool = json
        .get("tool")
        .and_then(Value::as_object)
        .expect("tool should be an object");
    assert_eq!(
        tool.get("name").and_then(Value::as_str),
        Some("hash_binary")
    );
}

#[test]
fn describe_tool_rejects_unknown_tool_name() {
    let output = run_capture(&["--describe-tool", "nonexistent-tool"]);

    assert!(
        !output.status.success(),
        "process should fail for unknown tool name"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unknown tool 'nonexistent-tool'"));
}

#[test]
fn describe_tool_rejects_ambiguous_query() {
    let output = run_capture(&["--describe-tool", "c"]);

    assert!(
        !output.status.success(),
        "process should fail for ambiguous describe-tool query"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Ambiguous tool query 'c'"));
    assert!(stderr.contains("scan_network"));
    assert!(stderr.contains("check_privilege_escalation_vectors"));
}

#[test]
fn list_tools_filter_json_contract_contains_filtered_result() {
    let output = run_capture(&[
        "--list-tools",
        "--tool-filter",
        "hash",
        "--introspection-format",
        "json",
    ]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    let tools = json
        .get("tools")
        .and_then(Value::as_array)
        .expect("tools should be an array");
    assert_eq!(tools.len(), 1, "filter should narrow tools to one result");
    assert_eq!(
        tools[0].get("name").and_then(Value::as_str),
        Some("hash_binary")
    );
}

#[test]
fn list_tools_filter_json_contract_supports_multi_term_query() {
    let output = run_capture(&[
        "--list-tools",
        "--tool-filter",
        "priv esc",
        "--introspection-format",
        "json",
    ]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
    let tools = json
        .get("tools")
        .and_then(Value::as_array)
        .expect("tools should be an array");
    assert_eq!(
        tools.len(),
        1,
        "multi-term filter should narrow to one result"
    );
    assert_eq!(
        tools[0].get("name").and_then(Value::as_str),
        Some("check_privilege_escalation_vectors")
    );
}

#[test]
fn list_tools_filter_rejects_no_matches() {
    let output = run_capture(&["--list-tools", "--tool-filter", "no-such-tool"]);

    assert!(
        !output.status.success(),
        "process should fail when filter has no matches"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No tools matched filter 'no-such-tool'"));
}

#[test]
fn list_tools_filter_rejects_separator_only_query() {
    let output = run_capture(&["--list-tools", "--tool-filter", "___"]);

    assert!(
        !output.status.success(),
        "process should fail when filter has no alphanumeric terms"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("at least one alphanumeric term"));
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be valid")
        .as_nanos();
    env::temp_dir().join(format!("{prefix}-{}-{stamp}", std::process::id()))
}
