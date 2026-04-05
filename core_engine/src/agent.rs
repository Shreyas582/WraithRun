use anyhow::Result;
use serde_json::{json, Map, Value};
use std::time::Instant;
use tracing::{debug, info};

use cyber_tools::ToolRegistry;
use inference_bridge::InferenceEngine;

use crate::{
    basic_tier_summary, builtin_investigation_templates, deduplicate_findings, derive_findings,
    extract_tag, max_severity, quality_checked_final_answer, sort_findings, AgentTurn,
    CoverageBaseline, EvidencePointer, Finding, FindingSeverity, InvestigationTemplate,
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

        // Scope validation (#83): detect out-of-scope tasks before tool execution.
        if let Some(scope_finding) = check_task_scope(task) {
            info!("task is out of scope for host-local tools");
            return Ok(RunReport {
                task: task.to_string(),
                case_id: None,
                max_severity: Some(FindingSeverity::Info),
                backend: None,
                model_capability: self.model_capability_report.clone(),
                live_fallback_decision: None,
                run_timing: Some(build_run_timing_metrics(run_started_at, None)),
                live_run_metrics: None,
                turns: Vec::new(),
                final_answer: "Task is outside the scope of available host-local investigation tools. No tools were executed.".to_string(),
                findings: vec![scope_finding],
                supplementary_findings: Vec::new(),
            });
        }

        // Dispatch based on capability tier (#92).
        let (turns, final_answer, first_token_latency_ms) = match self.capability_tier {
            ModelCapabilityTier::Basic => {
                // Template-driven tool execution (no LLM for tool selection).
                let (turns, _template) = self.run_template_phase(task).await;
                let raw_findings = derive_findings(&turns, "");
                let findings = deduplicate_findings(raw_findings);
                debug!("Basic tier: skipping LLM synthesis");
                let answer = basic_tier_summary(&findings);
                (turns, answer, None)
            }
            ModelCapabilityTier::Moderate | ModelCapabilityTier::Strong => {
                // ReAct loop: LLM decides tool selection (#92).
                self.run_react_loop(task, &run_started_at).await?
            }
        };

        let raw_findings = derive_findings(&turns, "");
        let mut findings = deduplicate_findings(raw_findings);
        sort_findings(&mut findings);

        // Tag finding relevance: tools used in turns are primary.
        let primary_tools: std::collections::HashSet<String> = turns
            .iter()
            .filter_map(|t| t.tool_call.as_ref().map(|c| c.tool.clone()))
            .collect();
        for finding in &mut findings {
            if let Some(tool) = finding.evidence_pointer.tool.as_deref() {
                if !primary_tools.contains(tool) {
                    finding.relevance = crate::FindingRelevance::Supplementary;
                }
            }
        }

        let report_max_severity = max_severity(&findings);

        Ok(RunReport {
            task: task.to_string(),
            case_id: None,
            max_severity: report_max_severity,
            backend: None,
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
            supplementary_findings: Vec::new(),
        })
    }

    /// Template-driven tool execution (Phase 1 fallback for Basic tier).
    async fn run_template_phase(
        &self,
        task: &str,
    ) -> (Vec<AgentTurn>, &'static InvestigationTemplate) {
        let template = resolve_investigation_template(task);
        let mut turns = Vec::new();

        for tool_name in template.tools.iter().take(self.max_steps) {
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

        (turns, template)
    }

    /// LLM-guided ReAct loop (#92).
    ///
    /// The LLM receives a system prompt with available tools and the task,
    /// then iteratively decides which tool to call. It emits:
    /// - `<call>{"tool":"name","args":{...}}</call>` to invoke a tool
    /// - `<final>...</final>` to produce the final answer
    async fn run_react_loop(
        &self,
        task: &str,
        run_started_at: &Instant,
    ) -> Result<(Vec<AgentTurn>, String, Option<u64>)> {
        let tool_manifest = self.tools.manifest_compact();
        let mut transcript = String::new();
        let mut turns = Vec::new();
        let mut first_token_latency_ms = None;

        // Build the initial ReAct prompt.
        let system_prompt = format_react_system_prompt(task, &tool_manifest);
        transcript.push_str(&system_prompt);

        for step in 0..self.max_steps {
            let output = self.brain.generate(&transcript).await?;
            if first_token_latency_ms.is_none() {
                first_token_latency_ms = Some(elapsed_ms_since(*run_started_at));
            }

            info!(step = step + 1, output = %output, "ReAct step");

            // Check for final answer first.
            if let Some(final_text) = extract_tag(&output, "final") {
                let raw_findings = derive_findings(&turns, "");
                let findings = deduplicate_findings(raw_findings);
                let answer = quality_checked_final_answer(&final_text, &findings);
                return Ok((turns, answer, first_token_latency_ms));
            }

            // Try to parse a tool call.
            if let Some(call_json) = extract_tag(&output, "call") {
                match parse_tool_call(&call_json) {
                    Some(mut call) => {
                        self.apply_coverage_baseline_to_call(&mut call);

                        let observation =
                            match self.tools.execute(&call.tool, call.args.clone()).await {
                                Ok(value) => value,
                                Err(err) => json!({ "error": err.to_string() }),
                            };

                        info!(tool = %call.tool, step = step + 1, "ReAct tool executed");

                        // Append observation to transcript for next iteration.
                        let obs_str = serde_json::to_string(&observation).unwrap_or_default();
                        let obs_truncated = if obs_str.len() > 2000 {
                            format!("{}...(truncated)", &obs_str[..2000])
                        } else {
                            obs_str
                        };
                        transcript.push_str(&format!("\n{output}\nObservation: {obs_truncated}\n"));

                        turns.push(AgentTurn {
                            thought: format!("ReAct step {}: invoking {}", step + 1, call.tool),
                            tool_call: Some(call),
                            observation: Some(observation),
                        });
                    }
                    None => {
                        debug!(step = step + 1, "ReAct output contained unparseable <call>");
                        transcript.push_str(&format!(
                            "\n{output}\nObservation: ERROR — could not parse tool call. Use <call>{{\"tool\":\"name\",\"args\":{{...}}}}</call> format.\n"
                        ));
                    }
                }
            } else {
                // No <call> or <final> tag — nudge the LLM.
                debug!(step = step + 1, "ReAct output contained no action tags");
                transcript.push_str(&format!(
                    "\n{output}\nObservation: No action detected. Respond with <call>...</call> to use a tool or <final>...</final> to finish.\n"
                ));
            }
        }

        // Max steps reached — fall back to template-driven evidence + synthesis.
        info!(
            max_steps = self.max_steps,
            "ReAct loop exhausted max steps, falling back to template synthesis"
        );
        if turns.is_empty() {
            let (template_turns, _) = self.run_template_phase(task).await;
            turns = template_turns;
        }
        let evidence_summary = build_evidence_summary(&turns);
        let synthesis_prompt = format_synthesis_prompt(task, &evidence_summary);
        let output = self.brain.generate(&synthesis_prompt).await?;
        let raw = extract_tag(&output, "final").unwrap_or(output);
        let raw_findings = derive_findings(&turns, "");
        let findings = deduplicate_findings(raw_findings);
        let answer = quality_checked_final_answer(&raw, &findings);

        Ok((turns, answer, first_token_latency_ms))
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

/// Keywords that indicate a task is within scope of host-local investigation tools.
const IN_SCOPE_KEYWORDS: &[&str] = &[
    "host",
    "account",
    "persistence",
    "network",
    "process",
    "privilege",
    "ssh",
    "listener",
    "port",
    "autorun",
    "hash",
    "integrity",
    "log",
];

/// Keywords that indicate a task targets capabilities outside the local toolset.
const OUT_OF_SCOPE_INDICATORS: &[&str] = &[
    "cloud",
    "aws",
    "azure",
    "gcp",
    "s3",
    "iam",
    "kubernetes",
    "container",
    "api",
    "email",
    "phishing",
    "siem",
];

/// Check whether a task is within the scope of available host-local tools.
///
/// Returns `Some(finding)` if the task is out of scope, `None` if in scope.
pub fn check_task_scope(task: &str) -> Option<Finding> {
    let lower = task.to_lowercase();

    let has_in_scope = IN_SCOPE_KEYWORDS.iter().any(|kw| has_word(&lower, kw));

    if has_in_scope {
        return None;
    }

    let matched_domains: Vec<&&str> = OUT_OF_SCOPE_INDICATORS
        .iter()
        .filter(|kw| has_word(&lower, kw))
        .collect();

    if matched_domains.is_empty() {
        return None;
    }

    let domain_hint = matched_domains
        .iter()
        .map(|kw| **kw)
        .collect::<Vec<_>>()
        .join(", ");

    Some(Finding::new(
        "Task is outside the scope of available host-local investigation tools".to_string(),
        FindingSeverity::Info,
        1.0,
        EvidencePointer {
            turn: None,
            tool: None,
            field: "scope_check".to_string(),
        },
        format!(
            "This task requires capabilities not present in the current toolset. Consider tools for: {domain_hint}."
        ),
    ))
}

/// Resolve the best-matching investigation template for a task.
///
/// Scores each built-in template by counting keyword matches in the task
/// description. Returns the highest scoring template, or the `broad-host-triage`
/// fallback if no template matches.
pub fn resolve_investigation_template(task: &str) -> &'static InvestigationTemplate {
    let lower = task.to_lowercase();
    let templates = builtin_investigation_templates();

    let mut best: Option<(&InvestigationTemplate, usize)> = None;

    for template in templates {
        if template.match_keywords.is_empty() {
            continue; // fallback template, skip scoring
        }

        let score = template
            .match_keywords
            .iter()
            .filter(|kw| has_word(&lower, kw))
            .count();

        if score > 0 {
            if let Some((_, best_score)) = best {
                if score > best_score {
                    best = Some((template, score));
                }
            } else {
                best = Some((template, score));
            }
        }
    }

    let selected = best.map(|(t, _)| t).unwrap_or(&templates[0]);
    info!(template = %selected.name, "investigation template selected");
    selected
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
            // Truncate large observations to keep within model context limits (#93).
            let obs_str = serde_json::to_string(obs).unwrap_or_default();
            let truncated = if obs_str.len() > 3000 {
                format!("{}...(truncated)", &obs_str[..3000])
            } else {
                obs_str
            };
            summary.push_str(&format!("[{tool_name}] {truncated}\n\n"));
            count += 1;
        }
    }
    summary
}

/// Format the synthesis prompt that asks the LLM to analyze collected evidence (#93).
///
/// Includes the task verbatim and structures the expected output format.
fn format_synthesis_prompt(task: &str, evidence: &str) -> String {
    format!(
        "You are a security analyst conducting a host-level investigation.\n\n\
         TASK (verbatim):\n{task}\n\n\
         EVIDENCE from host investigation tools:\n\
         {evidence}\n\
         Using ONLY the evidence above, write a structured report inside <final>...</final> tags.\n\
         Required sections:\n\
         SUMMARY: One-line verdict of the investigation.\n\
         FINDINGS: Numbered list of specific, evidence-backed observations.\n\
         RISK: One of critical / high / medium / low / info — with a one-line justification.\n\
         ACTIONS: Numbered remediation or follow-up steps.\n\n\
         <final>"
    )
}

/// Build the initial ReAct system prompt for LLM-guided tool selection (#92).
fn format_react_system_prompt(task: &str, tool_manifest: &str) -> String {
    format!(
        "You are an autonomous security investigation agent using a ReAct loop.\n\
         Available tools:\n{tool_manifest}\n\n\
         Task: {task}\n\n\
         At each step, decide your next action.\n\
         To call a tool, respond with: <call>{{\"tool\":\"tool_name\",\"args\":{{...}}}}</call>\n\
         When you have enough evidence, write your final report: <final>SUMMARY: ...\nFINDINGS: ...\nRISK: ...\nACTIONS: ...</final>\n\n\
         Begin your investigation.\n"
    )
}

/// Parse a tool call from a JSON string extracted from `<call>...</call>` tags.
fn parse_tool_call(json_str: &str) -> Option<ToolCall> {
    let parsed: Value = serde_json::from_str(json_str).ok()?;
    let tool = parsed.get("tool")?.as_str()?.to_string();
    let args = parsed
        .get("args")
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    Some(ToolCall { tool, args })
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
        // ReAct loop: LLM calls a tool, then produces final answer.
        let engine = MockEngine::new(vec![
            r#"<call>{"tool":"audit_account_changes","args":{}}</call>"#,
            r#"<call>{"tool":"scan_network","args":{"limit":40}}</call>"#,
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
        // ReAct loop should have executed tools from LLM decisions.
        assert!(
            report.turns.len() >= 2,
            "expected at least 2 tool turns from ReAct, got {}",
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
    async fn react_immediate_final_answer() {
        // ReAct loop: LLM immediately produces final answer without calling tools.
        let engine = MockEngine::new(vec!["<final>No significant anomalies detected.</final>"]);

        let agent = Agent::new(engine, ToolRegistry::with_default_tools());
        let report = agent
            .run("Perform a quick triage of this host")
            .await
            .expect("agent run should succeed");

        assert!(
            report.final_answer.contains("No significant anomalies"),
            "final answer: {}",
            report.final_answer
        );
    }

    #[tokio::test]
    async fn basic_tier_uses_template_driven_execution() {
        // Basic tier should use template-driven execution, not ReAct.
        let engine = MockEngine::new(vec![]);

        let agent = Agent::new(engine, ToolRegistry::with_default_tools())
            .with_capability_tier(crate::ModelCapabilityTier::Basic);
        let report = agent
            .run("Investigate suspicious account behavior")
            .await
            .expect("agent run should succeed");

        // Template-driven: should have run tools.
        assert!(
            report.turns.len() >= 3,
            "expected at least 3 tool turns from template, got {}",
            report.turns.len()
        );
    }

    #[tokio::test]
    async fn derives_structured_findings_from_tool_observations() {
        // ReAct loop: run tools then produce final answer.
        let engine = MockEngine::new(vec![
            r#"<call>{"tool":"audit_account_changes","args":{}}</call>"#,
            r#"<call>{"tool":"scan_network","args":{"limit":40}}</call>"#,
            "<final>summary</final>",
        ]);

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

    // ── Investigation template resolution tests (#84) ──

    use super::resolve_investigation_template;

    #[test]
    fn resolves_ssh_template_for_ssh_task() {
        let template = resolve_investigation_template("Investigate unauthorized SSH keys");
        assert_eq!(template.name, "ssh-key-investigation");
    }

    #[test]
    fn resolves_network_template_for_listener_task() {
        let template = resolve_investigation_template("Check suspicious listener ports");
        assert_eq!(template.name, "network-exposure-audit");
    }

    #[test]
    fn resolves_persistence_template() {
        let template = resolve_investigation_template("Analyze autorun persistence entries");
        assert_eq!(template.name, "persistence-analysis");
    }

    #[test]
    fn resolves_privilege_escalation_template() {
        let template =
            resolve_investigation_template("Review local privilege escalation indicators");
        assert_eq!(template.name, "privilege-escalation-check");
    }

    #[test]
    fn resolves_file_integrity_template() {
        let template = resolve_investigation_template("Verify hash integrity of system binaries");
        assert_eq!(template.name, "file-integrity-check");
    }

    #[test]
    fn falls_back_to_broad_triage_for_generic_task() {
        let template = resolve_investigation_template("Perform a quick triage of this host");
        assert_eq!(template.name, "broad-host-triage");
    }

    #[test]
    fn template_resolution_is_case_insensitive() {
        let template = resolve_investigation_template("CHECK SSH ACCESS NOW");
        assert_eq!(template.name, "ssh-key-investigation");
    }

    #[test]
    fn higher_keyword_count_wins() {
        // "network lateral" matches network-exposure-audit with 2 keywords
        let template =
            resolve_investigation_template("Investigate network lateral movement indicators");
        assert_eq!(template.name, "network-exposure-audit");
    }

    // ── Scope validation tests (#83) ──

    use super::check_task_scope;

    #[test]
    fn out_of_scope_cloud_task_returns_finding() {
        let finding = check_task_scope("Check if my AWS S3 buckets are misconfigured");
        assert!(finding.is_some());
        let f = finding.unwrap();
        assert_eq!(f.severity, crate::FindingSeverity::Info);
        assert!(f.title.contains("outside the scope"));
        assert!(f.recommended_action.contains("aws"));
    }

    #[test]
    fn out_of_scope_kubernetes_task_returns_finding() {
        let finding = check_task_scope("Analyze Kubernetes pod security policies");
        assert!(finding.is_some());
    }

    #[test]
    fn in_scope_host_task_returns_none() {
        let finding = check_task_scope("Investigate unauthorized SSH keys on this host");
        assert!(finding.is_none());
    }

    #[test]
    fn in_scope_network_task_returns_none() {
        let finding = check_task_scope("Check network listener ports for suspicious activity");
        assert!(finding.is_none());
    }

    #[test]
    fn mixed_scope_with_in_scope_keyword_returns_none() {
        // Has both "cloud" (out-of-scope) and "host" (in-scope) — in-scope wins
        let finding = check_task_scope("Check host logs for cloud credential leaks");
        assert!(finding.is_none());
    }

    #[test]
    fn generic_task_without_scope_keywords_returns_none() {
        // No in-scope AND no out-of-scope keywords → proceed normally
        let finding = check_task_scope("Perform a general security assessment");
        assert!(finding.is_none());
    }

    #[tokio::test]
    async fn out_of_scope_task_skips_tool_execution() {
        let engine = MockEngine::new(vec![]);
        let agent = Agent::new(engine, ToolRegistry::with_default_tools());
        let report = agent
            .run("Check if my AWS S3 buckets are misconfigured")
            .await
            .expect("agent run should succeed");

        assert!(report.turns.is_empty(), "no tools should be executed");
        assert_eq!(report.findings.len(), 1);
        assert!(report.findings[0].title.contains("outside the scope"));
    }

    // ── ReAct helpers (#92) ──

    use super::{format_react_system_prompt, parse_tool_call};

    #[test]
    fn parse_tool_call_valid_json() {
        let call = parse_tool_call(r#"{"tool":"scan_network","args":{"limit":40}}"#);
        assert!(call.is_some());
        let call = call.unwrap();
        assert_eq!(call.tool, "scan_network");
        assert_eq!(call.args["limit"], 40);
    }

    #[test]
    fn parse_tool_call_missing_args() {
        let call = parse_tool_call(r#"{"tool":"scan_network"}"#);
        assert!(call.is_some());
        let call = call.unwrap();
        assert_eq!(call.tool, "scan_network");
        assert!(call.args.is_object());
    }

    #[test]
    fn parse_tool_call_invalid_json() {
        assert!(parse_tool_call("not json").is_none());
    }

    #[test]
    fn parse_tool_call_missing_tool_field() {
        assert!(parse_tool_call(r#"{"args":{}}"#).is_none());
    }

    #[test]
    fn react_system_prompt_includes_task_and_tools() {
        let prompt = format_react_system_prompt("Check SSH keys", "- scan_network: scan");
        assert!(prompt.contains("Check SSH keys"));
        assert!(prompt.contains("scan_network"));
        assert!(prompt.contains("<call>"));
        assert!(prompt.contains("<final>"));
    }

    #[tokio::test]
    async fn react_loop_handles_unknown_tool_gracefully() {
        let engine = MockEngine::new(vec![
            r#"<call>{"tool":"nonexistent_tool","args":{}}</call>"#,
            "<final>Investigation complete with partial data.</final>",
        ]);

        let agent = Agent::new(engine, ToolRegistry::with_default_tools());
        let report = agent
            .run("Investigate suspicious network activity")
            .await
            .expect("agent run should succeed");

        // The unknown tool should produce an error observation, not crash.
        assert!(!report.turns.is_empty());
        let obs = report.turns[0].observation.as_ref().unwrap();
        assert!(obs.get("error").is_some());
    }
}
