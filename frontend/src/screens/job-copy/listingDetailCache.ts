import { apiGet, ApiAbortedError } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { HubSpotVersions } from './hubspotListings';

/**
 * 求人の詳細（本文・画像・履歴）を読む窓口。
 * - 同じ求人は60秒、手元の結果を即座に返す（開き直し・前後の求人への移動を速くする）。
 * - 同時に同じ求人を読もうとした呼び出しは1本にまとめる。
 * - 前後の求人は、表示中の取得が終わってから裏で読む（同時に2本まで、古い順番は捨てる）。
 * - 会社名は本文とは別に読み、届いてから差し込む。一覧を読んだ時点で、表示中の求人の分をまとめて読んでおく。
 * - 前後の求人を先読みしたら、その求人の画像も先頭の数枚だけ、ブラウザに読み込んでおく。
 */
const DETAIL_TTL_MS = 60_000;
const COMPANY_TTL_MS = 5 * 60_000;
const MAX_ENTRIES = 100;
export const MAX_PREFETCH = 2;
/** 先読みする画像は、1件の求人につき先頭のこの枚数まで。同時に読むのもこの数まで。 */
export const MAX_IMAGE_PREFETCH = 4;
const MAX_IMAGE_CONCURRENCY = 2;
const COMPANY_BATCH_SIZE = 50;
/** まとめ読みの途中で求人を開いたとき、その結果を待つ長さ。これを過ぎたら個別に読む。 */
const BATCH_WAIT_MS = 1_500;

interface Flight {
  promise: Promise<ApiResult<HubSpotVersions>>;
  controller: AbortController;
  holders: number;
}
const done = new Map<string, { at: number; data: HubSpotVersions }>();
const flights = new Map<string, Flight>();
const companies = new Map<string, { at: number; name: string | null }>();
const companyFlights = new Map<string, Promise<string | null | undefined>>();
const companyBatchFlights = new Map<string, Promise<void>>();
const imageQueue: string[] = [];
const imageSeen = new Set<string>();
const imageLoading = new Set<HTMLImageElement>();
const prefetching = new Map<string, () => void>();
let queue: string[] = [];
let foreground = 0;
let generation = 0;

const versionsUrl = (id: string, fresh: boolean) => `/api/job-copy/listings/${encodeURIComponent(id)}/versions${fresh ? '?fresh=1' : ''}`;

function remember<T extends { at: number }>(map: Map<string, T>, id: string, value: T) {
  if (map.size >= MAX_ENTRIES && !map.has(id)) {
    const oldest = [...map.entries()].sort((a, b) => a[1].at - b[1].at)[0];
    if (oldest) map.delete(oldest[0]);
  }
  map.set(id, value);
}
/** 手元にある60秒以内の結果。なければ null。 */
export function peekVersions(id: string): HubSpotVersions | null {
  const hit = done.get(id);
  if (!hit) return null;
  if (Date.now() - hit.at >= DETAIL_TTL_MS) { done.delete(id); return null; }
  return hit.data;
}
function acquire(id: string, fresh: boolean) {
  let flight = flights.get(id);
  if (!flight) {
    const controller = new AbortController();
    const created: Flight = { controller, holders: 0, promise: Promise.resolve({ ok: false, error: new ApiAbortedError('not started') }) };
    const started = generation;
    created.promise = apiGet<HubSpotVersions>(versionsUrl(id, fresh), { signal: controller.signal, timeoutMs: 120_000 }).then(result => {
      if (flights.get(id) === created) {
        flights.delete(id);
        // 読んでいる間に保存があった（invalidate された）結果は残さない。
        if (result.ok && started === generation) remember(done, id, { at: Date.now(), data: result.data });
      }
      return result;
    });
    flights.set(id, created);
    flight = created;
  }
  flight.holders += 1;
  const held = flight;
  let released = false;
  return {
    flight: held,
    release: () => {
      if (released) return;
      released = true;
      held.holders -= 1;
      // 誰も待たなくなった読み込みは止める（先の求人へ移った後の古い読み込みを残さない）。
      if (held.holders <= 0 && flights.get(id) === held) { flights.delete(id); held.controller.abort(); }
    },
  };
}
/** 表示する求人の詳細。手元に60秒以内の結果があれば通信しない。 */
export async function loadVersions(id: string, options: { signal?: AbortSignal; fresh?: boolean } = {}): Promise<ApiResult<HubSpotVersions>> {
  const { signal, fresh = false } = options;
  const aborted = (): ApiResult<HubSpotVersions> => ({ ok: false, error: new ApiAbortedError('aborted') });
  if (signal?.aborted) return aborted();
  if (!fresh) { const hit = peekVersions(id); if (hit) return { ok: true, data: hit }; }
  foreground += 1;
  const { flight, release } = acquire(id, fresh);
  let onAbort: (() => void) | undefined;
  const cancelled = new Promise<ApiResult<HubSpotVersions>>(resolve => {
    onAbort = () => { resolve(aborted()); };
    signal?.addEventListener('abort', onAbort, { once: true });
  });
  try {
    return await Promise.race([flight.promise, cancelled]);
  } finally {
    if (onAbort) signal?.removeEventListener('abort', onAbort);
    release();
    foreground = Math.max(0, foreground - 1);
    pump();
  }
}
/** 求人を開いたとき最初に見せる画像（現在の本文、なければ最後の版）の先頭の数枚。外部の https の画像だけ。 */
export function firstImageUrls(data: HubSpotVersions): string[] {
  const urls = data.current?.image_urls ?? [...data.versions].reverse().find(version => version.image_urls?.length)?.image_urls ?? [];
  return urls.filter(url => url.startsWith('https://')).slice(0, MAX_IMAGE_PREFETCH);
}
function startImages() {
  while (imageLoading.size < MAX_IMAGE_CONCURRENCY && imageQueue.length) {
    const url = imageQueue.shift();
    if (url === undefined) break;
    const image = new Image();
    imageLoading.add(image);
    const finish = () => { image.onload = null; image.onerror = null; imageLoading.delete(image); startImages(); };
    image.onload = finish; image.onerror = finish;
    image.src = url;
  }
}
function preloadImages(urls: readonly string[]) {
  for (const url of urls) {
    if (imageSeen.has(url)) continue;
    if (imageSeen.size >= 500) imageSeen.clear();
    imageSeen.add(url);
    imageQueue.push(url);
  }
  startImages();
}
function pump() {
  while (foreground === 0 && prefetching.size < MAX_PREFETCH && queue.length) {
    const id = queue.shift();
    if (id === undefined) break;
    if (peekVersions(id) || flights.has(id) || prefetching.has(id)) continue;
    const { flight, release } = acquire(id, false);
    prefetching.set(id, release);
    void flight.promise.then(result => {
      if (prefetching.get(id) === release) prefetching.delete(id);
      release();
      // 前後の求人は、画像もすぐ出せるように先頭の数枚を読み込んでおく（外部の画像だけ。HubSpot は呼ばない）。
      if (result.ok) preloadImages(firstImageUrls(result.data));
      pump();
    });
  }
}
/** 選んだ求人の前後1件を、表示中の取得が終わってから裏で読む。呼ぶたびに古い予定は捨てる。 */
export function prefetchNeighbors(orderedIds: readonly string[], currentId: string) {
  const index = orderedIds.indexOf(currentId);
  const wanted = index < 0 ? [] : [orderedIds[index + 1], orderedIds[index - 1]].filter((id): id is string => Boolean(id));
  queue = wanted;
  for (const [id, release] of [...prefetching]) if (!wanted.includes(id)) { prefetching.delete(id); release(); }
  pump();
}
/** この求人に保存した後などに、手元の結果と読み込み中の古い結果を捨てる。 */
export function invalidateListing(id: string) {
  done.delete(id);
  generation += 1;
  const flight = flights.get(id);
  if (flight) { flights.delete(id); if (flight.holders <= 0) flight.controller.abort(); }
  queue = queue.filter(item => item !== id);
}
export function invalidateListingUrl(url: string) {
  const id = /^\/api\/job-copy\/listings\/(\d{1,20})\//.exec(url)?.[1];
  if (id) invalidateListing(id);
}
/** 会社名。本文とは別に読む。読めなかったときは undefined（空欄のまま）。 */
export function peekCompany(id: string): string | null | undefined {
  const hit = companies.get(id);
  if (!hit) return undefined;
  if (Date.now() - hit.at >= COMPANY_TTL_MS) { companies.delete(id); return undefined; }
  return hit.name;
}
/** 一覧に出ている求人の会社名を、まとめて読んでおく。読めなかった分は黙って飛ばし、開いたときに個別に読む。 */
export function loadCompanies(ids: readonly string[]): Promise<void> {
  const wanted = [...new Set(ids)].filter(id => /^\d{1,20}$/.test(id) && peekCompany(id) === undefined && !companyBatchFlights.has(id));
  const jobs: Promise<void>[] = [];
  for (let start = 0; start < wanted.length; start += COMPANY_BATCH_SIZE) {
    const chunk = wanted.slice(start, start + COMPANY_BATCH_SIZE);
    const created: Promise<void> = apiGet<{ companies?: Record<string, string | null> }>(`/api/job-copy/listings/companies?ids=${chunk.join(',')}`, { timeoutMs: 60_000 }).then(result => {
      for (const id of chunk) if (companyBatchFlights.get(id) === created) companyBatchFlights.delete(id);
      if (!result.ok) return;
      for (const id of chunk) {
        const name = result.data.companies?.[id];
        if (name !== undefined) remember(companies, id, { at: Date.now(), name });
      }
    });
    for (const id of chunk) companyBatchFlights.set(id, created);
    jobs.push(created);
  }
  return Promise.all(jobs).then(() => undefined);
}
export async function loadCompany(id: string, signal?: AbortSignal): Promise<string | null | undefined> {
  // まとめ読みが走っている求人は、その結果を少しだけ待つ（同じ会社名を二重に読まない）。長引くときは個別に読む。
  const batch = companyBatchFlights.get(id);
  if (batch && peekCompany(id) === undefined) {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const waits: Promise<unknown>[] = [batch, new Promise<void>(resolve => { timer = setTimeout(resolve, BATCH_WAIT_MS); })];
    if (signal) waits.push(new Promise<void>(resolve => { if (signal.aborted) resolve(); else signal.addEventListener('abort', () => { resolve(); }, { once: true }); }));
    await Promise.race(waits);
    clearTimeout(timer);
  }
  return loadCompanyAlone(id, signal);
}
function loadCompanyAlone(id: string, signal?: AbortSignal): Promise<string | null | undefined> {
  const known = peekCompany(id);
  if (known !== undefined) return Promise.resolve(known);
  let flight = companyFlights.get(id);
  if (!flight) {
    const created: Promise<string | null | undefined> = apiGet<{ company_name?: string | null }>(`/api/job-copy/listings/${encodeURIComponent(id)}/company`, { timeoutMs: 60_000 }).then(result => {
      if (companyFlights.get(id) === created) companyFlights.delete(id);
      if (!result.ok) return undefined;
      const name = result.data.company_name ?? null;
      remember(companies, id, { at: Date.now(), name });
      return name;
    });
    companyFlights.set(id, created);
    flight = created;
  }
  if (!signal) return flight;
  return Promise.race([flight, new Promise<undefined>(resolve => { if (signal.aborted) resolve(undefined); else signal.addEventListener('abort', () => { resolve(undefined); }, { once: true }); })]);
}
/** テスト用: 手元の結果と予定をすべて捨てる。 */
export function resetListingDetailCache() {
  for (const flight of flights.values()) flight.controller.abort();
  flights.clear(); done.clear(); companies.clear(); companyFlights.clear(); companyBatchFlights.clear(); prefetching.clear();
  imageQueue.length = 0; imageSeen.clear(); imageLoading.clear();
  queue = []; foreground = 0; generation += 1;
}
