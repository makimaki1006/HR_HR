/**
 * 顧客報告のスライドに渡す数を、既存の読み込みから集める（新しい API は使わない）。
 * - 案件: 案件のまとめと同じ loadDealData（求人・応募日別・課金・本文と画像の変更）、
 *         契約・成果と同じ取引の項目（面接 mensetu・採用 syoudaku）、求人ごとの応募者（listingApplicationDates）。
 * - 求人: 求人の画面と同じ loadVersions・loadApplicationDates・fetchBodyDates。
 */
import { apiGet } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { loadDealData } from './dealSummaryLoad';
import { dealCutoff, eventSourceOf, perfBillingEntries } from './dealSummaryModel';
import type { DealListingInput } from './dealSummaryModel';
import { fetchBodyDates } from './bodyDatesShared';
import { indeedWeeklyPerformance, publicationLabel, weeklyPerfRaw } from './hubspotListings';
import type { HubSpotListing } from './hubspotListings';
import { loadVersions } from './listingDetailCache';
import { loadApplicationDates } from './listingApplicationDates';
import type { LoadedApplications } from './listingApplicationDates';
import { jstDate, todayJst } from './timelineModel';
import type { BillingEntry } from './timelineModel';
import type { EventSource, ListingChanges } from './dealSummaryModel';
import { APPLICANT_LISTING_LIMIT } from './reportSlidesModel';
import type { ReportApplicants, ReportInput, ReportListing } from './reportSlidesModel';
import type { ApplicationRow } from './data';

export interface ReportTarget { scope: 'deal' | 'listing'; id: string }
const ID = /^\d{1,30}$/;
/** URL の ?report=deal:123 / listing:456。 */
export function parseReportTarget(value: string | null): ReportTarget | null {
  const match = /^(deal|listing):(.+)$/.exec(value ?? '');
  if (!match?.[1] || !match[2] || !ID.test(match[2])) return null;
  return { scope: match[1] as ReportTarget['scope'], id: match[2] };
}
export const reportParam = (target: ReportTarget) => `${target.scope}:${target.id}`;

function reportListing(row: HubSpotListing, byDate: Record<string, number> | null, billing: BillingEntry[], billingUnavailable: boolean, bodyChangedAt: string | null, changes: ListingChanges | undefined): ReportListing {
  const events: EventSource = eventSourceOf(row, bodyChangedAt, changes, billing);
  const ends = (events.statusEvents ?? []).filter(event => event.kind === 'end').map(event => event.date).sort();
  return {
    id: row.id, title: row.title ?? '求人', media: row.media, ended: publicationLabel(row.publication_status) === '掲載終了',
    endedOn: row.listing_ended_on ?? ends.at(-1) ?? null, byDate, billing, billingUnavailable, events,
  };
}

/** 応募者と応募理由。全部の求人の分を読めたときだけ返す（一部だけを全体として出さない）。 */
export function combineApplicants(loaded: readonly (LoadedApplications | null)[], skipped: number): ReportApplicants {
  if (skipped > 0) return { rows: null, reasons: null, missingReason: `求人が多いため（${String(APPLICANT_LISTING_LIMIT)}件まで）、応募者の内訳を作っていません` };
  if (loaded.some(item => item === null)) return { rows: null, reasons: null, missingReason: '一部の求人の応募者を読み込めなかったため' };
  const items = loaded.filter((item): item is LoadedApplications => item !== null);
  const complete = items.every(item => item.rows.length === item.overall.total);
  const rows = new Map<string, ApplicationRow>();
  for (const item of items) for (const row of item.rows) if (!rows.has(row.key)) rows.set(row.key, row);
  const reasons = items.every(item => item.reasons?.available) ? items.map(item => item.reasons).filter(reason => reason !== undefined) : null;
  return {
    rows: complete ? [...rows.values()] : null,
    reasons,
    ...(!complete || !reasons ? { missingReason: !complete ? '応募者の属性がそろっていないため' : '応募理由を読み込めなかったため' } : {}),
  };
}

async function eachLimited<T, R>(items: readonly T[], limit: number, run: (item: T) => Promise<R>): Promise<R[]> {
  const out: R[] = new Array<R>(items.length);
  let next = 0;
  const worker = async () => { for (;;) { const index = next; next += 1; const item = items[index]; if (item === undefined) return; out[index] = await run(item); } };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
  return out;
}

export type LoadResult = { ok: true; input: ReportInput } | { ok: false; message: string };

async function loadDeal(id: string, signal: AbortSignal): Promise<LoadResult> {
  const [data, detail] = await Promise.all([
    loadDealData(id, '', signal),
    apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(id)}?${new URLSearchParams({ deal_props: 'mensetu,syoudaku' }).toString()}`, { signal, timeoutMs: 35_000 }),
  ]);
  if (!data) return { ok: false, message: 'この案件の求人を読み込めませんでした。時間を置いてもう一度開いてください。' };
  const listings = data.rows.map(row => {
    const perf = data.perf.get(row.id);
    const billing = perfBillingEntries(perf, row.media_job_id);
    return reportListing(row, data.applications.get(row.id) ?? null, billing, row.media === 'airwork' && perf === undefined, jstDate(data.bodyDates[row.id] ?? row.summary?.body_changed_at), data.changes?.[row.id]);
  });
  const inputs: DealListingInput[] = data.rows.map(row => ({ id: row.id, title: row.title ?? '', media: row.media, ended: false, byDate: data.applications.get(row.id) ?? null, billing: [], fetchedDate: data.fetched.get(row.id) ?? null }));
  const wanted = data.rows.slice(0, APPLICANT_LISTING_LIMIT);
  const loaded = await eachLimited(wanted, 3, row => loadApplicationDates(row.id, signal).catch(() => null));
  if (signal.aborted) return { ok: false, message: '' };
  const values = detail.ok && !detail.data.partial.some(part => part.part === 'selected_properties') ? detail.data.selected.deal : null;
  const count = (value: string | null | undefined) => { const n = value == null || value === '' ? NaN : Number(value); return Number.isFinite(n) && n >= 0 ? Math.round(n) : null; };
  return { ok: true, input: {
    scope: 'deal', id, name: (detail.ok ? detail.data.deal.name : null) ?? '案件', company: data.company?.name ?? null,
    listings, overlap: data.overlap, asOf: dealCutoff(inputs) ?? todayJst(),
    outcomes: values ? { interviews: count(values.mensetu), hires: count(values.syoudaku) } : null,
    outcomesFailed: values === null,
    applicants: combineApplicants(loaded, data.rows.length - wanted.length),
    notes: [...(data.capped ? ['求人が多いため、先頭の50件までで集計しています'] : []), ...(data.rows.length > 30 ? ['応募日別の件数は先頭の30件の求人の分です'] : [])],
  } };
}

async function loadListing(id: string, signal: AbortSignal): Promise<LoadResult> {
  const [versions, applications, dates] = await Promise.all([
    loadVersions(id, { signal }),
    loadApplicationDates(id, signal).catch(() => null),
    fetchBodyDates([id]),
  ]);
  if (signal.aborted) return { ok: false, message: '' };
  if (!versions.ok) return { ok: false, message: 'この求人を読み込めませんでした。時間を置いてもう一度開いてください。' };
  const row = versions.data.listing;
  const perf = indeedWeeklyPerformance(weeklyPerfRaw(versions.data, row.media), row.media, row.media_job_id);
  const billing = perfBillingEntries(perf, row.media_job_id);
  const listing = reportListing(row, applications?.overall.byDate ?? null, billing, row.media === 'airwork' && perf === undefined,
    jstDate((dates.ok ? dates.data.dates?.[id] : null) ?? row.summary?.body_changed_at), dates.ok ? dates.data.changes?.[id] : undefined);
  return { ok: true, input: {
    scope: 'listing', id, name: row.title ?? '求人', company: versions.data.company_name ?? null,
    listings: [listing], overlap: {}, asOf: jstDate(applications?.overall.fetchedAt) ?? todayJst(), outcomes: null,
    applicants: combineApplicants([applications], 0),
  } };
}

export function loadReportInput(target: ReportTarget, signal: AbortSignal): Promise<LoadResult> {
  return (target.scope === 'deal' ? loadDeal(target.id, signal) : loadListing(target.id, signal)).catch(() => ({ ok: false as const, message: signal.aborted ? '' : '読み込めませんでした。時間を置いてもう一度開いてください。' }));
}
