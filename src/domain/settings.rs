use serde::{Deserialize, Serialize};

/// Auto-refresh policy shared across views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshSettings {
    /// When false, only manual refresh (F5 / button).
    pub auto: bool,
    /// Interval in milliseconds when `auto` is true. Clamped to 500..=10_000.
    pub interval_ms: u64,
}

impl Default for RefreshSettings {
    fn default() -> Self {
        Self {
            auto: true,
            interval_ms: 1000,
        }
    }
}

impl RefreshSettings {
    pub fn clamp_interval(ms: u64) -> u64 {
        ms.clamp(500, 10_000)
    }

    pub fn with_interval(mut self, ms: u64) -> Self {
        self.interval_ms = Self::clamp_interval(ms);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MainPane {
    #[default]
    Processes,
    Ports,
    Files,
    Dns,
    Proxy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessViewMode {
    #[default]
    List,
    Tree,
}
