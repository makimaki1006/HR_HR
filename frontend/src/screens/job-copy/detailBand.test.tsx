// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import { DetailBand, MarketNowBar } from './DetailBand';
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
    expect(n).toMatchObject({ recent: 20, previous: 13, billingRecentYen: 30000, perApplicationYen: 1500, lastApplicationDate: '2026-10-05', lastBillingDate: '2026-10-05' });
  });
  it('14日の課金は4週あたりに直さず、直近4週に入る分だけ足す', () => {
    expect(bandNumbers(job, [bill('2026-09-22', '2026-10-05', 15000)]).billingRecentYen).toBe(15000);
  });
  it('課金が無い・金額が空なら応募1件あたりも課金も不明', () => {
    expect(bandNumbers(job, undefined)).toMatchObject({ perApplicationYen: null, billingRecentYen: null, lastBillingDate: null });
    expect(bandNumbers(job, [bill('2026-09-08', '2026-10-05', null)])).toMatchObject({ perApplicationYen: null, billingRecentYen: null, lastBillingDate: '2026-10-05' });
  });
  it('同じ最終日で金額が違う課金は決めない', () => {
    expect(bandNumbers(job, [bill('2026-09-08', '2026-10-05', 30000), bill('2026-09-08', '2026-10-05', 20000)]).billingRecentYen).toBeNull();
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

describe('求人のページの上の帯の表示', () => {
  it('数字を単位付きで表示し、全文を比べるボタンで呼び出しが走る', () => {
    const open = vi.fn();
    const { container } = render(<DetailBand job={job} billing={[bill('2026-09-08', '2026-10-05', 30000)]} onOpenFullCompare={open} />);
    expect(screen.getByTestId('band-recent').textContent).toBe('20件9/8〜10/5・前の4週は13件');
    expect(screen.getByText('応募（直近4週）')).toBeTruthy();
    expect(screen.getByText('応募1件の費用')).toBeTruthy();
    expect(screen.getByTestId('band-per-application').textContent).toBe('¥1,500直近4週の課金 ÷ 応募');
    // 課金は応募と同じ直近4週の28日。
    expect(screen.getByText('課金（直近4週）')).toBeTruthy();
    expect(screen.getByTestId('band-billing').textContent).toBe('¥30,000');
    // データの最後の日は帯の中の1行。右の「数字の前提」欄はない。
    expect(screen.getByTestId('rail-last-application').textContent).toMatch(/^最後の応募：(2026\/)?10\/5$/);
    expect(screen.getByTestId('rail-last-billing').textContent).toMatch(/^最後の課金：(2026\/)?10\/5$/);
    expect(container.querySelector('[aria-label="数字の前提"]')).toBeNull();
    expect(screen.queryByRole('button', { name: 'この求人から新しいパターンを作る' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '全文を比べる' }));
    expect(open).toHaveBeenCalledTimes(1);
  });
  it('常に使う 3 つを帯に並べ、本文の状態と会社・地域は押せない文字で出す', () => {
    const listing = vi.fn();
    render(<DetailBand job={{ ...job, company: '架空運輸株式会社' }} billing={undefined} onOpenFullCompare={() => undefined} onOpenListing={listing} reportHref="/app/job-copy/report?listing=1" bodyState="前の本文と同じ" />);
    fireEvent.click(screen.getByRole('button', { name: '求人の内容' }));
    expect(listing).toHaveBeenCalledTimes(1);
    const report = screen.getByRole('link', { name: '報告書（新しいタブで開きます）' });
    expect(report.getAttribute('target')).toBe('_blank');
    expect(report.textContent).toBe('↗ 報告書');
    expect(screen.getByTestId('band-body-state').textContent).toBe('本文：前の本文と同じ');
    expect(screen.getByTestId('band-body-state').closest('button')).toBeNull();
    expect(screen.getByTestId('band-place').textContent).toBe('架空運輸株式会社\u00a0・ 大分県大分市');
  });
  it('報告書の行き先が無い求人（HubSpot 以外）には報告書を出さない', () => {
    render(<DetailBand job={job} billing={undefined} onOpenFullCompare={() => undefined} onOpenListing={() => undefined} />);
    expect(screen.queryByRole('link', { name: /報告書/ })).toBeNull();
    expect(screen.getByRole('button', { name: '全文を比べる' })).toBeTruthy();
  });
  it('課金が不明なら「—」を出す', () => {
    render(<DetailBand job={job} billing={undefined} onOpenFullCompare={() => undefined} />);
    expect(screen.getByTestId('band-per-application').textContent).toBe('—課金データなし');
    expect(screen.getByText('課金（直近4週）')).toBeTruthy();
    expect(screen.getByTestId('band-billing').textContent).toBe('—課金データなし');
    expect(screen.getByTestId('rail-last-billing').textContent).toBe('最後の課金：なし');
  });
  it('「市場の今」に指定した職種・地域を出す', async () => {
    render(<MarketNowBar job={job} mode="demo" onIndeedTitleChanged={() => undefined} />);
    expect(screen.getByRole('region', { name: '市場の今' })).toBeTruthy();
    await waitFor(() => { expect(screen.getByText('都道府県：大分県')).toBeTruthy(); });
  });
});

describe('応募を読み込めなかったとき', () => {
  it('「読み込み中」のまま止めず、読み込めなかったことと再試行のボタンを出す', () => {
    const failed: JobCopyRecord = { ...job, hubspotId: '555', applicationDates: 'failed' };
    delete failed.overallApplications;
    const heard = vi.fn();
    const listen = (event: Event) => { heard((event as CustomEvent<string>).detail); };
    window.addEventListener('jobcopy:retry-applicants', listen);
    render(<DetailBand job={failed} billing={undefined} onOpenFullCompare={() => undefined} />);
    expect(screen.getByRole('alert').textContent).toContain('応募を読み込めませんでした。');
    expect(screen.queryByText('応募を読み込み中です。')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'もう一度読み込む' }));
    window.removeEventListener('jobcopy:retry-applicants', listen);
    expect(heard).toHaveBeenCalledWith('555');
  });
});
