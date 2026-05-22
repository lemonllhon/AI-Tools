export type ProxyRuntimeSourceKind = 'bundled' | 'override';

export interface ProxyRuntimeStatusItem {
  runtime: string;
  expectedVersion: string;
  manifestSha256: string;
  sourceKind: ProxyRuntimeSourceKind | null;
  sourcePath: string;
  cachePath: string;
  available: boolean;
  executable: boolean;
  cacheRefreshed: boolean;
  detectedVersion: string;
  versionOutput: string;
  error: string;
}

export interface ProxyRuntimeStatus {
  target: string;
  resourceDir: string;
  cacheRoot: string;
  runtimes: ProxyRuntimeStatusItem[];
}
