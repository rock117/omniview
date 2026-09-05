//! Platform probes. Windows is implemented; other OSes return [`ProbeError::Unsupported`].

use std::path::Path;
use std::sync::Arc;

use crate::domain::{
    DnsCacheEntry, EnvProxyInfo, OpenFile, PathHolder, Pid, ProbeError, ProcessInfo, SocketRow,
    SystemProxyInfo, WinHttpProxyInfo,
};

mod unsupported;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

pub trait ProcessProbe: Send + Sync {
    fn list_processes(&self) -> Result<Vec<ProcessInfo>, ProbeError>;
    fn kill(&self, pid: Pid, tree: bool) -> Result<(), ProbeError>;
}

pub trait NetProbe: Send + Sync {
    fn list_sockets(&self) -> Result<Vec<SocketRow>, ProbeError>;
}

pub trait HandleProbe: Send + Sync {
    fn open_files(&self, pid: Pid) -> Result<Vec<OpenFile>, ProbeError>;
    fn holders_of_path(&self, path: &Path) -> Result<Vec<PathHolder>, ProbeError>;
}

pub trait DnsCacheProbe: Send + Sync {
    fn list_cache(&self) -> Result<Vec<DnsCacheEntry>, ProbeError>;
    fn flush_cache(&self) -> Result<(), ProbeError>;
    /// Remove cache entries for a hostname when supported.
    fn remove_entry(&self, name: &str) -> Result<(), ProbeError>;
    fn supports_remove_entry(&self) -> bool {
        false
    }
}

pub trait ProxyProbe: Send + Sync {
    fn system_proxy(&self) -> Result<SystemProxyInfo, ProbeError>;
    fn winhttp_proxy(&self) -> Result<WinHttpProxyInfo, ProbeError>;
    fn env_proxy(&self) -> Result<EnvProxyInfo, ProbeError>;
    fn disable_system_proxy(&self) -> Result<(), ProbeError>;
    fn reset_winhttp_proxy(&self) -> Result<(), ProbeError>;
}

#[derive(Clone)]
pub struct PlatformServices {
    pub processes: Arc<dyn ProcessProbe>,
    pub net: Arc<dyn NetProbe>,
    pub handles: Arc<dyn HandleProbe>,
    pub dns: Arc<dyn DnsCacheProbe>,
    pub proxy: Arc<dyn ProxyProbe>,
}

pub fn services() -> PlatformServices {
    #[cfg(windows)]
    {
        windows::services()
    }
    #[cfg(target_os = "macos")]
    {
        macos::services()
    }
    #[cfg(target_os = "linux")]
    {
        linux::services()
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        compile_error!("Omniview supports Windows, macOS, and Linux only");
    }
}

pub fn config_dir() -> std::path::PathBuf {
    #[cfg(windows)]
    {
        dirs_config_windows()
    }
    #[cfg(target_os = "macos")]
    {
        dirs_home().join("Library/Application Support/Omniview")
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| dirs_home().join(".config"))
            .join("omniview")
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        std::path::PathBuf::from(".")
    }
}

#[cfg(windows)]
fn dirs_config_windows() -> std::path::PathBuf {
    std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| dirs_home().join("AppData/Roaming"))
        .join("Omniview")
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn dirs_home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

#[cfg(windows)]
fn dirs_home() -> std::path::PathBuf {
    std::env::var_os("USERPROFILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}
