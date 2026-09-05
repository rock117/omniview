use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui::prelude::*;
use gpui::*;

use crate::collect::{PendingAction, SnapshotEvent, SnapshotStore, SystemSnapshot};
use crate::domain::{
    children_map, filter_dns_entries, filter_processes, filter_sockets, format_bytes,
    is_common_proxy_port, sort_processes, HealthLevel, MainPane, Pid, ProcessInfo, ProcessSortKey,
    ProcessViewMode, Protocol, SortDir, SocketState,
};
use crate::shared::actions::*;
use crate::shared::theme;
use crate::ui::widgets::{ChipButton, ToolButton};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActiveField {
    Process,
    Port,
    PortProc,
    File,
    Dns,
}

pub struct WorkspaceView {
    store: Entity<SnapshotStore>,
    pane: MainPane,
    process_query: String,
    port_query: String,
    port_proc_query: String,
    port_proto: Option<Protocol>,
    file_query: String,
    dns_query: String,
    active_field: ActiveField,
    sort_key: ProcessSortKey,
    sort_dir: SortDir,
    view_mode: ProcessViewMode,
    expanded: HashSet<Pid>,
    selected_pid: Option<Pid>,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl WorkspaceView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let store = cx.new(SnapshotStore::new);
        let focus = cx.focus_handle();
        let mut view = Self {
            store: store.clone(),
            pane: MainPane::Processes,
            process_query: String::new(),
            port_query: String::new(),
            port_proc_query: String::new(),
            port_proto: None,
            file_query: String::new(),
            dns_query: String::new(),
            active_field: ActiveField::Process,
            sort_key: ProcessSortKey::Cpu,
            sort_dir: SortDir::Desc,
            view_mode: ProcessViewMode::List,
            expanded: HashSet::new(),
            selected_pid: None,
            focus,
            _subs: Vec::new(),
        };
        view._subs.push(cx.subscribe(&store, |this, _store, event, cx| {
            if matches!(event, SnapshotEvent::Updated) {
                if let Some(pid) = this.selected_pid {
                    let alive = this
                        .store
                        .read(cx)
                        .snapshot
                        .processes
                        .iter()
                        .any(|p| p.pid == pid);
                    if !alive {
                        this.selected_pid = None;
                    }
                }
                cx.notify();
            }
        }));
        store.update(cx, |s, cx| {
            s.request_refresh(cx);
            s.refresh_dns(cx);
            s.refresh_proxy_health(cx);
        });
        view
    }

    fn snapshot(&self, cx: &App) -> Arc<SystemSnapshot> {
        self.store.read(cx).snapshot.clone()
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let auto = store.refresh.auto;
        let interval = store.refresh.interval_ms;
        let busy = store.busy;
        let last = store.snapshot.collected_at_ms;
        let err = store.last_error.clone();
        let store_e = self.store.clone();

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme::BORDER)
            .bg(theme::SIDEBAR_BG)
            .child(nav_chip(self, "进程", MainPane::Processes, cx))
            .child(nav_chip(self, "端口", MainPane::Ports, cx))
            .child(nav_chip(self, "文件", MainPane::Files, cx))
            .child(nav_chip(self, "DNS", MainPane::Dns, cx))
            .child(nav_chip(self, "代理排障", MainPane::Proxy, cx))
            .child(
                div()
                    .flex_1()
                    .text_sm()
                    .text_color(theme::TEXT_MUTED)
                    .child(if busy {
                        "刷新中…".to_string()
                    } else {
                        format!("就绪 · 采样 {last}")
                    }),
            )
            .child(ToolButton::new("refresh", "刷新 F5", {
                let store = store_e.clone();
                move |_, _, cx| store.update(cx, |s, cx| s.request_refresh(cx))
            }))
            .child(ToolButton::new(
                "auto",
                if auto { "自动:开" } else { "自动:关" },
                {
                    let store = store_e.clone();
                    move |_, _, cx| {
                        store.update(cx, |s, cx| s.set_auto(!s.refresh.auto, cx));
                    }
                },
            ))
            .child(ToolButton::new("int-down", "频率-", {
                let store = store_e.clone();
                move |_, _, cx| {
                    store.update(cx, |s, cx| {
                        s.set_interval_ms(s.refresh.interval_ms.saturating_sub(500).max(500), cx);
                    });
                }
            }))
            .child(
                div()
                    .text_sm()
                    .text_color(theme::TEXT)
                    .child(format!("{interval}ms")),
            )
            .child(ToolButton::new("int-up", "频率+", {
                let store = store_e;
                move |_, _, cx| {
                    store.update(cx, |s, cx| {
                        s.set_interval_ms((s.refresh.interval_ms + 500).min(10_000), cx);
                    });
                }
            }))
            .when_some(err, |el, e| {
                el.child(div().text_xs().text_color(theme::DANGER).child(e))
            })
    }

    fn render_process_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let snap = self.snapshot(cx);
        let mut processes = snap.processes.clone();
        sort_processes(&mut processes, self.sort_key, self.sort_dir);
        let filtered: Vec<ProcessInfo> = filter_processes(&processes, &self.process_query)
            .into_iter()
            .cloned()
            .collect();

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme::BORDER)
            .child(search_box(
                "proc-q",
                &self.process_query,
                "搜索进程（名称 / PID / 路径）…",
                self.active_field == ActiveField::Process,
                cx.listener(|this, _, _, cx| {
                    this.active_field = ActiveField::Process;
                    cx.notify();
                }),
            ))
            .child(ChipButton::new(
                "mode-list",
                "列表",
                self.view_mode == ProcessViewMode::List,
                cx.listener(|this, _, _, cx| {
                    this.view_mode = ProcessViewMode::List;
                    cx.notify();
                }),
            ))
            .child(ChipButton::new(
                "mode-tree",
                "树形",
                self.view_mode == ProcessViewMode::Tree,
                cx.listener(|this, _, _, cx| {
                    this.view_mode = ProcessViewMode::Tree;
                    cx.notify();
                }),
            ))
            .child(sort_chip(self, "sort-cpu", "CPU", ProcessSortKey::Cpu, cx))
            .child(sort_chip(
                self,
                "sort-mem",
                "内存",
                ProcessSortKey::Memory,
                cx,
            ))
            .child(sort_chip(
                self,
                "sort-name",
                "名称",
                ProcessSortKey::Name,
                cx,
            ));

        let rows = match self.view_mode {
            ProcessViewMode::List => self.render_process_rows_flat(&filtered, cx),
            ProcessViewMode::Tree => self.render_process_rows_tree(&filtered, cx),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(header)
            .child(col_header(&[
                ("PID", 70.),
                ("名称", 0.),
                ("CPU%", 70.),
                ("内存", 90.),
                ("路径", 0.),
            ]))
            .child(
                div()
                    .id("proc-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(rows),
            )
    }

    fn render_process_rows_flat(
        &self,
        list: &[ProcessInfo],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        list.iter()
            .map(|p| self.process_row(p, 0, false, cx).into_any_element())
            .collect()
    }

    fn render_process_rows_tree(
        &mut self,
        list: &[ProcessInfo],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let by_pid: HashMap<Pid, ProcessInfo> =
            list.iter().map(|p| (p.pid, p.clone())).collect();
        let kids = children_map(list);
        let mut roots: Vec<Pid> = list
            .iter()
            .filter(|p| {
                p.parent_pid
                    .map(|pp| !by_pid.contains_key(&pp))
                    .unwrap_or(true)
            })
            .map(|p| p.pid)
            .collect();
        roots.sort_by(|a, b| cmp_proc(by_pid.get(a), by_pid.get(b), self.sort_key));
        if self.sort_dir == SortDir::Desc {
            roots.reverse();
        }
        let mut out = Vec::new();
        for root in roots {
            self.walk_tree(root, 0, &by_pid, &kids, &mut out, cx);
        }
        out
    }

    fn walk_tree(
        &self,
        pid: Pid,
        depth: u32,
        by_pid: &HashMap<Pid, ProcessInfo>,
        kids: &HashMap<Pid, Vec<Pid>>,
        out: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        let Some(proc_) = by_pid.get(&pid) else {
            return;
        };
        let has_kids = kids.get(&pid).map(|k| !k.is_empty()).unwrap_or(false);
        out.push(
            self.process_row(proc_, depth, has_kids, cx)
                .into_any_element(),
        );
        if has_kids && self.expanded.contains(&pid) {
            if let Some(children) = kids.get(&pid) {
                for child in children {
                    self.walk_tree(*child, depth + 1, by_pid, kids, out, cx);
                }
            }
        }
    }

    fn process_row(
        &self,
        p: &ProcessInfo,
        depth: u32,
        has_kids: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.selected_pid == Some(p.pid);
        let pid = p.pid;
        let name = p.name.clone();
        let expanded = self.expanded.contains(&pid);
        let indent = px(12.0 * depth as f32);

        div()
            .id(ElementId::Name(format!("proc-{pid}").into()))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .pl(px(12.0) + indent)
            .bg(if selected {
                theme::SELECTED
            } else {
                theme::BG
            })
            .hover(|s| s.bg(theme::HOVER))
            .cursor_pointer()
            .child(if has_kids {
                ToolButton::new(
                    ElementId::Name(format!("exp-{pid}").into()),
                    if expanded { "▼" } else { "▶" },
                    cx.listener(move |this, _, _, cx| {
                        if !this.expanded.insert(pid) {
                            this.expanded.remove(&pid);
                        }
                        cx.notify();
                    }),
                )
                .into_any_element()
            } else {
                div().w(px(28.)).into_any_element()
            })
            .child(
                div()
                    .w(px(70.))
                    .text_sm()
                    .text_color(theme::TEXT_MUTED)
                    .child(pid.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .text_sm()
                    .text_color(theme::TEXT)
                    .child(name.clone()),
            )
            .child(
                div()
                    .w(px(70.))
                    .text_sm()
                    .child(format!("{:.1}", p.cpu_percent)),
            )
            .child(
                div()
                    .w(px(90.))
                    .text_sm()
                    .child(format_bytes(p.memory_bytes)),
            )
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(theme::TEXT_MUTED)
                    .child(p.exe_path.clone().unwrap_or_default()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected_pid = Some(pid);
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, _, cx| {
                    this.selected_pid = Some(pid);
                    this.store.update(cx, |s, cx| {
                        s.ask_kill(pid, name.clone(), false, cx);
                    });
                }),
            )
    }

    fn render_port_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let snap = self.snapshot(cx);
        let name_by_pid: HashMap<Pid, String> = snap
            .processes
            .iter()
            .map(|p| (p.pid, p.name.clone()))
            .collect();
        let filtered = filter_sockets(
            &snap.sockets,
            &self.port_query,
            &self.port_proc_query,
            self.port_proto,
            |pid| name_by_pid.get(&pid).cloned(),
        );

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme::BORDER)
                    .child(search_box(
                        "port-q",
                        &self.port_query,
                        "按端口搜索…",
                        self.active_field == ActiveField::Port,
                        cx.listener(|this, _, _, cx| {
                            this.active_field = ActiveField::Port;
                            cx.notify();
                        }),
                    ))
                    .child(search_box(
                        "port-proc-q",
                        &self.port_proc_query,
                        "按进程名搜索…",
                        self.active_field == ActiveField::PortProc,
                        cx.listener(|this, _, _, cx| {
                            this.active_field = ActiveField::PortProc;
                            cx.notify();
                        }),
                    ))
                    .child(ChipButton::new(
                        "proto-all",
                        "全部",
                        self.port_proto.is_none(),
                        cx.listener(|this, _, _, cx| {
                            this.port_proto = None;
                            cx.notify();
                        }),
                    ))
                    .child(ChipButton::new(
                        "proto-tcp",
                        "TCP",
                        self.port_proto == Some(Protocol::Tcp),
                        cx.listener(|this, _, _, cx| {
                            this.port_proto = Some(Protocol::Tcp);
                            cx.notify();
                        }),
                    ))
                    .child(ChipButton::new(
                        "proto-udp",
                        "UDP",
                        self.port_proto == Some(Protocol::Udp),
                        cx.listener(|this, _, _, cx| {
                            this.port_proto = Some(Protocol::Udp);
                            cx.notify();
                        }),
                    ))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::TEXT_MUTED)
                            .child(format!("{} 条", filtered.len())),
                    ),
            )
            .child(col_header(&[
                ("协议", 50.),
                ("本地", 140.),
                ("远程", 140.),
                ("状态", 100.),
                ("PID", 70.),
                ("进程", 0.),
            ]))
            .child(
                div()
                    .id("port-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(filtered.into_iter().map(|r| {
                        let pid = r.pid;
                        let pname = name_by_pid
                            .get(&pid)
                            .cloned()
                            .unwrap_or_else(|| "?".into());
                        let remote = r
                            .remote
                            .map(|a| a.to_string())
                            .unwrap_or_else(|| "-".into());
                        let proxy_hl = is_common_proxy_port(r.local_port());
                        div()
                            .id(ElementId::Name(
                                format!(
                                    "sock-{}-{}-{}-{}",
                                    r.protocol.as_str(),
                                    r.local,
                                    r.state.as_str(),
                                    pid
                                )
                                .into(),
                            ))
                            .flex()
                            .flex_row()
                            .gap_2()
                            .px_3()
                            .py_1()
                            .bg(if proxy_hl {
                                Hsla {
                                    h: 0.48,
                                    s: 0.35,
                                    l: 0.18,
                                    a: 1.0,
                                }
                            } else {
                                theme::BG
                            })
                            .hover(|s| s.bg(theme::HOVER))
                            .cursor_pointer()
                            .child(
                                div()
                                    .w(px(50.))
                                    .text_sm()
                                    .text_color(theme::ACCENT)
                                    .child(r.protocol.as_str()),
                            )
                            .child(
                                div()
                                    .w(px(140.))
                                    .text_sm()
                                    .child(r.local.to_string()),
                            )
                            .child(
                                div()
                                    .w(px(140.))
                                    .text_sm()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(remote),
                            )
                            .child(
                                div()
                                    .w(px(100.))
                                    .text_xs()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(r.state.as_str()),
                            )
                            .child(div().w(px(70.)).text_sm().child(pid.to_string()))
                            .child(div().flex_1().text_sm().child(pname))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_pid = Some(pid);
                                cx.notify();
                            }))
                            .into_any_element()
                    })),
            )
    }

    fn render_file_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .px_4()
            .py_4()
            .gap_2()
            .child(
                div()
                    .text_lg()
                    .text_color(theme::TEXT)
                    .child("按文件 / 目录查占用"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme::TEXT_MUTED)
                    .child("HandleProbe 已预留，Windows 句柄枚举待实现。"),
            )
            .child(search_box(
                "file-q",
                &self.file_query,
                "粘贴路径…",
                self.active_field == ActiveField::File,
                cx.listener(|this, _, _, cx| {
                    this.active_field = ActiveField::File;
                    cx.notify();
                }),
            ))
    }

    fn render_dns_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let dns = self.store.read(cx).dns.clone();
        let supports_remove = dns.supports_remove_entry;
        let entries = filter_dns_entries(&dns.entries, &self.dns_query);
        let store = self.store.clone();
        let store2 = self.store.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme::BORDER)
                    .child(search_box(
                        "dns-q",
                        &self.dns_query,
                        "搜索域名 / IP…",
                        self.active_field == ActiveField::Dns,
                        cx.listener(|this, _, _, cx| {
                            this.active_field = ActiveField::Dns;
                            cx.notify();
                        }),
                    ))
                    .child(ToolButton::new("dns-refresh", "刷新缓存", {
                        let store = store.clone();
                        move |_, _, cx| store.update(cx, |s, cx| s.refresh_dns(cx))
                    }))
                    .child(
                        ToolButton::new("dns-flush", "清空全部", {
                            let store = store2;
                            move |_, _, cx| {
                                store.update(cx, |s, cx| {
                                    s.ask_action(PendingAction::FlushDns, cx);
                                });
                            }
                        })
                        .danger(),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::TEXT_MUTED)
                            .child(format!("{} 条", entries.len())),
                    ),
            )
            .when_some(dns.error.clone(), |el, e| {
                el.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(theme::DANGER)
                        .child(e),
                )
            })
            .child(col_header(&[("类型", 70.), ("名称", 0.), ("数据", 0.), ("操作", 80.)]))
            .child(
                div()
                    .id("dns-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(entries.into_iter().map(|e| {
                        let name = e.name.clone();
                        let store = self.store.clone();
                        div()
                            .id(ElementId::Name(format!("dns-{}-{}", e.record_type, name).into()))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1()
                            .hover(|s| s.bg(theme::HOVER))
                            .child(
                                div()
                                    .w(px(70.))
                                    .text_sm()
                                    .text_color(theme::ACCENT)
                                    .child(e.record_type.clone()),
                            )
                            .child(div().flex_1().text_sm().child(e.name.clone()))
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(if e.data.is_empty() {
                                        "—".into()
                                    } else {
                                        e.data.clone()
                                    }),
                            )
                            .child(if supports_remove {
                                ToolButton::new(
                                    ElementId::Name(format!("dns-del-{name}").into()),
                                    "删除",
                                    move |_, _, cx| {
                                        store.update(cx, |s, cx| {
                                            s.ask_action(
                                                PendingAction::RemoveDns { name: name.clone() },
                                                cx,
                                            );
                                        });
                                    },
                                )
                                .danger()
                                .into_any_element()
                            } else {
                                div().w(px(80.)).into_any_element()
                            })
                            .into_any_element()
                    })),
            )
    }

    fn render_proxy_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let proxy = self.store.read(cx).proxy.clone();
        let store = self.store.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .px_4()
            .py_3()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .text_color(theme::TEXT)
                            .child("代理排障体检"),
                    )
                    .child(ToolButton::new("proxy-refresh", "重新体检", {
                        let store = store.clone();
                        move |_, _, cx| {
                            store.update(cx, |s, cx| {
                                s.request_refresh(cx);
                                s.refresh_proxy_health(cx);
                            });
                        }
                    }))
                    .child(
                        ToolButton::new("proxy-off", "关闭系统代理", {
                            let store = store.clone();
                            move |_, _, cx| {
                                store.update(cx, |s, cx| {
                                    s.ask_action(PendingAction::DisableSystemProxy, cx);
                                });
                            }
                        })
                        .danger(),
                    )
                    .child(
                        ToolButton::new("winhttp-reset", "重置 WinHTTP", {
                            let store = store.clone();
                            move |_, _, cx| {
                                store.update(cx, |s, cx| {
                                    s.ask_action(PendingAction::ResetWinHttp, cx);
                                });
                            }
                        })
                        .danger(),
                    )
                    .child(
                        ToolButton::new("proxy-flush-dns", "清空 DNS", {
                            let store = store;
                            move |_, _, cx| {
                                store.update(cx, |s, cx| {
                                    s.ask_action(PendingAction::FlushDns, cx);
                                });
                            }
                        })
                        .danger(),
                    ),
            )
            .when_some(proxy.error.clone(), |el, e| {
                el.child(div().text_xs().text_color(theme::DANGER).child(e))
            })
            .child(info_block(
                "系统代理",
                format!(
                    "启用={}  服务器={}  例外={}",
                    proxy.system.enabled, proxy.system.server, proxy.system.override_list
                ),
            ))
            .child(info_block(
                "WinHTTP",
                if proxy.winhttp.available {
                    proxy.winhttp.summary.clone()
                } else {
                    "不可用".into()
                },
            ))
            .child(info_block(
                "环境变量",
                if proxy.env.entries.is_empty() {
                    "(无)".into()
                } else {
                    proxy
                        .env
                        .entries
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join("  |  ")
                },
            ))
            .child(
                div()
                    .text_sm()
                    .text_color(theme::TEXT_MUTED)
                    .child("体检结果"),
            )
            .child(
                div()
                    .id("findings")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(proxy.findings.into_iter().map(|f| {
                        let color = match f.level {
                            HealthLevel::Ok => theme::ACCENT,
                            HealthLevel::Warn => Hsla {
                                h: 0.12,
                                s: 0.7,
                                l: 0.55,
                                a: 1.0,
                            },
                            HealthLevel::Bad => theme::DANGER,
                        };
                        let tag = match f.level {
                            HealthLevel::Ok => "OK",
                            HealthLevel::Warn => "WARN",
                            HealthLevel::Bad => "BAD",
                        };
                        div()
                            .mb_2()
                            .p_2()
                            .rounded(px(theme::RADIUS_SM))
                            .bg(theme::PANEL_BG)
                            .border_1()
                            .border_color(theme::BORDER)
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(color)
                                    .child(format!("[{tag}] {}", f.title)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(f.detail),
                            )
                            .into_any_element()
                    })),
            )
    }

    fn render_detail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let snap = self.snapshot(cx);
        let Some(pid) = self.selected_pid else {
            return div()
                .w(px(340.))
                .h_full()
                .border_l_1()
                .border_color(theme::BORDER)
                .bg(theme::PANEL_BG)
                .px_3()
                .py_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(theme::TEXT_MUTED)
                        .child("选择进程以查看详情"),
                )
                .into_any_element();
        };

        let proc_ = snap.processes.iter().find(|p| p.pid == pid);
        let sockets: Vec<_> = snap
            .sockets
            .iter()
            .filter(|s| s.pid == pid)
            .cloned()
            .collect();
        let store = self.store.clone();
        let store2 = self.store.clone();
        let name = proc_
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "?".into());

        div()
            .w(px(340.))
            .h_full()
            .border_l_1()
            .border_color(theme::BORDER)
            .bg(theme::PANEL_BG)
            .flex()
            .flex_col()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme::BORDER)
                    .child(
                        div()
                            .text_color(theme::TEXT)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{name}  ({pid})")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .child(
                        ToolButton::new("kill", "结束进程", {
                            let store = store;
                            let name = name.clone();
                            move |_, _, cx| {
                                store.update(cx, |s, cx| s.ask_kill(pid, name.clone(), false, cx));
                            }
                        })
                        .danger(),
                    )
                    .child(
                        ToolButton::new("kill-tree", "结束进程树", {
                            let store = store2;
                            let name = name.clone();
                            move |_, _, cx| {
                                store.update(cx, |s, cx| s.ask_kill(pid, name.clone(), true, cx));
                            }
                        })
                        .danger(),
                    ),
            )
            .when_some(proc_.cloned(), |el, p| {
                el.child(
                    div()
                        .px_3()
                        .py_2()
                        .gap_1()
                        .flex()
                        .flex_col()
                        .text_sm()
                        .child(format!("CPU  {:.1}%", p.cpu_percent))
                        .child(format!("内存  {}", format_bytes(p.memory_bytes)))
                        .child(format!(
                            "父 PID  {}",
                            p.parent_pid
                                .map(|x| x.to_string())
                                .unwrap_or_else(|| "-".into())
                        ))
                        .child(format!(
                            "路径  {}",
                            p.exe_path.clone().unwrap_or_else(|| "-".into())
                        )),
                )
            })
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(theme::TEXT_MUTED)
                    .child(format!("网络（{}）", sockets.len())),
            )
            .child(
                div()
                    .id("detail-socks")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_3()
                    .children(sockets.into_iter().map(|s| {
                        div()
                            .py_1()
                            .text_xs()
                            .child(format!(
                                "{} {} {} {}",
                                s.protocol.as_str(),
                                s.local,
                                s.remote
                                    .map(|r| r.to_string())
                                    .unwrap_or_else(|| "-".into()),
                                s.state.as_str()
                            ))
                            .into_any_element()
                    })),
            )
            .into_any_element()
    }

    fn render_modals(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let kill = self.store.read(cx).kill_confirm.clone();
        let action = self.store.read(cx).pending_action.clone();
        let mut children: Vec<AnyElement> = Vec::new();

        if let Some(confirm) = kill {
            let store = self.store.clone();
            let store2 = self.store.clone();
            let label = if confirm.tree {
                "结束进程树"
            } else {
                "结束进程"
            };
            children.push(
                modal_shell(
                    format!("确认{label}？\n{} (PID {})", confirm.name, confirm.pid),
                    store,
                    store2,
                    |s, cx| s.cancel_kill(cx),
                    |s, cx| s.confirm_kill(cx),
                )
                .into_any_element(),
            );
        }

        if let Some(action) = action {
            let (title, _) = match &action {
                PendingAction::FlushDns => ("确认清空全部 DNS 缓存？".to_string(), ()),
                PendingAction::RemoveDns { name } => {
                    (format!("确认删除 DNS 缓存条目？\n{name}"), ())
                }
                PendingAction::DisableSystemProxy => {
                    ("确认关闭系统代理（ProxyEnable=0）？".to_string(), ())
                }
                PendingAction::ResetWinHttp => ("确认将 WinHTTP 代理重置为直连？".to_string(), ()),
            };
            let store = self.store.clone();
            let store2 = self.store.clone();
            children.push(
                modal_shell(
                    title,
                    store,
                    store2,
                    |s, cx| s.cancel_action(cx),
                    |s, cx| s.confirm_action(cx),
                )
                .into_any_element(),
            );
        }

        div().children(children)
    }

    fn type_into_active(&mut self, ch: &str) {
        let field = match self.active_field {
            ActiveField::Process => &mut self.process_query,
            ActiveField::Port => &mut self.port_query,
            ActiveField::PortProc => &mut self.port_proc_query,
            ActiveField::File => &mut self.file_query,
            ActiveField::Dns => &mut self.dns_query,
        };
        field.push_str(ch);
    }

    fn backspace_active(&mut self) {
        let field = match self.active_field {
            ActiveField::Process => &mut self.process_query,
            ActiveField::Port => &mut self.port_query,
            ActiveField::PortProc => &mut self.port_proc_query,
            ActiveField::File => &mut self.file_query,
            ActiveField::Dns => &mut self.dns_query,
        };
        field.pop();
    }

    fn clear_active(&mut self) {
        let field = match self.active_field {
            ActiveField::Process => &mut self.process_query,
            ActiveField::Port => &mut self.port_query,
            ActiveField::PortProc => &mut self.port_proc_query,
            ActiveField::File => &mut self.file_query,
            ActiveField::Dns => &mut self.dns_query,
        };
        field.clear();
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus.is_focused(window) {
            self.focus.focus(window);
        }

        let main = match self.pane {
            MainPane::Processes => self.render_process_pane(cx).into_any_element(),
            MainPane::Ports => self.render_port_pane(cx).into_any_element(),
            MainPane::Files => self.render_file_pane(cx).into_any_element(),
            MainPane::Dns => self.render_dns_pane(cx).into_any_element(),
            MainPane::Proxy => self.render_proxy_pane(cx).into_any_element(),
        };

        let show_detail = matches!(
            self.pane,
            MainPane::Processes | MainPane::Ports | MainPane::Files
        );

        div()
            .track_focus(&self.focus)
            .key_context("Omniview")
            .on_action(cx.listener(|this, _: &RefreshNow, _, cx| {
                this.store.update(cx, |s, cx| {
                    s.request_refresh(cx);
                    if this.pane == MainPane::Dns {
                        s.refresh_dns(cx);
                    }
                    if this.pane == MainPane::Proxy {
                        s.refresh_proxy_health(cx);
                    }
                });
            }))
            .on_action(cx.listener(|this, _: &ShowProcesses, _, cx| {
                this.pane = MainPane::Processes;
                this.active_field = ActiveField::Process;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ShowPorts, _, cx| {
                this.pane = MainPane::Ports;
                this.active_field = ActiveField::Port;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ShowFiles, _, cx| {
                this.pane = MainPane::Files;
                this.active_field = ActiveField::File;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ShowDns, _, cx| {
                this.pane = MainPane::Dns;
                this.active_field = ActiveField::Dns;
                this.store.update(cx, |s, cx| s.refresh_dns(cx));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ShowProxy, _, cx| {
                this.pane = MainPane::Proxy;
                this.store.update(cx, |s, cx| s.refresh_proxy_health(cx));
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "backspace" {
                    this.backspace_active();
                    cx.notify();
                    return;
                }
                if event.keystroke.key == "escape" {
                    this.clear_active();
                    cx.notify();
                    return;
                }
                if let Some(ch) = event.keystroke.key_char.as_ref() {
                    if !ch.is_empty() && !event.keystroke.modifiers.control {
                        this.type_into_active(ch);
                        cx.notify();
                    }
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::BG)
            .text_color(theme::TEXT)
            .font_family(theme::FONT_UI)
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    .child(div().flex_1().h_full().child(main))
                    .when(show_detail, |el| el.child(self.render_detail(cx))),
            )
            .child(self.render_modals(cx))
    }
}

fn nav_chip(
    this: &WorkspaceView,
    label: &'static str,
    pane: MainPane,
    cx: &mut Context<WorkspaceView>,
) -> ChipButton {
    ChipButton::new(
        ElementId::Name(format!("nav-{label}").into()),
        label,
        this.pane == pane,
        cx.listener(move |this, _, _, cx| {
            this.pane = pane;
            this.active_field = match pane {
                MainPane::Processes => ActiveField::Process,
                MainPane::Ports => ActiveField::Port,
                MainPane::Files => ActiveField::File,
                MainPane::Dns => ActiveField::Dns,
                MainPane::Proxy => ActiveField::Process,
            };
            if pane == MainPane::Dns {
                this.store.update(cx, |s, cx| s.refresh_dns(cx));
            }
            if pane == MainPane::Proxy {
                this.store.update(cx, |s, cx| s.refresh_proxy_health(cx));
            }
            cx.notify();
        }),
    )
}

fn sort_chip(
    this: &WorkspaceView,
    id: &'static str,
    label: &'static str,
    key: ProcessSortKey,
    cx: &mut Context<WorkspaceView>,
) -> ChipButton {
    let active = this.sort_key == key;
    let mark = if active {
        match this.sort_dir {
            SortDir::Asc => " ↑",
            SortDir::Desc => " ↓",
        }
    } else {
        ""
    };
    ChipButton::new(
        id,
        format!("{label}{mark}"),
        active,
        cx.listener(move |this, _, _, cx| {
            if this.sort_key == key {
                this.sort_dir = this.sort_dir.toggle();
            } else {
                this.sort_key = key;
                this.sort_dir = SortDir::Desc;
            }
            cx.notify();
        }),
    )
}

fn search_box(
    id: impl Into<ElementId>,
    value: &str,
    placeholder: &str,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex_1()
        .px_2()
        .py_1()
        .rounded(px(theme::RADIUS_SM))
        .bg(theme::ELEVATED)
        .border_1()
        .border_color(if active {
            theme::ACCENT
        } else {
            theme::BORDER
        })
        .text_sm()
        .text_color(if value.is_empty() {
            theme::TEXT_MUTED
        } else {
            theme::TEXT
        })
        .cursor_text()
        .child(if value.is_empty() {
            placeholder.to_string()
        } else {
            value.to_string()
        })
        .on_click(on_click)
}

fn col_header(cols: &[(&str, f32)]) -> impl IntoElement {
    let owned: Vec<(String, f32)> = cols
        .iter()
        .map(|(label, w)| ((*label).to_string(), *w))
        .collect();
    let mut row = div()
        .flex()
        .flex_row()
        .px_3()
        .py_1()
        .gap_3()
        .border_b_1()
        .border_color(theme::BORDER)
        .text_xs()
        .text_color(theme::TEXT_MUTED);
    for (label, w) in owned {
        let cell = if w > 0.0 {
            div().w(px(w)).child(label)
        } else {
            div().flex_1().child(label)
        };
        row = row.child(cell);
    }
    row
}

fn info_block(title: &str, body: String) -> impl IntoElement {
    div()
        .p_2()
        .rounded(px(theme::RADIUS_SM))
        .bg(theme::PANEL_BG)
        .border_1()
        .border_color(theme::BORDER)
        .child(
            div()
                .text_xs()
                .text_color(theme::TEXT_MUTED)
                .child(title.to_string()),
        )
        .child(div().text_sm().text_color(theme::TEXT).child(body))
}

fn modal_shell(
    title: String,
    store_cancel: Entity<SnapshotStore>,
    store_ok: Entity<SnapshotStore>,
    cancel: impl Fn(&mut SnapshotStore, &mut Context<SnapshotStore>) + 'static + Clone,
    ok: impl Fn(&mut SnapshotStore, &mut Context<SnapshotStore>) + 'static + Clone,
) -> impl IntoElement {
    let cancel2 = cancel.clone();
    let ok2 = ok.clone();
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 0.45,
        })
        .child(
            div()
                .w(px(400.))
                .p_4()
                .rounded(px(8.))
                .bg(theme::ELEVATED)
                .border_1()
                .border_color(theme::BORDER)
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_color(theme::TEXT).child(title))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .justify_end()
                        .child(ToolButton::new("modal-cancel", "取消", move |_, _, cx| {
                            store_cancel.update(cx, |s, cx| cancel2(s, cx));
                        }))
                        .child(
                            ToolButton::new("modal-ok", "确认", move |_, _, cx| {
                                store_ok.update(cx, |s, cx| ok2(s, cx));
                            })
                            .danger(),
                        ),
                ),
        )
}

fn cmp_proc(
    a: Option<&ProcessInfo>,
    b: Option<&ProcessInfo>,
    key: ProcessSortKey,
) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => match key {
            ProcessSortKey::Cpu => a
                .cpu_percent
                .partial_cmp(&b.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal),
            ProcessSortKey::Memory => a.memory_bytes.cmp(&b.memory_bytes),
            ProcessSortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            ProcessSortKey::Pid => a.pid.cmp(&b.pid),
        },
        _ => std::cmp::Ordering::Equal,
    }
}

#[allow(dead_code)]
fn _listen() -> SocketState {
    SocketState::Listen
}
