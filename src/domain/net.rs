use std::net::SocketAddr;

use crate::domain::Pid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::Udp => "UDP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    Listen,
    Established,
    TimeWait,
    CloseWait,
    SynSent,
    SynRecv,
    FinWait1,
    FinWait2,
    Closing,
    LastAck,
    Closed,
    Other,
}

impl SocketState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Listen => "LISTEN",
            Self::Established => "ESTABLISHED",
            Self::TimeWait => "TIME_WAIT",
            Self::CloseWait => "CLOSE_WAIT",
            Self::SynSent => "SYN_SENT",
            Self::SynRecv => "SYN_RECV",
            Self::FinWait1 => "FIN_WAIT1",
            Self::FinWait2 => "FIN_WAIT2",
            Self::Closing => "CLOSING",
            Self::LastAck => "LAST_ACK",
            Self::Closed => "CLOSED",
            Self::Other => "OTHER",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SocketRow {
    pub protocol: Protocol,
    pub local: SocketAddr,
    pub remote: Option<SocketAddr>,
    pub state: SocketState,
    pub pid: Pid,
}

impl SocketRow {
    pub fn local_port(&self) -> u16 {
        self.local.port()
    }

    pub fn matches_port(&self, port: u16, proto: Option<Protocol>) -> bool {
        if self.local_port() != port {
            return false;
        }
        match proto {
            None => true,
            Some(p) => self.protocol == p,
        }
    }
}

pub fn filter_sockets<'a>(
    sockets: &'a [SocketRow],
    port_query: &str,
    process_query: &str,
    proto: Option<Protocol>,
    process_name: impl Fn(u32) -> Option<String>,
) -> Vec<&'a SocketRow> {
    let port_q = port_query.trim();
    let proc_q = process_query.trim().to_lowercase();
    let port_num: Option<u16> = port_q.parse().ok();

    sockets
        .iter()
        .filter(|s| {
            if let Some(p) = proto {
                if s.protocol != p {
                    return false;
                }
            }
            if let Some(p) = port_num {
                if s.local_port() != p && s.remote.map(|r| r.port()) != Some(p) {
                    return false;
                }
            } else if !port_q.is_empty() {
                let local = s.local.to_string();
                let remote = s.remote.map(|r| r.to_string()).unwrap_or_default();
                if !local.contains(port_q) && !remote.contains(port_q) {
                    return false;
                }
            }
            if !proc_q.is_empty() {
                let name = process_name(s.pid).unwrap_or_default().to_lowercase();
                let pid_s = s.pid.to_string();
                if !name.contains(&proc_q) && !pid_s.contains(&proc_q) {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// Single search box: port number, address fragment, or process name (OR).
pub fn filter_sockets_unified<'a>(
    sockets: &'a [SocketRow],
    query: &str,
    proto: Option<Protocol>,
    process_name: impl Fn(u32) -> Option<String>,
) -> Vec<&'a SocketRow> {
    let q = query.trim();
    if q.is_empty() {
        return filter_sockets(sockets, "", "", proto, process_name);
    }
    let q_lower = q.to_lowercase();
    let port_num: Option<u16> = q.parse().ok();
    sockets
        .iter()
        .filter(|s| {
            if let Some(p) = proto {
                if s.protocol != p {
                    return false;
                }
            }
            let name = process_name(s.pid).unwrap_or_default().to_lowercase();
            let local = s.local.to_string().to_lowercase();
            let remote = s
                .remote
                .map(|r| r.to_string().to_lowercase())
                .unwrap_or_default();
            let pid_s = s.pid.to_string();
            if let Some(p) = port_num {
                if s.local_port() == p || s.remote.map(|r| r.port()) == Some(p) {
                    return true;
                }
            }
            name.contains(&q_lower)
                || pid_s.contains(&q_lower)
                || local.contains(&q_lower)
                || remote.contains(&q_lower)
        })
        .collect()
}

/// Compact per-process network summaries for the process table.
///
/// Includes listen ports (TCP LISTEN + UDP binds) and ESTABLISHED remotes.
/// Each summary keeps at most `max_parts` segments, then `+N` for the rest.
pub fn process_net_summaries(
    sockets: &[SocketRow],
    max_parts: usize,
) -> std::collections::HashMap<Pid, String> {
    use std::collections::{HashMap, HashSet};

    let mut listen: HashMap<Pid, Vec<u16>> = HashMap::new();
    let mut est: HashMap<Pid, Vec<String>> = HashMap::new();
    let mut listen_seen: HashMap<Pid, HashSet<u16>> = HashMap::new();
    let mut est_seen: HashMap<Pid, HashSet<String>> = HashMap::new();

    for s in sockets {
        match s.state {
            SocketState::Listen => {
                let p = s.local_port();
                if p == 0 {
                    continue;
                }
                if listen_seen.entry(s.pid).or_default().insert(p) {
                    listen.entry(s.pid).or_default().push(p);
                }
            }
            SocketState::Established => {
                let Some(remote) = s.remote else {
                    continue;
                };
                if remote.port() == 0 {
                    continue;
                }
                let part = format!("{}→{}", s.local_port(), remote);
                if est_seen.entry(s.pid).or_default().insert(part.clone()) {
                    est.entry(s.pid).or_default().push(part);
                }
            }
            SocketState::Other if s.protocol == Protocol::Udp => {
                let p = s.local_port();
                if p == 0 {
                    continue;
                }
                if listen_seen.entry(s.pid).or_default().insert(p) {
                    listen.entry(s.pid).or_default().push(p);
                }
            }
            _ => {}
        }
    }

    let mut pids: HashSet<Pid> = listen.keys().copied().collect();
    pids.extend(est.keys().copied());

    let mut out = HashMap::with_capacity(pids.len());
    for pid in pids {
        let mut ports = listen.remove(&pid).unwrap_or_default();
        ports.sort_unstable();
        let mut conns = est.remove(&pid).unwrap_or_default();
        conns.sort();
        let mut parts: Vec<String> = ports
            .into_iter()
            .map(|p| p.to_string())
            .chain(conns)
            .collect();
        if parts.is_empty() {
            continue;
        }
        let summary = if max_parts > 0 && parts.len() > max_parts {
            let extra = parts.len() - max_parts;
            parts.truncate(max_parts);
            format!("{}  +{extra}", parts.join("  "))
        } else {
            parts.join("  ")
        };
        out.insert(pid, summary);
    }
    out
}
