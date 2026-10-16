// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { RecentChangesButton, RecentChangesList, useRecentChanges } from './RecentChangesCard';
import type { RecentChangesResponse } from './RecentChangesCard';
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: api }));
afterEach(() => { cleanup(); vi.resetAllMocks(); vi.useRealTimers(); });
const ready: RecentChangesResponse = { status: 'ready', refresh_failed: false, report: { count: 3, listings: [
  { id: '1', title: '合成終了求人', media: 'airwork', publication_status: '掲載終了', deals: [{ id: '10', name: '合成案件A', company: '合成事業所' }, { id: '20', name: '合成案件B', company: '合成事業所' }], fields: ['本文', '給与', '求人名'], last_changed_at: '2026-10-15T16:12:00Z' },
  { id: '3', title: '合成配送求人', media: 'hrh', publication_status: '掲載中', deals: [{ id: '20', name: '合成案件B', company: '合成事業所' }], fields: ['本文'], last_changed_at: '2026-10-12T03:00:00Z' },
  { id: '2', title: '合成掲載求人', media: 'hrh', publication_status: '掲載中', deals: [{ id: '10', name: '合成案件A', company: null }], fields: ['給与'], last_changed_at: '2026-10-09T15:00:00Z' },
] } };
it('求人数・複数案件・媒体・掲載状態・変更項目・日本時間を表示し、求人と案件を開く', () => {
  const open = vi.fn(); const openListing = vi.fn();
  render(<><RecentChangesButton data={ready} open onClick={vi.fn()} /><RecentChangesList data={ready} error="" onOpenListing={openListing} onOpenDeal={open} /></>);
  const button = screen.getByRole('button', { name: /直近7日に変更/ });
  expect(within(button).getByText('3')).toBeTruthy();
  expect(button.textContent).toContain('掲載終了の求人も含みます');
  expect(screen.getAllByRole('row').slice(1).map(row => row.textContent)).toEqual([
    '合成事業所合成案件A・合成案件B合成終了求人AirWork掲載終了本文・給与・求人名2026/10/16 01:12',
    '合成事業所合成案件B合成配送求人HRハッカー掲載中本文2026/10/12 12:00',
    '—合成案件A合成掲載求人HRハッカー掲載中給与2026/10/10 00:00',
  ]);
  fireEvent.click(within(screen.getByRole('row', { name: /合成終了求人/ })).getByRole('button', { name: '合成案件A' }));
  expect(open).toHaveBeenCalledWith(ready.report.listings[0]?.deals[0]);
  fireEvent.click(screen.getByRole('button', { name: '合成終了求人' }));
  expect(openListing).toHaveBeenCalledWith('1');
});
it('集計中・取得失敗・前回分の表示を区別する', () => {
  const view = render(<><RecentChangesButton data={{ status: 'preparing' }} open onClick={vi.fn()} /><RecentChangesList data={{ status: 'preparing' }} error="" onOpenListing={vi.fn()} onOpenDeal={vi.fn()} /></>);
  expect(screen.getByText('集計中')).toBeTruthy();
  expect(screen.getByRole('status').textContent).toBe('変更のあった求人を集計しています…');
  view.rerender(<RecentChangesList data={{ status: 'failed' }} error="" onOpenListing={vi.fn()} onOpenDeal={vi.fn()} />);
  expect(screen.getByRole('alert').textContent).toContain('読み込めませんでした');
  view.rerender(<RecentChangesList data={{ ...ready, refresh_failed: true }} error="" onOpenListing={vi.fn()} onOpenDeal={vi.fn()} />);
  expect(screen.getByRole('status').textContent).toContain('前回取得した件数');
  expect(screen.getByText('合成終了求人')).toBeTruthy();
});
it('集計中だけ取り直し、取得後と画面を離れた後は停止する', async () => {
  vi.useFakeTimers();
  api.mockResolvedValueOnce({ ok: true, data: { status: 'preparing' } }).mockResolvedValue({ ok: true, data: ready });
  function Harness() { const recent = useRecentChanges(0); return <RecentChangesButton data={recent.data} open={false} onClick={vi.fn()} />; }
  render(<Harness />);
  await act(() => Promise.resolve());
  expect(screen.getByText('集計中')).toBeTruthy();
  await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
  expect(screen.getByText('3')).toBeTruthy();
  await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
  expect(api).toHaveBeenCalledTimes(2);
  cleanup();
  await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
  expect(api).toHaveBeenCalledTimes(2);
});
it('0件は集計中や取得失敗と区別する', () => {
  render(<RecentChangesList data={{ ...ready, report: { count: 0, listings: [] } }} error="" onOpenListing={vi.fn()} onOpenDeal={vi.fn()} />);
  expect(screen.getByText('該当する求人はありません。')).toBeTruthy();
});
it('ホームの読み込みが終わるまで追加の取得を始めない', async () => {
  api.mockResolvedValue({ ok: true, data: ready });
  function Harness({ enabled }: { enabled: boolean }) { const recent = useRecentChanges(0, enabled); return <RecentChangesButton data={recent.data} open={false} onClick={vi.fn()} />; }
  const view = render(<Harness enabled={false} />);
  await act(() => Promise.resolve());
  expect(api).not.toHaveBeenCalled();
  expect(screen.getByText('集計中')).toBeTruthy();
  view.rerender(<Harness enabled />);
  await act(() => Promise.resolve());
  expect(api).toHaveBeenCalledTimes(1);
  expect(screen.getByText('3')).toBeTruthy();
});

it('内訳の件数・重なるチップ・説明と、再選択・すべてでの絞り込み解除を表示する', () => {
  const data: RecentChangesResponse = { ...ready, report: { ...ready.report,
    listings: ready.report.listings.map((row, i) => ({ ...row, classifications: i === 0 ? ['out_of_scope'] : i === 1 ? ['less', 'costly'] : ['more'] })),
    breakdown: { status: 'ready', cards: [
      { key: 'less', title: '応募が少ない求人', rule: '合成の減少の説明。', count: 1 },
      { key: 'costly', title: '課金の説明', rule: '合成の課金の説明。', count: 1 },
      { key: 'more', title: '応募が多い求人', rule: '合成の増加の説明。', count: 1 },
      { key: 'flat', title: 'ほぼ同じ', rule: '合成の横ばいの説明。', count: 0 },
      { key: 'out_of_scope', title: '対象外', rule: '掲載終了など。', count: 1 },
    ] },
  } };
  render(<RecentChangesList data={data} error="" onOpenListing={vi.fn()} onOpenDeal={vi.fn()} />);
  const less = screen.getByRole('button', { name: '応募が減った 1件' });
  expect(screen.getByRole('button', { name: 'ほぼ同じ 0件' }).className).toBe('jh-change-zero');
  fireEvent.mouseEnter(less);
  expect(screen.getByText('合成の減少の説明。').textContent).toBe('合成の減少の説明。');
  fireEvent.click(less);
  expect(screen.getAllByRole('row').slice(1).map(row => row.textContent)).toEqual(['合成事業所合成案件B合成配送求人HRハッカー掲載中応募が減った課金が高いまま応募が少ない本文2026/10/12 12:00']);
  fireEvent.click(less);
  expect(screen.getAllByRole('row')).toHaveLength(4);
  fireEvent.click(screen.getByRole('button', { name: '課金が高いまま応募が少ない 1件' }));
  expect(screen.getAllByRole('row')).toHaveLength(2);
  fireEvent.click(screen.getByRole('button', { name: 'すべて 3件' }));
  expect(screen.getAllByRole('row')).toHaveLength(4);
  fireEvent.click(screen.getByRole('button', { name: 'ほぼ同じ 0件' }));
  expect(screen.getByText('この分類の求人はありません。')).toBeTruthy();
});

it('内訳だけが集計中でも一覧を保ち、完成まで取り直す', async () => {
  vi.useFakeTimers();
  const preparing = { ...ready, report: { ...ready.report, breakdown: { status: 'preparing' } } };
  const complete = { ...ready, report: { ...ready.report, breakdown: { status: 'ready', cards: [] } } };
  api.mockResolvedValueOnce({ ok: true, data: preparing }).mockResolvedValue({ ok: true, data: complete });
  function Harness() { const recent = useRecentChanges(0); return <RecentChangesList data={recent.data} error="" onOpenListing={vi.fn()} onOpenDeal={vi.fn()} />; }
  render(<Harness />);
  await act(() => Promise.resolve());
  expect(screen.getByText('内訳は集計中です。')).toBeTruthy();
  expect(screen.getAllByRole('row')).toHaveLength(4);
  await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
  expect(screen.queryByText('内訳は集計中です。')).toBeNull();
  expect(screen.getByRole('button', { name: 'すべて 3件' })).toBeTruthy();
  await act(async () => { await vi.advanceTimersByTimeAsync(6000); });
  expect(api).toHaveBeenCalledTimes(2);
});
