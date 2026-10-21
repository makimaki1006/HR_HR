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
const recentApi = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: (path: string, options: unknown) => path.startsWith('/api/job-copy/recent-changes') ? recentApi(path, options) as Promise<unknown> : api(path, options) as Promise<unknown> }));
beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy'); recentApi.mockResolvedValue({ ok: true, data: { status: 'ready', report: { count: 0, listings: [] }, refresh_failed: false } }); });
afterEach(() => { cleanup(); vi.resetAllMocks(); vi.restoreAllMocks(); });

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
    cards: [card('more', '直近4週の応募が、前の4週より多い求人', [row('1', '配送ドライバー・大分1', 20, 13)]), card('flat', '応募はあるが、前の4週とほぼ同じ求人', []), card('less', '直近4週の応募が、前の4週より少ない求人', [row('2', '介護スタッフ・熊本', 4, 5)], 130), card('costly', '課金が高いまま応募が少ない求人', []), card('stale', 'データが止まっている求人', [])],
    customers, live_without_company: 2, median_cost_per_app: 7000,
  },
};
const at = <T,>(items: T[], index: number): T => { const item = items[index]; if (item === undefined) throw new Error('missing'); return item; };
const open = vi.fn(); const openListing = vi.fn();
const renderHome = () => render(<JobCopyHome onOpenCompany={open} onOpenListing={openListing} onOpenDeal={vi.fn()} />);

it('常に出すカードは2枚まで。残りは件数だけの1行で、押すとその求人の一覧が開く', async () => {
  api.mockResolvedValue({ ok: true, data: response });
  renderHome();
  await screen.findByRole('button', { name: /課金が高いまま応募が少ない求人/ });
  await screen.findByRole('button', { name: /直近7日に変更 0件/ });
  const cards = [...document.querySelectorAll('.jh-cards .jh-card')];
  expect(cards).toHaveLength(1);
  expect(within(at(cards, 0) as HTMLElement).getByText('0')).toBeTruthy();
  expect(document.querySelector('.jh-rule')).toBeNull();
  expect(document.querySelector('.jh-more')?.textContent).toBe('ほかの見るもの：直近4週の応募が、前の4週より多い求人 1件・応募はあるが、前の4週とほぼ同じ求人 0件・直近4週の応募が、前の4週より少ない求人 130件・データが止まっている求人 0件・直近7日に変更 0件');
  expect(screen.getByText('掲載中の求人の応募・課金です（応募はHubSpotに記録された分だけ）。増減は並べて見るためのもので、原因とは限りません。')).toBeTruthy();
  expect(screen.queryByText(/効果あり|効果なし|成功|失敗/)).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: /より多い求人 1件/ }));
  const list = screen.getByRole('region', { name: '直近4週の応募が、前の4週より多い求人の一覧' });
  expect(within(list).getByText('13 → 20 件')).toBeTruthy();
  fireEvent.click(within(list).getByRole('button', { name: '配送ドライバー・大分1' }));
  expect(openListing).toHaveBeenCalledWith('1');
  // 一覧に出ている数より件数が多いときは、先頭だけだと伝える。
  fireEvent.click(screen.getByRole('button', { name: /より少ない求人 130件/ }));
  expect(screen.getByText('先頭の1件だけを表示しています。')).toBeTruthy();
});

it.each(['api', 'cache'] as const)('%s の古い職種カードと未知のキーを無視する', async source => {
  const old = { ...response, home: { ...response.home, cards: [...response.home.cards,
    { key: 'occupation', title: '市場の職種が未指定の求人', rule: '古いカード', count: 4, listings: [] },
    { key: 'toString', title: '未知のカード', rule: '未知の数え方', count: 9, listings: [] },
  ] } };
  window.history.replaceState({}, '', '?card=occupation');
  api.mockResolvedValue({ ok: true, data: old });
  // 実行時に受け取る旧形式は、現在の型の外にある。
  const cache = { current: source === 'cache' ? old as HomeResponse : null };
  render(<JobCopyHome cache={cache} onOpenCompany={open} onOpenListing={openListing} onOpenDeal={vi.fn()} />);
  await screen.findByRole('button', { name: /課金が高いまま応募が少ない求人/ });
  expect(document.querySelectorAll('.jh-cards .jh-card')).toHaveLength(1);
  expect(document.querySelector('.jh-more')?.textContent).not.toContain('未指定');
  expect(screen.queryByText('市場の職種が未指定の求人')).toBeNull();
  expect(screen.queryByText('未知のカード')).toBeNull();
  expect(screen.queryByRole('region', { name: /未指定/ })).toBeNull();
  expect(screen.queryByRole('link', { name: '職種をまとめて指定する' })).toBeNull();
  expect(screen.queryByText('合成運輸')).toBeNull();
  expect(api).toHaveBeenCalledTimes(source === 'cache' ? 0 : 1);
  fireEvent.click(screen.getByRole('button', { name: /直近4週の応募が、前の4週より多い求人 1件/ }));
  expect(screen.getByRole('region', { name: '直近4週の応募が、前の4週より多い求人の一覧' })).toBeTruthy();
});

it('顧客の表は応募の多い順で、並べ替え・絞り込み・顧客を開くができる', async () => {
  api.mockResolvedValue({ ok: true, data: response });
  renderHome();
  await screen.findByLabelText('顧客名で探す');
  // 検索前・「すべて表示」前は表を出さない。
  expect(screen.queryByRole('table')).toBeNull();
  expect(screen.queryByText('合成運輸')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'すべて表示（3件）' }));
  expect(screen.getByRole('button', { name: '表を閉じる' })).toBeTruthy();
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
  fireEvent.click(screen.getByRole('button', { name: '表を閉じる' }));
  expect(screen.getByRole('table')).toBeTruthy(); // 検索語が残っている間は表を出す
  fireEvent.change(screen.getByLabelText('顧客名で探す'), { target: { value: '' } });
  expect(screen.queryByRole('table')).toBeNull();
});

it('顧客名で探すと、すべて表示を押さなくても表が出て、契約が終わった顧客も見つかる', async () => {
  api.mockResolvedValue({ ok: true, data: response });
  renderHome();
  await screen.findByLabelText('顧客名で探す');
  expect(screen.queryByRole('table')).toBeNull();
  fireEvent.change(screen.getByLabelText('顧客名で探す'), { target: { value: '介護' } });
  expect(screen.getAllByRole('rowheader').map(cell => cell.textContent)).toEqual(['合成介護']);
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

it('demo: 常に2枚のカード、残り5枚と直近7日の変更は件数だけの1行。ゼロのままの求人の課金を出す', async () => {
  api.mockResolvedValue({ ok: true, data: demoHome });
  renderHome();
  await screen.findByRole('button', { name: /応募がゼロのままの求人 2件/ });
  await screen.findByRole('button', { name: /直近7日に変更 0件/ });
  const cards = [...document.querySelectorAll<HTMLElement>('.jh-cards .jh-card')];
  expect(cards.map(button => button.querySelector('.jh-card-title')?.textContent)).toEqual(['応募がゼロになった求人', '課金が高いまま応募が少ない求人']);
  expect(cards.map(button => button.querySelector('.jh-count')?.textContent)).toEqual(['1件', '2件']);
  expect(at(cards, 0).classList.contains('jh-alert')).toBe(true);
  expect(at(cards, 1).classList.contains('jh-warn')).toBe(true);
  expect(document.querySelector('.jh-more')?.textContent).toBe('ほかの見るもの：応募がゼロのままの求人 2件・直近4週の応募が、前の4週より少ない求人 1件・直近4週の応募が、前の4週より多い求人 1件・応募はあるが、前の4週とほぼ同じ求人 1件・データが止まっている求人 1件・直近7日に変更 0件');
  // 数え方は押すと開く説明の中にある。
  expect(screen.getByText(/2件以上、または20%以上の差を増減/).closest('details')?.hasAttribute('open')).toBe(false);
  expect(screen.getByText(/掲載中で、週ごとの実績の最新の週が14日より前に終わっている求人/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: /応募がゼロのままの求人 2件/ }));
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
    await act(async () => { await vi.advanceTimersByTimeAsync(3_600_000); });
    expect(api).toHaveBeenCalledTimes(2);
    cleanup();
    await act(async () => { await vi.advanceTimersByTimeAsync(3_600_000); });
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

it('やることの一覧を開くと取得を確認し、閉じるだけでは再取得しない', async () => {
  window.history.replaceState(null, '', '/app/job-copy?owner=11');
  api.mockResolvedValue({ ok: true, data: { ...response, work: demoOwnerTodo } });
  renderHome();
  const card = await screen.findByRole('button', { name: /接触が空いている/ });
  expect(api).toHaveBeenCalledTimes(1);
  fireEvent.click(card);
  await waitFor(() => { expect(api).toHaveBeenCalledTimes(2); });
  fireEvent.click(card);
  await act(() => Promise.resolve());
  expect(api).toHaveBeenCalledTimes(2);
});

it('集計が続く60秒間は6回、その次の60秒間は2回読み、ホームの更新で直近7日の変更を取り直さない', async () => {
  vi.useFakeTimers();
  try {
    window.history.replaceState(null, '', '?owner_field=consultant&owner=11');
    api.mockResolvedValue({ ok: true, data: { ...response, work: { ...demoOwnerTodo, todos_status: 'preparing', refreshing: true } } });
    renderHome();
    await act(() => Promise.resolve());
    expect(screen.getByText('やることを集計しています…')).toBeTruthy();
    expect(within(screen.getByRole('region', { name: '担当者で絞る' })).queryByRole('status')).toBeNull();
    expect(screen.getByText('まず見るもの')).toBeTruthy();
    expect(screen.getByLabelText('顧客名で探す')).toBeTruthy();
    for (const [seconds, calls] of [[2, 2], [4, 3], [8, 4], [16, 5], [30, 6]] as const) {
      await act(async () => { await vi.advanceTimersByTimeAsync(seconds * 1_000); });
      expect(api).toHaveBeenCalledTimes(calls);
      expect(recentApi).toHaveBeenCalledTimes(1);
    }
    for (let i = 0; i < 2; i++) await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(api).toHaveBeenCalledTimes(8);
    fireEvent.click(screen.getByRole('button', { name: /^再読み込み$/ }));
    await act(() => Promise.resolve());
    expect(api).toHaveBeenCalledTimes(9);
    expect(recentApi).toHaveBeenCalledTimes(1);
  } finally { vi.useRealTimers(); }
});

it('owner=meを担当者に置き換えてもホームは1回だけ読み、別の担当者を選ぶと読み直す', async () => {
  window.history.replaceState(null, '', '?owner=me');
  api.mockResolvedValue({ ok: true, data: { ...response, work: demoOwnerTodo } });
  renderHome();
  await waitFor(() => { expect(new URLSearchParams(window.location.search).get('owner')).toBe('11'); });
  expect(api).toHaveBeenCalledTimes(1);
  expect(recentApi).toHaveBeenCalledTimes(1);
  fireEvent.change(screen.getByRole('combobox', { name: '担当者' }), { target: { value: '22' } });
  await waitFor(() => { expect(api).toHaveBeenCalledTimes(2); });
  expect(api.mock.calls[1]?.[0]).toContain('owner=22');
});

it('タブが隠れている間はホームと直近7日の変更を取り直さず、戻ると続ける', async () => {
  vi.useFakeTimers();
  let hidden = false;
  vi.spyOn(document, 'hidden', 'get').mockImplementation(() => hidden);
  try {
    window.history.replaceState(null, '', '?owner=11');
    api.mockResolvedValue({ ok: true, data: { ...response, work: { ...demoOwnerTodo, todos_status: 'preparing' } } });
    recentApi.mockResolvedValue({ ok: true, data: { status: 'preparing' } });
    renderHome();
    await act(() => Promise.resolve());
    expect(api).toHaveBeenCalledTimes(1);
    expect(recentApi).toHaveBeenCalledTimes(1);
    hidden = true; document.dispatchEvent(new Event('visibilitychange'));
    await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
    expect(api).toHaveBeenCalledTimes(1); expect(recentApi).toHaveBeenCalledTimes(1);
    hidden = false; document.dispatchEvent(new Event('visibilitychange'));
    await act(async () => { await vi.advanceTimersByTimeAsync(2_000); });
    expect(api).toHaveBeenCalledTimes(2); expect(recentApi).toHaveBeenCalledTimes(2);
  } finally { vi.useRealTimers(); }
});

it('担当者を集計している間もカードと顧客の検索欄を表示する', async () => {
  api.mockResolvedValue({ ok: true, data: { ...response, work: { status: 'preparing' } } });
  renderHome();
  expect(await screen.findByText('担当者とやることを集計しています…')).toBeTruthy();
  expect(screen.getByText('まず見るもの')).toBeTruthy();
  expect(screen.getByLabelText('顧客名で探す')).toBeTruthy();
});
