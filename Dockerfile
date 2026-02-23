# ── Build stage ───────────────────────────────────────────────────────────────
FROM rust:1-slim-bookworm AS builder

WORKDIR /app

# System libraries needed to link reqwest (native-tls).
# SQLite is statically bundled via LIBSQLITE3_SYS_PREFER_BUNDLED so no
# libsqlite3-dev is required here and no libsqlite3-0 is needed at runtime.
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

ENV LIBSQLITE3_SYS_PREFER_BUNDLED=1

# ── Dependency cache layer ────────────────────────────────────────────────────
# Copy manifests first and build a dummy binary to compile all dependencies.
# This layer is only invalidated when Cargo.toml or Cargo.lock change.
# The dummy src/main.rs avoids triggering our template/derive macros, which
# require the full source tree to be present.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
    && echo 'fn main(){}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

# ── Application build ─────────────────────────────────────────────────────────
COPY . .
# Touch src/main.rs so cargo recognises the source as newer than the cached
# dummy artifact and rebuilds our crate (not the dependencies).
RUN touch src/main.rs \
    && cargo build --release --locked

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        libssl3 \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Run as a non-root user
RUN useradd -m -u 1000 plexbridge

COPY --from=builder /app/target/release/plexbridge /usr/local/bin/plexbridge

# /data is the default mount point for the SQLite file.
# Override with -v /host/path:/data in docker run, or set DATABASE_URL for
# PostgreSQL to avoid needing a persistent volume at all.
RUN mkdir -p /data && chown plexbridge:plexbridge /data

USER plexbridge
WORKDIR /data

EXPOSE 7878

ENV PLEXBRIDGE_DATABASE_URL=sqlite:///data/plexbridge.db
ENV PLEXBRIDGE_PORT=7878
ENV PLEXBRIDGE_LOG=info

ENTRYPOINT ["plexbridge"]
