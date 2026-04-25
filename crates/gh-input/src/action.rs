//! The action vocabulary the resolver emits.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operator {
    Delete,
    Yank,
    Change,
}

impl Operator {
    #[must_use]
    pub const fn key(self) -> char {
        match self {
            Self::Delete => 'd',
            Self::Yank => 'y',
            Self::Change => 'c',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Motion {
    Down,
    Up,
    Left,
    Right,
    WordForward,
    WordBackward,
    DocStart,
    /// Last line. With a count, the renderer should treat the count as a
    /// line number to jump to (matching vim's `<count>G`).
    DocEnd,
    LineStart,
    LineEnd,
    /// Used by linewise operators (`dd`, `yy`, `cc`).
    CurrentLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Quit the application.
    Quit,
    /// Bare motion (no operator).
    Move { count: usize, motion: Motion },
    /// Operator applied to a motion target. `count` is the product of the
    /// pre-operator count and the post-operator count (vim semantics:
    /// `2d3w` deletes 6 words).
    Operate {
        count: usize,
        op: Operator,
        motion: Motion,
    },
    /// No effect. Returned for keys that aren't bound.
    None,
}
