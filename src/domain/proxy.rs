use crate::domain::Pid;

/// Well-known local proxy listen ports for highlighting / health checks.
pub const COMMON_PROXY_PORTS: &[u16] = &[
    7890, 7891, 7897, 1080, 10808, 10809, 8080, 8888, 20171, 6152, 6153, 9050, 9060, 8118, 3128,
    2080, 2087, 9090, 9091,
];

pub fn is_common_proxy_port(port: u16) -> bool {
    COMMON_PROXY_PORTS.contains(&port)
}

#[derive(Debug, Clone, Default)]
pub struct SystemProxyInfo {
    pub enabled: bool,
    pub server: String,
    pub override_list: String,
}

#[derive(Debug, Clone, Default)]
pub struct WinHttpProxyInfo {
    pub available: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Default)]
pub struct EnvProxyInfo {
    pub entries: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthLevel {
    Ok,
    Warn,
    Bad,
}

#[derive(Debug, Clone)]
pub struct HealthFinding {
    pub level: HealthLevel,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct ProxyHealthSnapshot {
    pub system: SystemProxyInfo,
    pub winhttp: WinHttpProxyInfo,
    pub env: EnvProxyInfo,
    pub findings: Vec<HealthFinding>,
    pub error: Option<String>,
}

/// Build findings from proxy state + currently listening local ports.
pub fn build_proxy_findings(
    system: &SystemProxyInfo,
    winhttp: &WinHttpProxyInfo,
    env: &EnvProxyInfo,
    listening_local_ports: &[(u16, Pid, String)],
) -> Vec<HealthFinding> {
    let mut findings = Vec::new();
    let listening: std::collections::HashSet<u16> =
        listening_local_ports.iter().map(|(p, _, _)| *p).collect();

    if system.enabled {
        if let Some(port) = parse_local_proxy_port(&system.server) {
            if !listening.contains(&port) {
                findings.push(HealthFinding {
                    level: HealthLevel::Bad,
                    title: "系统代理指向的本地端口无进程监听".into(),
                    detail: format!(
                        "系统代理已开启 → {}，但本机 :{port} 无监听。代理退出后残留的典型症状。",
                        system.server
                    ),
                });
            } else {
                findings.push(HealthFinding {
                    level: HealthLevel::Ok,
                    title: "系统代理与本地监听一致".into(),
                    detail: format!("系统代理 {}，端口 {port} 有进程在听。", system.server),
                });
            }
        } else if !system.server.is_empty() {
            findings.push(HealthFinding {
                level: HealthLevel::Warn,
                title: "系统代理已开启".into(),
                detail: format!("ProxyServer = {}", system.server),
            });
        }
    } else {
        findings.push(HealthFinding {
            level: HealthLevel::Ok,
            title: "系统代理已关闭".into(),
            detail: "Internet 设置中 ProxyEnable = 0".into(),
        });
    }

    if winhttp.available && !winhttp.summary.is_empty() && winhttp.summary != "direct" {
        findings.push(HealthFinding {
            level: HealthLevel::Warn,
            title: "WinHTTP 代理非直连".into(),
            detail: winhttp.summary.clone(),
        });
    }

    if !env.entries.is_empty() {
        let list = env
            .entries
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        findings.push(HealthFinding {
            level: HealthLevel::Warn,
            title: "检测到代理相关环境变量".into(),
            detail: list,
        });
    }

    for (port, pid, name) in listening_local_ports {
        if is_common_proxy_port(*port) {
            findings.push(HealthFinding {
                level: HealthLevel::Ok,
                title: format!("代理常见端口 :{port} 在监听"),
                detail: format!("{name} (PID {pid})"),
            });
        }
    }

    if findings.is_empty() {
        findings.push(HealthFinding {
            level: HealthLevel::Ok,
            title: "未发现明显代理残留".into(),
            detail: "系统代理关闭，且无告警项。".into(),
        });
    }
    findings
}

pub fn parse_local_proxy_port(server: &str) -> Option<u16> {
    for part in server.split([';', ' ', '\t']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let hostport = part.split('=').next_back().unwrap_or(part);
        if let Some((_, port_s)) = hostport.rsplit_once(':') {
            if let Ok(port) = port_s.parse::<u16>() {
                return Some(port);
            }
        }
    }
    None
}
