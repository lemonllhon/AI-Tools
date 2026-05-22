import { invoke } from '@tauri-apps/api/core';
import type { ProxyRuntimeStatus } from '../types/proxyRuntime';

export async function getProxyRuntimeStatus(): Promise<ProxyRuntimeStatus> {
  return await invoke('proxy_runtime_get_status');
}

export async function verifyProxyRuntime(): Promise<ProxyRuntimeStatus> {
  return await invoke('proxy_runtime_verify');
}

export async function openProxyRuntimeCacheDir(): Promise<void> {
  return await invoke('proxy_runtime_open_cache_dir');
}
