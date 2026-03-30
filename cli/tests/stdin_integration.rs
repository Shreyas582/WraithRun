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
fn report_json_contract_contains_findings_layer() {
    let output = run_capture(&["--task", "Investigate unauthorized SSH keys"]);

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parse_stdout_json(&output);
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

#[test]
fn tools_json_contract_contains_expected_fields() {
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
