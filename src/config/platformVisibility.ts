import type { PlatformId } from '../types/platform';
import visibilityConfig from './platformVisibility.json';

const ALL_PLATFORM_ID_SET = new Set<PlatformId>([
  'antigravity',
  'codex',
  'zed',
  'github-copilot',
  'windsurf',
  'kiro',
  'cursor',
  'gemini',
  'codebuddy',
  'codebuddy_cn',
  'qoder',
  'trae',
  'workbuddy',
]);

// Global platform page switch.
// Edit platformVisibility.json and set enableAllPlatformPages to true
// to show every supported platform again.
export const ENABLE_ALL_PLATFORM_PAGES = Boolean(visibilityConfig.enableAllPlatformPages);

export const CODEX_ONLY_HIDDEN_PLATFORM_IDS: readonly PlatformId[] = (
  Array.isArray(visibilityConfig.hiddenPlatformIds)
    ? visibilityConfig.hiddenPlatformIds
    : []
).filter((platformId): platformId is PlatformId =>
  ALL_PLATFORM_ID_SET.has(platformId as PlatformId),
);

export const HIDDEN_PLATFORM_IDS: readonly PlatformId[] = ENABLE_ALL_PLATFORM_PAGES
  ? []
  : CODEX_ONLY_HIDDEN_PLATFORM_IDS;
