// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { JobCopyHome } from './JobCopyHome';
import { demoOwnerTodo } from './demoOwnerTodo';
import { demoHome } from './demoHome';
import { Breadcrumbs } from './Breadcrumbs';
import { buildCrumbs } from './trail';
import type { HomeCard, HomeCustomer, HomeResponse } from './homeModel';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: (path: string, options: unknown) => path.startsWith('/api/job-copy/recent-changes') ? Promise.resolve({ ok: true, data: { status: 'ready', report: { count: 0, listings: [] }, refresh_failed: false } }) : api(path, options) as Promise<unknown> }));
beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy'); });
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const row = (id: string, title: string, recent: number, prev: number) => ({ id, title, company: '合成運輸', media: 'hrh' as const, recent_apps: recent, prev_apps: prev, recent_cost: 12000, latest_week_end: '2026-10-04' });
const card = (key: HomeCard['key'], title: string, listings: HomeCard['listings'], count = listings.length): HomeCard => ({ key, title, rule: `${title}の数え方`, count, listings });
const customers: HomeCustomer[] = [
  { company_id: '11', name: '合成運輸', deals: 3, active_deals: 2, latest_deal: { name: '合成運輸 2026年案件', stage_label: '契約中', active: true, created_at: '2026-03-01' }, total_listings: 9, live_listings: 4, recent_apps: 58, prev_apps: 41, recent_cost: 439000, cost_per_app: 7569 },
  { company_id: '12', name: '合成介護', deals: 1, active_deals: 0, latest_deal: { name: '合成介護 案件', stage_label: '契約終了', active: false, created_at: '2025-06-01' }, total_listings: 5, live_listings: 2, recent_apps: 6, prev_apps: 9, recent_cost: 60000, cost_per_app: 10000 },
  { company_id: '13', name: '合成倉庫', deals: 2, active_deals: 1, latest_deal: { name: '合成倉庫 案件', stage_label: '契約中', active: true, created_at: '2026-09-01' }, total_listings: 1, live_listings: 1, recent_apps: 0, prev_apps: 0, recent_cost: 8000, cost_per_app: null },
];
const response: HomeResponse = {
  status: 'ready', index_built_at: '2026-10-14T01:00:00Z', today: '2026-10-14', companies_ok: true,
  rules: { trend_min_diff: 2, trend_min_percent: 20, cost_floor_yen: 10000, cost_per_app_times: 2, stale_days: 14, list_cap: 100 },
  home: {
    cards: [card('more', '直近4週の応募が、前の4週より多い求人', [row('1', '配送ドライバー・大分1', 20, 13)]), card('flat', '応募はあるが、前の4週とほぼ同じ求人', []), card('less', '直近4週の応募が、前の4週より少ない求人', [row('2', '介護スタッフ・熊本', 4, 5)], 130), card('costly', '課金が高いまま応募が少ない求人', []), card('stale', 'データが止まっている求人', []), card('occupation', '市場の職種が未指定の求人', [row('3', '事務スタッフ・大分', 1, 1)])],
    customers, live_without_company: 2, median_cost_per_app: 7000,
  },
};
const at = <T,>(items: T[], index: number): T => { const item = items[index]; if (item === undefined) throw new Error('missing'); return item; };
const open = vi.fn(); const openListing = vi.fn();
const renderHome = () => render(<JobCopyHome onOpenCompany={open} onOpenListing={openListing} onOpenDeal={vi.fn()} />);

it('カードに件数と数え方を出し、押すとその求人の一覧が開く', async () => {
  api.mockResolvedValue({ ok: true, data: response });
  renderHome();
  const cards = await screen.findAllByRole('button', { name: /の数え方/ });
  expect(cards).toHaveLength(6);
  expect(within(at(cards, 0)).getByText('1')).toBeTruthy();
  expect(within(at(cards, 2)).getByText('130')).toBeTruthy();
  expect(screen.queryByText(/効果あり|効果なし|成功|失敗/)).toBeNull();
  fireEvent.click(at(cards, 0));
  const list = screen.getByRole('region', { name: '直近4週の応募が、前の4週より多い求人の一覧' });
  expect(within(list).getByText('13 → 20 件')).toBeTruthy();
  fireEvent.click(within(list).getByRole('button', { name: '配送ドライバー・大分1' }));
  expect(openListing).toHaveBeenCalledWith('1');
  // 一覧に出ている数より件数が多いときは、先頭だけだと伝える。
  fireEvent.click(at(cards, 2));
  expect(screen.getByText('先頭の1件だけを表示しています。')).toBeTruthy();
});

it('職種が未指定のカードから、職種をまとめて指定する画面へ行ける', async () => {
  api.mockResolvedValue({ ok: true, data: response });
  renderHome();
  fireEvent.click(await screen.findByRole('button', { name: /市場の職種が未指定の求人/ }));
  expect(screen.getByRole('link', { name: '職種をまとめて指定する' }).getAttribute('href')).toBe('?card=occupation&view=occupations');
});

it('顧客の表は応募の多い順で、並べ替え・絞り込み・顧客を開くができる', async () => {
  api.mockResolvedValue({ ok: true, data: response });
  renderHome();
  await screen.findByText('合成運輸');
  const names = () => screen.getAllByRole('rowheader').map(cell => cell.textContent);
  // 既定は「契約中の案件がある顧客」だけ。合成介護（契約終了だけ）は外れる。
  expect(names()).toEqual(['合成運輸', '合成倉庫']);
  expect(screen.getByRole('button', { name: /契約中の案件がある顧客/ }).getAttribute('aria-pressed')).toBe('true');
  expect(screen.getByText('2 ／ 全 3')).toBeTruthy();
  expect(screen.getByText('合成運輸 2026年案件')).toBeTruthy();
  expect(screen.getByText(/契約中 ・ 作成 2026\/03\/01/)).toBeTruthy();
  expect(screen.getByText('4 ／ 全 9')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: /^すべて/ }));
  expect(names()).toEqual(['合成運輸', '合成介護', '合成倉庫']);
  expect(screen.getByText('合成介護 案件')).toBeTruthy();
  expect(screen.getByText('41 → 58 件')).toBeTruthy();
  expect(screen.getByText('¥439,000')).toBeTruthy();
  expect(screen.getByText('¥7,569')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: /応募 1 件あたり/ }));
  expect(names()).toEqual(['合成介護', '合成運輸', '合成倉庫']); // 高い順。出ない行は最後
  fireEvent.click(screen.getByRole('button', { name: /応募 1 件あたり/ }));
  expect(names()).toEqual(['合成運輸', '合成介護', '合成倉庫']); // 安い順。出ない行は最後
  fireEvent.change(screen.getByLabelText('顧客名で探す'), { target: { value: '介護' } });
  expect(names()).toEqual(['合成介護']);
  fireEvent.click(screen.getByRole('button', { name: '合成介護を開く' }));
  expect(open).toHaveBeenCalledWith({ id: '12', name: '合成介護' });
  expect(screen.getByText(/納品管理の案件に結び付いていない掲載中の求人が 2 件/)).toBeTruthy();
});

it('一覧を準備中のときは待ち、読めなかったときは理由を出す', async () => {
  api.mockResolvedValue({ ok: true, data: { status: 'preparing' } });
  renderHome();
  expect(await screen.findByText('求人の一覧を準備しています')).toBeTruthy();
  cleanup();
  api.mockResolvedValue({ ok: false, error: new Error('x') });
  renderHome();
  await waitFor(() => { expect(screen.getByRole('alert').textContent).toContain('読み込めませんでした'); });
});

it('パンくずは現在地だけがリンクにならず、上の階層を押すと戻る', () => {
  const select = vi.fn();
  render(<Breadcrumbs crumbs={buildCrumbs({ company: '1', deal: '2', listing: '3' }, { company: '合成運輸', deal: '配送案件', listing: '配送ドライバー' })} onSelect={select} />);
  const nav = screen.getByRole('navigation', { name: '現在地' });
  expect(within(nav).getAllByRole('listitem').map(item => item.textContent)).toEqual(['案件管理', '合成運輸', '配送案件', '配送ドライバー']);
  expect(within(nav).getByText('配送ドライバー').getAttribute('aria-current')).toBe('page');
  fireEvent.click(within(nav).getByRole('button', { name: '合成運輸' }));
  expect(select).toHaveBeenCalledWith('company');
});

it('新しい8枚の見出し・件数・説明と、ゼロのままの求人の課金を出す', async () => {
  api.mockResolvedValue({ ok: true, data: demoHome });
  renderHome();
  await screen.findByRole('button', { name: /応募がゼロのままの求人/ });
  const cards = screen.getAllByRole('button').filter(button => button.classList.contains('jh-card'));
  expect(cards).toHaveLength(9);
  for (const [index, item] of demoHome.home.cards.entries()) {
    const button = at(cards, index);
    expect(within(button).getByText(item.title)).toBeTruthy();
    expect(within(button).getByText(String(item.count))).toBeTruthy();
    expect(within(button).getByText(item.rule)).toBeTruthy();
  }
  expect(at(cards, 0).classList.contains('jh-alert')).toBe(true);
  expect(at(cards, 1).classList.contains('jh-alert')).toBe(true);
  expect(screen.getByText(/2件以上、または20%以上の差を増減/)).toBeTruthy();
  fireEvent.click(at(cards, 0));
  const list = screen.getByRole('region', { name: '応募がゼロのままの求人の一覧' });
  expect(within(list).getByRole('columnheader', { name: '課金（直近 4 週）' })).toBeTruthy();
  const rows = within(list).getAllByRole('row').slice(1);
  expect(rows.map(row => row.textContent)).toEqual([
    '合成求人・配送合成事業所HRハッカー0 → 0 件¥24,0002026/10/11',
    '合成求人・清掃合成事業所HRハッカー0 → 0 件¥02026/10/11',
  ]);
});

it('初回の準備中は2秒後に取り直し、表示できたら自動取得を止める', async () => {
  vi.useFakeTimers();
  try {
    api.mockResolvedValueOnce({ ok: true, data: { status: 'preparing' } }).mockResolvedValue({ ok: true, data: response });
    renderHome();
    await act(() => Promise.resolve());
    expect(screen.getByText('求人の一覧を準備しています')).toBeTruthy();
    await act(async () => { await vi.advanceTimersByTimeAsync(1_999); });
    expect(api).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(api).toHaveBeenCalledTimes(2);
    expect(screen.getByText('まず見るもの')).toBeTruthy();
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(api).toHaveBeenCalledTimes(2);
    cleanup();
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(api).toHaveBeenCalledTimes(2);
  } finally { vi.useRealTimers(); }
});

it('準備中に読み終わった件数と見込み件数を表示する', async () => {
  api.mockResolvedValue({ ok: true, data: { status: 'preparing', progress: { read_count: 12300, estimated_total: 41000, retry_after_seconds: null } } });
  renderHome();
  expect(await screen.findByText('約 41,000 件中 12,300 件まで読みました')).toBeTruthy();
});

it('読み込み失敗後は次に読み直すまでの時間を表示する', async () => {
  api.mockResolvedValue({ ok: true, data: { status: 'preparing', progress: { read_count: 41000, estimated_total: 41000, retry_after_seconds: 60 } } });
  renderHome();
  expect(await screen.findByText('読み込みに失敗したため、1 分後にもう一度読み込みます。')).toBeTruthy();
  expect(screen.getByText('約 41,000 件中 41,000 件まで読みました')).toBeTruthy();
});

it('ほかの人から自分に戻すとき、取得前の担当者でURLを上書きしない', async () => {
  window.history.replaceState(null, '', '/app/job-copy?owner=22');
  let resolveSelf: ((value: unknown) => void) | undefined;
  api.mockImplementation((path: string) => path.includes('owner=me') ? new Promise(resolve => { resolveSelf = resolve; }) : Promise.resolve({ ok: true, data: { ...response, work: { ...demoOwnerTodo, owner: path.includes('owner=11') ? '11' : '22' } } }));
  renderHome();
  await screen.findByRole('combobox', { name: '担当者' });
  fireEvent.click(screen.getByRole('button', { name: '自分に戻す' }));
  await waitFor(() => { expect(resolveSelf).toBeDefined(); });
  expect(new URLSearchParams(window.location.search).get('owner')).toBe('me');
  await act(async () => { resolveSelf?.({ ok: true, data: { ...response, work: demoOwnerTodo } }); await Promise.resolve(); });
  await waitFor(() => { expect(new URLSearchParams(window.location.search).get('owner')).toBe('11'); });
  expect(screen.getByRole<HTMLSelectElement>('combobox', { name: '担当者' }).value).toBe('11');
});

it('初期URLではコンサル担当と解決済み本人を選び、全員への注記は出さない', async () => {
  api.mockResolvedValue({ ok: true, data: { ...response, work: demoOwnerTodo } });
  renderHome();
  await screen.findByRole('combobox', { name: '担当者' });
  expect(screen.getByRole<HTMLSelectElement>('combobox', { name: '担当の項目' }).value).toBe('consultant');
  expect(screen.getByRole<HTMLSelectElement>('combobox', { name: '担当者' }).value).toBe('11');
  await waitFor(() => { expect(new URLSearchParams(window.location.search).get('owner')).toBe('11'); });
  expect(new URLSearchParams(window.location.search).get('owner_field')).toBe('consultant');
  expect(screen.queryByText(/全員を表示しています/)).toBeNull();
  expect(screen.queryByRole('region', { name: '期限切れの一覧' })).toBeNull();
});
