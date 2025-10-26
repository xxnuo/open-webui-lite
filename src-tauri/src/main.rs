// 防止在 Windows 发布版本中显示额外的控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, RunEvent};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

/// 切换窗口全屏状态
#[tauri::command]
fn toggle_fullscreen(window: tauri::Window) {
    if let Ok(is_fullscreen) = window.is_fullscreen() {
        let _ = window.set_fullscreen(!is_fullscreen);
    }
}

/// 发送关闭命令到 sidecar 进程
fn send_shutdown_command(process: &mut CommandChild) -> Result<(), String> {
    process
        .write(b"sidecar shutdown\n")
        .map_err(|e| format!("Failed to write to sidecar stdin: {}", e))
}

/// 强制终止 sidecar 进程
fn kill_process(process: CommandChild) {
    if let Err(e) = process.kill() {
        eprintln!("[tauri] Failed to kill sidecar process: {}", e);
    } else {
        println!("[tauri] Sidecar process killed");
    }
}

/// 生成并监控 sidecar 进程
fn spawn_and_monitor_sidecar(app_handle: tauri::AppHandle) -> Result<(), String> {
    // 检查 sidecar 进程是否已经在运行
    if let Some(state) = app_handle.try_state::<Arc<Mutex<Option<CommandChild>>>>() {
        if state.lock().unwrap().is_some() {
            println!("[tauri] Sidecar is already running, skipping spawn");
            return Ok(());
        }
    }

    // 选择一个未使用的端口
    let port = portpicker::pick_unused_port().expect("Failed to find a free port");
    println!("[tauri] Selected port: {}", port);

    // 获取主窗口
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Failed to get main window")?;

    // 创建并启动 sidecar 进程
    let sidecar_command = app_handle
        .shell()
        .sidecar("open-webui-lite")
        .map_err(|e| format!("Failed to create sidecar command: {}", e))?
        .env("PORT", port.to_string());

    let (mut rx, child) = sidecar_command
        .spawn()
        .map_err(|e| format!("Failed to spawn sidecar: {}", e))?;

    // 将子进程存储到应用状态中
    if let Some(state) = app_handle.try_state::<Arc<Mutex<Option<CommandChild>>>>() {
        *state.lock().unwrap() = Some(child);
    } else {
        return Err("Failed to access app state".to_string());
    }

    let app_handle_clone = app_handle.clone();

    // 启动异步任务处理 sidecar 通信
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                // 处理标准输出
                CommandEvent::Stdout(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes);
                    println!("[sidecar stdout] {}", line);

                    // 检测服务器是否就绪并导航到对应 URL
                    if line.contains("Server running at") {
                        let url = format!("http://localhost:{}", port);
                        println!("[tauri] Server ready, navigating to: {}", url);
                        let _ = window.eval(&format!("window.location.href = '{}'", url));
                    }

                    // 向前端发送标准输出事件
                    let _ = app_handle_clone.emit("sidecar-stdout", line.to_string());
                }
                // 处理标准错误输出
                CommandEvent::Stderr(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes);
                    eprintln!("[sidecar stderr] {}", line);
                    let _ = app_handle_clone.emit("sidecar-stderr", line.to_string());
                }
                // 处理进程终止事件
                CommandEvent::Terminated(payload) => {
                    println!(
                        "[tauri] Sidecar process terminated with code: {:?}",
                        payload.code
                    );

                    // 清除存储的子进程
                    if let Some(state) =
                        app_handle_clone.try_state::<Arc<Mutex<Option<CommandChild>>>>()
                    {
                        *state.lock().unwrap() = None;
                    }

                    let _ = app_handle_clone.emit("sidecar-terminated", payload.code);
                }
                // 处理错误事件
                CommandEvent::Error(error) => {
                    eprintln!("[tauri] Sidecar error: {}", error);
                    let _ = app_handle_clone.emit("sidecar-error", error);
                }
                _ => {}
            }
        }
        println!("[tauri] Sidecar event loop ended");
    });

    Ok(())
}

/// 关闭 sidecar 进程命令
#[tauri::command]
fn shutdown_sidecar(app_handle: tauri::AppHandle) -> Result<String, String> {
    println!("[tauri] Received command to shutdown sidecar");

    if let Some(state) = app_handle.try_state::<Arc<Mutex<Option<CommandChild>>>>() {
        let mut child_process = state
            .lock()
            .map_err(|_| "Failed to acquire lock on sidecar process")?;

        if let Some(mut process) = child_process.take() {
            match send_shutdown_command(&mut process) {
                Ok(_) => {
                    println!("[tauri] Sent shutdown command to sidecar");
                    Ok("Shutdown command sent".to_string())
                }
                Err(err) => {
                    // 如果关闭失败，恢复进程引用
                    *child_process = Some(process);
                    Err(err)
                }
            }
        } else {
            Err("No active sidecar process to shutdown".to_string())
        }
    } else {
        Err("Sidecar process state not found".to_string())
    }
}

/// 启动 sidecar 进程命令
#[tauri::command]
fn start_sidecar(app_handle: tauri::AppHandle) -> Result<String, String> {
    println!("[tauri] Received command to start sidecar");
    spawn_and_monitor_sidecar(app_handle)?;
    Ok("Sidecar spawned and monitoring started".to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_websocket::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // 初始化 sidecar 进程状态
            app.manage(Arc::new(Mutex::new(None::<CommandChild>)));

            // 在应用启动时启动 sidecar
            let app_handle = app.handle().clone();
            println!("[tauri] Creating sidecar...");
            if let Err(e) = spawn_and_monitor_sidecar(app_handle) {
                eprintln!("[tauri] Failed to spawn sidecar: {}", e);
            } else {
                println!("[tauri] Sidecar spawned and monitoring started");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_sidecar,
            shutdown_sidecar,
            toggle_fullscreen
        ])
        .build(tauri::generate_context!())
        .expect("Error while running tauri application")
        .run(|app_handle, event| {
            if let RunEvent::ExitRequested { .. } = event {
                // 应用退出时确保 sidecar 被正确关闭
                if let Some(state) = app_handle.try_state::<Arc<Mutex<Option<CommandChild>>>>() {
                    if let Ok(mut child) = state.lock() {
                        if let Some(mut process) = child.take() {
                            match send_shutdown_command(&mut process) {
                                Ok(_) => println!("[tauri] Sent shutdown command to sidecar"),
                                Err(e) => {
                                    eprintln!("[tauri] Failed to send shutdown command: {}", e);
                                    kill_process(process);
                                }
                            }
                        }
                    }
                }
            }
        });
}
