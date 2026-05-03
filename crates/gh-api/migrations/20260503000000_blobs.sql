CREATE TABLE IF NOT EXISTS blobs (
    sha         TEXT PRIMARY KEY NOT NULL,
    body        BLOB NOT NULL,
    fetched_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_blobs_fetched_at
    ON blobs (fetched_at);
