// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { DealBoard } from './DealBoard';
import { boardFixture, boardRows } from './dealBoard.testFixtures';
import type { DealRow } from './dealBoardModel';
import { demoOwnerTodo } from './demoOwnerTodo';
import { yen } from './homeModel';

vi.mock('./ProposalMemo', () => ({ ProposalMemo: () => <p>案件のメモ</p> }));
vi.mock('./NextTodo', () => ({ NextTodoButton: () => <button>次の予定を記録</button>, ReasonChips: () => null }));
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

const row = (i: number): DealRow => {
  const value = boardRows[i];
  if (!value) throw new Error('fixture missing');
  return value;
};
// 面接の数: 100=0 / 101=空 / 102=0 / 103=3。応募: 100=契約期間 0 件・期間 0 件、101=期間 2 件、102=応募日が不明、103=求人票なし。
const rows: DealRow[] = [
  { ...row(0), interviews: 0, listing_count: 2, contract_applications: 0, period_applications: 0, first_contract: true },
  { ...row(1), interviews: null, listing_count: 1, contract_applications: 5, period_applications: 2, first_contract: true },
  { ...row(2), interviews: 0, listing_count: 1, contract_applications: null, period_applications: null, first_contract: false },
  { ...row(3), interviews: 3, listing_count: null, contract_applications: null, period_applications: null, first_contract: false },
];
function stub(goodIds: string[] | 'fail' = ['101', '103']) {
  const urls: string[] = [];
  vi.stubGlobal('fetch', vi.fn((input: string) => {
    urls.push(input);
    const u = new URL(input, location.origin);
    if (u.pathname.endsWith('/consultant-good')) {
      if (goodIds === 'fail') return Promise.resolve(new Response('{}', { status: 500 }));
      return Promise.resolve(Response.json({ status: 'ready', data_day: '2026-10-20', previous_day: null, comparison: 'unavailable', rows: goodIds.map(id => ({ deal_id: id, name: `案件${id}`, company: null, consultant: '合成担当A', flags: [], risk_flags: [], good_flags: [{ key: 'applications', label: '応募 3 → 7', new: false, since: null }], days_left: null, days_since_contact: null, mtg_last: null, actionyotei: null, actionnaiyou: null })) }));
    }
    if (u.pathname.endsWith('/deals')) return Promise.resolve(Response.json({ ...boardFixture, rows, work: demoOwnerTodo }));
    return Promise.resolve(Response.json({ hubspot_portal_id: '123', selected: { deal: {} }, partial: [] }));
  }));
  return urls;
}
const open = async (query = '') => {
  window.history.replaceState(null, '', `/app/job-copy?view=deals&owner=11${query}`);
  render(<DealBoard />);
  await screen.findByRole('button', { name: row(0).name ?? '' });
};
const names = () => within(screen.getByRole('table')).getAllByRole('row').slice(1).map(r => r.querySelector('th button')?.textContent);

it('既定の列は8つで、採用・採用単価・金額・採用目標は行の「詳しい数字」を開くと見られる', async () => {
  stub(); await open();
  const headers = [...screen.getByRole('table').querySelectorAll('thead th')].map(th => th.textContent?.replace(/ ▲▼| ▼| ▲/gu, ''));
  expect(headers).toEqual(['案件・顧客', '要対応の印', '応募（直近4週）', '満了まで', '最後の接触', '最後のMTG', 'NPS', '担当', '詳しい数字']);
  expect(screen.queryByRole('button', { name: /^(採用|採用単価|金額|採用目標に対して) / })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: `${row(0).name ?? ''}の詳しい数字を開く` }));
  const detail = within(screen.getByRole('table', { name: `${row(0).name ?? ''}の詳しい数字` }));
  expect(detail.getAllByRole('columnheader').map(th => th.textContent?.replace(/ ▲▼/u, ''))).toEqual(['顧客名', 'ステージ', '何ヶ月目', '定期NPS', '応募', '面接', '採用', '採用単価', '金額', '採用目標に対して']);
  const cells = detail.getAllByRole('cell').map(td => td.textContent);
  expect(cells).toContain(yen(100_000));
  expect(cells).toContain(yen(50_000));
  expect(cells.at(-1)).toBe('20%2 / 10人');
});

it('「いい」は印の先頭に並び、いい案件の集計にある案件だけを出す', async () => {
  const urls = stub(['101', '103']); await open();
  const chip = await screen.findByRole('button', { name: 'いい 2' });
  const group = screen.getByRole('group', { name: '要対応の印で絞る' });
  expect([...group.querySelectorAll('button')].slice(0, 2).map(b => b.textContent?.replace(/\s+/gu, ' '))).toEqual(['すべて 4', 'いい 2']);
  fireEvent.click(chip);
  // 標準の並びは要対応の印の数、続いて金額の多い順
  expect(names()).toEqual([row(3).name, row(1).name]);
  expect(urls.some(u => u.includes('/api/job-copy/consultant-good?') && u.includes('owner=11'))).toBe(true);
});

it('いい案件の集計を読めないときは、印を押せず件数を出さない', async () => {
  stub('fail'); await open();
  const chip = await screen.findByRole('button', { name: /^いい/u });
  await waitFor(() => { expect((chip as HTMLButtonElement).disabled).toBe(true); });
  expect(chip.textContent).toContain('—');
});

it('面接が1件も無い取引だけを出し、面接の数が空の取引は0件に混ぜず別に数える', async () => {
  stub(); await open();
  fireEvent.click(screen.getByRole('checkbox', { name: '面接が1件も無い取引だけ' }));
  expect(names()).toEqual([row(0).name, row(2).name]);
  expect(screen.getByText(/面接の数が入っていない1件は、0件とは別に数えて含めていません/u)).toBeTruthy();
  expect(new URLSearchParams(window.location.search).get('deals_interview')).toBe('zero');
});

it('応募ゼロを調べる期間を指定すると、その期間の応募日が0件の取引だけを出し、数えられない取引は別に数える', async () => {
  const urls = stub(); await open('&deals_zero=yes');
  expect(screen.getByRole('heading', { name: /応募が1件も無い取引/u })).toBeTruthy();
  fireEvent.change(screen.getByRole('combobox', { name: '応募ゼロを調べる期間' }), { target: { value: 'weeks' } });
  await waitFor(() => { expect(urls.some(u => /period_from=\d{4}-\d{2}-\d{2}&period_to=\d{4}-\d{2}-\d{2}/u.test(u))).toBe(true); });
  const last = new URL(urls.filter(u => u.includes('period_from')).at(-1) ?? '', location.origin);
  const days = (Date.parse(last.searchParams.get('period_to') ?? '') - Date.parse(last.searchParams.get('period_from') ?? '')) / 86_400_000;
  expect(days).toBe(27);
  await waitFor(() => { expect(names()).toEqual([row(0).name]); });
  expect(screen.getByRole('heading', { name: /応募が1件も無い取引（.*〜.*） 1件/u })).toBeTruthy();
  expect(screen.getByText(/応募日が分からない応募がある1件は、期間の応募数を数えられないため含みません/u)).toBeTruthy();
  fireEvent.change(screen.getByRole('combobox', { name: '応募ゼロを調べる期間' }), { target: { value: 'contract' } });
  await waitFor(() => { expect(screen.getByRole('heading', { name: /初回契約の期間中/u })).toBeTruthy(); });
  expect(names()).toEqual([row(0).name]);
});
