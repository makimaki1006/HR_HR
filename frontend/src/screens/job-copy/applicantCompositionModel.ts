import type { CopyVersion, JobCopyRecord } from './data';

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
  const prefecture = supplied(row.prefecture);
  if (dimension === 'gender') return supplied(row.gender) ?? '不明';
  if (dimension === 'prefecture') return prefecture ?? '不明';
  if (dimension === 'municipality') {
    const municipality = supplied(row.municipality);
    return prefecture || municipality ? `${prefecture ?? '都道府県不明'} / ${municipality ?? '市区町村不明'}` : '不明';
  }
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
  return { total: rows.length, categories: entries.map(([category, count]) => ({ category, count, percentage: rows.length ? count / rows.length * 100 : null })) };
}

export function compareDistributions(before: ApplicantDistribution | null, after: ApplicantDistribution | null): DistributionComparison[] | null {
  if (before === null || after === null) return null;
  const categories = [...new Set([...before.categories.map(item => item.category), ...after.categories.map(item => item.category)])];
  return categories.map(category => {
    const beforeCount = before.categories.find(item => item.category === category)?.count ?? 0;
    const afterCount = after.categories.find(item => item.category === category)?.count ?? 0;
    const beforePercentage = before.total ? beforeCount / before.total * 100 : null;
    const afterPercentage = after.total ? afterCount / after.total * 100 : null;
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
  return version?.distributions?.[dimension] ?? buildDistribution(compositionRows(job, version), dimension);
}
