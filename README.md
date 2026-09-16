# vps-back

Rust/Axum backend running on my VPS. Handles analytics tracking, location stickers, and Homebrew bottle download counting. PostgreSQL via SeaORM.

## Setup

```bash
cp .env.example .env
sea-orm-cli migrate up
cargo run
```

## Configuration

| Variable          | Required | Default                                       |
|-------------------|----------|-----------------------------------------------|
| `API_KEY`         | yes      | —                                             |
| `DATABASE_URL`    | yes      | —                                             |
| `HOST`            | no       | `127.0.0.1`                                   |
| `PORT`            | no       | `8000`                                        |
| `ALLOWED_ORIGINS` | no       | `http://localhost:3000,http://localhost:5173`  |
| `RUST_LOG`        | no       | `info`                                        |
| `STATIC_DIR`      | no       | `static`                                      |
| `GITHUB_TOKEN`    | no       | —                                             |
| `MIAOU_URL`       | no       | `https://miaou.tomplanche.com`                |

`DATABASE_URL` format: `postgresql://user:password@host:port/dbname`

## Endpoints

### Public

**GET /** — health check

**GET /status**: status report, served on its own subdomain (`status.tomplanche.com`). Plain text by default, JSON when the `Accept` header asks for it (`application/json`, `text/json`).

```
$ curl https://status.tomplanche.com
vps-back status: ok

version    2.0.1
started    2026-09-15T10:12:00+00:00
uptime     23h 24m 13s

checks
  database      ok            3 ms
  miaou         ok            42 ms
  github-stats  ok            updated 2m 13s ago
```

```json
{
  "data": {
    "status": "ok",
    "version": "2.0.1",
    "started_at": "2026-09-15T10:12:00+00:00",
    "uptime_seconds": 84253,
    "checks": {
      "database": { "status": "ok", "latency_ms": 3 },
      "miaou": { "status": "ok", "latency_ms": 42 },
      "github_stats": { "status": "ok", "age_seconds": 133 }
    }
  }
}
```

Checks:

| Check          | What it does                                                                                | Statuses                          |
|----------------|---------------------------------------------------------------------------------------------|-----------------------------------|
| `database`     | pings `PostgreSQL` and measures the round trip                                                | `ok`, `down`                      |
| `miaou`        | `GET`s `MIAOU_URL` (10 s timeout); `2xx` and `3xx` count as up                                  | `ok`, `down`                      |
| `github_stats` | reads `last_updated_at` in `github-stats.json`; the file is refreshed every 5 minutes          | `ok`, `stale`, `unavailable`      |

The top level `status` is `ok` only when every check passes, `degraded` otherwise.

The HTTP code reports this API alone: `503` when the database is unreachable, `200` otherwise. So an uptime monitor watching the HTTP code is not woken up by `miaou` being down, while the body still shows exactly what is broken.

**GET /static/\<path\>** — static files (LastFM fetcher output, `github-stats.json`)

**GET /brew/track/:project/:filename** — records a Homebrew bottle download and redirects (`302`) to the real GitHub release asset. Used by setting `root_url` in the formula's bottle block:

```ruby
bottle do
  root_url "https://your-server.com/brew/track/rona"
  sha256 cellar: :any_skip_relocation, arm64_sequoia: "<sha>"
  sha256 cellar: :any_skip_relocation, sequoia:       "<sha>"
  sha256 cellar: :any_skip_relocation, x86_64_linux:  "<sha>"
end
```

**GET /brew/stats** — download counts grouped by project and version:

```json
{
  "data": {
    "rona": {
      "total_downloads": 120,
      "total_installs": 120,
      "2.17.7": { "downloads": 80, "installs": 80 }
    }
  }
}
```

**GET /stats/github** — 6 most recently updated repos (owner + collaborator), generated every 5 minutes from `GITHUB_TOKEN`. Returns `null` if the token is not set or the file has not been written yet:

```json
{
  "data": [
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

**GET /stats/brew** — download counts grouped by project and version (same data as `/brew/stats`).

**GET /stats/sources** — all analytics source counters, name → count:

```json
{ "data": { "twitter": 42, "github": 17 } }
```

### Protected

All `/secure/*` routes require `x-api-key: <key>` in the request headers.

| Method | Path                   | Description                        |
|--------|------------------------|------------------------------------|
| GET    | /secure/source         | All sources and their hit counts   |
| POST   | /secure/source         | Increment a source counter         |
| GET    | /secure/stickers       | All stickers, newest first         |
| GET    | /secure/stickers/:id   | Single sticker                     |
| POST   | /secure/stickers       | Create a sticker                   |

Paginated endpoints accept `?page=1&limit=20` (max limit: 100) and include a `_metadata` field in the response.

## Database

```bash
sea-orm-cli migrate up       # apply pending
sea-orm-cli migrate status   # check state
sea-orm-cli migrate down     # roll back one
sea-orm-cli migrate refresh  # reset and re-run all
```

Tables: `sources`, `stickers`, `brew_downloads`.

## Deployment

The status endpoint is exposed on its own subdomain through Caddy. The site block lives in `deploy/Caddyfile.example`: copy it into the VPS Caddyfile and reload Caddy. It rewrites every path to `/status`, so the whole subdomain answers the report.

`miaou.tomplanche.com` is checked through its public URL, so the check covers Caddy and the `10.10.0.2:80` upstream behind it, not just the container. Point `MIAOU_URL` somewhere else (`http://10.10.0.2:80` for instance) to check the upstream directly.

## Development

```bash
cargo check              # type-check
cargo clippy             # lint
cargo test               # tests
cargo build --release    # production build
RUST_LOG=sqlx::query=debug cargo run  # with SQL logging
```

## License

MIT
