//! Execution provider backend abstraction.
//!
//! This module defines the [`ExecutionProviderBackend`] trait that decouples
//! the inference loop from any specific hardware execution provider. Backends
//! register themselves in a [`ProviderRegistry`] and are auto-selected based
//! on availability and priority.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ModelConfig;

// ---------------------------------------------------------------------------
// Diagnostic types
// ---------------------------------------------------------------------------

/// Severity of a provider diagnostic entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Pass,
    Warn,
    Fail,
}

impl fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pass => write!(f, "pass"),
            Self::Warn => write!(f, "warn"),
            Self::Fail => write!(f, "fail"),
        }
    }
}

/// A single diagnostic entry produced by a backend's self-check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticEntry {
    pub severity: DiagnosticSeverity,
    pub check: String,
    pub message: String,
}

impl DiagnosticEntry {
    pub fn pass(check: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Pass,
            check: check.into(),
            message: message.into(),
        }
    }

    pub fn warn(check: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warn,
            check: check.into(),
            message: message.into(),
        }
    }

    pub fn fail(check: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Fail,
            check: check.into(),
            message: message.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Model format and quantization types
// ---------------------------------------------------------------------------

/// Supported model file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFormat {
    Onnx,
    Gguf,
    SafeTensors,
}

impl ModelFormat {
    /// Detect model format from file extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("onnx") => Some(Self::Onnx),
            Some("gguf") => Some(Self::Gguf),
            Some("safetensors") => Some(Self::SafeTensors),
            _ => None,
        }
    }
}

impl fmt::Display for ModelFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Onnx => write!(f, "ONNX"),
            Self::Gguf => write!(f, "GGUF"),
            Self::SafeTensors => write!(f, "SafeTensors"),
        }
    }
}

/// Quantization format of a model's weights.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantFormat {
    Fp32,
    Fp16,
    Int8,
    Int4,
    /// Block-quantized format (e.g. "awq", "gptq", "bnb-nf4").
    BlockQuantized(String),
    Unknown,
}

impl fmt::Display for QuantFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fp32 => write!(f, "FP32"),
            Self::Fp16 => write!(f, "FP16"),
            Self::Int8 => write!(f, "INT8"),
            Self::Int4 => write!(f, "INT4"),
            Self::BlockQuantized(name) => write!(f, "BlockQuantized({})", name),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl QuantFormat {
    /// Detect quantization format from ONNX model path by inspecting file name
    /// conventions and model metadata heuristics.
    pub fn detect_from_path(path: &Path) -> Self {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if stem.contains("int4") || stem.contains("q4") {
            Self::Int4
        } else if stem.contains("int8") || stem.contains("q8") || stem.contains("quantized") {
            Self::Int8
        } else if stem.contains("fp16") || stem.contains("f16") {
            Self::Fp16
        } else if stem.contains("fp32") || stem.contains("f32") {
            Self::Fp32
        } else if stem.contains("awq") {
            Self::BlockQuantized("awq".to_string())
        } else if stem.contains("gptq") {
            Self::BlockQuantized("gptq".to_string())
        } else {
            Self::Unknown
        }
    }
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

/// Information about a registered backend, returned by the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub priority: u32,
    pub available: bool,
}

/// Full diagnostic output for a single backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendDiagnostics {
    pub info: ProviderInfo,
    pub diagnostics: Vec<DiagnosticEntry>,
}

/// Abstraction over a hardware execution provider.
///
/// Backends report their availability at runtime (not just compile time) so
/// the registry can probe what is actually present on the host. Each backend
/// can produce an inference session and provider-specific diagnostics.
pub trait ExecutionProviderBackend: Send + Sync {
    /// Human-readable name (e.g. "CPU", "AMD Vitis NPU", "NVIDIA CUDA").
    fn name(&self) -> &str;

    /// Whether this backend is available on the current system.
    ///
    /// Backends should probe hardware/driver presence, not just check compile
    /// flags. A compiled backend where the driver is missing returns `false`.
    fn is_available(&self) -> bool;

    /// Priority for auto-selection. Higher values are preferred.
    ///
    /// Suggested baseline: CPU = 0, DirectML = 100, CoreML = 100,
    /// CUDA = 200, Vitis NPU = 300.
    fn priority(&self) -> u32;

    /// Provider-specific configuration keys that this backend reads from
    /// [`BackendOptions`] (e.g. `"device_id"`, `"config_file"`).
    fn config_keys(&self) -> &[&str] {
        &[]
    }

    /// Model formats this backend supports. Defaults to ONNX only.
    fn supported_formats(&self) -> Vec<ModelFormat> {
        vec![ModelFormat::Onnx]
    }

    /// Quantization formats this backend supports efficiently.
    fn supported_quant_formats(&self) -> Vec<QuantFormat> {
        vec![QuantFormat::Fp32, QuantFormat::Unknown]
    }

    /// Run provider-specific diagnostic checks.
    fn diagnose(&self) -> Vec<DiagnosticEntry>;

    /// Create a ready-to-use inference session for the given model config
    /// and provider-specific options.
    ///
    /// This is the primary extension point. The provider translates
    /// [`ModelConfig`] + [`BackendOptions`] into whatever internal session
    /// type the runtime needs.
    fn build_session(
        &self,
        config: &ModelConfig,
        options: &BackendOptions,
    ) -> anyhow::Result<Box<dyn InferenceSession>>;
}

/// Provider-specific options passed through from CLI/config.
///
/// This is a string-keyed map so that new backends can read their own config
/// keys without changing the core types.
pub type BackendOptions = HashMap<String, String>;

// ---------------------------------------------------------------------------
// Session trait
// ---------------------------------------------------------------------------

/// A provider-created inference session.
///
/// The inference loop calls `generate` regardless of which backend produced
/// the session.
pub trait InferenceSession: Send + Sync {
    /// Generate text from a prompt using this session.
    fn generate(&self, prompt: &str, max_new_tokens: usize) -> anyhow::Result<String>;
}

// ---------------------------------------------------------------------------
// Provider registry
// ---------------------------------------------------------------------------

/// Runtime registry of execution provider backends.
///
/// Created once at startup, it discovers which compile-time-enabled backends
/// are available and provides selection by priority or by name.
pub struct ProviderRegistry {
    backends: Vec<Box<dyn ExecutionProviderBackend>>,
}

impl ProviderRegistry {
    /// Build a registry with all compile-time-enabled backends.
    ///
    /// Each backend probes its own availability. The registry stores all
    /// backends (available or not) for diagnostic listing.
    pub fn discover() -> Self {
        #[allow(unused_mut)]
        let mut backends: Vec<Box<dyn ExecutionProviderBackend>> = vec![Box::new(CpuBackend)];

        // Vitis backend (only when compiled with the `vitis` feature).
        #[cfg(feature = "vitis")]
        backends.push(Box::new(VitisBackend));

        #[cfg(feature = "directml")]
        backends.push(Box::new(DirectMlBackend));

        #[cfg(feature = "coreml")]
        backends.push(Box::new(CoreMlBackend));

        #[cfg(feature = "cuda")]
        backends.push(Box::new(CudaBackend));

        #[cfg(feature = "tensorrt")]
        backends.push(Box::new(TensorRtBackend));

        #[cfg(feature = "qnn")]
        backends.push(Box::new(QnnBackend));

        Self { backends }
    }

    /// Returns the highest-priority available backend.
    pub fn best_available(&self) -> Option<&dyn ExecutionProviderBackend> {
        self.backends
            .iter()
            .filter(|b| b.is_available())
            .max_by_key(|b| b.priority())
            .map(|b| b.as_ref())
    }

    /// Returns a specific backend by name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&dyn ExecutionProviderBackend> {
        let name_lower = name.to_ascii_lowercase();
        self.backends
            .iter()
            .find(|b| b.name().to_ascii_lowercase() == name_lower)
            .map(|b| b.as_ref())
    }

    /// Lists all registered backends with availability status.
    pub fn list(&self) -> Vec<ProviderInfo> {
        self.backends
            .iter()
            .map(|b| ProviderInfo {
                name: b.name().to_string(),
                priority: b.priority(),
                available: b.is_available(),
            })
            .collect()
    }

    /// Run diagnostics on every registered backend and return results.
    ///
    /// Each entry pairs the backend's [`ProviderInfo`] with its diagnostic
    /// entries, sorted by descending priority so the most-preferred backend
    /// appears first.
    pub fn diagnose_all(&self) -> Vec<BackendDiagnostics> {
        let mut results: Vec<BackendDiagnostics> = self
            .backends
            .iter()
            .map(|b| BackendDiagnostics {
                info: ProviderInfo {
                    name: b.name().to_string(),
                    priority: b.priority(),
                    available: b.is_available(),
                },
                diagnostics: b.diagnose(),
            })
            .collect();
        results.sort_by_key(|b| std::cmp::Reverse(b.info.priority));
        results
    }

    /// Lists the names of all available backends.
    pub fn available_names(&self) -> Vec<String> {
        self.backends
            .iter()
            .filter(|b| b.is_available())
            .map(|b| b.name().to_string())
            .collect()
    }

    /// Try to build a session using the specified backend, with automatic
    /// fallback to the next-best available backend on failure.
    pub fn build_session_with_fallback(
        &self,
        config: &ModelConfig,
        options: &BackendOptions,
        preferred: Option<&str>,
    ) -> anyhow::Result<(String, Box<dyn InferenceSession>)> {
        // If a preferred backend is specified, try it first.
        if let Some(name) = preferred {
            if let Some(backend) = self.get(name) {
                if backend.is_available() {
                    match backend.build_session(config, options) {
                        Ok(session) => return Ok((backend.name().to_string(), session)),
                        Err(e) => {
                            tracing::warn!(
                                backend = backend.name(),
                                error = %e,
                                "preferred backend failed, trying fallback"
                            );
                        }
                    }
                }
            }
        }

        // Auto-select: try backends by descending priority.
        let mut candidates: Vec<&dyn ExecutionProviderBackend> = self
            .backends
            .iter()
            .filter(|b| b.is_available())
            .map(|b| b.as_ref())
            .collect();
        candidates.sort_by_key(|b| std::cmp::Reverse(b.priority()));

        for backend in candidates {
            match backend.build_session(config, options) {
                Ok(session) => return Ok((backend.name().to_string(), session)),
                Err(e) => {
                    tracing::warn!(
                        backend = backend.name(),
                        error = %e,
                        "backend session init failed, trying next"
                    );
                }
            }
        }

        anyhow::bail!("no execution provider backend could build a session")
    }
}

impl fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderRegistry")
            .field("backends", &self.list())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Built-in: CPU backend
// ---------------------------------------------------------------------------

/// CPU-only execution provider. Always available.
pub struct CpuBackend;

impl ExecutionProviderBackend for CpuBackend {
    fn name(&self) -> &str {
        "CPU"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn priority(&self) -> u32 {
        0
    }

    fn supported_quant_formats(&self) -> Vec<QuantFormat> {
        vec![
            QuantFormat::Fp32,
            QuantFormat::Fp16,
            QuantFormat::Int8,
            QuantFormat::Unknown,
        ]
    }

    fn diagnose(&self) -> Vec<DiagnosticEntry> {
        vec![DiagnosticEntry::pass(
            "cpu-backend",
            "CPU execution provider is always available",
        )]
    }

    fn build_session(
        &self,
        config: &ModelConfig,
        _options: &BackendOptions,
    ) -> anyhow::Result<Box<dyn InferenceSession>> {
        if config.dry_run {
            return Ok(Box::new(DryRunSession));
        }

        #[cfg(feature = "onnx")]
        {
            // Delegate to the existing onnx_vitis module for CPU session building.
            // This is a proof-of-concept bridge — full extraction happens in #51.
            Ok(Box::new(OnnxCpuSession {
                config: config.clone(),
            }))
        }

        #[cfg(not(feature = "onnx"))]
        {
            Ok(Box::new(DryRunSession))
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in: Vitis backend (cfg-gated)
// ---------------------------------------------------------------------------

/// AMD Vitis AI NPU execution provider.
#[cfg(feature = "vitis")]
pub struct VitisBackend;

#[cfg(feature = "vitis")]
impl ExecutionProviderBackend for VitisBackend {
    fn name(&self) -> &str {
        "AMD Vitis NPU"
    }

    fn is_available(&self) -> bool {
        // Check if Vitis runtime is discoverable.
        // Full hardware probe will be implemented in #50.
        std::env::var("RYZEN_AI_INSTALLER_PATH").is_ok()
            || std::env::var("XLNX_VART_FIRMWARE").is_ok()
            || cfg!(feature = "vitis")
    }

    fn priority(&self) -> u32 {
        300
    }

    fn config_keys(&self) -> &[&str] {
        &["config_file", "cache_dir", "cache_key"]
    }

    fn supported_quant_formats(&self) -> Vec<QuantFormat> {
        vec![QuantFormat::Int8, QuantFormat::Int4, QuantFormat::Unknown]
    }

    fn diagnose(&self) -> Vec<DiagnosticEntry> {
        let mut entries = vec![];
        if std::env::var("RYZEN_AI_INSTALLER_PATH").is_ok() {
            entries.push(DiagnosticEntry::pass(
                "vitis-sdk",
                "RYZEN_AI_INSTALLER_PATH is set",
            ));
        } else {
            entries.push(DiagnosticEntry::warn(
                "vitis-sdk",
                "RYZEN_AI_INSTALLER_PATH not set; Vitis NPU may not be available",
            ));
        }
        entries
    }

    fn build_session(
        &self,
        config: &ModelConfig,
        _options: &BackendOptions,
    ) -> anyhow::Result<Box<dyn InferenceSession>> {
        if config.dry_run {
            return Ok(Box::new(DryRunSession));
        }
        // Proof of concept — full Vitis session extraction happens in #50.
        // For now, delegate to the existing monolithic path via OnnxCpuSession.
        #[cfg(feature = "onnx")]
        {
            Ok(Box::new(OnnxCpuSession {
                config: config.clone(),
            }))
        }

        #[cfg(not(feature = "onnx"))]
        {
            anyhow::bail!("Vitis backend requires the 'onnx' feature")
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in: DirectML backend (cfg-gated)
// ---------------------------------------------------------------------------

/// DirectML GPU execution provider for Windows (DX12).
#[cfg(feature = "directml")]
pub struct DirectMlBackend;

#[cfg(feature = "directml")]
impl ExecutionProviderBackend for DirectMlBackend {
    fn name(&self) -> &str {
        "DirectML"
    }

    fn is_available(&self) -> bool {
        // DirectML requires Windows with a DX12 GPU.
        // At compile-time we gate on the feature; at runtime we check the OS.
        cfg!(target_os = "windows")
    }

    fn priority(&self) -> u32 {
        100
    }

    fn config_keys(&self) -> &[&str] {
        &["device_id"]
    }

    fn supported_quant_formats(&self) -> Vec<QuantFormat> {
        vec![
            QuantFormat::Fp32,
            QuantFormat::Fp16,
            QuantFormat::Int8,
            QuantFormat::Unknown,
        ]
    }

    fn diagnose(&self) -> Vec<DiagnosticEntry> {
        let mut entries = vec![];
        if cfg!(target_os = "windows") {
            entries.push(DiagnosticEntry::pass(
                "directml-platform",
                "Running on Windows — DirectML is supported",
            ));
        } else {
            entries.push(DiagnosticEntry::fail(
                "directml-platform",
                "DirectML requires Windows with a DirectX 12 GPU",
            ));
        }
        entries
    }

    fn build_session(
        &self,
        config: &ModelConfig,
        _options: &BackendOptions,
    ) -> anyhow::Result<Box<dyn InferenceSession>> {
        if config.dry_run {
            return Ok(Box::new(DryRunSession));
        }

        #[cfg(feature = "onnx")]
        {
            Ok(Box::new(OnnxCpuSession {
                config: config.clone(),
            }))
        }

        #[cfg(not(feature = "onnx"))]
        {
            anyhow::bail!("DirectML backend requires the 'onnx' feature")
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in: CoreML backend (cfg-gated)
// ---------------------------------------------------------------------------

/// Apple CoreML execution provider for macOS / Apple Silicon.
#[cfg(feature = "coreml")]
pub struct CoreMlBackend;

#[cfg(feature = "coreml")]
impl ExecutionProviderBackend for CoreMlBackend {
    fn name(&self) -> &str {
        "CoreML"
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn priority(&self) -> u32 {
        100
    }

    fn config_keys(&self) -> &[&str] {
        &["cache_dir"]
    }

    fn supported_quant_formats(&self) -> Vec<QuantFormat> {
        vec![
            QuantFormat::Fp32,
            QuantFormat::Fp16,
            QuantFormat::Int8,
            QuantFormat::Unknown,
        ]
    }

    fn diagnose(&self) -> Vec<DiagnosticEntry> {
        let mut entries = vec![];
        if cfg!(target_os = "macos") {
            entries.push(DiagnosticEntry::pass(
                "coreml-platform",
                "Running on macOS — CoreML is supported",
            ));
        } else {
            entries.push(DiagnosticEntry::fail(
                "coreml-platform",
                "CoreML requires macOS",
            ));
        }
        entries
    }

    fn build_session(
        &self,
        config: &ModelConfig,
        _options: &BackendOptions,
    ) -> anyhow::Result<Box<dyn InferenceSession>> {
        if config.dry_run {
            return Ok(Box::new(DryRunSession));
        }

        #[cfg(feature = "onnx")]
        {
            Ok(Box::new(OnnxCpuSession {
                config: config.clone(),
            }))
        }

        #[cfg(not(feature = "onnx"))]
        {
            anyhow::bail!("CoreML backend requires the 'onnx' feature")
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in: CUDA backend (cfg-gated)
// ---------------------------------------------------------------------------

/// NVIDIA CUDA GPU execution provider.
#[cfg(feature = "cuda")]
pub struct CudaBackend;

#[cfg(feature = "cuda")]
impl ExecutionProviderBackend for CudaBackend {
    fn name(&self) -> &str {
        "CUDA"
    }

    fn is_available(&self) -> bool {
        // Probe for CUDA runtime library at runtime.
        std::env::var("CUDA_PATH").is_ok()
            || cfg!(target_os = "linux")
                && std::path::Path::new("/usr/local/cuda/lib64/libcudart.so").exists()
    }

    fn priority(&self) -> u32 {
        200
    }

    fn config_keys(&self) -> &[&str] {
        &["device_id", "arena_extend_strategy"]
    }

    fn supported_quant_formats(&self) -> Vec<QuantFormat> {
        vec![
            QuantFormat::Fp32,
            QuantFormat::Fp16,
            QuantFormat::Int8,
            QuantFormat::Unknown,
        ]
    }

    fn diagnose(&self) -> Vec<DiagnosticEntry> {
        let mut entries = vec![];
        if std::env::var("CUDA_PATH").is_ok() {
            entries.push(DiagnosticEntry::pass("cuda-sdk", "CUDA_PATH is set"));
        } else if cfg!(target_os = "linux")
            && std::path::Path::new("/usr/local/cuda/lib64/libcudart.so").exists()
        {
            entries.push(DiagnosticEntry::pass(
                "cuda-sdk",
                "CUDA runtime found at /usr/local/cuda",
            ));
        } else {
            entries.push(DiagnosticEntry::warn(
                "cuda-sdk",
                "CUDA toolkit not detected; NVIDIA GPU acceleration unavailable",
            ));
        }
        entries
    }

    fn build_session(
        &self,
        config: &ModelConfig,
        _options: &BackendOptions,
    ) -> anyhow::Result<Box<dyn InferenceSession>> {
        if config.dry_run {
            return Ok(Box::new(DryRunSession));
        }

        #[cfg(feature = "onnx")]
        {
            Ok(Box::new(OnnxCpuSession {
                config: config.clone(),
            }))
        }

        #[cfg(not(feature = "onnx"))]
        {
            anyhow::bail!("CUDA backend requires the 'onnx' feature")
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in: TensorRT backend (cfg-gated)
// ---------------------------------------------------------------------------

/// NVIDIA TensorRT execution provider (higher performance than raw CUDA).
#[cfg(feature = "tensorrt")]
pub struct TensorRtBackend;

#[cfg(feature = "tensorrt")]
impl ExecutionProviderBackend for TensorRtBackend {
    fn name(&self) -> &str {
        "TensorRT"
    }

    fn is_available(&self) -> bool {
        // TensorRT requires both CUDA and the TensorRT SDK.
        (std::env::var("CUDA_PATH").is_ok()
            || cfg!(target_os = "linux")
                && std::path::Path::new("/usr/local/cuda/lib64/libcudart.so").exists())
            && std::env::var("LD_LIBRARY_PATH")
                .unwrap_or_default()
                .contains("tensorrt")
    }

    fn priority(&self) -> u32 {
        250
    }

    fn config_keys(&self) -> &[&str] {
        &["device_id", "cache_dir", "arena_extend_strategy"]
    }

    fn supported_quant_formats(&self) -> Vec<QuantFormat> {
        vec![
            QuantFormat::Fp32,
            QuantFormat::Fp16,
            QuantFormat::Int8,
            QuantFormat::Int4,
            QuantFormat::Unknown,
        ]
    }

    fn diagnose(&self) -> Vec<DiagnosticEntry> {
        let mut entries = vec![];
        if std::env::var("CUDA_PATH").is_ok() {
            entries.push(DiagnosticEntry::pass("tensorrt-cuda", "CUDA_PATH is set"));
        } else {
            entries.push(DiagnosticEntry::warn(
                "tensorrt-cuda",
                "CUDA_PATH not set; TensorRT requires CUDA",
            ));
        }

        let has_trt = std::env::var("LD_LIBRARY_PATH")
            .unwrap_or_default()
            .contains("tensorrt");
        if has_trt {
            entries.push(DiagnosticEntry::pass(
                "tensorrt-sdk",
                "TensorRT libraries found in LD_LIBRARY_PATH",
            ));
        } else {
            entries.push(DiagnosticEntry::warn(
                "tensorrt-sdk",
                "TensorRT SDK not detected in LD_LIBRARY_PATH",
            ));
        }
        entries
    }

    fn build_session(
        &self,
        config: &ModelConfig,
        _options: &BackendOptions,
    ) -> anyhow::Result<Box<dyn InferenceSession>> {
        if config.dry_run {
            return Ok(Box::new(DryRunSession));
        }

        #[cfg(feature = "onnx")]
        {
            Ok(Box::new(OnnxCpuSession {
                config: config.clone(),
            }))
        }

        #[cfg(not(feature = "onnx"))]
        {
            anyhow::bail!("TensorRT backend requires the 'onnx' feature")
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in: QNN backend (cfg-gated)
// ---------------------------------------------------------------------------

/// Qualcomm QNN / Hexagon NPU execution provider.
#[cfg(feature = "qnn")]
pub struct QnnBackend;

#[cfg(feature = "qnn")]
impl ExecutionProviderBackend for QnnBackend {
    fn name(&self) -> &str {
        "QNN"
    }

    fn is_available(&self) -> bool {
        // Check for QNN SDK presence.
        std::env::var("QNN_SDK_ROOT").is_ok()
    }

    fn priority(&self) -> u32 {
        280
    }

    fn config_keys(&self) -> &[&str] {
        &["backend_path", "device_id"]
    }

    fn supported_quant_formats(&self) -> Vec<QuantFormat> {
        vec![QuantFormat::Int8, QuantFormat::Int4, QuantFormat::Unknown]
    }

    fn diagnose(&self) -> Vec<DiagnosticEntry> {
        let mut entries = vec![];
        if std::env::var("QNN_SDK_ROOT").is_ok() {
            entries.push(DiagnosticEntry::pass("qnn-sdk", "QNN_SDK_ROOT is set"));
        } else {
            entries.push(DiagnosticEntry::warn(
                "qnn-sdk",
                "QNN_SDK_ROOT not set; Qualcomm NPU acceleration unavailable",
            ));
        }

        if cfg!(target_arch = "aarch64") && cfg!(target_os = "windows") {
            entries.push(DiagnosticEntry::pass(
                "qnn-platform",
                "Windows ARM64 — optimal QNN target",
            ));
        } else {
            entries.push(DiagnosticEntry::warn(
                "qnn-platform",
                "QNN is optimized for Windows ARM64 (Snapdragon X Elite)",
            ));
        }
        entries
    }

    fn build_session(
        &self,
        config: &ModelConfig,
        _options: &BackendOptions,
    ) -> anyhow::Result<Box<dyn InferenceSession>> {
        if config.dry_run {
            return Ok(Box::new(DryRunSession));
        }

        #[cfg(feature = "onnx")]
        {
            Ok(Box::new(OnnxCpuSession {
                config: config.clone(),
            }))
        }

        #[cfg(not(feature = "onnx"))]
        {
            anyhow::bail!("QNN backend requires the 'onnx' feature")
        }
    }
}

// ---------------------------------------------------------------------------
// Session implementations
// ---------------------------------------------------------------------------

/// Dry-run session that returns a placeholder response.
struct DryRunSession;

impl InferenceSession for DryRunSession {
    fn generate(&self, _prompt: &str, _max_new_tokens: usize) -> anyhow::Result<String> {
        Ok("<final>Dry-run session: no live inference performed.</final>".to_string())
    }
}

/// ONNX CPU session that delegates to the existing `onnx_vitis::run_prompt`.
#[cfg(feature = "onnx")]
struct OnnxCpuSession {
    config: ModelConfig,
}

#[cfg(feature = "onnx")]
impl InferenceSession for OnnxCpuSession {
    fn generate(&self, prompt: &str, _max_new_tokens: usize) -> anyhow::Result<String> {
        crate::onnx_vitis::run_prompt(&self.config, prompt)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_backend_is_always_available() {
        let cpu = CpuBackend;
        assert!(cpu.is_available());
        assert_eq!(cpu.name(), "CPU");
        assert_eq!(cpu.priority(), 0);
    }

    #[test]
    fn cpu_diagnostics_pass() {
        let cpu = CpuBackend;
        let diags = cpu.diagnose();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, DiagnosticSeverity::Pass);
    }

    #[test]
    fn registry_discover_includes_cpu() {
        let registry = ProviderRegistry::discover();
        let list = registry.list();
        assert!(!list.is_empty());
        assert!(list.iter().any(|p| p.name == "CPU" && p.available));
    }

    #[test]
    fn registry_best_available_returns_something() {
        let registry = ProviderRegistry::discover();
        let best = registry.best_available();
        assert!(best.is_some());
    }

    #[test]
    fn registry_get_by_name_case_insensitive() {
        let registry = ProviderRegistry::discover();
        assert!(registry.get("cpu").is_some());
        assert!(registry.get("CPU").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn registry_available_names_includes_cpu() {
        let registry = ProviderRegistry::discover();
        let names = registry.available_names();
        assert!(names.contains(&"CPU".to_string()));
    }

    #[test]
    fn dry_run_session_build() {
        let config = ModelConfig {
            model_path: std::path::PathBuf::from("test.onnx"),
            tokenizer_path: None,
            max_new_tokens: 1,
            temperature: 0.0,
            dry_run: true,
            backend_override: None,
            backend_config: Default::default(),
            token_stream_tx: None,
        };
        let cpu = CpuBackend;
        let session = cpu.build_session(&config, &BackendOptions::new());
        assert!(session.is_ok());
    }

    #[test]
    fn dry_run_session_generates() {
        let session = DryRunSession;
        let result = session.generate("test prompt", 10);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Dry-run"));
    }

    #[test]
    fn diagnostic_entry_constructors() {
        let pass = DiagnosticEntry::pass("check-a", "all good");
        assert_eq!(pass.severity, DiagnosticSeverity::Pass);

        let warn = DiagnosticEntry::warn("check-b", "maybe bad");
        assert_eq!(warn.severity, DiagnosticSeverity::Warn);

        let fail = DiagnosticEntry::fail("check-c", "broken");
        assert_eq!(fail.severity, DiagnosticSeverity::Fail);
    }

    #[test]
    fn provider_info_serializes() {
        let info = ProviderInfo {
            name: "CPU".to_string(),
            priority: 0,
            available: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"CPU\""));
    }

    #[test]
    fn build_session_with_fallback_works() {
        let registry = ProviderRegistry::discover();
        let config = ModelConfig {
            model_path: std::path::PathBuf::from("test.onnx"),
            tokenizer_path: None,
            max_new_tokens: 1,
            temperature: 0.0,
            dry_run: true,
            backend_override: None,
            backend_config: Default::default(),
            token_stream_tx: None,
        };
        let result = registry.build_session_with_fallback(&config, &BackendOptions::new(), None);
        assert!(result.is_ok());
        let (backend_name, _session) = result.unwrap();
        assert!(!backend_name.is_empty());
    }

    #[test]
    fn build_session_with_fallback_preferred() {
        let registry = ProviderRegistry::discover();
        let config = ModelConfig {
            model_path: std::path::PathBuf::from("test.onnx"),
            tokenizer_path: None,
            max_new_tokens: 1,
            temperature: 0.0,
            dry_run: true,
            backend_override: None,
            backend_config: Default::default(),
            token_stream_tx: None,
        };
        let result =
            registry.build_session_with_fallback(&config, &BackendOptions::new(), Some("CPU"));
        assert!(result.is_ok());
        let (name, _) = result.unwrap();
        assert_eq!(name, "CPU");
    }

    #[test]
    fn model_format_from_path_onnx() {
        assert_eq!(
            ModelFormat::from_path(Path::new("model.onnx")),
            Some(ModelFormat::Onnx)
        );
    }

    #[test]
    fn model_format_from_path_gguf() {
        assert_eq!(
            ModelFormat::from_path(Path::new("model.gguf")),
            Some(ModelFormat::Gguf)
        );
    }

    #[test]
    fn model_format_from_path_safetensors() {
        assert_eq!(
            ModelFormat::from_path(Path::new("model.safetensors")),
            Some(ModelFormat::SafeTensors)
        );
    }

    #[test]
    fn model_format_from_path_unknown() {
        assert_eq!(ModelFormat::from_path(Path::new("model.bin")), None);
    }

    #[test]
    fn quant_format_detect_int4() {
        assert_eq!(
            QuantFormat::detect_from_path(Path::new("model-int4.onnx")),
            QuantFormat::Int4
        );
        assert_eq!(
            QuantFormat::detect_from_path(Path::new("model-q4.gguf")),
            QuantFormat::Int4
        );
    }

    #[test]
    fn quant_format_detect_int8() {
        assert_eq!(
            QuantFormat::detect_from_path(Path::new("model-int8.onnx")),
            QuantFormat::Int8
        );
    }

    #[test]
    fn quant_format_detect_quantized_suffix() {
        // model_quantized.onnx is the default output of ONNX Runtime's quantization
        // tooling, which defaults to INT8 — must not fall through to Unknown.
        assert_eq!(
            QuantFormat::detect_from_path(Path::new("model_quantized.onnx")),
            QuantFormat::Int8
        );
        assert_eq!(
            QuantFormat::detect_from_path(Path::new("model-quantized.onnx")),
            QuantFormat::Int8
        );
    }

    #[test]
    fn quant_format_detect_fp16() {
        assert_eq!(
            QuantFormat::detect_from_path(Path::new("model-fp16.onnx")),
            QuantFormat::Fp16
        );
    }

    #[test]
    fn quant_format_detect_block_quantized() {
        assert_eq!(
            QuantFormat::detect_from_path(Path::new("model-awq.gguf")),
            QuantFormat::BlockQuantized("awq".to_string())
        );
        assert_eq!(
            QuantFormat::detect_from_path(Path::new("model-gptq.safetensors")),
            QuantFormat::BlockQuantized("gptq".to_string())
        );
    }

    #[test]
    fn quant_format_detect_unknown() {
        assert_eq!(
            QuantFormat::detect_from_path(Path::new("model.onnx")),
            QuantFormat::Unknown
        );
    }

    #[test]
    fn cpu_supported_formats_includes_onnx() {
        let cpu = CpuBackend;
        assert!(cpu.supported_formats().contains(&ModelFormat::Onnx));
    }

    #[test]
    fn cpu_supported_quant_formats() {
        let cpu = CpuBackend;
        let quants = cpu.supported_quant_formats();
        assert!(quants.contains(&QuantFormat::Fp32));
        assert!(quants.contains(&QuantFormat::Fp16));
        assert!(quants.contains(&QuantFormat::Int8));
    }

    #[test]
    fn model_format_display() {
        assert_eq!(format!("{}", ModelFormat::Onnx), "ONNX");
        assert_eq!(format!("{}", ModelFormat::Gguf), "GGUF");
        assert_eq!(format!("{}", ModelFormat::SafeTensors), "SafeTensors");
    }

    #[test]
    fn quant_format_display() {
        assert_eq!(format!("{}", QuantFormat::Fp32), "FP32");
        assert_eq!(format!("{}", QuantFormat::Fp16), "FP16");
        assert_eq!(format!("{}", QuantFormat::Int8), "INT8");
        assert_eq!(format!("{}", QuantFormat::Int4), "INT4");
        assert_eq!(
            format!("{}", QuantFormat::BlockQuantized("awq".into())),
            "BlockQuantized(awq)"
        );
    }
}
