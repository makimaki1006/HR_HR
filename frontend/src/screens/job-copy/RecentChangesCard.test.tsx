// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { RecentChangesButton, RecentChangesList, useRecentChanges } from './RecentChangesCard';
import type { RecentChangesResponse } from './RecentChangesCard';
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: api }));
afterEach(() => { cleanup(); vi.resetAllMocks(); vi.useRealTimers(); });
const ready: RecentChangesResponse = { status: 'ready', refresh_failed: false, report: { count: 2, deals: [
  { id: '10', name: '合成案件A', company: '合成事業所', changed_listings: 3, fields: ['本文', '給与', '求人名'], last_changed_at: '2026-10-15T16:12:00Z' },
  { id: '20', name: '合成案件B', company: null, changed_listings: 1, fields: ['給与'], last_changed_at: '2026-10-09T15:00:00Z' },
] } };
it('案件数・変更求人数・変更項目・日本時間を表示し、案件を開く', () => {
  const open = vi.fn();
  render(<><RecentChangesButton data={ready} open onClick={vi.fn()} /><RecentChangesList data={ready} error="" onOpenDeal={open} /></>);
  const button = screen.getByRole('button', { name: /直近7日に変更/ });
  expect(within(button).getByText('2')).toBeTruthy();
  expect(button.textContent).toContain('掲載終了の求人も含みます');
  expect(screen.getAllByRole('row').slice(1).map(row => row.textContent)).toEqual([
    '合成事業所合成案件A3件本文・給与・求人名2026/10/16 01:12',
    '—合成案件B1件給与2026/10/10 00:00',
  ]);
  fireEvent.click(screen.getByRole('button', { name: '合成案件A' }));
  expect(open).toHaveBeenCalledWith(ready.report.deals[0]);
});
it('集計中・取得失敗・前回分の表示を区別する', () => {
  const view = render(<><RecentChangesButton data={{ status: 'preparing' }} open onClick={vi.fn()} /><RecentChangesList data={{ status: 'preparing' }} error="" onOpenDeal={vi.fn()} /></>);
  expect(screen.getByText('集計中')).toBeTruthy();
  expect(screen.getByRole('status').textContent).toBe('変更のあった案件を集計しています…');
  view.rerender(<RecentChangesList data={{ status: 'failed' }} error="" onOpenDeal={vi.fn()} />);
  expect(screen.getByRole('alert').textContent).toContain('読み込めませんでした');
  view.rerender(<RecentChangesList data={{ ...ready, refresh_failed: true }} error="" onOpenDeal={vi.fn()} />);
  expect(screen.getByRole('status').textContent).toContain('前回取得した件数');
  expect(screen.getByText('3件')).toBeTruthy();
});
it('集計中だけ取り直し、取得後と画面を離れた後は停止する', async () => {
  vi.useFakeTimers();
  api.mockResolvedValueOnce({ ok: true, data: { status: 'preparing' } }).mockResolvedValue({ ok: true, data: ready });
  function Harness() { const recent = useRecentChanges(0); return <RecentChangesButton data={recent.data} open={false} onClick={vi.fn()} />; }
  render(<Harness />);
  await act(() => Promise.resolve());
  expect(screen.getByText('集計中')).toBeTruthy();
  await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
  expect(screen.getByText('2')).toBeTruthy();
  await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
  expect(api).toHaveBeenCalledTimes(2);
  cleanup();
  await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
  expect(api).toHaveBeenCalledTimes(2);
});
it('0件は集計中や取得失敗と区別する', () => {
  render(<RecentChangesList data={{ ...ready, report: { count: 0, deals: [] } }} error="" onOpenDeal={vi.fn()} />);
  expect(screen.getByText('該当する案件はありません。')).toBeTruthy();
});
