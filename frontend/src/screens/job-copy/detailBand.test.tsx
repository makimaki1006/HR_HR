// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import { AssumptionsRail, DetailBand } from './DetailBand';
import { applicationsCutoffDate, bandNumbers, billingInWindow } from './detailBandModel';
import type { BillingEntry } from './timelineModel';

afterEach(() => { cleanup(); });

// 応募を確認した日は 2026-10-05。直近4週 = 09-08〜10-05、その前の4週 = 08-11〜09-07。
const byDate = {
  '2026-10-05': 5, '2026-09-20': 10, '2026-09-08': 5, // 直近4週 20
  '2026-09-07': 8, '2026-08-11': 5, // 前の4週 13
  '2026-08-10': 99, // どちらにも入らない
};
const job: JobCopyRecord = { ...jobs[0]!, // eslint-disable-line @typescript-eslint/no-non-null-assertion
   location: '大分県大分市', overallApplications: { total: 130, missingDate: 0, fetchedAt: '2026-10-05T09:00:00+09:00', distributions: {}, byDate } };
const bill = (start: string, end: string, amountYen: number | null): BillingEntry => ({ source: 'csv', start, end, amountYen });

describe('案件の上の帯の数字', () => {
  it('直近4週20件・前の4週13件・課金3万円なら応募1件あたり1,500円', () => {
    const n = bandNumbers(job, [bill('2026-09-08', '2026-10-05', 30000)]);
    expect(n).toMatchObject({ recent: 20, previous: 13, billingPer4WeeksYen: 30000, perApplicationYen: 1500, lastApplicationDate: '2026-10-05', lastBillingDate: '2026-10-05' });
  });
  it('14日の課金は4週あたりに直す', () => {
    expect(bandNumbers(job, [bill('2026-09-22', '2026-10-05', 15000)]).billingPer4WeeksYen).toBe(30000);
  });
  it('課金が無い・金額が空なら応募1件あたりも課金も不明', () => {
    expect(bandNumbers(job, undefined)).toMatchObject({ perApplicationYen: null, billingPer4WeeksYen: null, lastBillingDate: null });
    expect(bandNumbers(job, [bill('2026-09-08', '2026-10-05', null)])).toMatchObject({ perApplicationYen: null, billingPer4WeeksYen: null, lastBillingDate: '2026-10-05' });
  });
  it('同じ最終日で金額が違う課金は決めない', () => {
    expect(bandNumbers(job, [bill('2026-09-08', '2026-10-05', 30000), bill('2026-09-08', '2026-10-05', 20000)]).billingPer4WeeksYen).toBeNull();
  });
  it('応募の日別データが無ければ応募は不明', () => {
    const rest: JobCopyRecord = { ...job };
    delete rest.overallApplications;
    expect(bandNumbers(rest, undefined)).toMatchObject({ recent: null, previous: null, lastApplicationDate: null });
  });
});

describe('基準日と窓の課金', () => {
  const withApps = (fetchedAt: string, data: Record<string, number>): JobCopyRecord => ({ ...job, overallApplications: { total: 0, missingDate: 0, fetchedAt, distributions: {}, byDate: data } });
  it('窓の終わりは取得日。最後の応募の日ではない', () => {
    const j = withApps('2026-10-13T09:00:00+09:00', { '2026-10-03': 4 });
    expect(applicationsCutoffDate(j)).toBe('2026-10-13');
    expect(bandNumbers(j, undefined)).toMatchObject({ asOf: '2026-10-13', lastApplicationDate: '2026-10-03', recent: 4 });
  });
  it('取得日が10-10なら09-13は窓の内、10-11は外。直近111・前12', () => {
    const j = withApps('2026-10-10T09:00:00+09:00', { '2026-09-13': 111, '2026-10-11': 50, '2026-09-12': 12 });
    expect(bandNumbers(j, undefined)).toMatchObject({ asOf: '2026-10-10', recent: 111, previous: 12 });
  });
  it('取得日が無いときは今日で代用せず、数字は不明', () => {
    const j = withApps('', { '2026-10-03': 4 });
    expect(applicationsCutoffDate(j)).toBeNull();
    expect(bandNumbers(j, [bill('2026-09-01', '2026-09-30', 30000)])).toMatchObject({ asOf: null, recent: null, previous: null, perApplicationYen: null });
  });
  it('9月30,000円は、取得日10-13の窓（09-16〜10-13）では15,000円', () => {
    expect(billingInWindow([bill('2026-09-01', '2026-09-30', 30000)], '2026-09-16', '2026-10-13')).toBe(15000);
  });
  it('1件あたりは窓の課金15,000円÷窓の応募で、最後に1回だけ丸める', () => {
    const j = withApps('2026-10-13T09:00:00+09:00', { '2026-10-01': 7 });
    expect(bandNumbers(j, [bill('2026-09-01', '2026-09-30', 30000)]).perApplicationYen).toBe(2143);
  });
  it('窓の中に金額不明または重なる課金があれば不明', () => {
    expect(billingInWindow([bill('2026-09-01', '2026-09-30', 30000), bill('2026-10-01', '2026-10-31', null)], '2026-09-16', '2026-10-13')).toBeNull();
    expect(billingInWindow([bill('2026-09-01', '2026-09-30', 30000), bill('2026-09-20', '2026-10-31', 1000)], '2026-09-16', '2026-10-13')).toBeNull();
  });
});

describe('案件の上の帯と数字の前提の表示', () => {
  it('数字を表示し、全文を比べるボタンで呼び出しが走る', () => {
    const open = vi.fn();
    render(<DetailBand job={job} billing={[bill('2026-09-08', '2026-10-05', 30000)]} onOpenFullCompare={open} />);
    expect(screen.getByTestId('band-recent').textContent).toBe('20前の4週 13');
    expect(screen.getByTestId('band-per-application').textContent).toBe('¥1,500');
    expect(screen.getByTestId('band-billing').textContent).toBe('¥30,000');
    fireEvent.click(screen.getByRole('button', { name: '求人の全文を比べる' }));
    expect(open).toHaveBeenCalledTimes(1);
  });
  it('課金が不明なら「—」を出す', () => {
    render(<DetailBand job={job} billing={undefined} onOpenFullCompare={() => undefined} />);
    expect(screen.getByTestId('band-per-application').textContent).toBe('—');
    expect(screen.getByTestId('band-billing').textContent).toBe('—');
  });
  it('右の前提にデータの最後の日と、指定した職種・地域を出す', async () => {
    render(<AssumptionsRail job={job} billing={[bill('2026-09-08', '2026-10-05', 30000)]} mode="demo" onIndeedTitleChanged={() => undefined} />);
    expect(screen.getByTestId('rail-last-application').textContent).toBe('2026年10月5日');
    expect(screen.getByTestId('rail-last-billing').textContent).toBe('2026年10月5日');
    await waitFor(() => { expect(screen.getByText('都道府県：大分県')).toBeTruthy(); });
  });
});
