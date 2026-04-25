//! Resolve the on-disk cache DB location.

use std::path::PathBuf;

use directories::ProjectDirs;

/// `$XDG_CACHE_HOME/gh-tui/cache.db` (or platform equivalent). Returns
/// `None` when no project directories are available — caller falls back
/// to the in-memory cache.
#[must_use]
pub fn cache_db_path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("", "", "gh-tui")?;
    Some(dirs.cache_dir().join("cache.db"))
}
