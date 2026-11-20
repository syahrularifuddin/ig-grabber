mod config;
mod error;
mod ig;
mod models;
mod routes;

use axum::{routing::get, Router};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ig_backend=info,tower_http=info".into()),
        )
        .init();

    let state = config::AppState::new();
    let port = state.port;

    let app: Router<()> = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(routes::router())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("failed to bind port");
    tracing::info!("ig-backend listening on http://0.0.0.0:{port}");
    axum::serve(listener, app).await.expect("server error");
}
