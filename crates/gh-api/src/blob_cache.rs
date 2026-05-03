//! Blob-SHA cache. Indefinite, append-only storage keyed by Git blob SHA.
//!
//! Git blobs are content-addressed: a given SHA always points at the same
//! bytes forever, so cached entries are never invalidated. A future
//! eviction policy could prune entries older than N days; for now we let
//! the table grow unbounded.
//!
//! Persistent variant shares the [`crate::EtagCache`]'s SQLite pool — one
//! file, two tables. In-memory fallback uses its own `HashMap` so the
//! two caches degrade independently when the disk path is unavailable.

use std::{collections::HashMap, sync::Arc, time::SystemTime};

use sqlx::SqlitePool;
use tokio::sync::Mutex;
use tracing::warn;

use crate::cache::{CacheError, EtagCache};

#[derive(Debug, Clone)]
pub enum BlobCache {
    Persistent(SqlitePool),
    InMemory(Arc<Mutex<HashMap<String, Vec<u8>>>>),
}

impl BlobCache {
    /// Build a blob cache that shares the etag cache's SQLite pool when
    /// persistent. Falls back to an independent in-memory map otherwise.
    #[must_use]
    pub fn paired_with(etag: &EtagCache) -> Self {
        match etag.pool() {
            Some(pool) => Self::Persistent(pool.clone()),
            None => Self::in_memory(),
        }
    }

    /// In-memory fallback. Lost on process exit.
    #[must_use]
    pub fn in_memory() -> Self {
        Self::InMemory(Arc::new(Mutex::new(HashMap::new())))
    }

    /// Look up a blob by SHA. Returns `None` on cache miss or SQL error
    /// (errors are logged + treated as miss to keep callers on the happy
    /// path).
    pub async fn get(&self, sha: &str) -> Option<Vec<u8>> {
        match self {
            Self::Persistent(pool) => {
                let res: Result<Option<(Vec<u8>,)>, sqlx::Error> =
                    sqlx::query_as("SELECT body FROM blobs WHERE sha = ?1 LIMIT 1")
                        .bind(sha)
                        .fetch_optional(pool)
                        .await;
                match res {
                    Ok(Some((body,))) => Some(body),
                    Ok(None) => None,
                    Err(e) => {
                        warn!(error = %e, "blob cache get failed; treating as miss");
                        None
                    }
                }
            }
            Self::InMemory(m) => m.lock().await.get(sha).cloned(),
        }
    }

    /// Store a blob. Idempotent on the SHA (Git's content-addressing
    /// guarantees the body is always the same), so we use INSERT OR
    /// REPLACE in the persistent path.
    pub async fn put(&self, sha: &str, body: &[u8]) -> Result<(), CacheError> {
        match self {
            Self::Persistent(pool) => {
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                sqlx::query(
                    "INSERT INTO blobs (sha, body, fetched_at) \
                     VALUES (?1, ?2, ?3) \
                     ON CONFLICT(sha) DO UPDATE SET \
                        body = excluded.body, \
                        fetched_at = excluded.fetched_at",
                )
                .bind(sha)
                .bind(body)
                .bind(now)
                .execute(pool)
                .await?;
            }
            Self::InMemory(m) => {
                m.lock().await.insert(sha.to_string(), body.to_vec());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_round_trip() {
        let c = BlobCache::in_memory();
        assert!(c.get("deadbeef").await.is_none());
        c.put("deadbeef", b"hello").await.unwrap();
        assert_eq!(c.get("deadbeef").await.unwrap(), b"hello");
    }

    #[tokio::test]
    async fn in_memory_idempotent_put() {
        let c = BlobCache::in_memory();
        c.put("sha", b"v1").await.unwrap();
        c.put("sha", b"v2").await.unwrap();
        assert_eq!(c.get("sha").await.unwrap(), b"v2");
    }

    #[tokio::test]
    async fn paired_with_in_memory_etag_falls_back_to_in_memory() {
        let etag = EtagCache::in_memory();
        let blob = BlobCache::paired_with(&etag);
        assert!(matches!(blob, BlobCache::InMemory(_)));
    }
}
