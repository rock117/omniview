use crate::domain::Pid;

/// Windows service hosted by a process (SCM), Task Manager expand-row style.
#[derive(Debug, Clone)]
pub struct HostedService {
    /// Service key name (e.g. `WinDefend`).
    pub name: String,
    /// Localized display name (e.g. `Microsoft Defender 防病毒服务`).
    pub display_name: String,
}

impl HostedService {
    pub fn label(&self) -> &str {
        let d = self.display_name.trim();
        if d.is_empty() {
            self.name.as_str()
        } else {
            d
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: Pid,
    pub parent_pid: Option<Pid>,
    /// Image / executable base name (e.g. `MsMpEng.exe`).
    pub name: String,
    /// Friendly name from PE `FileDescription` (Task Manager style), when available.
    pub display_name: Option<String>,
    /// Windows services hosted by this PID (may be empty / shared svchost).
    pub services: Vec<HostedService>,
    pub exe_path: Option<String>,
    pub cmd_line: Option<String>,
    pub user: Option<String>,
    /// CPU usage percent since last refresh (0–100+ for multi-core sum depending on backend).
    pub cpu_percent: f32,
    /// Resident / working-set style memory in bytes.
    pub memory_bytes: u64,
}

impl ProcessInfo {
    /// UI label: display name when present, otherwise image name.
    pub fn label(&self) -> &str {
        self.display_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(self.name.as_str())
    }

    pub fn has_services(&self) -> bool {
        !self.services.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessSortKey {
    Pid,
    Name,
    Cpu,
    Memory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

impl SortDir {
    pub fn toggle(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }
}

pub fn sort_processes(list: &mut [ProcessInfo], key: ProcessSortKey, dir: SortDir) {
    list.sort_by(|a, b| {
        let ord = match key {
            ProcessSortKey::Pid => a.pid.cmp(&b.pid),
            ProcessSortKey::Name => a
                .label()
                .to_lowercase()
                .cmp(&b.label().to_lowercase()),
            ProcessSortKey::Cpu => a
                .cpu_percent
                .partial_cmp(&b.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal),
            ProcessSortKey::Memory => a.memory_bytes.cmp(&b.memory_bytes),
        };
        match dir {
            SortDir::Asc => ord,
            SortDir::Desc => ord.reverse(),
        }
    });
}

pub fn filter_processes<'a>(
    list: &'a [ProcessInfo],
    query: &str,
) -> Vec<&'a ProcessInfo> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return list.iter().collect();
    }
    list.iter()
        .filter(|p| {
            p.name.to_lowercase().contains(&q)
                || p.display_name
                    .as_ref()
                    .is_some_and(|d| d.to_lowercase().contains(&q))
                || p.services.iter().any(|s| {
                    s.display_name.to_lowercase().contains(&q)
                        || s.name.to_lowercase().contains(&q)
                })
        })
        .collect()
}

/// Build adjacency for process tree: parent → children pids present in `by_pid`.
pub fn children_map(processes: &[ProcessInfo]) -> std::collections::HashMap<Pid, Vec<Pid>> {
    use std::collections::HashMap;
    let pids: std::collections::HashSet<Pid> = processes.iter().map(|p| p.pid).collect();
    let mut map: HashMap<Pid, Vec<Pid>> = HashMap::new();
    for p in processes {
        if let Some(ppid) = p.parent_pid {
            if pids.contains(&ppid) && ppid != p.pid {
                map.entry(ppid).or_default().push(p.pid);
            }
        }
    }
    for kids in map.values_mut() {
        kids.sort_unstable();
    }
    map
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}
