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
export function compareMetricPeriods(a: HrhPerformanceRow, b: HrhPerformanceRow) {
  const days = (row: HrhPerformanceRow) => (Date.parse(row.period_end) - Date.parse(row.period_start)) / 86_400_000 + 1;
  const start = a.period_start > b.period_start ? a.period_start : b.period_start;
  const end = a.period_end < b.period_end ? a.period_end : b.period_end;
  const ratiosA = performanceRatios(a); const ratiosB = performanceRatios(b);
  return { daysA: days(a), daysB: days(b), overlapDays: start <= end ? (Date.parse(end) - Date.parse(start)) / 86_400_000 + 1 : 0,
    ctrDeltaPp: ratiosA.ctr === null || ratiosB.ctr === null ? null : ratiosB.ctr - ratiosA.ctr,
    cvrA: a.clicks !== null && a.clicks > 0 && a.applications !== null ? a.applications / a.clicks * 100 : null,
    cvrB: b.clicks !== null && b.clicks > 0 && b.applications !== null ? b.applications / b.clicks * 100 : null };
}
