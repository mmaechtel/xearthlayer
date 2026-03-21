//! Axum HTTP server for the debug map.
//!
//! Serves the Leaflet.js map page at `GET /` and the JSON state API
//! at `GET /api/state`. Runs on the tokio runtime alongside the main
//! service.

use std::net::SocketAddr;

use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::{Json, Router};
use tokio_util::sync::CancellationToken;

use super::api::{collect_snapshot, DebugStateSnapshot};
use super::html::MAP_HTML;
use super::state::DebugMapState;

/// Default port for the debug map server.
pub const DEFAULT_DEBUG_MAP_PORT: u16 = 8087;

/// Debug map HTTP server.
///
/// Serves a live map at `http://localhost:{port}` and a JSON API
/// at `http://localhost:{port}/api/state`.
pub struct DebugMapServer {
    state: DebugMapState,
    port: u16,
}

impl DebugMapServer {
    /// Create a new server with the given shared state and port.
    pub fn new(state: DebugMapState, port: u16) -> Self {
        Self { state, port }
    }

    /// Start the server. Runs until the cancellation token is triggered.
    pub async fn run(self, cancellation: CancellationToken) {
        let app = Router::new()
            .route("/", get(serve_map))
            .route("/api/state", get(serve_state))
            .with_state(self.state);

        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));

        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(
                    port = self.port,
                    error = %e,
                    "Failed to start debug map server"
                );
                return;
            }
        };

        tracing::info!(
            port = self.port,
            "Debug map server started at http://localhost:{}",
            self.port
        );

        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(cancellation.cancelled_owned())
            .await
        {
            tracing::warn!(error = %e, "Debug map server error");
        }
    }
}

/// Serve the Leaflet.js map page.
async fn serve_map() -> Html<&'static str> {
    Html(MAP_HTML)
}

/// Serve the current state as JSON.
async fn serve_state(State(state): State<DebugMapState>) -> Json<DebugStateSnapshot> {
    Json(collect_snapshot(&state))
}
