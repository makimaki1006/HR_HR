import { apiGet } from '../../api/client';

interface BodyDates { dates?: Record<string, string | null> }
const recent = new Map<string, { at: number; promise: ReturnType<typeof apiGet<BodyDates>> }>();
const KEEP_MS = 15_000;

/** 本文の最終変更日。求人の表と案件のまとめが同じ求人を同時に読むので、同じ読み取りは1回にまとめる。 */
export function fetchBodyDates(unsorted: readonly string[]): ReturnType<typeof apiGet<BodyDates>> {
  const ids = [...unsorted].sort();
  const key = ids.join(',');
  const kept = recent.get(key);
  if (kept && Date.now() - kept.at < KEEP_MS) return kept.promise;
  const promise = apiGet<BodyDates>(`/api/job-copy/listing-body-dates?ids=${ids.map(encodeURIComponent).join(',')}`, { timeoutMs: 120_000 });
  recent.set(key, { at: Date.now(), promise });
  void promise.then(result => { if (!result.ok) recent.delete(key); });
  return promise;
}
