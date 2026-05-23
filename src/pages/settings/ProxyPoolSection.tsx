import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { FormEvent, WheelEvent, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  Activity,
  AlertCircle,
  Check,
  ChevronDown,
  ChevronUp,
  Eye,
  FileText,
  ListFilter,
  Link,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  ShieldCheck,
  Trash2,
  X,
} from 'lucide-react';
import {
  applyProxyPoolImport,
  applyProxyPoolSubscription,
  checkAllProxyPoolIpHealth,
  checkProxyPoolImportPreview,
  checkProxyPoolNodeIpHealth,
  checkProxyPoolSubscriptionPreview,
  deleteProxyPoolNode,
  deleteProxyPoolNodes,
  deleteProxyPoolSubscriptionSource,
  listProxyPoolNodes,
  previewProxyPoolImport,
  previewProxyPoolSubscription,
  PROXY_POOL_CHECK_PROGRESS_EVENT,
  PROXY_POOL_GATEWAY_FAILOVER_EVENT,
  refreshAllProxyPoolSubscriptions,
  refreshProxyPoolSubscription,
  saveProxyPoolNode,
  testAllProxyPoolLatency,
  testProxyPoolNodeLatency,
  updateProxyPoolServiceState,
  updateProxyPoolSubscriptionSource,
} from '../../services/proxyPoolService';
import type {
  ManualProxyNodeProtocol,
  ProxyGatewayFailoverEvent,
  ProxyPoolCheckProgressEvent,
  ProxyImportPreviewCheckKind,
  ProxyImportPreviewCheckResponse,
  ProxyImportPreviewResponse,
  ProxyPoolIpHealthResult,
  ProxyPoolListResponse,
  ProxyPoolNode,
  ProxyPoolOutletMode,
  ProxyPoolServiceState,
  ProxySource,
} from '../../types/proxyPool';
import { getCurrentLanguage } from '../../i18n';

interface ProxyPoolSectionProps {
  onServiceStateChange?: (state: ProxyPoolServiceState) => void;
}

interface ProxyNodeFormState {
  name: string;
  protocol: ManualProxyNodeProtocol;
  host: string;
  port: string;
  username: string;
  password: string;
  group: string;
  enabled: boolean;
}

interface ProxySourceFormState {
  url: string;
  group: string;
  namePrefix: string;
  dns: string;
}

interface ProxyPoolProgressState {
  completed: number;
  total: number;
}

const DEFAULT_FORM_STATE: ProxyNodeFormState = {
  name: '',
  protocol: 'http',
  host: '',
  port: '7890',
  username: '',
  password: '',
  group: '',
  enabled: false,
};

const NODE_LIST_VISIBLE_COUNT = 10;

const MANUAL_PROTOCOLS: ManualProxyNodeProtocol[] = ['http', 'https', 'socks5'];
type ImportMode = 'paste' | 'url';

function createProxyPoolCheckTaskId(kind: string) {
  return `${kind}-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

function formatProxyPoolProgress(progress: ProxyPoolProgressState | null) {
  if (!progress || progress.total <= 0) return '';
  return ` ${progress.completed}/${progress.total}`;
}

export function ProxyPoolSection({ onServiceStateChange }: ProxyPoolSectionProps) {
  const [data, setData] = useState<ProxyPoolListResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [form, setForm] = useState<ProxyNodeFormState>(DEFAULT_FORM_STATE);
  const [importMode, setImportMode] = useState<ImportMode>('paste');
  const [importContent, setImportContent] = useState('');
  const [subscriptionUrl, setSubscriptionUrl] = useState('');
  const [importGroup, setImportGroup] = useState('');
  const [importNamePrefix, setImportNamePrefix] = useState('');
  const [importPreview, setImportPreview] = useState<ProxyImportPreviewResponse | null>(null);
  const [selectedPreviewIds, setSelectedPreviewIds] = useState<Set<string>>(() => new Set());
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewCheckingLatency, setPreviewCheckingLatency] = useState(false);
  const [previewCheckingIpHealth, setPreviewCheckingIpHealth] = useState(false);
  const [checkingPreviewLatencyIds, setCheckingPreviewLatencyIds] = useState<Set<string>>(() => new Set());
  const [checkingPreviewIpHealthIds, setCheckingPreviewIpHealthIds] = useState<Set<string>>(() => new Set());
  const [importing, setImporting] = useState(false);
  const [refreshingAllSources, setRefreshingAllSources] = useState(false);
  const [refreshingSourceIds, setRefreshingSourceIds] = useState<Set<string>>(() => new Set());
  const [testingAllLatency, setTestingAllLatency] = useState(false);
  const [testingLatencyIds, setTestingLatencyIds] = useState<Set<string>>(() => new Set());
  const [latencyProgress, setLatencyProgress] = useState<ProxyPoolProgressState | null>(null);
  const [checkingAllIpHealth, setCheckingAllIpHealth] = useState(false);
  const [checkingIpHealthIds, setCheckingIpHealthIds] = useState<Set<string>>(() => new Set());
  const [ipHealthProgress, setIpHealthProgress] = useState<ProxyPoolProgressState | null>(null);
  const [editingSourceId, setEditingSourceId] = useState<string | null>(null);
  const [sourceForm, setSourceForm] = useState<ProxySourceFormState>({ url: '', group: '', namePrefix: '', dns: '' });
  const [sourceSavingId, setSourceSavingId] = useState<string | null>(null);
  const [sourceDeletingIds, setSourceDeletingIds] = useState<Set<string>>(() => new Set());
  const [serviceSaving, setServiceSaving] = useState(false);
  const [search, setSearch] = useState('');
  const [groupFilter, setGroupFilter] = useState('all');
  const [protocolFilter, setProtocolFilter] = useState('all');
  const [nodeSourceFilterIds, setNodeSourceFilterIds] = useState<Set<string>>(() => new Set());
  const [showSelectedNodesOnly, setShowSelectedNodesOnly] = useState(false);
  const [nodesCollapsed, setNodesCollapsed] = useState(false);
  const [ipHealthDetailNodeId, setIpHealthDetailNodeId] = useState<string | null>(null);
  const [nodeListMaxHeight, setNodeListMaxHeight] = useState<string | undefined>(undefined);
  const nodeListRef = useRef<HTMLDivElement | null>(null);
  const filteredNodeSelectAllRef = useRef<HTMLInputElement | null>(null);
  const activeLatencyTaskIdRef = useRef<string | null>(null);
  const activeIpHealthTaskIdRef = useRef<string | null>(null);
  const currentLanguage = getCurrentLanguage();

  const text = useMemo(() => {
    const isChinese = currentLanguage.toLowerCase().startsWith('zh');
    return isChinese
      ? {
          title: '代理节点池',
          desc: '当前阶段支持手动添加 http、https、socks5 节点，并可通过粘贴内容或 URL 订阅导入代理资源。',
          gatewayTitle: '内置代理网关',
          gatewayDesc: '全局代理启用后，受管进程和 Codex API 的跟随全局代理都会连接这个本地网关；直连、本地代理、节点池三种出口互斥，节点池内可多选备用节点。',
          gatewayEnabled: '网关已启用',
          gatewayDisabled: '网关未启用',
          outletMode: '出口模式',
          outletModeDirect: '直连',
          outletModeLocal: '本地代理',
          outletModeNodePool: '节点池',
          currentOutlet: '当前出口',
          currentOutletBadge: '当前出口',
          poolSelectedBadge: '已选出口',
          poolBackupBadge: '备用出口',
          setCurrentOutlet: '设为当前出口',
          selectPoolNode: '加入节点池出口',
          unselectPoolNode: '移出节点池出口',
          deletePick: '选择用于批量删除',
          addToPool: '保存后加入节点池',
          serviceUpdated: '内置代理网关设置已更新',
          outletUpdated: '出口模式已更新',
          serviceUpdateFailed: '更新内置代理网关失败',
          nodePoolEmpty: '请先添加或选择至少一个普通代理节点',
          invalidPort: '端口必须在 1-65535 之间',
          add: '添加节点',
          addResource: '添加资源',
          close: '收起',
          refresh: '刷新',
          nodeListTitle: '节点列表',
          nodeListCount: '显示 {{visible}} / {{total}} 个节点',
          nodeListScope: '显示范围',
          allNodes: '显示全部',
          sourceFilter: '订阅来源',
          sourceFilterAll: '全部来源',
          selectedSourcesCount: '{{count}} 个来源',
          selectedNodes: '已选择',
          selectedNodesCount: '已选择 {{count}}',
          selectFilteredNodes: '全选当前筛选节点',
          unselectFilteredNodes: '取消选择当前筛选节点',
          subscriptionNodes: '订阅',
          collapseNodes: '折叠节点列表',
          expandNodes: '展开节点列表',
          search: '搜索名称、地址、分组',
          allGroups: '全部分组',
          allProtocols: '全部协议',
          deleteSelected: '删除所选',
          empty: '暂无匹配节点',
          loading: '加载中...',
          dbPath: '数据库',
          builtin: '内置',
          enabled: '启用',
          disabled: '停用',
          group: '分组',
          protocol: '协议',
          host: '地址',
          port: '端口',
          username: '账号',
          password: '密码',
          name: '名称',
          save: '保存节点',
          saving: '保存中...',
          optional: '可选',
          defaultGroup: '默认',
          confirmDelete: '确认删除这个代理节点？',
          confirmBatchDelete: '确认删除所选代理节点？',
          deleteFailed: '删除代理节点失败',
          saveFailed: '保存代理节点失败',
          loadFailed: '加载代理节点池失败',
          statusFailed: '更新代理节点状态失败',
          builtinLocked: '内置节点不能删除',
          directLocked: '直连节点不能禁用',
          passwordStored: '已保存密码',
          latency: '延迟',
          latencyPending: '未测速',
          latencyFailed: '测速失败',
          testLatency: '测试延迟',
          testAllLatency: '测试全部',
          testingLatency: '测速中...',
          latencyTestFailed: '代理测速失败',
          latencyTestDone: '测速完成：{{count}} 个，失败 {{failed}} 个',
          ipHealth: 'IP健康',
          ipHealthPending: '未检测',
          checkIpHealth: '检查 IP 健康',
          checkAllIpHealth: '检查IP',
          checkingIpHealth: '检测中...',
          ipHealthFailed: 'IP 健康检测失败',
          ipHealthDone: 'IP 健康检测完成：{{count}} 个，失败 {{failed}} 个',
          viewIpHealth: '查看 IP 健康详情',
          ipHealthDetails: 'IP 健康详情',
          ipHealthNoDetails: '暂无 IP 健康详情，请先执行检查IP。',
          ipHealthStatus: '状态',
          ipHealthOk: '正常',
          ipHealthError: '异常',
          ipAddress: 'IP 地址',
          ipLocation: '位置',
          ipFraudScore: '风险分',
          ipResidential: '住宅 IP',
          ipBroadcast: '广播 IP',
          ipAsOrganization: '网络组织',
          ipSource: '来源',
          ipUpdatedAt: '更新时间',
          rawData: '原始数据',
          yes: '是',
          no: '否',
          unknown: '未知',
          importTitle: '添加资源',
          importDesc: '粘贴 Clash YAML、Base64 订阅内容、分享链接，或输入 http/https 订阅 URL 拉取并导入。',
          importModePaste: '粘贴内容',
          importModeUrl: 'URL 订阅',
          importContent: '资源内容',
          importContentPlaceholder: '粘贴 vmess://、vless://、trojan://、ss://、http://、socks5://、Base64 文本或 Clash YAML',
          subscriptionUrl: '订阅 URL',
          subscriptionUrlPlaceholder: 'https://example.com/sub',
          namePrefix: '名称前缀',
          previewImport: '预览',
          previewing: '解析中...',
          applyImport: '导入所选',
          importing: '导入中...',
          importFailed: '导入代理资源失败',
          previewFailed: '解析代理资源失败',
          previewEmpty: '暂无可导入节点',
          parseWarnings: '解析提示',
          selectAll: '全选',
          importedCount: '已导入 {{count}} 个节点',
          previewCount: '{{count}} 个节点',
          previewTestLatency: '测速所选',
          previewCheckIpHealth: '检测所选 IP',
          previewCheckingLatency: '测速中...',
          previewCheckingIpHealth: 'IP 检测中...',
          previewCheckFailed: '预览节点检测失败',
          sourcesTitle: '订阅来源',
          sourcesCount: '{{count}} 个来源',
          sourceNodeCount: '{{count}} 个节点',
          sourceLastRefresh: '上次刷新',
          sourceNever: '从未',
          sourceUrl: '订阅 URL',
          sourceNamePrefix: '名称前缀',
          refreshSource: '刷新订阅',
          refreshAllSources: '刷新全部订阅',
          refreshing: '刷新中...',
          refreshedCount: '已刷新 {{count}} 个订阅',
          refreshFailed: '刷新订阅失败',
          editSource: '编辑订阅',
          deleteSource: '删除订阅',
          saveSource: '保存订阅',
          cancelSourceEdit: '取消编辑',
          confirmDeleteSource: '删除这个订阅来源及其全部节点？',
          sourceUpdateFailed: '更新订阅来源失败',
          sourceDeleteFailed: '删除订阅来源失败',
          sourceUpdated: '订阅来源已更新',
          sourceDeleted: '订阅来源已删除',
        }
      : {
          title: 'Proxy Node Pool',
          desc: 'This stage supports manual http, https, and socks5 nodes plus paste or URL subscription imports.',
          gatewayTitle: 'Built-in Proxy Gateway',
          gatewayDesc: 'When global proxy is enabled, managed processes and Codex API follow-global-proxy mode connect to this local gateway. Direct, local proxy, and node pool are mutually exclusive; the node pool can hold multiple fallback nodes.',
          gatewayEnabled: 'Gateway enabled',
          gatewayDisabled: 'Gateway disabled',
          outletMode: 'Outlet mode',
          outletModeDirect: 'Direct',
          outletModeLocal: 'Local proxy',
          outletModeNodePool: 'Node pool',
          currentOutlet: 'Current outlet',
          currentOutletBadge: 'Current outlet',
          poolSelectedBadge: 'Selected outlet',
          poolBackupBadge: 'Backup outlet',
          setCurrentOutlet: 'Set current outlet',
          selectPoolNode: 'Add to node pool outlet',
          unselectPoolNode: 'Remove from node pool outlet',
          deletePick: 'Select for batch delete',
          addToPool: 'Add to node pool after saving',
          serviceUpdated: 'Built-in proxy gateway updated',
          outletUpdated: 'Outlet mode updated',
          serviceUpdateFailed: 'Failed to update built-in proxy gateway',
          nodePoolEmpty: 'Add or select at least one normal proxy node first',
          invalidPort: 'Port must be between 1 and 65535',
          add: 'Add Node',
          addResource: 'Add Resource',
          close: 'Collapse',
          refresh: 'Refresh',
          nodeListTitle: 'Node List',
          nodeListCount: 'Showing {{visible}} / {{total}} nodes',
          nodeListScope: 'Display scope',
          allNodes: 'Show all',
          sourceFilter: 'Sources',
          sourceFilterAll: 'All sources',
          selectedSourcesCount: '{{count}} sources',
          selectedNodes: 'Selected',
          selectedNodesCount: 'Selected {{count}}',
          selectFilteredNodes: 'Select current filtered nodes',
          unselectFilteredNodes: 'Unselect current filtered nodes',
          subscriptionNodes: 'Subscription',
          collapseNodes: 'Collapse node list',
          expandNodes: 'Expand node list',
          search: 'Search name, address, group',
          allGroups: 'All groups',
          allProtocols: 'All protocols',
          deleteSelected: 'Delete selected',
          empty: 'No matching nodes',
          loading: 'Loading...',
          dbPath: 'Database',
          builtin: 'Built-in',
          enabled: 'Enabled',
          disabled: 'Disabled',
          group: 'Group',
          protocol: 'Protocol',
          host: 'Host',
          port: 'Port',
          username: 'Username',
          password: 'Password',
          name: 'Name',
          save: 'Save node',
          saving: 'Saving...',
          optional: 'Optional',
          defaultGroup: 'Default',
          confirmDelete: 'Delete this proxy node?',
          confirmBatchDelete: 'Delete selected proxy nodes?',
          deleteFailed: 'Failed to delete proxy node',
          saveFailed: 'Failed to save proxy node',
          loadFailed: 'Failed to load proxy node pool',
          statusFailed: 'Failed to update proxy node status',
          builtinLocked: 'Built-in nodes cannot be deleted',
          directLocked: 'Direct node cannot be disabled',
          passwordStored: 'Password saved',
          latency: 'Latency',
          latencyPending: 'Not tested',
          latencyFailed: 'Failed',
          testLatency: 'Test latency',
          testAllLatency: 'Test all',
          testingLatency: 'Testing...',
          latencyTestFailed: 'Failed to test proxy latency',
          latencyTestDone: 'Latency checked: {{count}}, failed {{failed}}',
          ipHealth: 'IP health',
          ipHealthPending: 'Not checked',
          checkIpHealth: 'Check IP health',
          checkAllIpHealth: 'Check IP',
          checkingIpHealth: 'Checking...',
          ipHealthFailed: 'Failed to check IP health',
          ipHealthDone: 'IP health checked: {{count}}, failed {{failed}}',
          viewIpHealth: 'View IP health details',
          ipHealthDetails: 'IP Health Details',
          ipHealthNoDetails: 'No IP health details yet. Run Check IP first.',
          ipHealthStatus: 'Status',
          ipHealthOk: 'Healthy',
          ipHealthError: 'Error',
          ipAddress: 'IP address',
          ipLocation: 'Location',
          ipFraudScore: 'Fraud score',
          ipResidential: 'Residential IP',
          ipBroadcast: 'Broadcast IP',
          ipAsOrganization: 'Network organization',
          ipSource: 'Source',
          ipUpdatedAt: 'Updated at',
          rawData: 'Raw data',
          yes: 'Yes',
          no: 'No',
          unknown: 'Unknown',
          importTitle: 'Add Resource',
          importDesc: 'Paste Clash YAML, Base64 subscription text, share links, or fetch an http/https subscription URL.',
          importModePaste: 'Paste',
          importModeUrl: 'URL subscription',
          importContent: 'Resource content',
          importContentPlaceholder: 'Paste vmess://, vless://, trojan://, ss://, http://, socks5://, Base64 text, or Clash YAML',
          subscriptionUrl: 'Subscription URL',
          subscriptionUrlPlaceholder: 'https://example.com/sub',
          namePrefix: 'Name prefix',
          previewImport: 'Preview',
          previewing: 'Parsing...',
          applyImport: 'Import selected',
          importing: 'Importing...',
          importFailed: 'Failed to import proxy resource',
          previewFailed: 'Failed to parse proxy resource',
          previewEmpty: 'No importable nodes',
          parseWarnings: 'Parse notes',
          selectAll: 'Select all',
          importedCount: 'Imported {{count}} nodes',
          previewCount: '{{count}} nodes',
          previewTestLatency: 'Test selected',
          previewCheckIpHealth: 'Check selected IP',
          previewCheckingLatency: 'Testing...',
          previewCheckingIpHealth: 'Checking IP...',
          previewCheckFailed: 'Failed to check preview nodes',
          sourcesTitle: 'Subscription Sources',
          sourcesCount: '{{count}} sources',
          sourceNodeCount: '{{count}} nodes',
          sourceLastRefresh: 'Last refresh',
          sourceNever: 'Never',
          sourceUrl: 'Subscription URL',
          sourceNamePrefix: 'Name prefix',
          refreshSource: 'Refresh subscription',
          refreshAllSources: 'Refresh all',
          refreshing: 'Refreshing...',
          refreshedCount: 'Refreshed {{count}} subscriptions',
          refreshFailed: 'Failed to refresh subscription',
          editSource: 'Edit subscription',
          deleteSource: 'Delete subscription',
          saveSource: 'Save subscription',
          cancelSourceEdit: 'Cancel edit',
          confirmDeleteSource: 'Delete this subscription source and all of its nodes?',
          sourceUpdateFailed: 'Failed to update subscription source',
          sourceDeleteFailed: 'Failed to delete subscription source',
          sourceUpdated: 'Subscription source updated',
          sourceDeleted: 'Subscription source deleted',
        };
  }, [currentLanguage]);

  const applyProxyPoolListResponse = (response: ProxyPoolListResponse) => {
    setData(response);
    onServiceStateChange?.(response.serviceState);
  };

  const applyProxyPoolSnapshot = (snapshot: Pick<ProxyPoolListResponse, 'nodes' | 'groups' | 'sources'>) => {
    setData((current) => current ? {
      ...current,
      nodes: snapshot.nodes,
      groups: snapshot.groups,
      sources: snapshot.sources,
    } : current);
  };

  const applyProxyPoolProgressEvent = (payload: ProxyPoolCheckProgressEvent) => {
    const nextProgress = {
      completed: payload.completed,
      total: payload.total,
    };

    if (payload.kind === 'latency') {
      setLatencyProgress(nextProgress);
    } else if (payload.kind === 'ip_health') {
      setIpHealthProgress(nextProgress);
    }

    if (payload.phase !== 'node_done' || !payload.nodeId) {
      return;
    }

    if (payload.kind === 'latency') {
      setTestingLatencyIds((current) => {
        const next = new Set(current);
        next.delete(payload.nodeId);
        return next;
      });
      setData((current) => current ? {
        ...current,
        nodes: current.nodes.map((node) => node.id === payload.nodeId ? {
          ...node,
          latencyMs: payload.latencyMs,
          latencyStatus: payload.latencyStatus || (payload.ok ? 'ok' : payload.error),
        } : node),
      } : current);
      return;
    }

    if (payload.kind === 'ip_health') {
      setCheckingIpHealthIds((current) => {
        const next = new Set(current);
        next.delete(payload.nodeId);
        return next;
      });
      setData((current) => current ? {
        ...current,
        nodes: current.nodes.map((node) => node.id === payload.nodeId ? {
          ...node,
          ipHealth: payload.ipHealth ?? node.ipHealth,
          ipHealthSummary: payload.ipHealthSummary || payload.error || node.ipHealthSummary,
        } : node),
      } : current);
    }
  };

  const loadNodes = async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await listProxyPoolNodes();
      applyProxyPoolListResponse(response);
    } catch (err) {
      setError(`${text.loadFailed}: ${String(err)}`);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadNodes();
  }, []);

  const nodes = data?.nodes ?? [];
  const groups = data?.groups ?? [];
  const sources = data?.sources ?? [];
  const serviceState = data?.serviceState ?? null;
  const outletMode = serviceState?.outletMode ?? 'direct';
  const selectedPoolIds = serviceState?.selectedNodeIds ?? [];
  const selectedPoolIdSet = useMemo(() => new Set(selectedPoolIds), [selectedPoolIds]);
  const normalNodes = useMemo(() => nodes.filter((node) => !node.builtin), [nodes]);
  const selectedNodeCount = useMemo(
    () => nodes.filter((node) => selectedPoolIdSet.has(node.id)).length,
    [nodes, selectedPoolIdSet],
  );

  useEffect(() => {
    if (nodeSourceFilterIds.size === 0) return;

    const validSourceIds = new Set(sources.map((source) => source.id));
    setNodeSourceFilterIds((current) => {
      const next = new Set(Array.from(current).filter((sourceId) => validSourceIds.has(sourceId)));
      if (next.size === current.size) return current;
      if (next.size === 0 || next.size >= validSourceIds.size) return new Set();
      return next;
    });
  }, [nodeSourceFilterIds.size, sources]);

  useEffect(() => {
    if (showSelectedNodesOnly && selectedNodeCount === 0) {
      setShowSelectedNodesOnly(false);
    }
  }, [selectedNodeCount, showSelectedNodesOnly]);

  const protocolOptions = useMemo(() => {
    const order = ['direct', 'http', 'https', 'socks5', 'vmess', 'vless', 'trojan', 'ss', 'hysteria', 'hysteria2', 'tuic', 'anytls'];
    return Array.from(new Set(nodes.map((node) => node.protocol))).sort((left, right) => {
      const leftIndex = order.indexOf(left);
      const rightIndex = order.indexOf(right);
      if (leftIndex === -1 && rightIndex === -1) return left.localeCompare(right);
      if (leftIndex === -1) return 1;
      if (rightIndex === -1) return -1;
      return leftIndex - rightIndex;
    });
  }, [nodes]);

  const sourceFilterAll = nodeSourceFilterIds.size === 0;
  const activeSourceFilterIds = useMemo(() => {
    if (sourceFilterAll) return new Set<string>();
    const validSourceIds = new Set(sources.map((source) => source.id));
    return new Set(Array.from(nodeSourceFilterIds).filter((sourceId) => validSourceIds.has(sourceId)));
  }, [nodeSourceFilterIds, sourceFilterAll, sources]);
  const sourceFilterLabel = sourceFilterAll
    ? text.sourceFilterAll
    : text.selectedSourcesCount.replace('{{count}}', String(activeSourceFilterIds.size));

  const scopedNodes = useMemo(() => {
    if (showSelectedNodesOnly) {
      return nodes.filter((node) => selectedPoolIdSet.has(node.id));
    }
    if (!sourceFilterAll) {
      return nodes.filter((node) => Boolean(node.sourceId) && activeSourceFilterIds.has(node.sourceId ?? ''));
    }
    return nodes;
  }, [activeSourceFilterIds, nodes, selectedPoolIdSet, showSelectedNodesOnly, sourceFilterAll]);

  const filteredNodes = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return scopedNodes.filter((node) => {
      if (groupFilter !== 'all' && node.group !== groupFilter) return false;
      if (protocolFilter !== 'all' && node.protocol !== protocolFilter) return false;
      if (!needle) return true;
      return [node.name, node.protocol, node.group, node.maskedUrl, node.host]
        .join(' ')
        .toLowerCase()
        .includes(needle);
    });
  }, [groupFilter, protocolFilter, scopedNodes, search]);
  const filteredSelectableNodeIds = useMemo(
    () => filteredNodes.filter((node) => !node.builtin).map((node) => node.id),
    [filteredNodes],
  );
  const filteredSelectedNodeCount = useMemo(
    () => filteredSelectableNodeIds.filter((id) => selectedPoolIdSet.has(id)).length,
    [filteredSelectableNodeIds, selectedPoolIdSet],
  );
  const allFilteredNodesSelected = filteredSelectableNodeIds.length > 0
    && filteredSelectedNodeCount === filteredSelectableNodeIds.length;

  useEffect(() => {
    if (filteredNodeSelectAllRef.current) {
      filteredNodeSelectAllRef.current.indeterminate = filteredSelectedNodeCount > 0
        && filteredSelectedNodeCount < filteredSelectableNodeIds.length;
    }
  }, [filteredSelectableNodeIds.length, filteredSelectedNodeCount]);

  useEffect(() => {
    if (nodesCollapsed || filteredNodes.length <= NODE_LIST_VISIBLE_COUNT) {
      setNodeListMaxHeight(undefined);
      return;
    }

    const listElement = nodeListRef.current;
    if (!listElement) return;

    let animationFrame = 0;
    const measure = () => {
      cancelAnimationFrame(animationFrame);
      animationFrame = window.requestAnimationFrame(() => {
        const nodeRows = Array.from(
          listElement.querySelectorAll<HTMLElement>('.proxy-pool-node'),
        ).slice(0, NODE_LIST_VISIBLE_COUNT);
        if (nodeRows.length < NODE_LIST_VISIBLE_COUNT) {
          setNodeListMaxHeight(undefined);
          return;
        }
        const visibleHeight = nodeRows.reduce((total, row) => total + row.offsetHeight, 0);
        const borderHeight = listElement.offsetHeight - listElement.clientHeight;
        setNodeListMaxHeight(`${visibleHeight + borderHeight}px`);
      });
    };

    measure();
    const resizeObserver = new ResizeObserver(measure);
    resizeObserver.observe(listElement);
    Array.from(listElement.querySelectorAll<HTMLElement>('.proxy-pool-node')).forEach((row) => {
      resizeObserver.observe(row);
    });
    window.addEventListener('resize', measure);

    return () => {
      cancelAnimationFrame(animationFrame);
      resizeObserver.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, [filteredNodes, nodesCollapsed]);

  const selectedDeletableIds = useMemo(
    () => nodes.filter((node) => selectedPoolIdSet.has(node.id) && !node.builtin).map((node) => node.id),
    [nodes, selectedPoolIdSet],
  );
  const enabledNodeIds = useMemo(() => nodes.filter((node) => node.enabled).map((node) => node.id), [nodes]);

  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;

    listen<ProxyPoolCheckProgressEvent>(PROXY_POOL_CHECK_PROGRESS_EVENT, (event) => {
      const payload = event.payload;
      if (!payload?.taskId) return;

      if (payload.kind === 'latency') {
        if (payload.taskId !== activeLatencyTaskIdRef.current) return;
        applyProxyPoolProgressEvent(payload);
        return;
      }

      if (payload.kind === 'ip_health') {
        if (payload.taskId !== activeIpHealthTaskIdRef.current) return;
        applyProxyPoolProgressEvent(payload);
      }
    }).then((nextUnlisten) => {
      if (disposed) {
        nextUnlisten();
      } else {
        unlisten = nextUnlisten;
      }
    }).catch((err) => {
      console.error('监听代理检测进度失败:', err);
    });

    return () => {
      disposed = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;

    listen<ProxyGatewayFailoverEvent>(PROXY_POOL_GATEWAY_FAILOVER_EVENT, (event) => {
      const payload = event.payload;
      if (!payload?.serviceState) return;
      setData((current) => current ? {
        ...current,
        serviceState: payload.serviceState,
      } : current);
      onServiceStateChange?.(payload.serviceState);
    }).then((nextUnlisten) => {
      if (disposed) {
        nextUnlisten();
      } else {
        unlisten = nextUnlisten;
      }
    }).catch((err) => {
      console.error('监听代理网关故障切换失败:', err);
    });

    return () => {
      disposed = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  const resetNodeSearchFilters = () => {
    setSearch('');
    setGroupFilter('all');
    setProtocolFilter('all');
  };

  const resetNodeScopeFilters = () => {
    setNodeSourceFilterIds(new Set());
    setShowSelectedNodesOnly(false);
  };

  const handleSearchChange = (value: string) => {
    setSearch(value);
    resetNodeScopeFilters();
  };

  const handleGroupFilterChange = (value: string) => {
    setGroupFilter(value);
    resetNodeScopeFilters();
  };

  const handleProtocolFilterChange = (value: string) => {
    setProtocolFilter(value);
    resetNodeScopeFilters();
  };

  const handleSourceFilterChange = (sourceId: string, selected: boolean) => {
    const availableSourceIds = sources.map((source) => source.id);
    setNodeSourceFilterIds((current) => {
      const next = sourceFilterAll ? new Set(availableSourceIds) : new Set(current);
      if (selected) {
        next.add(sourceId);
      } else {
        next.delete(sourceId);
      }
      if (next.size === 0 || next.size >= availableSourceIds.length) return new Set();
      return next;
    });
    setShowSelectedNodesOnly(false);
    resetNodeSearchFilters();
  };

  const handleSelectAllSources = () => {
    setNodeSourceFilterIds(new Set());
    setShowSelectedNodesOnly(false);
    resetNodeSearchFilters();
  };

  const handleShowSelectedNodes = () => {
    setShowSelectedNodesOnly((current) => !current);
    setNodeSourceFilterIds(new Set());
    resetNodeSearchFilters();
  };

  const updateForm = <K extends keyof ProxyNodeFormState>(key: K, value: ProxyNodeFormState[K]) => {
    setForm((current) => ({ ...current, [key]: value }));
  };

  const resetForm = () => {
    setForm(DEFAULT_FORM_STATE);
  };

  const resetImportPreview = () => {
    setImportPreview(null);
    setSelectedPreviewIds(new Set());
    setPreviewCheckingLatency(false);
    setPreviewCheckingIpHealth(false);
    setCheckingPreviewLatencyIds(new Set());
    setCheckingPreviewIpHealthIds(new Set());
  };

  const updateImportMode = (mode: ImportMode) => {
    setImportMode(mode);
    resetImportPreview();
    setError(null);
    setNotice(null);
  };

  const formatSourceTime = (value: string | null) => {
    if (!value) return text.sourceNever;
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString(currentLanguage.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US');
  };

  const startEditSource = (source: ProxySource) => {
    setEditingSourceId(source.id);
    setSourceForm({
      url: source.url,
      group: source.group,
      namePrefix: source.namePrefix,
      dns: source.dns,
    });
    setError(null);
    setNotice(null);
  };

  const cancelEditSource = () => {
    setEditingSourceId(null);
    setSourceForm({ url: '', group: '', namePrefix: '', dns: '' });
  };

  const updateSourceForm = <K extends keyof ProxySourceFormState>(key: K, value: ProxySourceFormState[K]) => {
    setSourceForm((current) => ({ ...current, [key]: value }));
  };

  const handleSaveSource = async (sourceId: string) => {
    setSourceSavingId(sourceId);
    setError(null);
    setNotice(null);
    try {
      const response = await updateProxyPoolSubscriptionSource({
        sourceId,
        url: sourceForm.url,
        group: sourceForm.group || undefined,
        namePrefix: sourceForm.namePrefix || undefined,
        dns: sourceForm.dns || undefined,
      });
      applyProxyPoolListResponse(response);
      cancelEditSource();
      setNotice(text.sourceUpdated);
    } catch (err) {
      setError(`${text.sourceUpdateFailed}: ${String(err)}`);
    } finally {
      setSourceSavingId(null);
    }
  };

  const handleDeleteSource = async (source: ProxySource) => {
    if (!window.confirm(text.confirmDeleteSource)) return;
    setSourceDeletingIds((current) => new Set(current).add(source.id));
    setError(null);
    setNotice(null);
    try {
      const response = await deleteProxyPoolSubscriptionSource(source.id);
      applyProxyPoolListResponse(response);
      if (editingSourceId === source.id) {
        cancelEditSource();
      }
      setNotice(text.sourceDeleted);
    } catch (err) {
      setError(`${text.sourceDeleteFailed}: ${String(err)}`);
    } finally {
      setSourceDeletingIds((current) => {
        const next = new Set(current);
        next.delete(source.id);
        return next;
      });
    }
  };

  const handlePreviewImport = async () => {
    setPreviewLoading(true);
    setError(null);
    setNotice(null);
    setCheckingPreviewLatencyIds(new Set());
    setCheckingPreviewIpHealthIds(new Set());
    try {
      const preview =
        importMode === 'url'
          ? await previewProxyPoolSubscription({
              url: subscriptionUrl,
              group: importGroup || undefined,
              namePrefix: importNamePrefix || undefined,
            })
          : await previewProxyPoolImport({
              content: importContent,
              group: importGroup || undefined,
              namePrefix: importNamePrefix || undefined,
            });
      setImportPreview(preview);
      setSelectedPreviewIds(new Set());
    } catch (err) {
      setError(`${text.previewFailed}: ${String(err)}`);
    } finally {
      setPreviewLoading(false);
    }
  };

  const applyPreviewCheckResponse = (
    result: ProxyImportPreviewCheckResponse,
    checkKind: ProxyImportPreviewCheckKind,
  ) => {
    const resultMap = new Map(result.items.map((item) => [item.previewId, item]));
    setImportPreview((current) => current ? {
      ...current,
      items: current.items.map((item) => {
        const checked = resultMap.get(item.previewId);
        if (!checked) return item;
        if (checkKind === 'latency') {
          return {
            ...item,
            latencyMs: checked.latencyMs,
            latencyStatus: checked.latencyStatus || checked.error,
          };
        }
        return {
          ...item,
          ipHealth: checked.ipHealth ?? item.ipHealth,
          ipHealthSummary: checked.ipHealthSummary || checked.error || item.ipHealthSummary,
        };
      }),
    } : current);
  };

  const handleCheckPreviewNodes = async (checkKind: ProxyImportPreviewCheckKind) => {
    const selectedIds = Array.from(selectedPreviewIds);
    if (selectedIds.length === 0) return;

    const checkingIds = new Set(selectedIds);
    if (checkKind === 'latency') {
      setPreviewCheckingLatency(true);
      setCheckingPreviewLatencyIds(checkingIds);
    } else {
      setPreviewCheckingIpHealth(true);
      setCheckingPreviewIpHealthIds(checkingIds);
    }
    setError(null);
    setNotice(null);

    try {
      const result =
        importMode === 'url'
          ? await checkProxyPoolSubscriptionPreview({
              url: subscriptionUrl,
              group: importGroup || undefined,
              namePrefix: importNamePrefix || undefined,
              selectedPreviewIds: selectedIds,
              checkKind,
            })
          : await checkProxyPoolImportPreview({
              content: importContent,
              group: importGroup || undefined,
              namePrefix: importNamePrefix || undefined,
              selectedPreviewIds: selectedIds,
              checkKind,
            });
      applyPreviewCheckResponse(result, checkKind);
    } catch (err) {
      setError(`${text.previewCheckFailed}: ${String(err)}`);
    } finally {
      if (checkKind === 'latency') {
        setPreviewCheckingLatency(false);
        setCheckingPreviewLatencyIds(new Set());
      } else {
        setPreviewCheckingIpHealth(false);
        setCheckingPreviewIpHealthIds(new Set());
      }
    }
  };

  const handleApplyImport = async () => {
    if (selectedPreviewIds.size === 0) return;
    setImporting(true);
    setError(null);
    setNotice(null);
    try {
      const result =
        importMode === 'url'
          ? await applyProxyPoolSubscription({
              url: subscriptionUrl,
              group: importGroup || undefined,
              namePrefix: importNamePrefix || undefined,
              selectedPreviewIds: Array.from(selectedPreviewIds),
            })
          : await applyProxyPoolImport({
              content: importContent,
              group: importGroup || undefined,
              namePrefix: importNamePrefix || undefined,
              selectedPreviewIds: Array.from(selectedPreviewIds),
            });
      setData((current) => current ? { ...current, nodes: result.nodes } : current);
      setImportContent('');
      setSubscriptionUrl('');
      setImportGroup('');
      setImportNamePrefix('');
      resetImportPreview();
      setShowImport(false);
      await loadNodes();
      setNotice(text.importedCount.replace('{{count}}', String(result.imported)));
    } catch (err) {
      setError(`${text.importFailed}: ${String(err)}`);
    } finally {
      setImporting(false);
    }
  };

  const togglePreviewSelected = (previewId: string, selected: boolean) => {
    setSelectedPreviewIds((current) => {
      const next = new Set(current);
      if (selected) {
        next.add(previewId);
      } else {
        next.delete(previewId);
      }
      return next;
    });
  };

  const setAllPreviewSelected = (selected: boolean) => {
    setSelectedPreviewIds(selected ? new Set(importPreview?.items.map((item) => item.previewId) ?? []) : new Set());
  };

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const port = Number(form.port);
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const savedNode = await saveProxyPoolNode({
        name: form.name,
        protocol: form.protocol,
        host: form.host,
        port,
        username: form.username || undefined,
        password: form.password || undefined,
        group: form.group || undefined,
        enabled: false,
      });
      resetForm();
      setShowForm(false);
      if (form.enabled) {
        const nextSelectedIds = Array.from(new Set([...selectedPoolIds, savedNode.id]));
        const response = await updateProxyPoolServiceState({
          outletMode: 'node_pool',
          selectedNodeIds: nextSelectedIds,
          currentNodeId: savedNode.id,
        });
        applyProxyPoolListResponse(response);
        setNotice(text.outletUpdated);
      } else {
        await loadNodes();
      }
    } catch (err) {
      setError(`${text.saveFailed}: ${String(err)}`);
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (node: ProxyPoolNode) => {
    if (node.builtin) return;
    if (!window.confirm(text.confirmDelete)) return;
    setError(null);
    setNotice(null);
    try {
      await deleteProxyPoolNode(node.id);
      await loadNodes();
    } catch (err) {
      setError(`${text.deleteFailed}: ${String(err)}`);
    }
  };

  const handleDeleteSelected = async () => {
    if (selectedDeletableIds.length === 0) return;
    if (!window.confirm(text.confirmBatchDelete)) return;
    setError(null);
    setNotice(null);
    try {
      await deleteProxyPoolNodes(selectedDeletableIds);
      await loadNodes();
    } catch (err) {
      setError(`${text.deleteFailed}: ${String(err)}`);
    }
  };

  const updateServiceOutlet = async (
    request: {
      outletMode?: ProxyPoolOutletMode;
      selectedNodeIds?: string[];
      currentNodeId?: string;
    },
    message = text.outletUpdated,
  ) => {
    setServiceSaving(true);
    setError(null);
    setNotice(null);
    try {
      const response = await updateProxyPoolServiceState(request);
      applyProxyPoolListResponse(response);
      setNotice(message);
    } catch (err) {
      setError(`${text.serviceUpdateFailed}: ${String(err)}`);
    } finally {
      setServiceSaving(false);
    }
  };

  const handleSelectOutletMode = async (mode: ProxyPoolOutletMode) => {
    if (mode === 'node_pool') {
      const nextSelectedIds = selectedPoolIds.length > 0
        ? selectedPoolIds
        : normalNodes.slice(0, 1).map((node) => node.id);
      if (nextSelectedIds.length === 0) {
        setError(text.nodePoolEmpty);
        return;
      }
      const currentNodeId = nextSelectedIds.includes(serviceState?.currentNodeId ?? '')
        ? serviceState?.currentNodeId ?? nextSelectedIds[0]
        : nextSelectedIds[0];
      await updateServiceOutlet({
        outletMode: 'node_pool',
        selectedNodeIds: nextSelectedIds,
        currentNodeId,
      });
      return;
    }

    await updateServiceOutlet({
      outletMode: mode,
      selectedNodeIds: [],
      currentNodeId: mode === 'local' ? '__local__' : '__direct__',
    });
  };

  const handleTogglePoolNode = async (node: ProxyPoolNode, selected: boolean) => {
    if (node.builtin) return;
    const nextSelectedIds = selected
      ? Array.from(new Set([...selectedPoolIds, node.id]))
      : selectedPoolIds.filter((id) => id !== node.id);

    if (nextSelectedIds.length === 0) {
      await updateServiceOutlet({
        outletMode: 'direct',
        selectedNodeIds: [],
        currentNodeId: '__direct__',
      });
      return;
    }

    const currentNodeId = selected
      ? outletMode === 'node_pool' && nextSelectedIds.includes(serviceState?.currentNodeId ?? '')
        ? serviceState?.currentNodeId
        : node.id
      : serviceState?.currentNodeId === node.id
        ? nextSelectedIds[0]
        : serviceState?.currentNodeId ?? nextSelectedIds[0];

    await updateServiceOutlet({
      outletMode: 'node_pool',
      selectedNodeIds: nextSelectedIds,
      currentNodeId,
    });
  };

  const handleToggleFilteredPoolNodes = async (selected: boolean) => {
    if (filteredSelectableNodeIds.length === 0) return;
    const visibleSet = new Set(filteredSelectableNodeIds);
    const nextSelectedIds = selected
      ? Array.from(new Set([...selectedPoolIds, ...filteredSelectableNodeIds]))
      : selectedPoolIds.filter((id) => !visibleSet.has(id));

    if (nextSelectedIds.length === 0) {
      await updateServiceOutlet({
        outletMode: 'direct',
        selectedNodeIds: [],
        currentNodeId: '__direct__',
      });
      return;
    }

    const currentNodeId = nextSelectedIds.includes(serviceState?.currentNodeId ?? '')
      ? serviceState?.currentNodeId
      : selected
        ? filteredSelectableNodeIds[0]
        : nextSelectedIds[0];

    await updateServiceOutlet({
      outletMode: 'node_pool',
      selectedNodeIds: nextSelectedIds,
      currentNodeId,
    });
  };

  const handleSetCurrentPoolNode = async (node: ProxyPoolNode) => {
    if (node.builtin) return;
    const nextSelectedIds = selectedPoolIdSet.has(node.id)
      ? selectedPoolIds
      : Array.from(new Set([...selectedPoolIds, node.id]));
    await updateServiceOutlet({
      outletMode: 'node_pool',
      selectedNodeIds: nextSelectedIds,
      currentNodeId: node.id,
    });
  };

  const handleTestLatency = async (node: ProxyPoolNode) => {
    setTestingLatencyIds((current) => new Set(current).add(node.id));
    setError(null);
    setNotice(null);
    try {
      const result = await testProxyPoolNodeLatency(node.id);
      applyProxyPoolSnapshot(result);
      const failed = result.results.find((item) => !item.ok);
      if (failed) {
        setError(`${text.latencyTestFailed}: ${failed.error}`);
      } else {
        setNotice(text.latencyTestDone.replace('{{count}}', String(result.tested)).replace('{{failed}}', String(result.failed)));
      }
    } catch (err) {
      setError(`${text.latencyTestFailed}: ${String(err)}`);
    } finally {
      setTestingLatencyIds((current) => {
        const next = new Set(current);
        next.delete(node.id);
        return next;
      });
    }
  };

  const handleTestAllLatency = async () => {
    if (enabledNodeIds.length === 0) return;
    const taskId = createProxyPoolCheckTaskId('latency');
    activeLatencyTaskIdRef.current = taskId;
    setTestingAllLatency(true);
    setTestingLatencyIds(new Set(enabledNodeIds));
    setLatencyProgress({ completed: 0, total: enabledNodeIds.length });
    setError(null);
    setNotice(null);
    try {
      const result = await testAllProxyPoolLatency(taskId);
      applyProxyPoolSnapshot(result);
      if (result.failed > 0) {
        const firstError = result.results.find((item) => !item.ok)?.error || '';
        setError(`${text.latencyTestFailed}: ${result.failed}/${result.tested}${firstError ? ` - ${firstError}` : ''}`);
      } else {
        setNotice(text.latencyTestDone.replace('{{count}}', String(result.tested)).replace('{{failed}}', String(result.failed)));
      }
    } catch (err) {
      setError(`${text.latencyTestFailed}: ${String(err)}`);
    } finally {
      if (activeLatencyTaskIdRef.current === taskId) {
        activeLatencyTaskIdRef.current = null;
      }
      setTestingAllLatency(false);
      setTestingLatencyIds(new Set());
      setLatencyProgress(null);
    }
  };

  const handleCheckIpHealth = async (node: ProxyPoolNode) => {
    setCheckingIpHealthIds((current) => new Set(current).add(node.id));
    setError(null);
    setNotice(null);
    try {
      const result = await checkProxyPoolNodeIpHealth(node.id);
      applyProxyPoolSnapshot(result);
      const failed = result.results.find((item) => !item.ok);
      if (failed) {
        setError(`${text.ipHealthFailed}: ${failed.error}`);
      } else {
        setNotice(text.ipHealthDone.replace('{{count}}', String(result.checked)).replace('{{failed}}', String(result.failed)));
      }
    } catch (err) {
      setError(`${text.ipHealthFailed}: ${String(err)}`);
    } finally {
      setCheckingIpHealthIds((current) => {
        const next = new Set(current);
        next.delete(node.id);
        return next;
      });
    }
  };

  const handleCheckAllIpHealth = async () => {
    if (enabledNodeIds.length === 0) return;
    const taskId = createProxyPoolCheckTaskId('ip-health');
    activeIpHealthTaskIdRef.current = taskId;
    setCheckingAllIpHealth(true);
    setCheckingIpHealthIds(new Set(enabledNodeIds));
    setIpHealthProgress({ completed: 0, total: enabledNodeIds.length });
    setError(null);
    setNotice(null);
    try {
      const result = await checkAllProxyPoolIpHealth(taskId);
      applyProxyPoolSnapshot(result);
      if (result.failed > 0) {
        const firstError = result.results.find((item) => !item.ok)?.error || '';
        setError(`${text.ipHealthFailed}: ${result.failed}/${result.checked}${firstError ? ` - ${firstError}` : ''}`);
      } else {
        setNotice(text.ipHealthDone.replace('{{count}}', String(result.checked)).replace('{{failed}}', String(result.failed)));
      }
    } catch (err) {
      setError(`${text.ipHealthFailed}: ${String(err)}`);
    } finally {
      if (activeIpHealthTaskIdRef.current === taskId) {
        activeIpHealthTaskIdRef.current = null;
      }
      setCheckingAllIpHealth(false);
      setCheckingIpHealthIds(new Set());
      setIpHealthProgress(null);
    }
  };

  const applyRefreshResult = (result: Awaited<ReturnType<typeof refreshAllProxyPoolSubscriptions>>) => {
    applyProxyPoolSnapshot(result);
  };

  const handleRefreshSource = async (sourceId: string) => {
    setRefreshingSourceIds((current) => new Set(current).add(sourceId));
    setError(null);
    setNotice(null);
    try {
      const result = await refreshProxyPoolSubscription({ sourceId });
      applyRefreshResult(result);
      const failed = result.results.find((item) => !item.success);
      if (failed) {
        setError(`${text.refreshFailed}: ${failed.error || failed.displayName}`);
      } else {
        setNotice(text.refreshedCount.replace('{{count}}', String(result.refreshed)));
      }
    } catch (err) {
      setError(`${text.refreshFailed}: ${String(err)}`);
    } finally {
      setRefreshingSourceIds((current) => {
        const next = new Set(current);
        next.delete(sourceId);
        return next;
      });
    }
  };

  const handleRefreshAllSources = async () => {
    if (sources.length === 0) return;
    setRefreshingAllSources(true);
    setError(null);
    setNotice(null);
    try {
      const result = await refreshAllProxyPoolSubscriptions();
      applyRefreshResult(result);
      if (result.failed > 0) {
        const firstError = result.results.find((item) => !item.success)?.error || '';
        setError(`${text.refreshFailed}: ${result.failed}/${result.results.length}${firstError ? ` - ${firstError}` : ''}`);
      } else {
        setNotice(text.refreshedCount.replace('{{count}}', String(result.refreshed)));
      }
    } catch (err) {
      setError(`${text.refreshFailed}: ${String(err)}`);
    } finally {
      setRefreshingAllSources(false);
    }
  };

  const formatLatency = (node: ProxyPoolNode) => {
    if (testingLatencyIds.has(node.id)) return text.testingLatency;
    if (node.latencyStatus === 'ok' && node.latencyMs !== null) return `${node.latencyMs} ms`;
    if (node.latencyStatus) return text.latencyFailed;
    return text.latencyPending;
  };

  const formatPreviewLatency = (item: ProxyImportPreviewResponse['items'][number]) => {
    if (checkingPreviewLatencyIds.has(item.previewId)) return text.previewCheckingLatency;
    if (item.latencyStatus === 'ok' && item.latencyMs !== null && item.latencyMs !== undefined) {
      return `${item.latencyMs} ms`;
    }
    if (item.latencyStatus) return text.latencyFailed;
    return text.latencyPending;
  };

  const getPreviewLatencyTitle = (item: ProxyImportPreviewResponse['items'][number]) => {
    if (item.latencyStatus === 'ok' && item.latencyMs !== null && item.latencyMs !== undefined) {
      return `${text.latency}: ${item.latencyMs} ms`;
    }
    return item.latencyStatus || text.latencyPending;
  };

  const formatPreviewIpHealth = (item: ProxyImportPreviewResponse['items'][number]) => {
    if (checkingPreviewIpHealthIds.has(item.previewId)) return text.previewCheckingIpHealth;
    if (item.ipHealthSummary) return item.ipHealthSummary;
    return text.ipHealthPending;
  };

  const getLatencyTitle = (node: ProxyPoolNode) => {
    if (node.latencyStatus === 'ok' && node.latencyMs !== null) return `${text.latency}: ${node.latencyMs} ms`;
    if (node.latencyStatus) return node.latencyStatus;
    return text.latencyPending;
  };

  const handleNodeListWheel = (event: WheelEvent<HTMLDivElement>) => {
    const listElement = event.currentTarget;
    const deltaY = event.deltaY;
    if (deltaY === 0 || listElement.scrollHeight <= listElement.clientHeight + 1) return;

    const atTop = listElement.scrollTop <= 0;
    const atBottom = listElement.scrollTop + listElement.clientHeight >= listElement.scrollHeight - 1;
    if ((deltaY < 0 && !atTop) || (deltaY > 0 && !atBottom)) return;

    let scrollParent = listElement.parentElement;
    while (scrollParent) {
      const style = window.getComputedStyle(scrollParent);
      const canScroll = /(auto|scroll)/.test(style.overflowY)
        && scrollParent.scrollHeight > scrollParent.clientHeight;
      if (canScroll) break;
      scrollParent = scrollParent.parentElement;
    }

    event.preventDefault();
    if (scrollParent) {
      scrollParent.scrollBy({ top: deltaY, behavior: 'auto' });
    } else {
      window.scrollBy({ top: deltaY, behavior: 'auto' });
    }
  };

  const formatHealthValue = (value?: string | number | null) => {
    if (value === null || value === undefined) return '-';
    const textValue = String(value).trim();
    return textValue || '-';
  };

  const formatHealthBoolean = (value: boolean | null) => {
    if (value === null) return text.unknown;
    return value ? text.yes : text.no;
  };

  const formatHealthLocation = (health: ProxyPoolIpHealthResult) => {
    const location = [health.country, health.region, health.city]
      .map((value) => value.trim())
      .filter(Boolean)
      .join(' / ');
    return location || '-';
  };

  const ipHealthDetailNode = nodes.find((node) => node.id === ipHealthDetailNodeId) ?? null;
  const ipHealthDetail = ipHealthDetailNode?.ipHealth ?? null;
  const latencyProgressText = formatProxyPoolProgress(latencyProgress);
  const ipHealthProgressText = formatProxyPoolProgress(ipHealthProgress);
  const previewCheckBusy = previewCheckingLatency || previewCheckingIpHealth || previewLoading || importing;

  return (
    <>
      <div className="group-title">{text.title}</div>
      <div className="settings-group proxy-pool-panel">
        <div className="proxy-pool-header">
          <div className="proxy-pool-copy">
            <div className="row-title">{text.title}</div>
            <div className="row-desc">{text.desc}</div>
          </div>
          <div className="proxy-pool-actions">
            <button className="btn btn-secondary" type="button" onClick={loadNodes} disabled={loading}>
              <RefreshCw size={16} className={loading ? 'animate-spin' : undefined} />
              {loading ? text.loading : text.refresh}
            </button>
            <button
              className="btn btn-secondary"
              type="button"
              onClick={() => {
                setShowImport((visible) => !visible);
                setShowForm(false);
              }}
            >
              <FileText size={16} />
              {text.addResource}
            </button>
            <button
              className="btn btn-primary"
              type="button"
              onClick={() => {
                setShowForm((visible) => !visible);
                setShowImport(false);
              }}
            >
              {showForm ? <X size={16} /> : <Plus size={16} />}
              {showForm ? text.close : text.add}
            </button>
          </div>
        </div>

        {error && (
          <div className="proxy-pool-error">
            <AlertCircle size={16} />
            <span>{error}</span>
          </div>
        )}

        {notice && (
          <div className="proxy-pool-notice">
            <span>{notice}</span>
          </div>
        )}

        {serviceState && (
          <div className="proxy-pool-service">
            <div className="proxy-pool-service-head">
              <div className="proxy-pool-copy">
                <div className="row-title">{text.gatewayTitle}</div>
                <div className="row-desc">{text.gatewayDesc}</div>
              </div>
              <span className={`proxy-pool-service-badge ${serviceState.enabled ? 'is-enabled' : 'is-disabled'}`}>
                {serviceState.enabled ? text.gatewayEnabled : text.gatewayDisabled}
              </span>
            </div>

            <div className="proxy-pool-service-grid">
              <div className="proxy-pool-field proxy-pool-field--wide">
                <span>{text.outletMode}</span>
                <div className="proxy-pool-outlet-modes" role="group" aria-label={text.outletMode}>
                  {([
                    ['direct', text.outletModeDirect],
                    ['local', text.outletModeLocal],
                    ['node_pool', text.outletModeNodePool],
                  ] as const).map(([mode, label]) => (
                    <button
                      key={mode}
                      className={`proxy-pool-segment ${outletMode === mode ? 'is-active' : ''}`}
                      type="button"
                      onClick={() => void handleSelectOutletMode(mode)}
                      disabled={serviceSaving || (mode === 'node_pool' && normalNodes.length === 0)}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>
            </div>
            <div className="proxy-pool-service-note">
              {text.currentOutlet}: {serviceState.currentNodeName} · {serviceState.currentNodeProtocol}
            </div>
          </div>
        )}

        {showImport && (
          <div className="proxy-pool-import">
            <div className="proxy-pool-import-head">
              <div>
                <div className="row-title">{text.importTitle}</div>
                <div className="row-desc">{text.importDesc}</div>
              </div>
            </div>
            <div className="proxy-pool-import-mode" role="group" aria-label={text.importTitle}>
              <button
                className={`proxy-pool-segment ${importMode === 'paste' ? 'is-active' : ''}`}
                type="button"
                onClick={() => updateImportMode('paste')}
              >
                <FileText size={15} />
                {text.importModePaste}
              </button>
              <button
                className={`proxy-pool-segment ${importMode === 'url' ? 'is-active' : ''}`}
                type="button"
                onClick={() => updateImportMode('url')}
              >
                <Link size={15} />
                {text.importModeUrl}
              </button>
            </div>
            {importMode === 'paste' ? (
              <label className="proxy-pool-field proxy-pool-field--full">
                <span>{text.importContent}</span>
                <textarea
                  className="settings-input proxy-pool-import-textarea"
                  value={importContent}
                  onChange={(event) => {
                    setImportContent(event.target.value);
                    resetImportPreview();
                  }}
                  placeholder={text.importContentPlaceholder}
                />
              </label>
            ) : (
              <label className="proxy-pool-field proxy-pool-field--full">
                <span>{text.subscriptionUrl}</span>
                <input
                  className="settings-input"
                  value={subscriptionUrl}
                  onChange={(event) => {
                    setSubscriptionUrl(event.target.value);
                    resetImportPreview();
                  }}
                  placeholder={text.subscriptionUrlPlaceholder}
                />
              </label>
            )}
            <div className="proxy-pool-import-options">
              <label className="proxy-pool-field">
                <span>{text.group}</span>
                <input
                  className="settings-input"
                  value={importGroup}
                  onChange={(event) => {
                    setImportGroup(event.target.value);
                    resetImportPreview();
                  }}
                  placeholder={text.defaultGroup}
                />
              </label>
              <label className="proxy-pool-field">
                <span>{text.namePrefix}</span>
                <input
                  className="settings-input"
                  value={importNamePrefix}
                  onChange={(event) => {
                    setImportNamePrefix(event.target.value);
                    resetImportPreview();
                  }}
                  placeholder={text.optional}
                />
              </label>
              <div className="proxy-pool-import-actions">
                <button
                  className="btn btn-secondary"
                  type="button"
                  onClick={handlePreviewImport}
                  disabled={previewLoading || (importMode === 'url' ? !subscriptionUrl.trim() : !importContent.trim())}
                >
                  <RefreshCw size={16} className={previewLoading ? 'animate-spin' : undefined} />
                  {previewLoading ? text.previewing : text.previewImport}
                </button>
                <button
                  className="btn btn-primary"
                  type="button"
                  onClick={handleApplyImport}
                  disabled={importing || selectedPreviewIds.size === 0}
                >
                  {importing ? text.importing : text.applyImport}
                </button>
              </div>
            </div>

            {importPreview && (
              <div className="proxy-pool-preview">
                <div className="proxy-pool-preview-head">
                  <label className="proxy-pool-preview-select-all">
                    <input
                      type="checkbox"
                      checked={importPreview.items.length > 0 && selectedPreviewIds.size === importPreview.items.length}
                      disabled={importPreview.items.length === 0}
                      onChange={(event) => setAllPreviewSelected(event.target.checked)}
                    />
                    <span>{text.selectAll}</span>
                  </label>
                  <div className="proxy-pool-preview-head-actions">
                    <span>{text.previewCount.replace('{{count}}', String(importPreview.items.length))}</span>
                    <button
                      className="btn btn-secondary btn-compact"
                      type="button"
                      onClick={() => void handleCheckPreviewNodes('latency')}
                      disabled={previewCheckBusy || selectedPreviewIds.size === 0}
                    >
                      <Activity size={14} className={previewCheckingLatency ? 'animate-spin' : undefined} />
                      {previewCheckingLatency ? text.previewCheckingLatency : text.previewTestLatency}
                    </button>
                    <button
                      className="btn btn-secondary btn-compact"
                      type="button"
                      onClick={() => void handleCheckPreviewNodes('ip_health')}
                      disabled={previewCheckBusy || selectedPreviewIds.size === 0}
                    >
                      <ShieldCheck size={14} className={previewCheckingIpHealth ? 'animate-spin' : undefined} />
                      {previewCheckingIpHealth ? text.previewCheckingIpHealth : text.previewCheckIpHealth}
                    </button>
                  </div>
                </div>
                {importPreview.errors.length > 0 && (
                  <div className="proxy-pool-preview-errors">
                    <strong>{text.parseWarnings}</strong>
                    {importPreview.errors.map((item, index) => (
                      <span key={`${item}-${index}`}>{item}</span>
                    ))}
                  </div>
                )}
                <div className="proxy-pool-preview-list">
                  {importPreview.items.length === 0 ? (
                    <div className="proxy-pool-empty">{text.previewEmpty}</div>
                  ) : (
                    importPreview.items.map((item) => (
                      <label className="proxy-pool-preview-item" key={item.previewId}>
                        <input
                          type="checkbox"
                          checked={selectedPreviewIds.has(item.previewId)}
                          onChange={(event) => togglePreviewSelected(item.previewId, event.target.checked)}
                        />
                        <div className="proxy-pool-preview-main">
                          <div className="proxy-pool-node-title">
                            <span>{item.name}</span>
                            <span className={`proxy-pool-protocol is-${item.protocol}`}>{item.protocol}</span>
                            <span className="proxy-pool-badge">{item.sourceKind}</span>
                          </div>
                          <code title={item.maskedUrl}>{item.maskedUrl}</code>
                          <div className="proxy-pool-node-meta">
                            <span>{text.group}: {item.group || '-'}</span>
                            <span
                              className={`proxy-pool-health-chip ${
                                item.latencyStatus === 'ok' ? 'is-ok' : item.latencyStatus ? 'is-error' : 'is-muted'
                              }`}
                              title={getPreviewLatencyTitle(item)}
                            >
                              {text.latency}: {formatPreviewLatency(item)}
                            </span>
                            <span
                              className={`proxy-pool-health-chip ${
                                item.ipHealth?.ok ? 'is-ok' : item.ipHealthSummary ? 'is-error' : 'is-muted'
                              }`}
                              title={item.ipHealthSummary || text.ipHealthPending}
                            >
                              {text.ipHealth}: {formatPreviewIpHealth(item)}
                            </span>
                          </div>
                        </div>
                      </label>
                    ))
                  )}
                </div>
              </div>
            )}
          </div>
        )}

        {showForm && (
          <form className="proxy-pool-form" onSubmit={handleSubmit}>
            <label className="proxy-pool-field">
              <span>{text.name}</span>
              <input
                className="settings-input"
                value={form.name}
                onChange={(event) => updateForm('name', event.target.value)}
                placeholder="My proxy"
                required
              />
            </label>
            <label className="proxy-pool-field">
              <span>{text.protocol}</span>
              <select
                className="settings-select"
                value={form.protocol}
                onChange={(event) => updateForm('protocol', event.target.value as ManualProxyNodeProtocol)}
              >
                {MANUAL_PROTOCOLS.map((protocol) => (
                  <option key={protocol} value={protocol}>
                    {protocol}
                  </option>
                ))}
              </select>
            </label>
            <label className="proxy-pool-field proxy-pool-field--wide">
              <span>{text.host}</span>
              <input
                className="settings-input"
                value={form.host}
                onChange={(event) => updateForm('host', event.target.value)}
                placeholder="127.0.0.1"
                required
              />
            </label>
            <label className="proxy-pool-field">
              <span>{text.port}</span>
              <input
                className="settings-input"
                type="number"
                min={1}
                max={65535}
                value={form.port}
                onChange={(event) => updateForm('port', event.target.value)}
                required
              />
            </label>
            <label className="proxy-pool-field">
              <span>{text.username}</span>
              <input
                className="settings-input"
                value={form.username}
                onChange={(event) => updateForm('username', event.target.value)}
                placeholder={text.optional}
              />
            </label>
            <label className="proxy-pool-field">
              <span>{text.password}</span>
              <input
                className="settings-input"
                type="password"
                value={form.password}
                onChange={(event) => updateForm('password', event.target.value)}
                placeholder={text.optional}
              />
            </label>
            <label className="proxy-pool-field">
              <span>{text.group}</span>
              <input
                className="settings-input"
                value={form.group}
                onChange={(event) => updateForm('group', event.target.value)}
                placeholder={text.defaultGroup}
              />
            </label>
            <label className="proxy-pool-enabled">
              <input
                type="checkbox"
                checked={form.enabled}
                onChange={(event) => updateForm('enabled', event.target.checked)}
              />
              <span>{text.addToPool}</span>
            </label>
            <div className="proxy-pool-form-actions">
              <button
                className="btn btn-secondary"
                type="button"
                onClick={() => {
                  resetForm();
                  setShowForm(false);
                }}
                disabled={saving}
              >
                {text.close}
              </button>
              <button className="btn btn-primary" type="submit" disabled={saving}>
                {saving ? text.saving : text.save}
              </button>
            </div>
          </form>
        )}

        <div className="proxy-pool-toolbar">
          <div className="proxy-pool-search">
            <Search size={15} />
            <input
              value={search}
              onChange={(event) => handleSearchChange(event.target.value)}
              placeholder={text.search}
            />
          </div>
          <select className="settings-select" value={groupFilter} onChange={(event) => handleGroupFilterChange(event.target.value)}>
            <option value="all">{text.allGroups}</option>
            {groups.map((group) => (
              <option key={group} value={group}>
                {group}
              </option>
            ))}
          </select>
          <select
            className="settings-select"
            value={protocolFilter}
            onChange={(event) => handleProtocolFilterChange(event.target.value)}
          >
            <option value="all">{text.allProtocols}</option>
            {protocolOptions.map((protocol) => (
              <option key={protocol} value={protocol}>
                {protocol}
              </option>
            ))}
          </select>
          <button
            className="btn btn-secondary proxy-pool-test-all"
            type="button"
            onClick={() => void handleTestAllLatency()}
            disabled={testingAllLatency || enabledNodeIds.length === 0}
          >
            <Activity size={16} />
            {testingAllLatency ? `${text.testingLatency}${latencyProgressText}` : text.testAllLatency}
          </button>
          <button
            className="btn btn-secondary proxy-pool-check-all"
            type="button"
            onClick={() => void handleCheckAllIpHealth()}
            disabled={checkingAllIpHealth || enabledNodeIds.length === 0}
          >
            <ShieldCheck size={16} />
            {checkingAllIpHealth ? `${text.checkingIpHealth}${ipHealthProgressText}` : text.checkAllIpHealth}
          </button>
          <button
            className="btn btn-secondary proxy-pool-delete-selected"
            type="button"
            onClick={handleDeleteSelected}
            disabled={selectedDeletableIds.length === 0}
          >
            <Trash2 size={16} />
            {text.deleteSelected}
          </button>
        </div>

        {sources.length > 0 && (
          <div className="proxy-pool-sources">
            <div className="proxy-pool-sources-head">
              <div className="proxy-pool-sources-title">
                <span>{text.sourcesTitle}</span>
                <span>{text.sourcesCount.replace('{{count}}', String(sources.length))}</span>
              </div>
              <button
                className="btn btn-secondary proxy-pool-refresh-all"
                type="button"
                onClick={() => void handleRefreshAllSources()}
                disabled={refreshingAllSources}
              >
                <RefreshCw size={16} className={refreshingAllSources ? 'animate-spin' : undefined} />
                {refreshingAllSources ? text.refreshing : text.refreshAllSources}
              </button>
            </div>
            <div className="proxy-pool-source-list">
              {sources.map((source) => {
                const sourceBusy =
                  refreshingAllSources ||
                  refreshingSourceIds.has(source.id) ||
                  sourceSavingId === source.id ||
                  sourceDeletingIds.has(source.id);
                const editing = editingSourceId === source.id;

                return (
                  <div className="proxy-pool-source-item" key={source.id}>
                    <div className="proxy-pool-source-main">
                      <div className="proxy-pool-node-title">
                        <span>{source.displayName}</span>
                        <span className="proxy-pool-badge">
                          {text.sourceNodeCount.replace('{{count}}', String(source.nodeCount))}
                        </span>
                      </div>
                      <code title={source.url}>{source.url}</code>
                    </div>
                    <div className="proxy-pool-source-meta">
                      <span>{text.group}: {source.group || '-'}</span>
                      {source.namePrefix && <span>{text.sourceNamePrefix}: {source.namePrefix}</span>}
                      <span>{text.sourceLastRefresh}: {formatSourceTime(source.lastRefreshAt)}</span>
                      {source.lastError && <span>{source.lastError}</span>}
                    </div>
                    <div className="proxy-pool-source-actions">
                      <button
                        className="proxy-pool-icon-btn proxy-pool-refresh-btn"
                        type="button"
                        onClick={() => void handleRefreshSource(source.id)}
                        disabled={sourceBusy}
                        title={text.refreshSource}
                      >
                        <RefreshCw
                          size={16}
                          className={refreshingSourceIds.has(source.id) ? 'animate-spin' : undefined}
                        />
                      </button>
                      <button
                        className="proxy-pool-icon-btn"
                        type="button"
                        onClick={() => startEditSource(source)}
                        disabled={sourceBusy}
                        title={text.editSource}
                      >
                        <Pencil size={16} />
                      </button>
                      <button
                        className="proxy-pool-icon-btn"
                        type="button"
                        onClick={() => void handleDeleteSource(source)}
                        disabled={sourceBusy}
                        title={text.deleteSource}
                      >
                        <Trash2 size={16} />
                      </button>
                    </div>
                    {editing && (
                      <form
                        className="proxy-pool-source-edit"
                        onSubmit={(event) => {
                          event.preventDefault();
                          void handleSaveSource(source.id);
                        }}
                      >
                        <label className="proxy-pool-field proxy-pool-field--full">
                          <span>{text.sourceUrl}</span>
                          <input
                            className="settings-input"
                            value={sourceForm.url}
                            onChange={(event) => updateSourceForm('url', event.target.value)}
                          />
                        </label>
                        <label className="proxy-pool-field">
                          <span>{text.group}</span>
                          <input
                            className="settings-input"
                            value={sourceForm.group}
                            onChange={(event) => updateSourceForm('group', event.target.value)}
                            placeholder={text.defaultGroup}
                          />
                        </label>
                        <label className="proxy-pool-field">
                          <span>{text.sourceNamePrefix}</span>
                          <input
                            className="settings-input"
                            value={sourceForm.namePrefix}
                            onChange={(event) => updateSourceForm('namePrefix', event.target.value)}
                            placeholder={text.optional}
                          />
                        </label>
                        <div className="proxy-pool-source-edit-actions">
                          <button className="btn btn-primary" type="submit" disabled={sourceSavingId === source.id}>
                            <Check size={16} />
                            {sourceSavingId === source.id ? text.saving : text.saveSource}
                          </button>
                          <button
                            className="btn btn-secondary"
                            type="button"
                            onClick={cancelEditSource}
                            disabled={sourceSavingId === source.id}
                          >
                            <X size={16} />
                            {text.cancelSourceEdit}
                          </button>
                        </div>
                      </form>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}

        <div className="proxy-pool-list-head">
          <div className="proxy-pool-list-title">
            <label
              className="proxy-pool-filtered-select-all"
              title={allFilteredNodesSelected ? text.unselectFilteredNodes : text.selectFilteredNodes}
            >
              <input
                ref={filteredNodeSelectAllRef}
                type="checkbox"
                checked={allFilteredNodesSelected}
                disabled={serviceSaving || filteredSelectableNodeIds.length === 0}
                onChange={(event) => void handleToggleFilteredPoolNodes(event.target.checked)}
              />
            </label>
            <div className="proxy-pool-list-title-copy">
              <span>{text.nodeListTitle}</span>
              <span>
                {text.nodeListCount
                  .replace('{{visible}}', String(filteredNodes.length))
                  .replace('{{total}}', String(scopedNodes.length))}
              </span>
            </div>
            {sources.length > 0 && (
              <details className="proxy-pool-source-filter">
                <summary className="proxy-pool-source-filter-trigger" title={text.sourceFilter}>
                  <ListFilter size={16} />
                  <span>{sourceFilterLabel}</span>
                </summary>
                <div className="proxy-pool-source-filter-menu">
                  <label className="proxy-pool-source-filter-option">
                    <input
                      type="checkbox"
                      checked={sourceFilterAll}
                      onChange={handleSelectAllSources}
                      disabled={showSelectedNodesOnly}
                    />
                    <span>{text.sourceFilterAll}</span>
                  </label>
                  {sources.map((source) => (
                    <label className="proxy-pool-source-filter-option" key={source.id}>
                      <input
                        type="checkbox"
                        checked={sourceFilterAll || activeSourceFilterIds.has(source.id)}
                        onChange={(event) => handleSourceFilterChange(source.id, event.target.checked)}
                        disabled={showSelectedNodesOnly}
                      />
                      <span title={source.displayName}>
                        {source.displayName} ({source.nodeCount})
                      </span>
                    </label>
                  ))}
                </div>
              </details>
            )}
            <button
              className={`btn btn-secondary proxy-pool-selected-scope ${showSelectedNodesOnly ? 'is-active' : ''}`}
              type="button"
              onClick={handleShowSelectedNodes}
              disabled={selectedNodeCount === 0}
              title={text.selectedNodes}
            >
              <Check size={16} />
              {text.selectedNodesCount.replace('{{count}}', String(selectedNodeCount))}
            </button>
          </div>
          <button
            className="btn btn-secondary proxy-pool-list-toggle"
            type="button"
            onClick={() => setNodesCollapsed((collapsed) => !collapsed)}
            title={nodesCollapsed ? text.expandNodes : text.collapseNodes}
          >
            {nodesCollapsed ? <ChevronDown size={16} /> : <ChevronUp size={16} />}
            {nodesCollapsed ? text.expandNodes : text.collapseNodes}
          </button>
        </div>

        {!nodesCollapsed && (
          <div
            className="proxy-pool-list"
            ref={nodeListRef}
            style={nodeListMaxHeight ? { maxHeight: nodeListMaxHeight } : undefined}
            onWheel={handleNodeListWheel}
          >
            {loading && nodes.length === 0 ? (
              <div className="proxy-pool-empty">{text.loading}</div>
            ) : filteredNodes.length === 0 ? (
              <div className="proxy-pool-empty">{text.empty}</div>
            ) : (
              filteredNodes.map((node) => (
                <div className="proxy-pool-node" key={node.id}>
                  <label
                    className="proxy-pool-node-select"
                    title={
                      node.builtin
                        ? text.builtin
                        : selectedPoolIdSet.has(node.id)
                          ? text.unselectPoolNode
                          : text.selectPoolNode
                    }
                  >
                    <input
                      type="checkbox"
                      checked={selectedPoolIdSet.has(node.id)}
                      disabled={node.builtin || serviceSaving}
                      onChange={(event) => void handleTogglePoolNode(node, event.target.checked)}
                    />
                  </label>
                  <div className="proxy-pool-node-main">
                    <div className="proxy-pool-node-title">
                      <span>{node.name}</span>
                      <span className={`proxy-pool-protocol is-${node.protocol}`}>{node.protocol}</span>
                      {serviceState?.currentNodeId === node.id && (
                        <span className="proxy-pool-badge is-current">{text.currentOutletBadge}</span>
                      )}
                      {!node.builtin && selectedPoolIdSet.has(node.id) && serviceState?.currentNodeId !== node.id && (
                        <span className="proxy-pool-badge is-selected">{text.poolBackupBadge}</span>
                      )}
                      {node.builtin && <span className="proxy-pool-badge">{text.builtin}</span>}
                      {!node.builtin && node.sourceName && <span className="proxy-pool-badge">{node.sourceName}</span>}
                    </div>
                    <code title={node.maskedUrl}>{node.maskedUrl}</code>
                    <div className="proxy-pool-node-meta">
                      <span>{text.group}: {node.group || '-'}</span>
                      {node.hasPassword && <span>{text.passwordStored}</span>}
                      <span
                        className={`proxy-pool-health-chip ${
                          node.latencyStatus === 'ok' ? 'is-ok' : node.latencyStatus ? 'is-error' : 'is-muted'
                        }`}
                        title={getLatencyTitle(node)}
                      >
                        {text.latency}: {formatLatency(node)}
                      </span>
                      <button
                        className={`proxy-pool-health-eye ${node.ipHealthSummary ? 'is-info' : 'is-muted'}`}
                        type="button"
                        onClick={() => setIpHealthDetailNodeId(node.id)}
                        title={checkingIpHealthIds.has(node.id) ? text.checkingIpHealth : node.ipHealthSummary || text.ipHealthPending}
                        aria-label={text.viewIpHealth}
                      >
                        <Eye size={14} />
                      </button>
                    </div>
                  </div>
                  <div className="proxy-pool-node-state">
                    <span className={`proxy-pool-state-text ${node.enabled ? 'is-enabled' : 'is-disabled'}`}>
                      {node.enabled ? text.enabled : text.disabled}
                    </span>
                  </div>
                  <div className="proxy-pool-node-actions">
                    {!node.builtin && (
                      <button
                        className="proxy-pool-icon-btn"
                        type="button"
                        onClick={() => void handleSetCurrentPoolNode(node)}
                        disabled={serviceSaving || serviceState?.currentNodeId === node.id}
                        title={text.setCurrentOutlet}
                      >
                        <Check size={16} />
                      </button>
                    )}
                    <button
                      className="proxy-pool-icon-btn proxy-pool-latency-btn"
                      type="button"
                      onClick={() => void handleTestLatency(node)}
                      disabled={testingAllLatency || testingLatencyIds.has(node.id)}
                      title={text.testLatency}
                    >
                      <Activity size={16} />
                    </button>
                    <button
                      className="proxy-pool-icon-btn proxy-pool-health-btn"
                      type="button"
                      onClick={() => void handleCheckIpHealth(node)}
                      disabled={checkingAllIpHealth || checkingIpHealthIds.has(node.id)}
                      title={text.checkIpHealth}
                    >
                      <ShieldCheck size={16} />
                    </button>
                    <button
                      className="proxy-pool-icon-btn"
                      type="button"
                      onClick={() => void handleDelete(node)}
                      disabled={node.builtin}
                      title={node.builtin ? text.builtinLocked : text.deleteSelected}
                    >
                      <Trash2 size={16} />
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        )}

        {data?.dbPath && (
          <div className="proxy-pool-db-path" title={data.dbPath}>
            <span>{text.dbPath}</span>
            <code>{data.dbPath}</code>
          </div>
        )}
      </div>

      {ipHealthDetailNode && createPortal((
        <div
          className="proxy-pool-modal-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setIpHealthDetailNodeId(null);
            }
          }}
        >
          <div className="proxy-pool-modal" role="dialog" aria-modal="true" aria-label={text.ipHealthDetails}>
            <div className="proxy-pool-modal-head">
              <div>
                <div className="row-title">{text.ipHealthDetails}</div>
                <div className="row-desc">{ipHealthDetailNode.name}</div>
              </div>
              <button
                className="proxy-pool-icon-btn"
                type="button"
                onClick={() => setIpHealthDetailNodeId(null)}
                title={text.close}
              >
                <X size={16} />
              </button>
            </div>

            {ipHealthDetail ? (
              <>
                <div className="proxy-pool-health-detail-grid">
                  {[
                    [text.ipHealthStatus, ipHealthDetail.ok ? text.ipHealthOk : text.ipHealthError],
                    [text.ipAddress, formatHealthValue(ipHealthDetail.ip)],
                    [text.ipLocation, formatHealthLocation(ipHealthDetail)],
                    [text.ipFraudScore, formatHealthValue(ipHealthDetail.fraudScore)],
                    [text.ipResidential, formatHealthBoolean(ipHealthDetail.isResidential)],
                    [text.ipBroadcast, formatHealthBoolean(ipHealthDetail.isBroadcast)],
                    [text.ipAsOrganization, formatHealthValue(ipHealthDetail.asOrganization)],
                    [text.ipSource, formatHealthValue(ipHealthDetail.source)],
                    [text.ipUpdatedAt, formatHealthValue(ipHealthDetail.updatedAt)],
                  ].map(([label, value]) => (
                    <div className="proxy-pool-health-detail-item" key={label}>
                      <span>{label}</span>
                      <strong>{value}</strong>
                    </div>
                  ))}
                </div>
                {!ipHealthDetail.ok && ipHealthDetail.error && (
                  <div className="proxy-pool-health-detail-error">
                    <AlertCircle size={16} />
                    <span>{ipHealthDetail.error}</span>
                  </div>
                )}
                <div className="proxy-pool-health-raw">
                  <span>{text.rawData}</span>
                  <pre>{JSON.stringify(ipHealthDetail.rawData ?? {}, null, 2)}</pre>
                </div>
              </>
            ) : (
              <div className="proxy-pool-health-empty">
                <Eye size={18} />
                <span>{ipHealthDetailNode.ipHealthSummary || text.ipHealthNoDetails}</span>
              </div>
            )}
          </div>
        </div>
      ), document.body)}
    </>
  );
}
