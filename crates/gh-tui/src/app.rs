//! Event loop driver: reads crossterm events, feeds them through `gh_input`
//! to get `Action`s, maps `Action → Msg`, runs `gh_core::reduce`, dispatches
//! commands, and redraws.

use anyhow::Result;
use crossterm::event::{Event as CtEvent, EventStream};
use futures::StreamExt;
use gh_core::{initial_commands, reduce, Msg, State};
use gh_input::{Action, Resolution};
use tokio::sync::mpsc;
use tracing::{debug, info_span};

use crate::{terminal::Tui, workers};

const CHANNEL_CAPACITY: usize = 256;

pub async fn run(mut terminal: Tui) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<Msg>(CHANNEL_CAPACITY);

    for cmd in initial_commands() {
        workers::dispatch(cmd, tx.clone());
    }

    tokio::spawn(input_task(tx.clone()));
    tokio::spawn(ctrl_c_task(tx.clone()));

    let mut state = State::default();
    terminal.draw(|f| gh_ui::screens::normal::draw(&state, f))?;

    while let Some(msg) = rx.recv().await {
        let span = info_span!("reduce");
        let _enter = span.enter();
        debug!(?msg, "dispatching");
        let (new_state, cmds) = reduce(state, msg);
        state = new_state;
        drop(_enter);

        for cmd in cmds {
            workers::dispatch(cmd, tx.clone());
        }

        terminal.draw(|f| gh_ui::screens::normal::draw(&state, f))?;

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

async fn input_task(tx: mpsc::Sender<Msg>) {
    let mut events = EventStream::new();
    while let Some(Ok(event)) = events.next().await {
        if let CtEvent::Key(key) = event {
            match gh_input::resolve(key) {
                Resolution::Action(Action::Quit) => {
                    if tx.send(Msg::Quit).await.is_err() {
                        break;
                    }
                }
                Resolution::Action(Action::None) | Resolution::Pending | Resolution::Cancel => {}
            }
        }
    }
}

async fn ctrl_c_task(tx: mpsc::Sender<Msg>) {
    if tokio::signal::ctrl_c().await.is_ok() {
        let _ = tx.send(Msg::Quit).await;
    }
}
