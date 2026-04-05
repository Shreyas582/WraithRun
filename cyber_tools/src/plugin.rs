//! Plugin tool discovery and execution.
//!
//! Plugins are external tools defined by a `tool.toml` manifest and executed as
//! subprocesses. They communicate via JSON on stdin/stdout and are subject to
//! the same sandbox policy as built-in tools.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, warn};

use crate::{SandboxPolicy, Tool, ToolError, ToolSpec};

/// Default timeout for plugin process execution (30 seconds).
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum plugin stdout size (1 MiB).
const MAX_STDOUT_BYTES: usize = 1_048_576;

// ---------------------------------------------------------------------------
// Manifest types
// ---------------------------------------------------------------------------

/// Parsed `tool.toml` manifest for a plugin tool.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub version: String,
    /// Command to execute (relative to plugin directory).
    pub command: String,
    /// Supported platforms. Empty means all platforms.
    #[serde(default)]
    pub platforms: Vec<String>,
    /// Optional timeout override in seconds.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Parameter definitions.
    #[serde(default)]
    pub parameters: std::collections::HashMap<String, PluginParameter>,
}

/// A single parameter definition in the plugin manifest.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginParameter {
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: String,
}

// ---------------------------------------------------------------------------
// Plugin tool
// ---------------------------------------------------------------------------

/// A tool backed by an external subprocess.
#[derive(Debug, Clone)]
pub struct PluginTool {
    manifest: PluginManifest,
    /// Absolute path to the plugin directory.
    plugin_dir: PathBuf,
    /// Resolved absolute path to the command.
    command_path: PathBuf,
    timeout: Duration,
}

impl PluginTool {
    /// Resolve the command path from the manifest.
    fn resolve_command(plugin_dir: &Path, command: &str) -> PathBuf {
        let path = Path::new(command);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            plugin_dir.join(path)
        }
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
}

#[async_trait]
impl Tool for PluginTool {
    fn spec(&self) -> ToolSpec {
        // Build args schema from parameters.
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for (name, param) in &self.manifest.parameters {
            let mut prop = serde_json::Map::new();
            prop.insert("type".to_string(), json!(param.param_type));
            if !param.description.is_empty() {
                prop.insert("description".to_string(), json!(param.description));
            }
            properties.insert(name.clone(), Value::Object(prop));
            if param.required {
                required.push(json!(name));
            }
        }
        let schema = json!({
            "type": "object",
            "properties": properties,
            "required": required,
        });
        ToolSpec {
            name: self.manifest.name.clone(),
            description: self.manifest.description.clone(),
            args_schema: schema,
        }
    }

    async fn run(&self, args: Value) -> Result<Value, ToolError> {
        let stdin_payload = serde_json::to_string(&args)
            .map_err(|e| ToolError::Execution(format!("serialize args: {e}")))?;

        debug!(
            plugin = %self.manifest.name,
            command = %self.command_path.display(),
            "executing plugin tool"
        );

        let mut cmd = tokio::process::Command::new(&self.command_path);
        cmd.current_dir(&self.plugin_dir);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            ToolError::Execution(format!(
                "failed to spawn plugin '{}': {e}",
                self.manifest.name
            ))
        })?;

        // Write stdin.
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(stdin_payload.as_bytes()).await;
            drop(stdin);
        }

        // Wait with timeout.
        let output = tokio::time::timeout(self.timeout, child.wait_with_output())
            .await
            .map_err(|_| {
                ToolError::Execution(format!(
                    "plugin '{}' timed out after {}s",
                    self.manifest.name,
                    self.timeout.as_secs()
                ))
            })?
            .map_err(|e| {
                ToolError::Execution(format!(
                    "plugin '{}' execution error: {e}",
                    self.manifest.name
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError::Execution(format!(
                "plugin '{}' exited with {}: {}",
                self.manifest.name,
                output.status,
                stderr.chars().take(500).collect::<String>()
            )));
        }

        // Parse stdout as JSON.
        let stdout = &output.stdout[..output.stdout.len().min(MAX_STDOUT_BYTES)];
        let result: Value =
            serde_json::from_slice(stdout).map_err(|e| {
                ToolError::Execution(format!(
                    "plugin '{}' produced invalid JSON: {e}",
                    self.manifest.name
                ))
            })?;

        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Configuration for plugin loading.
#[derive(Debug, Clone)]
pub struct PluginConfig {
    /// Directory to scan for plugin subdirectories.
    pub tools_dir: PathBuf,
    /// Explicit allow-list of plugin names. Empty = deny all.
    pub allowed_plugins: HashSet<String>,
}

impl PluginConfig {
    pub fn new(tools_dir: PathBuf, allowed: Vec<String>) -> Self {
        Self {
            tools_dir,
            allowed_plugins: allowed.into_iter().collect(),
        }
    }

    /// Default tools directory: `~/.config/wraithrun/tools/`.
    pub fn default_tools_dir() -> PathBuf {
        dirs_path().join("tools")
    }
}

fn dirs_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("wraithrun")
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("wraithrun")
    }
}

/// Scan a tools directory and return loaded plugin tools.
///
/// Each subdirectory must contain a `tool.toml`. Plugins not in the
/// `allowed_plugins` set are skipped.
pub fn discover_plugins(
    config: &PluginConfig,
    policy: &SandboxPolicy,
) -> Vec<Arc<dyn Tool>> {
    let mut tools: Vec<Arc<dyn Tool>> = Vec::new();

    let entries = match std::fs::read_dir(&config.tools_dir) {
        Ok(entries) => entries,
        Err(e) => {
            debug!(
                dir = %config.tools_dir.display(),
                error = %e,
                "plugin directory not found, skipping plugin discovery"
            );
            return tools;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("tool.toml");
        if !manifest_path.is_file() {
            debug!(dir = %path.display(), "skipping directory without tool.toml");
            continue;
        }

        match load_plugin(&path, &manifest_path, config, policy) {
            Ok(tool) => {
                debug!(name = %tool.manifest().name, "loaded plugin tool");
                tools.push(Arc::new(tool));
            }
            Err(e) => {
                warn!(dir = %path.display(), error = %e, "failed to load plugin");
            }
        }
    }

    tools
}

fn load_plugin(
    plugin_dir: &Path,
    manifest_path: &Path,
    config: &PluginConfig,
    policy: &SandboxPolicy,
) -> Result<PluginTool, ToolError> {
    let content = std::fs::read_to_string(manifest_path)
        .map_err(|e| ToolError::Execution(format!("read manifest: {e}")))?;

    let manifest: PluginManifest = toml::from_str(&content)
        .map_err(|e| ToolError::Execution(format!("parse manifest: {e}")))?;

    // Check allow-list.
    if !config.allowed_plugins.contains(&manifest.name) {
        return Err(ToolError::PolicyDenied(format!(
            "plugin '{}' is not in the allowed_plugins list",
            manifest.name
        )));
    }

    // Check platform.
    if !manifest.platforms.is_empty() {
        let current = current_platform();
        if !manifest.platforms.iter().any(|p| p == current) {
            return Err(ToolError::Execution(format!(
                "plugin '{}' does not support platform '{current}'",
                manifest.name
            )));
        }
    }

    // Resolve and validate command.
    let command_path = PluginTool::resolve_command(plugin_dir, &manifest.command);
    if !command_path.is_file() {
        return Err(ToolError::Execution(format!(
            "plugin command not found: {}",
            command_path.display()
        )));
    }

    // Validate command against sandbox policy.
    let cmd_name = command_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if policy.command_denylist.contains(&cmd_name.to_ascii_lowercase()) {
        return Err(ToolError::PolicyDenied(format!(
            "plugin command '{cmd_name}' is in the sandbox denylist"
        )));
    }

    let timeout = Duration::from_secs(manifest.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));

    Ok(PluginTool {
        manifest,
        plugin_dir: plugin_dir.to_path_buf(),
        command_path,
        timeout,
    })
}

fn current_platform() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "unknown"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_manifest(dir: &Path, content: &str) {
        let mut f = std::fs::File::create(dir.join("tool.toml")).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn parse_manifest() {
        let toml_str = r#"
name = "test_tool"
description = "A test plugin"
version = "0.1.0"
command = "./run.sh"
platforms = ["linux", "macos"]

[parameters]
target = { type = "string", required = true, description = "target path" }
"#;
        let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.name, "test_tool");
        assert_eq!(manifest.platforms.len(), 2);
        assert!(manifest.parameters.contains_key("target"));
        assert!(manifest.parameters["target"].required);
    }

    #[test]
    fn discover_skips_disallowed_plugins() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("my_plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(
            &plugin_dir,
            r#"
name = "my_plugin"
description = "test"
command = "./run.sh"
"#,
        );
        // Create a dummy command file.
        std::fs::write(plugin_dir.join("run.sh"), "#!/bin/sh\necho {}").unwrap();

        let config = PluginConfig::new(tmp.path().to_path_buf(), vec![]); // empty allow-list
        let policy = SandboxPolicy::strict_default();
        let tools = discover_plugins(&config, &policy);
        assert!(tools.is_empty(), "disallowed plugin should not be loaded");
    }

    #[test]
    fn discover_loads_allowed_plugin() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("allowed_plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(
            &plugin_dir,
            &format!(
                r#"
name = "allowed_plugin"
description = "an allowed test plugin"
command = "./run.sh"
platforms = ["{platform}"]
"#,
                platform = current_platform()
            ),
        );
        // Create a dummy command file.
        let cmd_path = plugin_dir.join("run.sh");
        std::fs::write(&cmd_path, "#!/bin/sh\necho {}").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&cmd_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let config = PluginConfig::new(
            tmp.path().to_path_buf(),
            vec!["allowed_plugin".to_string()],
        );
        let policy = SandboxPolicy::strict_default();
        let tools = discover_plugins(&config, &policy);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].spec().name, "allowed_plugin");
    }

    #[test]
    fn discover_skips_wrong_platform() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("wrong_platform");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(
            &plugin_dir,
            r#"
name = "wrong_platform"
description = "wrong platform"
command = "./run.sh"
platforms = ["nonexistent_os"]
"#,
        );
        std::fs::write(plugin_dir.join("run.sh"), "#!/bin/sh\necho {}").unwrap();

        let config = PluginConfig::new(
            tmp.path().to_path_buf(),
            vec!["wrong_platform".to_string()],
        );
        let policy = SandboxPolicy::strict_default();
        let tools = discover_plugins(&config, &policy);
        assert!(tools.is_empty(), "wrong-platform plugin should not load");
    }

    #[test]
    fn plugin_spec_includes_parameters() {
        let manifest = PluginManifest {
            name: "test".to_string(),
            description: "desc".to_string(),
            version: "0.1.0".to_string(),
            command: "./run".to_string(),
            platforms: vec![],
            timeout_secs: None,
            parameters: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "path".to_string(),
                    PluginParameter {
                        param_type: "string".to_string(),
                        required: true,
                        description: "target path".to_string(),
                    },
                );
                m
            },
        };
        let tool = PluginTool {
            manifest,
            plugin_dir: PathBuf::from("."),
            command_path: PathBuf::from("./run"),
            timeout: Duration::from_secs(30),
        };
        let spec = tool.spec();
        assert_eq!(spec.name, "test");
        let props = spec.args_schema["properties"].as_object().unwrap();
        assert!(props.contains_key("path"));
        let required = spec.args_schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
    }
}
