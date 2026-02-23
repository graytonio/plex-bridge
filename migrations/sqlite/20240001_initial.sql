CREATE TABLE IF NOT EXISTS config (
    id INTEGER PRIMARY KEY NOT NULL DEFAULT 1,
    home_server_url TEXT NOT NULL DEFAULT '',
    home_plex_token TEXT NOT NULL DEFAULT '',
    local_server_url TEXT NOT NULL DEFAULT '',
    local_plex_token TEXT NOT NULL DEFAULT '',
    movies_path TEXT NOT NULL DEFAULT '',
    tv_path TEXT NOT NULL DEFAULT '',
    max_concurrent INTEGER NOT NULL DEFAULT 2,
    CHECK (id = 1)
);

CREATE TABLE IF NOT EXISTS sync_jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plex_rating_key TEXT NOT NULL,
    media_type TEXT NOT NULL,
    title TEXT NOT NULL,
    show_title TEXT,
    season_number INTEGER,
    episode_number INTEGER,
    file_size_bytes INTEGER NOT NULL DEFAULT 0,
    destination_path TEXT NOT NULL DEFAULT '',
    source_url TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'queued',
    bytes_downloaded INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at DATETIME NOT NULL DEFAULT (datetime('now')),
    updated_at DATETIME NOT NULL DEFAULT (datetime('now'))
);
