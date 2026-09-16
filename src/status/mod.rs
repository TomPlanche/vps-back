//! Status endpoint, served on its own subdomain (`status.tomplanche.com`).

pub mod handlers;
pub mod models;

use std::time::Duration;

use axum::{Router, routing::get};
use handlers::get_status;
use reqwest::Client;
use sea_orm::DatabaseConnection;

/// How long an upstream check waits before giving up. Generous on purpose: a
/// first request after an idle period can take several seconds to answer.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// Everything the status handler needs beyond the database, passed as an
/// axum `Extension`.
#[derive(Debug, Clone)]
pub struct StatusConfig {
    /// URL checked for the `miaou` upstream.
    pub miaou_url: String,
    /// Shared HTTP client, reused across requests.
    pub client: Client,
}

impl StatusConfig {
    /// Build the status configuration and its HTTP client.
    #[must_use]
    pub fn new(miaou_url: String) -> Self {
        let client = Client::builder()
            .timeout(CHECK_TIMEOUT)
            .user_agent("vps-back-status")
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!("Status checks: falling back to the default HTTP client: {e}");
                Client::new()
            });

        Self { miaou_url, client }
    }
}

/// Creates the status router.
pub fn router() -> Router<DatabaseConnection> {
    Router::new().route("/", get(get_status))
}
