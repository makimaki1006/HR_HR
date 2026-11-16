// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook } from '@testing-library/react';
import { backoff, runWhenDue, usePreparingRetry } from './preparingRetry';
import { useReportPolling } from '../screens/job-copy/useReportPolling';

let hidden = false;
const setHidden = (value: boolean) => { hidden = value; document.dispatchEvent(new Event('visibilitychange')); };
beforeEach(() => { vi.useFakeTimers(); vi.spyOn(document, 'hidden', 'get').mockImplementation(() => hidden); });
afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); hidden = false; });

it('間隔は最初の値から倍々に延び、60 秒で止まる', () => {
  const wait = backoff(3_000);
  expect(Array.from({ length: 7 }, () => wait())).toEqual([3_000, 6_000, 12_000, 24_000, 48_000, 60_000, 60_000]);
});

it('隠れている間は呼ばず、戻ったとき期限を過ぎていればすぐ 1 回呼ぶ', () => {
  const fn = vi.fn();
  runWhenDue(new AbortController().signal, 2_000, fn);
  setHidden(true);
  vi.advanceTimersByTime(600_000);
  expect(fn).toHaveBeenCalledTimes(0);
  setHidden(false);
  vi.advanceTimersByTime(0);
  expect(fn).toHaveBeenCalledTimes(1);
  vi.advanceTimersByTime(600_000);
  expect(fn).toHaveBeenCalledTimes(1);
});

it('戻ったときまだ期限前なら残りだけ待つ', () => {
  const fn = vi.fn();
  runWhenDue(new AbortController().signal, 10_000, fn);
  setHidden(true);
  vi.advanceTimersByTime(4_000);
  setHidden(false);
  vi.advanceTimersByTime(5_999);
  expect(fn).toHaveBeenCalledTimes(0);
  vi.advanceTimersByTime(1);
  expect(fn).toHaveBeenCalledTimes(1);
});

it('止めたあとは呼ばない', () => {
  const fn = vi.fn(); const ctl = new AbortController();
  runWhenDue(ctl.signal, 2_000, fn);
  ctl.abort();
  vi.advanceTimersByTime(10_000);
  expect(fn).toHaveBeenCalledTimes(0);
});

it('usePreparingRetry: 応答ごとに倍々の間隔で 1 回ずつ取り直し、準備が終われば止まる', () => {
  const times: number[] = [];
  let preparing = true;
  const start = Date.now();
  const { rerender } = renderHook(({ key }: { key: number }) => {
    usePreparingRetry(preparing, 3_000, key, () => { times.push(Date.now() - start); });
  }, { initialProps: { key: 0 } });
  // 取り直すたびに新しい応答 (key) が届く。
  for (let i = 1; i <= 5; i += 1) {
    act(() => { vi.advanceTimersByTime(60_000); });
    rerender({ key: i });
  }
  // 3 → 6 → 12 → 24 → 48 秒。各回は前の応答から数える (1 回の進みは 60 秒)。
  expect(times).toEqual([3_000, 66_000, 132_000, 204_000, 288_000]);
  preparing = false;
  rerender({ key: 6 });
  act(() => { vi.advanceTimersByTime(600_000); });
  expect(times).toHaveLength(5);
  // 準備が終わったら最初の 3 秒に戻る。
  preparing = true;
  rerender({ key: 7 });
  act(() => { vi.advanceTimersByTime(3_000); });
  expect(times).toHaveLength(6);
});

it('usePreparingRetry: 隠れている 1 時間は取り直さず、戻ったときすぐ 1 回', () => {
  const retry = vi.fn();
  renderHook(() => { usePreparingRetry(true, 2_000, 'preparing', retry); });
  act(() => { setHidden(true); });
  act(() => { vi.advanceTimersByTime(3_600_000); });
  expect(retry).toHaveBeenCalledTimes(0);
  act(() => { setHidden(false); });
  act(() => { vi.advanceTimersByTime(0); });
  expect(retry).toHaveBeenCalledTimes(1);
});

it('useReportPolling: 隠れている間は読まず、戻ったとき期限を過ぎていればすぐ読む', async () => {
  const load = vi.fn(() => Promise.resolve(true));
  renderHook(() => { useReportPolling(load); });
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  expect(load).toHaveBeenCalledTimes(1);
  act(() => { setHidden(true); });
  await act(async () => { await vi.advanceTimersByTimeAsync(3_600_000); });
  expect(load).toHaveBeenCalledTimes(1);
  act(() => { setHidden(false); });
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  expect(load).toHaveBeenCalledTimes(2);
  // 次は 4 秒後 (2 → 4 → 8 … 30 秒)。
  await act(async () => { await vi.advanceTimersByTimeAsync(3_999); });
  expect(load).toHaveBeenCalledTimes(2);
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  expect(load).toHaveBeenCalledTimes(3);
});
