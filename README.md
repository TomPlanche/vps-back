# vps-back

Rust/Axum backend running on my VPS. Serves static files and generates GitHub stats.

## Setup

```bash
cp .env.example .env
cargo run
```

## Configuration

| Variable          | Required | Default                                       |
|-------------------|----------|-----------------------------------------------|
| `HOST`            | no       | `127.0.0.1`                                   |
| `PORT`            | no       | `8000`                                        |
| `ALLOWED_ORIGINS` | no       | `http://localhost:3000,http://localhost:5173`  |
| `RUST_LOG`        | no       | `info`                                        |
| `STATIC_DIR`      | no       | `static`                                      |
| `GITHUB_TOKEN`    | no       | (none)                                        |

## Endpoints

**GET /**: health check

**GET /static/\<path\>**: static files (LastFM fetcher output, `github-stats.json`)

## GitHub stats

When `GITHUB_TOKEN` is set, a background task fetches the 6 most recently updated repos (owner + collaborator) every 5 minutes. It writes them to `<STATIC_DIR>/github-stats.json`, served at `/static/github-stats.json`:

```json
{
  "last_updated_at": "2025-01-01T00:00:00+00:00",
  "repos": [
    {
      "name": "rona",
      "link": "https://github.com/rona-rs/rona",
      "stars": 2,
      "owner": "rona-rs",
      "description": "A powerful CLI tool",
      "commits": {
        "total": 293,
        "last": { "hash": "faf085d", "link": "https://github.com/...", "date": "2025-01-01T00:00:00Z" }
      }
    }
  ]
}
```

## Development

```bash
cargo check              # type-check
cargo clippy             # lint
cargo test               # tests
cargo build --release    # production build
```

## License

MIT
