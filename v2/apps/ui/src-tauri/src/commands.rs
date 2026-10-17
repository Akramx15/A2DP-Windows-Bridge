//! The commands the frontend calls (`invoke`). They check their arguments:
//! the webview is the less trusted side, and opening URLs or paths is
//! limited to what the screens need.

use std::path::Path;
use std::sync::Arc;

use ipc::ErrorInfo;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State, WebviewWindow};

use crate::connection::{Ipc, Status};
use crate::windows;

/// Sends a request to the core -> its result, or the core's ErrorInfo
/// (codes "ipc.*" when the connection fails)
#[tauri::command]
pub async fn ipc_request(
    window: WebviewWindow,
    ipc: State<'_, Arc<Ipc>>,
    method: String,
    params: Option<Value>,
) -> Result<Value, ErrorInfo> {
    ipc.request(window.label(), &method, params).await
}

#[tauri::command]
pub fn ipc_status(ipc: State<'_, Arc<Ipc>>) -> Status {
    ipc.status()
}

#[derive(Serialize)]
pub struct UiInfo {
    /// "main" or "flyout"
    window: String,
    version: &'static str,
    /// The window has the Mica backdrop: the page keeps its background
    /// translucent
    mica: bool,
}

#[tauri::command]
pub fn ui_info(window: WebviewWindow) -> UiInfo {
    UiInfo { window: window.label().into(), version: env!("CARGO_PKG_VERSION"), mica: windows::mica_supported() }
}

// Async: a synchronous command runs on the main thread, and creating a
// webview window from there deadlocks on Windows
#[tauri::command]
pub async fn show_main(app: AppHandle, route: Option<String>) -> Result<(), String> {
    windows::show_main(&app, route).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn show_flyout(app: AppHandle) -> Result<(), String> {
    windows::show_flyout(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_theme(app: AppHandle, theme: String) {
    windows::set_theme(&app, &theme);
}

/// Opens a web page in the user's browser (https only)
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https URLs are opened".into());
    }
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Opens a folder in Explorer (the settings folder). Folders only: a file
/// path would run the file's program.
#[tauri::command]
pub fn open_folder(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.is_absolute() || !p.is_dir() {
        return Err(format!("{path} is not a folder"));
    }
    tauri_plugin_opener::open_path(p, None::<&str>).map_err(|e| e.to_string())
}

/// Shows a file selected in Explorer (an HCI capture)
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.is_absolute() || !p.exists() {
        return Err(format!("{path} does not exist"));
    }
    tauri_plugin_opener::reveal_item_in_dir(p).map_err(|e| e.to_string())
}
