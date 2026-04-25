//! Commands emitted by reducers; the binary's worker layer runs them.

#[derive(Debug, Clone)]
pub enum Cmd {
    AuthenticateFromGh,
}
