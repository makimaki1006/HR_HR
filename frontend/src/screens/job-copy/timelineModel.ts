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
import { imageChangeKind } from './images';
import type { ImageChangeKind } from './images';
import type { MarketRow } from './marketChartModel';
import type { BillingPeriod } from './billingTypes';
import { extractSalary, isSalaryLine, sameSalary } from './salaryExtract';
import { formatYen as formatYenJa } from './format';
import type { SalaryInfo } from './salaryExtract';
import { DUMMY_BILLING_ENABLED, dummyBillingEntries, isDemoJob, isDummyBilling, withoutRealDays } from './dummyBilling';

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
 * the amount was not given (never shown as 0). source 'dummy' is the made-up billing from
 * dummyBilling.ts: always labelled on screen and never added to a real amount.
 */
export interface BillingEntry {
  source: 'hrhacker' | 'csv' | 'dummy';
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
  /**
   * The amount is made up for the demo (the demo jobs' HRハッカー rows). Shown as
   * 「デモ用の架空の金額」 wherever it appears, including in print.
   */
  fictional?: boolean;
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
/**
 * True when an HRハッカー実績 row and a billing CSV row share at least one day. The screen says so
 * instead of choosing one of them.
 */
export function billingConflict(entries: readonly BillingEntry[]): boolean {
  const hrh = entries.filter(entry => entry.source === 'hrhacker');
  const csv = entries.filter(entry => entry.source === 'csv');
  return hrh.some(a => csv.some(b => a.start <= b.end && b.start <= a.end));
}

export interface BillingOptions {
  /** Day the dummy billing runs up to (default: the day the application counts were taken). */
  asOf?: string | undefined;
  /** Add the dummy billing for the days with no real billing (default DUMMY_BILLING_ENABLED). */
  dummy?: boolean | undefined;
}

/**
 * Real billing (HRハッカー実績 + billing CSV) and, for the days no real row covers, the dummy
 * billing. Use realBilling() / dummyBilling() to read them apart; never add the two together.
 */
export function billingEntries(job: JobCopyRecord, injected?: readonly BillingEntry[], options: BillingOptions = {}): BillingEntry[] {
  const real = realBillingEntries(job, injected);
  if (!(options.dummy ?? DUMMY_BILLING_ENABLED)) return real;
  const asOf = options.asOf ?? asOfDate(job);
  const first = buildPeriods(job, asOf)[0]?.start;
  if (!first) return real;
  const dummy = withoutRealDays(dummyBillingEntries(job.id, job.media, first, asOf), real);
  return [...real, ...dummy].sort((a, b) => a.start.localeCompare(b.start) || a.source.localeCompare(b.source));
}
export function realBilling(entries: readonly BillingEntry[]): BillingEntry[] {
  return entries.filter(entry => !isDummyBilling(entry));
}
export function dummyBilling(entries: readonly BillingEntry[]): BillingEntry[] {
  return entries.filter(entry => isDummyBilling(entry));
}

function realBillingEntries(job: JobCopyRecord, injected?: readonly BillingEntry[]): BillingEntry[] {
  const fromHrh: BillingEntry[] = (job.hrhPerformance?.rows ?? []).map(row => ({
    source: 'hrhacker', start: row.period_start, end: row.period_end, amountYen: row.cost_yen, taxIncluded: null,
    media: 'HRハッカー', mediaJobId: job.mediaJobId, impressions: row.impressions, clicks: row.clicks, mediaApplications: row.applications,
    ...(isDemoJob(job) ? { fictional: true } : {}),
  }));
  const csv = (injected ?? []).filter(entry => DATE.test(entry.start) && DATE.test(entry.end) && entry.start <= entry.end);
  // A billing CSV row never replaces an HRハッカー実績 row, even for the same period: which amount is
  // right is not known. Both stay, and billingConflict() marks the days they share, so no total is
  // made from either.
  return [...fromHrh, ...csv]
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
  /**
   * First day the version is known to be shown. 'captured': the day it was acquired.
   * 'published': the media publication day.
   */
  start: string;
  /**
   * Exclusive end day; null only for the latest version with media publication times (still
   * running). A version known only from acquisitions ends the day after its acquisition, or at the
   * next acquisition when nothing changed in between (the same content was seen again).
   */
  end: string | null;
  /** Days covered. A still-running period counts up to and including asOf. */
  days: number;
  ongoing: boolean;
  certainty: CopyVersion['certainty'];
  /** 'published': media publication times. 'captured': only the days the data was taken. */
  basis: 'published' | 'captured';
}

/**
 * Between two acquisitions of different (or not comparable) content the listing changed on some
 * day in between, and which content was shown on those days is not known. 'changed': a change was
 * found. 'unknown': whether it changed could not be checked (a body or the images could not be
 * compared).
 */
export interface UncertainSpan {
  kind: 'between' | 'unacquired';
  /** Acquisition day of the earlier version (取得日A). */
  from: string;
  /** Acquisition day of the later version (取得日B); null after the last acquisition. */
  to: string | null;
  /** First day of the span. */
  start: string;
  /** Exclusive end. */
  end: string;
  reason: 'changed' | 'unknown' | 'after_last';
  fromVersionId: string;
  toVersionId: string | null;
}

export type BoundaryStatus = 'changed' | 'same' | 'unknown';
/**
 * Whether the version differs from the previous one. 'same' only when the text and both image
 * comparisons say nothing changed; 'unknown' when something could not be compared.
 */
export function boundaryStatus(change: Pick<VersionChange, 'index' | 'salaryChanged' | 'otherBodyChanged' | 'bodyStatus' | 'imageChange'>): BoundaryStatus {
  if (change.index === 0) return 'same';
  if (changeKinds(change).length > 0) return 'changed';
  if (change.bodyStatus === 'unavailable') return 'unknown';
  return change.imageChange === 'same' ? 'same' : 'unknown';
}

function versionStart(version: CopyVersion): string {
  return jstDate(version.publishedFrom ?? version.observedAt) ?? '';
}

export function buildPeriods(job: JobCopyRecord, asOf: string): TimelinePeriod[] {
  const versions = publishedVersions(job);
  const changes = versionChanges(job);
  const starts = versions.map(versionStart);
  return versions.map((version, index) => {
    const start = starts[index] ?? '';
    const next = versions[index + 1];
    const nextStart = starts[index + 1] ?? null;
    const basis: TimelinePeriod['basis'] = version.publishedFrom ? 'published' : 'captured';
    let end: string | null;
    if (basis === 'published' && (!next || next.publishedFrom)) {
      // Media publication times: the version runs until it was taken down or the next one started.
      end = jstDate(version.publishedUntil) ?? nextStart;
      if (end !== null && nextStart !== null && end > nextStart) end = nextStart;
    } else {
      // Known only from acquisitions: the version was seen on its acquisition day. When the next
      // acquisition found the same content, it is taken to run until then; otherwise the days up to
      // the next acquisition are an UncertainSpan, and after the last acquisition nothing is known.
      const nextChange = changes[index + 1];
      const status = nextChange ? boundaryStatus(nextChange) : null;
      end = status === 'same' && nextStart !== null ? nextStart : addDays(start, 1);
      if (nextStart !== null && end > nextStart) end = nextStart;
    }
    if (end !== null && end < start) end = start;
    const ongoing = end === null;
    const days = ongoing ? Math.max(0, daysBetween(start, asOf) + 1) : daysBetween(start, end ?? start);
    return { versionId: version.id, label: version.label, start, end, days, ongoing, certainty: version.certainty, basis };
  });
}

/**
 * The days between acquisitions whose content is not known (between two versions that differ or
 * could not be compared), and the days after the last acquisition up to asOf.
 */
export function uncertainSpans(job: JobCopyRecord, asOf: string, periods: readonly TimelinePeriod[] = buildPeriods(job, asOf)): UncertainSpan[] {
  const changes = versionChanges(job);
  const spans: UncertainSpan[] = [];
  periods.forEach((period, index) => {
    const next = periods[index + 1];
    if (period.basis !== 'captured' || period.end === null) return;
    if (next) {
      if (period.end >= next.start) return;
      const change = changes[index + 1];
      const status = change ? boundaryStatus(change) : 'unknown';
      spans.push({ kind: 'between', from: period.start, to: next.start, start: period.end, end: next.start, reason: status === 'changed' ? 'changed' : 'unknown', fromVersionId: period.versionId, toVersionId: next.versionId });
    } else if (period.end <= asOf) {
      spans.push({ kind: 'unacquired', from: period.start, to: null, start: period.end, end: addDays(asOf, 1), reason: 'after_last', fromVersionId: period.versionId, toVersionId: null });
    }
  });
  return spans;
}

/** A period shorter than this gets no 「1日あたり」 (a rate from 1 or 2 days is not compared). */
export const MIN_RATE_DAYS = 7;

/** Days [start, end) whose applications are counted for a period's content. */
export interface CountRange { start: string; end: string }

/**
 * The days whose applications are counted for each period. Applications are dated by day only, and
 * a version known only from acquisitions was seen at one moment of its acquisition day: the change
 * to or from it may fall earlier or later on that same day. So an acquisition day counts for the
 * version only when the acquisitions on both sides of it found the same content. The first
 * acquisition day, and an acquisition day next to a change (or a change that could not be
 * checked), are left out of the version and counted with the days between the acquisitions (or
 * before the first one). Media publication times are exact and counted as they are.
 */
export function countRanges(job: JobCopyRecord, asOf: string, periods: readonly TimelinePeriod[] = buildPeriods(job, asOf)): CountRange[] {
  const changes = versionChanges(job);
  const versions = publishedVersions(job);
  return periods.map((period, index) => {
    const end = period.end ?? addDays(asOf, 1);
    const next = versions[index + 1];
    if (period.basis === 'published' && (!next || next.publishedFrom)) return { start: period.start, end };
    const same = (at: number) => { const change = changes[at]; return change !== undefined && boundaryStatus(change) === 'same'; };
    const knownFrom = period.basis === 'published' || (index > 0 && same(index) && periods[index - 1]?.end === period.start);
    const knownUntil = index + 1 < periods.length && same(index + 1) && period.end === periods[index + 1]?.start;
    const start = knownFrom ? period.start : addDays(period.start, 1);
    const last = knownUntil ? end : addDays(end, -1);
    return { start, end: last < start ? start : last };
  });
}

export type ImageChange = ImageChangeKind;
export interface VersionChange {
  versionId: string;
  label: string;
  /** Day the version starts (JST): its acquisition day, or its media publication day. */
  date: string;
  /** Day the previous version starts (取得日A); null for the first version. */
  previousDate: string | null;
  index: number;
  salary: SalaryInfo | null;
  /**
   * null for the first version. Otherwise true when the salary line text differs from the previous
   * version (even when both read as the same amount), false when it is the same.
   */
  salaryChanged: boolean | null;
  /**
   * Which way the pay moved when salaryChanged is true: 'up' / 'down' compare the lower bound (then
   * the upper bound) of the same kind of pay; 'other' when the kind differs (月給 → 時給) or an amount
   * is missing. null when the salary did not change or could not be compared.
   */
  salaryDirection: 'up' | 'down' | 'other' | null;
  bodyStatus: CopyComparisonStatus;
  bodyAdded: number;
  bodyRemoved: number;
  /** Lines other than the salary line were added or removed. */
  otherBodyChanged: boolean;
  imageChange: ImageChange;
}

function direction(before: SalaryInfo | null, after: SalaryInfo | null): 'up' | 'down' | 'other' {
  if (!before || !after) return 'other';
  if (before.kind !== after.kind || before.min === null || after.min === null) return 'other';
  if (after.min !== before.min) return after.min > before.min ? 'up' : 'down';
  if (before.max !== null && after.max !== null && after.max !== before.max) return after.max > before.max ? 'up' : 'down';
  return 'other';
}

/** The salary line as written, ignoring width and spacing. null when the body has no salary line. */
function salaryLineKey(info: SalaryInfo | null): string | null {
  return info ? info.raw.normalize('NFKC').replace(/\s+/gu, '') : null;
}

export function versionChanges(job: JobCopyRecord): VersionChange[] {
  const versions = publishedVersions(job);
  return versions.map((version, index) => {
    const previous = versions[index - 1];
    const salary = extractSalary(version.body);
    const previousSalary = previous ? extractSalary(previous.body) : null;
    // The salary line text decides whether the salary changed; the parsed amounts only decide the
    // direction. Two different lines that read as the same amount ("月給25万円" → "月給250,000円", or a
    // misread) are still a change ('other'), never "no change".
    const lineChanged = salaryLineKey(previousSalary) !== salaryLineKey(salary);
    const salaryChanged = !previous ? null : lineChanged;
    const result = compareCopy(previous?.body ?? null, version.body);
    const salaryDirection = !salaryChanged ? null
      : salary?.kind === '不明' || previousSalary?.kind === '不明' || sameSalary(previousSalary, salary) ? 'other'
        : direction(previousSalary, salary);
    const changed = result.lines.filter(line => line.kind !== 'same');
    return {
      versionId: version.id, label: version.label, date: versionStart(version), previousDate: previous ? versionStart(previous) : null, index, salary, salaryChanged, salaryDirection,
      bodyStatus: result.status,
      bodyAdded: previous ? changed.filter(line => line.kind === 'added').length : 0,
      bodyRemoved: previous ? changed.filter(line => line.kind === 'removed').length : 0,
      otherBodyChanged: Boolean(previous) && result.status === 'changed' && changed.some(line => !isSalaryLine(line.text)),
      imageChange: imageChangeKind(previous, version),
    };
  });
}

/** Kinds of change at a version, in lane order. 画像 covers 差し替え・並び順・中身. */
export function changeKinds(change: Pick<VersionChange, 'index' | 'salaryChanged' | 'otherBodyChanged' | 'imageChange'>): ('給与' | '本文' | '画像')[] {
  if (change.index === 0) return [];
  const kinds: ('給与' | '本文' | '画像')[] = [];
  if (change.salaryChanged) kinds.push('給与');
  if (change.otherBodyChanged) kinds.push('本文');
  if (change.imageChange === 'replaced' || change.imageChange === 'reordered' || change.imageChange === 'content') kinds.push('画像');
  return kinds;
}

/** Applications recorded on days in [start, endExclusive). */
export function countApplications(byDate: Record<string, number> | undefined, start: string, endExclusive: string): number {
  if (!byDate) return 0;
  let total = 0;
  for (const [date, count] of Object.entries(byDate)) if (date >= start && date < endExclusive) total += count;
  return total;
}

/**
 * The 本文 lane mark. The visible text and the spoken label come from the same status, so a
 * screen reader never hears "N行追加" for a mark that shows 改行のみ or 同じ. An empty or unread body
 * is 比べられない, never 追加0・削除0 (which would read as "nothing changed").
 */
export function bodyMark(change: Pick<VersionChange, 'index' | 'bodyStatus' | 'bodyAdded' | 'bodyRemoved'>): { text: string; spoken: string } {
  if (change.index === 0) return { text: '最初', spoken: '最初の版' };
  switch (change.bodyStatus) {
    case 'unchanged': return { text: '同じ', spoken: '前の版と同じ' };
    case 'format_only': return { text: '改行のみ', spoken: '改行だけが違います' };
    case 'unavailable': return { text: '比べられない', spoken: '本文が無いため、前の版と比べられません' };
    case 'initial': return { text: '最初', spoken: '最初の版' };
    case 'changed': return { text: `追加${String(change.bodyAdded)}・削除${String(change.bodyRemoved)}`, spoken: `${String(change.bodyAdded)}行追加・${String(change.bodyRemoved)}行削除` };
  }
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
  /** Last month with a market value at all, read from the data (it moves forward every month). */
  lastDataMonth: string | null;
  /**
   * Last month with a 市場求人数. The period table compares 求人数 only, so it ends here; when it
   * is earlier than lastDataMonth the lane says so.
   */
  lastJobsMonth: string | null;
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
  const lastJobsMonth = rows.filter(row => row.jobs !== null).map(row => row.month).sort().at(-1) ?? null;
  return { points, lastDataMonth, lastJobsMonth, noDataFrom: noDataFrom && noDataFrom < monthOf(range.start) ? monthOf(range.start) : noDataFrom };
}

export interface MarketChange {
  fromMonth: string; toMonth: string; fromJobs: number; toJobs: number; changePct: number;
  /** First month of the period with no market data (the period runs past the data); null otherwise. */
  noDataFrom: string | null;
}
/**
 * 'after_data': the whole period is after the last month with market data (the data is refreshed
 * monthly and has not reached the period yet). lastDataMonth says up to which month there is data.
 */
export type MarketChangeResult = { ok: true; value: MarketChange } | { ok: false; reason: 'not_selected' | 'no_data' | 'same_month' | 'after_data'; month?: string; jobs?: number | null; noDataFrom?: string | null; lastDataMonth?: string | null };

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
  if (lastDataMonth !== null && fromMonth > lastDataMonth) return { ok: false, reason: 'after_data', noDataFrom: nextMonth(lastDataMonth), lastDataMonth };
  if (lastDataMonth !== null && toMonth > lastDataMonth) { noDataFrom = nextMonth(lastDataMonth); toMonth = lastDataMonth; }
  const from = rows.find(row => row.month === fromMonth)?.jobs ?? null;
  const to = rows.find(row => row.month === toMonth)?.jobs ?? null;
  if (fromMonth === toMonth) return { ok: false, reason: from === null ? 'no_data' : 'same_month', month: fromMonth, jobs: from, noDataFrom };
  if (from === null || to === null || from === 0) return { ok: false, reason: 'no_data' };
  return { ok: true, value: { fromMonth, toMonth, fromJobs: from, toJobs: to, changePct: (to - from) / from * 100, noDataFrom } };
}

export interface PeriodRow {
  key: string;
  /**
   * 'period': days a version is known to be shown. 'between': days between two acquisitions whose
   * content is not known (取得日A〜取得日Bの間). 'unacquired': days after the last acquisition.
   * 'gap': days with no publication (media publication times only). Applications in rows other
   * than 'period' are counted separately and never put into a version's period.
   */
  kind: 'period' | 'gap' | 'between' | 'unacquired';
  label: string;
  /** How the row's days were found, in plain words (shown under the label). */
  detail: string;
  versionId: string | null;
  start: string;
  /** Exclusive end; null while running. The days whose applications the row counts. */
  end: string | null;
  lastDay: string;
  /** Days counted (0 for a version seen on one acquisition day next to changes on both sides). */
  days: number;
  ongoing: boolean;
  /**
   * Applications with a date in the row (applications linked to more than one job left out). null
   * when application dates were never fetched (not 0).
   */
  applications: number | null;
  /**
   * Applications per day. null for 'between' / 'unacquired' rows, for periods shorter than
   * MIN_RATE_DAYS (see shortPeriod), or when application dates were never fetched.
   */
  perDay: number | null;
  /** A version or gap row shorter than MIN_RATE_DAYS: its applications per day are not compared. */
  shortPeriod: boolean;
  /**
   * The period starts after the day the application counts were taken (asOf). Its applications are
   * not known yet (null), which is different from 0.
   */
  afterCounts: boolean;
  /** Real billing only (HRハッカー実績 and the billing CSV). Dummy amounts are never in it. */
  billing: { connected: false } | { connected: true; yen: number | null; prorated: boolean; missingAmount: boolean; entries: number; overlapping: boolean; conflict: boolean; fictional: boolean };
  /** The dummy billing covers days of this row. Its amounts are never added up in the table. */
  dummyBilling: boolean;
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
  // An HRハッカー実績 row and a billing CSV row for the same days is a conflict: neither is chosen.
  const conflict = billingConflict(touching);
  const overlapping = billingOverlaps(touching);
  return { connected: true, yen: count === 0 || overlapping || (missingAmount && yen === 0) ? null : Math.round(yen), prorated, missingAmount, entries: count, overlapping, conflict, fictional: touching.some(entry => entry.fictional === true) };
}

function touches(entries: readonly BillingEntry[], start: string, endExclusive: string): boolean {
  return entries.some(entry => entry.start < endExclusive && addDays(entry.end, 1) > start);
}

/** Applications linked to more than one job, by date (when the source says which ones). */
export function multiListingByDate(job: JobCopyRecord): Record<string, number> | null {
  return job.overallApplications?.multiListing?.byDate ?? null;
}
function applicationsIn(job: JobCopyRecord, start: string, endExclusive: string): number {
  const byDate = job.overallApplications?.byDate;
  return countApplications(byDate, start, endExclusive) - countApplications(multiListingByDate(job) ?? undefined, start, endExclusive);
}

/**
 * The period comparison table. Rows follow the acquisition days: a version's row covers the days
 * its content is known to be shown (countRanges), the days between two acquisitions with
 * different content (both acquisition days included) are a separate 「取得日A〜取得日Bの間」 row,
 * and the days after the last acquisition are 未取得. Rows never share a day. Applications per day
 * are only given for version rows of MIN_RATE_DAYS days or more.
 */
export function periodRows(job: JobCopyRecord, options: { asOf: string; billing?: readonly BillingEntry[] | undefined; market?: readonly MarketRow[] | null | undefined; dummyBilling?: boolean | undefined }): PeriodRow[] {
  const { asOf } = options;
  const all = billingEntries(job, options.billing, { asOf, dummy: options.dummyBilling });
  const billing = realBilling(all);
  const dummy = dummyBilling(all);
  const byDate = job.overallApplications?.byDate;
  const periods = buildPeriods(job, asOf);
  const ranges = countRanges(job, asOf, periods);
  const changes = versionChanges(job);
  const rows: PeriodRow[] = [];
  let cursor: string | null = null;
  const make = (key: string, kind: PeriodRow['kind'], label: string, detail: (start: string, lastDay: string, days: number) => string, versionId: string | null, proposedStart: string, end: string | null, ongoing: boolean, drawnStart: string): PeriodRow => {
    // Rows never share a day: a row starts no earlier than where the previous one ended.
    const start = cursor !== null && proposedStart < cursor ? cursor : proposedStart;
    const counted = end === null ? addDays(asOf, 1) : end < start ? start : end;
    cursor = counted;
    const days = Math.max(0, daysBetween(start, counted < start ? start : counted));
    const lastDay = addDays(counted, -1) < start ? start : addDays(counted, -1);
    const afterCounts = drawnStart > asOf;
    const applications = byDate === undefined || afterCounts ? null : applicationsIn(job, start, counted);
    const shortPeriod = (kind === 'period' || kind === 'gap') && days < MIN_RATE_DAYS;
    return { key, kind, label, detail: detail(start, lastDay, days), versionId, start, end: end === null ? null : counted, lastDay, days, ongoing, applications,
      perDay: (kind === 'period' || kind === 'gap') && applications !== null && !shortPeriod ? applications / days : null, shortPeriod, afterCounts,
      billing: billingFor(billing, start, counted), dummyBilling: touches(dummy, start, counted), market: marketChange(options.market ?? null, start, lastDay) };
  };
  periods.forEach((period, index) => {
    const range = ranges[index] ?? { start: period.start, end: period.end ?? addDays(asOf, 1) };
    if (period.basis === 'captured') {
      rows.push(make(period.versionId, 'period', `${formatDay(period.start)}に取得した内容`, (start, lastDay, days) => days === 0
        ? `${formatDay(period.start)}（取得した日。取得した時刻の前後で内容が変わった可能性があるため、この日の応募は別の行に数えます）`
        : `${formatDay(start)}〜${formatDay(lastDay)}（同じ内容を取得した日の間）`, period.versionId, range.start, range.end, false, period.start));
    } else {
      rows.push(make(period.versionId, 'period', period.label, start => `${formatDay(start)}〜${period.ongoing ? `継続中（${formatDay(asOf)}まで）` : formatDay(addDays(range.end, -1))}`, period.versionId, range.start, period.ongoing ? null : range.end, period.ongoing, period.start));
    }
    const next = periods[index + 1];
    const nextRange = ranges[index + 1];
    const from = cursor ?? range.end;
    if (next && nextRange) {
      if (from >= nextRange.start) return;
      if (period.basis === 'published' && next.basis === 'published') {
        rows.push(make(`gap-${period.versionId}`, 'gap', '掲載が確認できない期間', (start, lastDay) => `${formatDay(start)}〜${formatDay(lastDay)}`, null, from, nextRange.start, false, from));
        return;
      }
      const change = changes[index + 1];
      const status = change ? boundaryStatus(change) : 'unknown';
      rows.push(make(`between-${period.versionId}`, 'between', status === 'changed' ? `取得日${formatDay(period.start)}〜${formatDay(next.start)}の間に変化` : `取得日${formatDay(period.start)}〜${formatDay(next.start)}の間（変化したか確認できない）`,
        (start, lastDay) => {
          // Name the acquisition days the row holds (the first acquisition day is counted before the table instead).
          const held = [period.start, next.start].filter((day, at, days) => day >= start && day <= lastDay && days.indexOf(day) === at).map(formatDay);
          return `${formatDay(start)}〜${formatDay(lastDay)}（どちらの内容か分からない期間${held.length ? `。取得した日 ${held.join('・')} を含む` : ''}）`;
        }, null, from, nextRange.start, false, from));
    } else if (period.basis === 'captured' && !period.ongoing && from <= asOf) {
      rows.push(make(`unacquired-${period.versionId}`, 'unacquired', `最後の取得（${formatDay(period.start)}）より後`, (start, lastDay) => `${formatDay(start)}〜${formatDay(lastDay)}（${start <= period.start ? '最後に取得した日を含む。' : ''}未取得）`, null, from, addDays(asOf, 1), false, from));
    }
  });
  return rows;
}

/** Applications with a date outside every row (before the first one), leaving out multi-job ones. */
export function applicationsOutsidePeriods(job: JobCopyRecord, rows: readonly PeriodRow[]): number {
  const dated = Object.values(job.overallApplications?.byDate ?? {}).reduce((sum, count) => sum + count, 0);
  const multi = Object.values(multiListingByDate(job) ?? {}).reduce((sum, count) => sum + count, 0);
  return dated - multi - rows.reduce((sum, row) => sum + (row.applications ?? 0), 0);
}

/**
 * Applications that are in no version row of the period table: no application date, a date before
 * the first acquisition, a date in a 「取得日A〜Bの間」 or 未取得 row, or linked to more than one job.
 * null when applications were not fetched.
 */
export function applicationsOutsideVersions(job: JobCopyRecord, rows: readonly PeriodRow[]): number | null {
  const overall = job.overallApplications;
  if (!overall) return null;
  if (!overall.byDate) return overall.total;
  const inVersions = rows.filter(row => row.kind === 'period').reduce((sum, row) => sum + (row.applications ?? 0), 0);
  return overall.total - inVersions;
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
/**
 * 「市場データは2026年8月まで（毎月更新）」. The month is read from the market data itself, never
 * written into the code, because the Indeed data is refreshed every month.
 */
export function marketDataUntil(month: string): string {
  return `市場データは${formatMonthJa(month)}まで（毎月更新）`;
}
/**
 * "2026-08" → "2026年8月". Every sentence about where the market data ends uses this form (the
 * lane and the period table), so one month is never written two ways on the screen.
 */
export function formatMonthJa(month: string): string {
  return `${String(Number(month.slice(0, 4)))}年${String(Number(month.slice(5, 7)))}月`;
}
/** The last month with any market value in the rows; null when there is none. */
export function lastMarketMonth(rows: readonly MarketRow[]): string | null {
  return rows.filter(row => row.jobs !== null || row.viewers !== null).map(row => row.month).sort().at(-1) ?? null;
}
/** "2026-08" → "2026/08" (the same YYYY/MM form as formatDateJst). */
export function formatMonth(month: string): string {
  return month.replace('-', '/');
}
