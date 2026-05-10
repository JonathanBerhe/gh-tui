//! Async command workers. A reducer emits `Cmd`s; `dispatch` turns each one
//! into a task that runs the side effect and posts a `Msg` back on the mpsc
//! channel. One match arm per `Cmd` variant; future commands add branches.

use std::sync::Arc;

use gh_api::auth::{detect_auth, AuthOutcome};
use gh_api::{
    fetch_open_prs_page, fetch_pr_detail, fetch_pr_files, fetch_pr_review_threads,
    resolve_from_cwd, Client, EtagCache,
};
use gh_core::{Cmd, Msg};
use gh_ui::ImageCache;
use ratatui_image::picker::Picker;
use tokio::sync::{mpsc::Sender, OnceCell};
use tracing::{debug, warn};

/// Shared context every worker needs.
///
/// `client` is filled by the auth worker once a token is in hand. Subsequent
/// fetch workers read it; if not yet set, they post `PrListFailed("auth not
/// ready")` rather than panicking. The reducer also gates fetch dispatch on
/// `state.auth.is_authenticated()`, so this fallback is belt-and-braces.
///
/// `cache` is the ETag cache built at startup, shared across the lifetime of
/// the binary. Persistent (SQLite) when the cache directory is writable,
/// else falls back to in-memory.
#[derive(Clone)]
pub struct AppCtx {
    pub tx: Sender<Msg>,
    pub client: Arc<OnceCell<Client>>,
    pub cache: Arc<EtagCache>,
    /// Terminal image-protocol picker, populated at startup before the
    /// alt-screen is entered. `None` means we render placeholder text
    /// for image chunks (no Kitty/iTerm2/Sixel/halfblock support).
    pub picker: Arc<Option<Picker>>,
    /// URL → decoded `StatefulProtocol` cache, shared between the image
    /// fetch worker and the UI render loop. Mermaid renders share this
    /// cache too, keyed by `gh_render::mermaid_hash` of the source.
    pub images: ImageCache,
    /// `true` when `mmdc` is on the PATH; checked once at startup. The
    /// `RenderMermaid` worker reads this flag and either shells out or
    /// short-circuits to a `Failed` cache entry.
    pub mmdc_available: bool,
}

impl AppCtx {
    pub fn new(
        tx: Sender<Msg>,
        cache: Arc<EtagCache>,
        picker: Option<Picker>,
        mmdc_available: bool,
    ) -> Self {
        Self {
            tx,
            client: Arc::new(OnceCell::new()),
            cache,
            picker: Arc::new(picker),
            images: ImageCache::new(),
            mmdc_available,
        }
    }
}

pub fn dispatch(cmd: Cmd, ctx: AppCtx) {
    match cmd {
        Cmd::AuthenticateFromGh => {
            tokio::spawn(async move {
                let outcome = detect_auth().await;
                let msg = match outcome {
                    AuthOutcome::Token { token, host, user } => {
                        match Client::new(&token, &host, ctx.cache.clone()) {
                            Ok(client) => {
                                let client = client.with_tx(ctx.tx.clone());
                                // OnceCell::set returns Err if already set;
                                // a second auth attempt would land here.
                                // Today we only auth once at startup.
                                if ctx.client.set(client).is_err() {
                                    warn!("client already initialised; ignoring re-auth");
                                }
                                Msg::AuthReady { host, user }
                            }
                            Err(e) => Msg::AuthMissing {
                                reason: format!("client init failed: {e}"),
                            },
                        }
                    }
                    AuthOutcome::Missing { reason } => Msg::AuthMissing { reason },
                };
                let _ = ctx.tx.send(msg).await;
            });
        }
        Cmd::ResolveRepoFromCwd => {
            tokio::spawn(async move {
                let msg = match resolve_from_cwd().await {
                    Ok(repo) => {
                        debug!(slug = %repo.slug(), "repo resolved from cwd");
                        Msg::RepoResolved(repo)
                    }
                    Err(e) => Msg::RepoResolveFailed(e.to_string()),
                };
                let _ = ctx.tx.send(msg).await;
            });
        }
        Cmd::FetchPrPage { repo, page } => {
            tokio::spawn(async move {
                let Some(client) = ctx.client.get() else {
                    let _ = ctx
                        .tx
                        .send(Msg::PrListFailed("auth not ready".to_string()))
                        .await;
                    return;
                };
                let msg = match fetch_open_prs_page(client, &repo, page).await {
                    Ok(p) => Msg::PrPageReady {
                        repo,
                        page: p.page,
                        items: p.items,
                        has_more: p.has_more,
                    },
                    Err(e) => Msg::PrListFailed(e.to_string()),
                };
                let _ = ctx.tx.send(msg).await;
            });
        }
        Cmd::FetchPrDetail { repo, number } => {
            tokio::spawn(async move {
                let Some(client) = ctx.client.get() else {
                    let _ = ctx
                        .tx
                        .send(Msg::PrDetailFailed("auth not ready".to_string()))
                        .await;
                    return;
                };
                let msg = match fetch_pr_detail(client, &repo, number).await {
                    Ok(detail) => {
                        let chunks = gh_render::render_markdown_chunks(&detail.body);
                        // Sum chunk heights so body_lines reflects the
                        // chunked stack layout the UI actually renders.
                        let body_lines: u16 = chunks
                            .iter()
                            .map(|c| u32::from(c.height()))
                            .sum::<u32>()
                            .try_into()
                            .unwrap_or(u16::MAX);
                        // Pre-extract image URLs and mermaid blocks so
                        // the reducer can fan out fetch / render
                        // commands; both caches will be warm by the time
                        // the body renders.
                        let image_urls: Vec<String> = chunks
                            .iter()
                            .filter_map(|c| match c {
                                gh_render::BodyChunk::Image { url, .. } => Some(url.clone()),
                                _ => None,
                            })
                            .collect();
                        let mermaid_blocks = gh_render::markdown_mermaid_blocks(&detail.body);
                        Msg::PrDetailReady {
                            detail,
                            body_lines,
                            image_urls,
                            mermaid_blocks,
                        }
                    }
                    Err(e) => Msg::PrDetailFailed(e.to_string()),
                };
                let _ = ctx.tx.send(msg).await;
            });
        }
        Cmd::FetchImage { url } => {
            tokio::spawn(async move {
                // De-dupe via the cache: the first call wins, subsequent
                // calls for the same URL bail early.
                if !ctx.images.try_begin(&url) {
                    return;
                }
                let Some(picker) = ctx.picker.as_ref() else {
                    // No image protocol in this terminal; cache the
                    // failure so the renderer falls through to text.
                    ctx.images.set_failed(&url, "no image protocol".into());
                    return;
                };
                let bytes = match reqwest::get(&url).await {
                    Ok(resp) => match resp.bytes().await {
                        Ok(b) => b,
                        Err(e) => {
                            warn!(error = %e, %url, "image body read failed");
                            ctx.images.set_failed(&url, e.to_string());
                            return;
                        }
                    },
                    Err(e) => {
                        warn!(error = %e, %url, "image fetch failed");
                        ctx.images.set_failed(&url, e.to_string());
                        return;
                    }
                };
                // Decoding is CPU-bound; offload to a blocking task so we
                // don't stall the runtime on a big PNG.
                let url_for_decode = url.clone();
                let bytes_for_decode = bytes.to_vec();
                let decoded =
                    tokio::task::spawn_blocking(move || image::load_from_memory(&bytes_for_decode))
                        .await;
                let dyn_img = match decoded {
                    Ok(Ok(img)) => img,
                    Ok(Err(e)) => {
                        warn!(error = %e, url = %url_for_decode, "image decode failed");
                        ctx.images.set_failed(&url, e.to_string());
                        return;
                    }
                    Err(e) => {
                        warn!(error = %e, url = %url_for_decode, "image decode panicked");
                        ctx.images.set_failed(&url, e.to_string());
                        return;
                    }
                };
                let protocol = picker.new_resize_protocol(dyn_img);
                ctx.images.set_ready(&url, protocol);
                debug!(%url, "image ready");
                let _ = ctx.tx.send(Msg::ImageReady { url }).await;
            });
        }
        Cmd::RenderMermaid { hash, source } => {
            tokio::spawn(async move {
                // Dedupe via the shared image cache: same hash → only one
                // mmdc invocation, even across re-renders.
                if !ctx.images.try_begin(&hash) {
                    return;
                }
                if !ctx.mmdc_available {
                    ctx.images.set_failed(&hash, "mmdc not installed".into());
                    let _ = ctx.tx.send(Msg::ImageReady { url: hash }).await;
                    return;
                }
                let Some(picker) = ctx.picker.as_ref() else {
                    ctx.images.set_failed(&hash, "no image protocol".into());
                    let _ = ctx.tx.send(Msg::ImageReady { url: hash }).await;
                    return;
                };
                let bytes = match crate::mmdc::render_to_png(&hash, &source).await {
                    Ok(b) => b,
                    Err(e) => {
                        warn!(error = %e, %hash, "mermaid render failed");
                        ctx.images.set_failed(&hash, e);
                        let _ = ctx.tx.send(Msg::ImageReady { url: hash }).await;
                        return;
                    }
                };
                // Decoding is CPU-bound; offload to a blocking task so
                // we don't stall the runtime on a big PNG.
                let bytes_for_decode = bytes.clone();
                let decoded =
                    tokio::task::spawn_blocking(move || image::load_from_memory(&bytes_for_decode))
                        .await;
                let dyn_img = match decoded {
                    Ok(Ok(img)) => img,
                    Ok(Err(e)) => {
                        warn!(error = %e, %hash, "mermaid PNG decode failed");
                        ctx.images.set_failed(&hash, e.to_string());
                        let _ = ctx.tx.send(Msg::ImageReady { url: hash }).await;
                        return;
                    }
                    Err(e) => {
                        warn!(error = %e, %hash, "mermaid PNG decode panicked");
                        ctx.images.set_failed(&hash, e.to_string());
                        let _ = ctx.tx.send(Msg::ImageReady { url: hash }).await;
                        return;
                    }
                };
                let protocol = picker.new_resize_protocol(dyn_img);
                ctx.images.set_ready(&hash, protocol);
                debug!(%hash, "mermaid ready");
                let _ = ctx.tx.send(Msg::ImageReady { url: hash }).await;
            });
        }
        Cmd::FetchPrDiff { repo, number } => {
            tokio::spawn(async move {
                let Some(client) = ctx.client.get() else {
                    let _ = ctx
                        .tx
                        .send(Msg::DiffFailed("auth not ready".to_string()))
                        .await;
                    return;
                };
                // Fetch files (REST) and review threads (GraphQL) in parallel.
                // If files fail, surface the error; thread failure is degraded
                // (we still show the diff, just without inline comments).
                let files_fut = fetch_pr_files(client, &repo, number);
                let threads_fut = fetch_pr_review_threads(client, &repo, number);
                let (files_res, threads_res) = tokio::join!(files_fut, threads_fut);
                let msg = match files_res {
                    Ok(files) => {
                        let threads = threads_res.unwrap_or_else(|e| {
                            warn!(error = %e, "review threads fetch failed; rendering diff without inline comments");
                            Vec::new()
                        });
                        let (file_offsets, total_lines) =
                            gh_render::diff::file_line_layout(&files, &threads);
                        Msg::DiffReady {
                            repo,
                            number,
                            files,
                            threads,
                            file_offsets,
                            total_lines,
                        }
                    }
                    Err(e) => Msg::DiffFailed(e.to_string()),
                };
                let _ = ctx.tx.send(msg).await;
            });
        }
    }
}
