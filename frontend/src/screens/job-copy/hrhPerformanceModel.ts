export interface HrhPerformanceRow {
  period_start: string; period_end: string;
  impressions: number | null; clicks: number | null; cost_yen: number | null; applications: number | null;
}
export interface HrhPerformanceCollection {
  schema_version: 1; source: 'hrhacker'; job_id: string; captured_at: string; rows: HrhPerformanceRow[];
}
const invalid = (): never => { throw new Error('HRハッカー実績の媒体の求人番号・期間・数値を確認してください。'); };
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return invalid();
  return value as Record<string, unknown>;
}
function exactKeys(value: Record<string, unknown>, keys: string[]) {
  if (Object.keys(value).length !== keys.length || Object.keys(value).some(key => !keys.includes(key))) invalid();
}
export function metricDate(value: unknown): string {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return invalid();
  const parsed = new Date(`${value}T00:00:00Z`);
  if (!Number.isFinite(parsed.getTime()) || parsed.toISOString().slice(0, 10) !== value) return invalid();
  return value;
}
function metric(value: unknown, integer = true): number | null {
  if (value === null) return null;
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0 || value > Number.MAX_SAFE_INTEGER || (integer && !Number.isSafeInteger(value))) return invalid();
  return value;
}
export function parseHrhPerformance(value: unknown, jobId: string): HrhPerformanceCollection {
  const data = object(value);
  exactKeys(data, ['schema_version', 'source', 'job_id', 'captured_at', 'rows']);
  if (data.schema_version !== 1 || data.source !== 'hrhacker' || typeof data.job_id !== 'string' || !/^\d{8}$/.test(data.job_id) || data.job_id !== jobId || typeof data.captured_at !== 'string' || !/^\d{4}-\d{2}-\d{2}T.*(?:Z|[+-]\d{2}:\d{2})$/.test(data.captured_at) || !Number.isFinite(Date.parse(data.captured_at)) || !Array.isArray(data.rows) || data.rows.length > 1000) return invalid();
  metricDate(data.captured_at.slice(0, 10));
  const rows = data.rows.map(raw => {
    const row = object(raw);
    exactKeys(row, ['period_start', 'period_end', 'impressions', 'clicks', 'cost_yen', 'applications']);
    const start = metricDate(row.period_start); const end = metricDate(row.period_end);
    const impressions = metric(row.impressions); const clicks = metric(row.clicks);
    if (start > end || (impressions !== null && clicks !== null && clicks > impressions)) return invalid();
    return { period_start: start, period_end: end, impressions, clicks, cost_yen: metric(row.cost_yen, false), applications: metric(row.applications) };
  }).sort((a, b) => a.period_start.localeCompare(b.period_start));
  for (let index = 1; index < rows.length; index++) {
    const current = rows[index]; const previous = rows[index - 1];
    if (current && previous && current.period_start <= previous.period_end) return invalid();
  }
  return { schema_version: 1, source: 'hrhacker', job_id: data.job_id, captured_at: data.captured_at, rows };
}
export function performanceRatios(row: HrhPerformanceRow) {
  return {
    ctr: row.impressions !== null && row.impressions > 0 && row.clicks !== null ? row.clicks / row.impressions * 100 : null,
    cpc: row.clicks !== null && row.clicks > 0 && row.cost_yen !== null ? row.cost_yen / row.clicks : null,
    cpa: row.applications !== null && row.applications > 0 && row.cost_yen !== null ? row.cost_yen / row.applications : null,
  };
}
export function comparePerformance(before: HrhPerformanceRow, after: HrhPerformanceRow) {
  const first = performanceRatios(before); const last = performanceRatios(after);
  return { ctrDeltaPp: first.ctr === null || last.ctr === null ? null : last.ctr - first.ctr,
    clicksPerDayDelta: before.clicks === null || after.clicks === null ? null : after.clicks / days(after) - before.clicks / days(before) };
}
function days(row: HrhPerformanceRow) { return (Date.parse(row.period_end) - Date.parse(row.period_start)) / 86_400_000 + 1; }
