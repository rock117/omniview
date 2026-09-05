use serde::{Deserialize, Serialize};

/// Top-level app settings (extensible). Persist as JSON under config dir.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    pub refresh: RefreshSettings,
    // Future: theme, columns, proxy ports, …
}

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

/// Preset intervals shown in Settings UI.
pub const REFRESH_PRESETS_MS: &[u64] = &[500, 1000, 2000, 5000, 10_000];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MainPane {
    #[default]
    Processes,
    Ports,
    Files,
    Dns,
    Proxy,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessViewMode {
    #[default]
    List,
    Tree,
}
