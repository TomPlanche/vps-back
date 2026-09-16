//! Data structures for the status report.
//!
//! The report is rendered either as JSON (when the client asks for it through
//! the `Accept` header) or as plain text (the default, so `curl` output stays
//! readable).

use serde::Serialize;

/// Overall health of the service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    /// Every critical check passed.
    Ok,
    /// At least one critical check failed.
    Degraded,
}

impl ServiceStatus {
    /// Label used in the plain text rendering.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Degraded => "degraded",
        }
    }
}

/// Outcome of a single check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// The dependency answered as expected.
    Ok,
    /// The dependency answered but its data is older than expected.
    Stale,
    /// The dependency could not be read (missing or unusable data).
    Unavailable,
    /// The dependency is unreachable.
    Down,
}

impl CheckStatus {
    /// Label used in the plain text rendering.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
            Self::Down => "down",
        }
    }
}

/// A single dependency check.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub status: CheckStatus,
    /// Round trip time of the check, when it is measured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Age of the data the check looked at, when it is relevant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_seconds: Option<i64>,
    /// Human readable explanation, set when something went wrong.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Check {
    /// Create a check holding only a status.
    #[must_use]
    pub const fn new(status: CheckStatus) -> Self {
        Self {
            status,
            latency_ms: None,
            age_seconds: None,
            detail: None,
        }
    }

    /// Attach the measured round trip time.
    #[must_use]
    pub const fn with_latency(mut self, latency_ms: u64) -> Self {
        self.latency_ms = Some(latency_ms);
        self
    }

    /// Attach the age of the data the check looked at.
    #[must_use]
    pub const fn with_age(mut self, age_seconds: i64) -> Self {
        self.age_seconds = Some(age_seconds);
        self
    }

    /// Attach a human readable explanation.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Whether the check is considered passing.
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self.status, CheckStatus::Ok)
    }
}

/// Every check run by the status endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct Checks {
    /// `PostgreSQL` connectivity. Critical: a failure makes the endpoint answer `503`.
    pub database: Check,
    /// `miaou.tomplanche.com`, reached over HTTP.
    pub miaou: Check,
    /// Freshness of `github-stats.json`.
    pub github_stats: Check,
}

impl Checks {
    /// Every check, in the order they are reported.
    fn all(&self) -> [(&'static str, &Check); 3] {
        [
            ("database", &self.database),
            ("miaou", &self.miaou),
            ("github-stats", &self.github_stats),
        ]
    }

    /// Whether every check passed.
    #[must_use]
    pub fn all_ok(&self) -> bool {
        self.all().iter().all(|(_, check)| check.is_ok())
    }
}

/// Full status report.
#[derive(Debug, Clone, Serialize)]
pub struct StatusReport {
    pub status: ServiceStatus,
    pub version: &'static str,
    pub started_at: String,
    pub uptime_seconds: u64,
    pub checks: Checks,
}

impl StatusReport {
    /// Render the report as plain text, for browsers and `curl`.
    #[must_use]
    pub fn to_text(&self) -> String {
        let status = self.status.label();
        let version = self.version;
        let started_at = &self.started_at;
        let uptime = format_duration(self.uptime_seconds);

        let mut out = format!(
            "vps-back status: {status}\n\nversion    {version}\nstarted    {started_at}\nuptime     {uptime}\n\nchecks\n"
        );

        for (name, check) in self.checks.all() {
            out.push_str(&check_line(name, check));
        }

        out
    }
}

/// Render one check as a plain text line.
fn check_line(name: &str, check: &Check) -> String {
    let mut extra: Vec<String> = Vec::new();

    if let Some(latency_ms) = check.latency_ms {
        extra.push(format!("{latency_ms} ms"));
    }

    if let Some(age_seconds) = check.age_seconds {
        let age = format_duration(u64::try_from(age_seconds).unwrap_or(0));
        extra.push(format!("updated {age} ago"));
    }

    if let Some(detail) = &check.detail {
        extra.push(detail.clone());
    }

    let status = check.status.label();
    let extra = extra.join(", ");
    let line = format!("  {name:<14}{status:<14}{extra}");

    format!("{}\n", line.trim_end())
}

/// Format a number of seconds as a compact duration such as `2d 3h 4m 5s`.
#[must_use]
pub fn format_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let secs = seconds % 60;

    let mut parts: Vec<String> = Vec::new();

    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if secs > 0 || parts.is_empty() {
        parts.push(format!("{secs}s"));
    }

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::{Check, CheckStatus, Checks, ServiceStatus, StatusReport, format_duration};

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(45), "45s");
        assert_eq!(format_duration(133), "2m 13s");
        assert_eq!(format_duration(3_600), "1h");
        assert_eq!(format_duration(90_061), "1d 1h 1m 1s");
    }

    #[test]
    fn renders_a_healthy_report_as_text() {
        let report = StatusReport {
            status: ServiceStatus::Ok,
            version: "2.0.1",
            started_at: "2026-09-15T10:12:00+00:00".to_owned(),
            uptime_seconds: 84_253,
            checks: Checks {
                database: Check::new(CheckStatus::Ok).with_latency(3),
                miaou: Check::new(CheckStatus::Ok).with_latency(42),
                github_stats: Check::new(CheckStatus::Ok).with_age(133),
            },
        };

        let text = report.to_text();

        assert!(text.starts_with("vps-back status: ok\n"));
        assert!(text.contains("version    2.0.1\n"));
        assert!(text.contains("uptime     23h 24m 13s\n"));
        assert!(text.contains("  database      ok            3 ms\n"));
        assert!(text.contains("  miaou         ok            42 ms\n"));
        assert!(text.contains("  github-stats  ok            updated 2m 13s ago\n"));
        assert!(report.checks.all_ok());
    }

    #[test]
    fn renders_a_degraded_report_as_text() {
        let report = StatusReport {
            status: ServiceStatus::Degraded,
            version: "2.0.1",
            started_at: "2026-09-15T10:12:00+00:00".to_owned(),
            uptime_seconds: 12,
            checks: Checks {
                database: Check::new(CheckStatus::Down).with_detail("connection refused"),
                miaou: Check::new(CheckStatus::Down)
                    .with_latency(5_000)
                    .with_detail("HTTP 502"),
                github_stats: Check::new(CheckStatus::Stale).with_age(3_600),
            },
        };

        let text = report.to_text();

        assert!(text.starts_with("vps-back status: degraded\n"));
        assert!(text.contains("  database      down          connection refused\n"));
        assert!(text.contains("  miaou         down          5000 ms, HTTP 502\n"));
        assert!(text.contains("  github-stats  stale         updated 1h ago\n"));
        assert!(!report.checks.all_ok());
    }

    #[test]
    fn serializes_only_the_fields_that_are_set() {
        let json = serde_json::to_value(Check::new(CheckStatus::Ok).with_latency(3)).unwrap();

        assert_eq!(json["status"], "ok");
        assert_eq!(json["latency_ms"], 3);
        assert!(json.get("age_seconds").is_none());
        assert!(json.get("detail").is_none());
    }
}
