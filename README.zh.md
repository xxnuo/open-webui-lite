<div align="center">
  <img src="./assets/banner.png" alt="Open CoreUI" height="100">
</div>

<div align="center">

[![GitHub Stars](https://img.shields.io/github/stars/xxnuo/open-coreui?style=flat-square&logo=github&color=yellow)](https://github.com/xxnuo/open-coreui/stargazers)
[![GitHub Release](https://img.shields.io/github/v/release/xxnuo/open-coreui?style=flat-square&logo=github&color=green)](https://github.com/xxnuo/open-coreui/releases/latest)
[![GitHub Downloads](https://img.shields.io/github/downloads/xxnuo/open-coreui/total?style=flat-square&logo=github&color=orange)](https://github.com/xxnuo/open-coreui/releases)
[![Build Status](https://img.shields.io/github/actions/workflow/status/xxnuo/open-coreui/build.yml?style=flat-square&logo=github-actions&logoColor=white)](https://github.com/xxnuo/open-coreui/actions)

</div>

<div align="center">
  <h1>
    Open CoreUI
  </h1>
</div>

<img src="./assets/icon.png" alt="Open CoreUI" align="right" height="128">

[English](README.md) | [中文](README.zh.md)

Open WebUI 轻量版，一个轻量级的 Open WebUI 实现

> **⚠️ 早期开发阶段**  
> 本项目目前处于早期开发阶段，目前仅实现了基本的对话功能，其他功能正在逐步开发中。
>
> **这是一个 Open WebUI v0.6.32 的轻量版重写，不是官方版本。**

<img src="./assets/preview.png" alt="Open CoreUI Preview">

## 特性

- 下载一个可执行文件即可使用的桌面客户端
- 使用原版前端
- 无需安装 Docker、Python、PostgreSQL、Redis 等外部依赖环境的服务端
- 较原版内存占用更低(很多)
- 较原版配置要求更低
- 使用 Rust 后端性能更优

## 下载使用

支持 Windows、macOS、Linux 系统，支持 x86_64、aarch64 架构。

前往 [Releases](https://github.com/xxnuo/open-coreui/releases/latest) 页面下载适合你系统的版本。

### 两种客户端类型说明

本项目提供两种**完全独立**的客户端，根据使用场景选择其一使用：

#### 1. 桌面应用 (Desktop Application)

**适用场景**：个人电脑使用，提供原生窗口界面

**特点**：
- 开箱即用，双击安装即可
- 独立运行，不依赖服务端
- 原生窗口体验

#### 2. 服务端 (Backend Server / CLI)

**适用场景**：服务器部署，通过浏览器访问

**特点**：
- 命令行启动，通过浏览器访问
- 独立运行，不依赖桌面端
- 适合服务器部署和多用户访问

### 使用说明

**桌面端**：直接安装后打开应用即可使用

> **macOS 用户注意**：如果打开时提示"应用已损坏"等问题，请打开`终端`并执行以下命令：
> 
> ```bash
> sudo xattr -d com.apple.quarantine "/Applications/Open CoreUI Desktop.app"
> ```

**服务端**：
1. 下载对应系统的二进制文件
2. 赋予执行权限（Linux/macOS）：`chmod +x open-coreui-*`
3. 运行：`./open-coreui-*`
4. 在浏览器中访问显示的地址（通常是 `http://localhost:8168`）

详细的配置选项和环境变量说明请参考 [CLI 文档](CLI.md)。

## 依赖库

### 原版项目

> [open-webui/open-webui](https://github.com/open-webui/open-webui)

### Rust 后端

> [xxnuo/open-webui-rust](https://github.com/xxnuo/open-webui-rust)

基于 [knox](https://github.com/knoxchat) 大佬的 Rust 后端实现

#### 改动:

- 添加了桌面客户端的支持
- 添加了 SQLite 数据库的支持
- 移除了对 Postgres 数据库的依赖
- 移除了对 Redis 的依赖

> 有能力的朋友可以去[赞助支持大佬](https://github.com/knoxchat/open-webui-rust)开发的后端。
