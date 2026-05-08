//! Image cache + capability picker shared by the UI render loop and the
//! background image-fetch worker.
//!
//! The picker is created once at startup (synchronously, from the
//! terminal) before entering the alt-screen — see `gh-tui::main::run`.
//! The protocol detection is one of: Kitty graphics → iTerm2 → Sixel →
//! Unicode half-block fallback. If `Picker::from_query_stdio` fails (for
//! example when stdio isn't a TTY) we fall through to placeholder text
//! and never render real images.
//!
//! The cache is a flat `HashMap<url, ImageState>` behind a
//! `std::sync::Mutex` so the synchronous render path can lock it without
//! `await`. Workers must not hold the lock across `.await`; use the
//! provided helpers, which scope the guard to a single critical section.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use ratatui_image::{picker::Picker, protocol::StatefulProtocol};

/// One slot in the image cache. `Loading` blocks future re-fetches for
/// the same URL until the worker finishes. `Ready` is boxed because
/// `StatefulProtocol` is significantly larger than the other variants —
/// without the indirection every cache entry would pay that overhead.
pub enum ImageState {
    Loading,
    Ready(Box<StatefulProtocol>),
    Failed(String),
}

/// Thread-safe URL → state map. Cloning is cheap (`Arc` clone).
#[derive(Clone, Default)]
pub struct ImageCache {
    inner: Arc<Mutex<HashMap<String, ImageState>>>,
}

impl ImageCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Run `f` against the slot for `url` if any. The lock scope is the
    /// duration of `f`; do NOT call `.await` inside the closure.
    pub fn with_state<F, R>(&self, url: &str, f: F) -> Option<R>
    where
        F: FnOnce(&mut ImageState) -> R,
    {
        let mut guard = self.inner.lock().ok()?;
        guard.get_mut(url).map(f)
    }

    /// Mark a URL as in flight. Returns `true` if the worker should
    /// proceed (i.e. no entry existed) or `false` if a previous call
    /// already kicked off a fetch.
    pub fn try_begin(&self, url: &str) -> bool {
        let Ok(mut guard) = self.inner.lock() else {
            return false;
        };
        if guard.contains_key(url) {
            return false;
        }
        guard.insert(url.to_string(), ImageState::Loading);
        true
    }

    pub fn set_ready(&self, url: &str, protocol: StatefulProtocol) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(url.to_string(), ImageState::Ready(Box::new(protocol)));
        }
    }

    pub fn set_failed(&self, url: &str, reason: String) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(url.to_string(), ImageState::Failed(reason));
        }
    }
}

/// Try to detect the terminal's image protocol. Called once at startup
/// **before** entering the alt-screen so the queries reach the host
/// terminal directly. Returns `None` when stdio isn't a TTY (e.g. when
/// piped) or the terminal answers with no support.
#[must_use]
pub fn detect_picker() -> Option<Picker> {
    match Picker::from_query_stdio() {
        Ok(p) => {
            tracing::info!(font_size = ?p.font_size(), "image picker ready");
            Some(p)
        }
        Err(e) => {
            tracing::warn!(error = %e, "image picker unavailable; falling back to placeholders");
            None
        }
    }
}
