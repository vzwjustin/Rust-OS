//! GPollableUtils matching `gio/gpollableutils.h`.
//! Utility functions for pollable streams. In this no_std port we model
//! poll condition checking helpers.
//! Fully `no_std` compatible.

/// Poll condition flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollableCondition(pub u32);

impl PollableCondition {
    pub const NONE: Self = Self(0);
    pub const IN: Self = Self(1);
    pub const OUT: Self = Self(2);
    pub const PRI: Self = Self(4);
    pub const ERR: Self = Self(8);
    pub const HUP: Self = Self(16);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Checks if a poll condition indicates readable data.
pub fn is_readable(cond: PollableCondition) -> bool {
    cond.contains(PollableCondition::IN)
}

/// Checks if a poll condition indicates writable state.
pub fn is_writable(cond: PollableCondition) -> bool {
    cond.contains(PollableCondition::OUT)
}

/// Checks if a poll condition indicates an error or hangup.
pub fn is_closed(cond: PollableCondition) -> bool {
    cond.contains(PollableCondition::ERR) || cond.contains(PollableCondition::HUP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conditions() {
        assert!(is_readable(PollableCondition::IN));
        assert!(is_writable(PollableCondition::OUT));
        assert!(is_closed(PollableCondition::ERR));
        assert!(is_closed(PollableCondition::HUP));
        assert!(!is_closed(PollableCondition::IN));
    }
}
