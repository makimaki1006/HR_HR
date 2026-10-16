import { parseHrhFields, composeSalary } from './hrhCopy';
import { imagesByVersion } from './images';
import { publicationLabel } from './hubspotListings';
import type { CopyVersion, JobCopyRecord } from './data';
import { compositionDistribution, displayDistribution } from './applicantCompositionModel';
import type { ApplicantDimension } from './applicantCompositionModel';
import type { HrhPerformanceRow } from './hrhPerformanceModel';
import { performanceRatios } from './hrhPerformanceModel';

export type AbScope = 'record' | 'version';
export const publishedVariants = (job: JobCopyRecord) => job.versions.filter(version => version.kind === 'published');
export function variantCount(job: JobCopyRecord, version: CopyVersion | undefined, scope: AbScope): number | null {
  if (scope === 'record') return job.overallApplications?.total ?? null;
  if (version?.kind !== 'published' || !version.applications) return null;
  return version.applications.confirmed + version.applications.estimated;
}
export function variantDistribution(job: JobCopyRecord, version: CopyVersion | undefined, scope: AbScope, dimension: ApplicantDimension) {
  if (scope === 'version' && version?.kind !== 'published') return null;
  return scope === 'record' ? displayDistribution(job.overallApplications?.distributions[dimension], dimension) : compositionDistribution(job, version, dimension);
}
export type ComparisonPeriod = 'start' | 'recent';
export interface Period { start: string; end: string }
export const jstToday = () => new Date(Date.now() + 9 * 3_600_000).toISOString().slice(0, 10);
export function shiftDay(day: string, days: number) { return new Date(Date.parse(`${day}T00:00:00Z`) + days * 86_400_000).toISOString().slice(0, 10); }
/** 7日間の週の行だけを使う。月次・任意期間の実績を週とみなさない。 */
const weeklyRows = (job: JobCopyRecord) => (job.hrhPerformance?.rows ?? []).filter(row => shiftDay(row.period_start, 6) === row.period_end);
export function commonRecentPeriod(jobs: readonly JobCopyRecord[], today: string): Period | null {
  const first = jobs[0];
  if (!first) return null;
  const ends = weeklyRows(first).filter(row => row.period_end <= today).sort((a, b) => b.period_end.localeCompare(a.period_end));
  for (const row of ends) {
    const period = { start: shiftDay(row.period_end, -27), end: row.period_end };
    if (jobs.every(job => periodPerformance(job, period).covered)) return period;
  }
  return null;
}
export function comparisonPeriod(job: JobCopyRecord, mode: ComparisonPeriod, today: string, jobs: readonly JobCopyRecord[] = [job]): Period | null {
  if (mode === 'recent') return commonRecentPeriod(jobs, today);
  // 本文の確認日を掲載開始日の代わりには使わない。
  const start = job.listingStartedOn ?? (job.dataSource !== 'hubspot' ? publishedVariants(job).find(v => v.publishedFrom)?.publishedFrom?.slice(0, 10) : null);
  if (!start || !/^\d{4}-\d{2}-\d{2}$/.test(start) || !Number.isFinite(Date.parse(start)) || shiftDay(start, 0) !== start) return null;
  // 保存済みの週の区切りを使う。開始週の行が欠けても次の週へずらさない。
  const anchor = weeklyRows(job)[0]?.period_start;
  const days = anchor ? (Date.parse(start) - Date.parse(anchor)) / 86_400_000 : (new Date(`${start}T00:00:00Z`).getUTCDay() + 6) % 7;
  const weekStart = shiftDay(start, -((days % 7 + 7) % 7));
  return { start: weekStart, end: shiftDay(weekStart, 27) };
}
export function periodPerformance(job: JobCopyRecord, period: Period | null) {
  const rows = period ? weeklyRows(job).filter(row => row.period_start >= period.start && row.period_end <= period.end).sort((a, b) => a.period_start.localeCompare(b.period_start)) : [];
  const covered = Boolean(period && rows.length === 4 && shiftDay(period.start, 27) === period.end && rows.every((row, i) => row.period_start === shiftDay(period.start, i * 7)));
  const sum = (key: 'impressions' | 'clicks' | 'cost_yen' | 'applications') => covered && rows.every(row => row[key] !== null) ? rows.reduce((sum, row) => sum + (row[key] ?? 0), 0) : null;
  const total: HrhPerformanceRow = { period_start: period?.start ?? '', period_end: period?.end ?? '', impressions: sum('impressions'), clicks: sum('clicks'), cost_yen: sum('cost_yen'), applications: sum('applications') };
  return { total, ratios: performanceRatios(total), covered };
}
export function fourWeekLabel(period: Period | null): string {
  if (!period) return '未取得';
  const start = period.start.replaceAll('-', '/');
  const end = (period.start.slice(0, 4) === period.end.slice(0, 4) ? period.end.slice(5) : period.end).replaceAll('-', '/');
  return `${start}〜${end} の4週`;
}

export function comparisonAttributes(job: JobCopyRecord) {
  const version = publishedVariants(job).at(-1);
  const fields = job.currentListingFields ? new Map(Object.entries(job.currentListingFields)) : parseHrhFields(version?.comparisonBody ?? version?.body ?? '');
  const kind = job.listingSummary?.salary_kind ?? fields.get('給与形態') ?? '';
  const min = job.listingSummary?.salary_min ?? fields.get('基本給与 最小') ?? '';
  const max = job.listingSummary?.salary_max ?? fields.get('基本給与 最大') ?? '';
  const salary = fields.get('給与') ?? (kind || min || max ? composeSalary(kind, min, max) : version?.bodySections?.filter(section => section.heading === '給与').map(section => section.text).join('\n') ?? '');
  return {
    occupation: job.listingSummary?.occupation ?? fields.get('Indeed表示職種名') ?? fields.get('職種') ?? '',
    prefecture: job.listingPrefecture ?? (/^(東京都|北海道|(?:京都|大阪)府|.{2,3}県)/.exec(job.location)?.[0] ?? ''),
    salary,
    version,
    images: version?.images ?? (version ? imagesByVersion[version.id] : undefined),
  };
}
export function contentDifferences(jobs: JobCopyRecord[]) {
  const attrs = jobs.map(comparisonAttributes);
  const sections = [...new Set(jobs.flatMap(job => publishedVariants(job).at(-1)?.bodySections?.map(s => s.heading) ?? []))];
  const rows = [
    { key: 'title', label: '求人名', values: jobs.map(job => job.title.trim() ? job.title : null) },
    { key: 'salary', label: '給与', values: attrs.map(a => a.salary.trim() ? a.salary : null) },
    { key: 'images', label: '画像', values: attrs.map(a => a.images ? JSON.stringify(a.images.map(i => i.contentHash ?? i.url)) : null) },
    ...sections.filter(heading => heading !== '給与').map(heading => ({ key: `body-${heading}`, label: heading, values: attrs.map(a => a.version?.bodySections?.filter(s => s.heading === heading).map(s => s.text).join('\n') ?? null) })),
    ...(!sections.length || attrs.some(a => a.version?.body && !a.version.bodySections?.length) ? [{ key: 'body', label: '本文', values: attrs.map(a => a.version?.body ?? null) }] : []),
  ];
  return rows.map(row => ({ ...row, differences: row.values.map((v, i) => i > 0 && v !== null && row.values[0] != null && v !== row.values[0]), different: row.values.some((v, i) => i > 0 && v !== null && row.values[0] != null && v !== row.values[0]) }));
}
export function localCandidates(job: JobCopyRecord, records: JobCopyRecord[], tab: 'same' | 'other', filters: { occupation: string; prefecture: string; media: string; q: string }, today: string) {
  const sameAccount = (other: JobCopyRecord) => job.accountId ? other.accountId === job.accountId : other.company === job.company;
  const rank = (other: JobCopyRecord) => job.listingDealIds?.some(id => other.listingDealIds?.includes(id)) ? 0 : publicationLabel(other.listingPublicationStatus ?? null) === '掲載中' ? 1 : 2;
  return records.filter(other => other.id !== job.id && (tab === 'same' ? sameAccount(other) : !sameAccount(other) && (!filters.occupation || comparisonAttributes(other).occupation === filters.occupation) && (!filters.prefecture || comparisonAttributes(other).prefecture === filters.prefecture) && (!filters.media || other.media === filters.media) && other.title.normalize('NFKC').toLowerCase().includes(filters.q.normalize('NFKC').toLowerCase()))).sort((a, b) => tab === 'same' ? rank(a) - rank(b) || a.id.localeCompare(b.id) : (periodPerformance(b, comparisonPeriod(b, 'recent', today)).total.applications ?? -1) - (periodPerformance(a, comparisonPeriod(a, 'recent', today)).total.applications ?? -1) || a.id.localeCompare(b.id));
}
