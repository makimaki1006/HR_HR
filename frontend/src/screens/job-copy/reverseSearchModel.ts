import type { JobCopyRecord } from './data';
import { roundJointDemographics } from './applicantArea';
export interface DemographicCell { gender: string; age: string; prefecture: string; municipality: string; count: number }
export interface JointDemographics { total: number; cells: DemographicCell[] }
export interface ReverseSearchQuery { gender: string; age: string; prefecture: string; municipality: string; minimum: number }
export interface ReverseSearchResult { job: JobCopyRecord; count: number; denominator: number; percentage: number | null }
export function parseJointDemographics(value: unknown, total: number): JointDemographics {
  const fail = (): never => { throw new Error('性別・年代・地域を組み合わせた応募の集計を確認できませんでした。'); };
  if (!value || typeof value !== 'object' || Array.isArray(value)) return fail();
  const raw = value as Record<string, unknown>;
  if (Object.keys(raw).length !== 2 || raw.total !== total || !Array.isArray(raw.cells) || raw.cells.length > 10_000) return fail();
  const seen = new Set<string>();
  const cells = raw.cells.map((value: unknown) => {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return fail();
    const row = value as Record<string, unknown>;
    if (Object.keys(row).length !== 5 || ['gender', 'age', 'prefecture', 'municipality'].some(key => typeof row[key] !== 'string' || !row[key].trim() || row[key].length > 200) || typeof row.count !== 'number' || !Number.isSafeInteger(row.count) || row.count <= 0) return fail();
    const key = JSON.stringify([row.gender, row.age, row.prefecture, row.municipality]);
    if (seen.has(key)) return fail();
    seen.add(key);
    return { gender: row.gender as string, age: row.age as string, prefecture: row.prefecture as string, municipality: row.municipality as string, count: row.count };
  });
  if (cells.reduce((sum, row) => sum + row.count, 0) !== total) return fail();
  // 地域は都道府県 + 市区町村までに丸め、3件未満の地域は「その他」にまとめる（合計は変えない）
  return roundJointDemographics({ total, cells });
}
/** Query verified joint cells, never multiply separate demographic marginals. */
export function reverseSearch(records: JobCopyRecord[], query: ReverseSearchQuery): ReverseSearchResult[] {
  if (!Number.isSafeInteger(query.minimum) || query.minimum < 1) return [];
  return records.flatMap(job => {
    if (!job.jointDemographics) return [];
    const joint = roundJointDemographics(job.jointDemographics);
    const count = joint.cells.filter(row => (['gender', 'age', 'prefecture', 'municipality'] as const).every(key => !query[key] || row[key] === query[key])).reduce((sum, row) => sum + row.count, 0);
    return count >= query.minimum ? [{ job, count, denominator: joint.total, percentage: joint.total ? count / joint.total * 100 : null }] : [];
  }).sort((left, right) => right.count - left.count || left.job.id.localeCompare(right.job.id));
}

const areaOrder = (value: string) => value === 'その他' ? 1 : value === '不明' ? 2 : 0;
/** 逆検索の選択肢。地域は丸めた値だけを出す（元の住所の文字列は選択肢に出さない）。 */
export function reverseSearchOptions(records: JobCopyRecord[], key: 'gender' | 'age' | 'prefecture' | 'municipality'): string[] {
  const values = new Set(records.flatMap(job => job.jointDemographics ? roundJointDemographics(job.jointDemographics).cells.map(cell => cell[key]) : []));
  return [...values].sort((left, right) => areaOrder(left) - areaOrder(right) || left.localeCompare(right, 'ja'));
}
