//! Multi-backend conformance test harness.
//!
//! This module validates that every `ExecutionProviderBackend` implementation
//! satisfies the trait contract. Adding a new backend requires only one
//! invocation of the `backend_contract_tests!` macro.

use inference_bridge::backend::{
    BackendOptions, CpuBackend, DiagnosticSeverity, ExecutionProviderBackend, ProviderRegistry,
};
use inference_bridge::ModelConfig;
use std::path::PathBuf;

fn test_model_config(dry_run: bool) -> ModelConfig {
    ModelConfig {
        model_path: PathBuf::from("tests/fixtures/dummy.onnx"),
        tokenizer_path: None,
        max_new_tokens: 1,
        temperature: 0.0,
        dry_run,
        backend_override: None,
        backend_config: Default::default(),
    }
}

/// Contract test suite exercised against each compiled backend.
macro_rules! backend_contract_tests {
    ($mod_name:ident, $backend_expr:expr) => {
        mod $mod_name {
            use super::*;

            fn backend() -> impl ExecutionProviderBackend {
                $backend_expr
            }

            #[test]
            fn name_is_non_empty() {
                assert!(
                    !backend().name().is_empty(),
                    "backend name must not be empty"
                );
            }

            #[test]
            fn name_is_ascii() {
                assert!(
                    backend().name().is_ascii(),
                    "backend name should be ASCII for consistent matching"
                );
            }

            #[test]
            fn priority_is_finite() {
                // priority is u32 so always finite, but verify it's accessible
                let _ = backend().priority();
            }

            #[test]
            fn is_available_is_deterministic() {
                let a = backend().is_available();
                let b = backend().is_available();
                assert_eq!(a, b, "is_available() should be deterministic across calls");
            }

            #[test]
            fn config_keys_are_non_empty_strings() {
                for key in backend().config_keys() {
                    assert!(!key.is_empty(), "config key must not be empty");
                }
            }

            #[test]
            fn diagnose_returns_entries() {
                let entries = backend().diagnose();
                // Every backend should report at least one diagnostic.
                assert!(
                    !entries.is_empty(),
                    "diagnose() must return at least one entry"
                );
            }

            #[test]
            fn diagnose_entries_are_well_formed() {
                for entry in backend().diagnose() {
                    assert!(
                        !entry.check.is_empty(),
                        "diagnostic check name must not be empty"
                    );
                    assert!(
                        !entry.message.is_empty(),
                        "diagnostic message must not be empty"
                    );
                    // Severity must be one of the defined variants.
                    match entry.severity {
                        DiagnosticSeverity::Pass
                        | DiagnosticSeverity::Warn
                        | DiagnosticSeverity::Fail => {}
                    }
                }
            }

            #[test]
            fn dry_run_session_succeeds() {
                let config = test_model_config(true);
                let session = backend().build_session(&config, &BackendOptions::new());
                assert!(
                    session.is_ok(),
                    "build_session with dry_run=true should always succeed: {:?}",
                    session.err()
                );
            }

            #[test]
            fn dry_run_session_generates() {
                let config = test_model_config(true);
                let session = backend()
                    .build_session(&config, &BackendOptions::new())
                    .expect("dry-run session build");
                let result = session.generate("test prompt", 10);
                assert!(result.is_ok(), "dry-run generate should succeed");
                assert!(
                    !result.unwrap().is_empty(),
                    "dry-run generate should return non-empty text"
                );
            }
        }
    };
}

// -----------------------------------------------------------------------
// Invoke the conformance suite for each compiled backend.
// -----------------------------------------------------------------------

backend_contract_tests!(cpu_conformance, CpuBackend);

#[cfg(feature = "vitis")]
backend_contract_tests!(vitis_conformance, inference_bridge::backend::VitisBackend);

// -----------------------------------------------------------------------
// Registry-level tests
// -----------------------------------------------------------------------

#[test]
fn registry_discover_is_non_empty() {
    let registry = ProviderRegistry::discover();
    assert!(!registry.list().is_empty());
}

#[test]
fn registry_diagnose_all_covers_all_backends() {
    let registry = ProviderRegistry::discover();
    let list = registry.list();
    let diags = registry.diagnose_all();
    assert_eq!(
        list.len(),
        diags.len(),
        "diagnose_all should return one entry per registered backend"
    );
}

#[test]
fn registry_diagnose_all_sorted_by_priority_desc() {
    let registry = ProviderRegistry::discover();
    let diags = registry.diagnose_all();
    for window in diags.windows(2) {
        assert!(
            window[0].info.priority >= window[1].info.priority,
            "diagnose_all results should be sorted by descending priority"
        );
    }
}

#[test]
fn registry_best_available_matches_highest_priority() {
    let registry = ProviderRegistry::discover();
    let best = registry.best_available().expect("at least one backend");
    let all_available: Vec<_> = registry
        .list()
        .into_iter()
        .filter(|p| p.available)
        .collect();
    let max_priority = all_available.iter().map(|p| p.priority).max().unwrap();
    assert_eq!(best.priority(), max_priority);
}

#[test]
fn registry_build_session_with_fallback_dry_run() {
    let registry = ProviderRegistry::discover();
    let config = test_model_config(true);
    let result = registry.build_session_with_fallback(&config, &BackendOptions::new(), None);
    assert!(result.is_ok());
    let (name, session) = result.unwrap();
    assert!(!name.is_empty());
    let output = session.generate("hello", 1).unwrap();
    assert!(!output.is_empty());
}
