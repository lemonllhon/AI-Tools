import { create } from 'zustand';
import {
  CodexAccount,
  CodexApiProviderMode,
  CodexAppSpeed,
  CodexProviderWireApi,
  CodexQuota,
  hasCodexAccountStructure,
  hasCodexAccountName,
  isCodexTeamLikePlan,
} from '../types/codex';
import * as codexService from '../services/codexService';
import { emitAccountsChanged, emitCurrentAccountChanged } from '../utils/accountSyncEvents';

const CODEX_ACCOUNTS_CACHE_KEY = 'agtools.codex.accounts.cache';
const CODEX_CURRENT_ACCOUNT_CACHE_KEY = 'agtools.codex.accounts.current';
const CODEX_ACCOUNTS_CACHE_MAX_ITEMS = 300;
const CODEX_ACCOUNTS_CACHE_MAX_CHARS = 1_000_000;
const CODEX_PROFILE_SYNC_IN_FLIGHT = new Set<string>();
const CODEX_PROFILE_SYNC_LAST_ATTEMPT = new Map<string, number>();
const CODEX_PROFILE_SYNC_RETRY_INTERVAL_MS = 5 * 60 * 1000;
const CODEX_PROFILE_SYNC_BATCH_LIMIT = 12;
const CODEX_PROFILE_SYNC_AUTO_LOAD_MAX_ACCOUNTS = 200;
const CODEX_DELETED_ACCOUNT_TOMBSTONE_MS = 10 * 1000;
const CODEX_DELETED_ACCOUNT_TOMBSTONES = new Map<string, number>();
let allowNextEmptyCodexAccountList = false;
let allowNextEmptyCodexCurrentAccount = false;
let fetchAccountsPromise: Promise<void> | null = null;
let fetchCurrentPromise: Promise<void> | null = null;

const loadCachedCodexAccounts = () => {
  try {
    const raw = localStorage.getItem(CODEX_ACCOUNTS_CACHE_KEY);
    if (!raw) return [];
    if (raw.length > CODEX_ACCOUNTS_CACHE_MAX_CHARS) {
      localStorage.removeItem(CODEX_ACCOUNTS_CACHE_KEY);
      return [];
    }
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    if (parsed.length > CODEX_ACCOUNTS_CACHE_MAX_ITEMS) {
      localStorage.removeItem(CODEX_ACCOUNTS_CACHE_KEY);
      return [];
    }
    return parsed;
  } catch {
    return [];
  }
};

const loadCachedCodexCurrentAccount = () => {
  try {
    const raw = localStorage.getItem(CODEX_CURRENT_ACCOUNT_CACHE_KEY);
    if (!raw) return null;
    return JSON.parse(raw) as CodexAccount;
  } catch {
    return null;
  }
};

const persistCodexAccountsCache = (accounts: CodexAccount[]) => {
  try {
    if (accounts.length > CODEX_ACCOUNTS_CACHE_MAX_ITEMS) {
      localStorage.removeItem(CODEX_ACCOUNTS_CACHE_KEY);
      return;
    }
    localStorage.setItem(CODEX_ACCOUNTS_CACHE_KEY, JSON.stringify(accounts));
  } catch {
    // ignore cache write failures
  }
};

const persistCodexCurrentAccountCache = (account: CodexAccount | null) => {
  try {
    if (!account) {
      localStorage.removeItem(CODEX_CURRENT_ACCOUNT_CACHE_KEY);
      return;
    }
    localStorage.setItem(CODEX_CURRENT_ACCOUNT_CACHE_KEY, JSON.stringify(account));
  } catch {
    // ignore cache write failures
  }
};

const pruneDeletedCodexAccountTombstones = () => {
  const now = Date.now();
  for (const [accountId, deletedAt] of CODEX_DELETED_ACCOUNT_TOMBSTONES) {
    if (now - deletedAt >= CODEX_DELETED_ACCOUNT_TOMBSTONE_MS) {
      CODEX_DELETED_ACCOUNT_TOMBSTONES.delete(accountId);
    }
  }
};

const markCodexAccountsDeleted = (accountIds: string[]) => {
  const now = Date.now();
  for (const accountId of accountIds) {
    const normalized = accountId.trim();
    if (normalized) {
      CODEX_DELETED_ACCOUNT_TOMBSTONES.set(normalized, now);
      CODEX_PROFILE_SYNC_IN_FLIGHT.delete(normalized);
      CODEX_PROFILE_SYNC_LAST_ATTEMPT.delete(normalized);
    }
  }
  setTimeout(pruneDeletedCodexAccountTombstones, CODEX_DELETED_ACCOUNT_TOMBSTONE_MS + 1000);
};

const clearCodexAccountDeletedTombstones = (accountIds: string[]) => {
  for (const accountId of accountIds) {
    CODEX_DELETED_ACCOUNT_TOMBSTONES.delete(accountId);
  }
};

const filterDeletedCodexAccountTombstones = (accounts: CodexAccount[]) => {
  pruneDeletedCodexAccountTombstones();
  if (CODEX_DELETED_ACCOUNT_TOMBSTONES.size === 0) {
    return accounts;
  }
  return accounts.filter((account) => !CODEX_DELETED_ACCOUNT_TOMBSTONES.has(account.id));
};

const versionPart = (value: unknown) =>
  value == null ? '' : String(value).replace(/[\u001e\u001f]/g, ' ');

const buildCodexAccountVersionKey = (account: CodexAccount | null | undefined) => {
  if (!account) return '';
  const quota = account.quota;
  const quotaError = account.quota_error;
  return [
    account.id,
    account.email,
    account.auth_mode,
    account.app_speed,
    account.plan_type,
    account.subscription_active_until,
    account.auth_file_plan_type,
    account.account_id,
    account.organization_id,
    account.account_name,
    account.account_structure,
    account.account_note,
    account.token_generation,
    account.token_updated_at,
    account.token_source_mode,
    account.requires_reauth,
    account.reauth_reason,
    account.usage_updated_at,
    account.created_at,
    account.last_used,
    account.api_base_url,
    account.api_provider_mode,
    account.api_provider_id,
    account.api_provider_name,
    account.bound_oauth_account_id,
    quota?.hourly_percentage,
    quota?.hourly_reset_time,
    quota?.hourly_window_minutes,
    quota?.hourly_window_present,
    quota?.weekly_percentage,
    quota?.weekly_reset_time,
    quota?.weekly_window_minutes,
    quota?.weekly_window_present,
    quotaError?.code,
    quotaError?.message,
    quotaError?.timestamp,
    (account.tags ?? []).join('\u001e'),
  ]
    .map(versionPart)
    .join('\u001f');
};

const buildCodexAccountsVersionKey = (accounts: CodexAccount[]) =>
  accounts.map(buildCodexAccountVersionKey).join('\u001e');

const shouldHydrateCodexProfile = (account: CodexAccount): boolean =>
  !hasCodexAccountStructure(account) ||
  (isCodexTeamLikePlan(account.plan_type) && !hasCodexAccountName(account));

interface CodexAccountState {
  accounts: CodexAccount[];
  currentAccount: CodexAccount | null;
  loading: boolean;
  error: string | null;
  
  // Actions
  fetchAccounts: () => Promise<void>;
  fetchCurrentAccount: () => Promise<void>;
  switchAccount: (accountId: string) => Promise<CodexAccount>;
  deleteAccount: (accountId: string) => Promise<void>;
  deleteAccounts: (accountIds: string[]) => Promise<void>;
  refreshQuota: (accountId: string, options?: { reload?: boolean }) => Promise<CodexQuota>;
  refreshAllQuotas: (options?: codexService.RefreshAllCodexQuotaOptions) => Promise<number>;
  hydrateAccountProfilesIfNeeded: (accountIds?: string[]) => Promise<void>;
  importFromLocal: () => Promise<CodexAccount>;
  importFromJson: (jsonContent: string) => Promise<CodexAccount[]>;
  updateAccountName: (accountId: string, name: string) => Promise<CodexAccount>;
  updateApiKeyCredentials: (
    accountId: string,
    apiKey: string,
    apiBaseUrl?: string,
    apiProviderMode?: CodexApiProviderMode,
    apiProviderId?: string,
    apiProviderName?: string,
    apiWireApi?: CodexProviderWireApi | null,
  ) => Promise<CodexAccount>;
  updateApiKeyBoundOAuthAccount: (
    accountId: string,
    boundOauthAccountId: string | null,
  ) => Promise<CodexAccount>;
  updateAccountTags: (accountId: string, tags: string[]) => Promise<CodexAccount>;
  updateAccountNote: (accountId: string, note: string) => Promise<CodexAccount>;
  updateAccountAppSpeed: (accountId: string, speed: CodexAppSpeed) => Promise<CodexAccount>;
}

export const useCodexAccountStore = create<CodexAccountState>((set, get) => ({
  accounts: loadCachedCodexAccounts(),
  currentAccount: loadCachedCodexCurrentAccount(),
  loading: false,
  error: null,
  
  fetchAccounts: async () => {
    if (fetchAccountsPromise) {
      return fetchAccountsPromise;
    }

    fetchAccountsPromise = (async () => {
      if (get().accounts.length === 0) {
        set({ loading: true, error: null });
      } else if (get().error) {
        set({ error: null });
      }

      try {
        const accounts = filterDeletedCodexAccountTombstones(
          await codexService.listCodexAccounts(),
        );
        if (
          accounts.length === 0 &&
          get().accounts.length > 0 &&
          !allowNextEmptyCodexAccountList
        ) {
          console.warn('[CodexAccountStore] 忽略异常空账号列表，保留本地缓存账号');
          if (get().loading) set({ loading: false });
          return;
        }

        allowNextEmptyCodexAccountList = false;
        const previousAccounts = get().accounts;
        const changed =
          buildCodexAccountsVersionKey(accounts) !==
          buildCodexAccountsVersionKey(previousAccounts);

        if (changed) {
          set({ accounts, loading: false, error: null });
          persistCodexAccountsCache(accounts);
        } else if (get().loading || get().error) {
          set({ loading: false, error: null });
        }

        if (accounts.length <= CODEX_PROFILE_SYNC_AUTO_LOAD_MAX_ACCOUNTS) {
          void get().hydrateAccountProfilesIfNeeded(
            accounts.slice(0, CODEX_PROFILE_SYNC_BATCH_LIMIT).map((account) => account.id),
          );
        }
      } catch (e) {
        set({ error: String(e), loading: false });
      } finally {
        allowNextEmptyCodexAccountList = false;
        setTimeout(() => {
          fetchAccountsPromise = null;
        }, 100);
      }
    })();

    return fetchAccountsPromise;
  },
  
  fetchCurrentAccount: async () => {
    if (fetchCurrentPromise) {
      return fetchCurrentPromise;
    }

    fetchCurrentPromise = (async () => {
      try {
        const currentAccount = await codexService.getCurrentCodexAccount();
        if (
          !currentAccount &&
          get().currentAccount &&
          get().accounts.length > 0 &&
          !allowNextEmptyCodexCurrentAccount
        ) {
          console.warn('[CodexAccountStore] 忽略异常空当前账号，保留本地缓存当前账号');
          return;
        }

        allowNextEmptyCodexCurrentAccount = false;
        if (
          buildCodexAccountVersionKey(currentAccount) !==
          buildCodexAccountVersionKey(get().currentAccount)
        ) {
          set({ currentAccount });
          persistCodexCurrentAccountCache(currentAccount);
        }
      } catch (e) {
        console.error('获取当前 Codex 账号失败:', e);
      } finally {
        allowNextEmptyCodexCurrentAccount = false;
        setTimeout(() => {
          fetchCurrentPromise = null;
        }, 100);
      }
    })();

    return fetchCurrentPromise;
  },
  
  switchAccount: async (accountId: string) => {
    const account = await codexService.switchCodexAccount(accountId);
    set({ currentAccount: account });
    await get().fetchAccounts();
    await emitCurrentAccountChanged({
      platformId: 'codex',
      accountId: account.id,
      reason: 'switch',
    });
    return account;
  },
  
  deleteAccount: async (accountId: string) => {
    await get().deleteAccounts([accountId]);
  },
  
  deleteAccounts: async (accountIds: string[]) => {
    const uniqueAccountIds = Array.from(
      new Set(accountIds.map((accountId) => accountId.trim()).filter(Boolean)),
    );
    if (uniqueAccountIds.length === 0) {
      return;
    }

    const previousCurrentAccountId = get().currentAccount?.id ?? null;
    const deleteIdSet = new Set(uniqueAccountIds);
    allowNextEmptyCodexAccountList = get().accounts.every((account) =>
      deleteIdSet.has(account.id),
    );
    allowNextEmptyCodexCurrentAccount = previousCurrentAccountId
      ? deleteIdSet.has(previousCurrentAccountId)
      : false;
    try {
      await codexService.deleteCodexAccounts(uniqueAccountIds);
      markCodexAccountsDeleted(uniqueAccountIds);
      set((state) => {
        const nextAccounts = state.accounts.filter((account) => !deleteIdSet.has(account.id));
        const nextCurrentAccount =
          state.currentAccount && deleteIdSet.has(state.currentAccount.id)
            ? null
            : state.currentAccount;

        persistCodexAccountsCache(nextAccounts);
        persistCodexCurrentAccountCache(nextCurrentAccount);

        return {
          accounts: nextAccounts,
          currentAccount: nextCurrentAccount,
          loading: false,
          error: null,
        };
      });
    } finally {
      allowNextEmptyCodexAccountList = false;
      allowNextEmptyCodexCurrentAccount = false;
    }
    await emitAccountsChanged({
      platformId: 'codex',
      reason: 'delete',
    });
    const nextCurrentAccountId = get().currentAccount?.id ?? null;
    if (previousCurrentAccountId !== nextCurrentAccountId) {
      await emitCurrentAccountChanged({
        platformId: 'codex',
        accountId: nextCurrentAccountId,
        reason: 'delete',
      });
    }
  },
  
  refreshQuota: async (accountId: string, options?: { reload?: boolean }) => {
    const quota = await codexService.refreshCodexQuota(accountId);
    if (options?.reload !== false) {
      await get().fetchAccounts();
      await get().fetchCurrentAccount();
    }
    return quota;
  },
  
  refreshAllQuotas: async (options?: codexService.RefreshAllCodexQuotaOptions) => {
    const successCount = await codexService.refreshAllCodexQuotas(options);
    await get().fetchAccounts();
    await get().fetchCurrentAccount();
    return successCount;
  },

  hydrateAccountProfilesIfNeeded: async (accountIds?: string[]) => {
    const now = Date.now();
    const scope = accountIds ? new Set(accountIds) : null;
    const candidates = get().accounts.filter(
      (account) =>
        (!scope || scope.has(account.id)) &&
        shouldHydrateCodexProfile(account) &&
        !CODEX_PROFILE_SYNC_IN_FLIGHT.has(account.id) &&
        now - (CODEX_PROFILE_SYNC_LAST_ATTEMPT.get(account.id) ?? 0) >=
          CODEX_PROFILE_SYNC_RETRY_INTERVAL_MS,
    );

    for (const account of candidates.slice(0, CODEX_PROFILE_SYNC_BATCH_LIMIT)) {
      CODEX_PROFILE_SYNC_IN_FLIGHT.add(account.id);
      CODEX_PROFILE_SYNC_LAST_ATTEMPT.set(account.id, now);
      try {
        const updatedAccount = await codexService.refreshCodexAccountProfile(account.id);
        set((state) => {
          const nextAccounts = state.accounts.map((item) =>
            item.id === updatedAccount.id ? { ...item, ...updatedAccount } : item,
          );
          const nextCurrentAccount =
            state.currentAccount?.id === updatedAccount.id
              ? { ...state.currentAccount, ...updatedAccount }
              : state.currentAccount;

          persistCodexAccountsCache(nextAccounts);
          persistCodexCurrentAccountCache(nextCurrentAccount);

          return {
            accounts: nextAccounts,
            currentAccount: nextCurrentAccount,
          };
        });
      } catch (e) {
        console.warn('刷新 Codex 账号资料失败:', account.id, e);
      } finally {
        CODEX_PROFILE_SYNC_IN_FLIGHT.delete(account.id);
      }
    }
  },
  
  importFromLocal: async () => {
    const account = await codexService.importCodexFromLocal();
    clearCodexAccountDeletedTombstones([account.id]);
    await get().fetchAccounts();
    await emitAccountsChanged({
      platformId: 'codex',
      reason: 'import',
    });
    return account;
  },
  
  importFromJson: async (jsonContent: string) => {
    const accounts = await codexService.importCodexFromJson(jsonContent);
    clearCodexAccountDeletedTombstones(accounts.map((account) => account.id));
    await get().fetchAccounts();
    await emitAccountsChanged({
      platformId: 'codex',
      reason: 'import',
    });
    return accounts;
  },

  updateAccountName: async (accountId: string, name: string) => {
    const account = await codexService.updateCodexAccountName(accountId, name);
    await get().fetchAccounts();
    await get().fetchCurrentAccount();
    return account;
  },

  updateApiKeyCredentials: async (
    accountId: string,
    apiKey: string,
    apiBaseUrl?: string,
    apiProviderMode?: CodexApiProviderMode,
    apiProviderId?: string,
    apiProviderName?: string,
    apiWireApi?: CodexProviderWireApi | null,
  ) => {
    const account = await codexService.updateCodexApiKeyCredentials(
      accountId,
      apiKey,
      apiBaseUrl,
      apiProviderMode,
      apiProviderId,
      apiProviderName,
      apiWireApi,
    );
    await get().fetchAccounts();
    await get().fetchCurrentAccount();
    return account;
  },

  updateApiKeyBoundOAuthAccount: async (
    accountId: string,
    boundOauthAccountId: string | null,
  ) => {
    const account = await codexService.updateCodexApiKeyBoundOAuthAccount(
      accountId,
      boundOauthAccountId,
    );
    await get().fetchAccounts();
    await get().fetchCurrentAccount();
    return account;
  },

  updateAccountTags: async (accountId: string, tags: string[]) => {
    const account = await codexService.updateCodexAccountTags(accountId, tags);
    await get().fetchAccounts();
    return account;
  },

  updateAccountNote: async (accountId: string, note: string) => {
    const account = await codexService.updateCodexAccountNote(accountId, note);
    await get().fetchAccounts();
    await get().fetchCurrentAccount();
    return account;
  },

  updateAccountAppSpeed: async (accountId: string, speed: CodexAppSpeed) => {
    const account = await codexService.updateCodexAccountAppSpeed(accountId, speed);
    await get().fetchAccounts();
    await get().fetchCurrentAccount();
    return account;
  },
}));
