import React, { useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { confirm as confirmDialog } from '@tauri-apps/plugin-dialog';
import { useAccountStore } from '../stores/useAccountStore';
import { useCodexAccountStore } from '../stores/useCodexAccountStore';
import { useGitHubCopilotAccountStore } from '../stores/useGitHubCopilotAccountStore';
import { useWindsurfAccountStore } from '../stores/useWindsurfAccountStore';
import { useKiroAccountStore } from '../stores/useKiroAccountStore';
import { useCursorAccountStore } from '../stores/useCursorAccountStore';
import { useGeminiAccountStore } from '../stores/useGeminiAccountStore';
import { useCodebuddyAccountStore } from '../stores/useCodebuddyAccountStore';
import { useCodebuddyCnAccountStore } from '../stores/useCodebuddyCnAccountStore';
import { useQoderAccountStore } from '../stores/useQoderAccountStore';
import { useTraeAccountStore } from '../stores/useTraeAccountStore';
import { useWorkbuddyAccountStore } from '../stores/useWorkbuddyAccountStore';
import { useZedAccountStore } from '../stores/useZedAccountStore';
import {
  parseGroupEntryId,
  PlatformLayoutEntryId,
  resolveEntryDefaultPlatformId,
  resolveEntryPlatformIds,
  resolveGroupChildName,
  usePlatformLayoutStore,
} from '../stores/usePlatformLayoutStore';
import { Page } from '../types/navigation';
import { Users, CheckCircle2, Sparkles, RotateCw, Play, Github, Tag, ChevronDown, EyeOff, Server, Power, Settings2, FolderPlus, Gauge, Zap } from 'lucide-react';
import { TagEditModal } from '../components/TagEditModal';
import { Account } from '../types/account';
import {
  CodebuddyAccount,
  getCodebuddyResourceSummary,
  getCodebuddyExtraCreditSummary,
  getCodebuddyOfficialQuotaModel,
  getCodebuddyQuotaCategoryGroups,
} from '../types/codebuddy';
import {
  QoderAccount,
  getQoderSubscriptionInfo,
} from '../types/qoder';
import {
  TraeAccount,
  getTraeUsage,
} from '../types/trae';
import {
  WorkbuddyAccount,
  getWorkbuddyOfficialQuotaModel,
} from '../types/workbuddy';
import { CodexAccount, CodexAppSpeed } from '../types/codex';
import { GitHubCopilotAccount } from '../types/githubCopilot';
import {
  WindsurfAccount,
  getWindsurfCreditsSummary,
} from '../types/windsurf';
import {
  KiroAccount,
  getKiroCreditsSummary,
  isKiroAccountBanned,
} from '../types/kiro';
import { CursorAccount, getCursorUsage } from '../types/cursor';
import {
  GeminiAccount,
  getGeminiTierQuotaSummary,
} from '../types/gemini';
import { ZedAccount, getZedUsage } from '../types/zed';
import './DashboardPage.css';
import { RobotIcon } from '../components/icons/RobotIcon';
import { CodexIcon } from '../components/icons/CodexIcon';
import { WindsurfIcon } from '../components/icons/WindsurfIcon';
import { KiroIcon } from '../components/icons/KiroIcon';
import { CursorIcon } from '../components/icons/CursorIcon';
import { GeminiIcon } from '../components/icons/GeminiIcon';
import { CodebuddyIcon } from '../components/icons/CodebuddyIcon';
import { QoderIcon } from '../components/icons/QoderIcon';
import { TraeIcon } from '../components/icons/TraeIcon';
import { WorkbuddyIcon } from '../components/icons/WorkbuddyIcon';
import { ALL_PLATFORM_IDS, PlatformId, PLATFORM_PAGE_MAP, isMenuVisiblePlatform } from '../types/platform';
import { getPlatformLabel, renderPlatformIcon } from '../utils/platformMeta';
import { ManualHelpIconButton } from '../components/ManualHelpIconButton';
import { AnnouncementCenter } from '../components/AnnouncementCenter';
import { CodexLocalAccessModal } from '../components/CodexLocalAccessModal';
import { isPrivacyModeEnabledByDefault, maskSensitiveValue } from '../utils/privacy';
import { DisplayGroup, getDisplayGroups } from '../services/groupService';
import {
  CodexAccountGroup,
  getCodexAccountGroups,
} from '../services/codexAccountGroupService';
import {
  buildAntigravityAccountPresentation,
  buildCodebuddyAccountPresentation,
  buildCodexAccountPresentation,
  buildCursorAccountPresentation,
  buildGeminiAccountPresentation,
  buildGitHubCopilotAccountPresentation,
  buildKiroAccountPresentation,
  buildQoderAccountPresentation,
  buildTraeAccountPresentation,
  buildWorkbuddyAccountPresentation,
  buildZedAccountPresentation,
  UnifiedAccountPresentation,
  buildWindsurfAccountPresentation,
  UnifiedQuotaMetric,
} from '../presentation/platformAccountPresentation';
import * as codexLocalAccessService from '../services/codexLocalAccessService';
import * as codexService from '../services/codexService';
import {
  listCodexModelProviders,
  type CodexModelProvider,
} from '../services/codexModelProviderService';
import type {
  CodexLocalAccessAddressKind,
  CodexLocalAccessCustomRoutingRule,
  CodexLocalAccessRoutingStrategy,
  CodexLocalAccessScope,
  CodexLocalAccessSourceMode,
  CodexLocalAccessState,
  CodexLocalAccessTestResult,
  CodexLocalAccessUpstreamProxyMode,
  CodexLocalAccessWebSocketMode,
} from '../types/codexLocalAccess';

interface DashboardPageProps {
  onNavigate: (page: Page) => void;
  onOpenPlatformLayout: () => void;
  topCenterBanner?: React.ReactNode;
}

const DASHBOARD_DEFERRED_PREFETCH_DELAY_MS = 6000;
const DASHBOARD_DEFERRED_PREFETCH_BATCH_SIZE = 1;
const DASHBOARD_DEFERRED_PREFETCH_BATCH_DELAY_MS = 1200;
const DASHBOARD_CODEX_LOCAL_ACCESS_ADDRESS_KIND_KEY =
  'agtools.codex.local_access.address_kind.v1';
let dashboardStartupPrefetched = false;

function normalizeDashboardLocalAccessAddressKind(
  value: string | null | undefined,
): CodexLocalAccessAddressKind {
  return value === 'lan' ? 'lan' : 'local';
}

function readDashboardLocalAccessAddressKind(): CodexLocalAccessAddressKind {
  try {
    return normalizeDashboardLocalAccessAddressKind(
      localStorage.getItem(DASHBOARD_CODEX_LOCAL_ACCESS_ADDRESS_KIND_KEY),
    );
  } catch {
    return 'local';
  }
}

function persistDashboardLocalAccessAddressKind(
  value: CodexLocalAccessAddressKind,
): void {
  try {
    localStorage.setItem(DASHBOARD_CODEX_LOCAL_ACCESS_ADDRESS_KIND_KEY, value);
  } catch {
    // Ignore storage failures; address preference is non-critical.
  }
}

function toFiniteNumber(value: number | null | undefined): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function resolveDashboardCurrentAccount<T extends { id: string }>(
  accounts: T[],
  currentId: string | null | undefined,
  currentAccount?: T | null,
): T | null {
  const normalizedCurrentId = currentId?.trim();
  if (normalizedCurrentId) {
    const matched = accounts.find((account) => account.id === normalizedCurrentId);
    if (matched) return matched;
    if (currentAccount?.id === normalizedCurrentId) return currentAccount;
  }
  return accounts[0] ?? null;
}

function getZedRecommendationScore(account: ZedAccount): { remainingPercent: number; freshness: number } {
  const usage = getZedUsage(account);
  const remainingValues: number[] = [];

  if (
    usage.remainingCompletions != null &&
    usage.totalCompletions != null &&
    usage.totalCompletions > 0
  ) {
    remainingValues.push((usage.remainingCompletions / usage.totalCompletions) * 100);
  }

  if (
    usage.remainingChat != null &&
    usage.totalChat != null &&
    usage.totalChat > 0
  ) {
    remainingValues.push((usage.remainingChat / usage.totalChat) * 100);
  }

  return {
    remainingPercent:
      remainingValues.length > 0
        ? remainingValues.reduce((sum, value) => sum + value, 0) / remainingValues.length
        : -1,
    freshness: account.last_used || account.created_at || 0,
  };
}

interface DashboardCardCollapseState {
  workbuddy: boolean;
}

export function DashboardPage({
  onNavigate,
  onOpenPlatformLayout,
  topCenterBanner,
}: DashboardPageProps) {
  const { t } = useTranslation();

  const [tagModalState, setTagModalState] = React.useState<{ accountId: string; platform: PlatformId | 'codebuddy_cn'; tags: string[] } | null>(null);
  const [dashboardCardCollapse, setDashboardCardCollapse] = React.useState<DashboardCardCollapseState>({
    workbuddy: false,
  });
  const [apiServiceState, setApiServiceState] = React.useState<CodexLocalAccessState | null>(null);
  const [apiServiceBusy, setApiServiceBusy] = React.useState<'load' | 'toggle' | 'activate' | null>(null);
  const [apiServiceSaving, setApiServiceSaving] = React.useState(false);
  const [apiServiceSpeedSaving, setApiServiceSpeedSaving] = React.useState<CodexAppSpeed | null>(null);
  const [apiServiceTesting, setApiServiceTesting] = React.useState(false);
  const [apiServicePortCleanupBusy, setApiServicePortCleanupBusy] = React.useState(false);
  const [apiServiceMessage, setApiServiceMessage] = React.useState<{ text: string; tone?: 'success' | 'error' } | null>(null);
  const [apiServiceModalMode, setApiServiceModalMode] = React.useState<'panel' | 'members' | 'providers'>('panel');
  const [showApiServiceModal, setShowApiServiceModal] = React.useState(false);
  const [apiServiceAccountGroups, setApiServiceAccountGroups] = React.useState<CodexAccountGroup[]>([]);
  const [apiServiceModelProviders, setApiServiceModelProviders] = React.useState<CodexModelProvider[]>([]);
  const [apiServiceAddressKind, setApiServiceAddressKind] = React.useState<CodexLocalAccessAddressKind>(() =>
    readDashboardLocalAccessAddressKind(),
  );
  const apiServicePlatformIds = useMemo(
    () => ALL_PLATFORM_IDS.filter(isMenuVisiblePlatform),
    [],
  );

  const apiServiceRefreshTimersRef = React.useRef<number[]>([]);

  const reloadApiServiceState = useCallback(async (options?: { silent?: boolean }) => {
    const showBusy = !options?.silent;
    if (showBusy) {
      setApiServiceBusy((current) => current ?? 'load');
    }
    try {
      const nextState = await codexLocalAccessService.getCodexLocalAccessState();
      setApiServiceState(nextState);
    } catch (error) {
      setApiServiceMessage({
        text: t('dashboard.apiServices.loadFailed', {
          defaultValue: 'API 服务状态加载失败：{{error}}',
          error: error instanceof Error ? error.message : String(error),
        }),
        tone: 'error',
      });
    } finally {
      if (showBusy) {
        setApiServiceBusy((current) => (current === 'load' ? null : current));
      }
    }
  }, [t]);

  React.useEffect(() => {
    void reloadApiServiceState();
  }, [reloadApiServiceState]);

  const clearApiServiceRefreshTimers = useCallback(() => {
    for (const timer of apiServiceRefreshTimersRef.current) {
      window.clearTimeout(timer);
    }
    apiServiceRefreshTimersRef.current = [];
  }, []);

  const scheduleApiServiceStateRefresh = useCallback(
    (delays: number[] = [250, 1000, 2500]) => {
      clearApiServiceRefreshTimers();
      apiServiceRefreshTimersRef.current = delays.map((delay) => {
        const timer = window.setTimeout(() => {
          apiServiceRefreshTimersRef.current = apiServiceRefreshTimersRef.current.filter(
            (item) => item !== timer,
          );
          void reloadApiServiceState({ silent: true });
        }, delay);
        return timer;
      });
    },
    [clearApiServiceRefreshTimers, reloadApiServiceState],
  );

  React.useEffect(() => clearApiServiceRefreshTimers, [clearApiServiceRefreshTimers]);

  React.useEffect(() => {
    const handleLocalAccessStateUpdated = (event: Event) => {
      const nextState = (event as CustomEvent<CodexLocalAccessState>).detail;
      if (nextState) {
        setApiServiceState(nextState);
      } else {
        void reloadApiServiceState({ silent: true });
      }
    };

    window.addEventListener(
      codexLocalAccessService.CODEX_LOCAL_ACCESS_STATE_UPDATED_EVENT,
      handleLocalAccessStateUpdated,
    );
    return () => {
      window.removeEventListener(
        codexLocalAccessService.CODEX_LOCAL_ACCESS_STATE_UPDATED_EVENT,
        handleLocalAccessStateUpdated,
      );
    };
  }, [reloadApiServiceState]);

  React.useEffect(() => {
    const refreshVisibleState = () => {
      if (document.visibilityState === 'visible') {
        void reloadApiServiceState({ silent: true });
      }
    };

    window.addEventListener('focus', refreshVisibleState);
    document.addEventListener('visibilitychange', refreshVisibleState);
    return () => {
      window.removeEventListener('focus', refreshVisibleState);
      document.removeEventListener('visibilitychange', refreshVisibleState);
    };
  }, [reloadApiServiceState]);

  const reloadApiServiceAccountGroups = useCallback(async () => {
    try {
      const groups = await getCodexAccountGroups();
      setApiServiceAccountGroups(groups);
    } catch (error) {
      console.error('Failed to load Codex API service account groups:', error);
    }
  }, []);

  const reloadApiServiceModelProviders = useCallback(async () => {
    try {
      const providers = await listCodexModelProviders();
      setApiServiceModelProviders(providers);
    } catch (error) {
      console.error('Failed to load Codex API service model providers:', error);
    }
  }, []);

  React.useEffect(() => {
    if (!isMenuVisiblePlatform('codex')) return;
    void reloadApiServiceAccountGroups();
    void reloadApiServiceModelProviders();
  }, [reloadApiServiceAccountGroups, reloadApiServiceModelProviders]);

  const openCodexApiServiceModal = useCallback(
    (mode: 'panel' | 'members' | 'providers') => {
      setApiServiceModalMode(mode);
      setShowApiServiceModal(true);
      void reloadApiServiceState();
      void reloadApiServiceAccountGroups();
      void reloadApiServiceModelProviders();
    },
    [reloadApiServiceAccountGroups, reloadApiServiceModelProviders, reloadApiServiceState],
  );

  const runApiServiceStateMutation = useCallback(
    async (
      action: () => Promise<CodexLocalAccessState>,
      successMessage: string,
    ) => {
      setApiServiceSaving(true);
      try {
        const nextState = await action();
        setApiServiceState(nextState);
        setApiServiceMessage({ text: successMessage, tone: 'success' });
        return nextState;
      } catch (error) {
        setApiServiceMessage({
          text: t('dashboard.apiServices.actionFailed', {
            defaultValue: 'API 服务操作失败：{{error}}',
            error: error instanceof Error ? error.message : String(error),
          }),
          tone: 'error',
        });
        throw error;
      } finally {
        setApiServiceSaving(false);
      }
    },
    [t],
  );

  const handleToggleCodexApiService = useCallback(async () => {
    const collection = apiServiceState?.collection;
    if (!collection) {
      setApiServiceMessage({
        text: t('dashboard.apiServices.needCodexAccounts', '请先在仪表盘服务控制台添加 Codex API 服务账号集合。'),
        tone: 'error',
      });
      openCodexApiServiceModal('members');
      return;
    }

    setApiServiceBusy('toggle');
    try {
      const nextState = await codexLocalAccessService.setCodexLocalAccessEnabled(!collection.enabled);
      setApiServiceState(nextState);
      scheduleApiServiceStateRefresh();
      setApiServiceMessage({
        text: collection.enabled
          ? t('dashboard.apiServices.disabled', 'Codex API 服务已停用')
          : t('dashboard.apiServices.enabled', 'Codex API 服务已启用'),
        tone: 'success',
      });
    } catch (error) {
      setApiServiceMessage({
        text: t('dashboard.apiServices.actionFailed', {
          defaultValue: 'API 服务操作失败：{{error}}',
          error: error instanceof Error ? error.message : String(error),
        }),
        tone: 'error',
      });
    } finally {
      setApiServiceBusy(null);
    }
  }, [apiServiceState?.collection, openCodexApiServiceModal, scheduleApiServiceStateRefresh, t]);

  const handleActivateCodexApiService = useCallback(async () => {
    const collection = apiServiceState?.collection;
    if (!collection) {
      setApiServiceMessage({
        text: t('dashboard.apiServices.needCodexAccounts', '请先在仪表盘服务控制台添加 Codex API 服务账号集合。'),
        tone: 'error',
      });
      openCodexApiServiceModal('members');
      return;
    }

    setApiServiceBusy('activate');
    try {
      let nextState = apiServiceState;
      if (!collection.enabled) {
        nextState = await codexLocalAccessService.setCodexLocalAccessEnabled(true);
        setApiServiceState(nextState);
      }
      nextState = await codexLocalAccessService.activateCodexLocalAccess();
      setApiServiceState(nextState);
      scheduleApiServiceStateRefresh();
      setApiServiceMessage({
        text: t('dashboard.apiServices.activated', '已切换到 Codex API 服务'),
        tone: 'success',
      });
    } catch (error) {
      setApiServiceMessage({
        text: t('dashboard.apiServices.actionFailed', {
          defaultValue: 'API 服务操作失败：{{error}}',
          error: error instanceof Error ? error.message : String(error),
        }),
        tone: 'error',
      });
    } finally {
      setApiServiceBusy(null);
    }
  }, [apiServiceState, openCodexApiServiceModal, scheduleApiServiceStateRefresh, t]);

  const handleApiServiceAddressKindChange = useCallback((value: string) => {
    const next = normalizeDashboardLocalAccessAddressKind(value);
    setApiServiceAddressKind(next);
    persistDashboardLocalAccessAddressKind(next);
  }, []);

  const selectedApiServiceAddressKind: CodexLocalAccessAddressKind =
    apiServiceAddressKind === 'lan' && apiServiceState?.lanBaseUrl ? 'lan' : 'local';

  const apiServiceAddressOptions = useMemo(
    () => [
      {
        value: 'local',
        label: t('codex.localAccess.addressLocal', '本机'),
      },
      ...(apiServiceState?.lanBaseUrl
        ? [
            {
              value: 'lan',
              label: t('codex.localAccess.addressLan', '局域网'),
            },
          ]
        : []),
    ],
    [apiServiceState?.lanBaseUrl, t],
  );

  const apiServiceModalSelectedIds = useMemo(
    () => [...(apiServiceState?.collection?.accountIds ?? [])],
    [apiServiceState?.collection?.accountIds],
  );

  const handleSaveApiServiceAccounts = useCallback(
    async (
      accountIds: string[],
      options: {
        restrictFreeAccounts: boolean;
        autoIncludeNewAccounts: boolean;
      },
    ) => {
      await runApiServiceStateMutation(
        () => codexLocalAccessService.saveCodexLocalAccessAccounts(
          accountIds,
          options.restrictFreeAccounts,
          options.autoIncludeNewAccounts,
        ),
        t('codex.localAccess.saveSuccess', 'API 服务集合已更新'),
      );
    },
    [runApiServiceStateMutation, t],
  );

  const handleSaveApiServiceProviders = useCallback(
    async (
      providerIds: string[],
      options: {
        autoIncludeNewProviders: boolean;
      },
    ) => {
      await runApiServiceStateMutation(
        () =>
          codexLocalAccessService.saveCodexLocalAccessProviders(
            providerIds,
            options.autoIncludeNewProviders,
          ),
        t('codex.localAccess.providerSaveSuccess', 'API 服务供应商已更新'),
      );
    },
    [runApiServiceStateMutation, t],
  );

  const handleClearApiServiceStats = useCallback(async () => {
    await runApiServiceStateMutation(
      () => codexLocalAccessService.clearCodexLocalAccessStats(),
      t('codex.localAccess.clearStatsSuccess', 'API 服务统计已清空'),
    );
  }, [runApiServiceStateMutation, t]);

  const handleRotateApiServiceKey = useCallback(async () => {
    await runApiServiceStateMutation(
      () => codexLocalAccessService.rotateCodexLocalAccessApiKey(),
      t('codex.localAccess.rotateSuccess', 'API 服务密钥已重置'),
    );
  }, [runApiServiceStateMutation, t]);

  const handleUpdateApiServicePort = useCallback(
    async (port: number) => {
      await runApiServiceStateMutation(
        () => codexLocalAccessService.updateCodexLocalAccessPort(port),
        t('codex.localAccess.portSaveSuccess', 'API 服务端口已更新'),
      );
    },
    [runApiServiceStateMutation, t],
  );

  const handleUpdateApiServiceRoutingStrategy = useCallback(
    async (strategy: CodexLocalAccessRoutingStrategy) => {
      await runApiServiceStateMutation(
        () => codexLocalAccessService.updateCodexLocalAccessRoutingStrategy(strategy),
        t('codex.localAccess.routingSaveSuccess', 'API 服务调度策略已更新'),
      );
    },
    [runApiServiceStateMutation, t],
  );

  const handleUpdateApiServiceCustomRouting = useCallback(
    async (rules: CodexLocalAccessCustomRoutingRule[]) => {
      await runApiServiceStateMutation(
        () => codexLocalAccessService.updateCodexLocalAccessCustomRouting(rules),
        t('codex.localAccess.customRoutingSaveSuccess', 'API 服务自定义调度已更新'),
      );
    },
    [runApiServiceStateMutation, t],
  );

  const handleUpdateApiServiceAccessScope = useCallback(
    async (accessScope: CodexLocalAccessScope) => {
      await runApiServiceStateMutation(
        () => codexLocalAccessService.updateCodexLocalAccessAccessScope(accessScope),
        t('codex.localAccess.accessScopeSaveSuccess', 'API 服务访问范围已更新'),
      );
    },
    [runApiServiceStateMutation, t],
  );

  const handleUpdateApiServiceUpstreamProxyMode = useCallback(
    async (upstreamProxyMode: CodexLocalAccessUpstreamProxyMode) => {
      await runApiServiceStateMutation(
        () => codexLocalAccessService.updateCodexLocalAccessUpstreamProxyMode(upstreamProxyMode),
        t('codex.localAccess.upstreamProxySaveSuccess', 'API 服务上游连接方式已更新'),
      );
    },
    [runApiServiceStateMutation, t],
  );

  const handleUpdateApiServiceSourceMode = useCallback(
    async (sourceMode: CodexLocalAccessSourceMode) => {
      await runApiServiceStateMutation(
        () => codexLocalAccessService.updateCodexLocalAccessSourceMode(sourceMode),
        t('codex.localAccess.sourceModeSaveSuccess', 'API 服务来源模式已更新'),
      );
    },
    [runApiServiceStateMutation, t],
  );

  const handleUpdateApiServiceWebSocketMode = useCallback(
    async (webSocketMode: CodexLocalAccessWebSocketMode) => {
      await runApiServiceStateMutation(
        () => codexLocalAccessService.updateCodexLocalAccessWebSocketMode(webSocketMode),
        t('codex.localAccess.webSocketModeSaveSuccess', 'API 服务 WS 协议模式已更新'),
      );
    },
    [runApiServiceStateMutation, t],
  );

  const handleUpdateApiServiceBoundOAuthAccount = useCallback(
    async (accountId: string | null) => {
      await runApiServiceStateMutation(
        () => codexLocalAccessService.updateCodexLocalAccessBoundOAuthAccount(accountId),
        t('codex.localAccess.oauthBinding.saveSuccess', 'API 服务 OAuth 绑定已更新'),
      );
    },
    [runApiServiceStateMutation, t],
  );

  const handleKillApiServicePort = useCallback(async () => {
    const port = apiServiceState?.collection?.port;
    const confirmed = await confirmDialog(
      t('codex.localAccess.killPortConfirmMessage', {
        port,
        defaultValue:
          '将强制结束占用本机 {{port}} 端口的其他进程，然后重新启动 API 服务。确认继续吗？',
      }),
      {
        title: t('codex.localAccess.killPortTitle', '清理 API 服务端口'),
        okLabel: t('codex.localAccess.killPortAction', '清理端口'),
        cancelLabel: t('common.cancel', '取消'),
      },
    );
    if (!confirmed) return null;

    setApiServicePortCleanupBusy(true);
    try {
      const result = await codexLocalAccessService.killCodexLocalAccessPort();
      setApiServiceState(result.state);
      scheduleApiServiceStateRefresh();
      setApiServiceMessage({
        text:
          result.portChanged
            ? t('codex.localAccess.killPortChanged', {
                previousPort: result.previousPort,
                currentPort: result.currentPort,
                defaultValue:
                  '原端口 {{previousPort}} 未能释放，已自动切换到 {{currentPort}}',
              })
            : result.killedCount > 0
            ? t('codex.localAccess.killPortSuccess', {
                count: result.killedCount,
                defaultValue: '已清理 {{count}} 个占用端口的进程',
              })
            : t('codex.localAccess.killPortSuccessNone', '没有发现需要清理的占用进程'),
        tone: 'success',
      });
      return result;
    } catch (error) {
      setApiServiceMessage({
        text: t('dashboard.apiServices.actionFailed', {
          defaultValue: 'API 服务操作失败：{{error}}',
          error: error instanceof Error ? error.message : String(error),
        }),
        tone: 'error',
      });
      throw error;
    } finally {
      setApiServicePortCleanupBusy(false);
    }
  }, [apiServiceState?.collection?.port, scheduleApiServiceStateRefresh, t]);

  const handleTestApiService = useCallback(async (): Promise<CodexLocalAccessTestResult> => {
    if (!apiServiceState?.collection) {
      throw new Error(t('codex.localAccess.testUnavailable', '当前 API 服务地址不可用'));
    }
    setApiServiceTesting(true);
    try {
      return await codexLocalAccessService.testCodexLocalAccess();
    } finally {
      setApiServiceTesting(false);
    }
  }, [apiServiceState?.collection, t]);

  const toggleDashboardCardCollapse = useCallback((platform: keyof DashboardCardCollapseState) => {
    setDashboardCardCollapse((prev) => ({
      ...prev,
      [platform]: !prev[platform],
    }));
  }, []);

  const handleSaveTags = async (newTags: string[]) => {
    if (!tagModalState) return;
    try {
      const accountId = tagModalState.accountId;
      switch (tagModalState.platform) {
        case 'antigravity':
          await useAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
        case 'codex':
          await useCodexAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
        case 'github-copilot':
          await useGitHubCopilotAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
        case 'windsurf':
          await useWindsurfAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
        case 'kiro':
          await useKiroAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
        case 'cursor':
          await useCursorAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
        case 'gemini':
          await useGeminiAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
        case 'codebuddy':
          await useCodebuddyAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
        case 'codebuddy_cn':
          await useCodebuddyCnAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
        case 'qoder':
          await useQoderAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
        case 'trae':
          await useTraeAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
        case 'workbuddy':
          await useWorkbuddyAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
        case 'zed':
          await useZedAccountStore.getState().updateAccountTags(accountId, newTags);
          break;
      }
      setTagModalState(null);
    } catch (error) {
      console.error('Save tags failed:', error);
    }
  };

  const { orderedEntryIds, hiddenEntryIds, platformGroups, setHiddenEntry } = usePlatformLayoutStore();
  const hiddenEntrySet = useMemo(() => new Set(hiddenEntryIds), [hiddenEntryIds]);
  const visibleEntryOrder = useMemo(
    () =>
      orderedEntryIds.filter(
        (entryId) =>
          !hiddenEntrySet.has(entryId) &&
          resolveEntryPlatformIds(entryId, platformGroups).some(isMenuVisiblePlatform),
      ),
    [orderedEntryIds, hiddenEntrySet, platformGroups],
  );
  const visiblePlatformOrder = useMemo(
    () =>
      visibleEntryOrder
        .map((entryId) => {
          const platformIds = resolveEntryPlatformIds(entryId, platformGroups).filter(isMenuVisiblePlatform);
          const defaultPlatformId = resolveEntryDefaultPlatformId(entryId, platformGroups);
          return defaultPlatformId && platformIds.includes(defaultPlatformId)
            ? defaultPlatformId
            : platformIds[0];
        })
        .filter((platformId): platformId is PlatformId => !!platformId),
    [visibleEntryOrder, platformGroups],
  );
  const [privacyModeEnabled, setPrivacyModeEnabled] = React.useState<boolean>(() =>
    isPrivacyModeEnabledByDefault()
  );
  const maskAccountText = React.useCallback(
    (value?: string | null) => maskSensitiveValue(value, privacyModeEnabled),
    [privacyModeEnabled],
  );
  const [agDisplayGroups, setAgDisplayGroups] = React.useState<DisplayGroup[]>([]);

  React.useEffect(() => {
    const syncPrivacyMode = () => {
      setPrivacyModeEnabled(isPrivacyModeEnabledByDefault());
    };

    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') {
        syncPrivacyMode();
      }
    };

    window.addEventListener('focus', syncPrivacyMode);
    window.addEventListener('storage', syncPrivacyMode);
    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => {
      window.removeEventListener('focus', syncPrivacyMode);
      window.removeEventListener('storage', syncPrivacyMode);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, []);


  // Antigravity Data
  const {
    accounts: agAccounts,
    currentAccount: agCurrent,
    switchAccount: switchAgAccount,
    fetchAccounts: fetchAgAccounts,
    fetchCurrentAccount: fetchAgCurrent
  } = useAccountStore();

  // Codex Data
  const {
    accounts: codexAccounts,
    currentAccount: codexCurrent,
    switchAccount: switchCodexAccount,
    fetchAccounts: fetchCodexAccounts,
    fetchCurrentAccount: fetchCodexCurrent
  } = useCodexAccountStore();

  // GitHub Copilot Data
  const {
    accounts: githubCopilotAccounts,
    currentAccountId: githubCopilotCurrentId,
    fetchAccounts: fetchGitHubCopilotAccounts,
    switchAccount: switchGitHubCopilotAccount,
  } = useGitHubCopilotAccountStore();

  // Windsurf Data
  const {
    accounts: windsurfAccounts,
    currentAccountId: windsurfCurrentId,
    fetchAccounts: fetchWindsurfAccounts,
    switchAccount: switchWindsurfAccount,
  } = useWindsurfAccountStore();

  // Kiro Data
  const {
    accounts: kiroAccounts,
    currentAccountId: kiroCurrentId,
    fetchAccounts: fetchKiroAccounts,
    switchAccount: switchKiroAccount,
  } = useKiroAccountStore();

  // Cursor Data
  const {
    accounts: cursorAccounts,
    currentAccountId: cursorCurrentId,
    fetchAccounts: fetchCursorAccounts,
    switchAccount: switchCursorAccount,
  } = useCursorAccountStore();

  // Gemini Data
  const {
    accounts: geminiAccounts,
    currentAccountId: geminiCurrentId,
    fetchAccounts: fetchGeminiAccounts,
    switchAccount: switchGeminiAccount,
  } = useGeminiAccountStore();

  const {
    accounts: codebuddyAccounts,
    currentAccountId: codebuddyCurrentId,
    fetchAccounts: fetchCodebuddyAccounts,
    switchAccount: switchCodebuddyAccount,
  } = useCodebuddyAccountStore();

  const {
    accounts: codebuddyCnAccounts,
    currentAccountId: codebuddyCnCurrentId,
    fetchAccounts: fetchCodebuddyCnAccounts,
    switchAccount: switchCodebuddyCnAccount,
  } = useCodebuddyCnAccountStore();

  const {
    accounts: qoderAccounts,
    currentAccountId: qoderCurrentId,
    fetchAccounts: fetchQoderAccounts,
    switchAccount: switchQoderAccount,
  } = useQoderAccountStore();

  const {
    accounts: traeAccounts,
    currentAccountId: traeCurrentId,
    fetchAccounts: fetchTraeAccounts,
    switchAccount: switchTraeAccount,
  } = useTraeAccountStore();

  const {
    accounts: workbuddyAccounts,
    currentAccountId: workbuddyCurrentId,
    fetchAccounts: fetchWorkbuddyAccounts,
    switchAccount: switchWorkbuddyAccount,
  } = useWorkbuddyAccountStore();

  const {
    accounts: zedAccounts,
    currentAccountId: zedCurrentId,
    fetchAccounts: fetchZedAccounts,
    switchAccount: switchZedAccount,
  } = useZedAccountStore();

  const agCurrentId = agCurrent?.id;
  const codexCurrentId = codexCurrent?.id;

  const agCurrentAccount = useMemo(() => {
    return resolveDashboardCurrentAccount(agAccounts, agCurrentId, agCurrent);
  }, [agAccounts, agCurrent, agCurrentId]);

  const codexCurrentAccount = useMemo(() => {
    return resolveDashboardCurrentAccount(codexAccounts, codexCurrentId, codexCurrent);
  }, [codexAccounts, codexCurrent, codexCurrentId]);

  const codexAccountIdSignature = useMemo(
    () => codexAccounts.map((account) => account.id).sort().join('\n'),
    [codexAccounts],
  );

  const codexSpeedSummary = useMemo(() => {
    const total = codexAccounts.length;
    const standard = codexAccounts.filter(
      (account) => (account.app_speed ?? 'standard') === 'standard',
    ).length;
    const fast = codexAccounts.filter(
      (account) => (account.app_speed ?? 'standard') === 'fast',
    ).length;
    const active: CodexAppSpeed | null =
      total > 0 && standard === total
        ? 'standard'
        : total > 0 && fast === total
          ? 'fast'
          : null;
    return { active, fast, standard, total };
  }, [codexAccounts]);

  const applyCodexSpeedToAllAccounts = useCallback(
    async (speed: CodexAppSpeed) => {
      if (apiServiceSpeedSaving) {
        return { total: codexSpeedSummary.total };
      }

      const targetAccounts = useCodexAccountStore.getState().accounts;
      if (targetAccounts.length === 0) {
        throw new Error(t('dashboard.apiServices.codexSpeedNoAccounts', '暂无 Codex 账号可设置速度'));
      }

      setApiServiceSpeedSaving(speed);
      try {
        let firstFailure: unknown = null;
        let failureCount = 0;
        for (const account of targetAccounts) {
          if ((account.app_speed ?? 'standard') !== speed) {
            try {
              await codexService.updateCodexAccountAppSpeed(account.id, speed);
            } catch (error) {
              firstFailure = firstFailure ?? error;
              failureCount += 1;
            }
          }
        }
        try {
          await codexService.saveCodexApiServiceAppSpeed(speed);
        } catch (error) {
          firstFailure = firstFailure ?? error;
          failureCount += 1;
        }
        await Promise.allSettled([fetchCodexAccounts(), fetchCodexCurrent()]);
        if (firstFailure) {
          throw new Error(
            t('dashboard.apiServices.codexSpeedPartialFailed', {
              count: failureCount,
              error: firstFailure instanceof Error ? firstFailure.message : String(firstFailure),
              defaultValue: '{{count}} 项速度设置失败：{{error}}',
            }),
          );
        }
        return { total: targetAccounts.length };
      } finally {
        setApiServiceSpeedSaving(null);
      }
    },
    [apiServiceSpeedSaving, codexSpeedSummary.total, fetchCodexAccounts, fetchCodexCurrent, t],
  );

  const handleApplyCodexSpeedToAllAccounts = useCallback(
    async (speed: CodexAppSpeed) => {
      const speedLabel =
        speed === 'fast'
          ? t('codex.speed.fast', '快速')
          : t('codex.speed.standard', '标准');
      try {
        const result = await applyCodexSpeedToAllAccounts(speed);
        setApiServiceMessage({
          text: t('dashboard.apiServices.codexSpeedApplied', {
            count: result.total,
            speed: speedLabel,
            defaultValue: '已将 {{count}} 个 Codex 账号和 API 服务默认速度设置为{{speed}}',
          }),
          tone: 'success',
        });
      } catch (error) {
        setApiServiceMessage({
          text: t('dashboard.apiServices.codexSpeedFailed', {
            defaultValue: 'Codex 速度批量设置失败：{{error}}',
            error: error instanceof Error ? error.message : String(error),
          }),
          tone: 'error',
        });
      }
    },
    [applyCodexSpeedToAllAccounts, t],
  );

  React.useEffect(() => {
    if (!isMenuVisiblePlatform('codex')) return;
    void reloadApiServiceState({ silent: true });
  }, [codexAccountIdSignature, reloadApiServiceState]);

  React.useEffect(() => {
    let disposed = false;
    let deferredTimer: number | null = null;
    let deferredBatchTimer: number | null = null;

    const loadDisplayGroups = () => {
      getDisplayGroups()
        .then((groups) => {
          if (!disposed) {
            setAgDisplayGroups(groups);
          }
        })
        .catch((error) => {
          console.error('Failed to load display groups:', error);
        });
    };

    const immediateTasks: Array<() => Promise<unknown>> = [];
    if (isMenuVisiblePlatform('antigravity')) {
      immediateTasks.push(fetchAgAccounts, fetchAgCurrent);
      loadDisplayGroups();
    }
    if (isMenuVisiblePlatform('codex')) {
      immediateTasks.push(fetchCodexAccounts, fetchCodexCurrent);
    }
    if (immediateTasks.length > 0) {
      void Promise.allSettled(immediateTasks.map((task) => task()));
    }

    const deferredTasks: Array<() => Promise<unknown>> = [
      ...(isMenuVisiblePlatform('codex') ? [] : [fetchCodexAccounts, fetchCodexCurrent]),
      ...(isMenuVisiblePlatform('zed') ? [fetchZedAccounts] : []),
      ...(isMenuVisiblePlatform('github-copilot') ? [fetchGitHubCopilotAccounts] : []),
      ...(isMenuVisiblePlatform('windsurf') ? [fetchWindsurfAccounts] : []),
      ...(isMenuVisiblePlatform('kiro') ? [fetchKiroAccounts] : []),
      ...(isMenuVisiblePlatform('cursor') ? [fetchCursorAccounts] : []),
      ...(isMenuVisiblePlatform('gemini') ? [fetchGeminiAccounts] : []),
      ...(isMenuVisiblePlatform('codebuddy') ? [fetchCodebuddyAccounts] : []),
      ...(isMenuVisiblePlatform('codebuddy_cn') ? [fetchCodebuddyCnAccounts] : []),
      ...(isMenuVisiblePlatform('qoder') ? [fetchQoderAccounts] : []),
      ...(isMenuVisiblePlatform('trae') ? [fetchTraeAccounts] : []),
      ...(isMenuVisiblePlatform('workbuddy') ? [fetchWorkbuddyAccounts] : []),
    ];

    const loadDeferredPlatforms = () => {
      if (disposed) {
        return;
      }

      let nextTaskIndex = 0;
      const runNextBatch = () => {
        if (disposed || nextTaskIndex >= deferredTasks.length) {
          return;
        }

        const batch = deferredTasks.slice(
          nextTaskIndex,
          nextTaskIndex + DASHBOARD_DEFERRED_PREFETCH_BATCH_SIZE,
        );
        nextTaskIndex += batch.length;

        void Promise.allSettled(batch.map((task) => task()));

        if (nextTaskIndex < deferredTasks.length) {
          deferredBatchTimer = window.setTimeout(runNextBatch, DASHBOARD_DEFERRED_PREFETCH_BATCH_DELAY_MS);
        }
      };

      runNextBatch();
    };

    if (!dashboardStartupPrefetched) {
      dashboardStartupPrefetched = true;
      deferredTimer = window.setTimeout(loadDeferredPlatforms, DASHBOARD_DEFERRED_PREFETCH_DELAY_MS);
    }

    return () => {
      disposed = true;
      if (deferredTimer !== null) {
        window.clearTimeout(deferredTimer);
      }
      if (deferredBatchTimer !== null) {
        window.clearTimeout(deferredBatchTimer);
      }
    };
  }, []);

  // Statistics
  const stats = useMemo(() => {
    const platformAccountCounts: Record<PlatformId, number> = {
      antigravity: agAccounts.length,
      codex: codexAccounts.length,
      zed: zedAccounts.length,
      'github-copilot': githubCopilotAccounts.length,
      windsurf: windsurfAccounts.length,
      kiro: kiroAccounts.length,
      cursor: cursorAccounts.length,
      gemini: geminiAccounts.length,
      codebuddy: codebuddyAccounts.length,
      codebuddy_cn: codebuddyCnAccounts.length,
      qoder: qoderAccounts.length,
      trae: traeAccounts.length,
      workbuddy: workbuddyAccounts.length,
    };
    return {
      total: Object.entries(platformAccountCounts).reduce(
        (sum, [platformId, count]) =>
          isMenuVisiblePlatform(platformId as PlatformId) ? sum + count : sum,
        0,
      ),
      antigravity: agAccounts.length,
      codex: codexAccounts.length,
      zed: zedAccounts.length,
      githubCopilot: githubCopilotAccounts.length,
      windsurf: windsurfAccounts.length,
      kiro: kiroAccounts.length,
      cursor: cursorAccounts.length,
      gemini: geminiAccounts.length,
      codebuddy: codebuddyAccounts.length,
      codebuddy_cn: codebuddyCnAccounts.length,
      qoder: qoderAccounts.length,
      trae: traeAccounts.length,
      workbuddy: workbuddyAccounts.length,
    };
  }, [agAccounts, codexAccounts, zedAccounts, githubCopilotAccounts, windsurfAccounts, kiroAccounts, cursorAccounts, geminiAccounts, codebuddyAccounts, codebuddyCnAccounts, qoderAccounts, traeAccounts, workbuddyAccounts]);

  const dashboardAvailableTags = useMemo(() => {
    const tagSet = new Set<string>();
    const allAccounts = [
      ...agAccounts,
      ...codexAccounts,
      ...zedAccounts,
      ...githubCopilotAccounts,
      ...windsurfAccounts,
      ...kiroAccounts,
      ...cursorAccounts,
      ...geminiAccounts,
      ...codebuddyAccounts,
      ...codebuddyCnAccounts,
      ...qoderAccounts,
      ...traeAccounts,
      ...workbuddyAccounts,
    ];
    for (const acc of allAccounts) {
      if (acc.tags) {
        for (const tag of acc.tags) {
          tagSet.add(tag);
        }
      }
    }
    return Array.from(tagSet).sort((a, b) => a.localeCompare(b));
  }, [agAccounts, codexAccounts, zedAccounts, githubCopilotAccounts, windsurfAccounts, kiroAccounts, cursorAccounts, geminiAccounts, codebuddyAccounts, codebuddyCnAccounts, qoderAccounts, traeAccounts, workbuddyAccounts]);


  // Refresh States
  const [refreshing, setRefreshing] = React.useState<Set<string>>(new Set());
  const [switching, setSwitching] = React.useState<Set<string>>(new Set());
  const [cardRefreshing, setCardRefreshing] = React.useState<{
    ag: boolean;
    codex: boolean;
    zed: boolean;
    githubCopilot: boolean;
    windsurf: boolean;
    kiro: boolean;
    cursor: boolean;
    gemini: boolean;
    codebuddy: boolean;
    codebuddyCn: boolean;
    qoder: boolean;
    trae: boolean;
    workbuddy: boolean;
  }>({
    ag: false,
    codex: false,
    zed: false,
    githubCopilot: false,
    windsurf: false,
    kiro: false,
    cursor: false,
    gemini: false,
    codebuddy: false,
    codebuddyCn: false,
    qoder: false,
    trae: false,
    workbuddy: false,
  });

  // Refresh Handlers
  const handleRefreshAg = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing(prev => new Set(prev).add(accountId));
    try {
      await useAccountStore.getState().refreshQuota(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing(prev => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshCodex = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing(prev => new Set(prev).add(accountId));
    try {
      await useCodexAccountStore.getState().refreshQuota(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing(prev => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshZed = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useZedAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshGitHubCopilot = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing(prev => new Set(prev).add(accountId));
    try {
      await useGitHubCopilotAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing(prev => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshWindsurf = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useWindsurfAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshKiro = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useKiroAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshCursor = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useCursorAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshGemini = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useGeminiAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshAgCard = async () => {
    if (cardRefreshing.ag) return;
    setCardRefreshing(prev => ({ ...prev, ag: true }));
    const idsToRefresh = Array.from(new Set([agCurrentAccount?.id, agRecommended?.id].filter(Boolean))) as string[];
    try {
      for (const id of idsToRefresh) {
        await useAccountStore.getState().refreshQuota(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing(prev => ({ ...prev, ag: false }));
    }
  };

  const handleRefreshCodexCard = async () => {
    if (cardRefreshing.codex) return;
    setCardRefreshing(prev => ({ ...prev, codex: true }));
    const idsToRefresh = Array.from(new Set([codexCurrentAccount?.id, codexRecommended?.id].filter(Boolean))) as string[];
    try {
      for (const id of idsToRefresh) {
        await useCodexAccountStore.getState().refreshQuota(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing(prev => ({ ...prev, codex: false }));
    }
  };

  const handleRefreshZedCard = async () => {
    if (cardRefreshing.zed) return;
    setCardRefreshing((prev) => ({ ...prev, zed: true }));
    const idsToRefresh = [zedCurrent?.id, zedRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useZedAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, zed: false }));
    }
  };

  const handleRefreshGitHubCopilotCard = async () => {
    if (cardRefreshing.githubCopilot) return;
    setCardRefreshing(prev => ({ ...prev, githubCopilot: true }));
    const idsToRefresh = [githubCopilotCurrent?.id, githubCopilotRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useGitHubCopilotAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing(prev => ({ ...prev, githubCopilot: false }));
    }
  };

  const handleRefreshWindsurfCard = async () => {
    if (cardRefreshing.windsurf) return;
    setCardRefreshing((prev) => ({ ...prev, windsurf: true }));
    const idsToRefresh = [windsurfCurrent?.id, windsurfRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useWindsurfAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, windsurf: false }));
    }
  };

  const handleRefreshKiroCard = async () => {
    if (cardRefreshing.kiro) return;
    setCardRefreshing((prev) => ({ ...prev, kiro: true }));
    const idsToRefresh = [kiroCurrent?.id, kiroRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useKiroAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, kiro: false }));
    }
  };

  const handleRefreshCursorCard = async () => {
    if (cardRefreshing.cursor) return;
    setCardRefreshing((prev) => ({ ...prev, cursor: true }));
    const idsToRefresh = [cursorCurrent?.id, cursorRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useCursorAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, cursor: false }));
    }
  };

  const handleRefreshGeminiCard = async () => {
    if (cardRefreshing.gemini) return;
    setCardRefreshing((prev) => ({ ...prev, gemini: true }));
    const idsToRefresh = [geminiCurrent?.id, geminiRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useGeminiAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, gemini: false }));
    }
  };

  const handleSwitchGitHubCopilot = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchGitHubCopilotAccount(accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchZed = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchZedAccount(accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchWindsurf = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchWindsurfAccount(accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchKiro = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchKiroAccount(accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchCursor = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchCursorAccount(accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchGemini = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchGeminiAccount(accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshCodebuddy = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useCodebuddyAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshCodebuddyCn = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useCodebuddyCnAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshQoder = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useQoderAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshTrae = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useTraeAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshWorkbuddy = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useWorkbuddyAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshCodebuddyCard = async () => {
    if (cardRefreshing.codebuddy) return;
    setCardRefreshing((prev) => ({ ...prev, codebuddy: true }));
    const idsToRefresh = [codebuddyCurrent?.id, codebuddyRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useCodebuddyAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, codebuddy: false }));
    }
  };

  const handleRefreshCodebuddyCnCard = async () => {
    if (cardRefreshing.codebuddyCn) return;
    setCardRefreshing((prev) => ({ ...prev, codebuddyCn: true }));
    const idsToRefresh = Array.from(new Set([codebuddyCnCurrent?.id, codebuddyCnRecommended?.id].filter(Boolean))) as string[];
    try {
      for (const id of idsToRefresh) {
        await useCodebuddyCnAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, codebuddyCn: false }));
    }
  };

  const handleRefreshQoderCard = async () => {
    if (cardRefreshing.qoder) return;
    setCardRefreshing((prev) => ({ ...prev, qoder: true }));
    const idsToRefresh = [qoderCurrent?.id, qoderRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useQoderAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, qoder: false }));
    }
  };

  const handleRefreshTraeCard = async () => {
    if (cardRefreshing.trae) return;
    setCardRefreshing((prev) => ({ ...prev, trae: true }));
    const idsToRefresh = [traeCurrent?.id, traeRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useTraeAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, trae: false }));
    }
  };

  const handleRefreshWorkbuddyCard = async () => {
    if (cardRefreshing.workbuddy) return;
    setCardRefreshing((prev) => ({ ...prev, workbuddy: true }));
    const idsToRefresh = Array.from(new Set([workbuddyCurrent?.id, workbuddyRecommended?.id].filter(Boolean))) as string[];
    try {
      for (const id of idsToRefresh) {
        await useWorkbuddyAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, workbuddy: false }));
    }
  };

  const handleSwitchCodebuddy = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchCodebuddyAccount(accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchCodebuddyCn = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchCodebuddyCnAccount(accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchQoder = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchQoderAccount(accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchTrae = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchTraeAccount(accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchWorkbuddy = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchWorkbuddyAccount(accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  // Antigravity Recommendation Logic
  const agRecommended = useMemo(() => {
    if (agAccounts.length <= 1) return null;
    const currentId = agCurrentAccount?.id;

    // Simple logic: find account with highest overall quota that isn't current
    const others = agAccounts.filter((a) => {
      if (a.id === currentId) return false;
      if (a.disabled) return false;
      if (a.quota?.is_forbidden) return false;
      if (!a.quota?.models || a.quota.models.length === 0) return false;
      return true;
    });
    if (others.length === 0) return null;

    return others.reduce((prev, curr) => {
      // Calculate a score based on quotas
      const getScore = (acc: Account) => {
        if (!acc.quota?.models) return -1;
        // Average percentage of all models
        const total = acc.quota.models.reduce((sum, m) => sum + m.percentage, 0);
        return total / acc.quota.models.length;
      };

      return getScore(curr) > getScore(prev) ? curr : prev;
    });
  }, [agAccounts, agCurrentAccount?.id]);

  // Codex Recommendation Logic
  const codexRecommended = useMemo(() => {
    if (codexAccounts.length <= 1) return null;
    const currentId = codexCurrentAccount?.id;

    const others = codexAccounts.filter((a) => {
      if (a.id === currentId) return false;
      if (!a.quota) return false;
      return true;
    });
    if (others.length === 0) return null;

    return others.reduce((prev, curr) => {
      const getScore = (acc: CodexAccount) => {
        if (!acc.quota) return -1;
        return (acc.quota.hourly_percentage + acc.quota.weekly_percentage) / 2;
      };
      return getScore(curr) > getScore(prev) ? curr : prev;
    });
  }, [codexAccounts, codexCurrentAccount?.id]);

  const githubCopilotCurrent = useMemo(
    () => resolveDashboardCurrentAccount(githubCopilotAccounts, githubCopilotCurrentId),
    [githubCopilotAccounts, githubCopilotCurrentId],
  );

  const windsurfCurrent = useMemo(
    () => resolveDashboardCurrentAccount(windsurfAccounts, windsurfCurrentId),
    [windsurfAccounts, windsurfCurrentId],
  );

  const kiroCurrent = useMemo(
    () => resolveDashboardCurrentAccount(kiroAccounts, kiroCurrentId),
    [kiroAccounts, kiroCurrentId],
  );

  const cursorCurrent = useMemo(
    () => resolveDashboardCurrentAccount(cursorAccounts, cursorCurrentId),
    [cursorAccounts, cursorCurrentId],
  );

  const geminiCurrent = useMemo(
    () => resolveDashboardCurrentAccount(geminiAccounts, geminiCurrentId),
    [geminiAccounts, geminiCurrentId],
  );

  const codebuddyCurrent = useMemo(
    () => resolveDashboardCurrentAccount(codebuddyAccounts, codebuddyCurrentId),
    [codebuddyAccounts, codebuddyCurrentId],
  );

  const codebuddyCnCurrent = useMemo(
    () => resolveDashboardCurrentAccount(codebuddyCnAccounts, codebuddyCnCurrentId),
    [codebuddyCnAccounts, codebuddyCnCurrentId],
  );

  const qoderCurrent = useMemo(
    () => resolveDashboardCurrentAccount(qoderAccounts, qoderCurrentId),
    [qoderAccounts, qoderCurrentId],
  );

  const traeCurrent = useMemo(
    () => resolveDashboardCurrentAccount(traeAccounts, traeCurrentId),
    [traeAccounts, traeCurrentId],
  );

  const workbuddyCurrent = useMemo(
    () => resolveDashboardCurrentAccount(workbuddyAccounts, workbuddyCurrentId),
    [workbuddyAccounts, workbuddyCurrentId],
  );

  const zedCurrent = useMemo(
    () => resolveDashboardCurrentAccount(zedAccounts, zedCurrentId),
    [zedAccounts, zedCurrentId],
  );

  const githubCopilotRecommended = useMemo(() => {
    if (githubCopilotAccounts.length <= 1) return null;
    const currentId = githubCopilotCurrent?.id;
    const others = githubCopilotAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (acc: GitHubCopilotAccount) => {
      const scores = [acc.quota?.hourly_percentage, acc.quota?.weekly_percentage].filter(
        (value): value is number => typeof value === 'number',
      );
      if (scores.length === 0) return 101;
      return scores.reduce((sum, value) => sum + value, 0) / scores.length;
    };

    return others.reduce((prev, curr) => (getScore(curr) < getScore(prev) ? curr : prev));
  }, [githubCopilotAccounts, githubCopilotCurrent?.id]);

  const windsurfRecommended = useMemo(() => {
    if (windsurfAccounts.length <= 1) return null;
    const currentId = windsurfCurrent?.id;
    const others = windsurfAccounts.filter((account) => account.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: WindsurfAccount) => {
      const credits = getWindsurfCreditsSummary(account);
      const promptLeft = toFiniteNumber(credits.promptCreditsLeft);
      const addOnLeft = toFiniteNumber(credits.addOnCredits);

      if (promptLeft != null) {
        return promptLeft * 1000 + (addOnLeft ?? 0);
      }

      const quotaValues = [account.quota?.hourly_percentage, account.quota?.weekly_percentage].filter(
        (value): value is number => typeof value === 'number',
      );
      if (quotaValues.length > 0) {
        const avgUsed = quotaValues.reduce((sum, value) => sum + value, 0) / quotaValues.length;
        return 100 - avgUsed;
      }

      return (account.last_used || account.created_at || 0) / 1e9;
    };

    return others.reduce((prev, curr) => (getScore(curr) > getScore(prev) ? curr : prev));
  }, [windsurfAccounts, windsurfCurrent?.id]);

  const kiroRecommended = useMemo(() => {
    if (kiroAccounts.length <= 1) return null;
    const currentId = kiroCurrent?.id;
    const others = kiroAccounts.filter(
      (account) => account.id !== currentId && !isKiroAccountBanned(account),
    );
    if (others.length === 0) return null;

    const getScore = (account: KiroAccount) => {
      const credits = getKiroCreditsSummary(account);
      const promptLeft = toFiniteNumber(credits.promptCreditsLeft);
      const addOnLeft = toFiniteNumber(credits.addOnCredits);

      if (promptLeft != null) {
        return promptLeft * 1000 + (addOnLeft ?? 0);
      }

      const quotaValues = [account.quota?.hourly_percentage, account.quota?.weekly_percentage].filter(
        (value): value is number => typeof value === 'number',
      );
      if (quotaValues.length > 0) {
        const avgUsed = quotaValues.reduce((sum, value) => sum + value, 0) / quotaValues.length;
        return 100 - avgUsed;
      }

      return (account.last_used || account.created_at || 0) / 1e9;
    };

    return others.reduce((prev, curr) => (getScore(curr) > getScore(prev) ? curr : prev));
  }, [kiroAccounts, kiroCurrent?.id]);

  const cursorRecommended = useMemo(() => {
    if (cursorAccounts.length <= 1) return null;
    const currentId = cursorCurrent?.id;
    const others = cursorAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: CursorAccount) => {
      const usage = getCursorUsage(account);
      const planLimit = toFiniteNumber(usage.planLimitCents);
      const planUsedRaw = toFiniteNumber(usage.planUsedCents);
      const hasPlanBudget = planLimit != null && planLimit > 0;
      const planUsed = planUsedRaw != null ? Math.max(planUsedRaw, 0) : null;
      const remainingBudget = hasPlanBudget
        ? Math.max((planLimit ?? 0) - (planUsed ?? 0), 0)
        : -1;

      const totalUsedPercent = toFiniteNumber(
        usage.totalPercentUsed ??
        (hasPlanBudget && planUsed != null && planLimit != null && planLimit > 0
          ? (planUsed / planLimit) * 100
          : null),
      );
      const usedPercentList = [
        totalUsedPercent,
        toFiniteNumber(usage.autoPercentUsed),
        toFiniteNumber(usage.apiPercentUsed),
      ].filter((value): value is number => value != null);
      const avgUsedPercent = usedPercentList.length > 0
        ? usedPercentList.reduce((sum, value) => sum + value, 0) / usedPercentList.length
        : 101;

      return {
        hasPlanBudget,
        remainingBudget,
        avgUsedPercent,
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);

      // 优先推荐有明确套餐额度（limit > 0）的账号，避免 0/0 FREE 抢占推荐位。
      if (bestScore.hasPlanBudget !== candidateScore.hasPlanBudget) {
        return candidateScore.hasPlanBudget ? candidate : best;
      }

      // 主排序：按剩余额度（limit - used）降序。
      if (bestScore.remainingBudget !== candidateScore.remainingBudget) {
        return candidateScore.remainingBudget > bestScore.remainingBudget
          ? candidate
          : best;
      }

      // 兜底：同剩余额度时，已用百分比更低优先；再按最近使用时间。
      if (bestScore.avgUsedPercent !== candidateScore.avgUsedPercent) {
        return candidateScore.avgUsedPercent < bestScore.avgUsedPercent
          ? candidate
          : best;
      }

      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [cursorAccounts, cursorCurrent?.id]);

  const geminiRecommended = useMemo(() => {
    if (geminiAccounts.length <= 1) return null;
    const currentId = geminiCurrent?.id;
    const others = geminiAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: GeminiAccount) => {
      const tiers = getGeminiTierQuotaSummary(account);
      const remainingValues = [tiers.pro.remainingPercent, tiers.flash.remainingPercent].filter(
        (value): value is number => typeof value === 'number' && Number.isFinite(value),
      );
      const totalUsed = remainingValues.length > 0
        ? 100 - Math.min(...remainingValues)
        : null;
      return {
        remainingPercent: totalUsed == null ? -1 : 100 - totalUsed,
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);
      if (candidateScore.remainingPercent !== bestScore.remainingPercent) {
        return candidateScore.remainingPercent > bestScore.remainingPercent
          ? candidate
          : best;
      }
      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [geminiAccounts, geminiCurrent?.id]);

  const codebuddyRecommended = useMemo(() => {
    if (codebuddyAccounts.length <= 1) return null;
    const currentId = codebuddyCurrent?.id;
    const others = codebuddyAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: CodebuddyAccount) => {
      const resource = getCodebuddyResourceSummary(account);
      const extra = getCodebuddyExtraCreditSummary(account);
      const remain = resource?.remainPercent ?? (extra.remainPercent ?? -1);
      return {
        remainPercent: remain,
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);
      if (candidateScore.remainPercent !== bestScore.remainPercent) {
        return candidateScore.remainPercent > bestScore.remainPercent ? candidate : best;
      }
      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [codebuddyAccounts, codebuddyCurrent?.id]);


  const codebuddyCnRecommended = useMemo(() => {
    if (codebuddyCnAccounts.length <= 1) return null;
    const currentId = codebuddyCnCurrent?.id;
    const others = codebuddyCnAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: CodebuddyAccount) => {
      const model = getCodebuddyOfficialQuotaModel(account);
      // 只使用基础包进行计算，不包含加量包
      const baseResources = model.resources.filter(r => r.total > 0 || r.remain > 0);

      // 计算平均剩余百分比（剩余越多越好）
      let avgRemainPercent = -1;
      if (baseResources.length > 0) {
        const totalRemainPercent = baseResources.reduce((sum, r) => {
          const pct = r.remainPercent ?? (r.total > 0 ? Math.max(0, (r.remain / r.total) * 100) : 0);
          return sum + pct;
        }, 0);
        avgRemainPercent = totalRemainPercent / baseResources.length;
      }

      return {
        remaining: avgRemainPercent, // 剩余百分比越高越好
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);
      if (candidateScore.remaining !== bestScore.remaining) {
        return candidateScore.remaining > bestScore.remaining ? candidate : best;
      }
      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [codebuddyCnAccounts, codebuddyCnCurrent?.id]);

  const qoderRecommended = useMemo(() => {
    if (qoderAccounts.length <= 1) return null;
    const currentId = qoderCurrent?.id;
    const others = qoderAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: QoderAccount) => {
      const sub = getQoderSubscriptionInfo(account);
      const usedPercent = sub.totalUsagePercentage ?? sub.userQuota.percentage ?? 101;
      return {
        remaining: 100 - usedPercent,
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);
      if (candidateScore.remaining !== bestScore.remaining) {
        return candidateScore.remaining > bestScore.remaining ? candidate : best;
      }
      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [qoderAccounts, qoderCurrent?.id]);

  const traeRecommended = useMemo(() => {
    if (traeAccounts.length <= 1) return null;
    const currentId = traeCurrent?.id;
    const others = traeAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: TraeAccount) => {
      const usage = getTraeUsage(account);
      const usedPercent = usage.usedPercent ?? 101;
      return {
        remaining: 100 - usedPercent,
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);
      if (candidateScore.remaining !== bestScore.remaining) {
        return candidateScore.remaining > bestScore.remaining ? candidate : best;
      }
      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [traeAccounts, traeCurrent?.id]);

  const workbuddyRecommended = useMemo(() => {
    if (workbuddyAccounts.length <= 1) return null;
    const currentId = workbuddyCurrent?.id;
    const others = workbuddyAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: WorkbuddyAccount) => {
      const model = getWorkbuddyOfficialQuotaModel(account);
      // 只使用基础包进行计算，不包含加量包
      const baseResources = model.resources.filter(r => r.total > 0 || r.remain > 0);

      // 计算平均剩余百分比（剩余越多越好）
      let avgRemainPercent = -1;
      if (baseResources.length > 0) {
        const totalRemainPercent = baseResources.reduce((sum, r) => {
          const pct = r.remainPercent ?? (r.total > 0 ? Math.max(0, (r.remain / r.total) * 100) : 0);
          return sum + pct;
        }, 0);
        avgRemainPercent = totalRemainPercent / baseResources.length;
      }

      return {
        remaining: avgRemainPercent, // 剩余百分比越高越好
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);
      if (candidateScore.remaining !== bestScore.remaining) {
        return candidateScore.remaining > bestScore.remaining ? candidate : best;
      }
      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [workbuddyAccounts, workbuddyCurrent?.id]);

  const zedRecommended = useMemo(() => {
    if (zedAccounts.length <= 1) return null;
    const currentId = zedCurrent?.id;
    const others = zedAccounts.filter((account) => account.id !== currentId);
    if (others.length === 0) return null;

    return others.reduce((best, candidate) => {
      const bestScore = getZedRecommendationScore(best);
      const candidateScore = getZedRecommendationScore(candidate);
      if (candidateScore.remainingPercent !== bestScore.remainingPercent) {
        return candidateScore.remainingPercent > bestScore.remainingPercent
          ? candidate
          : best;
      }
      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [zedAccounts, zedCurrent?.id]);

  // Render Helpers
  const formatQuotaValue = (value: number) => {
    if (!Number.isFinite(value)) return '0';
    return new Intl.NumberFormat('en-US', { maximumFractionDigits: 2 }).format(Math.max(0, value));
  };

  const buildCodebuddyCategoryQuotaItems = (account: CodebuddyAccount): UnifiedQuotaMetric[] => {
    const groups = getCodebuddyQuotaCategoryGroups(account, (key, defaultValue) => t(key, defaultValue || key));
    return groups
      .filter((group) => group.visible)
      .map((group) => ({
        key: `category_${group.key}`,
        label: `${group.label} (${group.items.length})`,
        percentage: Math.max(0, Math.min(100, group.usedPercent)),
        progressPercent: Math.max(0, Math.min(100, group.usedPercent)),
        quotaClass: group.quotaClass,
        valueText: `${formatQuotaValue(group.used)} / ${formatQuotaValue(group.total)}`,
        used: group.used,
        total: group.total,
        left: group.remain,
        showProgress: true,
      }));
  };

  const renderPresentationQuotaItems = (
    presentation: UnifiedAccountPresentation,
    limit = 3,
  ) => {
    const quotaItems = presentation.quotaItems.slice(0, Math.max(0, limit));
    if (quotaItems.length === 0) {
      return <span className="no-data-text">{t('dashboard.noData', '暂无数据')}</span>;
    }

    return quotaItems.map((item) => {
      const progressPercent = Math.max(
        0,
        Math.min(100, item.progressPercent ?? item.percentage ?? 0),
      );
      return (
        <div key={item.key} className="mini-quota-row-stacked">
          <div className="mini-quota-header">
            <span className="model-name">{item.label}</span>
            <span className={`model-pct ${item.quotaClass || ''}`}>
              {item.valueText || '-'}
            </span>
          </div>
          {item.showProgress !== false && (
            <div className="mini-progress-track">
              <div
                className={`mini-progress-bar ${item.quotaClass || ''}`}
                style={{ width: `${progressPercent}%` }}
              />
            </div>
          )}
          {item.resetText && <div className="mini-reset-time">{item.resetText}</div>}
        </div>
      );
    });
  };

  const renderUnifiedAccountCard = ({
    presentation,
    onRefresh,
    onSwitch,
    isRefreshing,
    isSwitching,
    switchDisabled = false,
    sublineText,
    sublineTitle,
    maxMetrics = 3,
    onEditTags,
  }: {
    presentation: UnifiedAccountPresentation;
    onRefresh: () => void;
    onSwitch: () => void;
    isRefreshing: boolean;
    isSwitching: boolean;
    switchDisabled?: boolean;
    sublineText?: string;
    sublineTitle?: string;
    maxMetrics?: number;
    onEditTags?: () => void;
  }) => {
    const resolvedSublineText = sublineText || presentation.sublineText || '';
    const shouldShowPlan = Boolean(presentation.planLabel) && presentation.planLabel !== 'UNKNOWN';

    return (
      <div className="account-mini-card">
        <div className="account-mini-header">
          <div className="account-info-row">
            <span className="account-email" title={maskAccountText(presentation.displayName)}>
              {maskAccountText(presentation.displayName)}
            </span>
            {shouldShowPlan && (
              <span className={`tier-badge ${presentation.planClass}`}>{presentation.planLabel}</span>
            )}
          </div>
        </div>

        {resolvedSublineText && (
          <div
            className="account-mini-subline"
            title={sublineTitle || resolvedSublineText}
          >
            {resolvedSublineText}
          </div>
        )}

        <div className="account-mini-quotas">
          {renderPresentationQuotaItems(presentation, maxMetrics)}
        </div>

        <div className="account-mini-actions icon-only-row">
          {onEditTags && (
            <button
              className="mini-icon-btn"
              onClick={onEditTags}
              title={t('accounts.editTags', '编辑标签')}
            >
              <Tag size={14} />
            </button>
          )}
          <button
            className="mini-icon-btn"
            onClick={onRefresh}
            title={t('common.refresh', '刷新')}
            disabled={isRefreshing || isSwitching}
          >
            <RotateCw size={14} className={isRefreshing ? 'loading-spinner' : ''} />
          </button>
          <button
            className="mini-icon-btn"
            onClick={onSwitch}
            title={t('dashboard.switch', '切换')}
            disabled={isSwitching || switchDisabled}
          >
            {isSwitching ? <RotateCw size={14} className="loading-spinner" /> : <Play size={14} />}
          </button>
        </div>
      </div>
    );
  };

  const renderAgAccountContent = (account: Account | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildAntigravityAccountPresentation(account, agDisplayGroups, t);
    const quotaDisplayItems = presentation.quotaItems.slice(0, 4);

    return (
      <div className="account-mini-card">
        <div className="account-mini-header">
          <div className="account-info-row">
            <span className="account-email" title={maskAccountText(presentation.displayName)}>
              {maskAccountText(presentation.displayName)}
            </span>
            <span className={`tier-badge ${presentation.planClass}`}>{presentation.planLabel}</span>
          </div>
        </div>

        <div className="account-mini-quotas">
          {quotaDisplayItems.map((item) => (
            <div key={item.key} className="mini-quota-row-stacked">
              <div className="mini-quota-header">
                <span className="model-name">{item.label}</span>
                <span className={`model-pct ${item.quotaClass}`}>{item.valueText}</span>
              </div>
              <div className="mini-progress-track">
                <div
                  className={`mini-progress-bar ${item.quotaClass}`}
                  style={{ width: `${item.percentage}%` }}
                />
              </div>
              {item.resetText && (
                <div className="mini-reset-time">
                  {item.resetText}
                </div>
              )}
            </div>
          ))}
          {quotaDisplayItems.length === 0 && <span className="no-data-text">{t('dashboard.noData', '暂无数据')}</span>}
        </div>

        <div className="account-mini-actions icon-only-row">
          <button
            className="mini-icon-btn"
            onClick={() => setTagModalState({ accountId: account.id, platform: 'antigravity', tags: account.tags || [] })}
            title={t('accounts.editTags', '编辑标签')}
          >
            <Tag size={14} />
          </button>
          <button
            className="mini-icon-btn"
            onClick={() => handleRefreshAg(account.id)}
            title={t('common.refresh', '刷新')}
            disabled={refreshing.has(account.id)}
          >
            <RotateCw size={14} className={refreshing.has(account.id) ? 'loading-spinner' : ''} />
          </button>
          <button
            className="mini-icon-btn"
            onClick={() => switchAgAccount(account.id)}
            title={t('dashboard.switch', '切换')}
          >
            <Play size={14} />
          </button>
        </div>
      </div>
    );
  };

  const renderCodexAccountContent = (account: CodexAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildCodexAccountPresentation(account, t);
    return renderUnifiedAccountCard({
      presentation,
      onRefresh: () => handleRefreshCodex(account.id),
      onSwitch: () => switchCodexAccount(account.id),
      isRefreshing: refreshing.has(account.id),
      isSwitching: false,
      maxMetrics: 4,
      onEditTags: () => setTagModalState({ accountId: account.id, platform: 'codex', tags: account.tags || [] }),
    });
  };

  const renderZedAccountContent = (account: ZedAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildZedAccountPresentation(account, t);
    return renderUnifiedAccountCard({
      presentation,
      onRefresh: () => handleRefreshZed(account.id),
      onSwitch: () => handleSwitchZed(account.id),
      isRefreshing: refreshing.has(account.id),
      isSwitching: switching.has(account.id),
      onEditTags: () => setTagModalState({ accountId: account.id, platform: 'zed', tags: account.tags || [] }),
    });
  };

  const renderGitHubCopilotAccountContent = (account: GitHubCopilotAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildGitHubCopilotAccountPresentation(account, t);
    return renderUnifiedAccountCard({
      presentation,
      onRefresh: () => handleRefreshGitHubCopilot(account.id),
      onSwitch: () => handleSwitchGitHubCopilot(account.id),
      isRefreshing: refreshing.has(account.id),
      isSwitching: switching.has(account.id),
      onEditTags: () => setTagModalState({ accountId: account.id, platform: 'github-copilot', tags: account.tags || [] }),
    });
  };

  const renderWindsurfAccountContent = (account: WindsurfAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildWindsurfAccountPresentation(account, t);
    return renderUnifiedAccountCard({
      presentation,
      onRefresh: () => handleRefreshWindsurf(account.id),
      onSwitch: () => handleSwitchWindsurf(account.id),
      isRefreshing: refreshing.has(account.id),
      isSwitching: switching.has(account.id),
      onEditTags: () => setTagModalState({ accountId: account.id, platform: 'windsurf', tags: account.tags || [] }),
    });
  };

  const renderKiroAccountContent = (account: KiroAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildKiroAccountPresentation(account, t);
    return renderUnifiedAccountCard({
      presentation,
      onRefresh: () => handleRefreshKiro(account.id),
      onSwitch: () => handleSwitchKiro(account.id),
      isRefreshing: refreshing.has(account.id),
      isSwitching: switching.has(account.id),
      switchDisabled: presentation.isBanned,
      onEditTags: () => setTagModalState({ accountId: account.id, platform: 'kiro', tags: account.tags || [] }),
    });
  };

  const renderCursorAccountContent = (account: CursorAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildCursorAccountPresentation(account, t);
    const authIdText = (account.auth_id || '').trim();
    const maskedAuthIdText = authIdText ? maskAccountText(authIdText) : '--';
    return renderUnifiedAccountCard({
      presentation,
      onRefresh: () => handleRefreshCursor(account.id),
      onSwitch: () => handleSwitchCursor(account.id),
      isRefreshing: refreshing.has(account.id),
      isSwitching: switching.has(account.id),
      switchDisabled: presentation.isBanned,
      sublineText: `Auth ID: ${maskedAuthIdText}`,
      sublineTitle: `Auth ID: ${maskedAuthIdText}`,
      onEditTags: () => setTagModalState({ accountId: account.id, platform: 'cursor', tags: account.tags || [] }),
    });
  };

  const renderGeminiAccountContent = (account: GeminiAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const formatRelativeDuration = (seconds: number) => {
      const safe = Math.max(0, Math.floor(seconds));
      const totalMinutes = Math.floor(safe / 60);
      if (totalMinutes < 1) {
        return t('common.shared.time.lessThanMinute', '<1分钟');
      }
      const days = Math.floor(totalMinutes / (60 * 24));
      const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
      const minutes = totalMinutes % 60;
      if (days > 0 && hours > 0) {
        return t('common.shared.time.relativeDaysHours', '{{days}}天{{hours}}小时', { days, hours });
      }
      if (days > 0) {
        return t('common.shared.time.relativeDays', '{{days}}天', { days });
      }
      if (hours > 0 && minutes > 0) {
        return t('common.shared.time.relativeHoursMinutes', '{{hours}}小时{{minutes}}分钟', { hours, minutes });
      }
      if (hours > 0) {
        return t('common.shared.time.relativeHours', '{{hours}}小时', { hours });
      }
      return t('common.shared.time.relativeMinutes', '{{minutes}}分钟', { minutes });
    };

    const updatedAt = account.last_used || account.created_at || 0;
    const updatedDiffSeconds = Math.floor(Date.now() / 1000) - updatedAt;
    const updatedText = t('gemini.updated.label', 'Updated {{relative}} ago', {
      relative: formatRelativeDuration(updatedDiffSeconds),
    });
    const presentation = buildGeminiAccountPresentation(account, t);
    return renderUnifiedAccountCard({
      presentation,
      onRefresh: () => handleRefreshGemini(account.id),
      onSwitch: () => handleSwitchGemini(account.id),
      isRefreshing: refreshing.has(account.id),
      isSwitching: switching.has(account.id),
      sublineText: updatedText,
      onEditTags: () => setTagModalState({ accountId: account.id, platform: 'gemini', tags: account.tags || [] }),
    });
  };

  const renderCodebuddyAccountContent = (account: CodebuddyAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildCodebuddyAccountPresentation(account, t);
    const mergedQuotaItems = buildCodebuddyCategoryQuotaItems(account);
    return renderUnifiedAccountCard({
      presentation: {
        ...presentation,
        quotaItems: mergedQuotaItems,
      },
      onRefresh: () => handleRefreshCodebuddy(account.id),
      onSwitch: () => handleSwitchCodebuddy(account.id),
      isRefreshing: refreshing.has(account.id),
      isSwitching: switching.has(account.id),
      onEditTags: () => setTagModalState({ accountId: account.id, platform: 'codebuddy', tags: account.tags || [] }),
    });
  };

  const renderCodebuddyCnAccountContent = (account: CodebuddyAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildCodebuddyAccountPresentation(account, t);
    const mergedQuotaItems = buildCodebuddyCategoryQuotaItems(account);
    return renderUnifiedAccountCard({
      presentation: {
        ...presentation,
        quotaItems: mergedQuotaItems,
      },
      onRefresh: () => handleRefreshCodebuddyCn(account.id),
      onSwitch: () => handleSwitchCodebuddyCn(account.id),
      isRefreshing: refreshing.has(account.id),
      isSwitching: switching.has(account.id),
      onEditTags: () => setTagModalState({ accountId: account.id, platform: 'codebuddy_cn', tags: account.tags || [] }),
    });
  };

  const renderQoderAccountContent = (account: QoderAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildQoderAccountPresentation(account, t);
    return renderUnifiedAccountCard({
      presentation,
      onRefresh: () => handleRefreshQoder(account.id),
      onSwitch: () => handleSwitchQoder(account.id),
      isRefreshing: refreshing.has(account.id),
      isSwitching: switching.has(account.id),
      onEditTags: () => setTagModalState({ accountId: account.id, platform: 'qoder', tags: account.tags || [] }),
    });
  };

  const renderTraeAccountContent = (account: TraeAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildTraeAccountPresentation(account, t);
    return renderUnifiedAccountCard({
      presentation,
      onRefresh: () => handleRefreshTrae(account.id),
      onSwitch: () => handleSwitchTrae(account.id),
      isRefreshing: refreshing.has(account.id),
      isSwitching: switching.has(account.id),
      onEditTags: () => setTagModalState({ accountId: account.id, platform: 'trae', tags: account.tags || [] }),
    });
  };

  const renderWorkbuddyAccountContent = (account: WorkbuddyAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildWorkbuddyAccountPresentation(account, t);
    return renderUnifiedAccountCard({
      presentation,
      onRefresh: () => handleRefreshWorkbuddy(account.id),
      onSwitch: () => handleSwitchWorkbuddy(account.id),
      isRefreshing: refreshing.has(account.id),
      isSwitching: switching.has(account.id),
      onEditTags: () => setTagModalState({ accountId: account.id, platform: 'workbuddy', tags: account.tags || [] }),
    });
  };

  const platformCounts: Record<PlatformId, number> = {
    antigravity: stats.antigravity,
    codex: stats.codex,
    zed: stats.zed,
    'github-copilot': stats.githubCopilot,
    windsurf: stats.windsurf,
    kiro: stats.kiro,
    cursor: stats.cursor,
    gemini: stats.gemini,
    codebuddy: stats.codebuddy,
    codebuddy_cn: stats.codebuddy_cn,
    qoder: stats.qoder,
    trae: stats.trae,
    workbuddy: stats.workbuddy,
  };

  const entryCounts = useMemo(() => {
    const result = new Map<PlatformLayoutEntryId, number>();
    for (const entryId of visibleEntryOrder) {
      const platformIds = resolveEntryPlatformIds(entryId, platformGroups);
      const count = platformIds.reduce((sum, platformId) => sum + (platformCounts[platformId] ?? 0), 0);
      result.set(entryId, count);
    }
    return result;
  }, [visibleEntryOrder, platformGroups, platformCounts]);

  const visibleCardPlatformIds = visiblePlatformOrder;
  const isSinglePlatformMode = visibleCardPlatformIds.length === 1;
  const cardRows = useMemo(() => {
    const rows: PlatformId[][] = [];
    for (let i = 0; i < visibleCardPlatformIds.length; i += 2) {
      rows.push(visibleCardPlatformIds.slice(i, i + 2));
    }
    return rows;
  }, [visibleCardPlatformIds]);

  const handleHidePlatformCard = useCallback((platformId: PlatformId) => {
    const entryId = orderedEntryIds.find(
      (candidate) => resolveEntryDefaultPlatformId(candidate, platformGroups) === platformId,
    );
    if (!entryId) {
      return;
    }
    setHiddenEntry(entryId, true);
  }, [orderedEntryIds, platformGroups, setHiddenEntry]);

  const renderHideCardButton = (platformId: PlatformId) => (
    <button
      className="header-action-btn header-icon-btn"
      onClick={() => handleHidePlatformCard(platformId)}
      title={t('accounts.compact.hide', '隐藏')}
      aria-label={t('accounts.compact.hide', '隐藏')}
    >
      <EyeOff size={14} />
    </button>
  );

  const renderPlatformCard = (platformId: PlatformId) => {
    if (platformId === 'antigravity') {
      return (
        <div className="main-card antigravity-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <RobotIcon className="" style={{ width: 18, height: 18 }} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshAgCard}
                disabled={cardRefreshing.ag}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.ag ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderAgAccountContent(agCurrentAccount)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {agRecommended ? (
                renderAgAccountContent(agRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('overview')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'codex') {
      return (
        <div className="main-card codex-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <CodexIcon size={18} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshCodexCard}
                disabled={cardRefreshing.codex}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.codex ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderCodexAccountContent(codexCurrentAccount)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {codexRecommended ? (
                renderCodexAccountContent(codexRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('codex')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'zed') {
      return (
        <div className="main-card codex-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              {renderPlatformIcon(platformId, 18)}
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshZedCard}
                disabled={cardRefreshing.zed}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.zed ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderZedAccountContent(zedCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {zedRecommended ? (
                renderZedAccountContent(zedRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('zed')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'github-copilot') {
      return (
        <div className="main-card github-copilot-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <Github size={18} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshGitHubCopilotCard}
                disabled={cardRefreshing.githubCopilot}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.githubCopilot ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderGitHubCopilotAccountContent(githubCopilotCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {githubCopilotRecommended ? (
                renderGitHubCopilotAccountContent(githubCopilotRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('github-copilot')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'windsurf') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <WindsurfIcon className="" style={{ width: 18, height: 18 }} />
              <h3>Windsurf</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshWindsurfCard}
                disabled={cardRefreshing.windsurf}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.windsurf ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderWindsurfAccountContent(windsurfCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {windsurfRecommended ? (
                renderWindsurfAccountContent(windsurfRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('windsurf')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'kiro') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <KiroIcon style={{ width: 18, height: 18 }} />
              <h3>Kiro</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshKiroCard}
                disabled={cardRefreshing.kiro}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.kiro ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderKiroAccountContent(kiroCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {kiroRecommended ? (
                renderKiroAccountContent(kiroRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('kiro')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'cursor') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <CursorIcon style={{ width: 18, height: 18 }} />
              <h3>Cursor</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshCursorCard}
                disabled={cardRefreshing.cursor}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.cursor ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderCursorAccountContent(cursorCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {cursorRecommended ? (
                renderCursorAccountContent(cursorRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('cursor')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'gemini') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <GeminiIcon style={{ width: 18, height: 18 }} />
              <h3>Gemini Cli</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshGeminiCard}
                disabled={cardRefreshing.gemini}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.gemini ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderGeminiAccountContent(geminiCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {geminiRecommended ? (
                renderGeminiAccountContent(geminiRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('gemini')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'codebuddy') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <CodebuddyIcon style={{ width: 18, height: 18 }} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshCodebuddyCard}
                disabled={cardRefreshing.codebuddy}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.codebuddy ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderCodebuddyAccountContent(codebuddyCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {codebuddyRecommended ? (
                renderCodebuddyAccountContent(codebuddyRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('codebuddy')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'codebuddy_cn') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <CodebuddyIcon style={{ width: 18, height: 18 }} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshCodebuddyCnCard}
                disabled={cardRefreshing.codebuddyCn}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.codebuddyCn ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderCodebuddyCnAccountContent(codebuddyCnCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {codebuddyCnRecommended ? (
                renderCodebuddyCnAccountContent(codebuddyCnRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('codebuddy-cn')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'qoder') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <QoderIcon style={{ width: 18, height: 18 }} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshQoderCard}
                disabled={cardRefreshing.qoder}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.qoder ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderQoderAccountContent(qoderCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {qoderRecommended ? (
                renderQoderAccountContent(qoderRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('qoder')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'trae') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <TraeIcon style={{ width: 18, height: 18 }} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshTraeCard}
                disabled={cardRefreshing.trae}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.trae ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderTraeAccountContent(traeCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {traeRecommended ? (
                renderTraeAccountContent(traeRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('trae')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'workbuddy') {
      const workbuddyCollapsed = dashboardCardCollapse.workbuddy;
      return (
        <div
          className={`main-card windsurf-card main-card-collapsible ${workbuddyCollapsed ? 'main-card-collapsed' : ''}`}
          key={platformId}
        >
          <div className="main-card-header">
            <div className="header-title">
              <WorkbuddyIcon style={{ width: 18, height: 18 }} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <div className="header-action-group">
              <button
                className="header-action-btn"
                onClick={handleRefreshWorkbuddyCard}
                disabled={cardRefreshing.workbuddy}
                title={t('common.refresh', '刷新')}
              >
                <RotateCw size={14} className={cardRefreshing.workbuddy ? 'loading-spinner' : ''} />
                <span>{t('common.refresh', '刷新')}</span>
              </button>
              {renderHideCardButton(platformId)}
              <button
                className="header-action-btn header-collapse-btn"
                onClick={() => toggleDashboardCardCollapse('workbuddy')}
                title={workbuddyCollapsed ? t('common.expand', '展开') : t('common.collapse', '收起')}
                aria-label={workbuddyCollapsed ? t('common.expand', '展开') : t('common.collapse', '收起')}
              >
                <ChevronDown size={14} className={`collapse-arrow ${workbuddyCollapsed ? 'collapsed' : ''}`} />
              </button>
            </div>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderWorkbuddyAccountContent(workbuddyCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {workbuddyRecommended ? (
                renderWorkbuddyAccountContent(workbuddyRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('workbuddy')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    // Generic fallback for any unknown/future platform
    return (
      <div className="main-card windsurf-card" key={platformId}>
        <div className="main-card-header">
          <div className="header-title">
            {renderPlatformIcon(platformId, 18)}
            <h3>{getPlatformLabel(platformId, t)}</h3>
          </div>
          <div className="header-action-group">
            {renderHideCardButton(platformId)}
          </div>
        </div>

        <div className="split-content">
          <div className="split-half current-half">
            <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
            <div className="empty-slot-text">{t('dashboard.noData', '暂无数据')}</div>
          </div>

          <div className="split-divider"></div>

          <div className="split-half recommend-half">
            <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
            <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
          </div>
        </div>

        <button className="card-footer-action" onClick={() => onNavigate(PLATFORM_PAGE_MAP[platformId])}>
          {t('dashboard.viewAllAccounts', '查看所有账号')}
        </button>
      </div>
    );
  };

  const renderApiServiceConsole = () => {
    const codexCollection = apiServiceState?.collection ?? null;
    const codexRunning = Boolean(apiServiceState?.running);
    const codexStatus = !codexCollection
      ? t('dashboard.apiServices.unconfigured', '待配置')
      : codexRunning
        ? t('dashboard.apiServices.running', '运行中')
        : codexCollection.enabled
          ? t('dashboard.apiServices.enabledStatus', '已启用')
          : t('dashboard.apiServices.disabledStatus', '已停用');
    const codexTone = !codexCollection
      ? 'pending'
      : codexRunning
        ? 'running'
        : codexCollection.enabled
          ? 'enabled'
          : 'disabled';

    return (
      <section className="api-services-console" aria-label={t('dashboard.apiServices.title', 'API 服务控制台')}>
        <div className="api-services-head">
          <div>
            <span className="api-services-kicker">
              <Server size={14} />
              {t('dashboard.apiServices.kicker', '统一入口')}
            </span>
            <h2>{t('dashboard.apiServices.title', 'API 服务控制台')}</h2>
            <p>{t('dashboard.apiServices.desc', '所有平台的 API 服务启动、停用和状态观察集中在仪表盘完成；账号和平台专属功能仍保留在对应平台页面。')}</p>
          </div>
          <button className="header-action-btn" onClick={() => void reloadApiServiceState()} disabled={apiServiceBusy !== null}>
            <RotateCw size={14} className={apiServiceBusy === 'load' ? 'loading-spinner' : ''} />
            <span>{t('common.refresh', '刷新')}</span>
          </button>
        </div>

        {apiServiceMessage && (
          <div className={`api-services-message ${apiServiceMessage.tone ?? 'success'}`}>
            <span>{apiServiceMessage.text}</span>
            <button onClick={() => setApiServiceMessage(null)} aria-label={t('common.close', '关闭')}>×</button>
          </div>
        )}

        <div className="api-services-grid">
          {apiServicePlatformIds.map((platformId) => {
            const isCodex = platformId === 'codex';
            const memberCount = isCodex ? apiServiceState?.memberCount ?? 0 : 0;
            const baseUrl = isCodex
              ? apiServiceState?.baseUrl || (codexCollection ? `http://127.0.0.1:${codexCollection.port}/v1` : '-')
              : '-';
            const wsUrl = isCodex ? apiServiceState?.webSocketUrl || '-' : '-';
            const wsStatus = isCodex
              ? apiServiceState?.webSocketEnabled
                ? t('codex.localAccess.webSocketStatusReady', '可用')
                : codexCollection?.enabled
                  ? t('codex.localAccess.webSocketStatusStopped', '等待服务运行')
                  : t('codex.localAccess.webSocketStatusDisabled', '随服务停用')
              : '-';
            const cardStatus = isCodex
              ? codexStatus
              : t('dashboard.apiServices.reserved', '待接入');
            const cardTone = isCodex ? codexTone : 'reserved';
            const activateButtonClass = codexRunning
              ? 'btn btn-secondary api-service-action-start is-running'
              : 'btn btn-primary api-service-action-start';
            const toggleButtonClass = codexCollection?.enabled
              ? 'btn btn-danger api-service-action-toggle is-stop'
              : 'btn btn-success api-service-action-toggle is-enable';
            const speedButtonsDisabled =
              apiServiceSpeedSaving !== null ||
              apiServiceBusy !== null ||
              apiServiceSaving ||
              codexSpeedSummary.total === 0;
            const speedOptions: CodexAppSpeed[] = ['standard', 'fast'];

            return (
              <article className={`api-service-card tone-${cardTone}`} key={platformId}>
                <div className="api-service-card-top">
                  <div className="api-service-icon">{renderPlatformIcon(platformId, 22)}</div>
                  <div className="api-service-title">
                    <h3>{getPlatformLabel(platformId, t)}</h3>
                    <span className={`api-service-status tone-${cardTone}`}>{cardStatus}</span>
                  </div>
                </div>
                <div className="api-service-meta">
                  <span>
                    {t('dashboard.apiServices.accounts', {
                      defaultValue: '{{count}} 个账号',
                      count: memberCount,
                    })}
                  </span>
                  {isCodex && <span>WS {wsStatus}</span>}
                  <span>{t('dashboard.apiServices.pageVisible', '页面可见')}</span>
                </div>
                <div className="api-service-endpoint" title={baseUrl}>
                  <span>{t('codex.localAccess.baseUrl', '地址')}</span>
                  <code>{baseUrl}</code>
                </div>
                {isCodex && (
                  <div className="api-service-endpoint" title={wsUrl}>
                    <span>{t('codex.localAccess.webSocketUrl', 'WebSocket')}</span>
                    <code>{wsUrl}</code>
                  </div>
                )}
                {isCodex && (
                  <div className="api-service-speed-panel">
                    <div className="api-service-speed-title">
                      <span>{t('dashboard.apiServices.codexSpeedTitle', '全账号速度')}</span>
                      <small>
                        {t('dashboard.apiServices.codexSpeedCount', {
                          count: codexSpeedSummary.total,
                          defaultValue: '{{count}} 个账号',
                        })}
                      </small>
                    </div>
                    <div
                      className="api-service-speed-options"
                      role="group"
                      aria-label={t('dashboard.apiServices.codexSpeedTitle', '全账号速度')}
                    >
                      {speedOptions.map((speed) => {
                        const speedLabel =
                          speed === 'fast'
                            ? t('codex.speed.fast', '快速')
                            : t('codex.speed.standard', '标准');
                        const isSaving = apiServiceSpeedSaving === speed;
                        return (
                          <button
                            key={speed}
                            type="button"
                            className={`api-service-speed-btn ${speed} ${
                              codexSpeedSummary.active === speed ? 'is-active' : ''
                            }`}
                            onClick={() => void handleApplyCodexSpeedToAllAccounts(speed)}
                            disabled={speedButtonsDisabled}
                            title={t('dashboard.apiServices.codexSpeedAction', {
                              speed: speedLabel,
                              defaultValue: '一键设置为{{speed}}',
                            })}
                            aria-label={t('dashboard.apiServices.codexSpeedAction', {
                              speed: speedLabel,
                              defaultValue: '一键设置为{{speed}}',
                            })}
                          >
                            {isSaving ? (
                              <RotateCw size={13} className="loading-spinner" />
                            ) : speed === 'fast' ? (
                              <Zap size={13} />
                            ) : (
                              <Gauge size={13} />
                            )}
                            <span>{speedLabel}</span>
                          </button>
                        );
                      })}
                    </div>
                  </div>
                )}
                <div className="api-service-actions">
                  {isCodex ? (
                    <>
                      <button
                        className={activateButtonClass}
                        onClick={() => void handleActivateCodexApiService()}
                        disabled={apiServiceBusy !== null || apiServiceSaving}
                      >
                        <Play size={14} />
                        {apiServiceBusy === 'activate'
                          ? t('common.loading', '加载中...')
                          : t('dashboard.apiServices.activate', '启动服务')}
                      </button>
                      <button
                        className={toggleButtonClass}
                        onClick={() => void handleToggleCodexApiService()}
                        disabled={apiServiceBusy !== null || apiServiceSaving}
                      >
                        <Power size={14} />
                        {codexCollection?.enabled
                          ? t('dashboard.apiServices.disable', '停用')
                          : t('dashboard.apiServices.enable', '启用')}
                      </button>
                      <button
                        className="btn btn-secondary"
                        onClick={() => openCodexApiServiceModal('panel')}
                        disabled={apiServiceBusy !== null || apiServiceSaving}
                      >
                        <Settings2 size={14} />
                        {t('codex.localAccess.dashboardAction', '服务面板')}
                      </button>
                      <button
                        className="btn btn-secondary"
                        onClick={() => openCodexApiServiceModal('members')}
                        disabled={apiServiceBusy !== null || apiServiceSaving}
                      >
                        <FolderPlus size={14} />
                        {t('codex.localAccess.modal.manageMembers', '管理成员')}
                      </button>
                      <button
                        className="btn btn-secondary"
                        onClick={() => openCodexApiServiceModal('providers')}
                        disabled={apiServiceBusy !== null || apiServiceSaving}
                      >
                        <Server size={14} />
                        {t('codex.localAccess.modal.manageProviders', '管理供应商')}
                      </button>
                    </>
                  ) : (
                    <button className="btn btn-secondary" disabled>
                      <Settings2 size={14} />
                      {t('dashboard.apiServices.waiting', '待开放')}
                    </button>
                  )}
                </div>
              </article>
            );
          })}
        </div>
      </section>
    );
  };

  return (
    <main className="main-content dashboard-page fade-in">
      <div className="page-tabs-row" style={{ minHeight: '60px' }}>
        <div className="page-tabs-label dashboard-title-label">
          <span>{t('nav.dashboard', '仪表盘')}</span>
          <ManualHelpIconButton className="header-action-btn dashboard-manual-btn dashboard-title-manual-btn" />
        </div>
        {topCenterBanner}
        <div className="dashboard-top-actions">
          <button className="header-action-btn" onClick={onOpenPlatformLayout}>
            <span>{t('platformLayout.title', '平台布局')}</span>
          </button>
          <AnnouncementCenter onNavigate={onNavigate} variant="inline" trigger="button" />
        </div>
      </div>

      {/* Top Stats */}
      <div className="stats-row">
        <div className="stat-card">
          <div className="stat-icon-bg primary"><Users size={24} /></div>
          <div className="stat-info">
            <span className="stat-label">{t('dashboard.totalAccounts', '账号总数')}</span>
            <span className="stat-value">{stats.total}</span>
          </div>
        </div>

        {visibleEntryOrder.map((entryId) => {
          const platformId = resolveEntryDefaultPlatformId(entryId, platformGroups);
          if (!platformId) {
            return null;
          }
          const groupId = parseGroupEntryId(entryId);
          const group = groupId ? platformGroups.find((item) => item.id === groupId) : null;
          const groupChildLabels = group
            ? group.platformIds.map((childPlatformId) =>
              resolveGroupChildName(group, childPlatformId, getPlatformLabel(childPlatformId, t)),
            )
            : [];
          const groupExtraCount = Math.max(groupChildLabels.length - 1, 0);
          const groupTooltip = groupChildLabels.join(', ');
          const label = group
            ? group.name
            : getPlatformLabel(platformId, t);
          const iconClass =
            platformId === 'antigravity'
              ? 'success'
              : platformId === 'codex'
                ? 'info'
                : platformId === 'zed'
                  ? 'info'
                  : platformId === 'github-copilot'
                    ? 'github'
                    : platformId === 'kiro'
                      ? 'github'
                      : platformId === 'cursor'
                        ? 'info'
                        : platformId === 'gemini'
                          ? 'info'
                          : 'windsurf';
          return (
            <button
              className="stat-card stat-card-button"
              key={entryId}
              onClick={() => onNavigate(PLATFORM_PAGE_MAP[platformId])}
              title={
                groupExtraCount > 0
                  ? `${t('dashboard.switchTo', '切换到此账号')} · ${groupTooltip}`
                  : t('dashboard.switchTo', '切换到此账号')
              }
            >
              {groupExtraCount > 0 && (
                <span className="stat-group-more-badge stat-group-more-badge-corner" title={groupTooltip} aria-label={groupTooltip}>
                  +{groupExtraCount}
                </span>
              )}
              <div
                className={`stat-icon-bg ${iconClass}`}
              >
                {group?.iconKind === 'custom' && group.iconCustomDataUrl ? (
                  <img
                    src={group.iconCustomDataUrl}
                    alt={label}
                    className="dashboard-group-icon"
                    style={{ width: 24, height: 24 }}
                  />
                ) : (
                  renderPlatformIcon(group?.iconPlatformId ?? platformId, 24)
                )}
              </div>
              <div className="stat-info">
                <span className="stat-label">{label}</span>
                <span className="stat-value">{entryCounts.get(entryId) ?? 0}</span>
              </div>
            </button>
          );
        })}
      </div>

      {renderApiServiceConsole()}

      {/* Main Comparison Section */}
      <div className="cards-section">
        {cardRows.map((row, rowIndex) => (
          <div
            className={`cards-split-row${isSinglePlatformMode ? ' single-platform-row' : ''}`}
            key={`row-${rowIndex}`}
          >
            {row.map((platformId) => renderPlatformCard(platformId))}
            {!isSinglePlatformMode && row.length < 2 && <div className="main-card main-card-placeholder" />}
          </div>
        ))}
      </div>

      {tagModalState && (
        <TagEditModal
          isOpen={true}
          onClose={() => setTagModalState(null)}
          initialTags={tagModalState.tags}
          availableTags={dashboardAvailableTags}
          onSave={handleSaveTags}
        />
      )}

      <CodexLocalAccessModal
        isOpen={showApiServiceModal}
        mode={apiServiceModalMode}
        state={apiServiceState}
        addressKind={selectedApiServiceAddressKind}
        addressOptions={apiServiceAddressOptions}
        onAddressKindChange={handleApiServiceAddressKindChange}
        accounts={codexAccounts}
        modelProviders={apiServiceModelProviders}
        accountGroups={apiServiceAccountGroups}
        initialSelectedIds={apiServiceModalSelectedIds}
        maskAccountText={maskAccountText}
        onClose={() => setShowApiServiceModal(false)}
        onSaveAccounts={({ accountIds, restrictFreeAccounts, autoIncludeNewAccounts }) =>
          handleSaveApiServiceAccounts(accountIds, {
            restrictFreeAccounts,
            autoIncludeNewAccounts,
          })
        }
        onSaveProviders={({ providerIds, autoIncludeNewProviders }) =>
          handleSaveApiServiceProviders(providerIds, {
            autoIncludeNewProviders,
          })
        }
        onClearStats={handleClearApiServiceStats}
        onRefreshStats={reloadApiServiceState}
        onUpdatePort={handleUpdateApiServicePort}
        onUpdateRoutingStrategy={handleUpdateApiServiceRoutingStrategy}
        onUpdateCustomRouting={handleUpdateApiServiceCustomRouting}
        onUpdateAccessScope={handleUpdateApiServiceAccessScope}
        onUpdateUpstreamProxyMode={handleUpdateApiServiceUpstreamProxyMode}
        onUpdateSourceMode={handleUpdateApiServiceSourceMode}
        onUpdateWebSocketMode={handleUpdateApiServiceWebSocketMode}
        onUpdateBoundOAuthAccount={handleUpdateApiServiceBoundOAuthAccount}
        onRotateApiKey={handleRotateApiServiceKey}
        onKillPort={handleKillApiServicePort}
        onTest={handleTestApiService}
        onApplyAllAccountSpeed={applyCodexSpeedToAllAccounts}
        bulkSpeedSaving={apiServiceSpeedSaving}
        saving={apiServiceSaving}
        testing={apiServiceTesting}
        portCleanupBusy={apiServicePortCleanupBusy}
      />

    </main>
  );
}
