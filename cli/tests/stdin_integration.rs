use std::io::Write;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

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
fn task_templates_json_contract_contains_expected_fields() {
    let output = run_capture(&["--list-task-templates", "--introspection-format", "json"]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
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
