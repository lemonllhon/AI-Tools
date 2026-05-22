import { invoke } from '@tauri-apps/api/core';
import type {
  ProxyImportApplyRequest,
  ProxyImportApplyResponse,
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
  ProxySubscriptionPreviewRequest,
  ProxySubscriptionRefreshRequest,
  ProxySubscriptionRefreshResponse,
} from '../types/proxyPool';

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

export async function testAllProxyPoolLatency(): Promise<ProxyPoolLatencyTestResponse> {
  return await invoke('proxy_pool_test_all_latency');
}

export async function checkProxyPoolNodeIpHealth(id: string): Promise<ProxyPoolIpHealthResponse> {
  return await invoke('proxy_pool_check_node_ip_health', { id });
}

export async function checkAllProxyPoolIpHealth(): Promise<ProxyPoolIpHealthResponse> {
  return await invoke('proxy_pool_check_all_ip_health');
}

export async function updateProxyPoolServiceState(
  request: ProxyPoolServiceUpdateRequest,
): Promise<ProxyPoolListResponse> {
  return await invoke('proxy_pool_update_service_state', { request });
}
