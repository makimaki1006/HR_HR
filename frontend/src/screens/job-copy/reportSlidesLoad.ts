/**
 * 顧客報告のスライドに渡す数を、既存の読み込みから集める（新しい API は使わない）。
 * - 案件: 案件のまとめと同じ loadDealData（求人・応募日別・本文と画像の変更。課金は読むが紙には出さない）、
 *         契約・成果と同じ案件の読み取り（案件名・面接 mensetu・採用 syoudaku）、求人ごとの応募者（listingApplicationDates）、
 *         危ない案件の小窓と同じ案件の記録（MTG の日。前回の MTG から期間を決めるのに使う）。
 * 媒体の表示回数・クリック数は、求人の読み取りに入っている週ごとの実績（Indeed・AirWork）をそのまま使う。
 * - 求人: 求人の画面と同じ loadVersions・loadApplicationDates・fetchBodyDates。
 * 読んだ結果はこのタブのメモリに持ち、開き直したときは読み直さない（「最新にする」で読み直す）。
 */
import { apiGet } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { loadDealData } from './dealSummaryLoad';
import { dealCutoff, eventSourceOf, perfBillingEntries } from './dealSummaryModel';
import type { DealListingInput } from './dealSummaryModel';
import { fetchBodyDates } from './bodyDatesShared';
import { indeedWeeklyPerformance, publicationLabel, weeklyPerfRaw } from './hubspotListings';
import type { HubSpotListing } from './hubspotListings';
import type { HrhPerformanceCollection } from './hrhPerformanceModel';
import { loadVersions } from './listingDetailCache';
import { loadApplicationDates } from './listingApplicationDates';
import type { LoadedApplications } from './listingApplicationDates';
import { jstDate, todayJst } from './timelineModel';
import type { BillingEntry } from './timelineModel';
import type { EventSource, ListingChanges } from './dealSummaryModel';
import { APPLICANT_LISTING_LIMIT } from './reportSlidesModel';
import type { ReportApplicants, ReportInput, ReportListing } from './reportSlidesModel';

export interface ReportTarget { scope: 'deal' | 'listing'; id: string }
const ID = /^\d{1,30}$/;
/** URL の ?report=deal:123 / listing:456。 */
export function parseReportTarget(value: string | null): ReportTarget | null {
  const match = /^(deal|listing):(.+)$/.exec(value ?? '');
  if (!match?.[1] || !match[2] || !ID.test(match[2])) return null;
  return { scope: match[1] as ReportTarget['scope'], id: match[2] };
}
export const reportParam = (target: ReportTarget) => `${target.scope}:${target.id}`;

function reportListing(row: HubSpotListing, byDate: Record<string, number> | null, perf: HrhPerformanceCollection | undefined, bodyChangedAt: string | null, changes: ListingChanges | undefined): ReportListing {
  const billing: BillingEntry[] = perfBillingEntries(perf, row.media_job_id);
  const events: EventSource = eventSourceOf(row, bodyChangedAt, changes, billing);
  const ends = (events.statusEvents ?? []).filter(event => event.kind === 'end').map(event => event.date).sort();
  return {
    id: row.id, title: row.title ?? '求人', media: row.media, place: [row.prefecture, row.municipality].filter(Boolean).join('') || null,
    ended: publicationLabel(row.publication_status) === '掲載終了', endedOn: row.listing_ended_on ?? ends.at(-1) ?? null, byDate, events, perf: perf?.rows ?? null,
  };
}

/**
 * 応募者と応募理由。全部の求人の分を読めたときだけ返す（一部だけを全体として出さない）。
 * 応募のキーは求人ごとに別なので、同じ方が複数の求人に応募すると重なって見える。案件の単位では、
 * HubSpot が「ほかの求人にも結び付いている」とした応募（multiListingApplicants）を両方から外す（求人の画面の応募理由と同じ決まり）。
 * skipped: 読まなかった求人（上限を超えた）。zero: 応募が無いと分かっていて読まなかった求人（数えなくてよい）。
 */
export function combineApplicants(loaded: readonly (LoadedApplications | null)[], skipped: number, scope: 'deal' | 'listing' = 'deal'): ReportApplicants {
  if (skipped > 0) return { rows: null, reasons: null, missingReason: `応募のある求人が多いため（${String(APPLICANT_LISTING_LIMIT)}件まで）、応募者の内訳を作っていません` };
  if (loaded.some(item => item === null)) return { rows: null, reasons: null, missingReason: '一部の求人の応募者を読み込めなかったため' };
  const items = loaded.filter((item): item is LoadedApplications => item !== null);
  const many = scope === 'deal' && items.length > 1;
  if (many && items.some(item => !Array.isArray(item.reasons?.multiListingApplicants))) return { rows: null, reasons: null, missingReason: '複数の求人に結び付いた応募を見分けられないため（重ねて数えないよう外しています）' };
  const multi = new Set(many ? items.flatMap(item => item.reasons?.multiListingApplicants ?? []) : []);
  const complete = items.every(item => item.rows.length === item.overall.total);
  const all = items.flatMap(item => item.rows);
  const rows = all.filter(row => !multi.has(row.key));
  const reasons = items.every(item => item.reasons?.available) ? items.map(item => item.reasons).filter(reason => reason !== undefined) : null;
  return {
    rows: complete ? rows : null,
    reasons,
    // 応募理由は、求人の単位でも複数の求人に結び付いた応募を外す（求人の画面と同じ）。
    multiKeys: [...new Set(items.flatMap(item => item.reasons?.multiListingApplicants ?? []))],
    excludedMulti: all.length - rows.length,
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

interface DealRecords { events?: { kind: string; date: string; attach?: { state: string } }[] }
/**
 * 案件の MTG の記録の日（古い順）。危ない案件の小窓の「最後のMTG」と同じ決まり（MTG の記録と Zoom の開催。別の契約の記録は入れない）。
 * 読めなかったときは null（期間は画面で選ぶ）。
 */
async function loadMtgDates(id: string, signal: AbortSignal): Promise<string[] | null> {
  const result = await apiGet<DealRecords>(`/api/consulting/deal-detail?deal_id=${encodeURIComponent(id)}`, { signal, timeoutMs: 30_000 }).catch(() => null);
  if (!result?.ok || !Array.isArray(result.data.events)) return null;
  const dates = result.data.events
    .filter(event => (event.kind === 'mtg' || event.kind === 'zoom_mtg') && (!event.attach || event.attach.state === 'own' || event.attach.state === 'moved_in'))
    .map(event => event.date.slice(0, 10)).filter(date => /^\d{4}-\d{2}-\d{2}$/.test(date));
  return [...new Set(dates)].sort();
}

export type LoadResult = { ok: true; input: ReportInput } | { ok: false; message: string };
/** 先に出した後で読み終えた分（案件の応募者）。読み終えた入力を渡す。 */
type OnUpdate = (input: ReportInput) => void;
/** 応募者を読んでいる間の値。応募者を使うのは最初は入れない補足のスライドだけなので、先に画面を出す。 */
const APPLICANTS_LOADING: ReportApplicants = { rows: null, reasons: null, missingReason: '応募者を読み込んでいる途中のため（少し待つと入ります）' };

async function loadDeal(id: string, signal: AbortSignal, onUpdate: OnUpdate): Promise<LoadResult> {
  const [data, detail, mtgDates] = await Promise.all([
    loadDealData(id, '', signal),
    apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(id)}?${new URLSearchParams({ deal_props: 'mensetu,syoudaku' }).toString()}`, { signal, timeoutMs: 35_000 }),
    loadMtgDates(id, signal),
  ]);
  if (!data) return { ok: false, message: 'この案件の求人を読み込めませんでした。時間を置いてもう一度開いてください。' };
  const listings = data.rows.map(row => {
    return reportListing(row, data.applications.get(row.id) ?? null, data.perf.get(row.id), jstDate(data.bodyDates[row.id] ?? row.summary?.body_changed_at), data.changes?.[row.id]);
  });
  const inputs: DealListingInput[] = data.rows.map(row => ({ id: row.id, title: row.title ?? '', media: row.media, ended: false, byDate: data.applications.get(row.id) ?? null, billing: [], fetchedDate: data.fetched.get(row.id) ?? null }));
  // 応募日別に1件も無いと分かっている求人は読まない（期間の応募者・理由は応募日で数えるので、入るものが無い）。
  const wanted = listings.filter(listing => listing.byDate === null || Object.values(listing.byDate).some(count => count > 0));
  const values = detail.ok && !detail.data.partial.some(part => part.part === 'selected_properties') ? detail.data.selected.deal : null;
  const count = (value: string | null | undefined) => { const n = value == null || value === '' ? NaN : Number(value); return Number.isFinite(n) && n >= 0 ? Math.round(n) : null; };
  const input: ReportInput = {
    scope: 'deal', id, name: (detail.ok ? detail.data.deal.name : null) ?? null, company: data.company?.name ?? null,
    listings, overlap: data.overlap, asOf: dealCutoff(inputs) ?? todayJst(),
    outcomes: values ? { interviews: count(values.mensetu), hires: count(values.syoudaku), readOn: todayJst() } : null, outcomesFailed: !values,
    applicants: APPLICANTS_LOADING,
    partial: data.capped ? ['求人が多いため、先頭の50件までしか読んでいません'] : [],
    mtgDates,
  };
  // 求人ごとの応募者は時間がかかるので待たずに画面を出し、読み終えたら入れ替える。
  if (wanted.length > APPLICANT_LISTING_LIMIT) return { ok: true, input: { ...input, applicants: combineApplicants([], wanted.length, 'deal') } };
  if (!wanted.length) return { ok: true, input: { ...input, applicants: combineApplicants([], 0, 'deal') } };
  void eachLimited(wanted, 3, listing => loadApplicationDates(listing.id, signal).catch(() => null)).then(loaded => {
    if (!signal.aborted) onUpdate({ ...input, applicants: combineApplicants(loaded, 0, 'deal') });
  });
  return { ok: true, input };
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
  const listing = reportListing(row, applications?.overall.byDate ?? null, perf,
    jstDate((dates.ok ? dates.data.dates?.[id] : null) ?? row.summary?.body_changed_at), dates.ok ? dates.data.changes?.[id] : undefined);
  return { ok: true, input: {
    scope: 'listing', id, name: row.title ?? null, company: versions.data.company_name ?? null,
    listings: [listing], overlap: {}, asOf: jstDate(applications?.overall.fetchedAt) ?? todayJst(), outcomes: null,
    applicants: combineApplicants([applications], 0, 'listing'),
    // 求人の読み取りと同じ応答に入っている（HubSpot の呼び出しは増えない）。
    competitor: versions.data.competitor_monthly ?? [],
  } };
}

/** このタブで読んだ結果。開き直したときは読み直さない。 */
const loadedReports = new Map<string, ReportInput>();
export function cachedReportInput(target: ReportTarget): ReportInput | undefined {
  return loadedReports.get(reportParam(target));
}
/**
 * onUpdate: 案件の応募者を読み終えたときに、入れ替えた入力を渡す。
 * 途中の入力（応募者を読んでいる間）は残さず、読み終えたものだけをこのタブに残す。
 */
export function loadReportInput(target: ReportTarget, signal: AbortSignal, options: { fresh?: boolean; onUpdate?: OnUpdate } = {}): Promise<LoadResult> {
  const kept = options.fresh ? undefined : cachedReportInput(target);
  if (kept) return Promise.resolve({ ok: true, input: kept });
  const keep = (input: ReportInput) => { if (input.applicants !== APPLICANTS_LOADING) loadedReports.set(reportParam(target), input); };
  const onUpdate: OnUpdate = input => { keep(input); options.onUpdate?.(input); };
  return (target.scope === 'deal' ? loadDeal(target.id, signal, onUpdate) : loadListing(target.id, signal))
    .then(result => { if (result.ok) keep(result.input); return result; })
    .catch(() => ({ ok: false as const, message: signal.aborted ? '' : '読み込めませんでした。時間を置いてもう一度開いてください。' }));
}
