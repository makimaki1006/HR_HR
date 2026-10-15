/**
 * 求人ひとつの応募を、応募日ごとに HubSpot から読む（/api/job-copy/listings/{id}/application-dates）。
 * 応募が0件の求人は total 0・by_date 空で返る。これは「応募が無い」という実際の答えで、「分からない」ではない。
 * 読めなかった・形が合わなかったときだけ null を返す。
 */
import { apiGet } from '../../api/client';
import { parseApplicantReasons } from './applicantReasonsParser';
import { overallFromLiveSummary } from './liveApplications';
import type { ApplicationRow, JobCopyRecord } from './data';

interface ApplicationDates { listing_id: string; total: number; missing_date: number; by_date: Record<string, number>; fetched_at: string; applications?: ApplicationRow[]; applicant_reasons?: unknown }

export interface LoadedApplications {
  overall: NonNullable<JobCopyRecord['overallApplications']>;
  rows: ApplicationRow[];
  reasons: JobCopyRecord['applicantReasons'];
}

const text = (value: unknown): string | null => typeof value === 'string' && value.trim() ? value : null;

export function applicationRows(raw: unknown): ApplicationRow[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((item: unknown) => {
    if (typeof item !== 'object' || item === null) return [];
    const row = item as Record<string, unknown>;
    const key = text(row.key);
    if (!key) return [];
    const date = text(row.date);
    return [{ key, date: date && /^\d{4}-\d{2}-\d{2}$/.test(date) ? date : null, gender: text(row.gender), age: text(row.age), prefecture: text(row.prefecture), municipality: text(row.municipality) }];
  });
}

export async function loadApplicationDates(id: string, signal?: AbortSignal): Promise<LoadedApplications | null> {
  const result = await apiGet<ApplicationDates>(`/api/job-copy/listings/${encodeURIComponent(id)}/application-dates`, { timeoutMs: 120_000, ...(signal ? { signal } : {}) });
  if (!result.ok || result.data.listing_id !== id) return null;
  const overall = overallFromLiveSummary({ total: result.data.total, missing_date: result.data.missing_date, by_date: result.data.by_date, dimensions: {} }, result.data.fetched_at);
  if (!overall) return null;
  let reasons: JobCopyRecord['applicantReasons'];
  try { reasons = parseApplicantReasons(result.data.applicant_reasons, result.data.total, []); } catch { reasons = undefined; }
  return { overall, rows: applicationRows(result.data.applications), reasons };
}
