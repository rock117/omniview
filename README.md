# Omniview

跨平台操作系统资源查看与管理：进程、端口、文件占用、DNS 缓存、代理排障等。

- UI：[GPUI](https://github.com/zed-industries/zed)（Rust）
- 设计：跨平台抽象；**当前仅实现 Windows**
- 需求：[docs/REQUIREMENTS.md](docs/REQUIREMENTS.md)
- 架构：[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)

## 开发

```bash
cargo run
```

代理排障与 DNS 清空等写操作可能需要相应权限。

## 许可

MIT
