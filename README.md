<img src="./assets/banner.png" alt="Open WebUI Lite" align="right" height="128">

# Open WebUI Lite

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
