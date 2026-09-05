use std::path::Path;
use std::sync::Arc;

use crate::domain::{
    DnsCacheEntry, EnvProxyInfo, OpenFile, PathHolder, Pid, ProbeError, ProcessInfo, SocketRow,
    SystemProxyInfo, WinHttpProxyInfo,
};
use crate::platform::{DnsCacheProbe, HandleProbe, NetProbe, ProcessProbe, ProxyProbe};

pub struct UnsupportedProcess {
    pub capability: &'static str,
}

impl ProcessProbe for UnsupportedProcess {
    fn list_processes(&self) -> Result<Vec<ProcessInfo>, ProbeError> {
        Err(ProbeError::unsupported(self.capability))
    }

    fn kill(&self, _pid: Pid, _tree: bool) -> Result<(), ProbeError> {
        Err(ProbeError::unsupported(self.capability))
    }
}

pub struct UnsupportedNet {
    pub capability: &'static str,
}

impl NetProbe for UnsupportedNet {
    fn list_sockets(&self) -> Result<Vec<SocketRow>, ProbeError> {
        Err(ProbeError::unsupported(self.capability))
    }
}

pub struct UnsupportedHandle {
    pub capability: &'static str,
}

impl HandleProbe for UnsupportedHandle {
    fn open_files(&self, _pid: Pid) -> Result<Vec<OpenFile>, ProbeError> {
        Err(ProbeError::unsupported(self.capability))
    }

    fn holders_of_path(&self, _path: &Path) -> Result<Vec<PathHolder>, ProbeError> {
        Err(ProbeError::unsupported(self.capability))
    }
}

pub struct UnsupportedDns {
    pub capability: &'static str,
}

impl DnsCacheProbe for UnsupportedDns {
    fn list_cache(&self) -> Result<Vec<DnsCacheEntry>, ProbeError> {
        Err(ProbeError::unsupported(self.capability))
    }

    fn flush_cache(&self) -> Result<(), ProbeError> {
        Err(ProbeError::unsupported(self.capability))
    }

    fn remove_entry(&self, _name: &str) -> Result<(), ProbeError> {
        Err(ProbeError::unsupported(self.capability))
    }
}

pub struct UnsupportedProxy {
    pub capability: &'static str,
}

impl ProxyProbe for UnsupportedProxy {
    fn system_proxy(&self) -> Result<SystemProxyInfo, ProbeError> {
        Err(ProbeError::unsupported(self.capability))
    }

    fn winhttp_proxy(&self) -> Result<WinHttpProxyInfo, ProbeError> {
        Err(ProbeError::unsupported(self.capability))
    }

    fn env_proxy(&self) -> Result<EnvProxyInfo, ProbeError> {
        Ok(read_env_proxy())
    }

    fn disable_system_proxy(&self) -> Result<(), ProbeError> {
        Err(ProbeError::unsupported(self.capability))
    }

    fn reset_winhttp_proxy(&self) -> Result<(), ProbeError> {
        Err(ProbeError::unsupported(self.capability))
    }
}

pub fn read_env_proxy() -> EnvProxyInfo {
    const KEYS: &[&str] = &[
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
    ];
    let mut entries = Vec::new();
    for key in KEYS {
        if let Ok(val) = std::env::var(key) {
            if !val.is_empty() {
                entries.push(((*key).to_string(), val));
            }
        }
    }
    EnvProxyInfo { entries }
}

pub fn stub_handles() -> Arc<dyn HandleProbe> {
    Arc::new(UnsupportedHandle {
        capability: "file handle enumeration",
    })
}
