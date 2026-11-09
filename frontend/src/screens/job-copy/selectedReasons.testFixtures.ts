import type { ApplicationRow, JobCopyRecord, SelectionOptions } from './data';
import { APPLICATION_TEXT_SOURCES } from './applicantReasonsModel';
import { MOTIVE, PRIORITIES } from './selectedReasonsModel';

export const selectedOptions = Array.from({ length: 13 }, (_, index) => ({ value: `option-${String(index)}`, label: `合成項目${String(index + 1)}`, display_order: index }));
export const selectionOptions: SelectionOptions = Object.fromEntries([MOTIVE, ...PRIORITIES].map(property => [property, selectedOptions]));
export function selectedRow(key: string, date: string | null, selections: Record<string, string[]> = { [MOTIVE]: ['option-0'], [PRIORITIES[0]]: ['option-1'], [PRIORITIES[1]]: ['option-2'], [PRIORITIES[2]]: ['option-3'] }): ApplicationRow {
  return { key, date, selections, gender: null, age: null, prefecture: null, municipality: null };
}

/** 既存の日付の例に架空の選択値を付ける。文章の内容は選択値に使わない。 */
export function withSelectedReasons(job: JobCopyRecord): JobCopyRecord {
  const records = new Map<string, ApplicationRow>();
  for (const item of job.applicantReasons?.items ?? []) {
    if (APPLICATION_TEXT_SOURCES.includes(item.sourceProperty)) records.set(item.applicant ?? item.id, selectedRow(item.applicant ?? item.id, item.applicationDate));
  }
  const rows = [...records.values()];
  for (const [date, count] of Object.entries(job.overallApplications?.byDate ?? {})) {
    const missing = count - rows.filter(row => row.date === date).length;
    for (let index = 0; index < missing; index += 1) rows.push(selectedRow(`blank-${date}-${String(index)}`, date, {}));
  }
  const missing = (job.overallApplications?.missingDate ?? 0) - rows.filter(row => row.date === null).length;
  for (let index = 0; index < missing; index += 1) rows.push(selectedRow(`undated-${String(index)}`, null, {}));
  return { ...job, applicationRows: rows, ...(job.overallApplications ? { overallApplications: { ...job.overallApplications, selectionOptions } } : {}) };
}
