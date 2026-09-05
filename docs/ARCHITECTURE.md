# Omniview 架构

跨平台 OS 资源查看与管理工具。学习 Zed 的 Entity / 后台任务等思路；参考 Loom 文档中的 **GPUI 避坑**（UI 线程、锁、输入、关窗），不复用 Loom 产品结构。勿拷贝 Zed GPL 源码。

## 目标

| 目标 | 含义 |
|------|------|
| 跨平台 | `domain` + `platform` trait；OS 细节在 `platform/*` |
| 可扩展 | 新资源 = 新 Probe + 可选并入快照/独立面板 |
| Windows 先行 | 完整实现；其它 OS stub / `Unsupported` |
| UI 不卡死 | 采样、DNS、代理查询均在后台；UI 只吃快照 |

## 分层

```
ui/        → 进程 | 端口 | 文件 | DNS | 代理排障
collect/   → 刷新调度、SystemSnapshot、按需 DNS/代理采集
domain/    → 纯数据与过滤排序
platform/  → ProcessProbe / NetProbe / HandleProbe
             DnsCacheProbe / ProxyProbe
```

## 平台抽象（扩展）

```rust
ProcessProbe   // list / kill
NetProbe       // list_sockets
HandleProbe    // open_files / holders_of_path
DnsCacheProbe  // list / flush / remove_entry?
ProxyProbe     // system_proxy / winhttp / env_proxy / set_system_proxy_off / reset_winhttp
```

工厂：`platform::services()` 按 `cfg(target_os)` 组装。

## 模块与入口

```
进程     SnapshotStore.processes
端口     SnapshotStore.sockets + 进程名关联 + 过滤
文件     HandleProbe（按需）
DNS      DnsCacheProbe（按需刷新，非高频）
代理排障 ProxyProbe + sockets 派生体检项
```

## UI 线程规则（摘要）

1. `Render` / 点击里不做系统枚举  
2. 短锁快照再画  
3. 高频刷新合并 `notify`  
4. 关窗路径不做重型枚举  

详见需求 [REQUIREMENTS.md](./REQUIREMENTS.md)、任务 [BACKLOG.md](./BACKLOG.md)。
