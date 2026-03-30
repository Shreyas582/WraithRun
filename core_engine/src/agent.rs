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