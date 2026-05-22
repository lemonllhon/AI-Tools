use tauri_plugin_opener::OpenerExt;

use crate::modules::proxy_pool::runtime::{self, ProxyRuntimeStatus};

#[tauri::command]
pub fn proxy_runtime_get_status(app: tauri::AppHandle) -> Result<ProxyRuntimeStatus, String> {
    runtime::get_runtime_status(&app)
}

#[tauri::command]
pub fn proxy_runtime_verify(app: tauri::AppHandle) -> Result<ProxyRuntimeStatus, String> {
    runtime::get_runtime_status(&app)
}

#[tauri::command]
pub fn proxy_runtime_open_cache_dir(app: tauri::AppHandle) -> Result<(), String> {
    let cache_root = runtime::cache_root_for_current_target()?;
    std::fs::create_dir_all(&cache_root)
        .map_err(|err| format!("创建代理内核缓存目录失败 {}: {}", cache_root.display(), err))?;
    app.opener()
        .open_path(cache_root.to_string_lossy().to_string(), None::<String>)
        .map_err(|err| format!("打开代理内核缓存目录失败: {}", err))
}
