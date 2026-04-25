//! Messages dispatched to the reducer.
//!
//! `Msg` expresses **domain events**, not raw key presses. Keys are resolved
//! to `gh_input::Action` first; the binary maps `Action -> Msg`.

#[derive(Debug, Clone)]
pub enum Msg {
    Tick,
    AuthReady { host: String, user: Option<String> },
    AuthMissing { reason: String },
    Quit,
}
