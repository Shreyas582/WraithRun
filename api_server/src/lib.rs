pub mod data_store;
mod routes;
mod state;

pub use data_store::DataStore;
pub use routes::build_router;
pub use state::{AppState, RunEntry, RunStatus, ServerConfig};

/// Start the API server on the given address. Blocks until shutdown.
pub async fn run_server(config: ServerConfig) -> anyhow::Result<()> {
    let addr = std::net::SocketAddr::from((config.bind_addr, config.port));
    eprintln!("WraithRun API server listening on http://{addr}");
    eprintln!("API token: {}", config.api_token);
    let state = AppState::new(config);
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
