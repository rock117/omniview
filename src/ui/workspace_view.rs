use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui::prelude::*;
use gpui::*;

use crate::collect::{PendingAction, SnapshotEvent, SnapshotStore, SystemSnapshot};
use crate::domain::{
    children_map, filter_dns_entries, filter_processes, filter_sockets_unified, format_bytes,
    is_common_proxy_port, sort_processes, HealthLevel, MainPane, Pid, ProcessInfo, ProcessSortKey,
    ProcessViewMode, Protocol, SortDir, SocketState, REFRESH_PRESETS_MS,
};
use crate::shared::actions::*;
use crate::shared::theme;
use crate::ui::text_edit::TextEdit;
use crate::ui::widgets::{ChipButton, NavItem, ToolButton};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActiveField {
    Process,
    Port,
    File,
    Dns,
}

/// Focused copyable value in the process detail panel.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailCopyTarget {
    ParentPid,
    Path,
    OpenFile(usize),
}

pub struct WorkspaceView {
    store: Entity<SnapshotStore>,
    pane: MainPane,
    process_query: TextEdit,
    port_query: TextEdit,
    port_proto: Option<Protocol>,
    file_query: TextEdit,
    dns_query: TextEdit,
    active_field: ActiveField,
    sort_key: ProcessSortKey,
    sort_dir: SortDir,
    view_mode: ProcessViewMode,
    expanded: HashSet<Pid>,
    selected_pid: Option<Pid>,
    detail_open: bool,
    detail_copy: Option<DetailCopyTarget>,
    detail_edit: TextEdit,
    input_bounds: Option<Bounds<Pixels>>,
    input_selecting: bool,
    focus: FocusHandle,
    _caret_blink: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl WorkspaceView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let store = cx.new(SnapshotStore::new);
        let focus = cx.focus_handle();
        let mut view = Self {
            store: store.clone(),
            pane: MainPane::Processes,
            process_query: TextEdit::default(),
            port_query: TextEdit::default(),
            port_proto: None,
            file_query: TextEdit::default(),
            dns_query: TextEdit::default(),
            active_field: ActiveField::Process,
            sort_key: ProcessSortKey::Cpu,
            sort_dir: SortDir::Desc,
            view_mode: ProcessViewMode::List,
            expanded: HashSet::new(),
            selected_pid: None,
            detail_open: false,
            detail_copy: None,
            detail_edit: TextEdit::default(),
            input_bounds: None,
            input_selecting: false,
            focus,
            _caret_blink: None,
            _subs: Vec::new(),
        };
        view.start_caret_blink(cx);
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
                        this.detail_copy = None;
                    } else {
                        this.sync_detail_edit(cx);
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

    fn focus_detail_copy(&mut self, target: DetailCopyTarget, text: String) {
        self.detail_copy = Some(target);
        self.detail_edit = TextEdit::new(text);
        self.detail_edit.select_all();
        self.input_selecting = false;
    }

    fn start_caret_blink(&mut self, cx: &mut Context<Self>) {
        self._caret_blink = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(530))
                    .await;
                let keep = this
                    .update(cx, |this, cx| {
                        if this.detail_copy.is_some() {
                            this.detail_edit.caret_visible = !this.detail_edit.caret_visible;
                        } else {
                            let edit = match this.active_field {
                                ActiveField::Process => &mut this.process_query,
                                ActiveField::Port => &mut this.port_query,
                                ActiveField::File => &mut this.file_query,
                                ActiveField::Dns => &mut this.dns_query,
                            };
                            edit.caret_visible = !edit.caret_visible;
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        }));
    }

    fn index_at_pointer(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.input_bounds else {
            return self.active_edit().char_len();
        };
        let pad: f32 = 8.0; // matches px_2 on text_input_box
        let local_x: f32 = (position.x - bounds.origin.x).into();
        self.active_edit().char_index_at_x(local_x - pad)
    }

    fn active_edit(&self) -> &TextEdit {
        match self.active_field {
            ActiveField::Process => &self.process_query,
            ActiveField::Port => &self.port_query,
            ActiveField::File => &self.file_query,
            ActiveField::Dns => &self.dns_query,
        }
    }

    fn detail_copy_text(&self, target: DetailCopyTarget, cx: &App) -> Option<String> {
        let pid = self.selected_pid?;
        match target {
            DetailCopyTarget::ParentPid => {
                let p = self
                    .store
                    .read(cx)
                    .snapshot
                    .processes
                    .iter()
                    .find(|p| p.pid == pid)?;
                Some(
                    p.parent_pid
                        .map(|x| x.to_string())
                        .unwrap_or_else(|| "-".into()),
                )
            }
            DetailCopyTarget::Path => {
                let p = self
                    .store
                    .read(cx)
                    .snapshot
                    .processes
                    .iter()
                    .find(|p| p.pid == pid)?;
                Some(p.exe_path.clone().unwrap_or_else(|| "-".into()))
            }
            DetailCopyTarget::OpenFile(idx) => {
                let pf = &self.store.read(cx).process_files;
                if pf.pid != Some(pid) {
                    return None;
                }
                pf.files.get(idx).map(|f| f.path.clone())
            }
        }
    }

    fn sync_detail_edit(&mut self, cx: &App) {
        let Some(target) = self.detail_copy else {
            return;
        };
        match self.detail_copy_text(target, cx) {
            Some(text) => self.detail_edit.set_text_if_changed(text),
            None => {
                self.detail_copy = None;
                self.detail_edit.clear();
            }
        }
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(theme::NAV_WIDTH))
            .h_full()
            .bg(theme::SIDEBAR_BG)
            .border_r_1()
            .border_color(theme::BORDER)
            .flex()
            .flex_col()
            .pt_3()
            .px_2()
            .child(
                div()
                    .px_2()
                    .pb_3()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::TEXT)
                    .child("Omniview"),
            )
            .child(nav_item(self, "进程", MainPane::Processes, cx))
            .child(nav_item(self, "端口", MainPane::Ports, cx))
            .child(nav_item(self, "文件", MainPane::Files, cx))
            .child(nav_item(self, "DNS", MainPane::Dns, cx))
            .child(nav_item(self, "代理排障", MainPane::Proxy, cx))
            .child(div().flex_1())
            .child(nav_item(self, "设置", MainPane::Settings, cx))
            .child(
                div()
                    .px_2()
                    .pb_2()
                    .text_xs()
                    .text_color(theme::TEXT_MUTED)
                    .child("Ctrl+, 打开设置"),
            )
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let auto = store.settings.refresh.auto;
        let interval = store.settings.refresh.interval_ms;
        let busy = store.busy;
        let count = store.snapshot.processes.len();
        let socks = store.snapshot.sockets.len();
        let err = store.last_error.clone();
        let store_e = self.store.clone();

        div()
            .h(px(theme::STATUS_HEIGHT))
            .w_full()
            .px_3()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .bg(theme::SIDEBAR_BG)
            .border_t_1()
            .border_color(theme::BORDER)
            .text_xs()
            .text_color(theme::TEXT_MUTED)
            .child(format!("{count} 进程 · {socks} 连接"))
            .child(div().flex_1())
            .when_some(err, |el, e| {
                el.child(div().text_color(theme::DANGER).child(e))
            })
            .child(if busy { "刷新中…" } else { "就绪" })
            .child(
                div()
                    .id("status-refresh-mode")
                    .cursor_pointer()
                    .text_color(if auto {
                        theme::ACCENT
                    } else {
                        theme::TEXT_MUTED
                    })
                    .child(if auto {
                        format!("自动 {interval}ms")
                    } else {
                        "仅手动".into()
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.pane = MainPane::Settings;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("status-settings")
                    .cursor_pointer()
                    .text_color(theme::ACCENT)
                    .child("设置")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.pane = MainPane::Settings;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("status-refresh")
                    .cursor_pointer()
                    .text_color(theme::ACCENT)
                    .child("刷新 F5")
                    .on_click({
                        let store = store_e;
                        move |_, _, cx| {
                            store.update(cx, |s, cx| s.request_refresh(cx));
                        }
                    }),
            )
    }

    fn render_page_header(
        &self,
        title: &str,
        trailing: impl IntoElement,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(44.))
            .border_b_1()
            .border_color(theme::BORDER)
            .bg(theme::PANEL_BG)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::TEXT)
                    .child(title.to_string()),
            )
            .child(div().flex_1())
            .child(trailing)
    }

    fn render_process_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let snap = self.snapshot(cx);
        let mut processes = snap.processes.clone();
        sort_processes(&mut processes, self.sort_key, self.sort_dir);
        let filtered: Vec<ProcessInfo> = filter_processes(&processes, &self.process_query.text)
            .into_iter()
            .cloned()
            .collect();
        let mem_peak = filtered
            .iter()
            .map(|p| p.memory_bytes)
            .max()
            .unwrap_or(1);

        let tools = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(text_input_box(
                "proc-q",
                &self.process_query,
                "搜索名称、PID 或路径",
                self.active_field == ActiveField::Process,
                px(280.),
                ActiveField::Process,
                cx,
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
            ));

        let rows = match self.view_mode {
            ProcessViewMode::List => self.render_process_rows_flat(&filtered, mem_peak, cx),
            ProcessViewMode::Tree => self.render_process_rows_tree(&filtered, mem_peak, cx),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::PANEL_BG)
            .child(self.render_page_header("进程", tools))
            .child(self.process_table_header(cx))
            .child(
                div()
                    .id("proc-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(rows),
            )
    }

    fn toggle_process_sort(&mut self, key: ProcessSortKey) {
        if self.sort_key == key {
            self.sort_dir = self.sort_dir.toggle();
        } else {
            self.sort_key = key;
            self.sort_dir = match key {
                ProcessSortKey::Name => SortDir::Asc,
                _ => SortDir::Desc,
            };
        }
    }

    fn process_table_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sort_key = self.sort_key;
        let sort_dir = self.sort_dir;
        div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(28.))
            .w_full()
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(theme::BORDER)
            .bg(theme::SIDEBAR_BG)
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .child(div().w(px(theme::COL_TREE)))
            .child(sortable_header_cell(
                "hdr-name",
                "名称",
                theme::COL_NAME,
                false,
                sort_key == ProcessSortKey::Name,
                sort_dir,
                cx.listener(|this, _, _, cx| {
                    this.toggle_process_sort(ProcessSortKey::Name);
                    cx.notify();
                }),
            ))
            .child(sortable_header_cell(
                "hdr-pid",
                "PID",
                theme::COL_PID,
                true,
                sort_key == ProcessSortKey::Pid,
                sort_dir,
                cx.listener(|this, _, _, cx| {
                    this.toggle_process_sort(ProcessSortKey::Pid);
                    cx.notify();
                }),
            ))
            .child(sortable_header_cell(
                "hdr-cpu",
                "CPU",
                theme::COL_CPU,
                true,
                sort_key == ProcessSortKey::Cpu,
                sort_dir,
                cx.listener(|this, _, _, cx| {
                    this.toggle_process_sort(ProcessSortKey::Cpu);
                    cx.notify();
                }),
            ))
            .child(sortable_header_cell(
                "hdr-mem",
                "内存",
                theme::COL_MEM,
                true,
                sort_key == ProcessSortKey::Memory,
                sort_dir,
                cx.listener(|this, _, _, cx| {
                    this.toggle_process_sort(ProcessSortKey::Memory);
                    cx.notify();
                }),
            ))
            .child(
                div()
                    .flex_1()
                    .text_color(theme::TEXT_MUTED)
                    .child("路径"),
            )
    }

    fn render_process_rows_flat(
        &self,
        list: &[ProcessInfo],
        mem_peak: u64,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        list.iter()
            .map(|p| {
                self.process_row(p, 0, false, mem_peak, cx)
                    .into_any_element()
            })
            .collect()
    }

    fn render_process_rows_tree(
        &mut self,
        list: &[ProcessInfo],
        mem_peak: u64,
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
            self.walk_tree(root, 0, &by_pid, &kids, mem_peak, &mut out, cx);
        }
        out
    }

    fn walk_tree(
        &self,
        pid: Pid,
        depth: u32,
        by_pid: &HashMap<Pid, ProcessInfo>,
        kids: &HashMap<Pid, Vec<Pid>>,
        mem_peak: u64,
        out: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        let Some(proc_) = by_pid.get(&pid) else {
            return;
        };
        let has_kids = kids.get(&pid).map(|k| !k.is_empty()).unwrap_or(false);
        out.push(
            self.process_row(proc_, depth, has_kids, mem_peak, cx)
                .into_any_element(),
        );
        if has_kids && self.expanded.contains(&pid) {
            if let Some(children) = kids.get(&pid) {
                for child in children {
                    self.walk_tree(*child, depth + 1, by_pid, kids, mem_peak, out, cx);
                }
            }
        }
    }

    fn process_row(
        &self,
        p: &ProcessInfo,
        depth: u32,
        has_kids: bool,
        mem_peak: u64,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.selected_pid == Some(p.pid);
        let pid = p.pid;
        let name = p.name.clone();
        let expanded = self.expanded.contains(&pid);
        let indent = px(14.0 * depth as f32);
        let cpu_bg = theme::cpu_heat(p.cpu_percent);
        let mem_bg = theme::mem_heat(p.memory_bytes, mem_peak);
        let path = p.exe_path.clone().unwrap_or_default();

        div()
            .id(ElementId::Name(format!("proc-{pid}").into()))
            .h(px(theme::ROW_HEIGHT))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(theme::BORDER_SUBTLE)
            .bg(if selected {
                theme::SELECTED
            } else {
                theme::PANEL_BG
            })
            .hover(|s| s.bg(theme::HOVER))
            .cursor_pointer()
            // Tree toggle — fixed width so columns stay aligned.
            .child(
                div()
                    .w(px(theme::COL_TREE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(if has_kids {
                        div()
                            .id(ElementId::Name(format!("exp-{pid}").into()))
                            .size(px(theme::COL_TREE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(theme::RADIUS_SM))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::HOVER))
                            .child(
                                svg()
                                    .path(if expanded {
                                        "icons/ui/chevron-down.svg"
                                    } else {
                                        "icons/ui/chevron-right.svg"
                                    })
                                    .size(px(14.))
                                    .text_color(theme::TEXT_MUTED)
                                    .flex_shrink_0(),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if !this.expanded.insert(pid) {
                                    this.expanded.remove(&pid);
                                }
                                cx.notify();
                            }))
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    }),
            )
            // Name — fixed width; tree indent only inside this cell.
            .child(
                div()
                    .w(px(theme::COL_NAME))
                    .min_w(px(theme::COL_NAME))
                    .max_w(px(theme::COL_NAME))
                    .pl(indent)
                    .overflow_hidden()
                    .text_sm()
                    .text_color(theme::TEXT)
                    .whitespace_nowrap()
                    .child(name.clone()),
            )
            .child(
                div()
                    .w(px(theme::COL_PID))
                    .min_w(px(theme::COL_PID))
                    .flex()
                    .justify_end()
                    .text_sm()
                    .text_color(theme::TEXT_MUTED)
                    .font_family("Consolas")
                    .child(pid.to_string()),
            )
            .child(
                div()
                    .w(px(theme::COL_CPU))
                    .min_w(px(theme::COL_CPU))
                    .h(px(22.))
                    .px_1()
                    .rounded(px(2.))
                    .bg(cpu_bg)
                    .flex()
                    .items_center()
                    .justify_end()
                    .text_sm()
                    .font_family("Consolas")
                    .child(format!("{:.1}%", p.cpu_percent)),
            )
            .child(
                div()
                    .w(px(theme::COL_MEM))
                    .min_w(px(theme::COL_MEM))
                    .h(px(22.))
                    .px_1()
                    .rounded(px(2.))
                    .bg(mem_bg)
                    .flex()
                    .items_center()
                    .justify_end()
                    .text_sm()
                    .font_family("Consolas")
                    .child(format_bytes(p.memory_bytes)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(80.))
                    .overflow_hidden()
                    .text_xs()
                    .text_color(theme::TEXT_MUTED)
                    .whitespace_nowrap()
                    .child(path),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.selected_pid != Some(pid) {
                    this.detail_copy = None;
                }
                this.selected_pid = Some(pid);
                this.detail_open = true;
                this.store.update(cx, |s, cx| s.load_process_files(pid, cx));
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, _, cx| {
                    if this.selected_pid != Some(pid) {
                        this.detail_copy = None;
                    }
                    this.selected_pid = Some(pid);
                    this.detail_open = true;
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
        let filtered = filter_sockets_unified(
            &snap.sockets,
            &self.port_query.text,
            self.port_proto,
            |pid| name_by_pid.get(&pid).cloned(),
        );

        let tools = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(text_input_box(
                "port-q",
                &self.port_query,
                "搜索端口、地址或进程名",
                self.active_field == ActiveField::Port,
                px(280.),
                ActiveField::Port,
                cx,
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
            );

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::PANEL_BG)
            .child(self.render_page_header("端口", tools))
            .child(col_header(&[
                ("", 6.),
                ("协议", 50.),
                ("本地", 150.),
                ("远程", 150.),
                ("状态", 110.),
                ("PID", 72.),
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
                            .h(px(theme::ROW_HEIGHT))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .border_b_1()
                            .border_color(theme::BORDER_SUBTLE)
                            .bg(theme::PANEL_BG)
                            .hover(|s| s.bg(theme::HOVER))
                            .cursor_pointer()
                            .child(
                                div()
                                    .w(px(3.))
                                    .h(px(16.))
                                    .rounded(px(2.))
                                    .bg(if proxy_hl {
                                        theme::WARN
                                    } else {
                                        theme::PANEL_BG
                                    }),
                            )
                            .child(
                                div()
                                    .w(px(50.))
                                    .text_sm()
                                    .text_color(theme::ACCENT)
                                    .child(r.protocol.as_str()),
                            )
                            .child(
                                div()
                                    .w(px(150.))
                                    .text_sm()
                                    .child(r.local.to_string()),
                            )
                            .child(
                                div()
                                    .w(px(150.))
                                    .text_sm()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(remote),
                            )
                            .child(
                                div()
                                    .w(px(110.))
                                    .text_xs()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(r.state.as_str()),
                            )
                            .child(div().w(px(72.)).text_sm().child(pid.to_string()))
                            .child(div().flex_1().text_sm().child(pname))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_pid = Some(pid);
                                this.detail_open = true;
                                this.detail_copy = None;
                                this.store.update(cx, |s, cx| s.load_process_files(pid, cx));
                                cx.notify();
                            }))
                            .into_any_element()
                    })),
            )
    }

    fn render_file_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let search = self.store.read(cx).file_search.clone();
        let name_by_pid: HashMap<Pid, String> = self
            .snapshot(cx)
            .processes
            .iter()
            .map(|p| (p.pid, p.name.clone()))
            .collect();

        let tools = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(text_input_box(
                "file-q",
                &self.file_query,
                "文件/目录路径（建议管理员）",
                self.active_field == ActiveField::File,
                px(420.),
                ActiveField::File,
                cx,
            ))
            .child(
                ToolButton::new(
                    "file-search",
                    "查询占用",
                    cx.listener(|this, _, _, cx| {
                        let q = this.file_query.text.clone();
                        this.store.update(cx, |s, cx| s.search_path_holders(q, cx));
                    }),
                )
                .primary(),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme::TEXT_MUTED)
                    .child(if search.busy {
                        "扫描中…".to_string()
                    } else {
                        format!("{} 命中", search.holders.len())
                    }),
            );

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::PANEL_BG)
            .child(self.render_page_header("文件占用", tools))
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(theme::TEXT_MUTED)
                    .child("回车查询。句柄扫描较慢，结果可能不完整。"),
            )
            .when_some(search.error.clone(), |el, e| {
                el.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(theme::DANGER)
                        .child(e),
                )
            })
            .child(col_header(&[
                ("PID", 70.),
                ("进程", 140.),
                ("打开路径", 0.),
                ("访问", 90.),
            ]))
            .child(
                div()
                    .id("file-holders")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(search.holders.into_iter().map(|h| {
                        let pid = h.pid;
                        let pname = name_by_pid
                            .get(&pid)
                            .cloned()
                            .unwrap_or_else(|| "?".into());
                        div()
                            .id(ElementId::Name(
                                format!("holder-{pid}-{}", h.path).into(),
                            ))
                            .h(px(theme::ROW_HEIGHT))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .border_b_1()
                            .border_color(theme::BORDER_SUBTLE)
                            .hover(|s| s.bg(theme::HOVER))
                            .cursor_pointer()
                            .child(div().w(px(70.)).text_sm().child(pid.to_string()))
                            .child(div().w(px(140.)).text_sm().child(pname))
                            .child(div().flex_1().text_xs().child(h.path))
                            .child(
                                div()
                                    .w(px(90.))
                                    .text_xs()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(h.access.unwrap_or_default()),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_pid = Some(pid);
                                this.detail_open = true;
                                this.detail_copy = None;
                                this.store.update(cx, |s, cx| s.load_process_files(pid, cx));
                                cx.notify();
                            }))
                            .into_any_element()
                    })),
            )
    }

    fn render_dns_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let dns = self.store.read(cx).dns.clone();
        let supports_remove = dns.supports_remove_entry;
        let entries = filter_dns_entries(&dns.entries, &self.dns_query.text);
        let store = self.store.clone();
        let store2 = self.store.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::PANEL_BG)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .h(px(44.))
                    .border_b_1()
                    .border_color(theme::BORDER)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("DNS 缓存"),
                    )
                    .child(div().flex_1())
                    .child(text_input_box(
                        "dns-q",
                        &self.dns_query,
                        "搜索域名 / IP",
                        self.active_field == ActiveField::Dns,
                        px(280.),
                        ActiveField::Dns,
                        cx,
                    ))
                    .child(ToolButton::new("dns-refresh", "刷新", {
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
                            HealthLevel::Ok => theme::OK,
                            HealthLevel::Warn => theme::WARN,
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

    fn render_settings_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let refresh = self.store.read(cx).settings.refresh.clone();
        let path = crate::shared::persist::settings_path()
            .to_string_lossy()
            .to_string();

        let mut presets = div().flex().flex_row().flex_wrap().gap_1();
        for &ms in REFRESH_PRESETS_MS {
            let active = refresh.interval_ms == ms;
            let label = if ms >= 1000 {
                format!("{}s", ms / 1000)
            } else {
                format!("{ms}ms")
            };
            presets = presets.child(ChipButton::new(
                ElementId::Name(format!("preset-{ms}").into()),
                label,
                active,
                cx.listener(move |this, _, _, cx| {
                    this.store.update(cx, |s, cx| s.set_interval_ms(ms, cx));
                }),
            ));
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::PANEL_BG)
            .child(self.render_page_header("设置", div()))
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_4()
                    .py_3()
                    .gap_4()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::TEXT)
                            .child("刷新"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::TEXT_MUTED)
                            .child("控制进程/端口列表的自动采样频率。DNS 与文件占用仍为手动刷新。"),
                    )
                    .child(
                        div()
                            .p_3()
                            .rounded(px(theme::RADIUS_MD))
                            .border_1()
                            .border_color(theme::BORDER)
                            .bg(theme::SIDEBAR_BG)
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme::TEXT)
                                            .child("自动刷新"),
                                    )
                                    .child(div().flex_1())
                                    .child(ChipButton::new(
                                        "settings-auto-on",
                                        "开",
                                        refresh.auto,
                                        cx.listener(|this, _, _, cx| {
                                            this.store.update(cx, |s, cx| s.set_auto(true, cx));
                                        }),
                                    ))
                                    .child(ChipButton::new(
                                        "settings-auto-off",
                                        "关（仅手动 F5）",
                                        !refresh.auto,
                                        cx.listener(|this, _, _, cx| {
                                            this.store.update(cx, |s, cx| s.set_auto(false, cx));
                                        }),
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme::TEXT)
                                            .child("刷新间隔"),
                                    )
                                    .child(div().flex_1())
                                    .child(presets),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(format!(
                                        "当前：{} · {} ms",
                                        if refresh.auto {
                                            "自动"
                                        } else {
                                            "仅手动"
                                        },
                                        refresh.interval_ms
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .mt_2()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::TEXT)
                            .child("关于配置文件"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::TEXT_MUTED)
                            .child(format!("设置已保存到：{path}")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::TEXT_MUTED)
                            .child("后续可在此扩展主题、列显示、代理端口高亮等选项。"),
                    ),
            )
    }

    fn render_detail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let snap = self.snapshot(cx);
        let Some(pid) = self.selected_pid.filter(|_| self.detail_open) else {
            return div().into_any_element();
        };

        let proc_ = snap.processes.iter().find(|p| p.pid == pid);
        let sockets: Vec<_> = snap
            .sockets
            .iter()
            .filter(|s| s.pid == pid)
            .cloned()
            .collect();
        let pf = self.store.read(cx).process_files.clone();
        let store = self.store.clone();
        let store2 = self.store.clone();
        let store3 = self.store.clone();
        let name = proc_
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "?".into());

        div()
            .w_full()
            .h(px(theme::DETAIL_HEIGHT))
            .border_t_1()
            .border_color(theme::BORDER)
            .bg(theme::PANEL_BG)
            .flex()
            .flex_col()
            .child(
                div()
                    .px_3()
                    .h(px(36.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme::BORDER_SUBTLE)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::TEXT)
                            .child(format!("{name}  (PID {pid})")),
                    )
                    .child(div().flex_1())
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
                    )
                    .child(ToolButton::new("load-files", "加载文件", {
                        let store = store3;
                        move |_, _, cx| {
                            store.update(cx, |s, cx| s.load_process_files(pid, cx));
                        }
                    }))
                    .child(
                        div()
                            .id("detail-close")
                            .px_2()
                            .cursor_pointer()
                            .text_color(theme::TEXT_MUTED)
                            .child("✕")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.detail_open = false;
                                this.detail_copy = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("detail-info")
                            .w(px(320.))
                            .h_full()
                            .px_3()
                            .py_2()
                            .overflow_y_scroll()
                            .border_r_1()
                            .border_color(theme::BORDER_SUBTLE)
                            .when_some(proc_.cloned(), |el, p| {
                                let parent = p
                                    .parent_pid
                                    .map(|x| x.to_string())
                                    .unwrap_or_else(|| "-".into());
                                let path = p.exe_path.clone().unwrap_or_else(|| "-".into());
                                let parent_focused =
                                    self.detail_copy == Some(DetailCopyTarget::ParentPid);
                                let path_focused =
                                    self.detail_copy == Some(DetailCopyTarget::Path);
                                el.child(
                                    div()
                                        .gap_1()
                                        .flex()
                                        .flex_col()
                                        .text_sm()
                                        .child(format!("CPU  {:.1}%", p.cpu_percent))
                                        .child(format!(
                                            "内存  {}",
                                            format_bytes(p.memory_bytes)
                                        ))
                                        .child(copyable_detail_row(
                                            "detail-parent",
                                            "父 PID",
                                            &parent,
                                            parent_focused,
                                            if parent_focused {
                                                Some(&self.detail_edit)
                                            } else {
                                                None
                                            },
                                            cx.listener({
                                                let parent = parent.clone();
                                                move |this, _, _, cx| {
                                                    this.focus_detail_copy(
                                                        DetailCopyTarget::ParentPid,
                                                        parent.clone(),
                                                    );
                                                    cx.notify();
                                                }
                                            }),
                                        ))
                                        .child(copyable_detail_row(
                                            "detail-path",
                                            "路径",
                                            &path,
                                            path_focused,
                                            if path_focused {
                                                Some(&self.detail_edit)
                                            } else {
                                                None
                                            },
                                            cx.listener({
                                                let path = path.clone();
                                                move |this, _, _, cx| {
                                                    this.focus_detail_copy(
                                                        DetailCopyTarget::Path,
                                                        path.clone(),
                                                    );
                                                    cx.notify();
                                                }
                                            }),
                                        ))
                                        .child(
                                            div()
                                                .mt_1()
                                                .text_xs()
                                                .text_color(theme::TEXT_MUTED)
                                                .child("点击选中，Ctrl+C 复制"),
                                        ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .flex_col()
                            .px_2()
                            .py_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(format!("网络（{}）", sockets.len())),
                            )
                            .child(
                                div()
                                    .id("detail-socks")
                                    .flex_1()
                                    .overflow_y_scroll()
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
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .flex_col()
                            .px_2()
                            .py_1()
                            .border_l_1()
                            .border_color(theme::BORDER_SUBTLE)
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(if pf.busy {
                                        "文件句柄加载中…".to_string()
                                    } else if pf.pid == Some(pid) {
                                        format!("打开文件（{}）· 点击选中 Ctrl+C 复制", pf.files.len())
                                    } else {
                                        "打开文件".into()
                                    }),
                            )
                            .when_some(pf.error.clone(), |el, e| {
                                el.child(div().text_xs().text_color(theme::DANGER).child(e))
                            })
                            .child(
                                div()
                                    .id("detail-files")
                                    .flex_1()
                                    .overflow_y_scroll()
                                    .children({
                                        let files = if pf.pid == Some(pid) {
                                            pf.files
                                        } else {
                                            Vec::new()
                                        };
                                        files.into_iter().enumerate().map(|(idx, f)| {
                                            let focused = self.detail_copy
                                                == Some(DetailCopyTarget::OpenFile(idx));
                                            let path = f.path;
                                            div()
                                                .id(ElementId::Name(
                                                    format!("detail-file-{idx}").into(),
                                                ))
                                                .py_1()
                                                .px_1()
                                                .rounded(px(theme::RADIUS_SM))
                                                .text_xs()
                                                .cursor_text()
                                                .bg(if focused {
                                                    theme::SELECTED
                                                } else {
                                                    theme::PANEL_BG
                                                })
                                                .hover(|s| {
                                                    if focused {
                                                        s
                                                    } else {
                                                        s.bg(theme::HOVER)
                                                    }
                                                })
                                                .child(if focused {
                                                    self.detail_edit
                                                        .render_content(true, "")
                                                } else {
                                                    div()
                                                        .text_color(theme::TEXT)
                                                        .child(path.clone())
                                                        .into_any_element()
                                                })
                                                .on_click(cx.listener({
                                                    let path = path.clone();
                                                    move |this, _, _, cx| {
                                                        this.focus_detail_copy(
                                                            DetailCopyTarget::OpenFile(idx),
                                                            path.clone(),
                                                        );
                                                        cx.notify();
                                                    }
                                                }))
                                                .into_any_element()
                                        })
                                    }),
                            ),
                    ),
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

        if children.is_empty() {
            return div().into_any_element();
        }
        // Cover the whole workspace (parent must be `.relative()`), not a flex-row sibling.
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
                a: 0.35,
            })
            .children(children)
            .into_any_element()
    }

    fn active_edit_mut(&mut self) -> &mut TextEdit {
        match self.active_field {
            ActiveField::Process => &mut self.process_query,
            ActiveField::Port => &mut self.port_query,
            ActiveField::File => &mut self.file_query,
            ActiveField::Dns => &mut self.dns_query,
        }
    }

    fn clear_active(&mut self) {
        self.active_edit_mut().clear();
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
            MainPane::Settings => self.render_settings_pane(cx).into_any_element(),
        };

        let show_detail = self.detail_open
            && self.selected_pid.is_some()
            && matches!(
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
            .on_action(cx.listener(|this, _: &ShowSettings, _, cx| {
                this.pane = MainPane::Settings;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if this.detail_copy.is_some() && this.detail_open {
                    if event.keystroke.key == "escape" {
                        this.detail_copy = None;
                        cx.notify();
                        return;
                    }
                    if this.detail_edit.handle_key_readonly(event, cx) {
                        cx.notify();
                        return;
                    }
                    // Don't fall through to search typing while a detail value is focused.
                    if event.keystroke.key_char.is_some()
                        || event.keystroke.key.as_str() == "space"
                    {
                        return;
                    }
                }

                if this.active_field == ActiveField::File && event.keystroke.key == "enter" {
                    let q = this.file_query.text.clone();
                    this.store.update(cx, |s, cx| s.search_path_holders(q, cx));
                    cx.notify();
                    return;
                }

                if event.keystroke.key == "escape" {
                    if this.detail_copy.is_some() {
                        this.detail_copy = None;
                        cx.notify();
                        return;
                    }
                    if this.detail_open {
                        this.detail_open = false;
                        this.detail_copy = None;
                        cx.notify();
                        return;
                    }
                    this.clear_active();
                    cx.notify();
                    return;
                }

                if this.active_edit_mut().handle_key(event, cx) {
                    this.active_edit_mut().caret_visible = true;
                    this.start_caret_blink(cx);
                    cx.notify();
                    return;
                }
                // Printable text (incl. Chinese) comes via EntityInputHandler / WM_CHAR.
            }))
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(theme::BG)
            .text_color(theme::TEXT)
            .font_family(theme::FONT_UI)
            .child(
                canvas(
                    {
                        let entity = cx.entity();
                        move |bounds, _, cx| {
                            entity.update(cx, |this, _| {
                                // Keep a fallback bounds for IME candidate positioning.
                                if this.input_bounds.is_none() {
                                    this.input_bounds = Some(bounds);
                                }
                            });
                            bounds
                        }
                    },
                    {
                        let entity = cx.entity();
                        move |_bounds, _, window, cx| {
                            let focus = entity.read(cx).focus.clone();
                            let input_bounds = entity
                                .read(cx)
                                .input_bounds
                                .unwrap_or(_bounds);
                            window.handle_input(
                                &focus,
                                ElementInputHandler::new(input_bounds, entity.clone()),
                                cx,
                            );
                        }
                    },
                )
                .absolute()
                .size(px(0.)),
            )
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(div().flex_1().overflow_hidden().child(main))
                    .when(show_detail, |el| el.child(self.render_detail(cx)))
                    .child(self.render_status_bar(cx)),
            )
            .child(self.render_modals(cx))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if !this.input_selecting || this.detail_copy.is_some() {
                    return;
                }
                let idx = this.index_at_pointer(event.position);
                this.active_edit_mut().set_caret(idx, true);
                this.active_edit_mut().caret_visible = true;
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.input_selecting {
                        this.input_selecting = false;
                        cx.notify();
                    }
                }),
            )
    }
}

impl EntityInputHandler for WorkspaceView {
    fn text_for_range(
        &mut self,
        range: std::ops::Range<usize>,
        adjusted_range: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let edit = if self.detail_copy.is_some() {
            &self.detail_edit
        } else {
            self.active_edit()
        };
        let start = TextEdit::utf16_to_char(&edit.text, range.start);
        let end = TextEdit::utf16_to_char(&edit.text, range.end);
        *adjusted_range = Some(
            TextEdit::char_to_utf16(&edit.text, start)..TextEdit::char_to_utf16(&edit.text, end),
        );
        Some(edit.text.chars().skip(start).take(end.saturating_sub(start)).collect())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let edit = if self.detail_copy.is_some() {
            &self.detail_edit
        } else {
            self.active_edit()
        };
        Some(UTF16Selection {
            range: edit.selection_utf16(),
            reversed: edit.cursor < edit.anchor,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        let edit = if self.detail_copy.is_some() {
            &self.detail_edit
        } else {
            self.active_edit()
        };
        edit.marked.map(|(lo, hi)| {
            TextEdit::char_to_utf16(&edit.text, lo)..TextEdit::char_to_utf16(&edit.text, hi)
        })
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if self.detail_copy.is_some() {
            self.detail_edit.unmark();
        } else {
            self.active_edit_mut().unmark();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.detail_copy.is_some() {
            return;
        }
        self.active_edit_mut().replace_utf16_range(range, text);
        self.start_caret_blink(cx);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        new_text: &str,
        new_selected_range: Option<std::ops::Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.detail_copy.is_some() {
            return;
        }
        self.active_edit_mut()
            .replace_and_mark_utf16(range, new_text, new_selected_range);
        self.start_caret_blink(cx);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: std::ops::Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(self.input_bounds.unwrap_or(element_bounds))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let edit = self.active_edit();
        let idx = self.index_at_pointer(point);
        Some(TextEdit::char_to_utf16(&edit.text, idx))
    }
}

fn nav_item(
    this: &WorkspaceView,
    label: &'static str,
    pane: MainPane,
    cx: &mut Context<WorkspaceView>,
) -> NavItem {
    NavItem::new(
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
                MainPane::Settings => ActiveField::Process,
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

fn text_input_box(
    id: impl Into<ElementId>,
    edit: &TextEdit,
    placeholder: &str,
    active: bool,
    width: Pixels,
    field: ActiveField,
    cx: &mut Context<WorkspaceView>,
) -> impl IntoElement {
    let entity = cx.entity();
    div()
        .id(id)
        .w(width)
        .h(px(28.))
        .px_2()
        .rounded(px(theme::RADIUS_SM))
        .bg(theme::PANEL_BG)
        .border_1()
        .border_color(if active {
            theme::ACCENT
        } else {
            theme::BORDER
        })
        .flex()
        .items_center()
        .min_w_0()
        .overflow_hidden()
        .text_sm()
        .cursor_text()
        .relative()
        .child(edit.render_content(active, placeholder))
        .child(
            canvas(
                {
                    let entity = entity.clone();
                    move |bounds, _, cx| {
                        entity.update(cx, |this, _| {
                            if this.active_field == field {
                                this.input_bounds = Some(bounds);
                            }
                        });
                        bounds
                    }
                },
                |_bounds, _, _, _| {},
            )
            .absolute()
            .inset_0()
            .size_full(),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                this.detail_copy = None;
                this.active_field = field;
                this.focus.focus(window);
                this.start_caret_blink(cx);
                if event.click_count >= 2 {
                    this.active_edit_mut().select_all();
                    this.input_selecting = false;
                } else {
                    // Ensure bounds exist for this field before hit-test.
                    let idx = this.index_at_pointer(event.position);
                    this.active_edit_mut()
                        .set_caret(idx, event.modifiers.shift);
                    this.input_selecting = true;
                }
                cx.notify();
            }),
        )
}

fn copyable_detail_row(
    id: impl Into<ElementId>,
    label: &str,
    value: &str,
    focused: bool,
    edit: Option<&TextEdit>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_col()
        .gap_0()
        .py_1()
        .px_1()
        .rounded(px(theme::RADIUS_SM))
        .cursor_text()
        .bg(if focused {
            theme::SELECTED
        } else {
            theme::PANEL_BG
        })
        .hover(|s| {
            if focused {
                s
            } else {
                s.bg(theme::HOVER)
            }
        })
        .child(
            div()
                .text_xs()
                .text_color(theme::TEXT_MUTED)
                .child(label.to_string()),
        )
        .child(
            div().min_w_0().overflow_hidden().text_sm().child(
                if focused {
                    edit.map(|e| e.render_content(true, ""))
                        .unwrap_or_else(|| {
                            div().text_color(theme::TEXT).child(value.to_string()).into_any_element()
                        })
                } else {
                    div()
                        .text_color(theme::TEXT)
                        .child(value.to_string())
                        .into_any_element()
                },
            ),
        )
        .on_click(on_click)
}

fn sortable_header_cell(
    id: impl Into<ElementId>,
    label: &str,
    width: f32,
    end_align: bool,
    active: bool,
    dir: SortDir,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let mark = if active {
        match dir {
            SortDir::Asc => " ↑",
            SortDir::Desc => " ↓",
        }
    } else {
        ""
    };
    let mut cell = div()
        .id(id)
        .w(px(width))
        .min_w(px(width))
        .h_full()
        .flex()
        .items_center()
        .cursor_pointer()
        .hover(|s| s.text_color(theme::TEXT))
        .text_color(if active {
            theme::ACCENT
        } else {
            theme::TEXT_MUTED
        })
        .child(format!("{label}{mark}"))
        .on_click(on_click);
    if end_align {
        cell = cell.justify_end();
    }
    cell
}

fn col_header(cols: &[(&str, f32)]) -> impl IntoElement {
    let owned: Vec<(String, f32)> = cols
        .iter()
        .map(|(label, w)| ((*label).to_string(), *w))
        .collect();
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(28.))
        .px_3()
        .gap_2()
        .border_b_1()
        .border_color(theme::BORDER)
        .bg(theme::SIDEBAR_BG)
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
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
        .w(px(400.))
        .p_4()
        .rounded(px(theme::RADIUS_MD))
        .bg(theme::PANEL_BG)
        .border_1()
        .border_color(theme::BORDER)
        .shadow_md()
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
