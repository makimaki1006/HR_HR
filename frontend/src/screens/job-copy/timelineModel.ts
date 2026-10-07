/**
 * Pure model for the job timeline: one job's publication periods, salary/body/image changes,
 * billing, applications and market rows on one day-based (JST) axis.
 *
 * Dates are calendar days in Japan time, written YYYY-MM-DD. Period ends are exclusive: the day a
 * new version starts belongs to the new version. Nothing here allocates an application to a
 * version; counts are only grouped by the application date HubSpot recorded.
 */
import type { CopyVersion, JobCopyRecord } from './data';
import { compareCopy } from './diff';
import type { CopyComparisonStatus } from './diff';
import { compareImages, referenceImages } from './images';
import type { MarketRow } from './marketChartModel';
import type { BillingPeriod } from './billingTypes';
import { extractSalary, isSalaryLine, sameSalary } from './salaryExtract';
import { formatYen as formatYenJa } from './format';
import type { SalaryInfo } from './salaryExtract';

const DAY_MS = 86_400_000;
const JST_OFFSET_MS = 9 * 60 * 60 * 1000;
const DATE = /^\d{4}-\d{2}-\d{2}$/;

/** Calendar day in Japan time for an ISO timestamp (or a plain YYYY-MM-DD). null when unreadable. */
export function jstDate(value: string | undefined | null): string | null {
  if (!value) return null;
  if (DATE.test(value)) return value;
  const time = Date.parse(value);
  if (!Number.isFinite(time)) return null;
  return new Date(time + JST_OFFSET_MS).toISOString().slice(0, 10);
}
export function dayNumber(date: string): number {
  return Math.floor(Date.parse(`${date}T00:00:00Z`) / DAY_MS);
}
export function addDays(date: string, days: number): string {
  return new Date((dayNumber(date) + days) * DAY_MS).toISOString().slice(0, 10);
}
/** b − a in days. */
export function daysBetween(a: string, b: string): number {
  return dayNumber(b) - dayNumber(a);
}
export function monthOf(date: string): string {
  return date.slice(0, 7);
}
export function nextMonth(month: string): string {
  const year = Number(month.slice(0, 4)); const value = Number(month.slice(5, 7));
  return `${String(value === 12 ? year + 1 : year).padStart(4, '0')}-${String(value === 12 ? 1 : value + 1).padStart(2, '0')}`;
}
/** Today in Japan time. */
export function todayJst(now: Date = new Date()): string {
  return new Date(now.getTime() + JST_OFFSET_MS).toISOString().slice(0, 10);
}
/** The day the application counts were taken (falls back to today). */
export function asOfDate(job: JobCopyRecord, now?: Date  ): string {
  return jstDate(job.overallApplications?.fetchedAt) ?? todayJst(now);
}

/**
 * One billing period. The CSV import team passes these in (source 'csv'); HRハッカー rows already
 * in the snapshot become source 'hrhacker'. Dates are inclusive YYYY-MM-DD. amountYen null means
 * the amount was not given (never shown as 0).
 */
export interface BillingEntry {
  source: 'hrhacker' | 'csv';
  start: string;
  end: string;
  amountYen: number | null;
  /** Whether amountYen includes tax; null when the source does not say. */
  taxIncluded?: boolean | null;
  media?: string;
  mediaJobId?: string;
  plan?: string | null;
  impressions?: number | null;
  clicks?: number | null;
  /** Applications counted by the media itself (not HubSpot). */
  mediaApplications?: number | null;
  /** Row number in the billing CSV (header is row 1). */
  sourceRow?: number | null;
}

/**
 * Billing CSV periods (billingImport.ts) grouped by job id, in the shape the timeline reads.
 * HRハッカー実績 rows are not converted here: billingEntries() reads them from the job itself.
 */
export function billingEntriesByJob(periods: readonly BillingPeriod[]): Record<string, BillingEntry[]> {
  const byJob: Record<string, BillingEntry[]> = {};
  for (const period of periods) {
    if (period.source !== 'csv') continue;
    (byJob[period.jobId] ??= []).push({
      source: 'csv', start: period.periodStart, end: period.periodEnd, amountYen: period.amountYen,
      taxIncluded: period.taxBasis === '税込' ? true : period.taxBasis === '税抜' ? false : null,
      media: period.media, mediaJobId: period.mediaJobId, plan: period.planName,
      impressions: period.impressions, clicks: period.clicks, mediaApplications: period.mediaApplications, sourceRow: period.sourceRow,
    });
  }
  return byJob;
}

/** True when two of the billing periods share at least one day. Such amounts are never added up. */
export function billingOverlaps(entries: readonly BillingEntry[]): boolean {
  const sorted = [...entries].sort((a, b) => a.start.localeCompare(b.start));
  for (let index = 1; index < sorted.length; index += 1) {
    const previous = sorted[index - 1]; const current = sorted[index];
    if (previous && current && current.start <= previous.end) return true;
  }
  return false;
}

export function billingEntries(job: JobCopyRecord, injected?: readonly BillingEntry[]  ): BillingEntry[] {
  const fromHrh: BillingEntry[] = (job.hrhPerformance?.rows ?? []).map(row => ({
    source: 'hrhacker', start: row.period_start, end: row.period_end, amountYen: row.cost_yen, taxIncluded: null,
    media: 'HRハッカー', mediaJobId: job.mediaJobId, impressions: row.impressions, clicks: row.clicks, mediaApplications: row.applications,
  }));
  const csv = (injected ?? []).filter(entry => DATE.test(entry.start) && DATE.test(entry.end) && entry.start <= entry.end);
  // A billing CSV row for exactly the same HRハッカー period (same start and end) replaces the
  // HRハッカー実績 row, so one period is not counted twice. A CSV row that covers only part of an
  // HRハッカー period does not remove it: both stay, and billingOverlaps() leaves the total blank.
  const hrh = fromHrh.filter(row => !csv.some(entry => (entry.media ?? 'HRハッカー') === 'HRハッカー' && entry.start === row.start && entry.end === row.end));
  return [...hrh, ...csv]
    .filter(entry => DATE.test(entry.start) && DATE.test(entry.end) && entry.start <= entry.end)
    .sort((a, b) => a.start.localeCompare(b.start) || a.source.localeCompare(b.source));
}

export function publishedVersions(job: JobCopyRecord): CopyVersion[] {
  return job.versions
    .filter(version => version.kind === 'published' && jstDate(version.publishedFrom ?? version.observedAt) !== null)
    .sort((a, b) => Date.parse(a.publishedFrom ?? a.observedAt) - Date.parse(b.publishedFrom ?? b.observedAt));
}

export interface TimelinePeriod {
  versionId: string;
  label: string;
  start: string;
  /** Exclusive end day; null while the version is still the latest one. */
  end: string | null;
  /** Days covered. A still-running period counts up to and including asOf. */
  days: number;
  ongoing: boolean;
  certainty: CopyVersion['certainty'];
  /** 'published': media publication times. 'captured': only the days the data was taken. */
  basis: 'published' | 'captured';
}

export function buildPeriods(job: JobCopyRecord, asOf: string): TimelinePeriod[] {
  const versions = publishedVersions(job);
  const starts = versions.map(version => jstDate(version.publishedFrom ?? version.observedAt) ?? '');
  return versions.map((version, index) => {
    const start = starts[index] ?? '';
    const nextStart = starts[index + 1] ?? null;
    let end = jstDate(version.publishedUntil) ?? nextStart;
    if (end !== null && nextStart !== null && end > nextStart) end = nextStart;
    if (end !== null && end < start) end = start;
    const ongoing = end === null;
    const days = ongoing ? Math.max(0, daysBetween(start, asOf) + 1) : daysBetween(start, end ?? start);
    return { versionId: version.id, label: version.label, start, end, days, ongoing, certainty: version.certainty, basis: version.publishedFrom ? 'published' : 'captured' };
  });
}

export type ImageChange = 'initial' | 'changed' | 'same' | 'unknown';
export interface VersionChange {
  versionId: string;
  label: string;
  /** Day the version starts (JST). */
  date: string;
  index: number;
  salary: SalaryInfo | null;
  /** null for the first version, or when either salary could not be read. */
  salaryChanged: boolean | null;
  bodyStatus: CopyComparisonStatus;
  bodyAdded: number;
  bodyRemoved: number;
  /** Lines other than the salary line were added or removed. */
  otherBodyChanged: boolean;
  imageChange: ImageChange;
}

export function versionChanges(job: JobCopyRecord): VersionChange[] {
  const versions = publishedVersions(job);
  return versions.map((version, index) => {
    const previous = versions[index - 1];
    const salary = extractSalary(version.body);
    const previousSalary = previous ? extractSalary(previous.body) : null;
    const salaryChanged = !previous ? null
      : salary?.kind === '不明' || previousSalary?.kind === '不明' ? null
        : !sameSalary(previousSalary, salary);
    const result = compareCopy(previous?.body ?? null, version.body);
    const changed = result.lines.filter(line => line.kind !== 'same');
    const images = compareImages(referenceImages(previous), referenceImages(version));
    const imageChange: ImageChange = !previous ? (referenceImages(version) ? 'initial' : 'unknown')
      : images.status === 'unknown' ? 'unknown' : images.status === 'same_reference' ? 'same' : 'changed';
    return {
      versionId: version.id, label: version.label, date: jstDate(version.publishedFrom ?? version.observedAt) ?? '', index, salary, salaryChanged,
      bodyStatus: result.status,
      bodyAdded: previous ? changed.filter(line => line.kind === 'added').length : 0,
      bodyRemoved: previous ? changed.filter(line => line.kind === 'removed').length : 0,
      otherBodyChanged: Boolean(previous) && result.status === 'changed' && changed.some(line => !isSalaryLine(line.text)),
      imageChange,
    };
  });
}

/** Kinds of change at a version, in lane order. */
export function changeKinds(change: VersionChange): ('給与' | '本文' | '画像')[] {
  if (change.index === 0) return [];
  const kinds: ('給与' | '本文' | '画像')[] = [];
  if (change.salaryChanged) kinds.push('給与');
  if (change.otherBodyChanged) kinds.push('本文');
  if (change.imageChange === 'changed') kinds.push('画像');
  return kinds;
}

/** Applications recorded on days in [start, endExclusive). */
export function countApplications(byDate: Record<string, number> | undefined, start: string, endExclusive: string): number {
  if (!byDate) return 0;
  let total = 0;
  for (const [date, count] of Object.entries(byDate)) if (date >= start && date < endExclusive) total += count;
  return total;
}

export type Granularity = 'day' | 'week' | 'month';
export interface ApplicationBucket { start: string; end: string; count: number }
/** Buckets with at least one dated application. end is exclusive. Monday-start weeks. */
export function applicationBuckets(byDate: Record<string, number> | undefined, granularity: Granularity): ApplicationBucket[] {
  const buckets = new Map<string, number>();
  for (const [date, count] of Object.entries(byDate ?? {})) {
    if (!DATE.test(date) || count <= 0) continue;
    let key = date;
    if (granularity === 'week') {
      const weekday = (new Date(`${date}T00:00:00Z`).getUTCDay() + 6) % 7;
      key = addDays(date, -weekday);
    } else if (granularity === 'month') key = `${monthOf(date)}-01`;
    buckets.set(key, (buckets.get(key) ?? 0) + count);
  }
  return [...buckets.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([start, count]) => ({
    start, count,
    end: granularity === 'day' ? addDays(start, 1) : granularity === 'week' ? addDays(start, 7) : `${nextMonth(monthOf(start))}-01`,
  }));
}

export interface TimelineRange { start: string; end: string }
/** Inclusive day range covering periods, dated applications, billing and asOf. */
export function timelineRange(job: JobCopyRecord, asOf: string, billing: readonly BillingEntry[]): TimelineRange | null {
  const periods = buildPeriods(job, asOf);
  const starts = [...periods.map(period => period.start), ...Object.keys(job.overallApplications?.byDate ?? {}).filter(date => DATE.test(date)), ...billing.map(entry => entry.start)];
  if (!starts.length) return null;
  const ends = [asOf, ...periods.map(period => period.end ? addDays(period.end, -1) : asOf), ...Object.keys(job.overallApplications?.byDate ?? {}).filter(date => DATE.test(date)), ...billing.map(entry => entry.end)];
  const start = starts.reduce((a, b) => (a < b ? a : b));
  const end = ends.reduce((a, b) => (a > b ? a : b));
  return { start, end: end < start ? start : end };
}
/** Position of the start of a day on the range, in percent (0–100). */
export function positionOf(date: string, range: TimelineRange): number {
  const total = daysBetween(range.start, range.end) + 1;
  return Math.min(100, Math.max(0, daysBetween(range.start, date) / total * 100));
}

export interface MarketLanePoint { month: string; jobs: number | null; viewers: number | null }
export interface MarketLane {
  points: MarketLanePoint[];
  /** Last month with a market value at all (2026-08 at the time of writing). */
  lastDataMonth: string | null;
  /** First month in the range after the data ends; null when the data covers the whole range. */
  noDataFrom: string | null;
}
export function marketLane(rows: readonly MarketRow[], range: TimelineRange): MarketLane {
  const byMonth = new Map(rows.map(row => [row.month, row]));
  const withData = rows.filter(row => row.jobs !== null || row.viewers !== null).map(row => row.month).sort();
  const lastDataMonth = withData.at(-1) ?? null;
  const points: MarketLanePoint[] = [];
  const last = monthOf(range.end);
  for (let month = monthOf(range.start); month <= last && points.length < 240; month = nextMonth(month)) {
    const row = byMonth.get(month);
    const afterData = lastDataMonth === null || month > lastDataMonth;
    points.push({ month, jobs: afterData ? null : row?.jobs ?? null, viewers: afterData ? null : row?.viewers ?? null });
  }
  // No row with a value at all: there is no "data ends here" point (the caller says there is no
  // market data for this choice instead).
  const noDataFrom = lastDataMonth === null ? null : nextMonth(lastDataMonth) <= last ? nextMonth(lastDataMonth) : null;
  return { points, lastDataMonth, noDataFrom: noDataFrom && noDataFrom < monthOf(range.start) ? monthOf(range.start) : noDataFrom };
}

export interface MarketChange {
  fromMonth: string; toMonth: string; fromJobs: number; toJobs: number; changePct: number;
  /** First month of the period with no market data (the period runs past the data); null otherwise. */
  noDataFrom: string | null;
}
export type MarketChangeResult = { ok: true; value: MarketChange } | { ok: false; reason: 'not_selected' | 'no_data' | 'same_month'; month?: string; jobs?: number | null; noDataFrom?: string | null };

/**
 * Market job count at the month of the first day vs the month of the last day. When the period runs
 * past the last month with market data, the comparison stops at that month and says from which
 * month there is no data (instead of dropping the months that do have data).
 */
export function marketChange(rows: readonly MarketRow[] | null, firstDay: string, lastDay: string): MarketChangeResult {
  if (!rows) return { ok: false, reason: 'not_selected' };
  const lastDataMonth = rows.filter(row => row.jobs !== null).map(row => row.month).sort().at(-1) ?? null;
  const fromMonth = monthOf(firstDay); let toMonth = monthOf(lastDay);
  let noDataFrom: string | null = null;
  if (lastDataMonth !== null && toMonth > lastDataMonth && fromMonth <= lastDataMonth) { noDataFrom = nextMonth(lastDataMonth); toMonth = lastDataMonth; }
  const from = rows.find(row => row.month === fromMonth)?.jobs ?? null;
  const to = rows.find(row => row.month === toMonth)?.jobs ?? null;
  if (fromMonth === toMonth) return { ok: false, reason: from === null ? 'no_data' : 'same_month', month: fromMonth, jobs: from, noDataFrom };
  if (from === null || to === null || from === 0) return { ok: false, reason: 'no_data' };
  return { ok: true, value: { fromMonth, toMonth, fromJobs: from, toJobs: to, changePct: (to - from) / from * 100, noDataFrom } };
}

export interface PeriodRow {
  key: string;
  kind: 'period' | 'gap';
  label: string;
  versionId: string | null;
  start: string;
  /** Exclusive end; null while running. */
  end: string | null;
  lastDay: string;
  days: number;
  ongoing: boolean;
  /** Applications with a date in the period. null when application dates were never fetched (not 0). */
  applications: number | null;
  /** Applications per day. null when the period has no days or application dates were never fetched. */
  perDay: number | null;
  /**
   * The period starts after the day the application counts were taken (asOf). Its applications are
   * not known yet (null), which is different from 0.
   */
  afterCounts: boolean;
  billing: { connected: false } | { connected: true; yen: number | null; prorated: boolean; missingAmount: boolean; entries: number; overlapping: boolean };
  market: MarketChangeResult;
}

function billingFor(entries: readonly BillingEntry[], start: string, endExclusive: string): PeriodRow['billing'] {
  if (!entries.length) return { connected: false };
  let yen = 0; let prorated = false; let missingAmount = false; let count = 0;
  const touching: BillingEntry[] = [];
  for (const entry of entries) {
    const entryEnd = addDays(entry.end, 1);
    const overlap = Math.min(dayNumber(entryEnd), dayNumber(endExclusive)) - Math.max(dayNumber(entry.start), dayNumber(start));
    if (overlap <= 0) continue;
    count += 1;
    touching.push(entry);
    const entryDays = daysBetween(entry.start, entryEnd);
    if (entry.amountYen === null) { missingAmount = true; continue; }
    if (overlap < entryDays) prorated = true;
    yen += entry.amountYen * overlap / entryDays;
  }
  // Billing periods that overlap each other are not added together (the amount is left blank).
  const overlapping = billingOverlaps(touching);
  return { connected: true, yen: count === 0 || overlapping || (missingAmount && yen === 0) ? null : Math.round(yen), prorated, missingAmount, entries: count, overlapping };
}

/**
 * The period comparison table: one row per version period (plus gaps between periods), with
 * applications per day so periods of different length can be read side by side.
 */
export function periodRows(job: JobCopyRecord, options: { asOf: string; billing?: readonly BillingEntry[] | undefined; market?: readonly MarketRow[] | null | undefined }): PeriodRow[] {
  const { asOf } = options;
  const billing = billingEntries(job, options.billing);
  const byDate = job.overallApplications?.byDate;
  const periods = buildPeriods(job, asOf);
  const rows: PeriodRow[] = [];
  const make = (key: string, kind: PeriodRow['kind'], label: string, versionId: string | null, start: string, end: string | null, days: number, ongoing: boolean): PeriodRow => {
    const endExclusive = end ?? addDays(asOf, 1);
    const lastDay = addDays(endExclusive, -1) < start ? start : addDays(endExclusive, -1);
    const afterCounts = start > asOf;
    const applications = byDate === undefined || afterCounts ? null : countApplications(byDate, start, endExclusive < start ? start : endExclusive);
    return { key, kind, label, versionId, start, end, lastDay, days, ongoing, applications, perDay: applications !== null && days > 0 ? applications / days : null, afterCounts,
      billing: billingFor(billing, start, endExclusive), market: marketChange(options.market ?? null, start, lastDay) };
  };
  periods.forEach((period, index) => {
    rows.push(make(period.versionId, 'period', period.label, period.versionId, period.start, period.end, period.days, period.ongoing));
    const next = periods[index + 1];
    if (next && period.end && period.end < next.start) {
      rows.push(make(`gap-${period.versionId}`, 'gap', '掲載が確認できない期間', null, period.end, next.start, daysBetween(period.end, next.start), false));
    }
  });
  return rows;
}

/** Applications with a date outside every period (before the first one, or in no row). */
export function applicationsOutsidePeriods(job: JobCopyRecord, rows: readonly PeriodRow[]): number {
  const dated = Object.values(job.overallApplications?.byDate ?? {}).reduce((sum, count) => sum + count, 0);
  return dated - rows.reduce((sum, row) => sum + (row.applications ?? 0), 0);
}

export function formatPerDay(value: number | null): string {
  return value === null ? '—' : `${value.toFixed(2)}件/日`;
}
/** Yen in the one form used across the screen (format.ts): 45000 → "4万5,000円". */
export function formatYen(value: number): string {
  return formatYenJa(value);
}
export function formatDay(date: string): string {
  return date.replaceAll('-', '/');
}
/** "2026-08" → "2026/08" (the same YYYY/MM form as formatDateJst). */
export function formatMonth(month: string): string {
  return month.replace('-', '/');
}
