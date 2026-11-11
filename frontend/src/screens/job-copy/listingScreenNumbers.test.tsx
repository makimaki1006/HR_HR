// @vitest-environment happy-dom
// 求人の画面の追加分: 上の帯の掲載開始日・右の欄の「市場の今」・週ごとの表の番号・出来事の前後4週の課金と応募1件あたり。
// 値はすべて架空。
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { jobs } from './data';
import type { CopyVersion, JobCopyRecord } from './data';
import { AssumptionsRail, DetailBand } from './DetailBand';
import { listingPeriodText, marketMonthOverMonth, signedPct } from './detailBandModel';
import { EventCostTable, LifelineChart, ListingOverview } from './ListingOverview';
import { buildLifeline, changeMarkerNumbers } from './lifelineModel';
import { buildListingEvents, eventCostNotes, eventCosts, eventCounts, listingPeriodOf } from './listingEventsModel';
import type { BillingEntry } from './timelineModel';

afterEach(() => { cleanup(); });

const bill = (start: string, end: string, amountYen: number | null): BillingEntry => ({ source: 'csv', start, end, amountYen });
const version = (id: string, publishedFrom: string, salary: string, extra = ''): CopyVersion => ({
  id, label: id, observedAt: `${publishedFrom}T09:00:00+09:00`, publishedFrom: `${publishedFrom}T09:00:00+09:00`, certainty: 'confirmed', kind: 'published', source: 'test',
  body: `仕事内容\n決まったルートで配送します。${extra}\n給与：${salary}\n休日：土日祝`, applications: null, note: '',
});

// 取得日 2026-10-09。応募は 8/05〜9/01 に 1 日 1 件、9/02〜10/09 に 1 日 2 件（listingEvents.test.ts と同じ）。
const byDate: Record<string, number> = {};
for (let day = new Date('2026-06-10T00:00:00Z'); day <= new Date('2026-10-09T00:00:00Z'); day = new Date(day.getTime() + 86_400_000)) {
  const date = day.toISOString().slice(0, 10);
  byDate[date] = date >= '2026-09-02' ? 2 : date >= '2026-08-05' ? 1 : 0;
}
const job: JobCopyRecord = {
  id: 'j', title: '配送ドライバー', company: '架空運輸', media: 'HRハッカー', mediaJobId: 'M1', location: '大分県大分市', dataSource: 'hubspot', listingPublicationStatus: '公開',
  overallApplications: { total: 0, missingDate: 0, fetchedAt: '2026-10-09T09:00:00+09:00', distributions: {}, byDate },
  versions: [
    version('v1', '2026-06-10', '時給 1,100 円'),
    version('v2', '2026-08-05', '時給 1,200 円'),
    version('v3', '2026-09-12', '時給 1,200 円', '1日平均40件を担当します。'),
  ],
};
// 6/10〜8/09 の 61 日で 61,000 円（1 日 1,000 円）、8/10〜10/09 の 61 日で 30,500 円（1 日 500 円）。
const billing = [bill('2026-06-10', '2026-08-09', 61000), bill('2026-08-10', '2026-10-09', 30500)];
const events = buildListingEvents(job, billing);
const at = (n: number) => { const event = events.find(item => item.n === n); if (!event) throw new Error(`出来事${String(n)}がありません`); return event; };

describe('上の帯の掲載の期間', () => {
  it('掲載中なら開始日だけ「（6/10〜）」、掲載終了なら終了日まで', () => {
    expect(listingPeriodText('2026-06-10', null, '2026-11-12')).toBe('（6/10〜）');
    expect(listingPeriodText('2026-06-10', '2026-09-01', '2026-11-12', true)).toBe('（6/10〜9/1）');
  });
  it('掲載中なのに終了日がある（終了の出来事のあと公開状況だけ戻った）ときは、終了日を出さない', () => {
    expect(listingPeriodText('2026-06-10', '2026-09-01', '2026-11-12', false)).toBe('（6/10〜）');
  });
  it('掲載終了なのに終了の出来事が無いときは、終了日の記録が無いと書く', () => {
    expect(listingPeriodText('2026-06-10', null, '2026-11-12', true)).toBe('（6/10〜、終了日の記録なし）');
  });
  it('今年でない日は年を付ける。開始日が分からないときは何も出さない', () => {
    expect(listingPeriodText('2025-12-03', null, '2026-11-12')).toBe('（2025/12/3〜）');
    expect(listingPeriodText(null, null, '2026-11-12')).toBe('');
    expect(listingPeriodText('不明', null, '2026-11-12')).toBe('');
  });
  it('出来事から開始・終了を決める。復活したあとは終了日を出さない', () => {
    expect(listingPeriodOf(events)).toEqual({ start: '2026-06-10', end: null });
    expect(listingPeriodOf([{ date: '2026-06-10', kind: 'start' }, { date: '2026-08-01', kind: 'end' }])).toEqual({ start: '2026-06-10', end: '2026-08-01' });
    expect(listingPeriodOf([{ date: '2026-06-10', kind: 'start' }, { date: '2026-08-01', kind: 'end' }, { date: '2026-08-20', kind: 'revive' }])).toEqual({ start: '2026-06-10', end: null });
    expect(listingPeriodOf([{ date: '2026-08-01', kind: 'content' }])).toEqual({ start: null, end: null });
  });
  it('帯に「HRハッカー · 公開状況：掲載中（6/10〜）」と出す。開始日が無ければ期間を出さない', () => {
    vi.useFakeTimers({ now: new Date('2026-11-12T03:00:00Z'), toFake: ['Date'] });
    try {
      render(<DetailBand job={job} billing={billing} listingStart="2026-06-10" listingEnd={null} onOpenFullCompare={() => undefined} />);
      expect(screen.getByTestId('band-status').textContent).toBe('HRハッカー · 公開状況：掲載中（6/10〜）');
      cleanup();
      render(<DetailBand job={job} billing={billing} onOpenFullCompare={() => undefined} />);
      expect(screen.getByTestId('band-status').textContent).toBe('HRハッカー · 公開状況：掲載中');
      cleanup();
      // 掲載中なのに終了日がある組み合わせ: 終了日は出さない
      render(<DetailBand job={job} billing={billing} listingStart="2026-06-10" listingEnd="2026-09-01" onOpenFullCompare={() => undefined} />);
      expect(screen.getByTestId('band-status').textContent).toBe('HRハッカー · 公開状況：掲載中（6/10〜）');
      cleanup();
      const ended: JobCopyRecord = { ...job, listingPublicationStatus: '非公開' };
      render(<DetailBand job={ended} billing={billing} listingStart="2026-06-10" listingEnd="2026-09-01" onOpenFullCompare={() => undefined} />);
      expect(screen.getByTestId('band-status').textContent).toBe('HRハッカー · 公開状況：掲載終了（6/10〜9/1）');
      cleanup();
      // 掲載終了なのに終了の出来事が無い組み合わせ
      render(<DetailBand job={ended} billing={billing} listingStart="2026-06-10" listingEnd={null} onOpenFullCompare={() => undefined} />);
      expect(screen.getByTestId('band-status').textContent).toBe('HRハッカー · 公開状況：掲載終了（6/10〜、終了日の記録なし）');
    } finally { vi.useRealTimers(); }
  });
});

describe('右の欄の「市場の今」', () => {
  it('最新の月と前の月の求人数から前月比を出す', () => {
    expect(marketMonthOverMonth([{ month: '2026-07', jobs: 106 }, { month: '2026-08', jobs: 111 }, { month: '2026-09', jobs: null }])).toEqual({ month: '2026-08', jobs: 111, previousMonth: '2026-07', previousJobs: 106, changePct: (111 - 106) / 106 * 100 });
    expect(signedPct((111 - 106) / 106 * 100)).toBe('+5%');
    expect(signedPct(-12.4)).toBe('−12%');
    expect(signedPct(0.2)).toBe('±0%');
  });
  it('年をまたぐ前の月・前の月の数字が無い・前の月が0件', () => {
    expect(marketMonthOverMonth([{ month: '2025-12', jobs: 50 }, { month: '2026-01', jobs: 40 }])).toMatchObject({ previousMonth: '2025-12', previousJobs: 50, changePct: -20 });
    expect(marketMonthOverMonth([{ month: '2026-08', jobs: 111 }])).toMatchObject({ previousJobs: null, changePct: null });
    expect(marketMonthOverMonth([{ month: '2026-07', jobs: 0 }, { month: '2026-08', jobs: 3 }])).toMatchObject({ previousJobs: 0, changePct: null });
    expect(marketMonthOverMonth([{ month: '2026-08', jobs: null }])).toBeNull();
  });
  it('職種を指定した求人は、競合の求人数と前月比、「市場の推移を見る」を出す（デモの市場データ: ドライバー・大分県）', async () => {
    const open = vi.fn();
    const demo = jobs[0]!; // eslint-disable-line @typescript-eslint/no-non-null-assertion
    render(<AssumptionsRail job={demo} billing={undefined} mode="demo" onIndeedTitleChanged={() => undefined} onOpenMarket={open} />);
    await waitFor(() => { expect(screen.getByTestId('rail-market-jobs').textContent).toBe('111件'); });
    const section = screen.getByRole('region', { name: '市場の今' });
    expect(within(section).getByText('競合の求人数（2026年8月）')).toBeTruthy();
    expect(within(section).getByText('前月比（2026年7月と比べて）')).toBeTruthy();
    expect(screen.getByTestId('rail-market-change').textContent).toBe('+5%（106件 → 111件）');
    expect(section.textContent).not.toContain('時給');
    fireEvent.click(within(section).getByRole('button', { name: '市場の推移を見る' }));
    expect(open).toHaveBeenCalledTimes(1);
  });
  it('職種が未指定なら案内だけを出し、市場の数字は取りに行かない', () => {
    const unset: JobCopyRecord = { ...jobs[0]!, indeedTitle: null, indeedTitleState: 'unset' }; // eslint-disable-line @typescript-eslint/no-non-null-assertion
    render(<AssumptionsRail job={unset} billing={undefined} mode="demo" onIndeedTitleChanged={() => undefined} onOpenMarket={() => undefined} />);
    expect(screen.getByTestId('rail-market-note').textContent).toBe('市場と比べる職種が未指定です。上の「市場（Indeed）と比べる範囲」で職種を指定すると、競合の求人数と前月比が出ます。');
    expect(screen.queryByTestId('rail-market-jobs')).toBeNull();
  });
  it('市場と比べないと指定した求人は、そのことだけを出す', () => {
    const none: JobCopyRecord = { ...jobs[0]!, indeedTitle: null, indeedTitleState: 'none' }; // eslint-disable-line @typescript-eslint/no-non-null-assertion
    render(<AssumptionsRail job={none} billing={undefined} mode="demo" onIndeedTitleChanged={() => undefined} />);
    expect(screen.getByTestId('rail-market-note').textContent).toBe('市場と比べないと指定されています。');
  });
});

describe('出来事の前後4週の課金と応募1件あたり', () => {
  const asOf = '2026-10-09';
  it('9/12 の本文の変更: 前（8/15〜9/11）課金 14,000 円・応募 38 件、後（9/12〜10/09）課金 14,000 円・応募 56 件', () => {
    const counts = eventCounts(job, at(5), asOf, '2026-06-10');
    expect(eventCosts(at(5), counts, billing)).toEqual({ before: { billing: 14000, perApplication: 368, reason: null }, after: { billing: 14000, perApplication: 250, reason: null }, billingThrough: '2026-10-09' });
  });
  it('8/05 の給与の変更: 前（7/08〜8/04）は応募0件なので1件あたりなし、後（8/05〜9/01）は 5×1,000 + 23×500 = 16,500 円 ÷ 28 件', () => {
    const counts = eventCounts(job, at(3), asOf, '2026-06-10');
    expect(counts).toMatchObject({ before: 0, after: 28 });
    expect(eventCosts(at(3), counts, billing)).toEqual({ before: { billing: 28000, perApplication: null, reason: 'no_applications' }, after: { billing: 16500, perApplication: 589, reason: null }, billingThrough: '2026-10-09' });
  });
  it('掲載開始は前が無い。課金のデータが無ければ課金も1件あたりも出さない', () => {
    expect(eventCosts(at(1), eventCounts(job, at(1), asOf, '2026-06-10'), billing).before).toBeNull();
    expect(eventCosts(at(5), eventCounts(job, at(5), asOf, '2026-06-10'), [])).toEqual({ before: { billing: null, perApplication: null, reason: 'no_billing' }, after: { billing: null, perApplication: null, reason: 'no_billing' }, billingThrough: null });
  });
  it('取得日で後の4週が切れていれば、その日数分の課金だけを足す（9/30 の出来事: 9/30〜10/09 の10日 × 500 円）', () => {
    const counts = eventCounts(job, { date: '2026-09-30', kind: 'content' }, asOf, '2026-06-10');
    expect(counts).toMatchObject({ after: 20, afterDays: 10 });
    expect(eventCosts({ date: '2026-09-30' }, counts, billing).after).toEqual({ billing: 5000, perApplication: 250, reason: null });
  });
  it('課金のデータが後の4週の途中（10/05）までなら、後の4週の課金と1件あたりは出さず、理由を書く', () => {
    // 8/10〜10/05 の 57 日で 28,500 円（1 日 500 円）。9/12 の出来事の後の4週は 9/12〜10/09 なので、10/06〜10/09 の課金が無い。
    const cut = [bill('2026-06-10', '2026-08-09', 61000), bill('2026-08-10', '2026-10-05', 28500)];
    const counts = eventCounts(job, at(5), asOf, '2026-06-10');
    const costs = eventCosts(at(5), counts, cut);
    // 途中までの額（24 日 × 500 = 12,000 円 ÷ 56 件 = 214 円）にすると安く出るので出さない。前の4週（8/15〜9/11）は課金のデータの内側。
    expect(costs).toEqual({ before: { billing: 14000, perApplication: 368, reason: null }, after: { billing: null, perApplication: null, reason: 'billing_ends' }, billingThrough: '2026-10-05' });
    expect(eventCostNotes(at(5), counts, costs, asOf)).toEqual(['課金のデータは10/05までのため、後の4週の課金と1件あたりは出せません。']);
    render(<ListingOverview job={job} events={events} billing={cut} market={{ state: 'unset' }} selectedId={at(5).id} onSelect={() => undefined} onCompare={() => undefined} onOpenDrawer={() => undefined} />);
    expect(screen.getByTestId('cost-after-billing').textContent).toBe('—');
    expect(screen.getByTestId('cost-after-per-application').textContent).toBe('—');
    expect(screen.getByTestId('cost-before-per-application').textContent).toBe('¥368');
    expect(screen.getByTestId('event-cost-notes').textContent).toBe('課金のデータは10/05までのため、後の4週の課金と1件あたりは出せません。');
  });
  it('掲載から28日たたない出来事は、前の窓を掲載を始めた日から数える（見出しの「掲載後N日分」と同じ日数）', () => {
    // 掲載開始を 8/29 とする。9/12 の出来事の前は 8/29〜9/11 の 14 日: 応募 1×4 + 2×10 = 24 件、課金 14 × 500 = 7,000 円。
    const counts = eventCounts(job, at(5), asOf, '2026-08-29');
    expect(counts).toEqual({ before: 24, after: 56, beforeDays: 14, afterDays: 28 });
    expect(eventCosts(at(5), counts, billing).before).toEqual({ billing: 7000, perApplication: 292, reason: null });
    const shortStart = events.map(event => event.kind === 'start' ? { ...event, date: '2026-08-29' } : event);
    render(<ListingOverview job={job} events={shortStart} billing={billing} market={{ state: 'unset' }} selectedId={at(5).id} onSelect={() => undefined} onCompare={() => undefined} onOpenDrawer={() => undefined} />);
    const heads = [...screen.getByTestId('event-costs').querySelectorAll('thead th')].map(element => element.textContent);
    expect(heads.slice(1)).toEqual(['前の4週（掲載後14日分）', '後の4週']);
    expect(screen.getByTestId('cost-before-applications').textContent).toBe('24件');
    expect(screen.getByTestId('cost-before-billing').textContent).toBe('¥7,000');
    // 一覧の行も同じ窓・同じ言い方
    expect(document.querySelector('.lo-open .lo-result')?.textContent).toBe('24 → 56 件差 +32 件（前は掲載後14日分）');
  });
  it('後の4週が無い出来事（応募を取得した日より後）は、その理由を書く。注記が2つなら1つずつ別の行に出す', () => {
    const late = { date: '2026-10-12', kind: 'content' as const };
    const counts = eventCounts(job, late, asOf, '2026-06-10');
    // 前の4週 9/14〜10/11 のうち応募があるのは取得日 10/09 まで: 26 日 × 2 = 52 件
    expect(counts).toMatchObject({ before: 52, after: null, beforeDays: 28 });
    const costs = eventCosts(late, counts, billing);
    // 前の4週は 9/14〜10/11 で、課金のデータ（10/09 まで）を越えるので出さない
    expect(costs).toMatchObject({ before: { reason: 'billing_ends' }, after: null });
    expect(eventCostNotes(late, counts, costs, asOf)).toEqual([
      '応募を取得した日（10/09）より後の出来事なので、後の4週はまだありません。',
      '課金のデータは10/09までのため、前の4週の課金と1件あたりは出せません。',
    ]);
    render(<EventCostTable event={late} counts={counts} costs={costs} asOf={asOf} />);
    expect([...screen.getByTestId('event-cost-notes').querySelectorAll('li')].map(element => element.textContent)).toEqual([
      '応募を取得した日（10/09）より後の出来事なので、後の4週はまだありません。',
      '課金のデータは10/09までのため、前の4週の課金と1件あたりは出せません。',
    ]);
    expect(screen.getByTestId('cost-after-applications').textContent).toBe('—');
  });
  it('出来事を開くと、前後4週の応募・課金・応募1件あたりの表を出す', () => {
    render(<ListingOverview job={job} events={events} billing={billing} market={{ state: 'unset' }} selectedId={at(3).id} onSelect={() => undefined} onCompare={() => undefined} onOpenDrawer={() => undefined} />);
    expect(screen.getByTestId('cost-before-applications').textContent).toBe('0件');
    expect(screen.getByTestId('cost-after-applications').textContent).toBe('28件');
    expect(screen.getByTestId('cost-before-billing').textContent).toBe('¥28,000');
    expect(screen.getByTestId('cost-after-billing').textContent).toBe('¥16,500');
    expect(screen.getByTestId('cost-before-per-application').textContent).toBe('—');
    expect(screen.getByTestId('cost-after-per-application').textContent).toBe('¥589');
    const table = screen.getByTestId('event-costs');
    expect(screen.getByTestId('event-cost-notes').textContent).toBe('前の4週は応募が0件のため、1件あたりは出せません。');
    expect(table.parentElement?.textContent).not.toMatch(/増えた|減った|見直/u);
  });
});

describe('週ごとの表の変更の印の番号', () => {
  it('番号の書き方: 1件・2件・3件以上', () => {
    expect(changeMarkerNumbers([{ n: 4 }])).toBe('4');
    expect(changeMarkerNumbers([{ n: 4 }, { n: 7 }])).toBe('4・7');
    expect(changeMarkerNumbers([{ n: 4 }, { n: 7 }, { n: 9 }])).toBe('4ほか2');
  });
  it('印の番号は、出来事の一覧の番号と同じ（8/05 の給与の変更は 3、9/12 の本文の変更は 5）', () => {
    const lifeline = buildLifeline(job, events, billing, new Date('2026-10-09T03:00:00Z'));
    if (!lifeline) throw new Error('ライフラインが作れません');
    render(<LifelineChart lifeline={lifeline} job={job} selectedWeek={null} onSelect={() => undefined} />);
    expect(screen.getAllByTestId('lifeline-change-num').map(element => element.textContent)).toEqual(['3', '5']);
    expect(screen.getByRole('button', { name: /^出来事3 2026-08-05 給与を変更/u })).toBeTruthy();
    cleanup();
    render(<ListingOverview job={job} events={events} billing={billing} market={{ state: 'unset' }} selectedId={null} onSelect={() => undefined} onCompare={() => undefined} onOpenDrawer={() => undefined} />);
    const listed = [...document.querySelectorAll('.lo-event')].map(element => [element.querySelector('.lo-num')?.textContent, element.querySelector('.lo-date')?.textContent]);
    expect(listed).toContainEqual(['3', '8/05']);
    expect(listed).toContainEqual(['5', '9/12']);
  });
});
