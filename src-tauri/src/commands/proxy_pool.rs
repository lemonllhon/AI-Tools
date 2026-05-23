use std::sync::Arc;
use tauri::Emitter;
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;

use crate::modules::proxy_pool::models::{
    ProxyImportApplyRequest, ProxyImportApplyResponse, ProxyImportPreviewRequest,
    ProxyImportPreviewCheckRequest, ProxyImportPreviewCheckResponse, ProxyImportPreviewResponse,
    ProxyNodeSaveRequest, ProxyPoolCheckProgressEvent, ProxyPoolIpHealthResponse,
    ProxyPoolLatencyTestResponse, ProxyPoolListResponse, ProxyPoolNode, ProxyPoolServiceUpdateRequest,
    ProxySourceUpdateRequest, ProxySubscriptionApplyRequest, ProxySubscriptionApplyResponse,
    ProxySubscriptionPreviewCheckRequest, ProxySubscriptionPreviewRequest,
    ProxySubscriptionRefreshRequest, ProxySubscriptionRefreshResponse,
};
use crate::modules::proxy_pool::gateway;
use crate::modules::proxy_pool::runtime::{self, ProxyRuntimeStatus};
use crate::modules::proxy_pool::store;

const PROXY_POOL_CHECK_PROGRESS_EVENT: &str = "proxy_pool://check_progress";

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
pub fn proxy_runtime_open_resource_dir(app: tauri::AppHandle) -> Result<(), String> {
    let resource_dir = runtime::resolve_resource_runtime_dir(&app)?;
    app.opener()
        .open_path(resource_dir.to_string_lossy().to_string(), None::<String>)
        .map_err(|err| format!("打开代理内核目录失败: {}", err))
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
pub async fn proxy_pool_check_import_preview(
    request: ProxyImportPreviewCheckRequest,
) -> Result<ProxyImportPreviewCheckResponse, String> {
    store::check_import_preview(request).await
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
pub async fn proxy_pool_check_subscription_preview(
    request: ProxySubscriptionPreviewCheckRequest,
) -> Result<ProxyImportPreviewCheckResponse, String> {
    store::check_subscription_preview(request).await
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

#[tauri::command]
pub fn proxy_pool_update_subscription_source(
    request: ProxySourceUpdateRequest,
) -> Result<ProxyPoolListResponse, String> {
    store::update_subscription_source(request)
}

#[tauri::command]
pub fn proxy_pool_delete_subscription_source(
    source_id: String,
) -> Result<ProxyPoolListResponse, String> {
    store::delete_subscription_source(&source_id)
}

#[tauri::command]
pub async fn proxy_pool_test_node_latency(
    id: String,
) -> Result<ProxyPoolLatencyTestResponse, String> {
    store::test_node_latency(&id).await
}

#[tauri::command]
pub async fn proxy_pool_test_all_latency(
    app: tauri::AppHandle,
    task_id: Option<String>,
) -> Result<ProxyPoolLatencyTestResponse, String> {
    let task_id = normalize_progress_task_id(task_id);
    let progress_app = app.clone();
    let emitter: store::ProxyPoolProgressEmitter = Arc::new(move |event: ProxyPoolCheckProgressEvent| {
        let _ = progress_app.emit(PROXY_POOL_CHECK_PROGRESS_EVENT, event);
    });
    store::test_all_latency_with_progress(Some(task_id), Some(emitter)).await
}

#[tauri::command]
pub async fn proxy_pool_check_node_ip_health(
    id: String,
) -> Result<ProxyPoolIpHealthResponse, String> {
    store::check_node_ip_health(&id).await
}

#[tauri::command]
pub async fn proxy_pool_check_all_ip_health(
    app: tauri::AppHandle,
    task_id: Option<String>,
) -> Result<ProxyPoolIpHealthResponse, String> {
    let task_id = normalize_progress_task_id(task_id);
    let progress_app = app.clone();
    let emitter: store::ProxyPoolProgressEmitter = Arc::new(move |event: ProxyPoolCheckProgressEvent| {
        let _ = progress_app.emit(PROXY_POOL_CHECK_PROGRESS_EVENT, event);
    });
    store::check_all_ip_health_with_progress(Some(task_id), Some(emitter)).await
}

#[tauri::command]
pub async fn proxy_pool_update_service_state(
    request: ProxyPoolServiceUpdateRequest,
) -> Result<ProxyPoolListResponse, String> {
    store::update_service_state_config(request)?;
    gateway::sync_gateway_state().await?;
    store::list_nodes()
}

fn normalize_progress_task_id(task_id: Option<String>) -> String {
    let value = task_id.unwrap_or_default().trim().to_string();
    if value.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        value
    }
}
