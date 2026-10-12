// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, renderHook, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { CallQueueScreen, HOVER_PREFETCH_DELAY_MS } from './CallQueueScreen';
import { makeItem, makeResponse, okMetadataFetch, okUserFetch, okCatalogFetch, resetDockStorage, setScreenHeight } from './queueTestUtil';
import type { QueueFilters } from './queueModel';
import { fixtureQueuePage } from './queueFixture';
import { DEFAULT_FILTERS } from './queueModel';
import { fixtureDetail } from './workspaceFixture';
import { detailUrl, useDealDetail } from './useDealDetail';
import type { DetailFetch, DetailFetchOptions } from './useDealDetail';
import { fixtureOwnersFetch } from './useOwners';

beforeEach(() => { resetDockStorage(); setScreenHeight(900); });
afterEach(() => { cleanup(); vi.useRealTimers(); });

const sampleId = fixtureQueuePage(DEFAULT_FILTERS, null).items[0]?.deal_id ?? '';
/** 詳細の中身: 会社名を「詳細会社<ID>」にして、一覧の行の「架空会社<ID>」と見分ける */
function detailOf(id: string, name = `詳細会社${id}`): WorkspaceResponse {
  const d = fixtureDetail(sampleId);
  if (d === null) throw new Error('fixture');
  return { ...d, deal: { ...d.deal, id }, companies: d.companies.map(c => ({ ...c, name })) };
}

interface Call { id: string; opts: DetailFetchOptions | undefined; resolve: (r: ApiResult<WorkspaceResponse>) => void }
/** 応答を手で返す偽の取得関数 (先読みも通常の読み取りも記録する) */
function manual() {
  const calls: Call[] = [];
  const fetcher: DetailFetch = (id, _s, _p, opts) => new Promise(resolve => { calls.push({ id, opts, resolve }); });
  const normal = () => calls.filter(c => c.opts?.prefetch !== true);
  const prefetches = () => calls.filter(c => c.opts?.prefetch === true);
  return { calls, fetcher, normal, prefetches };
}
const okR = (d: WorkspaceResponse): ApiResult<WorkspaceResponse> => ({ ok: true, data: d });
const answer = async (c: Call | undefined, d: WorkspaceResponse) => { await act(async () => { c?.resolve(okR(d)); await Promise.resolve(); }); };

const ITEMS = [makeItem('1'), makeItem('2'), makeItem('3'), makeItem('4')];
async function renderScreen(df: DetailFetch, items = ITEMS) {
  const queue = (f: QueueFilters) => Promise.resolve<ApiResult<ReturnType<typeof makeResponse>>>({ ok: true, data: makeResponse(f, items) });
  render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={queue} ownersFetcher={fixtureOwnersFetch} detailFetcher={df}
    metadataFetcher={okMetadataFetch} zoomOptions={{ loadTimeoutMs: 600_000 }} initialSearch="?view=queue&owner=all" />);
  await waitFor(() => { expect(screen.getByRole('list', { name: '架電キュー' })).toBeTruthy(); });
}
const row = (n: string) => within(screen.getByRole('list', { name: '架電キュー' })).getByText(`架空会社${n}`).closest('button') as HTMLElement;
const heading = (text: string) => screen.queryByText(text, { selector: 'h2' });

describe('一覧の行の値で出す見出し', () => {
  it('shows the row values at once while the detail loads, and replaces them when the detail arrives', async () => {
    const m = manual();
    const item = makeItem('1', { stage_label: '不在', phone: '+81300000005', next_call_date: '2026-10-05', next_call_time: '10:30', last_call_date: '2026-10-01',
      contact: { id: 'c1', name: '行の担当 太郎', phone: null, mobile: null, job_title: null, extra_count: 0 } });
    await renderScreen(m.fetcher, [item, makeItem('2')]);
    fireEvent.click(row('1'));
    const preview = screen.getByTestId('row-preview');
    expect(preview.getAttribute('data-deal-id')).toBe('1');
    expect(preview.textContent).toContain('架空会社1');
    expect(preview.textContent).toContain('行の担当 太郎');
    expect(preview.textContent).toContain('03-0000-0005');
    expect(preview.textContent).toContain('不在');
    expect(preview.textContent).toContain('次回 2026/10/05 10:30');
    expect(preview.textContent).toContain('最終 2026/10/01');
    expect(within(preview).getByRole('status').textContent).toBe('詳細を読み込み中…');
    // 発信・編集のボタンは出さない (詳細が届くまでの仮の表示)
    expect(within(preview).queryAllByRole('button')).toHaveLength(0);
    await answer(m.normal()[0], detailOf('1'));
    expect(screen.queryByTestId('row-preview')).toBeNull();
    expect(heading('詳細会社1')).toBeTruthy();
  });

  it('keeps showing the newly selected row (not the previous deal) while the arrow-key delay is pending', async () => {
    const m = manual();
    await renderScreen(m.fetcher);
    fireEvent.click(row('1'));
    await answer(m.normal()[0], detailOf('1'));
    fireEvent.keyDown(row('1'), { key: 'ArrowDown' });
    expect(screen.getByTestId('row-preview').getAttribute('data-deal-id')).toBe('2');
    expect(heading('詳細会社1')).toBeNull();
  });
});

describe('画面のキャッシュ', () => {
  it('re-selecting a loaded deal shows the cached detail with no loading state, then revalidates and swaps in fresh data', async () => {
    const m = manual();
    await renderScreen(m.fetcher);
    fireEvent.click(row('1'));
    await answer(m.normal()[0], detailOf('1'));
    fireEvent.click(row('2'));
    await answer(m.normal()[1], detailOf('2'));
    expect(heading('詳細会社2')).toBeTruthy();
    fireEvent.click(row('1'));
    // 開いた直後 (応答を待たずに) 前に読んだ内容が出る。「読み込み中」も一覧の行の見出しも出ない
    expect(heading('詳細会社1')).toBeTruthy();
    expect(screen.queryByTestId('row-preview')).toBeNull();
    expect(screen.queryByText('詳細を読み込み中…')).toBeNull();
    // 裏で確かめる (fresh ではない通常の読み取り)
    const again = m.normal()[2];
    expect(again?.id).toBe('1');
    expect(again?.opts?.fresh).not.toBe(true);
    await answer(again, detailOf('1', '更新後の会社'));
    expect(heading('更新後の会社')).toBeTruthy();
    expect(heading('詳細会社1')).toBeNull();
  });

  it('keeps the cached content when the background check fails and says so', async () => {
    const m = manual();
    await renderScreen(m.fetcher);
    fireEvent.click(row('1'));
    await answer(m.normal()[0], detailOf('1'));
    fireEvent.click(row('2'));
    await answer(m.normal()[1], detailOf('2'));
    fireEvent.click(row('1'));
    await act(async () => { m.normal()[2]?.resolve({ ok: false, error: new ApiHttpError(503, { error_kind: 'hubspot_busy' }) }); await Promise.resolve(); });
    expect(heading('詳細会社1')).toBeTruthy();
    expect(within(screen.getByTestId('detail-freshness')).getByRole('alert').textContent).toContain('最新の情報を読めませんでした');
  });

  it('a cached entry older than 60 s is not used (loading state again)', async () => {
    const m = manual();
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date('2026-10-12T00:00:00Z'));
    await renderScreen(m.fetcher);
    fireEvent.click(row('1'));
    await answer(m.normal()[0], detailOf('1'));
    fireEvent.click(row('2'));
    await answer(m.normal()[1], detailOf('2'));
    vi.setSystemTime(new Date('2026-10-12T00:01:01Z'));
    fireEvent.click(row('1'));
    expect(heading('詳細会社1')).toBeNull();
    expect(screen.getByTestId('row-preview')).toBeTruthy();
  });

  it('「最新にする」 reads with fresh=1 and drops the cached entry of that deal', async () => {
    const m = manual();
    await renderScreen(m.fetcher);
    fireEvent.click(row('1'));
    await answer(m.normal()[0], detailOf('1'));
    fireEvent.click(within(screen.getByTestId('detail-freshness')).getByRole('button', { name: '最新にする' }));
    const fresh = m.normal()[1];
    expect(fresh?.id).toBe('1');
    expect(fresh?.opts?.fresh).toBe(true);
    // 読み直しの間に別の案件へ移って戻ると、捨てた古い内容は出さない
    fireEvent.click(row('2'));
    await answer(m.normal()[2], detailOf('2'));
    fireEvent.click(row('1'));
    expect(heading('詳細会社1')).toBeNull();
  });
});

describe('useDealDetail のキャッシュ', () => {
  it('a deal whose call ended (staleIds) is read with fresh=1 and never from the cache', async () => {
    const m = manual();
    const stale = new Set<string>();
    const { result, rerender } = renderHook(({ id }: { id: string }) => useDealDetail(id, 'live', m.fetcher, undefined, stale), { initialProps: { id: '1' } });
    await answer(m.calls[0], detailOf('1'));
    expect(result.current.state.phase).toBe('ready');
    rerender({ id: '2' });
    await answer(m.calls[1], detailOf('2'));
    stale.add('1');
    rerender({ id: '1' });
    expect(result.current.state.phase).toBe('loading');
    expect(m.calls[2]?.opts?.fresh).toBe(true);
  });

  it('the detail URL carries prefetch=1 only for prefetches and fresh=1 only for fresh reads', () => {
    expect(detailUrl('5', undefined, { prefetch: true })).toBe('/api/crm/workspace/deals/5?prefetch=1');
    expect(detailUrl('5', undefined, { fresh: true })).toBe('/api/crm/workspace/deals/5?fresh=1');
    expect(detailUrl('5')).toBe('/api/crm/workspace/deals/5');
  });
});

describe('先読み', () => {
  it('prefetches the next row exactly once after the selected detail loads, and shows it instantly when opened', async () => {
    const m = manual();
    await renderScreen(m.fetcher);
    fireEvent.click(row('1'));
    expect(m.prefetches()).toHaveLength(0);
    await answer(m.normal()[0], detailOf('1'));
    expect(m.prefetches().map(c => c.id)).toEqual(['2']);
    await answer(m.prefetches()[0], detailOf('2'));
    // 同じ案件のまま描き直しても増えない
    fireEvent.click(within(screen.getByTestId('detail-freshness')).getByRole('button', { name: '最新にする' }));
    await answer(m.normal()[1], detailOf('1'));
    expect(m.prefetches()).toHaveLength(1);
    // 先読みした案件は開いた瞬間に出る
    fireEvent.click(row('2'));
    expect(heading('詳細会社2')).toBeTruthy();
    expect(screen.queryByText('詳細を読み込み中…')).toBeNull();
  });

  it('does not prefetch a next row that is already cached, but prefetches the one after a newly loaded deal', async () => {
    const m = manual();
    await renderScreen(m.fetcher);
    fireEvent.click(row('1'));
    await answer(m.normal()[0], detailOf('1'));
    await answer(m.prefetches()[0], detailOf('2'));
    fireEvent.click(row('2'));
    await answer(m.normal()[1], detailOf('2'));
    // 2 の次 (3) は先読みする
    expect(m.prefetches().map(c => c.id)).toEqual(['2', '3']);
    await answer(m.prefetches()[1], detailOf('3'));
    // 1 に戻る: 次の行 (2) は画面のキャッシュにあるので読まない
    fireEvent.click(row('1'));
    await answer(m.normal()[2], detailOf('1'));
    expect(m.prefetches().map(c => c.id)).toEqual(['2', '3']);
  });

  it('never prefetches after the last row, and a failed prefetch is ignored', async () => {
    const m = manual();
    await renderScreen(m.fetcher, [makeItem('1'), makeItem('2')]);
    fireEvent.click(row('1'));
    await answer(m.normal()[0], detailOf('1'));
    await act(async () => { m.prefetches()[0]?.resolve({ ok: false, error: new Error('busy') }); await Promise.resolve(); });
    expect(screen.queryByRole('alert')).toBeNull();
    fireEvent.click(row('2'));
    await answer(m.normal()[1], detailOf('2'));
    expect(m.prefetches().map(c => c.id)).toEqual(['2']);
  });

  it('prefetches a hovered row only after the 200 ms dwell, and not at all if the pointer leaves earlier', async () => {
    const m = manual();
    {
      await renderScreen(m.fetcher);
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
      expect(HOVER_PREFETCH_DELAY_MS).toBe(200);
      fireEvent.pointerEnter(row('3'));
      act(() => { vi.advanceTimersByTime(199); });
      expect(m.prefetches()).toHaveLength(0);
      fireEvent.pointerLeave(row('3'));
      act(() => { vi.advanceTimersByTime(1000); });
      expect(m.prefetches()).toHaveLength(0);
      fireEvent.pointerEnter(row('3'));
      act(() => { vi.advanceTimersByTime(200); });
      expect(m.prefetches().map(c => [c.id, c.opts])).toEqual([['3', { prefetch: true }]]);
      expect(m.normal()).toHaveLength(0);
    }
  });

  it('keeps at most one prefetch in flight, and does not prefetch the selected row or an already cached row', async () => {
    const m = manual();
    await renderScreen(m.fetcher);
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    fireEvent.pointerEnter(row('3'));
    act(() => { vi.advanceTimersByTime(200); });
    fireEvent.pointerLeave(row('3'));
    fireEvent.pointerEnter(row('4'));
    act(() => { vi.advanceTimersByTime(200); });
    // 3 が返るまで 4 は読まない
    expect(m.prefetches().map(c => c.id)).toEqual(['3']);
    await answer(m.prefetches()[0], detailOf('3'));
    fireEvent.pointerLeave(row('4'));
    // 読み終えた 3 に戻っても読まない
    fireEvent.pointerEnter(row('3'));
    act(() => { vi.advanceTimersByTime(200); });
    expect(m.prefetches().map(c => c.id)).toEqual(['3']);
    // 選んでいる行は先読みしない
    fireEvent.click(row('4'));
    fireEvent.pointerEnter(row('4'));
    act(() => { vi.advanceTimersByTime(200); });
    expect(m.prefetches().map(c => c.id)).toEqual(['3']);
  });
});
