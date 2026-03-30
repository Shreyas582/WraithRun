use std::io::Write;
use std::process::{Command, Output, Stdio};

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
