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
    expect((await screen.findByTestId('metric-outbound')).textContent).toBe('架電数52（チーム平均 47.5・3位/8人）着信 3');
    expect(screen.getByTestId('metric-over_90s').textContent).toBe('90秒以上11・21.2%（チーム平均 8.4・2位/8人）');
    expect(screen.getByTestId('metric-duration').textContent).toBe('通話時間55:12・1件 2:18（チーム平均 44:00・2位/8人）');
    expect(screen.getByTestId('metric-appointments').textContent).toBe('アポ2（チーム平均 1.1・1位/8人）');
    expect(screen.getByTestId('metrics-as-of').textContent).toBe('14時05分時点');
    expect(screen.getByText('今日の成績').getAttribute('title')).toBe('同じチーム（架空BPOチーム）で今日架電した 8 人と比べています。最初の架電 9:03・最後の架電 14:01。');
  });

  it('狭い幅で隠れる「着信 N」も、重ねたときの title に入っている', async () => {
    render(<TodayMetrics fixture />);
    expect((await screen.findByTestId('metric-outbound')).getAttribute('title')).toBe('架電数 52（チーム平均 47.5・3位/8人）・着信 3');
  });

  it('アポを数えないチームの人にはアポ欄を出さない', async () => {
    const f = () => Promise.resolve({ ok: true as const, data: { ...FIXTURE_TODAY, appointments_tracked: false } });
    render(<TodayMetrics fixture={false} fetcher={f} />);
    await screen.findByTestId('metric-outbound');
    expect(screen.queryByTestId('metric-appointments')).toBeNull();
    expect(screen.queryByText('アポ')).toBeNull();
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

  it('集計中は赤いエラーにせず、12 秒おきに取り直して数字が来たら表示する', async () => {
    vi.useFakeTimers();
    const loading = { ...FIXTURE_TODAY, loading: true, as_of: '', me: null, team_avg: null };
    const f = vi.fn()
      .mockResolvedValueOnce({ ok: true as const, data: loading })
      .mockResolvedValueOnce({ ok: true as const, data: loading })
      .mockResolvedValue({ ok: true as const, data: FIXTURE_TODAY });
    render(<TodayMetrics fixture={false} fetcher={f} />);
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    expect(screen.getByTestId('metrics-warming').textContent).toBe('今日の成績を集計しています…');
    expect(screen.queryByText('今日の成績を読み込めませんでした。')).toBeNull();
    expect(screen.queryByRole('button', { name: '再読み込み' })).toBeNull();
    await act(async () => { await vi.advanceTimersByTimeAsync(11_000); });
    expect(f).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(2_000); });
    expect(f).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId('metrics-warming')).toBeTruthy();
    await act(async () => { await vi.advanceTimersByTimeAsync(12_000); });
    expect(f).toHaveBeenCalledTimes(3);
    expect(screen.getByTestId('metric-outbound').textContent).toContain('52');
    expect(screen.queryByTestId('metrics-warming')).toBeNull();
  });

  it('本当の失敗は今のまま「読み込めませんでした」と再読み込みを出す', async () => {
    const f = () => Promise.resolve({ ok: false as const, error: new Error('boom') });
    render(<TodayMetrics fixture={false} fetcher={f} />);
    expect(await screen.findByText('今日の成績を読み込めませんでした。')).toBeTruthy();
    expect(screen.getByRole('button', { name: '再読み込み' })).toBeTruthy();
    expect(screen.queryByTestId('metrics-warming')).toBeNull();
  });
});
