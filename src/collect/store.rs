use std::sync::Arc;
use std::time::Duration;

use gpui::*;

use crate::collect::{collect_snapshot, SystemSnapshot};
use crate::domain::{
    build_proxy_findings, DnsCacheSnapshot, Pid, ProxyHealthSnapshot, RefreshSettings, SocketState,
};
use crate::platform::PlatformServices;

pub struct SnapshotStore {
    pub services: PlatformServices,
    pub snapshot: Arc<SystemSnapshot>,
    pub dns: DnsCacheSnapshot,
    pub proxy: ProxyHealthSnapshot,
    pub refresh: RefreshSettings,
    pub busy: bool,
    pub last_error: Option<String>,
    pub kill_confirm: Option<KillConfirm>,
    pub pending_action: Option<PendingAction>,
    generation: u64,
    timer_generation: u64,
}

#[derive(Debug, Clone)]
pub struct KillConfirm {
    pub pid: Pid,
    pub name: String,
    pub tree: bool,
}

#[derive(Debug, Clone)]
pub enum PendingAction {
    FlushDns,
    RemoveDns { name: String },
    DisableSystemProxy,
    ResetWinHttp,
}

pub enum SnapshotEvent {
    Updated,
}

impl EventEmitter<SnapshotEvent> for SnapshotStore {}

impl SnapshotStore {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let services = crate::platform::services();
        let snapshot = Arc::new(collect_snapshot(&services));
        let mut store = Self {
            services,
            snapshot,
            dns: DnsCacheSnapshot {
                supports_remove_entry: true,
                ..Default::default()
            },
            proxy: ProxyHealthSnapshot::default(),
            refresh: RefreshSettings::default(),
            busy: false,
            last_error: None,
            kill_confirm: None,
            pending_action: None,
            generation: 0,
            timer_generation: 0,
        };
        store.arm_timer(cx);
        store
    }

    pub fn set_auto(&mut self, auto: bool, cx: &mut Context<Self>) {
        self.refresh.auto = auto;
        self.arm_timer(cx);
        cx.notify();
    }

    pub fn set_interval_ms(&mut self, ms: u64, cx: &mut Context<Self>) {
        self.refresh.interval_ms = RefreshSettings::clamp_interval(ms);
        self.arm_timer(cx);
        cx.notify();
    }

    pub fn request_refresh(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        cx.notify();

        let services = self.services.clone();
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let (tx, rx) = flume::bounded::<SystemSnapshot>(1);
        std::thread::spawn(move || {
            let snap = collect_snapshot(&services);
            let _ = tx.send(snap);
        });

        cx.spawn(async move |this, cx| {
            let snap = match rx.recv_async().await {
                Ok(s) => s,
                Err(_) => return,
            };
            this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                this.snapshot = Arc::new(snap);
                this.busy = false;
                this.last_error = this
                    .snapshot
                    .process_error
                    .clone()
                    .or_else(|| this.snapshot.socket_error.clone());
                cx.emit(SnapshotEvent::Updated);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn refresh_dns(&mut self, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let supports = services.dns.supports_remove_entry();
                    match services.dns.list_cache() {
                        Ok(entries) => DnsCacheSnapshot {
                            entries,
                            error: None,
                            supports_remove_entry: supports,
                        },
                        Err(e) => DnsCacheSnapshot {
                            entries: Vec::new(),
                            error: Some(e.to_string()),
                            supports_remove_entry: supports,
                        },
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.dns = result;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn refresh_proxy_health(&mut self, cx: &mut Context<Self>) {
        let services = self.services.clone();
        let sockets = self.snapshot.sockets.clone();
        let processes = self.snapshot.processes.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let system = services.proxy.system_proxy();
                    let winhttp = services.proxy.winhttp_proxy();
                    let env = services.proxy.env_proxy();

                    let mut error = None;
                    let system = match system {
                        Ok(s) => s,
                        Err(e) => {
                            error = Some(e.to_string());
                            Default::default()
                        }
                    };
                    let winhttp = match winhttp {
                        Ok(w) => w,
                        Err(e) => {
                            error = Some(e.to_string());
                            Default::default()
                        }
                    };
                    let env = match env {
                        Ok(e) => e,
                        Err(e) => {
                            error = Some(e.to_string());
                            Default::default()
                        }
                    };

                    let name_by_pid: std::collections::HashMap<_, _> = processes
                        .iter()
                        .map(|p| (p.pid, p.name.clone()))
                        .collect();
                    let listening: Vec<(u16, Pid, String)> = sockets
                        .iter()
                        .filter(|s| s.state == SocketState::Listen || s.protocol == crate::domain::Protocol::Udp)
                        .filter(|s| s.local.ip().is_loopback() || s.local.ip().is_unspecified())
                        .map(|s| {
                            (
                                s.local_port(),
                                s.pid,
                                name_by_pid
                                    .get(&s.pid)
                                    .cloned()
                                    .unwrap_or_else(|| "?".into()),
                            )
                        })
                        .collect();

                    let findings = build_proxy_findings(&system, &winhttp, &env, &listening);
                    ProxyHealthSnapshot {
                        system,
                        winhttp,
                        env,
                        findings,
                        error,
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.proxy = result;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn ask_kill(&mut self, pid: Pid, name: String, tree: bool, cx: &mut Context<Self>) {
        self.kill_confirm = Some(KillConfirm { pid, name, tree });
        cx.notify();
    }

    pub fn cancel_kill(&mut self, cx: &mut Context<Self>) {
        self.kill_confirm = None;
        cx.notify();
    }

    pub fn confirm_kill(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.kill_confirm.take() else {
            return;
        };
        let services = self.services.clone();
        let pid = confirm.pid;
        let tree = confirm.tree;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { services.processes.kill(pid, tree) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.last_error = None;
                        this.request_refresh(cx);
                    }
                    Err(e) => {
                        this.last_error = Some(e.to_string());
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub fn ask_action(&mut self, action: PendingAction, cx: &mut Context<Self>) {
        self.pending_action = Some(action);
        cx.notify();
    }

    pub fn cancel_action(&mut self, cx: &mut Context<Self>) {
        self.pending_action = None;
        cx.notify();
    }

    pub fn confirm_action(&mut self, cx: &mut Context<Self>) {
        let Some(action) = self.pending_action.take() else {
            return;
        };
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    match action {
                        PendingAction::FlushDns => services.dns.flush_cache(),
                        PendingAction::RemoveDns { name } => services.dns.remove_entry(&name),
                        PendingAction::DisableSystemProxy => services.proxy.disable_system_proxy(),
                        PendingAction::ResetWinHttp => services.proxy.reset_winhttp_proxy(),
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.last_error = None;
                        this.refresh_dns(cx);
                        this.refresh_proxy_health(cx);
                        this.request_refresh(cx);
                    }
                    Err(e) => {
                        this.last_error = Some(e.to_string());
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn arm_timer(&mut self, cx: &mut Context<Self>) {
        self.timer_generation = self.timer_generation.wrapping_add(1);
        if !self.refresh.auto {
            return;
        }
        let generation = self.timer_generation;
        let interval = Duration::from_millis(self.refresh.interval_ms);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(interval).await;
            this.update(cx, |this, cx| {
                if this.timer_generation != generation || !this.refresh.auto {
                    return;
                }
                this.request_refresh(cx);
                this.arm_timer(cx);
            })
            .ok();
        })
        .detach();
    }
}
