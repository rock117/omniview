//! Background sampling and snapshot store (UI must not call probes on the frame path).

mod store;

pub use store::*;

use crate::domain::{ProcessInfo, SocketRow};
use crate::platform::PlatformServices;

#[derive(Debug, Clone, Default)]
pub struct SystemSnapshot {
    pub processes: Vec<ProcessInfo>,
    pub sockets: Vec<SocketRow>,
    pub process_error: Option<String>,
    pub socket_error: Option<String>,
    pub collected_at_ms: u128,
}

pub fn collect_snapshot(services: &PlatformServices) -> SystemSnapshot {
    let collected_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    let (processes, process_error) = match services.processes.list_processes() {
        Ok(p) => (p, None),
        Err(e) => (Vec::new(), Some(e.to_string())),
    };
    let (sockets, socket_error) = match services.net.list_sockets() {
        Ok(s) => (s, None),
        Err(e) => (Vec::new(), Some(e.to_string())),
    };

    SystemSnapshot {
        processes,
        sockets,
        process_error,
        socket_error,
        collected_at_ms,
    }
}
