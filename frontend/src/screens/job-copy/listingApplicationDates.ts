/**
 * 求人ひとつの応募の属性・理由・電話で聞いた内容を一緒に HubSpot から読む（/api/job-copy/listings/{id}/applicants）。
 * 応募が0件の求人は total 0・by_date 空で返る。これは「応募が無い」という実際の答えで、「分からない」ではない。
 * 読めなかった・形が合わなかったときだけ null を返す。
 */
import { isPhoneHearing } from './phoneHearingResponse';
import { apiGet } from '../../api/client';
import { parseApplicantReasons } from './applicantReasonsParser';
import { overallFromLiveSummary } from './liveApplications';
import { buildDistribution } from './applicantCompositionModel';
import { roundJointDemographics } from './applicantArea';
import type { ApplicationRow, JobCopyRecord, SelectionOptions } from './data';

interface ApplicationDates { selection_options?: SelectionOptions | null; created_date_count?: number; listing_id: string; total: number; missing_date: number; by_date: Record<string, number>; fetched_at: string; applications?: ApplicationRow[]; applicant_reasons?: unknown; phone_hearing?: unknown }

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
    const selections = row.selections && typeof row.selections === 'object' ? Object.fromEntries(Object.entries(row.selections).filter(([, values]) => Array.isArray(values) && values.every(value => typeof value === 'string')).map(([property, values]) => [property, [...new Set(values as string[])]])) : undefined;
    return [{ ...(selections ? { selections } : {}), key, media: text(row.media), date: date && /^\d{4}-\d{2}-\d{2}$/.test(date) ? date : null, gender: text(row.gender), age: text(row.age), prefecture: text(row.prefecture), municipality: text(row.municipality) }];
  });
}

export async function loadApplicationDates(id: string, signal?: AbortSignal): Promise<LoadedApplications | null> {
  const result = await apiGet<ApplicationDates>(`/api/job-copy/listings/${encodeURIComponent(id)}/applicants`, { timeoutMs: APPLICANTS_TIMEOUT_MS, ...(signal ? { signal } : {}) });
  if (!result.ok || result.data.listing_id !== id) return null;
  const overall = overallFromLiveSummary({ total: result.data.total, missing_date: result.data.missing_date, created_date_count: result.data.created_date_count, by_date: result.data.by_date, dimensions: {} }, result.data.fetched_at);
  if (!overall) return null;
  let reasons: JobCopyRecord['applicantReasons'];
  try { reasons = parseApplicantReasons(result.data.applicant_reasons, result.data.total, []); } catch { return null; }
  const rows = applicationRows(result.data.applications);
  overall.selectionOptions = rows.length === overall.total ? result.data.selection_options ?? null : null;
  if (rows.length === overall.total) {
    const attributes = rows.map(row => ({ ...row, age: row.age ? Number(row.age) : null }));
    for (const dimension of ['gender', 'age', 'prefecture', 'municipality'] as const) {
      const distribution = buildDistribution(attributes, dimension);
      if (distribution) overall.distributions[dimension] = distribution;
    }
    const protectedAreas = roundJointDemographics({ total: rows.length, cells: attributes.map(row => ({
      gender: row.gender ?? '不明', age: buildDistribution([row], 'age')?.categories[0]?.category ?? '不明',
      prefecture: row.prefecture ?? '不明', municipality: row.municipality ?? '不明', count: 1,
    })) });
    for (const dimension of ['prefecture', 'municipality'] as const) {
      const counts = new Map<string, number>();
      for (const cell of protectedAreas.cells) counts.set(cell[dimension], (counts.get(cell[dimension]) ?? 0) + cell.count);
      overall.distributions[dimension] = { total: rows.length, categories: [...counts].map(([category, count]) => ({ category, count, percentage: rows.length ? count / rows.length * 100 : null })) };
    }
  }
  return { overall, rows, reasons, phoneHearing: isPhoneHearing(result.data.phone_hearing) ? result.data.phone_hearing : undefined };
}
