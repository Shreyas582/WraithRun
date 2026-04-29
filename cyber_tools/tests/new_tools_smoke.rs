//! Smoke tests for the two new tools added in this session: enumerate_scheduled_tasks (#171)
//! and analyze_process_tree (#169). These exercise the live host code paths.

use cyber_tools::{
    persistence_checker::enumerate_scheduled_tasks, process_correlation::collect_process_tree,
    AnalyzeProcessTreeTool, EnumerateScheduledTasksTool, Tool,
};
use serde_json::json;

#[tokio::test]
async fn enumerate_scheduled_tasks_returns_real_entries() {
    let tasks = enumerate_scheduled_tasks(64)
        .await
        .expect("enumerate_scheduled_tasks failed");

    println!("Got {} scheduled tasks", tasks.len());
    for t in tasks.iter().take(5) {
        println!(
            "  source={} name={:?} schedule={:?} suspicious={}",
            t.source, t.name, t.schedule, t.suspicious
        );
    }
    // On any modern Windows host, there should be at least a few scheduled tasks.
    // On Unix-without-cron, this might be zero — we tolerate either, but the call
    // must succeed and return a Vec.
    assert!(tasks.len() <= 64, "limit should be respected");
}

#[tokio::test]
async fn analyze_process_tree_returns_pid_relationships() {
    let tree = collect_process_tree(128)
        .await
        .expect("collect_process_tree failed");

    println!("Got {} processes", tree.len());
    assert!(!tree.is_empty(), "process tree must not be empty");

    let with_children = tree.iter().filter(|e| !e.child_pids.is_empty()).count();
    println!("  {} processes have at least one child", with_children);

    // System hosts should have at least one parent-child relationship visible.
    assert!(
        with_children > 0,
        "expected at least one parent-child relationship"
    );

    // Check fields are populated. On Windows the first row is "System Idle
    // Process" with pid=0; pick the first non-zero pid for the field check.
    let sample = tree.iter().find(|p| p.pid > 0).unwrap_or(&tree[0]);
    println!(
        "  sample: pid={} ppid={} name={:?}",
        sample.pid, sample.ppid, sample.name
    );
    assert!(!sample.name.is_empty());
}

#[tokio::test]
async fn enumerate_scheduled_tasks_tool_wrapper() {
    let tool = EnumerateScheduledTasksTool;
    let result = tool
        .run(json!({"limit": 32}))
        .await
        .expect("tool run failed");
    println!(
        "tool result: {}",
        serde_json::to_string_pretty(&result).unwrap()
    );

    let task_count = result["task_count"].as_u64().expect("task_count missing");
    let suspicious_count = result["suspicious_count"]
        .as_u64()
        .expect("suspicious_count missing");
    assert!(suspicious_count <= task_count);
    assert!(result["tasks"].is_array());
}

#[tokio::test]
async fn analyze_process_tree_tool_wrapper() {
    let tool = AnalyzeProcessTreeTool;
    let result = tool
        .run(json!({"limit": 64}))
        .await
        .expect("tool run failed");

    let process_count = result["process_count"]
        .as_u64()
        .expect("process_count missing");
    let suspicious_count = result["suspicious_count"]
        .as_u64()
        .expect("suspicious_count missing");
    println!(
        "process_count={} suspicious_count={}",
        process_count, suspicious_count
    );
    assert!(process_count > 0);
    assert!(suspicious_count <= process_count);
    assert!(result["processes"].is_array());
}
