import { invoke } from '@tauri-apps/api/core';
import type {
  CodexLocalAccessCustomRoutingRule,
  CodexLocalAccessPortCleanupResult,
  CodexLocalAccessRoutingStrategy,
  CodexLocalAccessScope,
  CodexLocalAccessSourceMode,
  CodexLocalAccessState,
  CodexLocalAccessTestResult,
  CodexLocalAccessUpstreamProxyMode,
} from '../types/codexLocalAccess';

export const CODEX_LOCAL_ACCESS_STATE_UPDATED_EVENT = 'codex-local-access-state-updated';

function dispatchCodexLocalAccessStateUpdated(state: CodexLocalAccessState): void {
  if (typeof window === 'undefined') return;
  window.dispatchEvent(
    new CustomEvent<CodexLocalAccessState>(CODEX_LOCAL_ACCESS_STATE_UPDATED_EVENT, {
      detail: state,
    }),
  );
}

async function invokeCodexLocalAccessStateMutation(
  command: string,
  args?: Record<string, unknown>,
): Promise<CodexLocalAccessState> {
  const state = await invoke<CodexLocalAccessState>(command, args);
  dispatchCodexLocalAccessStateUpdated(state);
  return state;
}

export async function getCodexLocalAccessState(): Promise<CodexLocalAccessState> {
  return await invoke('codex_local_access_get_state');
}

export async function saveCodexLocalAccessAccounts(
  accountIds: string[],
  restrictFreeAccounts: boolean,
  autoIncludeNewAccounts: boolean,
): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_save_accounts', {
    accountIds,
    restrictFreeAccounts,
    autoIncludeNewAccounts,
  });
}

export async function saveCodexLocalAccessProviders(
  providerIds: string[],
  autoIncludeNewProviders: boolean,
): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_save_providers', {
    providerIds,
    autoIncludeNewProviders,
  });
}

export async function removeCodexLocalAccessAccount(
  accountId: string,
): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_remove_account', { accountId });
}

export async function rotateCodexLocalAccessApiKey(): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_rotate_api_key');
}

export async function updateCodexLocalAccessBoundOAuthAccount(
  boundOauthAccountId: string | null,
): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_update_bound_oauth_account', {
    boundOauthAccountId,
  });
}

export async function clearCodexLocalAccessStats(): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_clear_stats');
}

export async function prepareCodexLocalAccessForRestart(): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_prepare_restart');
}

export async function killCodexLocalAccessPort(): Promise<CodexLocalAccessPortCleanupResult> {
  const result = await invoke<CodexLocalAccessPortCleanupResult>('codex_local_access_kill_port');
  dispatchCodexLocalAccessStateUpdated(result.state);
  return result;
}

export async function updateCodexLocalAccessPort(
  port: number,
): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_update_port', { port });
}

export async function updateCodexLocalAccessRoutingStrategy(
  strategy: CodexLocalAccessRoutingStrategy,
): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_update_routing_strategy', { strategy });
}

export async function updateCodexLocalAccessCustomRouting(
  rules: CodexLocalAccessCustomRoutingRule[],
): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_update_custom_routing', { rules });
}

export async function updateCodexLocalAccessUpstreamProxyMode(
  upstreamProxyMode: CodexLocalAccessUpstreamProxyMode,
): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_update_upstream_proxy_mode', {
    upstreamProxyMode,
  });
}

export async function updateCodexLocalAccessSourceMode(
  sourceMode: CodexLocalAccessSourceMode,
): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_update_source_mode', {
    sourceMode,
  });
}

export async function updateCodexLocalAccessAccessScope(
  accessScope: CodexLocalAccessScope,
): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_update_access_scope', {
    accessScope,
  });
}

export async function setCodexLocalAccessEnabled(
  enabled: boolean,
): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_set_enabled', { enabled });
}

export async function activateCodexLocalAccess(): Promise<CodexLocalAccessState> {
  return await invokeCodexLocalAccessStateMutation('codex_local_access_activate');
}

export async function testCodexLocalAccess(): Promise<CodexLocalAccessTestResult> {
  return await invoke('codex_local_access_test');
}
