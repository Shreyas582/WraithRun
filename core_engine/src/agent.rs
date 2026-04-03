use anyhow::Result;
use serde_json::{json, Map, Value};
use std::time::Instant;
use tracing::{debug, info};

use cyber_tools::ToolRegistry;
use inference_bridge::InferenceEngine;

use crate::{
    basic_tier_summary, deduplicate_findings, derive_findings, extract_tag, max_severity,
    quality_checked_final_answer, sort_findings, AgentTurn, CoverageBaseline,
    ModelCapabilityReport, ModelCapabilityTier, RunReport, RunTimingMetrics, ToolCall,
};

pub struct Agent<B: InferenceEngine> {
    brain: B,
    tools: ToolRegistry,
    max_steps: usize,
    coverage_baseline: Option<CoverageBaseline>,
    capability_tier: ModelCapabilityTier,
    model_capability_report: Option<ModelCapabilityReport>,
}

impl<B: InferenceEngine> Agent<B> {
    pub fn new(brain: B, tools: ToolRegistry) -> Self {
        Self {
            brain,
            tools,
            max_steps: 8,
            coverage_baseline: None,
            capability_tier: ModelCapabilityTier::Strong,
            model_capability_report: None,
        }
    }

    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps.max(1);
        self
    }

    pub fn with_coverage_baseline(mut self, coverage_baseline: CoverageBaseline) -> Self {
        self.coverage_baseline = if coverage_baseline.is_empty() {
            None
        } else {
            Some(coverage_baseline)
        };
        self
    }

    pub fn with_capability_tier(mut self, tier: ModelCapabilityTier) -> Self {
        self.capability_tier = tier;
        self
    }

    pub fn with_model_capability_report(mut self, report: ModelCapabilityReport) -> Self {
        self.model_capability_report = Some(report);
        self
    }

    fn apply_coverage_baseline_to_call(&self, call: &mut ToolCall) {
        let Some(coverage_baseline) = self.coverage_baseline.as_ref() else {
            return;
        };

        if !call.args.is_object() {
            call.args = Value::Object(Map::new());
        }

        let Some(args) = call.args.as_object_mut() else {
            return;
        };

        match call.tool.as_str() {
            "inspect_persistence_locations" => {
                set_string_list_arg(
                    args,
                    "baseline_entries",
                    &coverage_baseline.baseline_entries,
                );
            }
            "audit_account_changes" => {
                set_string_list_arg(
                    args,
                    "baseline_privileged_accounts",
                    &coverage_baseline.baseline_privileged_accounts,
                );
                set_string_list_arg(
                    args,
                    "approved_privileged_accounts",
                    &coverage_baseline.approved_privileged_accounts,
                );
            }
            "correlate_process_network" => {
                set_string_list_arg(
                    args,
                    "baseline_exposed_bindings",
                    &coverage_baseline.baseline_exposed_bindings,
                );
                set_string_list_arg(
                    args,
                    "expected_processes",
                    &coverage_baseline.expected_processes,
                );
            }
            _ => {}
        }
    }

    pub async fn run(&self, task: &str) -> Result<RunReport> {
        let run_started_at = Instant::now();

        // Phase 1: deterministic tool execution — gather evidence.
        let tool_plan = investigation_plan(task);
        let mut turns = Vec::new();

        for tool_name in tool_plan.iter().take(self.max_steps) {
            // #74: skip tools with known-failing preconditions.
            if !self.check_tool_precondition(tool_name) {
                debug!(tool = %tool_name, "skipping tool: precondition not met");
                continue;
            }

            let mut call = ToolCall {
                tool: tool_name.to_string(),
                args: Value::Object(Map::new()),
            };
            self.apply_coverage_baseline_to_call(&mut call);

            let observation = match self.tools.execute(&call.tool, call.args.clone()).await {
                Ok(value) => value,
                Err(err) => json!({ "error": err.to_string() }),
            };

            info!(tool = %call.tool, "tool executed");

            turns.push(AgentTurn {
                thought: format!("Executing {} to gather evidence.", call.tool),
                tool_call: Some(call),
                observation: Some(observation),
            });
        }

        // Phase 2: synthesis — behavior depends on capability tier.
        let raw_findings = derive_findings(&turns, "");
        let mut findings = deduplicate_findings(raw_findings);
        sort_findings(&mut findings);

        let (final_answer, first_token_latency_ms) = match self.capability_tier {
            ModelCapabilityTier::Basic => {
                // Skip LLM entirely; build deterministic summary from findings.
                debug!("Basic tier: skipping LLM synthesis");
                let answer = basic_tier_summary(&findings);
                (answer, None)
            }
            ModelCapabilityTier::Moderate => {
                // Call LLM with reduced evidence (top-5 observations).
                let evidence_summary = build_evidence_summary_limited(&turns, 5);
                let synthesis_prompt = format_synthesis_prompt(task, &evidence_summary);
                let output = self.brain.generate(&synthesis_prompt).await?;
                let latency = Some(elapsed_ms_since(run_started_at));
                info!(output = %output, "agent synthesis output (moderate)");
                let raw = extract_tag(&output, "final").unwrap_or(output);
                let answer = quality_checked_final_answer(&raw, &findings);
                (answer, latency)
            }
            ModelCapabilityTier::Strong => {
                // Full evidence, full synthesis.
                let evidence_summary = build_evidence_summary(&turns);
                let synthesis_prompt = format_synthesis_prompt(task, &evidence_summary);
                let output = self.brain.generate(&synthesis_prompt).await?;
                let latency = Some(elapsed_ms_since(run_started_at));
                info!(output = %output, "agent synthesis output (strong)");
                let raw = extract_tag(&output, "final").unwrap_or(output);
                let answer = quality_checked_final_answer(&raw, &findings);
                (answer, latency)
            }
        };

        let report_max_severity = max_severity(&findings);

        Ok(RunReport {
            task: task.to_string(),
            case_id: None,
            max_severity: report_max_severity,
            model_capability: self.model_capability_report.clone(),
            live_fallback_decision: None,
            run_timing: Some(build_run_timing_metrics(
                run_started_at,
                first_token_latency_ms,
            )),
            live_run_metrics: None,
            turns,
            final_answer,
            findings,
        })
    }

    /// Check whether a tool's preconditions are met before executing it.
    /// Returns false if the tool should be skipped.
    fn check_tool_precondition(&self, tool_name: &str) -> bool {
        match tool_name {
            "read_syslog" => {
                // Default path is ./agent.log — skip if it doesn't exist and
                // the sandbox policy would deny access anyway.
                let default_path = std::path::Path::new("./agent.log");
                if !default_path.exists() {
                    return false;
                }
                self.tools
                    .policy()
                    .ensure_path_allowed(default_path)
                    .is_ok()
            }
            _ => true,
        }
    }
}

fn elapsed_ms_since(started_at: Instant) -> u64 {
    started_at
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// Select tools to run based on the task description.
/// All tools are cheap local introspection, so we run a broad set by default.
fn investigation_plan(task: &str) -> Vec<&'static str> {
    let lower = task.to_lowercase();
    let mut plan = Vec::new();

    // Always start with broad evidence gathering.
    plan.push("audit_account_changes");
    plan.push("inspect_persistence_locations");
    plan.push("read_syslog");

    // Network-related tasks.
    if lower.contains("network")
        || lower.contains("connection")
        || has_word(&lower, "port")
        || lower.contains("listen")
        || lower.contains("ssh")
        || lower.contains("lateral")
        || lower.contains("beacon")
    {
        plan.push("scan_network");
        plan.push("correlate_process_network");
    }

    // Privilege / escalation tasks.
    if lower.contains("privilege")
        || lower.contains("escalat")
        || lower.contains("admin")
        || lower.contains("root")
        || lower.contains("sudo")
        || lower.contains("unauthori")
    {
        plan.push("check_privilege_escalation_vectors");
    }

    // If the plan is still small (generic task), add more tools.
    if plan.len() <= 3 {
        plan.push("scan_network");
        plan.push("check_privilege_escalation_vectors");
    }

    plan
}

/// Check whether `word` appears as a standalone word in `text` (not as a substring of another word).
fn has_word(text: &str, word: &str) -> bool {
    for (idx, _) in text.match_indices(word) {
        let before_ok = idx == 0 || !text.as_bytes()[idx - 1].is_ascii_alphanumeric();
        let after_idx = idx + word.len();
        let after_ok =
            after_idx >= text.len() || !text.as_bytes()[after_idx].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Build a concise evidence summary from tool observations for LLM synthesis.
fn build_evidence_summary(turns: &[AgentTurn]) -> String {
    build_evidence_summary_limited(turns, usize::MAX)
}

/// Build an evidence summary limited to the first `max_turns` observations.
/// Used by Moderate tier to reduce prompt size.
fn build_evidence_summary_limited(turns: &[AgentTurn], max_turns: usize) -> String {
    let mut summary = String::new();
    let mut count = 0;
    for turn in turns {
        if count >= max_turns {
            break;
        }
        let tool_name = turn
            .tool_call
            .as_ref()
            .map(|c| c.tool.as_str())
            .unwrap_or("unknown");
        if let Some(obs) = &turn.observation {
            // Truncate large observations to keep within model context limits.
            let obs_str = serde_json::to_string(obs).unwrap_or_default();
            let truncated = if obs_str.len() > 1500 {
                format!("{}...(truncated)", &obs_str[..1500])
            } else {
                obs_str
            };
            summary.push_str(&format!("[{tool_name}] {truncated}\n\n"));
            count += 1;
        }
    }
    summary
}

/// Format the synthesis prompt that asks the LLM to analyze collected evidence.
fn format_synthesis_prompt(task: &str, evidence: &str) -> String {
    format!(
        "You are a security analyst. Analyze the evidence below and write a report.\n\
         Task: {task}\n\n\
         Evidence from host investigation tools:\n\
         {evidence}\n\
         Write your report inside <final>...</final> tags.\n\
         Format:\n\
         SUMMARY: One-line verdict.\n\
         FINDINGS: Numbered list of specific observations from the evidence.\n\
         RISK: critical/high/medium/low/info\n\
         ACTIONS: Numbered remediation steps.\n\n\
         <final>"
    )
}

fn build_run_timing_metrics(
    run_started_at: Instant,
    first_token_latency_ms: Option<u64>,
) -> RunTimingMetrics {
    RunTimingMetrics {
        first_token_latency_ms,
        total_run_duration_ms: elapsed_ms_since(run_started_at),
    }
}

fn set_string_list_arg(args: &mut serde_json::Map<String, Value>, key: &str, values: &[String]) {
    if args.contains_key(key) || values.is_empty() {
        return;
    }

    let list = values
        .iter()
        .map(|value| Value::String(value.clone()))
        .collect();
    args.insert(key.to_string(), Value::Array(list));
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::{json, Value};

    use cyber_tools::ToolRegistry;
    use inference_bridge::InferenceEngine;

    use super::Agent;
    use crate::{CoverageBaseline, ToolCall};

    #[derive(Clone)]
    struct MockEngine {
        responses: Arc<Mutex<VecDeque<String>>>,
    }

    #[test]
    fn injects_baseline_arguments_for_supported_tools() {
        let baseline = CoverageBaseline {
            baseline_entries: vec!["autorun-a".to_string()],
            baseline_privileged_accounts: vec!["svc-admin".to_string()],
            approved_privileged_accounts: vec!["svc-admin".to_string()],
            baseline_exposed_bindings: vec!["0.0.0.0:443".to_string()],
            expected_processes: vec!["nginx".to_string()],
        };

        let agent = Agent::new(MockEngine::new(vec![]), ToolRegistry::new())
            .with_coverage_baseline(baseline);

        let mut persistence_call = ToolCall {
            tool: "inspect_persistence_locations".to_string(),
            args: json!({"limit": 32}),
        };
        agent.apply_coverage_baseline_to_call(&mut persistence_call);
        assert_eq!(
            persistence_call.args["baseline_entries"][0],
            Value::String("autorun-a".to_string())
        );

        let mut account_call = ToolCall {
            tool: "audit_account_changes".to_string(),
            args: json!({}),
        };
        agent.apply_coverage_baseline_to_call(&mut account_call);
        assert_eq!(
            account_call.args["baseline_privileged_accounts"][0],
            Value::String("svc-admin".to_string())
        );
        assert_eq!(
            account_call.args["approved_privileged_accounts"][0],
            Value::String("svc-admin".to_string())
        );

        let mut network_call = ToolCall {
            tool: "correlate_process_network".to_string(),
            args: json!({"limit": 16}),
        };
        agent.apply_coverage_baseline_to_call(&mut network_call);
        assert_eq!(
            network_call.args["baseline_exposed_bindings"][0],
            Value::String("0.0.0.0:443".to_string())
        );
        assert_eq!(
            network_call.args["expected_processes"][0],
            Value::String("nginx".to_string())
        );
    }

    #[test]
    fn does_not_override_explicit_tool_args_with_baseline() {
        let baseline = CoverageBaseline {
            baseline_entries: vec!["autorun-a".to_string()],
            ..CoverageBaseline::default()
        };

        let agent = Agent::new(MockEngine::new(vec![]), ToolRegistry::new())
            .with_coverage_baseline(baseline);

        let mut call = ToolCall {
            tool: "inspect_persistence_locations".to_string(),
            args: json!({"baseline_entries": ["manual-entry"]}),
        };
        agent.apply_coverage_baseline_to_call(&mut call);

        assert_eq!(
            call.args["baseline_entries"][0],
            Value::String("manual-entry".to_string())
        );
    }

    impl MockEngine {
        fn new(responses: Vec<&str>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(
                    responses
                        .into_iter()
                        .map(|value| value.to_string())
                        .collect(),
                )),
            }
        }
    }

    #[async_trait]
    impl InferenceEngine for MockEngine {
        async fn generate(&self, _prompt: &str) -> Result<String> {
            let mut responses = self
                .responses
                .lock()
                .expect("response queue mutex poisoned");
            Ok(responses
                .pop_front()
                .unwrap_or_else(|| "<final>fallback</final>".to_string()))
        }
    }

    #[tokio::test]
    async fn executes_investigation_plan_and_synthesizes() {
        let engine = MockEngine::new(vec![
            "<final>SUMMARY: Found 1 non-default privileged account.\nRISK: high</final>",
        ]);

        let agent = Agent::new(engine, ToolRegistry::with_default_tools());
        let report = agent
            .run("Investigate suspicious account behavior")
            .await
            .expect("agent run should succeed");

        assert!(
            report
                .final_answer
                .contains("non-default privileged account"),
            "final answer should contain synthesis: {}",
            report.final_answer
        );
        // Agent should have run multiple tools from the investigation plan.
        assert!(
            report.turns.len() >= 3,
            "expected at least 3 tool turns, got {}",
            report.turns.len()
        );
        // All turns should have tool calls with observations.
        for turn in &report.turns {
            assert!(
                turn.tool_call.is_some(),
                "every turn should have a tool call"
            );
            assert!(
                turn.observation.is_some(),
                "every turn should have an observation"
            );
        }
    }

    #[tokio::test]
    async fn falls_back_to_raw_output_when_no_final_tag() {
        let engine = MockEngine::new(vec!["No significant anomalies detected."]);

        let agent = Agent::new(engine, ToolRegistry::with_default_tools());
        let report = agent
            .run("Perform a quick triage")
            .await
            .expect("agent run should succeed");

        assert_eq!(report.final_answer, "No significant anomalies detected.");
        // Investigation plan tools should still have been executed.
        assert!(
            report.turns.len() >= 3,
            "expected at least 3 tool turns, got {}",
            report.turns.len()
        );
    }

    #[tokio::test]
    async fn derives_structured_findings_from_tool_observations() {
        let engine = MockEngine::new(vec!["<final>summary</final>"]);

        let agent = Agent::new(engine, ToolRegistry::with_default_tools());
        let report = agent
            .run("Investigate unauthorized access")
            .await
            .expect("agent run should succeed");

        // derive_findings should produce findings from real tool observations.
        assert!(
            !report.findings.is_empty(),
            "expected at least one finding from tool observations"
        );
    }
}
