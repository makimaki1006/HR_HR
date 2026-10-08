import type { CopyVersion, JobCopyRecord } from './data';
import { AREA_OTHER, AREA_UNKNOWN, municipalityLabel, parseApplicantArea, prefectureLabel, roundAreaDistribution } from './applicantArea';

export type ApplicantDimension = 'gender' | 'age' | 'prefecture' | 'municipality';
export interface ApplicantAttributes { gender: string | null; age: number | null; prefecture: string | null; municipality: string | null }
export interface DistributionCategory { category: string; count: number; percentage: number | null }
export interface ApplicantDistribution { total: number; categories: DistributionCategory[] }
export interface DistributionComparison { category: string; beforeCount: number; afterCount: number; beforePercentage: number | null; afterPercentage: number | null; deltaPp: number | null }

function supplied(value: string | null): string | null {
  const trimmed = value?.trim();
  return trimmed === '' ? null : trimmed ?? null;
}

function categoryOf(row: ApplicantAttributes, dimension: ApplicantDimension): string {
  if (dimension === 'gender') return supplied(row.gender) ?? '不明';
  // 住所は都道府県 + 市区町村までに丸める。元の文字列はラベルに使わない。
  if (dimension === 'prefecture') return prefectureLabel(parseApplicantArea(row.prefecture, row.municipality));
  if (dimension === 'municipality') return municipalityLabel(parseApplicantArea(row.prefecture, row.municipality));
  const age = row.age;
  if (age === null || !Number.isSafeInteger(age) || age < 0 || age > 120) return '不明';
  if (age < 20) return '19歳以下';
  if (age >= 60) return '60歳以上';
  return `${String(Math.floor(age / 10) * 10)}代`;
}

/** All attributed applications form the denominator, including missing attributes. */
export function buildDistribution(rows: ApplicantAttributes[] | null, dimension: ApplicantDimension): ApplicantDistribution | null {
  if (rows === null) return null;
  const counts = new Map<string, number>();
  for (const row of rows) {
    const category = categoryOf(row, dimension);
    counts.set(category, (counts.get(category) ?? 0) + 1);
  }
  const entries = [...counts];
  if (dimension === 'age') entries.sort(([left], [right]) => {
    const order = ['19歳以下', '20代', '30代', '40代', '50代', '60歳以上', '不明'];
    return order.indexOf(left) - order.indexOf(right);
  });
  const distribution = { total: rows.length, categories: entries.map(([category, count]) => ({ category, count, percentage: rows.length ? count / rows.length * 100 : null })) };
  return dimension === 'prefecture' || dimension === 'municipality' ? roundAreaDistribution(distribution, dimension) : distribution;
}

/** 地域の分布は表示の直前にも丸める（取り込み時に丸め済みでも結果は同じ）。 */
export function displayDistribution(distribution: ApplicantDistribution | null | undefined, dimension: ApplicantDimension): ApplicantDistribution | null {
  if (!distribution) return null;
  return dimension === 'prefecture' || dimension === 'municipality' ? roundAreaDistribution(distribution, dimension) : distribution;
}

/**
 * Puts a named area into 「その他」 on both sides when one side does not show it but has a
 * 「その他」: there it was merged in (fewer than 3 applicants), so its count is 1 or 2, not 0.
 * Comparing it as 0 would show a false −100 points. A side without 「その他」 really has 0.
 */
function alignSuppressedAreas(before: ApplicantDistribution, after: ApplicantDistribution): [ApplicantDistribution, ApplicantDistribution] {
  const hasOther = (side: ApplicantDistribution) => side.categories.some(item => item.category === AREA_OTHER && item.count > 0);
  const named = (side: ApplicantDistribution) => new Set(side.categories.map(item => item.category).filter(category => category !== AREA_OTHER && category !== AREA_UNKNOWN));
  const beforeNamed = named(before); const afterNamed = named(after);
  const hidden = new Set([
    ...(hasOther(after) ? [...beforeNamed].filter(category => !afterNamed.has(category)) : []),
    ...(hasOther(before) ? [...afterNamed].filter(category => !beforeNamed.has(category)) : []),
  ]);
  if (!hidden.size) return [before, after];
  const merge = (side: ApplicantDistribution): ApplicantDistribution => {
    const counts = new Map<string, number>();
    for (const item of side.categories) {
      const category = hidden.has(item.category) ? AREA_OTHER : item.category;
      counts.set(category, (counts.get(category) ?? 0) + item.count);
    }
    const rows = [...counts].map(([category, count]) => ({ category, count, percentage: side.total ? count / side.total * 100 : null }));
    const rank = (label: string) => label === AREA_OTHER ? 1 : label === AREA_UNKNOWN ? 2 : 0;
    return { total: side.total, categories: rows.sort((left, right) => rank(left.category) - rank(right.category)) };
  };
  return [merge(before), merge(after)];
}

/**
 * areas: the categories are areas (都道府県・市区町村), where 「その他」 holds the areas with fewer
 * than 3 applicants. For gender, 「その他」 is a real answer and is left alone.
 */
export function compareDistributions(before: ApplicantDistribution | null, after: ApplicantDistribution | null, options: { areas?: boolean } = {}): DistributionComparison[] | null {
  if (before === null || after === null) return null;
  const [left, right] = options.areas ? alignSuppressedAreas(before, after) : [before, after];
  const categories = [...new Set([...left.categories.map(item => item.category), ...right.categories.map(item => item.category)])];
  return categories.map(category => {
    const beforeCount = left.categories.find(item => item.category === category)?.count ?? 0;
    const afterCount = right.categories.find(item => item.category === category)?.count ?? 0;
    const beforePercentage = left.total ? beforeCount / left.total * 100 : null;
    const afterPercentage = right.total ? afterCount / right.total * 100 : null;
    return { category, beforeCount, afterCount, beforePercentage, afterPercentage, deltaPp: beforePercentage === null || afterPercentage === null ? null : afterPercentage - beforePercentage };
  });
}

// Explicit per-application synthetic attributes. These are never applied to
// imported jobs, received drafts, AI proposals, or other demo jobs.
const rowsByVersion: Record<string, ApplicantAttributes[] | undefined> = {
  'demo-001-v1': [
    ['男性', 24, '大分県', '大分市'], ['男性', 28, '大分県', '大分市'], ['女性', 31, '大分県', '別府市'],
    ['男性', 36, '大分県', '大分市'], ['女性', 42, '大分県', '別府市'], ['男性', 45, '福岡県', '福岡市'],
    ['男性', 51, '大分県', '大分市'], ['女性', 55, '大分県', '中津市'], ['男性', 61, '大分県', '大分市'],
    ['女性', 22, '福岡県', '福岡市'], [null, null, null, null], ['男性', 33, '大分県', null],
  ].map(row => ({ gender: row[0] as string | null, age: row[1] as number | null, prefecture: row[2] as string | null, municipality: row[3] as string | null })),
  'demo-001-v2': [
    ['女性', 23, '大分県', '大分市'], ['女性', 29, '大分県', '別府市'], ['男性', 34, '大分県', '大分市'],
    ['女性', 38, '大分県', '大分市'], ['女性', 43, '福岡県', '福岡市'], ['男性', 48, '大分県', '別府市'],
    ['その他', 52, '熊本県', '熊本市'], [null, null, null, null], ['男性', 26, '大分県', null],
  ].map(row => ({ gender: row[0] as string | null, age: row[1] as number | null, prefecture: row[2] as string | null, municipality: row[3] as string | null })),
  'demo-001-v3': [
    ['女性', 27, '大分県', '大分市'], ['女性', 41, '大分県', '別府市'], ['男性', 62, '福岡県', '福岡市'], [null, null, null, null],
  ].map(row => ({ gender: row[0] as string | null, age: row[1] as number | null, prefecture: row[2] as string | null, municipality: row[3] as string | null })),
};

export function compositionRows(job: JobCopyRecord, version: CopyVersion | undefined): ApplicantAttributes[] | null {
  if (job.id !== 'demo-job-001' || version?.kind !== 'published' || !version.applications) return null;
  const rows = rowsByVersion[version.id];
  return rows?.length === version.applications.confirmed ? rows : null;
}

export function compositionDistribution(job: JobCopyRecord, version: CopyVersion | undefined, dimension: ApplicantDimension): ApplicantDistribution | null {
  return displayDistribution(version?.distributions?.[dimension], dimension) ?? buildDistribution(compositionRows(job, version), dimension);
}
