import type { JobCopyRecord } from './data';
import type { ApplicantDimension, ApplicantDistribution } from './applicantCompositionModel';
import { parseMediaCapture } from './mediaCaptureParser';
import { parseApplicantReasons } from './applicantReasonsParser';
import { parseHrhPerformance } from './hrhPerformanceModel';
import { parseJointDemographics } from './reverseSearchModel';

const dimensions: ApplicantDimension[] = ['gender', 'age', 'prefecture', 'municipality'];
const invalid = (): never => { throw new Error('実データMOCの集計と求人の対応を確認できませんでした。'); };
function object(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return invalid();
  return value as Record<string, unknown>;
}
function count(value: unknown): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) return invalid();
  return value;
}
function id(value: unknown): string {
  if (typeof value !== 'string' || !/^\d{1,30}$/.test(value)) return invalid();
  return value;
}
function timestamp(value: unknown): string {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}T/.test(value) || !Number.isFinite(Date.parse(value))) return invalid();
  return value;
}
function totals(value: unknown, total: number): ApplicantDistribution {
  const categories = Object.entries(object(value)).map(([category, amount]) => ({ category, count: count(amount), percentage: total ? count(amount) / total * 100 : null }));
  if (categories.reduce((sum, row) => sum + row.count, 0) !== total) return invalid();
  return { total, categories };
}
function versionDistribution(value: unknown, total: number): ApplicantDistribution | undefined {
  if (value === null || value === undefined) return undefined;
  const distribution = object(value);
  if (count(distribution.denominator) !== total || !Array.isArray(distribution.categories)) return invalid();
  const seen = new Set<string>();
  const categories = distribution.categories.map(raw => {
    const row = object(raw);
    if (typeof row.category !== 'string' || seen.has(row.category)) return invalid();
    seen.add(row.category);
    const amount = count(row.count);
    const percentage = total ? amount / total * 100 : null;
    if (row.percentage !== null && (typeof row.percentage !== 'number' || percentage === null || Math.abs(row.percentage - percentage) > 0.11)) return invalid();
    return { category: row.category, count: amount, percentage };
  });
  if (categories.reduce((sum, row) => sum + row.count, 0) !== total) return invalid();
  return { total, categories };
}

/** Captured server aggregates only. No applicant records or synthesized attribution. */
export function parseRealMoc(text: string): JobCopyRecord[] {
  const payload = object(JSON.parse(text) as unknown);
  if (payload.schemaVersion !== 1 || !Array.isArray(payload.results)) return invalid();
  const fetchedAt = timestamp(payload.capturedAt);
  const bundle = object(payload.capture_bundle);
  if (!Array.isArray(bundle.jobs)) return invalid();
  const records = parseMediaCapture(JSON.stringify(bundle));
  const listings = bundle.jobs.map(value => id(object(value).hubspotListingId));
  if (new Set(listings).size !== listings.length) return invalid();
  const results = new Map<string, Record<string, unknown>>();
  for (const raw of payload.results) {
    const result = object(raw);
    const listing = id(result.listing_id);
    if (results.has(listing) || !listings.includes(listing)) return invalid();
    results.set(listing, result);
  }
  if (results.size !== records.length) return invalid();
  return records.map((job, index) => {
    const listing = listings[index];
    if (listing === undefined) return invalid();
    const result = results.get(listing);
    if (result === undefined) return invalid();
    const summary = object(result.summary);
    const total = count(summary.total);
    const missingDate = count(summary.missing_date);
    const dated = object(summary.by_date);
    if (missingDate + Object.values(dated).reduce<number>((sum, amount) => sum + count(amount), 0) !== total) return invalid();
    const summaryDimensions = object(summary.dimensions);
    const distributions: Partial<Record<ApplicantDimension, ApplicantDistribution>> = {};
    for (const dimension of dimensions) if (summaryDimensions[dimension] !== null && summaryDimensions[dimension] !== undefined) distributions[dimension] = totals(summaryDimensions[dimension], total);
    const comparison = result.dated_comparison === null || result.dated_comparison === undefined ? undefined : object(result.dated_comparison);
    const unknown = comparison ? count(comparison.unknown) : total;
    const buckets = comparison ? object(comparison.by_version) : {};
    if (comparison && (count(comparison.total) !== total || unknown + Object.values(buckets).reduce<number>((sum, raw) => sum + count(object(raw).count), 0) !== total)) return invalid();
    if (Object.keys(buckets).some(versionId => !job.versions.some(version => version.id === versionId))) return invalid();
    const representatives = comparison?.daily_representatives === undefined ? {} : object(comparison.daily_representatives);
    for (const day of Object.values(representatives)) if (typeof object(day).version_id !== 'string' || !job.versions.some(version => version.id === object(day).version_id)) return invalid();
    return { ...job, hubspotId: listing, dataSource: 'hubspot', attributionUnknown: unknown,
      applicantReasons: parseApplicantReasons(result.applicant_reasons, total, job.versions.filter(version => version.kind === 'published').map(version => version.id)),
      hrhPerformance: result.hrh_performance == null ? undefined : parseHrhPerformance(result.hrh_performance, job.mediaJobId),
      jointDemographics: summary.joint_demographics == null ? undefined : parseJointDemographics(summary.joint_demographics, total),
      overallApplications: { total, missingDate, fetchedAt, distributions },
      versions: job.versions.map(version => {
        if (buckets[version.id] === undefined) return version;
        const bucket = object(buckets[version.id]);
        const bucketCount = count(bucket.count);
        const bucketDimensions = object(bucket.dimensions);
        const versionDistributions: Partial<Record<ApplicantDimension, ApplicantDistribution>> = {};
        for (const dimension of dimensions) {
          const distribution = versionDistribution(bucketDimensions[dimension], bucketCount);
          if (distribution) versionDistributions[dimension] = distribution;
        }
        return { ...version, certainty: 'estimated', applications: { confirmed: 0, estimated: bucketCount, unknown: 0 },
          attributesFetchedAt: fetchedAt, distributions: versionDistributions,
          observationDates: Object.entries(representatives).filter(([, day]) => object(day).version_id === version.id).map(([day]) => day).sort(),
          note: `${version.note.replace('source filename acquisition label; not publication timestamp', 'ファイル取得日時による観測ラベル（掲載変更日時ではありません）')} ${typeof comparison?.basis === 'string' ? comparison.basis : '日付による観測対応'}` };
      }),
    };
  });
}
