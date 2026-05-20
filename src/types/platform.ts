import { Page } from './navigation';
import { HIDDEN_PLATFORM_IDS } from '../config/platformVisibility';

export type PlatformId =
  | 'antigravity'
  | 'codex'
  | 'zed'
  | 'github-copilot'
  | 'windsurf'
  | 'kiro'
  | 'cursor'
  | 'gemini'
  | 'codebuddy'
  | 'codebuddy_cn'
  | 'qoder'
  | 'trae'
  | 'workbuddy';

export const ALL_PLATFORM_IDS: PlatformId[] = [
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
];

export const MENU_HIDDEN_PLATFORM_IDS: PlatformId[] = [...HIDDEN_PLATFORM_IDS];

export const MENU_VISIBLE_PLATFORM_IDS: PlatformId[] = ALL_PLATFORM_IDS.filter(
  (platformId) => !MENU_HIDDEN_PLATFORM_IDS.includes(platformId),
);

export function isMenuVisiblePlatform(platformId: PlatformId): boolean {
  return !MENU_HIDDEN_PLATFORM_IDS.includes(platformId);
}

export const PAGE_PLATFORM_ID_MAP: Partial<Record<Page, PlatformId>> = {
  overview: 'antigravity',
  codex: 'codex',
  zed: 'zed',
  'github-copilot': 'github-copilot',
  windsurf: 'windsurf',
  kiro: 'kiro',
  cursor: 'cursor',
  gemini: 'gemini',
  codebuddy: 'codebuddy',
  'codebuddy-cn': 'codebuddy_cn',
  qoder: 'qoder',
  trae: 'trae',
  workbuddy: 'workbuddy',
};

export function isPageVisible(page: Page): boolean {
  const platformId = PAGE_PLATFORM_ID_MAP[page];
  return !platformId || isMenuVisiblePlatform(platformId);
}

export const PLATFORM_PAGE_MAP: Record<PlatformId, Page> = {
  antigravity: 'overview',
  codex: 'codex',
  zed: 'zed',
  'github-copilot': 'github-copilot',
  windsurf: 'windsurf',
  kiro: 'kiro',
  cursor: 'cursor',
  gemini: 'gemini',
  codebuddy: 'codebuddy',
  codebuddy_cn: 'codebuddy-cn',
  qoder: 'qoder',
  trae: 'trae',
  workbuddy: 'workbuddy',
};
