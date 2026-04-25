CREATE TABLE IF NOT EXISTS etag_cache (
    url         TEXT PRIMARY KEY NOT NULL,
    etag        TEXT NOT NULL,
    body        BLOB NOT NULL,
    fetched_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_etag_cache_fetched_at
    ON etag_cache (fetched_at);
