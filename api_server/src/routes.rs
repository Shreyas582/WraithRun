use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use core_engine::agent::Agent;
use cyber_tools::ToolRegistry;
use inference_bridge::{ModelConfig, OnnxVitisEngine};

use crate::audit::{audit_event, details, AuditEventKind};
use crate::state::{chrono_now, AppState, CaseEntry, CaseStatus, RunEntry, RunStatus};

/// Build the full application router with all v1 endpoints.
pub fn build_router(state: AppState) -> Router {
    let body_limit = state.config.max_request_body_bytes;

    // Authenticated endpoints requiring Bearer token.
    let authed = Router::new()
        .route("/ready", get(ready))
        .route("/runs", post(create_run))
        .route("/runs", get(list_runs))
        .route("/runs/{id}", get(get_run))
        .route("/runs/{id}/cancel", post(cancel_run))
        .route("/cases", post(create_case))
        .route("/cases", get(list_cases))
        .route("/cases/{id}", get(get_case))
        .route("/cases/{id}", axum::routing::patch(update_case))
        .route("/cases/{id}/runs", get(list_case_runs))
        .route("/runtime/status", get(runtime_status))
        .route("/audit/events", get(list_audit_events))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            bearer_auth_middleware,
        ));

    // Health is unauthenticated.
    let api_v1 = Router::new().route("/health", get(health)).merge(authed);

    Router::new()
        .route("/", get(dashboard))
        .nest("/api/v1", api_v1)
        .layer(RequestBodyLimitLayer::new(body_limit))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

static DASHBOARD_HTML: &str = include_str!("dashboard.html");

async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

// ---------------------------------------------------------------------------
// Bearer token authentication middleware
// ---------------------------------------------------------------------------

async fn bearer_auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let expected = &state.config.api_token;

    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            if token == expected {
                state
                    .audit
                    .emit(audit_event(
                        AuditEventKind::AuthSuccess,
                        "api-token:default",
                        "auth",
                        details(&[]),
                    ))
                    .await;
                Ok(next.run(req).await)
            } else {
                state
                    .audit
                    .emit(audit_event(
                        AuditEventKind::AuthFailure,
                        "unknown",
                        "auth",
                        details(&[("reason", "invalid bearer token")]),
                    ))
                    .await;
                tracing::warn!("API auth failure: invalid bearer token");
                Err((
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorResponse {
                        error: "invalid bearer token".to_string(),
                    }),
                ))
            }
        }
        _ => {
            state
                .audit
                .emit(audit_event(
                    AuditEventKind::AuthFailure,
                    "unknown",
                    "auth",
                    details(&[("reason", "missing or malformed Authorization header")]),
                ))
                .await;
            tracing::warn!("API auth failure: missing or malformed Authorization header");
            Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "missing or malformed Authorization header".to_string(),
                }),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Health & readiness
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    uptime_secs: u64,
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let started: u64 = state.started_at.parse().unwrap_or(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        uptime_secs: now.saturating_sub(started),
    })
}

#[derive(Serialize)]
struct ReadyResponse {
    ready: bool,
    tools_available: usize,
}

async fn ready() -> Json<ReadyResponse> {
    let registry = ToolRegistry::with_default_tools();
    Json(ReadyResponse {
        ready: true,
        tools_available: registry.tool_names().len(),
    })
}

// ---------------------------------------------------------------------------
// Run management
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateRunRequest {
    task: String,
    #[serde(default = "default_max_steps")]
    max_steps: usize,
    #[serde(default)]
    case_id: Option<Uuid>,
}

fn default_max_steps() -> usize {
    8
}

#[derive(Serialize)]
struct CreateRunResponse {
    id: Uuid,
    status: RunStatus,
}

async fn create_run(
    State(state): State<AppState>,
    Json(body): Json<CreateRunRequest>,
) -> Result<(StatusCode, Json<CreateRunResponse>), (StatusCode, Json<ErrorResponse>)> {
    let task = body.task.trim().to_string();
    if task.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "task must not be empty".to_string(),
            }),
        ));
    }

    // Enforce concurrency limit.
    {
        let count = state.active_run_count.lock().await;
        if *count >= state.config.max_concurrent_runs {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(ErrorResponse {
                    error: format!(
                        "max concurrent runs ({}) reached",
                        state.config.max_concurrent_runs
                    ),
                }),
            ));
        }
    }

    let run_id = Uuid::new_v4();
    let entry_task = task.clone();
    let entry = RunEntry {
        id: run_id,
        task: task.clone(),
        status: RunStatus::Queued,
        report: None,
        error: None,
        created_at: chrono_now(),
        completed_at: None,
        case_id: body.case_id,
    };

    // Persist to database if available.
    if let Some(db) = &state.db {
        let _ = db.insert_run(&entry).await;
    }

    {
        let mut runs = state.runs.write().await;
        runs.insert(run_id, entry);
    }

    // Spawn background task to execute the investigation.
    let state_clone = state.clone();
    let max_steps = body.max_steps;
    tokio::spawn(async move {
        execute_run(state_clone, run_id, task, max_steps).await;
    });

    state
        .audit
        .emit(audit_event(
            AuditEventKind::RunCreated,
            "api-token:default",
            &format!("run/{run_id}"),
            details(&[("task", &entry_task)]),
        ))
        .await;

    Ok((
        StatusCode::ACCEPTED,
        Json(CreateRunResponse {
            id: run_id,
            status: RunStatus::Queued,
        }),
    ))
}

async fn execute_run(state: AppState, run_id: Uuid, task: String, max_steps: usize) {
    // Mark as running.
    {
        let mut runs = state.runs.write().await;
        if let Some(entry) = runs.get_mut(&run_id) {
            entry.status = RunStatus::Running;
        }
    }
    {
        let mut count = state.active_run_count.lock().await;
        *count += 1;
    }

    let result = run_investigation(&task, max_steps).await;

    // Decrement active count.
    {
        let mut count = state.active_run_count.lock().await;
        *count = count.saturating_sub(1);
    }

    // Store result.
    let mut runs = state.runs.write().await;
    if let Some(entry) = runs.get_mut(&run_id) {
        // Don't overwrite if cancelled.
        if entry.status == RunStatus::Cancelled {
            return;
        }
        match result {
            Ok(report) => {
                entry.status = RunStatus::Completed;
                entry.report = Some(report);
                drop(runs);
                state
                    .audit
                    .emit(audit_event(
                        AuditEventKind::RunCompleted,
                        "system",
                        &format!("run/{run_id}"),
                        details(&[("task", &task)]),
                    ))
                    .await;
                let mut runs = state.runs.write().await;
                let entry = runs.get_mut(&run_id).unwrap();
                entry.completed_at = Some(chrono_now());
                if let Some(db) = &state.db {
                    let _ = db.update_run(entry).await;
                }
            }
            Err(e) => {
                let err_msg = e.to_string();
                entry.status = RunStatus::Failed;
                entry.error = Some(err_msg.clone());
                entry.completed_at = Some(chrono_now());
                if let Some(db) = &state.db {
                    let _ = db.update_run(entry).await;
                }
                drop(runs);
                state
                    .audit
                    .emit(audit_event(
                        AuditEventKind::RunFailed,
                        "system",
                        &format!("run/{run_id}"),
                        details(&[("task", &task), ("error", &err_msg)]),
                    ))
                    .await;
            }
        }
    }
}

async fn run_investigation(task: &str, max_steps: usize) -> anyhow::Result<core_engine::RunReport> {
    let model_config = ModelConfig {
        model_path: std::path::PathBuf::from("./models/llm.onnx"),
        tokenizer_path: None,
        max_new_tokens: 256,
        temperature: 0.2,
        dry_run: true,
        backend_override: None,
        backend_config: Default::default(),
    };
    let engine = OnnxVitisEngine::new(model_config);
    let tools = ToolRegistry::with_default_tools();
    let agent = Agent::new(engine, tools).with_max_steps(max_steps);
    agent.run(task).await
}

#[derive(Serialize)]
struct RunListEntry {
    id: Uuid,
    task: String,
    status: RunStatus,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    completed_at: Option<String>,
}

async fn list_runs(State(state): State<AppState>) -> Json<Vec<RunListEntry>> {
    let runs = state.runs.read().await;
    let mut entries: Vec<RunListEntry> = runs
        .values()
        .map(|entry| RunListEntry {
            id: entry.id,
            task: entry.task.clone(),
            status: entry.status.clone(),
            created_at: entry.created_at.clone(),
            completed_at: entry.completed_at.clone(),
        })
        .collect();
    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Json(entries)
}

async fn get_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RunEntry>, (StatusCode, Json<ErrorResponse>)> {
    let runs = state.runs.read().await;
    match runs.get(&id) {
        Some(entry) => Ok(Json(entry.clone())),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("run {id} not found"),
            }),
        )),
    }
}

async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RunEntry>, (StatusCode, Json<ErrorResponse>)> {
    let mut runs = state.runs.write().await;
    match runs.get_mut(&id) {
        Some(entry) => {
            let was_active =
                entry.status == RunStatus::Queued || entry.status == RunStatus::Running;
            if was_active {
                entry.status = RunStatus::Cancelled;
                entry.completed_at = Some(chrono_now());
            }
            let result = entry.clone();
            drop(runs);
            if was_active {
                state
                    .audit
                    .emit(audit_event(
                        AuditEventKind::RunCancelled,
                        "api-token:default",
                        &format!("run/{id}"),
                        details(&[]),
                    ))
                    .await;
            }
            Ok(Json(result))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("run {id} not found"),
            }),
        )),
    }
}

// ---------------------------------------------------------------------------
// Case management
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateCaseRequest {
    title: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Serialize)]
struct CreateCaseResponse {
    id: Uuid,
    status: CaseStatus,
}

async fn create_case(
    State(state): State<AppState>,
    Json(body): Json<CreateCaseRequest>,
) -> Result<(StatusCode, Json<CreateCaseResponse>), (StatusCode, Json<ErrorResponse>)> {
    let title = body.title.trim().to_string();
    if title.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "title must not be empty".to_string(),
            }),
        ));
    }

    let db = state.db.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "case management requires database persistence".to_string(),
            }),
        )
    })?;

    let now = chrono_now();
    let case = CaseEntry {
        id: Uuid::new_v4(),
        title: title.clone(),
        description: body.description,
        status: CaseStatus::Open,
        created_at: now.clone(),
        updated_at: now,
        run_count: 0,
        max_severity: None,
    };

    db.insert_case(&case).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to create case: {e}"),
            }),
        )
    })?;

    state
        .audit
        .emit(audit_event(
            AuditEventKind::CaseCreated,
            "api-token:default",
            &format!("case/{}", case.id),
            details(&[("title", &title)]),
        ))
        .await;

    Ok((
        StatusCode::CREATED,
        Json(CreateCaseResponse {
            id: case.id,
            status: CaseStatus::Open,
        }),
    ))
}

async fn list_cases(
    State(state): State<AppState>,
) -> Result<Json<Vec<CaseEntry>>, (StatusCode, Json<ErrorResponse>)> {
    let db = state.db.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "case management requires database persistence".to_string(),
            }),
        )
    })?;
    let cases = db.list_cases().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to list cases: {e}"),
            }),
        )
    })?;
    Ok(Json(cases))
}

async fn get_case(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CaseEntry>, (StatusCode, Json<ErrorResponse>)> {
    let db = state.db.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "case management requires database persistence".to_string(),
            }),
        )
    })?;
    match db.get_case(id).await {
        Ok(Some(case)) => Ok(Json(case)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("case {id} not found"),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to get case: {e}"),
            }),
        )),
    }
}

#[derive(Deserialize)]
struct UpdateCaseRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    status: Option<CaseStatus>,
}

async fn update_case(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCaseRequest>,
) -> Result<Json<CaseEntry>, (StatusCode, Json<ErrorResponse>)> {
    let db = state.db.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "case management requires database persistence".to_string(),
            }),
        )
    })?;

    let mut case = match db.get_case(id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("case {id} not found"),
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to get case: {e}"),
                }),
            ));
        }
    };

    if let Some(title) = &body.title {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "title must not be empty".to_string(),
                }),
            ));
        }
        case.title = title;
    }
    if let Some(desc) = body.description {
        case.description = Some(desc);
    }
    if let Some(status) = body.status {
        case.status = status;
    }
    case.updated_at = chrono_now();

    db.update_case(&case).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to update case: {e}"),
            }),
        )
    })?;

    state
        .audit
        .emit(audit_event(
            AuditEventKind::CaseUpdated,
            "api-token:default",
            &format!("case/{id}"),
            details(&[]),
        ))
        .await;

    Ok(Json(case))
}

async fn list_case_runs(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<Vec<RunEntry>>, (StatusCode, Json<ErrorResponse>)> {
    let db = state.db.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "case management requires database persistence".to_string(),
            }),
        )
    })?;
    let runs = db.list_runs_for_case(case_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to list case runs: {e}"),
            }),
        )
    })?;
    Ok(Json(runs))
}

// ---------------------------------------------------------------------------
// Runtime status
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct RuntimeStatusResponse {
    mode: &'static str,
    tools_available: Vec<String>,
    plugin_tools: Vec<String>,
    max_concurrent_runs: usize,
}

async fn runtime_status(State(state): State<AppState>) -> Json<RuntimeStatusResponse> {
    let registry = ToolRegistry::with_default_tools();
    Json(RuntimeStatusResponse {
        mode: "dry-run",
        tools_available: registry.tool_names(),
        plugin_tools: state.config.plugin_tool_names.clone(),
        max_concurrent_runs: state.config.max_concurrent_runs,
    })
}

// ---------------------------------------------------------------------------
// Audit events
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AuditEventsQuery {
    #[serde(default = "default_audit_limit")]
    limit: usize,
}

fn default_audit_limit() -> usize {
    100
}

async fn list_audit_events(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<AuditEventsQuery>,
) -> Json<Vec<crate::audit::AuditEvent>> {
    let events = state.audit.recent_events(query.limit).await;
    Json(events)
}

// ---------------------------------------------------------------------------
// Error envelope
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServerConfig;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    const TEST_TOKEN: &str = "test-secret-token";

    fn test_state() -> AppState {
        let mut config = ServerConfig::default();
        config.api_token = TEST_TOKEN.to_string();
        AppState::new(config)
    }

    fn auth_header() -> (&'static str, String) {
        ("authorization", format!("Bearer {TEST_TOKEN}"))
    }

    #[tokio::test]
    async fn dashboard_returns_html() {
        let app = build_router(test_state());
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("html"));
    }

    #[tokio::test]
    async fn health_endpoint_returns_ok_without_auth() {
        let app = build_router(test_state());
        let req = Request::builder()
            .uri("/api/v1/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn authenticated_endpoint_rejects_missing_token() {
        let app = build_router(test_state());
        let req = Request::builder()
            .uri("/api/v1/ready")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn authenticated_endpoint_rejects_wrong_token() {
        let app = build_router(test_state());
        let req = Request::builder()
            .uri("/api/v1/ready")
            .header("authorization", "Bearer wrong-token")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn ready_endpoint_returns_tools() {
        let (key, val) = auth_header();
        let app = build_router(test_state());
        let req = Request::builder()
            .uri("/api/v1/ready")
            .header(key, val)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ready"], true);
        assert!(json["tools_available"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn create_run_rejects_empty_task() {
        let (key, val) = auth_header();
        let app = build_router(test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/runs")
            .header("content-type", "application/json")
            .header(key, val)
            .body(Body::from(r#"{"task":""}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn create_run_accepts_valid_task() {
        let (key, val) = auth_header();
        let app = build_router(test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/runs")
            .header("content-type", "application/json")
            .header(key, val)
            .body(Body::from(
                r#"{"task":"Investigate unauthorized SSH keys"}"#,
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "queued");
        assert!(json["id"].as_str().is_some());
    }

    #[tokio::test]
    async fn list_runs_returns_array() {
        let (key, val) = auth_header();
        let app = build_router(test_state());
        let req = Request::builder()
            .uri("/api/v1/runs")
            .header(key, val)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.is_array());
    }

    #[tokio::test]
    async fn get_run_returns_not_found_for_unknown_id() {
        let (key, val) = auth_header();
        let app = build_router(test_state());
        let fake_id = Uuid::new_v4();
        let req = Request::builder()
            .uri(&format!("/api/v1/runs/{fake_id}"))
            .header(key, val)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn cancel_run_returns_not_found_for_unknown_id() {
        let (key, val) = auth_header();
        let app = build_router(test_state());
        let fake_id = Uuid::new_v4();
        let req = Request::builder()
            .method("POST")
            .uri(&format!("/api/v1/runs/{fake_id}/cancel"))
            .header(key, val)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn runtime_status_returns_tools() {
        let (key, val) = auth_header();
        let app = build_router(test_state());
        let req = Request::builder()
            .uri("/api/v1/runtime/status")
            .header(key, val)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["mode"], "dry-run");
        assert!(json["tools_available"].as_array().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn create_and_retrieve_run() {
        let state = test_state();
        let app = build_router(state.clone());
        let (key, val) = auth_header();

        // Create a run.
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/runs")
            .header("content-type", "application/json")
            .header(key, &val)
            .body(Body::from(
                r#"{"task":"Investigate unauthorized SSH keys"}"#,
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let create_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let run_id = create_json["id"].as_str().unwrap();

        // Give the background task a moment to start.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Retrieve it.
        let app2 = build_router(state);
        let (key2, val2) = auth_header();
        let req = Request::builder()
            .uri(&format!("/api/v1/runs/{run_id}"))
            .header(key2, val2)
            .body(Body::empty())
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let run_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(run_json["id"], run_id);
        assert_eq!(run_json["task"], "Investigate unauthorized SSH keys");
    }

    #[tokio::test]
    async fn concurrency_limit_rejects_excess_runs() {
        let mut config = ServerConfig::default();
        config.max_concurrent_runs = 1;
        config.api_token = TEST_TOKEN.to_string();
        let state = AppState::new(config);

        // Manually occupy one slot.
        {
            let mut count = state.active_run_count.lock().await;
            *count = 1;
        }

        let (key, val) = auth_header();
        let app = build_router(state);
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/runs")
            .header("content-type", "application/json")
            .header(key, val)
            .body(Body::from(r#"{"task":"test"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
