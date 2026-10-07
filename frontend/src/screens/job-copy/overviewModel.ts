/**
 * Cross-job overview: one row per job on a shared calendar. Shows the latest change and the
 * applications per day in the N days before and after it. Rows are sortable but never ranked as
 * winners; the numbers sit side by side and do not say why applications changed.
 */
import type { JobCopyRecord } from './data';
import {
  addDays, applicationBuckets, asOfDate, billingEntries, billingOverlaps, changeKinds, countApplications, daysBetween, versionChanges,
} from './timelineModel';
import type { ApplicationBucket, BillingEntry } from './timelineModel';

export const OVERVIEW_WINDOW_DAYS = 14;

export interface WindowRate {
  /** Days actually covered (the after-window stops at the day counts were taken). */
  days: number;
  applications: number;
  perDay: number | null;
}
export interface OverviewRow {
  jobId: string;
  title: string;
  company: string;
  media: string;
  /** Start day of the latest version that changed something; null with fewer than two versions. */
  lastChange: string | null;
  kinds: ('給与' | '本文' | '画像')[];
  /** Every version start after the first (change markers on the calendar). */
  changeDates: string[];
  before: WindowRate | null;
  after: WindowRate | null;
  /** Applications recorded in HubSpot with a date, by week, for the mini calendar. */
  weeks: ApplicationBucket[];
  /** null: no billing data connected (not zero). */
  billingYen: number | null;
  billingConnected: boolean;
  billingMissingAmount: boolean;
  /** Two billing periods share days; the total is left blank instead of adding them up. */
  billingOverlapping: boolean;
  /** No application data at all (different from zero applications). */
  applicationsAvailable: boolean;
  asOf: string;
}

function rate(job: JobCopyRecord, start: string, endExclusive: string): WindowRate {
  const days = Math.max(0, daysBetween(start, endExclusive));
  const applications = countApplications(job.overallApplications?.byDate, start, endExclusive);
  return { days, applications, perDay: days > 0 ? applications / days : null };
}

export function overviewRow(job: JobCopyRecord, options: { billing?: readonly BillingEntry[] | undefined; now?: Date | undefined; windowDays?: number | undefined } = {}): OverviewRow {
  const windowDays = options.windowDays ?? OVERVIEW_WINDOW_DAYS;
  const asOf = asOfDate(job, options.now);
  const changes = versionChanges(job);
  const later = changes.slice(1);
  const latest = [...later].reverse().find(change => changeKinds(change).length > 0) ?? later.at(-1) ?? null;
  const applicationsAvailable = job.overallApplications?.byDate !== undefined;
  let before: WindowRate | null = null; let after: WindowRate | null = null;
  if (latest && applicationsAvailable) {
    before = rate(job, addDays(latest.date, -windowDays), latest.date);
    const afterEnd = addDays(latest.date, windowDays);
    const cappedEnd = afterEnd > addDays(asOf, 1) ? addDays(asOf, 1) : afterEnd;
    after = rate(job, latest.date, cappedEnd);
  }
  const billing = billingEntries(job, options.billing);
  const known = billing.filter(entry => entry.amountYen !== null);
  return {
    jobId: job.id, title: job.title, company: job.company, media: job.media,
    lastChange: latest?.date ?? null, kinds: latest ? changeKinds(latest) : [], changeDates: later.map(change => change.date),
    before, after, weeks: applicationBuckets(job.overallApplications?.byDate, 'week'),
    billingYen: known.length && !billingOverlaps(billing) ? known.reduce((sum, entry) => sum + (entry.amountYen ?? 0), 0) : null,
    billingConnected: billing.length > 0, billingMissingAmount: billing.length > known.length, billingOverlapping: billingOverlaps(billing),
    applicationsAvailable, asOf,
  };
}

export function overviewRows(jobs: readonly JobCopyRecord[], options: { billing?: Readonly<Record<string, readonly BillingEntry[]>> | undefined; now?: Date | undefined; windowDays?: number | undefined } = {}): OverviewRow[] {
  return jobs.map(job => overviewRow(job, { billing: options.billing?.[job.id], now: options.now, windowDays: options.windowDays }));
}

export type OverviewSort = 'source' | 'lastChange' | 'afterPerDay' | 'beforePerDay' | 'billing';
/** Missing values always go last; ties keep the original order. */
export function sortOverview(rows: readonly OverviewRow[], sort: OverviewSort): OverviewRow[] {
  if (sort === 'source') return [...rows];
  const value = (row: OverviewRow): number | string | null => sort === 'lastChange' ? row.lastChange
    : sort === 'afterPerDay' ? row.after?.perDay ?? null : sort === 'beforePerDay' ? row.before?.perDay ?? null : row.billingYen;
  return rows.map((row, index) => ({ row, index, value: value(row) }))
    .sort((a, b) => {
      if (a.value === null || b.value === null) return a.value === b.value ? a.index - b.index : a.value === null ? 1 : -1;
      if (a.value === b.value) return a.index - b.index;
      return a.value < b.value ? 1 : -1;
    })
    .map(item => item.row);
}

/** Shared calendar for the mini charts: earliest version / application day to the latest asOf. */
export function overviewRange(jobs: readonly JobCopyRecord[], rows: readonly OverviewRow[]): { start: string; end: string } | null {
  const days: string[] = [];
  for (const job of jobs) {
    for (const change of versionChanges(job)) if (change.date) days.push(change.date);
    for (const date of Object.keys(job.overallApplications?.byDate ?? {})) days.push(date);
  }
  for (const row of rows) if (row.applicationsAvailable) days.push(row.asOf);
  if (!days.length) return null;
  const sorted = days.filter(day => /^\d{4}-\d{2}-\d{2}$/.test(day)).sort();
  const start = sorted[0]; const end = sorted.at(-1);
  return start && end ? { start, end } : null;
}
