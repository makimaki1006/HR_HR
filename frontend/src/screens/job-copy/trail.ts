/** 案件管理の階層（ホーム → 顧客 → 案件 → 求人）。URL の ?company= ?deal= ?listing= が正本。 */
export interface Trail { company?: string; deal?: string; listing?: string }
export type Level = 'home' | 'company' | 'deal' | 'listing';
const idPattern = /^\d{1,20}$/;
const keys = ['company', 'deal', 'listing'] as const;

/** URL の検索部分から階層を読む。数字の ID 以外は無いものとして扱う。 */
export function readTrail(search: string): Trail {
  const params = new URLSearchParams(search);
  const trail: Trail = {};
  for (const key of keys) {
    const value = params.get(key);
    if (value && idPattern.test(value)) trail[key] = value;
  }
  return trail;
}
/** 階層だけを差し替えた検索部分を返す。階層以外の指定（?job= ?demo= ?view= など）はそのまま残す。 */
export function writeTrail(search: string, trail: Trail): string {
  const params = new URLSearchParams(search);
  for (const key of keys) {
    const value = trail[key];
    if (value) params.set(key, value); else params.delete(key);
  }
  const text = params.toString();
  return text ? `?${text}` : '';
}
/** 上の階層へ戻ったときに残す階層。 */
export function trailAt(trail: Trail, level: Level): Trail {
  if (level === 'home') return {};
  const kept: Trail = {};
  if (trail.company) kept.company = trail.company;
  if (level === 'company') return kept;
  if (trail.deal) kept.deal = trail.deal;
  if (level === 'deal') return kept;
  if (trail.listing) kept.listing = trail.listing;
  return kept;
}
export interface TrailNames { company?: string | undefined; deal?: string | undefined; listing?: string | undefined }
export interface Crumb { level: Level; label: string; current: boolean }
/** 「案件管理 › 顧客 › 案件 › 求人」。いま開いている階層だけがリンクにならない。 */
const named = (value: string | undefined) => value?.trim() ? value : undefined;
export function buildCrumbs(trail: Trail, names: TrailNames): Crumb[] {
  const items: Omit<Crumb, 'current'>[] = [{ level: 'home', label: '案件管理' }];
  if (trail.company) items.push({ level: 'company', label: named(names.company) ?? '顧客' });
  if (trail.deal) items.push({ level: 'deal', label: named(names.deal) ?? '案件' });
  if (trail.listing) items.push({ level: 'listing', label: named(names.listing) ?? '求人' });
  return items.map((item, index) => ({ ...item, current: index === items.length - 1 }));
}
