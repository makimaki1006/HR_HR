import { afterEach, expect, it, vi } from 'vitest';
import { loadCompanyDatas } from './dealSummaryLoad';
import type { CompanyDeal } from './companyDeals';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', () => ({ apiGet: api }));
vi.mock('./bodyDatesShared', () => ({ fetchBodyDates: () => Promise.resolve({ ok: true, data: { dates: {} } }) }));
afterEach(() => { vi.resetAllMocks(); });

const deals = (ids: string[]) => ids.map(id => ({ id, name: `案件${id}` })) as CompanyDeal[];
const listing = (id: string) => ({ id, title: `求人${id}`, media: 'hrh', publication_status: '掲載中', deal_ids: [] });
/** 取引ごとに求人 1 件、応募日ごとの件数 { '2026-10-01': 取引番号 } を返す。`broken` の取引は読み取りに失敗する。 */
function fake(broken: string[]) {
  api.mockImplementation((url: string) => {
    const params = new URL(url, 'http://localhost').searchParams;
    const deal = params.get('deal') ?? '';
    if (broken.includes(deal)) return Promise.resolve({ ok: false, error: new Error('failed') });
    if (url.startsWith('/api/job-copy/listings')) return Promise.resolve({ ok: true, data: { status: 'ready', listings: [listing(`L${deal}`)], next_offset: null } });
    return Promise.resolve({ ok: true, data: { deal_id: deal, listings: [{ listing_id: `L${deal}`, total: Number(deal), by_date: { '2026-10-01': Number(deal) }, fetched_at: '2026-10-02T00:00:00Z' }], shared_applications: 0 } });
  });
}

it('取引を並べたまま読み、読めた順に途中経過を出して全部そろったら ready にする', async () => {
  fake([]);
  const seen: string[][] = [];
  const result = await loadCompanyDatas('1', new AbortController().signal, deals(['1', '2', '3', '4']), datas => { seen.push(datas.map(data => data.id)); });
  expect(result.status).toBe('ready');
  if (result.status !== 'ready') return;
  expect(result.datas.map(data => data.id)).toEqual(['1', '2', '3', '4']);
  expect(result.datas.map(data => data.applications.get(`L${data.id}`))).toEqual([{ '2026-10-01': 1 }, { '2026-10-01': 2 }, { '2026-10-01': 3 }, { '2026-10-01': 4 }]);
  expect(result.notes).toEqual([]);
  expect(seen.length).toBe(4);
  expect(seen.at(-1)).toEqual(['1', '2', '3', '4']);
});

it('1 つの取引が読めなくても、ほかの取引の数字は出して、読めなかった件数を伝える', async () => {
  fake(['2']);
  const result = await loadCompanyDatas('1', new AbortController().signal, deals(['1', '2', '3']));
  expect(result.status).toBe('ready');
  if (result.status !== 'ready') return;
  expect(result.datas.map(data => data.id)).toEqual(['1', '3']);
  expect(result.notes).toEqual(['読み込めなかった取引が 1件あります。その取引の求人は合計に含まれていません。']);
});

it('全部の取引が読めないときは、読み込み中のまま止めず error を返す', async () => {
  fake(['1', '2']);
  const result = await loadCompanyDatas('1', new AbortController().signal, deals(['1', '2']));
  expect(result).toEqual({ status: 'error', message: 'この事業所の求人を読み込めませんでした。' });
});

it('全体の待ちを過ぎたら、まだ終わらない取引を待たずに結果を返す', async () => {
  vi.useFakeTimers();
  try {
    api.mockImplementation((url: string, options?: { signal?: AbortSignal }) => {
      const deal = new URL(url, 'http://localhost').searchParams.get('deal') ?? '';
      if (deal === '2') return new Promise(resolve => { options?.signal?.addEventListener('abort', () => { resolve({ ok: false, error: new Error('aborted') }); }); });
      if (url.startsWith('/api/job-copy/listings')) return Promise.resolve({ ok: true, data: { status: 'ready', listings: [listing(`L${deal}`)], next_offset: null } });
      return Promise.resolve({ ok: true, data: { deal_id: deal, listings: [{ listing_id: `L${deal}`, total: 1, by_date: { '2026-10-01': 1 }, fetched_at: '2026-10-02T00:00:00Z' }], shared_applications: 0 } });
    });
    const pending = loadCompanyDatas('1', new AbortController().signal, deals(['1', '2']));
    await vi.advanceTimersByTimeAsync(120_001);
    const result = await pending;
    expect(result.status).toBe('ready');
    if (result.status !== 'ready') return;
    expect(result.datas.map(data => data.id)).toEqual(['1']);
    expect(result.notes[0]).toContain('読み込めなかった取引が 1件');
  } finally { vi.useRealTimers(); }
});
