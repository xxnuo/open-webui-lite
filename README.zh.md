<div align="center">
  <img src="./assets/banner.png" alt="Open WebUI Lite" height="100">
</div>

# Open WebUI Lite

[English](README.md) | [中文](README.zh.md)

Open WebUI 轻量版，一个轻量级的 Open WebUI 实现

## 特性

- 下载一个可执行文件即可使用
- 使用原版前端
- 无需 Docker、Python、PostgreSQL、Redis 等依赖环境
- 较原版内存占用更低
- 配置要求更低
- 使用 Rust 后端性能更优
  - 更快的响应时间 10-50 倍
  - 更低的内存使用率 70%

## 依赖库

### 原版前端

> [open-webui/open-webui](https://github.com/open-webui/open-webui)

#### 改动:

- 适配 Rust Lite 后端

### Rust 后端

> [knoxchat/open-webui-rust](https://github.com/knoxchat/open-webui-rust)

基于 [knox](https://github.com/knoxchat) 大佬的 Rust 后端实现

#### 改动:

- 添加了 SQLite 数据库的支持
- 移除了对 Postgres 数据库的依赖

> 有能力的朋友可以去[赞助支持大佬](https://github.com/knoxchat/open-webui-rust)开发的后端。
