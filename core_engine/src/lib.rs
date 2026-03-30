pub mod agent;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    pub thought: String,
    pub tool_call: Option<ToolCall>,
    pub observation: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub task: String,
    pub turns: Vec<AgentTurn>,
    pub final_answer: String,
}

pub fn format_system_prompt(tool_manifest_json: &str) -> String {
    format!(
        "You are a local-first cyber operations agent.\n\
         Reason carefully, use tools when needed, and keep outputs machine-parseable.\n\n\
         Available tools JSON:\n\
         {tool_manifest_json}\n\n\
         Output contract:\n\
         - Tool call format: <call>{{\"tool\":\"tool_name\",\"args\":{{...}}}}</call>\n\
         - Final answer format: <final>your conclusion</final>\n\
         - Never emit both <call> and <final> in the same response."
    )
}

pub fn extract_tag(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");

    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;

    Some(text[start..end].trim().to_string())
}

pub fn parse_tool_call(text: &str) -> Option<ToolCall> {
    let body = extract_tag(text, "call")?;

    if let Ok(call) = serde_json::from_str::<ToolCall>(&body) {
        return Some(call);
    }

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(ToolCall {
        tool: trimmed.to_string(),
        args: Value::Object(serde_json::Map::new()),
    })
}

#[cfg(test)]
mod tests {
    use super::{extract_tag, parse_tool_call};

    #[test]
    fn parses_json_tool_call() {
        let text = r#"<call>{"tool":"scan_network","args":{"limit":5}}</call>"#;
        let call = parse_tool_call(text).expect("tool call should parse");

        assert_eq!(call.tool, "scan_network");
        assert_eq!(call.args["limit"], 5);
    }

    #[test]
    fn extracts_final_tag() {
        let text = "noise <final>done</final> trailing";
        let final_text = extract_tag(text, "final").expect("final tag should parse");
        assert_eq!(final_text, "done");
    }
}
