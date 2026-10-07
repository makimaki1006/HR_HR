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
  return { total, missingDate, fetchedAt, distributions, byDate };
}
