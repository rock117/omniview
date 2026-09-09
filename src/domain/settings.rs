use serde::{Deserialize, Serialize};

/// Top-level app settings (extensible). Persist as JSON under config dir.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    pub refresh: RefreshSettings,
    #[serde(default)]
    pub process_columns: ProcessColumnWidths,
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

/// Process table column widths (path remains flex).
/// Defaults match `theme::COL_*`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProcessColumnWidths {
    pub name: f32,
    pub pid: f32,
    pub cpu: f32,
    pub mem: f32,
    pub net: f32,
}

impl Default for ProcessColumnWidths {
    fn default() -> Self {
        Self {
            name: 220.0,
            pid: 72.0,
            cpu: 80.0,
            mem: 96.0,
            net: 200.0,
        }
    }
}

impl ProcessColumnWidths {
    pub const MIN_NAME: f32 = 120.0;
    pub const MIN_PID: f32 = 48.0;
    pub const MIN_CPU: f32 = 56.0;
    pub const MIN_MEM: f32 = 64.0;
    pub const MIN_NET: f32 = 80.0;
    pub const MAX: f32 = 600.0;

    pub fn clamp_all(mut self) -> Self {
        self.name = self.name.clamp(Self::MIN_NAME, Self::MAX);
        self.pid = self.pid.clamp(Self::MIN_PID, Self::MAX);
        self.cpu = self.cpu.clamp(Self::MIN_CPU, Self::MAX);
        self.mem = self.mem.clamp(Self::MIN_MEM, Self::MAX);
        self.net = self.net.clamp(Self::MIN_NET, Self::MAX);
        self
    }

    pub fn get(self, col: ProcessCol) -> f32 {
        match col {
            ProcessCol::Name => self.name,
            ProcessCol::Pid => self.pid,
            ProcessCol::Cpu => self.cpu,
            ProcessCol::Mem => self.mem,
            ProcessCol::Net => self.net,
        }
    }

    pub fn set(&mut self, col: ProcessCol, width: f32) {
        let w = match col {
            ProcessCol::Name => width.clamp(Self::MIN_NAME, Self::MAX),
            ProcessCol::Pid => width.clamp(Self::MIN_PID, Self::MAX),
            ProcessCol::Cpu => width.clamp(Self::MIN_CPU, Self::MAX),
            ProcessCol::Mem => width.clamp(Self::MIN_MEM, Self::MAX),
            ProcessCol::Net => width.clamp(Self::MIN_NET, Self::MAX),
        };
        match col {
            ProcessCol::Name => self.name = w,
            ProcessCol::Pid => self.pid = w,
            ProcessCol::Cpu => self.cpu = w,
            ProcessCol::Mem => self.mem = w,
            ProcessCol::Net => self.net = w,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessCol {
    Name,
    Pid,
    Cpu,
    Mem,
    Net,
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
