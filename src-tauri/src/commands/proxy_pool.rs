use tauri_plugin_opener::OpenerExt;

use crate::modules::proxy_pool::models::{
    ProxyImportApplyRequest, ProxyImportApplyResponse, ProxyImportPreviewRequest,
    ProxyImportPreviewResponse, ProxyNodeSaveRequest, ProxyPoolListResponse, ProxyPoolNode,
    ProxySubscriptionApplyRequest, ProxySubscriptionApplyResponse, ProxySubscriptionPreviewRequest,
    ProxySubscriptionRefreshRequest, ProxySubscriptionRefreshResponse,
};
use crate::modules::proxy_pool::runtime::{self, ProxyRuntimeStatus};
use crate::modules::proxy_pool::store;

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

#[tauri::command]
pub fn proxy_pool_list_nodes() -> Result<ProxyPoolListResponse, String> {
    store::list_nodes()
}

#[tauri::command]
pub fn proxy_pool_save_node(request: ProxyNodeSaveRequest) -> Result<ProxyPoolNode, String> {
    store::save_node(request)
}

#[tauri::command]
pub fn proxy_pool_delete_node(id: String) -> Result<(), String> {
    store::delete_node(&id)
}

#[tauri::command]
pub fn proxy_pool_delete_nodes(ids: Vec<String>) -> Result<(), String> {
    store::delete_nodes(&ids)
}

#[tauri::command]
pub fn proxy_pool_set_node_enabled(id: String, enabled: bool) -> Result<ProxyPoolNode, String> {
    store::set_node_enabled(&id, enabled)
}

#[tauri::command]
pub fn proxy_pool_preview_import(
    request: ProxyImportPreviewRequest,
) -> Result<ProxyImportPreviewResponse, String> {
    store::preview_import(request)
}

#[tauri::command]
pub fn proxy_pool_apply_import(
    request: ProxyImportApplyRequest,
) -> Result<ProxyImportApplyResponse, String> {
    store::apply_import(request)
}

#[tauri::command]
pub async fn proxy_pool_preview_subscription(
    request: ProxySubscriptionPreviewRequest,
) -> Result<ProxyImportPreviewResponse, String> {
    store::preview_subscription(request).await
}

#[tauri::command]
pub async fn proxy_pool_apply_subscription(
    request: ProxySubscriptionApplyRequest,
) -> Result<ProxySubscriptionApplyResponse, String> {
    store::apply_subscription(request).await
}

#[tauri::command]
pub async fn proxy_pool_refresh_subscription(
    request: ProxySubscriptionRefreshRequest,
) -> Result<ProxySubscriptionRefreshResponse, String> {
    store::refresh_subscription(request).await
}

#[tauri::command]
pub async fn proxy_pool_refresh_all_subscriptions(
) -> Result<ProxySubscriptionRefreshResponse, String> {
    store::refresh_all_subscriptions().await
}
