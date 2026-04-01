use anyhow::Result;
use serde_json::{json, Map, Value};
use tracing::info;

use cyber_tools::ToolRegistry;
use inference_bridge::InferenceEngine;

use crate::{
    derive_findings, extract_tag, format_system_prompt, parse_tool_call, AgentTurn,
    CoverageBaseline, RunReport, ToolCall,
};

pub struct Agent<B: InferenceEngine> {
    brain: B,
    tools: ToolRegistry,
    max_steps: usize,
    coverage_baseline: Option<CoverageBaseline>,
}

impl<B: InferenceEngine> Agent<B> {
    pub fn new(brain: B, tools: ToolRegistry) -> Self {
        Self {
            brain,
            tools,
            max_steps: 8,
            coverage_baseline: None,
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
        let mut turns = Vec::new();
        let mut transcript = format!("Task: {task}\n");
        let system_prompt = format_system_prompt(&self.tools.manifest_json_pretty());

        for step in 0..self.max_steps {
            let prompt = format!(
                "{system_prompt}\n\
                 ReAct transcript so far:\n\
                 {transcript}\n\
                 Decide your next action."
            );

            let output = self.brain.generate(&prompt).await?;
            info!(step = step + 1, output = %output, "agent brain output");

            if let Some(final_answer) = extract_tag(&output, "final") {
                let findings = derive_findings(&turns, &final_answer);
                return Ok(RunReport {
                    task: task.to_string(),
                    case_id: None,
                    live_fallback_decision: None,
                    turns,
                    final_answer,
                    findings,
                });
            }

            if let Some(mut call) = parse_tool_call(&output) {
                self.apply_coverage_baseline_to_call(&mut call);

                let observation = match self.tools.execute(&call.tool, call.args.clone()).await {
                    Ok(value) => value,
                    Err(err) => json!({ "error": err.to_string() }),
                };

                transcript.push_str(&format!(
                    "Assistant: {output}\nObservation: {observation}\n"
                ));

                turns.push(AgentTurn {
                    thought: output,
                    tool_call: Some(call),
                    observation: Some(observation),
                });

                continue;
            }

            turns.push(AgentTurn {
                thought: output.clone(),
                tool_call: None,
                observation: None,
            });

            let findings = derive_findings(&turns, &output);

            return Ok(RunReport {
                task: task.to_string(),
                case_id: None,
                live_fallback_decision: None,
                turns,
                final_answer: output,
                findings,
            });
        }

        let final_answer = "Maximum step count reached before receiving <final>.".to_string();
        let findings = derive_findings(&turns, &final_answer);

        Ok(RunReport {
            task: task.to_string(),
            case_id: None,
            live_fallback_decision: None,
            turns,
            final_answer,
            findings,
        })
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
    async fn executes_tool_then_finalizes() {
        let engine = MockEngine::new(vec![
            r#"<call>{"tool":"unknown_tool","args":{}}</call>"#,
            "<final>investigation completed</final>",
        ]);

        let agent = Agent::new(engine, ToolRegistry::with_default_tools()).with_max_steps(4);
        let report = agent
            .run("Investigate suspicious account behavior")
            .await
            .expect("agent run should succeed");

        assert_eq!(report.final_answer, "investigation completed");
        assert_eq!(report.turns.len(), 1);
        assert_eq!(
            report.turns[0]
                .tool_call
                .as_ref()
                .expect("tool call should exist")
                .tool,
            "unknown_tool"
        );
        assert!(report.turns[0]
            .observation
            .as_ref()
            .expect("observation should exist")
            .get("error")
            .is_some());
    }

    #[tokio::test]
    async fn returns_direct_output_when_no_tags_are_present() {
        let engine = MockEngine::new(vec!["No significant anomalies detected."]);

        let agent = Agent::new(engine, ToolRegistry::with_default_tools()).with_max_steps(3);
        let report = agent
            .run("Perform a quick triage")
            .await
            .expect("agent run should succeed");

        assert_eq!(report.final_answer, "No significant anomalies detected.");
        assert_eq!(report.turns.len(), 1);
        assert!(report.turns[0].tool_call.is_none());
        assert!(report.turns[0].observation.is_none());
    }

    #[tokio::test]
    async fn stops_when_max_steps_is_reached() {
        let engine = MockEngine::new(vec![
            r#"<call>{"tool":"unknown_tool","args":{}}</call>"#,
            r#"<call>{"tool":"unknown_tool","args":{}}</call>"#,
            r#"<call>{"tool":"unknown_tool","args":{}}</call>"#,
        ]);

        let agent = Agent::new(engine, ToolRegistry::with_default_tools()).with_max_steps(2);
        let report = agent
            .run("Keep collecting observations")
            .await
            .expect("agent run should succeed");

        assert_eq!(report.turns.len(), 2);
        assert_eq!(
            report.final_answer,
            "Maximum step count reached before receiving <final>."
        );
    }
}
