import { invoke } from '@tauri-apps/api/core';
import type {
  ProxyImportApplyRequest,
  ProxyImportApplyResponse,
  ProxyImportPreviewCheckRequest,
  ProxyImportPreviewCheckResponse,
  ProxyPoolIpHealthResponse,
  ProxyPoolLatencyTestResponse,
  ProxyImportPreviewRequest,
  ProxyImportPreviewResponse,
  ProxyNodeSaveRequest,
  ProxyPoolListResponse,
  ProxyPoolNode,
  ProxyPoolServiceUpdateRequest,
  ProxySourceUpdateRequest,
  ProxySubscriptionApplyRequest,
  ProxySubscriptionApplyResponse,
  ProxySubscriptionPreviewCheckRequest,
  ProxySubscriptionPreviewRequest,
  ProxySubscriptionRefreshRequest,
  ProxySubscriptionRefreshResponse,
} from '../types/proxyPool';

export const PROXY_POOL_CHECK_PROGRESS_EVENT = 'proxy_pool://check_progress';
export const PROXY_POOL_GATEWAY_FAILOVER_EVENT = 'proxy_pool://gateway_failover';

export async function listProxyPoolNodes(): Promise<ProxyPoolListResponse> {
  return await invoke('proxy_pool_list_nodes');
}

export async function saveProxyPoolNode(request: ProxyNodeSaveRequest): Promise<ProxyPoolNode> {
  return await invoke('proxy_pool_save_node', { request });
}

export async function deleteProxyPoolNode(id: string): Promise<void> {
  return await invoke('proxy_pool_delete_node', { id });
}

export async function deleteProxyPoolNodes(ids: string[]): Promise<void> {
  return await invoke('proxy_pool_delete_nodes', { ids });
}

export async function setProxyPoolNodeEnabled(id: string, enabled: boolean): Promise<ProxyPoolNode> {
  return await invoke('proxy_pool_set_node_enabled', { id, enabled });
}

export async function previewProxyPoolImport(
  request: ProxyImportPreviewRequest,
): Promise<ProxyImportPreviewResponse> {
  return await invoke('proxy_pool_preview_import', { request });
}

export async function checkProxyPoolImportPreview(
  request: ProxyImportPreviewCheckRequest,
): Promise<ProxyImportPreviewCheckResponse> {
  return await invoke('proxy_pool_check_import_preview', { request });
}

export async function applyProxyPoolImport(
  request: ProxyImportApplyRequest,
): Promise<ProxyImportApplyResponse> {
  return await invoke('proxy_pool_apply_import', { request });
}

export async function previewProxyPoolSubscription(
  request: ProxySubscriptionPreviewRequest,
): Promise<ProxyImportPreviewResponse> {
  return await invoke('proxy_pool_preview_subscription', { request });
}

export async function checkProxyPoolSubscriptionPreview(
  request: ProxySubscriptionPreviewCheckRequest,
): Promise<ProxyImportPreviewCheckResponse> {
  return await invoke('proxy_pool_check_subscription_preview', { request });
}

export async function applyProxyPoolSubscription(
  request: ProxySubscriptionApplyRequest,
): Promise<ProxySubscriptionApplyResponse> {
  return await invoke('proxy_pool_apply_subscription', { request });
}

export async function refreshProxyPoolSubscription(
  request: ProxySubscriptionRefreshRequest,
): Promise<ProxySubscriptionRefreshResponse> {
  return await invoke('proxy_pool_refresh_subscription', { request });
}

export async function refreshAllProxyPoolSubscriptions(): Promise<ProxySubscriptionRefreshResponse> {
  return await invoke('proxy_pool_refresh_all_subscriptions');
}

export async function updateProxyPoolSubscriptionSource(
  request: ProxySourceUpdateRequest,
): Promise<ProxyPoolListResponse> {
  return await invoke('proxy_pool_update_subscription_source', { request });
}

export async function deleteProxyPoolSubscriptionSource(sourceId: string): Promise<ProxyPoolListResponse> {
  return await invoke('proxy_pool_delete_subscription_source', { sourceId });
}

export async function testProxyPoolNodeLatency(id: string): Promise<ProxyPoolLatencyTestResponse> {
  return await invoke('proxy_pool_test_node_latency', { id });
}

export async function testAllProxyPoolLatency(taskId?: string): Promise<ProxyPoolLatencyTestResponse> {
  return await invoke('proxy_pool_test_all_latency', { taskId });
}

export async function checkProxyPoolNodeIpHealth(id: string): Promise<ProxyPoolIpHealthResponse> {
  return await invoke('proxy_pool_check_node_ip_health', { id });
}

export async function checkAllProxyPoolIpHealth(taskId?: string): Promise<ProxyPoolIpHealthResponse> {
  return await invoke('proxy_pool_check_all_ip_health', { taskId });
}

export async function updateProxyPoolServiceState(
  request: ProxyPoolServiceUpdateRequest,
): Promise<ProxyPoolListResponse> {
  return await invoke('proxy_pool_update_service_state', { request });
}

export async function prepareProxyPoolGatewayForRestart(): Promise<ProxyPoolListResponse> {
  return await invoke('proxy_pool_prepare_gateway_restart');
}

export async function restoreProxyPoolGatewayState(): Promise<ProxyPoolListResponse> {
  return await invoke('proxy_pool_restore_gateway_state');
}
