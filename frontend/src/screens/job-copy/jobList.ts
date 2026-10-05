import type { JobCopyRecord } from './data';

export type JobListOrder = 'source' | 'applications';

/** Use the acquired overall aggregate; versions may overlap or be unassigned. */
export function actualApplicationCount(job: JobCopyRecord): number | null {
  const total = job.overallApplications?.total;
  return job.dataSource === 'hubspot' && total !== undefined && Number.isSafeInteger(total) && total >= 0 ? total : null;
}

export function applicationCountLabel(job: JobCopyRecord): string {
  const total = actualApplicationCount(job);
  return total === null ? '応募未取得' : `応募${String(total)}件`;
}

/** Call after filtering. Unknown counts follow known zero; ties retain source order. */
export function orderJobs(jobs: JobCopyRecord[], order: JobListOrder): JobCopyRecord[] {
  if (order === 'source') return [...jobs];
  return jobs.map((job, index) => ({ job, index, count: actualApplicationCount(job) ?? -1 }))
    .sort((left, right) => right.count - left.count || left.index - right.index)
    .map(row => row.job);
}
