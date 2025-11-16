#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, RunEvent};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

#[tauri::command]
fn toggle_fullscreen(window: tauri::Window) {
    if let Ok(is_fullscreen) = window.is_fullscreen() {
        let _ = window.set_fullscreen(!is_fullscreen);
    }
}

fn send_shutdown_command(process: &mut CommandChild) -> Result<(), String> {
    process
        .write(b"sidecar shutdown\n")
        .map_err(|e| format!("Failed to write to sidecar stdin: {}", e))
}

fn kill_process(process: CommandChild) {
    if let Err(e) = process.kill() {
        eprintln!("[tauri] Failed to kill sidecar process: {}", e);
    } else {
        println!("[tauri] Sidecar process killed");
    }
}

fn spawn_and_monitor_sidecar(app_handle: tauri::AppHandle) -> Result<(), String> {
    if let Some(state) = app_handle.try_state::<Arc<Mutex<Option<CommandChild>>>>() {
        if state.lock().unwrap().is_some() {
            println!("[tauri] Sidecar is already running, skipping spawn");
            return Ok(());
        }
    }

    let port = portpicker::pick_unused_port().expect("Failed to find a free port");
    println!("[tauri] Selected port: {}", port);

    let window = app_handle
        .get_webview_window("main")
        .ok_or("Failed to get main window")?;

    let sidecar_command = app_handle
        .shell()
        .sidecar("open-coreui")
        .map_err(|e| format!("Failed to create sidecar command: {}", e))?
        .env("PORT", port.to_string());

    let (mut rx, child) = sidecar_command
        .spawn()
        .map_err(|e| format!("Failed to spawn sidecar: {}", e))?;

    if let Some(state) = app_handle.try_state::<Arc<Mutex<Option<CommandChild>>>>() {
        *state.lock().unwrap() = Some(child);
    } else {
        return Err("Failed to access app state".to_string());
    }

    let app_handle_clone = app_handle.clone();

    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes);
                    println!("[sidecar stdout] {}", line);

                    if line.contains("Server running at") {
                        let url = format!("http://localhost:{}", port);
                        println!("[tauri] Server ready, navigating to: {}", url);
                        let _ = window.eval(&format!("window.location.href = '{}'", url));
                    }

                    let _ = app_handle_clone.emit("sidecar-stdout", line.to_string());
                }
                CommandEvent::Stderr(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes);
                    eprintln!("[sidecar stderr] {}", line);
                    let _ = app_handle_clone.emit("sidecar-stderr", line.to_string());
                }
                CommandEvent::Terminated(payload) => {
                    println!(
                        "[tauri] Sidecar process terminated with code: {:?}",
                        payload.code
                    );

                    if let Some(state) =
                        app_handle_clone.try_state::<Arc<Mutex<Option<CommandChild>>>>()
                    {
                        *state.lock().unwrap() = None;
                    }

                    let _ = app_handle_clone.emit("sidecar-terminated", payload.code);
                }
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

#[tauri::command]
fn start_sidecar(app_handle: tauri::AppHandle) -> Result<String, String> {
    println!("[tauri] Received command to start sidecar");
    spawn_and_monitor_sidecar(app_handle)?;
    Ok("Sidecar spawned and monitoring started".to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_websocket::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            app.manage(Arc::new(Mutex::new(None::<CommandChild>)));

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
