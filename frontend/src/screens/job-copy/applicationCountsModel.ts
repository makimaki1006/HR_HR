import type { CopyVersion, JobCopyRecord } from './data';

/**
 * 応募件数の分母を 1 つにそろえる。本文・画像、応募者構成、2求人の比較はすべてここを使う。
 * 応募は HubSpot に記録されたものだけを数える。
 */

const publishedWithCounts = (job: JobCopyRecord) => job.versions.filter(version => version.kind === 'published' && version.applications !== null);

/** どの版への応募か分からない件数（求人全体で 1 つの値）。分からなければ null。 */
export function unmatchedApplicationCount(job: JobCopyRecord): number | null {
  if (job.attributionUnknown !== undefined) return job.attributionUnknown;
  // HubSpot の応募全体だけがあり、版との対応が取れていない求人は全件が対象
  if (job.overallApplications) return job.overallApplications.total;
  const versions = publishedWithCounts(job);
  return versions.length ? versions.reduce((sum, version) => sum + (version.applications?.unknown ?? 0), 0) : null;
}

/** 求人全体の応募件数。分からなければ null。 */
export function jobApplicationTotal(job: JobCopyRecord): number | null {
  if (job.overallApplications) return job.overallApplications.total;
  const versions = publishedWithCounts(job);
  return versions.length ? versions.reduce((sum, version) => sum + (version.applications ? version.applications.confirmed + version.applications.estimated + version.applications.unknown : 0), 0) : null;
}

/** この版に結びつく応募件数（確定 + 推定）。応募が未取得なら null。 */
export function linkedApplicationCount(version: CopyVersion | undefined): number | null {
  if (!version?.applications) return null;
  return version.applications.confirmed + version.applications.estimated;
}

/** 版に結びつく応募が 0 件のときの一文。 */
export function noLinkedApplicationsMessage(jobTotal: number | null): string {
  return jobTotal === null ? 'この版に結びつく応募はまだありません（求人全体の件数は未取得）' : `この版に結びつく応募はまだありません（求人全体では${String(jobTotal)}件）`;
}
