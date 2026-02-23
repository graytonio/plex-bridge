# PlexBridge

A self-hosted, single-binary web service that bridges your home Plex server to a mobile one (laptop, NUC, travel machine). Browse your home library, queue content for download, and watch it land in your local Plex automatically — all from a web UI with live progress.

## Features

- **Browse your home library** — movies, TV shows, seasons, and individual episodes
- **Queue downloads** — select individual items or entire seasons
- **Live progress** — real-time progress bars via Server-Sent Events (no polling)
- **Resumable downloads** — interrupted transfers pick up where they left off on restart
- **Auto library scan** — completed downloads trigger a local Plex library refresh automatically
- **Synced status** — already-downloaded items are marked in the browser so you don't queue duplicates
- **Single binary** — no Redis, no message broker, no external dependencies beyond an optional Postgres instance
- **Two database backends** — SQLite (default, zero-dep) or PostgreSQL (for Kubernetes / multi-replica)

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   Single Rust Binary                │
│                                                     │
│  ┌─────────────┐   ┌──────────────┐   ┌──────────┐  │
│  │  Axum Web   │   │  Sync Engine │   │ SQLite   │  │
│  │  Server     │◄──│  (tokio      │──►│   OR     │  │
│  │  + HTMX UI  │   │   tasks)     │   │ Postgres │  │
│  └──────┬──────┘   └──────┬───────┘   └──────────┘  │
│         │                 │                         │
│         │ SSE             │ reqwest                 │
│         ▼                 ▼                         │
│     Browser          Remote Plex API                │
└─────────────────────────────────────────────────────┘
         │                 │
    Local Plex API    Home Plex Server
    (library scan)    (file download)
```

**Stack:** Rust · Axum · HTMX · SQLite/PostgreSQL (sqlx) · SSE

## Quickstart

### Prerequisites

- [Rust](https://rustup.rs) (stable toolchain)
- A running Plex Media Server on your home network with a valid **X-Plex-Token**
- A running Plex Media Server on the local machine (for the destination library)

### Finding your Plex token

Sign into [plex.tv](https://www.plex.tv), open any media item in the web app, click the three-dot menu → **Get Info** → **View XML**. The `X-Plex-Token` value appears in the URL.

### Build and run (SQLite)

```bash
git clone https://github.com/youruser/plex-bridge
cd plex-bridge

cargo build --release

PLEXBRIDGE_DATABASE_URL=sqlite://./plexbridge.db \
  ./target/release/plexbridge
```

Open **http://localhost:7878** in your browser. On first launch you'll be redirected to the Settings page to configure your servers.

### Settings

Fill in the settings form at `/settings`:

| Field | Example | Description |
|---|---|---|
| Home server URL | `http://192.168.1.10:32400` | Your remote Plex server |
| Home Plex token | `xxxxxxxxxxxxxxxxxxxx` | X-Plex-Token for the home server |
| Local server URL | `http://localhost:32400` | Plex running on this machine |
| Local Plex token | `xxxxxxxxxxxxxxxxxxxx` | X-Plex-Token for the local server |
| Movies path | `/media/Movies` | Absolute path to your local movies folder |
| TV path | `/media/TV` | Absolute path to your local TV folder |
| Max concurrent | `2` | Simultaneous downloads (1–5) |

Use the **Test Connection** button to verify connectivity before saving.

## Configuration

All options can be set via environment variable or CLI flag.

```bash
# Environment variables
PLEXBRIDGE_DATABASE_URL=sqlite://./plexbridge.db
PLEXBRIDGE_PORT=7878
PLEXBRIDGE_LOG=info          # trace | debug | info | warn | error
```

```bash
# CLI flags
plexbridge --database-url sqlite://./plexbridge.db --port 7878
```

The database URL scheme determines the backend — no other flag needed:

```bash
# SQLite (default — single file, zero external dependencies)
PLEXBRIDGE_DATABASE_URL=sqlite://./plexbridge.db

# PostgreSQL (for Kubernetes or environments where SQLite file locking is unreliable)
PLEXBRIDGE_DATABASE_URL=postgres://user:password@localhost:5432/plexbridge
```

Database migrations run automatically on every startup.

## Deployment

### Docker (SQLite)

```dockerfile
FROM rust:alpine AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM alpine:latest
COPY --from=builder /app/target/release/plexbridge /usr/local/bin/
ENTRYPOINT ["plexbridge"]
```

```bash
docker run -d \
  -p 7878:7878 \
  -v $(pwd)/data:/data \
  -v /path/to/media:/media \
  -e PLEXBRIDGE_DATABASE_URL=sqlite:///data/plexbridge.db \
  plexbridge:latest
```

### Docker Compose (PostgreSQL)

```yaml
services:
  plexbridge:
    image: plexbridge:latest
    ports:
      - "7878:7878"
    volumes:
      - /path/to/media:/media
    environment:
      PLEXBRIDGE_DATABASE_URL: postgres://plexbridge:secret@db:5432/plexbridge
      PLEXBRIDGE_PORT: 7878
    depends_on:
      db:
        condition: service_healthy

  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: plexbridge
      POSTGRES_USER: plexbridge
      POSTGRES_PASSWORD: secret
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U plexbridge"]
      interval: 5s
      retries: 5

volumes:
  pgdata:
```

### Kubernetes

Deploy as a single-replica `Deployment` — download state is in-process so horizontal scaling is not supported in the current release. Point `DATABASE_URL` at an external managed Postgres or an in-cluster `postgres` StatefulSet. Mount media paths as a `PersistentVolumeClaim`.

```yaml
env:
  - name: PLEXBRIDGE_DATABASE_URL
    value: postgres://plexbridge:secret@postgres-svc:5432/plexbridge
volumeMounts:
  - name: media
    mountPath: /media
```

## Download behavior

- **Resumable** — on restart, in-progress jobs resume from their last saved byte offset using HTTP `Range` headers.
- **Concurrent** — a configurable worker pool (default 2) processes jobs from a queue. Adjust `max_concurrent` in settings (1–5).
- **Graceful shutdown** — on `SIGTERM`, active workers flush their current progress to the database before exiting.
- **Auto-scan** — on completion, the local Plex library section is refreshed via the Plex API so the file appears immediately.

## Development

```bash
# Run in development mode with debug logging
PLEXBRIDGE_DATABASE_URL=sqlite://./plexbridge.db \
PLEXBRIDGE_LOG=debug \
  cargo run

# Run tests
cargo test

# Check test coverage (requires cargo-llvm-cov)
cargo install cargo-llvm-cov
cargo llvm-cov --summary-only
```

## Limitations (current release)

- Single-user only — no authentication. Run on a trusted local network or behind a reverse proxy with auth.
- Manual sync only — no scheduled or watchlist-driven automation.
- Movies and TV shows only — music and photo libraries are not supported.
- Single home server — one source Plex server per instance.
