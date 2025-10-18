# Open WebUI Slim

[English](README.md) | [中文](README.zh.md)

Open WebUI 轻量版，一个轻量级的 Open WebUI 实现。

## 特性

- 低内存占用
- 低配置要求
- 下载一个可执行文件即可启动

## 依赖库

### 原版前端

> [open-webui/open-webui](https://github.com/open-webui/open-webui)

#### 改动:

- 保留核心功能
- 移除了不必要的内容

### Rust 后端

> [knoxchat/open-webui-rust](https://github.com/knoxchat/open-webui-rust)

基于 [knox](https://github.com/knoxchat) 大佬的 Rust 后端实现

#### 改动:

- 移除了数据库的依赖
- 移除了 Redis 的依赖

> 有能力的朋友可以去[赞助支持大佬](https://github.com/knoxchat/open-webui-rust)开发的后端。
