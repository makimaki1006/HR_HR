import type { ApplicationRow, JobCopyRecord } from './data';
import { asOfDate, buildPeriods, countRanges } from './timelineModel';

export const MOTIVE = 'oubodouki_1';
export const PRIORITIES = ['kinmujoukennoyuusenjuni_1', 'kinmujoukennoyuusenjuni_2', 'kinmujoukennoyuusenjuni_3'] as const;
export const selectionNote = '選択式の記録は 2026/09/18 の応募からです';

/** 同じ応募・同じ選択肢を1回だけ数える。文章と旧カテゴリは参照しない。 */
export function selectedCounts(rows: readonly ApplicationRow[], property: string) {
  const unique = [...new Map(rows.map(row => [row.key, row])).values()];
  const counts = new Map<string, number>();
  let answered = 0;
  for (const row of unique) {
    const values = [...new Set(row.selections?.[property] ?? [])].filter(value => value.trim() && !['unset', '未設定'].includes(value.trim().toLowerCase()));
    if (values.length) answered += 1;
    for (const value of values) counts.set(value, (counts.get(value) ?? 0) + 1);
  }
  return { answered, counts };
}

export function versionApplications(job: JobCopyRecord, versionId: string): ApplicationRow[] {
  const asOf = asOfDate(job);
  const periods = buildPeriods(job, asOf);
  const index = periods.findIndex(period => period.versionId === versionId);
  const range = countRanges(job, asOf, periods)[index];
  return range ? (job.applicationRows ?? []).filter(row => row.date !== null && row.date >= range.start && row.date < range.end) : [];
}

/** 変更前後・期間ごとの理由は、応募件数と同じく複数求人への応募を除く。 */
export function reasonApplications(job: JobCopyRecord): ApplicationRow[] {
  const multi = new Set(job.applicantReasons?.multiListingApplicants ?? []);
  return [...new Map((job.applicationRows ?? []).filter(row => !multi.has(row.key)).map(row => [row.key, row])).values()];
}

export function hasSelectedReason(row: ApplicationRow): boolean {
  return [MOTIVE, ...PRIORITIES].some(property => selectedCounts([row], property).answered > 0);
}
