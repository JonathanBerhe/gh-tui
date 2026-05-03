//! ETag cache for conditional GETs.
//!
//! Two backends: a SQLite-backed [`EtagCache::Persistent`] (the real one,
//! survives across runs) and an [`EtagCache::InMemory`] fallback for when
//! the SQLite open fails (corrupt DB, locked, etc.) — keeps the binary
//! launchable instead of panicking. Both honor the same async API.
//!
//! Cache key is the request URL verbatim. The request layer is responsible
//! for canonicalizing the URL before calling [`EtagCache::get`] /
//! [`EtagCache::put`] if it constructs the same logical request more than
//! one way.

use std::{collections::HashMap, path::Path, sync::Arc, time::SystemTime};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("could not create cache directory: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct CachedEntry {
    pub etag: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum EtagCache {
    Persistent(SqlitePool),
    InMemory(Arc<Mutex<HashMap<String, CachedEntry>>>),
}

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

impl EtagCache {
    /// Open or create a persistent SQLite cache at `path`. Runs migrations.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, CacheError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(opts)
            .await?;
        MIGRATOR.run(&pool).await?;
        debug!(path = %path.display(), "etag cache opened");
        Ok(Self::Persistent(pool))
    }

    /// In-memory fallback. Lost on process exit.
    #[must_use]
    pub fn in_memory() -> Self {
        Self::InMemory(Arc::new(Mutex::new(HashMap::new())))
    }

    /// Underlying SQLite pool when persistent. `None` for the in-memory
    /// fallback. Used by `BlobCache` to share a single pool/file with the
    /// etag cache.
    #[must_use]
    pub fn pool(&self) -> Option<&SqlitePool> {
        match self {
            Self::Persistent(p) => Some(p),
            Self::InMemory(_) => None,
        }
    }

    pub async fn get(&self, url: &str) -> Option<CachedEntry> {
        match self {
            Self::Persistent(pool) => {
                let res: Result<Option<(String, Vec<u8>)>, sqlx::Error> =
                    sqlx::query_as("SELECT etag, body FROM etag_cache WHERE url = ?1 LIMIT 1")
                        .bind(url)
                        .fetch_optional(pool)
                        .await;
                match res {
                    Ok(Some((etag, body))) => Some(CachedEntry { etag, body }),
                    Ok(None) => None,
                    Err(e) => {
                        warn!(error = %e, "cache get failed; treating as miss");
                        None
                    }
                }
            }
            Self::InMemory(m) => m.lock().await.get(url).cloned(),
        }
    }

    pub async fn put(&self, url: &str, etag: &str, body: &[u8]) -> Result<(), CacheError> {
        match self {
            Self::Persistent(pool) => {
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                sqlx::query(
                    "INSERT INTO etag_cache (url, etag, body, fetched_at) \
                     VALUES (?1, ?2, ?3, ?4) \
                     ON CONFLICT(url) DO UPDATE SET \
                        etag = excluded.etag, \
                        body = excluded.body, \
                        fetched_at = excluded.fetched_at",
                )
                .bind(url)
                .bind(etag)
                .bind(body)
                .bind(now)
                .execute(pool)
                .await?;
            }
            Self::InMemory(m) => {
                m.lock().await.insert(
                    url.to_string(),
                    CachedEntry {
                        etag: etag.to_string(),
                        body: body.to_vec(),
                    },
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn in_memory_round_trip() {
        let c = EtagCache::in_memory();
        assert!(c.get("u").await.is_none());
        c.put("u", "W/\"abc\"", b"hello").await.unwrap();
        let got = c.get("u").await.unwrap();
        assert_eq!(got.etag, "W/\"abc\"");
        assert_eq!(got.body, b"hello");
    }

    #[tokio::test]
    async fn in_memory_overwrites_on_conflict() {
        let c = EtagCache::in_memory();
        c.put("u", "v1", b"old").await.unwrap();
        c.put("u", "v2", b"new").await.unwrap();
        let got = c.get("u").await.unwrap();
        assert_eq!(got.etag, "v2");
        assert_eq!(got.body, b"new");
    }

    #[tokio::test]
    async fn persistent_round_trip() {
        let dir = tempdir();
        let path = dir.join("cache.db");
        let c = EtagCache::open(&path).await.unwrap();
        assert!(c.get("u").await.is_none());
        c.put("u", "W/\"abc\"", b"hello").await.unwrap();
        let got = c.get("u").await.unwrap();
        assert_eq!(got.etag, "W/\"abc\"");
        assert_eq!(got.body, b"hello");
    }

    #[tokio::test]
    async fn persistent_overwrites_on_conflict() {
        let dir = tempdir();
        let path = dir.join("cache.db");
        let c = EtagCache::open(&path).await.unwrap();
        c.put("u", "v1", b"old").await.unwrap();
        c.put("u", "v2", b"new").await.unwrap();
        let got = c.get("u").await.unwrap();
        assert_eq!(got.etag, "v2");
        assert_eq!(got.body, b"new");
    }

    fn tempdir() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        p.push(format!("gh-tui-test-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
