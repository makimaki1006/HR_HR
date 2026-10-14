// @vitest-environment happy-dom
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { FIXTURE_MONTH, FIXTURE_MONTH_ALL, FIXTURE_TODAY, FIXTURE_TODAY_SALES, monthName, monthStripItems, sortMonthRows } from './metricsModel';
import { TodayMetrics } from './TodayMetrics';

afterEach(() => { cleanup(); vi.useRealTimers(); window.localStorage.clear(); });

const ok = <T,>(data: T) => () => Promise.resolve({ ok: true as const, data });

describe('新規営業の今月の帯', () => {
  it('今月の数字を主にし、今日の架電数を小さく添える。30/90/360 秒の項目は出さない', async () => {
    render(<TodayMetrics fixture={false} fetcher={ok(FIXTURE_TODAY_SALES)} monthFetcher={ok(FIXTURE_MONTH)} />);
    await screen.findByText('10月の成績');
    const outbound = screen.getByTestId('metric-outbound');
    expect(outbound.textContent).toBe('架電数812（チーム平均 760.4・3位/14人）今日 52');
    expect(screen.getByTestId('metric-today').textContent).toBe('今日 52');
    expect(screen.getByTestId('metric-duration').textContent).toBe('通話時間10:15:00・1件 2:03（チーム平均 9:30:00・2位/14人）');
    expect(screen.getByTestId('metric-appointments').textContent).toBe('アポ9（チーム平均 6.2・1位/14人）');
    expect(screen.getByText('10月の成績')).toBeTruthy();
    expect(screen.queryByText('90秒以上')).toBeNull();
    expect(screen.queryByText('今日の成績')).toBeNull();
    expect(screen.getByTestId('metrics-as-of').textContent).toBe('14時05分時点');
  });

  it('BPO の人は今日の帯のまま。今月は読みにいかない', async () => {
    const month = vi.fn(ok(FIXTURE_MONTH));
    render(<TodayMetrics fixture={false} fetcher={ok(FIXTURE_TODAY)} monthFetcher={month} />);
    expect((await screen.findByTestId('metric-outbound')).textContent).toBe('架電数52（チーム平均 47.5・3位/8人）着信 3');
    expect(screen.getByText('今日の成績')).toBeTruthy();
    expect(screen.getByTestId('metric-over_90s')).toBeTruthy();
    expect(month).not.toHaveBeenCalled();
  });

  it('今月が集計中のあいだは今日の帯と案内を出し、数字が来たら今月に替わる', async () => {
    vi.useFakeTimers();
    const loading = { ...FIXTURE_MONTH, loading: true, as_of: '', me: null, team_avg: null };
    const month = vi.fn()
      .mockResolvedValueOnce({ ok: true as const, data: loading })
      .mockResolvedValue({ ok: true as const, data: FIXTURE_MONTH });
    render(<TodayMetrics fixture={false} fetcher={ok(FIXTURE_TODAY_SALES)} monthFetcher={month} />);
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    expect(screen.getByTestId('month-warming')).toBeTruthy();
    expect(screen.getByText('今日の成績')).toBeTruthy();
    await act(async () => { await vi.advanceTimersByTimeAsync(13_000); });
    expect(screen.getByText('10月の成績')).toBeTruthy();
    expect(screen.queryByTestId('month-warming')).toBeNull();
  });

  it('今月を読めないときは今日の帯のまま', async () => {
    const month = () => Promise.resolve({ ok: false as const, error: new Error('x') as never });
    render(<TodayMetrics fixture={false} fetcher={ok(FIXTURE_TODAY_SALES)} monthFetcher={month} />);
    expect(await screen.findByText('今日の成績')).toBeTruthy();
  });
});

describe('今月の表示用の計算', () => {
  it('月の名前と、担当者が引けないときは null', () => {
    expect(monthName('2026-10')).toBe('10月');
    expect(monthName('x')).toBe('今月');
    expect(monthStripItems({ ...FIXTURE_MONTH, me: null })).toBeNull();
  });

  it('順位なしの人は「順位なし」', () => {
    const me = FIXTURE_MONTH.me;
    if (me === null) throw new Error('fixture');
    const items = monthStripItems({ ...FIXTURE_MONTH, me: { ...me, rank: { outbound: null, total_duration: null, appointments: null } } });
    expect(items?.[0]?.compare).toBe('チーム平均 760.4・順位なし');
  });

  it('管理者の表の並べ替え。値の無い行は最後', () => {
    expect(sortMonthRows(FIXTURE_MONTH_ALL.callers, 'avg_duration', 'desc').at(-1)?.name).toBe('架空 三郎');
    expect(sortMonthRows(FIXTURE_MONTH_ALL.callers, 'avg_duration', 'asc').at(-1)?.name).toBe('架空 三郎');
    expect(sortMonthRows(FIXTURE_MONTH_ALL.callers, 'appointments', 'desc')[0]?.name).toBe('サンプル 花子');
  });
});
