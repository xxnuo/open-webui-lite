<div align="center">
  <img src="./assets/banner.png" alt="Open WebUI Lite" height="100">
</div>

<div align="center">

[![GitHub Stars](https://img.shields.io/github/stars/xxnuo/open-webui-lite?style=flat-square&logo=github&color=yellow)](https://github.com/xxnuo/open-webui-lite/stargazers)
[![GitHub Release](https://img.shields.io/github/v/release/xxnuo/open-webui-lite?style=flat-square&logo=github&color=green)](https://github.com/xxnuo/open-webui-lite/releases/latest)
[![GitHub Downloads](https://img.shields.io/github/downloads/xxnuo/open-webui-lite/total?style=flat-square&logo=github&color=orange)](https://github.com/xxnuo/open-webui-lite/releases)
[![Build Status](https://img.shields.io/github/actions/workflow/status/xxnuo/open-webui-lite/build.yml?style=flat-square&logo=github-actions&logoColor=white)](https://github.com/xxnuo/open-webui-lite/actions)

</div>

<div align="center">
  <h1>
    Open WebUI Lite
  </h1>
</div>

<img src="./assets/icon.png" alt="Open WebUI Lite" align="right" height="128">

[English](README.md) | [中文](README.zh.md)

Open WebUI Lite, a lightweight implementation of Open WebUI

## Features

- Single executable download to get started
- Use original frontend
- No Docker, Python, PostgreSQL, Redis dependencies
- Lower memory footprint
- Lower hardware requirements
- Better performance with Rust backend
  - Faster response time 10-50x
  - Lower memory usage 70%

## Dependencies

### Original Frontend

> [open-webui/open-webui](https://github.com/open-webui/open-webui)

#### Changes:

- Adapt to Rust Lite backend

### Rust Backend

> [knoxchat/open-webui-rust](https://github.com/knoxchat/open-webui-rust)

Based on [knox's](https://github.com/knoxchat) Rust backend

#### Changes:

- Add SQLite database support
- Remove Postgres database dependencies

> You can support [knox's](https://github.com/knoxchat/open-webui-rust) backend development if you find it useful.
