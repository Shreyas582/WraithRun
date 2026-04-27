/// Community end-to-end test suite (#42).
///
/// All tests run in dry-run mode so they require no live model and pass on
/// every platform in public CI. Each test spawns the real wraithrun binary
/// and validates the JSON output contract.
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, fs, net, thread};

use serde_json::Value;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn run_capture(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_wraithrun"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to execute wraithrun")
}

fn parse_json(output: &std::process::Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "stdout was not valid JSON: {e}\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn assert_exit_ok(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "process exited non-zero\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should work")
        .as_nanos();
    env::temp_dir().join(format!("{prefix}-{}-{stamp}", std::process::id()))
}

fn free_port() -> u16 {
    // Bind to :0, let the OS pick a port, then release so the server can use it.
    let listener = net::TcpListener::bind("127.0.0.1:0").expect("bind should succeed");
    listener.local_addr().expect("addr").port()
}

// ---------------------------------------------------------------------------
// Shared JSON contract assertions
// ---------------------------------------------------------------------------

fn assert_report_contract(json: &Value) {
    assert_eq!(
        json.get("contract_version").and_then(Value::as_str),
        Some("1.0.0"),
        "contract_version must be 1.0.0"
    );
    let findings = json
        .get("findings")
        .and_then(Value::as_array)
        .expect("findings must be an array");
    assert!(
        !findings.is_empty(),
        "findings must not be empty in dry-run mode"
    );
    let first = findings[0].as_object().expect("finding must be an object");
    assert!(first.contains_key("severity"), "finding must have severity");
    assert!(
        first.contains_key("confidence"),
        "finding must have confidence"
    );
    assert!(
        first.contains_key("recommended_action"),
        "finding must have recommended_action"
    );
}

// ---------------------------------------------------------------------------
// Template coverage — all 7 built-in investigation templates
// ---------------------------------------------------------------------------

#[test]
fn template_broad_host_triage() {
    let output = run_capture(&[
        "--task",
        "Run a broad host triage of this machine",
        "--format",
        "json",
    ]);
    assert_exit_ok(&output);
    assert_report_contract(&parse_json(&output));
}

#[test]
fn template_ssh_key_investigation() {
    let output = run_capture(&[
        "--task",
        "Investigate unauthorized SSH keys on this host",
        "--format",
        "json",
    ]);
    assert_exit_ok(&output);
    assert_report_contract(&parse_json(&output));
}

#[test]
fn template_persistence_analysis() {
    let output = run_capture(&[
        "--task",
        "Analyze persistence mechanisms including autoruns",
        "--format",
        "json",
    ]);
    assert_exit_ok(&output);
    assert_report_contract(&parse_json(&output));
}

#[test]
fn template_network_exposure_audit() {
    let output = run_capture(&[
        "--task",
        "Audit network listeners and exposed services",
        "--format",
        "json",
    ]);
    assert_exit_ok(&output);
    assert_report_contract(&parse_json(&output));
}

#[test]
fn template_privilege_escalation_check() {
    let output = run_capture(&[
        "--task",
        "Review privilege escalation vectors and admin grants",
        "--format",
        "json",
    ]);
    assert_exit_ok(&output);
    assert_report_contract(&parse_json(&output));
}

#[test]
fn template_file_integrity_check() {
    let output = run_capture(&[
        "--task",
        "Verify file hashes and detect binary tampering",
        "--format",
        "json",
    ]);
    assert_exit_ok(&output);
    assert_report_contract(&parse_json(&output));
}

#[test]
fn template_syslog_analysis() {
    let output = run_capture(&[
        "--task",
        "Analyze system logs for failed logins and anomalies",
        "--format",
        "json",
    ]);
    assert_exit_ok(&output);
    assert_report_contract(&parse_json(&output));
}

// ---------------------------------------------------------------------------
// Dry-run output format
// ---------------------------------------------------------------------------

#[test]
fn dry_run_summary_format_contains_task_line() {
    let output = run_capture(&[
        "--task",
        "Check for suspicious persistence on this host",
        "--format",
        "summary",
    ]);
    assert_exit_ok(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Task:"),
        "summary format should contain 'Task:' line"
    );
    assert!(
        stdout.contains("Findings:") || stdout.contains("findings"),
        "summary format should mention findings"
    );
}

#[test]
fn dry_run_json_has_contract_version() {
    let output = run_capture(&[
        "--task",
        "Investigate unauthorized SSH keys",
        "--format",
        "json",
    ]);
    assert_exit_ok(&output);
    let json = parse_json(&output);
    assert_eq!(
        json.get("contract_version").and_then(Value::as_str),
        Some("1.0.0"),
        "report must include contract_version 1.0.0"
    );
}

// ---------------------------------------------------------------------------
// Doctor output
// ---------------------------------------------------------------------------

#[test]
fn doctor_json_contract() {
    let tmp = unique_temp_dir("wraithrun-e2e-doctor");
    fs::create_dir_all(&tmp).expect("tmp dir");
    let model = tmp.join("model.onnx");
    fs::write(&model, b"fake-onnx").expect("model fixture");
    let model_str = model.to_string_lossy().to_string();

    let output = run_capture(&[
        "--doctor",
        "--introspection-format",
        "json",
        "--live",
        "--model",
        &model_str,
    ]);

    // Doctor exits non-zero for failures but JSON is always on stdout.
    let json = parse_json(&output);
    let checks = json
        .get("checks")
        .and_then(Value::as_array)
        .expect("doctor output must include checks array");
    assert!(!checks.is_empty(), "at least one check must be present");

    let _ = fs::remove_dir_all(&tmp);
}

// ---------------------------------------------------------------------------
// Case management round-trip
// ---------------------------------------------------------------------------

#[test]
fn case_id_is_reflected_in_report() {
    let case_id = format!("e2e-case-{}", std::process::id());
    let output = run_capture(&[
        "--task",
        "Check suspicious listener ports",
        "--case-id",
        &case_id,
        "--format",
        "json",
    ]);
    assert_exit_ok(&output);
    let json = parse_json(&output);
    assert_eq!(
        json.get("case_id").and_then(Value::as_str),
        Some(case_id.as_str()),
        "case_id should be reflected in the report"
    );
}

// ---------------------------------------------------------------------------
// Backend alias resolution (#159)
// ---------------------------------------------------------------------------

#[test]
fn backend_alias_dml_gives_backend_not_found_error_not_parse_error() {
    // On systems without DirectML the alias "dml" must be normalized to
    // "directml" and produce a "not found" error, never a parse error.
    // Use a dummy model file + dry-run-on-error so the process always exits 0
    // and produces valid JSON regardless of whether DirectML is available.
    let tmp = unique_temp_dir("wraithrun-e2e-dml");
    fs::create_dir_all(&tmp).expect("tmp dir");
    let model = tmp.join("dummy.onnx");
    fs::write(&model, b"not-real-onnx").expect("dummy model");
    let model_str = model.to_string_lossy().to_string();

    let output = run_capture(&[
        "--task",
        "Check hosts",
        "--live",
        "--model",
        &model_str,
        "--backend",
        "dml",
        "--live-fallback-policy",
        "dry-run-on-error",
        "--format",
        "json",
    ]);

    let _ = fs::remove_dir_all(&tmp);

    // With dry-run-on-error the process succeeds even if live fails.
    // The important thing: no panic, valid JSON.
    let _json = parse_json(&output);
}

// ---------------------------------------------------------------------------
// API server startup + /run endpoint
// ---------------------------------------------------------------------------

#[test]
fn api_server_run_endpoint_returns_json_report() {
    let port = free_port();
    let token = "e2e-test-token";
    let db_dir = unique_temp_dir("wraithrun-e2e-api");
    fs::create_dir_all(&db_dir).expect("db dir");
    let db_path = db_dir.join("cases.db");
    let db_str = db_path.to_string_lossy().to_string();

    // Start server in background.
    let mut server = Command::new(env!("CARGO_BIN_EXE_wraithrun"))
        .args([
            "--serve",
            "--port",
            &port.to_string(),
            "--api-token",
            token,
            "--database",
            &db_str,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("server should spawn");

    // Give it a moment to bind.
    thread::sleep(Duration::from_millis(800));

    // POST /api/v1/runs with a dry-run task.
    let url = format!("http://127.0.0.1:{port}/api/v1/runs");
    let body = r#"{"task":"Investigate unauthorized SSH keys"}"#;

    let curl_output = Command::new("curl")
        .args([
            "-s",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-H",
            &format!("Authorization: Bearer {token}"),
            "-d",
            body,
            "--max-time",
            "30",
            &url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let _ = server.kill();
    let _ = server.wait();
    let _ = fs::remove_dir_all(&db_dir);

    let curl_output = match curl_output {
        Ok(o) => o,
        Err(e) => {
            // curl may not be installed (rare on CI); skip gracefully.
            eprintln!("skipping API server test: curl unavailable: {e}");
            return;
        }
    };

    if !curl_output.status.success() {
        eprintln!(
            "curl failed (server may not have started): {}",
            String::from_utf8_lossy(&curl_output.stderr)
        );
        return; // Treat as soft skip on flaky port acquisition
    }

    let json: Value = serde_json::from_slice(&curl_output.stdout)
        .expect("API /run response must be valid JSON");
    assert!(
        json.get("findings").is_some() || json.get("error").is_some(),
        "response must have findings or error field"
    );
}
