//! Event loop driver: reads crossterm events, feeds them through `gh_input`
//! to get `Action`s, maps `Action → Msg`, runs `gh_core::reduce`, dispatches
//! commands, and redraws.

use std::sync::Arc;

use anyhow::Result;
use crossterm::event::{Event as CtEvent, EventStream};
use futures::StreamExt;
use gh_api::{cache_db_path, EtagCache};
use gh_core::{initial_commands, reduce, Cmd, JumpDirection, Msg, RepoRef, SelectionJump, State};
use gh_input::{Action, Direction, Motion, Resolution, Resolver};
use tokio::sync::mpsc;
use tracing::{debug, info_span, warn};

use crate::{
    terminal::Tui,
    workers::{self, AppCtx},
};

const CHANNEL_CAPACITY: usize = 256;

pub async fn run(mut terminal: Tui, repo_arg: Option<String>) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<Msg>(CHANNEL_CAPACITY);

    // Build the ETag cache. Persistent SQLite when the cache dir is
    // writable; falls back to an in-memory cache so the binary still
    // launches if the cache is corrupt or the FS is read-only.
    let cache = Arc::new(open_cache_or_fallback().await);
    let ctx = AppCtx::new(tx.clone(), cache);

    // Always kick off auth detection.
    for cmd in initial_commands() {
        workers::dispatch(cmd, ctx.clone());
    }

    // Then the repo path: argv-parse if present, else shell out.
    match repo_arg.as_deref() {
        Some(arg) => match RepoRef::parse(arg) {
            Ok(repo) => {
                let _ = tx.send(Msg::RepoResolved(repo)).await;
            }
            Err(e) => {
                let _ = tx.send(Msg::RepoResolveFailed(e.to_string())).await;
            }
        },
        None => workers::dispatch(Cmd::ResolveRepoFromCwd, ctx.clone()),
    }

    tokio::spawn(input_task(tx.clone()));
    tokio::spawn(ctrl_c_task(tx.clone()));

    let mut state = State::default();
    terminal.draw(|f| gh_ui::draw(&state, f))?;

    while let Some(msg) = rx.recv().await {
        let span = info_span!("reduce");
        let _enter = span.enter();
        debug!(?msg, "dispatching");
        let (new_state, cmds) = reduce(state, msg);
        state = new_state;
        drop(_enter);

        for cmd in cmds {
            workers::dispatch(cmd, ctx.clone());
        }

        terminal.draw(|f| gh_ui::draw(&state, f))?;

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

async fn input_task(tx: mpsc::Sender<Msg>) {
    let mut events = EventStream::new();
    let mut resolver = Resolver::new();
    let mut last_pending = String::new();
    while let Some(Ok(event)) = events.next().await {
        let CtEvent::Key(key) = event else {
            continue;
        };

        let resolution = resolver.feed(key);

        // Emit pending-buffer updates only when the display changes.
        let cur_pending = resolver.pending_display();
        if cur_pending != last_pending {
            if tx
                .send(Msg::PendingChanged(cur_pending.clone()))
                .await
                .is_err()
            {
                break;
            }
            last_pending = cur_pending;
        }

        match resolution {
            Resolution::Action(action) => {
                if let Some(msg) = action_to_msg(action) {
                    if tx.send(msg).await.is_err() {
                        break;
                    }
                }
            }
            Resolution::Pending | Resolution::Cancel => {}
        }
    }
}

/// Map a resolver Action to a domain Msg, where applicable. Selection
/// motions only make sense when the active screen is a list — but the
/// reducer is the canonical place to decide what to do (it inspects
/// `state.screen`), so we forward the intent unconditionally.
fn action_to_msg(action: Action) -> Option<Msg> {
    match action {
        Action::Quit => Some(Msg::Quit),
        Action::Open => Some(Msg::OpenSelectedPr),
        Action::OpenDiff => Some(Msg::OpenDiff),
        Action::ToggleSplitView => Some(Msg::ToggleDiffViewMode),
        Action::Back => Some(Msg::Back),
        Action::JumpSection { count, direction } => Some(Msg::SectionJump {
            count,
            direction: match direction {
                Direction::Next => JumpDirection::Next,
                Direction::Prev => JumpDirection::Prev,
            },
        }),
        Action::None => None,
        Action::Move { count, motion } => match motion {
            Motion::Down => Some(Msg::SelectionDelta(
                i32::try_from(count).unwrap_or(i32::MAX),
            )),
            Motion::Up => Some(Msg::SelectionDelta(
                i32::try_from(count).unwrap_or(i32::MAX).saturating_neg(),
            )),
            Motion::DocStart => Some(Msg::SelectionJump(SelectionJump::First)),
            Motion::DocEnd => Some(Msg::SelectionJump(SelectionJump::Last)),
            // Repurpose horizontal motions for nav-stack movement: there's
            // no horizontal cursor concept in this app, so `l`/`h` give us
            // a vim-spatial "into / back". If we ever add text editing or
            // visual mode, revisit.
            Motion::Right => Some(Msg::OpenSelectedPr),
            Motion::Left => Some(Msg::Back),
            // w/b/$/0 still unbound — quietly ignore.
            _ => None,
        },
        Action::Operate { .. } => None,
    }
}

async fn ctrl_c_task(tx: mpsc::Sender<Msg>) {
    if tokio::signal::ctrl_c().await.is_ok() {
        let _ = tx.send(Msg::Quit).await;
    }
}

async fn open_cache_or_fallback() -> EtagCache {
    let Some(path) = cache_db_path() else {
        warn!("no cache directory available; using in-memory cache");
        return EtagCache::in_memory();
    };
    match EtagCache::open(&path).await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, path = %path.display(), "cache open failed; falling back to in-memory");
            EtagCache::in_memory()
        }
    }
}
