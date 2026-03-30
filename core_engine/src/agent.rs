use anyhow::Result;
use serde_json::json;
use tracing::info;

use cyber_tools::ToolRegistry;
use inference_bridge::InferenceEngine;

use crate::{extract_tag, format_system_prompt, parse_tool_call, AgentTurn, RunReport};

pub struct Agent<B: InferenceEngine> {
    brain: B,
    tools: ToolRegistry,
    max_steps: usize,
}

impl<B: InferenceEngine> Agent<B> {
    pub fn new(brain: B, tools: ToolRegistry) -> Self {
        Self {
            brain,
            tools,
            max_steps: 8,
        }
    }

    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps.max(1);
        self
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
                return Ok(RunReport {
                    task: task.to_string(),
                    turns,
                    final_answer,
                });
            }

            if let Some(call) = parse_tool_call(&output) {
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

            return Ok(RunReport {
                task: task.to_string(),
                turns,
                final_answer: output,
            });
        }

        Ok(RunReport {
            task: task.to_string(),
            turns,
            final_answer: "Maximum step count reached before receiving <final>.".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use anyhow::Result;
    use async_trait::async_trait;

    use cyber_tools::ToolRegistry;
    use inference_bridge::InferenceEngine;

    use super::Agent;

    #[derive(Clone)]
    struct MockEngine {
        responses: Arc<Mutex<VecDeque<String>>>,
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
