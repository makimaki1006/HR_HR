/**
 * Applications of one job read live from HubSpot (/api/job-copy/live?listing=...), turned into the
 * record's overallApplications so the timeline, the period table and the overview can use the
 * application dates HubSpot returned.
 */
import type { ApplicantDimension, ApplicantDistribution } from './applicantCompositionModel';
import type { JobCopyRecord } from './data';

/** Source shown for a body read from HubSpot's 仕事内容 property (plain words on screen). */
export const HUBSPOT_BODY_SOURCE = 'HubSpotの仕事内容';

export interface LiveApplicationSummary {
  total: number;
  missing_date: number;
  by_date: Record<string, number>;
  dimensions: Record<string, Record<string, number>>;
  /** Applications HubSpot also links to another job, by application date. Absent when not checked. */
  multi_listing_by_date?: Record<string, number> | undefined;
  /** The same, for applications with no application date. */
  multi_listing_missing_date?: number | undefined;
}

/**
 * The applications linked to more than one job, checked against the dated counts. undefined when
 * the source did not check them; null when they do not fit the counts (nothing is guessed).
 */
export function multiListingFromSummary(summary: Pick<LiveApplicationSummary, 'multi_listing_by_date' | 'multi_listing_missing_date'>, byDate: Record<string, number>, missingDate: number): { byDate: Record<string, number>; missingDate: number } | null | undefined {
  if (summary.multi_listing_by_date === undefined && summary.multi_listing_missing_date === undefined) return undefined;
  const missing = wholeCount(summary.multi_listing_missing_date ?? 0);
  // The values come from JSON: check the shape at run time, whatever the type says.
  const dated: unknown = summary.multi_listing_by_date;
  if (missing === null || missing > missingDate || typeof dated !== 'object' || dated === null) return null;
  const result: Record<string, number> = {};
  for (const [date, raw] of Object.entries(dated as Record<string, unknown>)) {
    const amount = wholeCount(raw);
    if (!calendarDay(date) || amount === null || amount > (byDate[date] ?? 0)) return null;
    result[date] = amount;
  }
  return { byDate: result, missingDate: missing };
}

const DIMENSIONS: readonly ApplicantDimension[] = ['gender', 'age', 'prefecture', 'municipality'];
const wholeCount = (value: unknown): number | null => typeof value === 'number' && Number.isInteger(value) && value >= 0 ? value : null;
const calendarDay = (date: string) => /^\d{4}-\d{2}-\d{2}$/.test(date) && Number.isFinite(Date.parse(`${date}T00:00:00Z`)) && new Date(`${date}T00:00:00Z`).toISOString().slice(0, 10) === date;

/**
 * null when the summary cannot be trusted (a count is not a whole number, a date is not a calendar
 * day, or dated + undated applications do not add up to the total). Nothing is guessed.
 */
export function overallFromLiveSummary(summary: LiveApplicationSummary, fetchedAt: string): NonNullable<JobCopyRecord['overallApplications']> | null {
  const total = wholeCount(summary.total); const missingDate = wholeCount(summary.missing_date);
  if (total === null || missingDate === null) return null;
  const byDate: Record<string, number> = {};
  let dated = 0;
  for (const [date, raw] of Object.entries(summary.by_date)) {
    const amount = wholeCount(raw);
    if (!calendarDay(date) || amount === null) return null;
    byDate[date] = amount; dated += amount;
  }
  if (dated + missingDate !== total) return null;
  const multiListing = multiListingFromSummary(summary, byDate, missingDate);
  if (multiListing === null) return null;
  const distributions: Partial<Record<ApplicantDimension, ApplicantDistribution>> = {};
  for (const dimension of DIMENSIONS) {
    const buckets = summary.dimensions[dimension];
    if (!buckets) continue;
    const categories: ApplicantDistribution['categories'] = Object.entries(buckets).map(([category, raw]) => {
      const amount = wholeCount(raw) ?? 0;
      return { category, count: amount, percentage: total ? amount / total * 100 : null };
    });
    // Leave a dimension out rather than show shares that do not add up to the total.
    if (categories.reduce((sum, row) => sum + row.count, 0) === total) distributions[dimension] = { total, categories };
  }
  return { total, missingDate, fetchedAt, distributions, byDate, ...(multiListing ? { multiListing } : {}) };
}
