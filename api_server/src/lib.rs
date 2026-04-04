pub mod audit;
pub mod data_store;
mod routes;
mod state;

pub use audit::{AuditLog, AuditLogConfig};
pub use data_store::DataStore;
pub use routes::build_router;
pub use state::{AppState, CaseEntry, CaseStatus, RunEntry, RunStatus, ServerConfig};

/// Start the API server on the given address. Blocks until shutdown.
pub async fn run_server(config: ServerConfig) -> anyhow::Result<()> {
    let addr = std::net::SocketAddr::from((config.bind_addr, config.port));
    eprintln!("WraithRun API server listening on http://{addr}");
    eprintln!("API token: {}", config.api_token);
    let state = AppState::new(config);

    // Emit server-started audit event.
    use audit::{audit_event, details, AuditEventKind};
    state
        .audit
        .emit(audit_event(
            AuditEventKind::ServerStarted,
            "system",
            "server",
            details(&[("bind", &format!("http://{addr}"))]),
        ))
        .await;

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
