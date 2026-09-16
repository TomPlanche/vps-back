//! HTTP handler for the status endpoint.

use std::{sync::OnceLock, time::Instant};

use axum::{
    Extension,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use reqwest::Client;
use sea_orm::DatabaseConnection;
use serde_json::Value;
use tracing::{error, info};

use crate::{
    data_response,
    status::{
        StatusConfig,
        models::{Check, CheckStatus, Checks, ServiceStatus, StatusReport},
    },
};

/// `github-stats.json` is rewritten every 5 minutes, so anything older than
/// this is considered stale.
const GITHUB_STATS_STALE_AFTER_SECONDS: i64 = 15 * 60;

/// Process start time, used to compute the uptime.
static STARTED: OnceLock<(Instant, DateTime<Utc>)> = OnceLock::new();

/// Record the process start time.
///
/// Called once from `main` at startup so the reported uptime covers the whole
/// process lifetime, not just the time since the first `/status` call.
pub fn init_start_time() {
    started();
}

fn started() -> &'static (Instant, DateTime<Utc>) {
    STARTED.get_or_init(|| (Instant::now(), Utc::now()))
}

/// Handles GET requests to the status path ("/status").
///
/// Answers JSON when the `Accept` header asks for it (`application/json`,
/// `text/json`), plain text otherwise. The reported `status` covers every
/// check, while the HTTP code only reflects this API: `503` when the database
/// is unreachable, `200` otherwise, so an external monitor watching the HTTP
/// code is not woken up by another service being down.
pub async fn get_status(
    State(db): State<DatabaseConnection>,
    Extension(static_dir): Extension<String>,
    Extension(status_config): Extension<StatusConfig>,
    headers: HeaderMap,
) -> Response {
    info!("GET `/status` endpoint called");

    let report = build_report(&db, &static_dir, &status_config).await;

    let code = if report.checks.database.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    if wants_json(&headers) {
        (code, data_response(&report)).into_response()
    } else {
        (
            code,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            report.to_text(),
        )
            .into_response()
    }
}

/// Run every check and assemble the report.
async fn build_report(
    db: &DatabaseConnection,
    static_dir: &str,
    status_config: &StatusConfig,
) -> StatusReport {
    let (instant, started_at) = started();

    // The checks are independent, so a slow upstream does not delay the others.
    let (database, miaou, github_stats) = tokio::join!(
        check_database(db),
        check_upstream(&status_config.client, &status_config.miaou_url),
        check_github_stats(static_dir),
    );

    let checks = Checks {
        database,
        miaou,
        github_stats,
    };

    let status = if checks.all_ok() {
        ServiceStatus::Ok
    } else {
        ServiceStatus::Degraded
    };

    StatusReport {
        status,
        version: env!("CARGO_PKG_VERSION"),
        started_at: started_at.to_rfc3339(),
        uptime_seconds: instant.elapsed().as_secs(),
        checks,
    }
}

/// Request an upstream over HTTP and report whether it answered.
async fn check_upstream(client: &Client, url: &str) -> Check {
    let start = Instant::now();

    match client.get(url).send().await {
        Ok(response) => {
            let latency_ms = elapsed_ms(start);
            let code = response.status();

            if code.is_success() || code.is_redirection() {
                Check::new(CheckStatus::Ok).with_latency(latency_ms)
            } else {
                error!("Status check: {url} answered HTTP {}", code.as_u16());
                Check::new(CheckStatus::Down)
                    .with_latency(latency_ms)
                    .with_detail(format!("HTTP {}", code.as_u16()))
            }
        }
        Err(e) => {
            error!("Status check: {url} is unreachable: {e}");

            let detail = if e.is_timeout() {
                "timed out".to_owned()
            } else if e.is_connect() {
                "connection failed".to_owned()
            } else {
                e.to_string()
            };

            Check::new(CheckStatus::Down).with_detail(detail)
        }
    }
}

/// Ping the database and measure the round trip time.
async fn check_database(db: &DatabaseConnection) -> Check {
    let start = Instant::now();

    match db.ping().await {
        Ok(()) => Check::new(CheckStatus::Ok).with_latency(elapsed_ms(start)),
        Err(e) => {
            error!("Status check: database ping failed: {e}");
            Check::new(CheckStatus::Down).with_detail(e.to_string())
        }
    }
}

/// Read `github-stats.json` and report how fresh it is.
async fn check_github_stats(static_dir: &str) -> Check {
    let path = format!("{static_dir}/github-stats.json");

    let Ok(contents) = tokio::fs::read_to_string(&path).await else {
        return Check::new(CheckStatus::Unavailable).with_detail(format!("{path} is not readable"));
    };

    let Ok(value) = serde_json::from_str::<Value>(&contents) else {
        return Check::new(CheckStatus::Unavailable)
            .with_detail(format!("{path} is not valid JSON"));
    };

    let updated_at = value
        .get("last_updated_at")
        .and_then(Value::as_str)
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok());

    let Some(updated_at) = updated_at else {
        return Check::new(CheckStatus::Unavailable).with_detail("missing `last_updated_at`");
    };

    let age_seconds = (Utc::now() - updated_at.with_timezone(&Utc)).num_seconds();

    let status = if age_seconds > GITHUB_STATS_STALE_AFTER_SECONDS {
        CheckStatus::Stale
    } else {
        CheckStatus::Ok
    };

    Check::new(status).with_age(age_seconds)
}

/// Milliseconds elapsed since `start`.
fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Whether the client asked for JSON through the `Accept` header.
fn wants_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| accept.to_ascii_lowercase().contains("json"))
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    };
    use chrono::{Duration, Utc};
    use sea_orm::DatabaseConnection;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::{check_github_stats, check_upstream, wants_json};
    use crate::status::{StatusConfig, models::CheckStatus};

    /// Start a local server answering every request with `code`, and return its URL.
    async fn spawn_upstream(code: StatusCode) -> String {
        let app = Router::new().fallback(move || async move { code });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        format!("http://{addr}")
    }

    /// Return the URL of a port nothing listens on.
    async fn closed_port_url() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        format!("http://{addr}")
    }

    fn accept(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn json_is_served_only_when_asked_for() {
        assert!(wants_json(&accept("application/json")));
        assert!(wants_json(&accept("text/json")));
        assert!(wants_json(&accept("text/html, application/json;q=0.9")));

        assert!(!wants_json(&HeaderMap::new()));
        assert!(!wants_json(&accept("*/*")));
        assert!(!wants_json(&accept(
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
        )));
    }

    /// Create an isolated directory holding a `github-stats.json` with the given contents.
    fn stats_dir(name: &str, contents: Option<&str>) -> String {
        let dir = std::env::temp_dir().join(format!("vps-back-status-{name}"));
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("github-stats.json");
        match contents {
            Some(contents) => std::fs::write(&path, contents).unwrap(),
            None => drop(std::fs::remove_file(&path)),
        }

        dir.to_str().unwrap().to_owned()
    }

    #[tokio::test]
    async fn fresh_github_stats_are_ok() {
        let last_updated_at = Utc::now().to_rfc3339();
        let dir = stats_dir(
            "fresh",
            Some(&format!(
                "{{\"last_updated_at\":\"{last_updated_at}\",\"repos\":[]}}"
            )),
        );

        let check = check_github_stats(&dir).await;

        assert_eq!(check.status, CheckStatus::Ok);
        assert!(check.age_seconds.unwrap() < 5);
    }

    #[tokio::test]
    async fn old_github_stats_are_stale() {
        let last_updated_at = (Utc::now() - Duration::hours(2)).to_rfc3339();
        let dir = stats_dir(
            "stale",
            Some(&format!(
                "{{\"last_updated_at\":\"{last_updated_at}\",\"repos\":[]}}"
            )),
        );

        let check = check_github_stats(&dir).await;

        assert_eq!(check.status, CheckStatus::Stale);
    }

    #[tokio::test]
    async fn missing_github_stats_are_unavailable() {
        let dir = stats_dir("missing", None);

        let check = check_github_stats(&dir).await;

        assert_eq!(check.status, CheckStatus::Unavailable);
        assert!(check.detail.unwrap().contains("not readable"));
    }

    #[tokio::test]
    async fn invalid_github_stats_are_unavailable() {
        let dir = stats_dir("invalid", Some("not json at all"));

        let check = check_github_stats(&dir).await;

        assert_eq!(check.status, CheckStatus::Unavailable);
        assert!(check.detail.unwrap().contains("not valid JSON"));
    }

    /// Router wired like `main`, but with a connection that always fails to ping
    /// and a local stand in for the `miaou` upstream.
    async fn degraded_app() -> Router {
        let upstream = spawn_upstream(StatusCode::OK).await;

        Router::new()
            .nest("/status", crate::status::router())
            .layer(axum::Extension(stats_dir("router", None)))
            .layer(axum::Extension(StatusConfig::new(upstream)))
            .with_state(DatabaseConnection::Disconnected)
    }

    async fn call(accept: Option<&str>) -> (StatusCode, String, String) {
        let mut request = Request::builder().uri("/status");
        if let Some(accept) = accept {
            request = request.header(header::ACCEPT, accept);
        }

        let response = degraded_app()
            .await
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();

        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        (
            status,
            content_type,
            String::from_utf8(body.to_vec()).unwrap(),
        )
    }

    #[tokio::test]
    async fn a_reachable_upstream_is_ok() {
        let url = spawn_upstream(StatusCode::OK).await;

        let check = check_upstream(&StatusConfig::new(url.clone()).client, &url).await;

        assert_eq!(check.status, CheckStatus::Ok);
        assert!(check.latency_ms.is_some());
        assert!(check.detail.is_none());
    }

    #[tokio::test]
    async fn an_upstream_answering_an_error_is_down() {
        let url = spawn_upstream(StatusCode::BAD_GATEWAY).await;

        let check = check_upstream(&StatusConfig::new(url.clone()).client, &url).await;

        assert_eq!(check.status, CheckStatus::Down);
        assert_eq!(check.detail.unwrap(), "HTTP 502");
    }

    #[tokio::test]
    async fn an_unreachable_upstream_is_down() {
        let url = closed_port_url().await;

        let check = check_upstream(&StatusConfig::new(url.clone()).client, &url).await;

        assert_eq!(check.status, CheckStatus::Down);
        assert_eq!(check.detail.unwrap(), "connection failed");
    }

    #[tokio::test]
    async fn serves_plain_text_by_default() {
        let (status, content_type, body) = call(None).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(content_type, "text/plain; charset=utf-8");
        assert!(body.starts_with("vps-back status: degraded\n"));
    }

    #[tokio::test]
    async fn serves_json_when_asked_for() {
        let (status, content_type, body) = call(Some("application/json")).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(content_type.starts_with("application/json"));

        let json: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["data"]["status"], "degraded");
        assert_eq!(json["data"]["checks"]["database"]["status"], "down");
        assert_eq!(json["data"]["checks"]["miaou"]["status"], "ok");
        assert_eq!(json["data"]["version"], env!("CARGO_PKG_VERSION"));
    }
}
