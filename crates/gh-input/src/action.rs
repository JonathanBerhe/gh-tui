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
    /// Activate the current selection (Enter on a list row).
    Open,
    /// Open the diff sub-view from PR detail (Tab).
    OpenDiff,
    /// Flip the diff view between unified and split layouts (`s`).
    ToggleSplitView,
    /// Pop one screen off the nav stack (Backspace from a sub-screen).
    Back,
    /// Jump to the next/previous "section" within the current screen
    /// (review entry, diff hunk, etc.). `count` defaults to 1 when no
    /// pre-count was given.
    JumpSection { count: usize, direction: Direction },
    /// No effect. Returned for keys that aren't bound.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Next,
    Prev,
}
