//! Static file serving module
//!
//! This module serves the files of the static directory.
//! When a request points to a directory, it renders an HTML page
//! that lists the files and directories in it.

use std::{fmt::Write as _, path::PathBuf, sync::Arc};

use axum::{
    Router,
    extract::{OriginalUri, Request, State},
    handler::Handler,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use tower_http::services::ServeDir;

/// Characters to percent-encode in a path segment of a link.
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// One entry of a directory listing.
struct Entry {
    name: String,
    is_dir: bool,
    size: u64,
    modified: Option<DateTime<Utc>>,
}

/// Creates a router that serves static files from `dir`.
///
/// A request to a file returns the file. A request to a directory
/// returns an HTML page that lists its content. Hidden entries
/// (names that start with a dot) are not listed.
///
/// # Parameters
/// * `dir` - The directory path to serve files from.
///
/// # Returns
/// * `Router` - A router to nest under the static path.
pub fn static_files_service(dir: &str) -> Router {
    let root = Arc::new(PathBuf::from(dir));
    let listing = directory_listing.with_state(root);

    let serve_dir = ServeDir::new(dir)
        .append_index_html_on_directories(false)
        .fallback(listing);

    Router::new().fallback_service(serve_dir)
}

/// Renders the listing of a directory, or returns `404` when the
/// path is not a directory.
async fn directory_listing(
    State(root): State<Arc<PathBuf>>,
    OriginalUri(original_uri): OriginalUri,
    request: Request,
) -> Response {
    let Some(relative) = sanitize_path(request.uri().path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let dir_path = root.join(&relative);
    let Ok(mut read_dir) = tokio::fs::read_dir(&dir_path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mut entries = Vec::new();
    while let Ok(Some(dir_entry)) = read_dir.next_entry().await {
        let name = dir_entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(metadata) = dir_entry.metadata().await else {
            continue;
        };
        entries.push(Entry {
            name,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified: metadata.modified().ok().map(DateTime::<Utc>::from),
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let mut base = original_uri.path().to_owned();
    if !base.ends_with('/') {
        base.push('/');
    }

    Html(render_page(
        &base,
        !relative.as_os_str().is_empty(),
        &entries,
    ))
    .into_response()
}

/// Decodes a request path and keeps only its normal segments.
///
/// Returns `None` when the path contains a `..` segment or a backslash,
/// so that a request cannot leave the static directory.
fn sanitize_path(path: &str) -> Option<PathBuf> {
    let decoded = percent_decode_str(path).decode_utf8().ok()?;
    let mut relative = PathBuf::new();

    for segment in decoded.split('/') {
        match segment {
            "" | "." => {}
            ".." => return None,
            s if s.contains('\\') => return None,
            s => relative.push(s),
        }
    }

    Some(relative)
}

/// Builds the HTML page of a directory listing.
fn render_page(base: &str, has_parent: bool, entries: &[Entry]) -> String {
    let title = escape_html(base);
    let mut rows = String::new();

    if has_parent {
        rows.push_str(r#"<tr><td><a class="dir" href="../">../</a></td><td></td><td></td></tr>"#);
    }

    for entry in entries {
        let suffix = if entry.is_dir { "/" } else { "" };
        let href = format!(
            "{base}{}{suffix}",
            utf8_percent_encode(&entry.name, PATH_SEGMENT)
        );
        let class = if entry.is_dir { "dir" } else { "file" };
        let size = if entry.is_dir {
            "-".to_owned()
        } else {
            format_size(entry.size)
        };
        let modified = entry.modified.map_or_else(
            || "-".to_owned(),
            |date| date.format("%Y-%m-%d %H:%M UTC").to_string(),
        );

        let _ = write!(
            rows,
            r#"<tr><td><a class="{class}" href="{href}">{name}{suffix}</a></td><td class="num">{size}</td><td>{modified}</td></tr>"#,
            href = escape_html(&href),
            name = escape_html(&entry.name),
        );
    }

    if entries.is_empty() {
        rows.push_str(r#"<tr><td colspan="3" class="empty">This directory is empty.</td></tr>"#);
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Index of {title}</title>
<style>
:root {{ --bg: #fafafa; --fg: #1f2328; --muted: #6e7781; --line: #d0d7de; --link: #0969da; --row: #f0f3f6; }}
@media (prefers-color-scheme: dark) {{
  :root {{ --bg: #0d1117; --fg: #e6edf3; --muted: #8b949e; --line: #30363d; --link: #58a6ff; --row: #161b22; }}
}}
* {{ box-sizing: border-box; }}
body {{ margin: 0; padding: 32px 16px; background: var(--bg); color: var(--fg); font: 15px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace; }}
main {{ max-width: 860px; margin: 0 auto; }}
h1 {{ font-size: 20px; font-weight: 600; margin: 0 0 20px; word-break: break-all; }}
table {{ width: 100%; border-collapse: collapse; }}
th, td {{ text-align: left; padding: 8px 12px; border-bottom: 1px solid var(--line); }}
th {{ color: var(--muted); font-weight: 500; }}
tr:hover td {{ background: var(--row); }}
td.num {{ text-align: right; white-space: nowrap; }}
td:last-child {{ color: var(--muted); white-space: nowrap; }}
td.empty {{ color: var(--muted); text-align: center; }}
a {{ color: var(--link); text-decoration: none; word-break: break-all; }}
a:hover {{ text-decoration: underline; }}
a.dir {{ font-weight: 600; }}
@media (max-width: 600px) {{ th:last-child, td:last-child {{ display: none; }} }}
</style>
</head>
<body>
<main>
<h1>Index of {title}</h1>
<table>
<thead><tr><th>Name</th><th class="num">Size</th><th>Modified</th></tr></thead>
<tbody>
{rows}
</tbody>
</table>
</main>
</body>
</html>
"#
    )
}

/// Formats a size in bytes as a human readable string.
fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];

    if bytes < 1024 {
        return format!("{bytes} B");
    }

    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    format!("{value:.1} {}", UNITS[unit])
}

/// Escapes the HTML special characters of a string.
fn escape_html(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(c),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_rejects_parent_segments() {
        assert!(sanitize_path("/../etc").is_none());
        assert!(sanitize_path("/a/%2E%2E/b").is_none());
        assert!(sanitize_path("/a\\b").is_none());
    }

    #[test]
    fn sanitize_keeps_normal_segments() {
        assert_eq!(sanitize_path("/"), Some(PathBuf::new()));
        assert_eq!(
            sanitize_path("/a/./my%20dir/"),
            Some(PathBuf::from("a/my dir"))
        );
    }

    #[test]
    fn format_size_uses_units() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn escape_html_escapes_special_chars() {
        assert_eq!(
            escape_html(r#"<a href="x">&'"#),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }
}
