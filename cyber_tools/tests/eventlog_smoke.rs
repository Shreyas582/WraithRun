//! Smoke test for Windows Event Log reader (#165).

#[cfg(target_os = "windows")]
#[test]
fn windows_event_log_reads_application_channel() {
    use cyber_tools::log_parser::{is_windows_event_channel, read_windows_event_log};

    assert!(is_windows_event_channel("Application"));
    assert!(is_windows_event_channel("System"));
    assert!(is_windows_event_channel("Security"));
    assert!(!is_windows_event_channel("/var/log/syslog"));
    assert!(!is_windows_event_channel("./agent.log"));

    let lines = read_windows_event_log("Application", 5).expect("read_windows_event_log failed");
    println!("Got {} lines from Application channel", lines.len());
    for line in lines.iter().take(2) {
        println!("  {}", line);
    }
    assert!(!lines.is_empty(), "Application channel should have entries");
}
