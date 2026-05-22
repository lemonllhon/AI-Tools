export type ProxyNodeProtocol =
  | 'direct'
  | 'http'
  | 'https'
  | 'socks5'
  | 'vmess'
  | 'vless'
  | 'trojan'
  | 'ss'
  | 'hysteria'
  | 'hysteria2'
  | 'tuic'
  | 'anytls';

export type ManualProxyNodeProtocol = 'http' | 'https' | 'socks5';

export interface ProxyPoolNode {
  id: string;
  name: string;
  protocol: ProxyNodeProtocol;
  host: string;
  port: number;
  username: string;
  hasPassword: boolean;
  group: string;
  sourceId: string | null;
  sourceName: string;
  sortOrder: number;
  enabled: boolean;
  builtin: boolean;
  latencyMs: number | null;
  latencyStatus: string;
  ipHealthSummary: string;
  maskedUrl: string;
  createdAt: string;
  updatedAt: string;
}

export interface ProxySource {
  id: string;
  url: string;
  displayName: string;
  namePrefix: string;
  group: string;
  dns: string;
  autoRefreshEnabled: boolean;
  refreshIntervalMinutes: number;
  lastRefreshAt: string | null;
  lastError: string;
  nodeCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface ProxyPoolListResponse {
  dbPath: string;
  nodes: ProxyPoolNode[];
  groups: string[];
  sources: ProxySource[];
}

export interface ProxyNodeSaveRequest {
  id?: string;
  name: string;
  protocol: ManualProxyNodeProtocol;
  host: string;
  port: number;
  username?: string;
  password?: string;
  group?: string;
  enabled?: boolean;
}

export interface ProxyImportPreviewRequest {
  content: string;
  group?: string;
  namePrefix?: string;
}

export interface ProxyImportPreviewItem {
  previewId: string;
  name: string;
  protocol: ProxyNodeProtocol;
  host: string;
  port: number;
  group: string;
  sourceKind: string;
  maskedUrl: string;
}

export interface ProxyImportPreviewResponse {
  items: ProxyImportPreviewItem[];
  errors: string[];
}

export interface ProxyImportApplyRequest extends ProxyImportPreviewRequest {
  selectedPreviewIds: string[];
}

export interface ProxyImportApplyResponse {
  imported: number;
  skipped: number;
  nodes: ProxyPoolNode[];
}

export interface ProxySubscriptionPreviewRequest {
  url: string;
  group?: string;
  namePrefix?: string;
}

export interface ProxySubscriptionApplyRequest extends ProxySubscriptionPreviewRequest {
  selectedPreviewIds: string[];
}

export interface ProxySubscriptionApplyResponse {
  imported: number;
  skipped: number;
  nodes: ProxyPoolNode[];
  source: ProxySource;
}

export interface ProxySubscriptionRefreshRequest {
  sourceId: string;
}

export interface ProxySubscriptionRefreshItem {
  sourceId: string;
  displayName: string;
  imported: number;
  success: boolean;
  error: string | null;
}

export interface ProxySubscriptionRefreshResponse {
  refreshed: number;
  failed: number;
  nodes: ProxyPoolNode[];
  groups: string[];
  sources: ProxySource[];
  results: ProxySubscriptionRefreshItem[];
}
