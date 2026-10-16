/** 取引ひとつ分の求人・応募・課金を読み込む。取引の画面と事業所の画面が同じものを使う。 */
import { apiGet } from '../../api/client';
import { fetchBodyDates } from './bodyDatesShared';
import { applicationsCutoffDate } from './detailBandModel';
import { indeedWeeklyPerformance, weeklyPerfRaw } from './hubspotListings';
import type { HubSpotListing, HubSpotListingPage } from './hubspotListings';
import type { HrhPerformanceCollection } from './hrhPerformanceModel';
import { perfBillingEntries, ROLLUP_RULES } from './dealSummaryModel';
import { loadCompanyDeals } from './companyDeals';
import type { CompanyDeal } from './companyDeals';
import type { DealGroup, DealListingInput } from './dealSummaryModel';
import { publicationLabel } from './hubspotListings';
import type { BillingEntry } from './timelineModel';

interface DealApplications {
  listings: { listing_id: string; total?: number; by_date?: Record<string, number>; fetched_at?: string; error?: string; indeed_weekly_perf?: unknown; airwork_weekly_perf?: unknown }[];
  shared_applications: number;
  shared_by_date?: Record<string, number>;
  company?: { id: string; name: string } | null;
}

export interface DealData {
  id: string;
  name: string;
  rows: HubSpotListing[];
  capped: boolean;
  applications: Map<string, Record<string, number> | null>;
  shared: number;
  overlap: Record<string, number>;
  fetched: Map<string, string | null>;
  bodyDates: Record<string, string | null>;
  perf: Map<string, HrhPerformanceCollection | undefined>;
  company: { id: string; name: string } | null;
}

export async function loadDealData(dealId: string, name: string, signal: AbortSignal): Promise<DealData | null> {
  const page = await apiGet<HubSpotListingPage>(`/api/job-copy/listings?${new URLSearchParams({ deal: dealId, offset: '0', sort: 'title', descending: 'false' }).toString()}`, { signal, timeoutMs: 120_000 });
  if (signal.aborted || !page.ok) return null;
  const rows = page.data.listings;
  const data: DealData = {
    id: dealId, name, rows, capped: page.data.next_offset !== null,
    applications: new Map(rows.map(row => [row.id, null])), shared: 0, overlap: {}, fetched: new Map(), bodyDates: {}, perf: new Map(), company: null,
  };
  if (!rows.length) return data;
  const ids = rows.slice(0, 30).map(row => encodeURIComponent(row.id)).join(',');
  const [apps, dates] = await Promise.all([
    apiGet<DealApplications>(`/api/job-copy/deal-applications?deal=${encodeURIComponent(dealId)}&ids=${ids}`, { signal, timeoutMs: 120_000 }),
    fetchBodyDates(rows.map(row => row.id)),
  ]);
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- aborted can change while awaiting
  if (signal.aborted) return null;
  if (apps.ok) {
    const find = (id: string) => apps.data.listings.find(item => item.listing_id === id);
    data.applications = new Map(rows.map(row => { const found = find(row.id); return [row.id, found?.by_date && !found.error ? found.by_date : null]; }));
    data.shared = apps.data.shared_applications;
    data.overlap = apps.data.shared_by_date ?? {};
    data.fetched = new Map(rows.map(row => [row.id, applicationsCutoffDate({ overallApplications: { fetchedAt: find(row.id)?.fetched_at } })]));
    data.perf = new Map(rows.map(row => [row.id, indeedWeeklyPerformance(weeklyPerfRaw(find(row.id) ?? {}, row.media), row.media, row.media_job_id)]));
    data.company = apps.data.company ?? null;
  }
  if (dates.ok && dates.data.dates) data.bodyDates = dates.data.dates;
  return data;
}

/** 計算に渡す求人。HRハッカーは HubSpot の週ごとの実績 (と取り込んだ課金CSV)、AirWork は HubSpot の週ごとの実績があるものだけ (無い求人は課金データなし)。 */
export function dealInputs(data: DealData, billingByJob: Readonly<Record<string, readonly BillingEntry[]>>): DealListingInput[] {
  return data.rows.map(row => ({
    id: row.id, title: row.title ?? '求人名未取得', media: row.media, ended: publicationLabel(row.publication_status) === '掲載終了',
    byDate: data.applications.get(row.id) ?? null,
    billing: row.media === 'airwork' ? perfBillingEntries(data.perf.get(row.id), row.media_job_id) : [...perfBillingEntries(data.perf.get(row.id), row.media_job_id), ...(billingByJob[`hubspot-history-${row.id}`] ?? [])],
    fetchedDate: data.fetched.get(row.id) ?? null,
    billingUnavailable: row.media === 'airwork' && data.perf.get(row.id) === undefined,
    inOtherDeals: (row.deal_ids?.length ?? 0) > 1,
  }));
}

export function dealGroup(data: DealData, billingByJob: Readonly<Record<string, readonly BillingEntry[]>>): DealGroup {
  return { id: data.id, name: data.name, inputs: dealInputs(data, billingByJob), overlap: data.overlap };
}

/** 顧客（事業所）の全部の取引の求人を集める。同じ求人は取引をまたいで 1 回だけ数える（合計は呼び出し側）。 */
export async function loadCompanyDatas(companyId: string, signal: AbortSignal, knownDeals?: CompanyDeal[]): Promise<{ status: 'ready'; datas: DealData[]; notes: string[] } | { status: 'error'; message: string }> {
  const deals = knownDeals ? { ok: true as const, data: { deals: knownDeals } } : await loadCompanyDeals(companyId);
  if (!deals.ok) return { status: 'error', message: 'この事業所の取引を読み込めませんでした。時間を置いて再取得してください。' };
  const wanted = deals.data.deals.slice(0, ROLLUP_RULES.maxDeals);
  const loaded: DealData[] = [];
  let missing = 0;
  for (const deal of wanted) {
    const data = await loadDealData(deal.id, deal.name ?? '取引名未取得', signal);
     
    if (signal.aborted) return { status: 'error', message: '' };
    if (data === null) missing += 1; else if (data.rows.length) loaded.push(data);
  }
  const notes: string[] = [];
  if (deals.data.deals.length > wanted.length) notes.push(`取引が多いため、先頭の${String(wanted.length)}件までで集計しています。`);
  if (missing > 0) notes.push(`読み込めなかった取引が ${String(missing)}件あります。その取引の求人は合計に含まれていません。`);
  if (loaded.length === 0 && missing > 0) return { status: 'error', message: 'この事業所の求人を読み込めませんでした。時間を置いて再取得してください。' };
  return { status: 'ready', datas: loaded, notes };
}
