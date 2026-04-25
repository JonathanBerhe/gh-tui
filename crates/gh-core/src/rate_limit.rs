//! Primary rate-limit window reported by GitHub's API.
//!
//! Populated from the `x-ratelimit-{limit,remaining,reset}` response headers
//! by `gh_api::Client`; rendered in the status bar by `gh_ui`.

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    pub remaining: u32,
    pub limit: u32,
    pub reset_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Plenty of budget; render in muted colour.
    Healthy,
    /// Getting tight; render in yellow.
    Warning,
    /// Almost out; render in red.
    Critical,
}

impl RateLimit {
    /// Severity bucket based on `remaining`. Thresholds match the values
    /// the status bar uses to flip colours: `>100` healthy, `20..=100`
    /// warning, `<20` critical.
    #[must_use]
    pub fn tier(&self) -> Tier {
        match self.remaining {
            0..=19 => Tier::Critical,
            20..=100 => Tier::Warning,
            _ => Tier::Healthy,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn rl(remaining: u32) -> RateLimit {
        RateLimit {
            remaining,
            limit: 5000,
            reset_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn tier_thresholds() {
        assert_eq!(rl(0).tier(), Tier::Critical);
        assert_eq!(rl(19).tier(), Tier::Critical);
        assert_eq!(rl(20).tier(), Tier::Warning);
        assert_eq!(rl(100).tier(), Tier::Warning);
        assert_eq!(rl(101).tier(), Tier::Healthy);
        assert_eq!(rl(5000).tier(), Tier::Healthy);
    }
}
