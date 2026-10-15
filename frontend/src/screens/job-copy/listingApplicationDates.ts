/**
 * 求人ひとつの応募を、応募日ごとに HubSpot から読む（/api/job-copy/listings/{id}/application-dates）。
 * 応募が0件の求人は total 0・by_date 空で返る。これは「応募が無い」という実際の答えで、「分からない」ではない。
 * 読めなかった・形が合わなかったときだけ null を返す。
 */
import { apiGet } from '../../api/client';
import { overallFromLiveSummary } from './liveApplications';
import type { JobCopyRecord } from './data';

interface ApplicationDates { listing_id: string; total: number; missing_date: number; by_date: Record<string, number>; fetched_at: string }

export async function loadApplicationDates(id: string, signal?: AbortSignal): Promise<NonNullable<JobCopyRecord['overallApplications']> | null> {
  const result = await apiGet<ApplicationDates>(`/api/job-copy/listings/${encodeURIComponent(id)}/application-dates`, { timeoutMs: 120_000, ...(signal ? { signal } : {}) });
  if (!result.ok || result.data.listing_id !== id) return null;
  return overallFromLiveSummary({ total: result.data.total, missing_date: result.data.missing_date, by_date: result.data.by_date, dimensions: {} }, result.data.fetched_at);
}
