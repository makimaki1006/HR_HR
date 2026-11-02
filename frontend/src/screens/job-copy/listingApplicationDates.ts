/**
 * 求人ひとつの応募の属性・理由・電話で聞いた内容を一緒に HubSpot から読む（/api/job-copy/listings/{id}/applicants）。
 * 応募が0件の求人は total 0・by_date 空で返る。これは「応募が無い」という実際の答えで、「分からない」ではない。
 * 読めなかった・形が合わなかったときだけ null を返す。
 */
import { isPhoneHearing } from './phoneHearingResponse';
import { apiGet } from '../../api/client';
import { parseApplicantReasons } from './applicantReasonsParser';
import { overallFromLiveSummary } from './liveApplications';
import type { ApplicationRow, JobCopyRecord } from './data';

interface ApplicationDates { listing_id: string; total: number; missing_date: number; by_date: Record<string, number>; fetched_at: string; applications?: ApplicationRow[]; applicant_reasons?: unknown; phone_hearing?: unknown }

export interface LoadedApplications {
  overall: NonNullable<JobCopyRecord['overallApplications']>;
  rows: ApplicationRow[];
  phoneHearing?: JobCopyRecord['phoneHearing'];
  reasons: JobCopyRecord['applicantReasons'];
}

/** サーバーの締め切り（40 秒）より少し長い。これを過ぎたら「読み込めませんでした」にする。 */
export const APPLICANTS_TIMEOUT_MS = 45_000;
/** 画面の「もう一度読み込む」から、開いている求人の応募を読み直させる合図。 */
export const RETRY_APPLICANTS_EVENT = 'jobcopy:retry-applicants';
export function requestApplicantsRetry(id: string): void {
  window.dispatchEvent(new CustomEvent<string>(RETRY_APPLICANTS_EVENT, { detail: id }));
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
    return [{ key, media: text(row.media), date: date && /^\d{4}-\d{2}-\d{2}$/.test(date) ? date : null, gender: text(row.gender), age: text(row.age), prefecture: text(row.prefecture), municipality: text(row.municipality) }];
  });
}

export async function loadApplicationDates(id: string, signal?: AbortSignal): Promise<LoadedApplications | null> {
  const result = await apiGet<ApplicationDates>(`/api/job-copy/listings/${encodeURIComponent(id)}/applicants`, { timeoutMs: APPLICANTS_TIMEOUT_MS, ...(signal ? { signal } : {}) });
  if (!result.ok || result.data.listing_id !== id) return null;
  const overall = overallFromLiveSummary({ total: result.data.total, missing_date: result.data.missing_date, by_date: result.data.by_date, dimensions: {} }, result.data.fetched_at);
  if (!overall) return null;
  let reasons: JobCopyRecord['applicantReasons'];
  try { reasons = parseApplicantReasons(result.data.applicant_reasons, result.data.total, []); } catch { return null; }
  return { overall, rows: applicationRows(result.data.applications), reasons, phoneHearing: isPhoneHearing(result.data.phone_hearing) ? result.data.phone_hearing : undefined };
}
