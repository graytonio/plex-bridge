CREATE TABLE IF NOT EXISTS config (
    id BIGSERIAL PRIMARY KEY,
    home_server_url TEXT NOT NULL DEFAULT '',
    home_plex_token TEXT NOT NULL DEFAULT '',
    local_server_url TEXT NOT NULL DEFAULT '',
    local_plex_token TEXT NOT NULL DEFAULT '',
    movies_path TEXT NOT NULL DEFAULT '',
    tv_path TEXT NOT NULL DEFAULT '',
    max_concurrent BIGINT NOT NULL DEFAULT 2,
    CONSTRAINT config_single_row CHECK (id = 1)
);

CREATE TABLE IF NOT EXISTS sync_jobs (
    id BIGSERIAL PRIMARY KEY,
    plex_rating_key TEXT NOT NULL,
    media_type TEXT NOT NULL,
    title TEXT NOT NULL,
    show_title TEXT,
    season_number BIGINT,
    episode_number BIGINT,
    file_size_bytes BIGINT NOT NULL DEFAULT 0,
    destination_path TEXT NOT NULL DEFAULT '',
    source_url TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'queued',
    bytes_downloaded BIGINT NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
