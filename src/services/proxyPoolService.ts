import { invoke } from '@tauri-apps/api/core';
import type {
  ProxyImportApplyRequest,
  ProxyImportApplyResponse,
  ProxyImportPreviewRequest,
  ProxyImportPreviewResponse,
  ProxyNodeSaveRequest,
  ProxyPoolListResponse,
  ProxyPoolNode,
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
