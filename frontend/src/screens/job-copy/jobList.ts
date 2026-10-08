import type { JobCopyRecord } from './data';

export type JobListOrder = 'source' | 'applications';

const validTotal = (job: JobCopyRecord): number | null => {
  const total = job.overallApplications?.total;
  return total !== undefined && Number.isSafeInteger(total) && total >= 0 ? total : null;
};

/** Use the acquired overall aggregate; versions may overlap or be unassigned. HubSpot records only. */
export function actualApplicationCount(job: JobCopyRecord): number | null {
  return job.dataSource === 'hubspot' ? validTotal(job) : null;
}

/**
 * The count the list shows: the same overall total the timeline, 応募分析 and 横断比較 use. Demo
 * (fictional) jobs show their fictional total, marked as such, so the list never says 応募未取得
 * next to a timeline that shows applications for the same job.
 */
export function listedApplicationCount(job: JobCopyRecord): number | null {
  return validTotal(job);
}

export function applicationCountLabel(job: JobCopyRecord): string {
  const total = listedApplicationCount(job);
  if (total === null) return '応募未取得';
  return job.dataSource === 'hubspot' ? `応募${String(total)}件` : `応募${String(total)}件（架空）`;
}

/** Call after filtering. Unknown counts follow known zero; ties retain source order. */
export function orderJobs(jobs: JobCopyRecord[], order: JobListOrder): JobCopyRecord[] {
  if (order === 'source') return [...jobs];
  return jobs.map((job, index) => ({ job, index, count: listedApplicationCount(job) ?? -1 }))
    .sort((left, right) => right.count - left.count || left.index - right.index)
    .map(row => row.job);
}
