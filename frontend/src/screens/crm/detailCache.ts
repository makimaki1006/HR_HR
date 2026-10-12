import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';

/** 画面の中の詳細のキャッシュ: 直近 20 件を 60 秒 (サーバのキャッシュと同じ長さ)。新しさの確認はそのたびに裏で行う */
export const DETAIL_CACHE_TTL_MS = 60_000;
export const DETAIL_CACHE_MAX = 20;

interface Entry { dealId: string; data: WorkspaceResponse; at: number }

/**
 * 取得関数ごとに別のキャッシュを持つ (実データと架空サンプル、テストの偽の取得関数が互いの結果を見ない)。
 * 鍵は (案件, モード, 選んだ項目の鍵)。
 */
export class DetailCache {
  private readonly map = new Map<string, Entry>();
  constructor(private readonly now: () => number = Date.now) {}

  static key(dealId: string, mode: string, propsKey: string): string { return `${dealId}|${mode}|${propsKey}`; }

  get(key: string): WorkspaceResponse | null {
    const e = this.map.get(key);
    if (e === undefined) return null;
    if (this.now() - e.at > DETAIL_CACHE_TTL_MS) { this.map.delete(key); return null; }
    // 使った順に並べ直す (古いものから捨てる)
    this.map.delete(key);
    this.map.set(key, e);
    return e.data;
  }

  has(key: string): boolean { return this.get(key) !== null; }

  set(key: string, dealId: string, data: WorkspaceResponse): void {
    this.map.delete(key);
    this.map.set(key, { dealId, data, at: this.now() });
    while (this.map.size > DETAIL_CACHE_MAX) {
      const oldest = this.map.keys().next();
      if (oldest.done === true) break;
      this.map.delete(oldest.value);
    }
  }

  /** その案件の全部の鍵を捨てる (「最新にする」・通話が終わった後・書き込み) */
  dropDeal(dealId: string): void {
    for (const [k, e] of this.map) if (e.dealId === dealId) this.map.delete(k);
  }

  clear(): void { this.map.clear(); }
  get size(): number { return this.map.size; }
}

/** 書き込みなどで、全部の取得関数のキャッシュからその案件を捨てるための登録簿 */
const registry = new Set<WeakRef<DetailCache>>();
const caches = new WeakMap<object, DetailCache>();
let clockOverride: (() => number) | null = null;

/** 取得関数ごとのキャッシュ */
export function detailCacheFor(fetcher: object): DetailCache {
  let c = caches.get(fetcher);
  if (c === undefined) { c = new DetailCache(() => (clockOverride ?? Date.now)()); caches.set(fetcher, c); registry.add(new WeakRef(c)); }
  return c;
}

/** 全部の取得関数のキャッシュからその案件を捨てる (書き込み) */
export function dropDealEverywhere(dealId: string): void {
  for (const ref of registry) {
    const c = ref.deref();
    if (c === undefined) registry.delete(ref); else c.dropDeal(dealId);
  }
}

/** テスト用: 時計の差し替え */
export function setDetailCacheClock(now: (() => number) | null): void { clockOverride = now; }
