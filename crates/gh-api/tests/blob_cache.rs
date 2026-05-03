//! Integration tests for `BlobCache`'s persistent (SQLite) path. The
//! in-memory variant is exercised by the unit tests in `src/blob_cache.rs`;
//! these confirm the schema migration runs cleanly and that a separate
//! process (well, a separate `BlobCache::paired_with` after closing the
//! pool) can read previously-written blobs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gh_api::{BlobCache, EtagCache};

fn tempdir() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "gh-tui-blob-cache-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[tokio::test]
async fn persistent_round_trip() {
    let dir = tempdir();
    let path = dir.join("cache.db");
    let etag = EtagCache::open(&path).await.unwrap();
    let blob = BlobCache::paired_with(&etag);
    assert!(blob.get("deadbeef").await.is_none(), "miss before put");
    blob.put("deadbeef", b"hello world").await.unwrap();
    let got = blob.get("deadbeef").await.unwrap();
    assert_eq!(got, b"hello world");
}

#[tokio::test]
async fn persistent_persists_across_reopen() {
    let dir = tempdir();
    let path = dir.join("cache.db");

    // First session: write and drop.
    {
        let etag = EtagCache::open(&path).await.unwrap();
        let blob = BlobCache::paired_with(&etag);
        blob.put("abc", b"first session").await.unwrap();
    }

    // Second session: read what the first wrote.
    {
        let etag = EtagCache::open(&path).await.unwrap();
        let blob = BlobCache::paired_with(&etag);
        let got = blob.get("abc").await.unwrap();
        assert_eq!(got, b"first session");
    }
}

#[tokio::test]
async fn persistent_blob_and_etag_share_the_same_db_file() {
    let dir = tempdir();
    let path = dir.join("cache.db");
    let etag = EtagCache::open(&path).await.unwrap();
    let blob = BlobCache::paired_with(&etag);

    etag.put("https://example.test/x", "etag-1", b"etag-body")
        .await
        .unwrap();
    blob.put("blob-sha-1", b"blob-body").await.unwrap();

    // Both reads succeed against the same file.
    assert_eq!(
        etag.get("https://example.test/x").await.unwrap().body,
        b"etag-body"
    );
    assert_eq!(blob.get("blob-sha-1").await.unwrap(), b"blob-body");
}
