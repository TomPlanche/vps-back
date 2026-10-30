mod fetcher;
mod models;

use std::time::Duration;

use models::GithubStatsFile;

pub async fn run_periodic_update(token: String, static_dir: String) {
    let client = match fetcher::build_client(&token) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to build HTTP client for GitHub stats: {e:#}");
            return;
        }
    };

    let mut interval = tokio::time::interval(Duration::from_mins(5));

    loop {
        interval.tick().await;

        match fetcher::fetch_stats(&client).await {
            Ok(stats) => write_stats(&stats, &static_dir).await,
            Err(e) => tracing::error!("GitHub stats fetch failed: {e:#}"),
        }
    }
}

async fn write_stats(stats: &[models::RepoStats], static_dir: &str) {
    let payload = GithubStatsFile {
        last_updated_at: chrono::Utc::now().to_rfc3339(),
        repos: stats,
    };

    let json = match serde_json::to_string_pretty(&payload) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Failed to serialize GitHub stats: {e}");
            return;
        }
    };

    let path = format!("{static_dir}/github-stats.json");

    match tokio::fs::write(&path, json).await {
        Ok(()) => tracing::info!("GitHub stats written to {path}"),
        Err(e) => tracing::error!("Failed to write GitHub stats to {path}: {e}"),
    }
}
