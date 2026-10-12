// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { FIXTURE_TODAY } from './metricsModel';
import { TodayMetrics } from './TodayMetrics';
import { fixtureTodayFetch } from './useTodayMetrics';

afterEach(() => { cleanup(); vi.useRealTimers(); window.localStorage.clear(); });

describe('今日の成績の帯', () => {
  it('値・平均・順位・時点を出す', async () => {
    render(<TodayMetrics fixture />);
    expect(await screen.findByTestId('metric-outbound')).toHaveProperty('textContent', '架電数52着信 3平均 47.53位 / 8人');
    expect(screen.getByTestId('metric-over_90s').textContent).toBe('90秒以上1121.2%平均 8.42位 / 8人');
    expect(screen.getByTestId('metric-duration').textContent).toBe('通話時間合計55:12平均 2:18平均 44:002位 / 8人');
    expect(screen.getByTestId('metric-appointments').textContent).toBe('アポ2平均 1.11位 / 8人');
    expect(screen.getByTestId('metrics-as-of').textContent).toBe('14時05分時点');
    expect(screen.getByTestId('metrics-population').textContent).toContain('同じチーム 8人と比較');
    expect(screen.getByTestId('metrics-population').getAttribute('title')).toBe('同じチーム（架空BPOチーム）で今日架電した 8 人と比べています。最初の架電 9:03・最後の架電 14:01。');
  });

  it('担当者が引けないときは理由を出す', async () => {
    const f = () => Promise.resolve({ ok: true as const, data: { ...FIXTURE_TODAY, me: null, message: '結び付けられません' } });
    render(<TodayMetrics fixture={false} fetcher={f} />);
    expect(await screen.findByText('結び付けられません')).toBeTruthy();
  });

  it('閉じると項目を隠し、状態を覚える', async () => {
    render(<TodayMetrics fixture />);
    await screen.findByTestId('metric-outbound');
    fireEvent.click(screen.getByRole('button', { name: '閉じる' }));
    expect(screen.queryByTestId('metric-outbound')).toBeNull();
    expect(window.localStorage.getItem('hrhr.crm.todayMetrics.collapsed')).toBe('1');
  });

  it('2 分ごとに読み直し、隠れている間は止める', async () => {
    vi.useFakeTimers();
    const f = vi.fn(fixtureTodayFetch);
    render(<TodayMetrics fixture={false} fetcher={f} />);
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    expect(f).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(119_000); });
    expect(f).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(2_000); });
    expect(f).toHaveBeenCalledTimes(2);
    // 隠れる → 止まる
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
    await act(async () => { document.dispatchEvent(new Event('visibilitychange')); await vi.advanceTimersByTimeAsync(300_000); });
    expect(f).toHaveBeenCalledTimes(2);
    // 見えたら (2 分以上たっているので) すぐ読み直す
    Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true });
    await act(async () => { document.dispatchEvent(new Event('visibilitychange')); await vi.advanceTimersByTimeAsync(10); });
    expect(f).toHaveBeenCalledTimes(3);
  });

  it('読み直しに失敗しても前回の数字を残して知らせる', async () => {
    vi.useFakeTimers();
    let n = 0;
    const f = () => Promise.resolve(++n === 1 ? { ok: true as const, data: FIXTURE_TODAY } : { ok: false as const, error: new Error('x') as never });
    render(<TodayMetrics fixture={false} fetcher={f} />);
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    await act(async () => { await vi.advanceTimersByTimeAsync(121_000); });
    expect(screen.getByTestId('metric-outbound')).toBeTruthy();
    expect(screen.getByText(/前回の数字を表示中/)).toBeTruthy();
  });
});
