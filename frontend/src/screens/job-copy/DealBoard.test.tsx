// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { DealBoard } from './DealBoard';
import { boardFixture, boardRows } from './dealBoard.testFixtures';
import type { DealRow } from './dealBoardModel';
import { demoOwnerTodo } from './demoOwnerTodo';
import { yen } from './homeModel';

vi.mock('./ProposalMemo', () => ({ ProposalMemo: () => <p>案件のメモ</p> }));
vi.mock('./NextTodo', () => ({ NextTodoButton: () => <button>次の予定を記録</button>, ReasonChips: () => null, OverdueBadge: () => null }));
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
function stub(goodIds: string[] | 'fail' = ['101', '103'], deals: DealRow[] = rows) {
  const urls: string[] = [];
  vi.stubGlobal('fetch', vi.fn((input: string) => {
    urls.push(input);
    const u = new URL(input, location.origin);
    if (u.pathname.endsWith('/consultant-good')) {
      if (goodIds === 'fail') return Promise.resolve(new Response('{}', { status: 500 }));
      return Promise.resolve(Response.json({ status: 'ready', data_day: '2026-10-20', previous_day: null, comparison: 'unavailable', rows: goodIds.map(id => ({ deal_id: id, name: `案件${id}`, company: null, consultant: '合成担当A', flags: [], risk_flags: [], good_flags: [{ key: 'applications', label: '応募 3 → 7', new: false, since: null }], days_left: null, days_since_contact: null, mtg_last: null, actionyotei: null, actionnaiyou: null })) }));
    }
    if (u.pathname.endsWith('/deals')) return Promise.resolve(Response.json({ ...boardFixture, rows: deals, work: demoOwnerTodo }));
    return Promise.resolve(Response.json({ hubspot_portal_id: '123', selected: { deal: {} }, partial: [] }));
  }));
  return urls;
}
const open = async (query = '') => {
  window.history.replaceState(null, '', `/app/job-copy?view=deals&owner=11${query}`);
  render(<DealBoard />);
  await screen.findByRole('link', { name: row(0).name ?? '' });
};
const names = () => within(screen.getByRole('table')).getAllByRole('row').slice(1).map(r => r.querySelector('th .jc-row-name')?.textContent);

it('既定の列は8つで、採用・採用単価・金額・採用目標は行の「詳しい数字」を開くと見られる', async () => {
  stub(); await open();
  const headers = [...screen.getByRole('table').querySelectorAll('thead th')].map(th => th.textContent.replace(/ ⇅| ▼| ▲/gu, ''));
  expect(headers).toEqual(['案件・顧客', 'タグ', '応募（HubSpot・直近4週）', '満了まで', '最後の接触', '最後のMTG', '満足度', '担当', '詳しい数字・詳細']);
  expect(screen.queryByRole('button', { name: /^(採用|採用単価|金額|採用目標に対して) / })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: `${row(0).name ?? ''}の詳しい数字を開く` }));
  const detail = within(screen.getByRole('table', { name: `${row(0).name ?? ''}の詳しい数字` }));
  expect(detail.getAllByRole('columnheader').map(th => th.textContent.replace(/ ⇅/u, ''))).toEqual(['顧客名', 'ステージ', '何ヶ月目', '定期の満足度', '案件に記録された応募（累計）', '面接', '採用', '採用単価', '金額', '採用目標に対して']);
  const cells = detail.getAllByRole('cell').map(td => td.textContent);
  expect(cells).toContain(yen(100_000));
  expect(cells).toContain(yen(50_000));
  expect(cells.at(-1)).toBe('20%2 / 10人');
});

it('「順調」は印の先頭に並び、いい案件の集計にある案件だけを出す', async () => {
  const urls = stub(['101', '103']); await open();
  const chip = await screen.findByRole('button', { name: '順調 2' });
  const group = screen.getByRole('group', { name: 'タグで絞る' });
  expect([...group.querySelectorAll('button')].slice(0, 2).map(b => b.textContent.replace(/\s+/gu, ' '))).toEqual(['すべて 4', '順調 2']);
  fireEvent.click(chip);
  // 標準の並びはタグが多い順、続いて金額が多い順
  expect(names()).toEqual([row(3).name, row(1).name]);
  expect(urls.some(u => u.includes('/api/job-copy/consultant-good?') && u.includes('owner=11'))).toBe(true);
});

it('いい案件の集計を読めないときは、印を押せず件数を出さない', async () => {
  stub('fail'); await open();
  const chip = await screen.findByRole('button', { name: /^順調/u });
  await waitFor(() => { expect((chip as HTMLButtonElement).disabled).toBe(true); });
  expect(chip.textContent).toContain('—');
});

it('面接が1件も無い案件だけを出し、面接の数が空の案件は0件に混ぜず別に数える', async () => {
  stub(); await open();
  fireEvent.click(screen.getByRole('checkbox', { name: '面接が1件も無い案件だけ' }));
  expect(names()).toEqual([row(0).name, row(2).name]);
  expect(screen.getByText(/面接の数が入っていない1件は、0件とは別に数えて含めていません/u)).toBeTruthy();
  expect(new URLSearchParams(window.location.search).get('deals_interview')).toBe('zero');
});

it('応募ゼロを調べる期間を指定すると、その期間の応募日が0件の案件だけを出し、数えられない案件は別に数える', async () => {
  const urls = stub(); await open('&deals_zero=yes');
  expect(screen.getByRole('heading', { name: /応募が1件も無い案件/u })).toBeTruthy();
  fireEvent.change(screen.getByRole('combobox', { name: '応募ゼロを調べる期間' }), { target: { value: 'weeks' } });
  await waitFor(() => { expect(urls.some(u => /period_from=\d{4}-\d{2}-\d{2}&period_to=\d{4}-\d{2}-\d{2}/u.test(u))).toBe(true); });
  const last = new URL(urls.filter(u => u.includes('period_from')).at(-1) ?? '', location.origin);
  const days = (Date.parse(last.searchParams.get('period_to') ?? '') - Date.parse(last.searchParams.get('period_from') ?? '')) / 86_400_000;
  expect(days).toBe(27);
  await waitFor(() => { expect(names()).toEqual([row(0).name]); });
  expect(screen.getByRole('heading', { name: /応募が1件も無い案件（.*〜.*） 1件/u })).toBeTruthy();
  expect(screen.getByText(/応募日も作成日も分からない応募がある1件は、期間の応募数を数えられないため含みません/u)).toBeTruthy();
  fireEvent.change(screen.getByRole('combobox', { name: '応募ゼロを調べる期間' }), { target: { value: 'contract' } });
  await waitFor(() => { expect(screen.getByRole('heading', { name: /初回契約の期間中/u })).toBeTruthy(); });
  expect(names()).toEqual([row(0).name]);
});

it('まとめは案件をまたぐ応募を1回に数え、絞り込みに従い、行に前の4週との差を出す', async () => {
  stub(); await open();
  const summary = () => within(screen.getByRole('region', { name: '絞り込み中の応募数' }));
  expect(summary().getByText(/直近4週/u).textContent).toBe('直近4週 4件 ／ 前の4週 1件 ／ 差 +3件');
  const first = screen.getByRole('table').querySelector('tbody tr');
  expect(first?.textContent).toContain('前の4週 1件差 0件');
  fireEvent.click(screen.getByRole('checkbox', { name: '面接が1件も無い案件だけ' }));
  expect(summary().getByText(/直近4週/u).textContent).toBe('直近4週 3件 ／ 前の4週 1件 ／ 差 +2件');
  fireEvent.click(screen.getByRole('button', { name: '定期の満足度が低い 1' }));
  expect(summary().getByText(/直近4週/u).textContent).toBe('直近4週 1件 ／ 前の4週 1件 ／ 差 0件');
});

it('日付不明を添えて負の差を表示し、未取得の案件があればまとめを0件にしない', async () => {
  stub([], [{ ...row(0), recent_applications: 1, previous_applications: 3, applications_change: -2, unknown_application_ids: ['u'] }, { ...row(1), recent_applications: null, recent_application_ids: null, previous_applications: null, previous_application_ids: null, unknown_application_ids: null, applications_change: null }]);
  await open();
  const summary = within(screen.getByRole('region', { name: '絞り込み中の応募数' }));
  expect(summary.getByText(/直近4週/u).textContent).toBe('直近4週 不明 ／ 前の4週 不明 ／ 差 不明');
  expect(summary.getByText('含めていないもの：日付不明の応募 1件')).toBeTruthy();
  expect(summary.getByText('応募数が分からない案件が 1 件あるため、合計は出していません。')).toBeTruthy();
  const first = screen.getByRole('table').querySelector('tbody tr');
  expect(first?.textContent).toContain('前の4週 3件差 -2件');
  expect(first?.textContent).toContain('日付不明 1件を除外');
});

it('案件ごとの応募動機を行に出し、詳しい数字で全件の表を開く', async () => {
  const { default: application_reasons } = await import('../../../../scripts/e2e/deal_application_reasons.json');
  stub([], [{ ...row(0), application_reasons }]);
  render(<DealBoard />);
  await screen.findByText('応募動機を聞けた 3件／2026/09/14 以降の応募 5件');
  expect(screen.getByText('休日 2件').textContent).toBe('休日 2件');
  fireEvent.click(screen.getByRole('button', { name: `${row(0).name ?? ''}の詳しい数字を開く` }));
  const motives = within(screen.getByRole('table', { name: '応募動機の選択件数' }));
  expect(motives.getAllByRole('cell').map(cell => cell.textContent)).toEqual(['2件', '2件', '0件', '1件']);
  const priorities = within(screen.getByRole('table', { name: '勤務条件の優先順位の選択件数' }));
  expect(priorities.getAllByRole('cell').map(cell => cell.textContent)).toEqual(['2件', '0件', '0件', '1件', '2件', '0件', '0件', '0件', '1件', '0件', '0件', '0件']);
});

it('MTGが60日以上空いている（60日以上）と MTGが30〜59日空いている（30〜59日）を別の印で数え、接触はメールを添えるだけにする', async () => {
  stub(['101'], [row(0), { ...row(1), reasons: [{ kind: 'mtg_gap', text: 'MTGが30〜59日空いている（最後のMTGから 45 日）' }] }, row(2), { ...row(23), active: true }]);
  await open('&deals_ended=yes');
  const filters = screen.getByRole('group', { name: 'タグで絞る' });
  expect(within(filters).getByRole('button', { name: 'MTGが60日以上空いている 1' })).toBeTruthy();
  expect(within(filters).getByRole('button', { name: 'MTGが30〜59日空いている 1' })).toBeTruthy();
  fireEvent.click(within(filters).getByRole('button', { name: 'MTGが30〜59日空いている 1' }));
  await waitFor(() => { expect(names()).toEqual([row(1).name]); });
  fireEvent.click(within(filters).getByRole('button', { name: 'MTGが30〜59日空いている 1' }));
  await waitFor(() => { expect(names()).toHaveLength(4); });
  const cell = (r: DealRow) => screen.getByRole('link', { name: r.name ?? '' }).closest('tr')?.querySelectorAll('td')[3]?.textContent;
  expect(cell(row(0))).toBe('2026/10/12最後のメール 2026/10/14');
  expect(cell(row(2))).toBe('2026/10/12');
  expect(cell(row(23))).toBe('記録なし（対象外）');
});
