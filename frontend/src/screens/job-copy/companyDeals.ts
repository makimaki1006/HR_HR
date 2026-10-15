import { apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';

/** 顧客の案件 1 件。納品管理のパイプラインの取引だけが来る（サーバが絞る）。 */
export interface CompanyDeal {
  id: string; name: string | null;
  pipeline_id: string; pipeline_label: string; stage_label: string | null; active: boolean;
  /** その顧客の「今の取引」（契約中で最新。契約中が無ければ最新の取引）。顧客ごとに 1 件。 */
  current: boolean;
  created_at: string | null; contract_start: string | null; contract_end: string | null;
  /** 結び付いた求人。索引の準備前は null。 */
  listings_total: number | null; listings_live: number | null;
  recent_apps: number | null; recent_cost: number | null;
}
export interface CompanyDeals {
  company: { id: string; name: string | null };
  deals: CompanyDeal[];
  delivery_pipelines?: { id: string; label: string }[];
  index_ready?: boolean;
}
export const dealName = (deal: { name: string | null }) => deal.name ?? '取引名未取得';
/** 「掲載中 X ／ 全 Y」。索引の準備前は「集計中」。 */
export const listingsLabel = (deal: Pick<CompanyDeal, 'listings_live' | 'listings_total'>) => deal.listings_total === null || deal.listings_live === null ? '集計中' : `掲載中 ${String(deal.listings_live)} ／ 全 ${String(deal.listings_total)}`;
/** 契約の期間。片方だけ分かるときは分かる側だけ。 */
export const contractLabel = (deal: Pick<CompanyDeal, 'contract_start' | 'contract_end'>) => deal.contract_start || deal.contract_end ? `${deal.contract_start ?? '—'} 〜 ${deal.contract_end ?? '—'}` : '—';

const cache = new Map<string, { at: number; value: Promise<ApiResult<CompanyDeals>> }>();
const TTL_MS = 30_000;
/** 同じ顧客の案件を、左の一覧と顧客の画面で二重に読まない（30 秒だけ使い回す。失敗は覚えない）。 */
export function loadCompanyDeals(companyId: string, options: { fresh?: boolean } = {}): Promise<ApiResult<CompanyDeals>> {
  const hit = cache.get(companyId);
  if (!options.fresh && hit && Date.now() - hit.at < TTL_MS) return hit.value;
  const value = apiGet<CompanyDeals>(`/api/job-copy/company-deals?company=${encodeURIComponent(companyId)}`, { timeoutMs: 120_000 }).then(result => {
    if (!result.ok && cache.get(companyId)?.value === value) cache.delete(companyId);
    return result;
  });
  cache.set(companyId, { at: Date.now(), value });
  return value;
}
export function clearCompanyDealsCache() { cache.clear(); }
