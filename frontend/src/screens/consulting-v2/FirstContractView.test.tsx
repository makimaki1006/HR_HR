// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ConsultingV2Screen } from './ConsultingV2Screen';
import type { FirstResponse, FirstRow } from './FirstContractView';
import { FirstContractView } from './FirstContractView';
import { TodayView } from './TodayView';
import { parseRoute, routeHref } from './route';

const row = (id: string, name: string, consultant: string): FirstRow => ({
  deal_id: id, name, consultant, start: '2026-01-01', expiry: '2026-07-01',
  first_contract: { overdue_count: 1, gates: [
    { label: '初回MTGの記録', achieved: true, deadline: '2026-01-08', overdue: false, text: '達成' },
    { label: '求人が掲載中', achieved: false, deadline: '2026-01-15', overdue: true, text: '期限超過' },
    { label: '応募1件以上', achieved: null, deadline: '2026-01-31', overdue: false, text: '判定できない' },
    { label: '面接1件以上', achieved: false, deadline: '2026-05-02', overdue: false, text: '未達' },
    { label: '満足度の回答', achieved: false, deadline: '2026-05-02', overdue: false, text: '未達' },
  ] },
  goals: { failed: false, draft: true, ideal: '採用を2名増やす', minimum: '未入力' },
});
const response: FirstResponse = {
  meta: { today: '2026-01-20', hubspot_portal_id: '12345' }, consultants: ['架空の担当甲', '架空の担当乙'],
  rows: [row('80000000001', '架空の契約甲', '架空の担当甲'), { ...row('80000000002', '架空の契約乙', '架空の担当乙'), goals: { failed: true } }],
  not_started: [{ ...row('80000000003', '架空の開始前', '架空の担当甲'), start: '2026-07-01' }],
};
const mockFetch = (ok = true) => {
  const fetcher = vi.fn<typeof fetch>(() => Promise.resolve(new Response(JSON.stringify(ok ? response : { error: '読み込みに失敗しました' }), { status: ok ? 200 : 503, headers: { 'Content-Type': 'application/json' } })));
  vi.stubGlobal('fetch', fetcher);
  return fetcher;
};
afterEach(() => { cleanup(); vi.unstubAllGlobals(); window.history.replaceState(null, '', '/'); });

it('入口から共通一覧を1回読み、判定・目標・開始前を表示する', async () => {
  const fetcher = mockFetch();
  window.history.replaceState(null, '', '/app/consulting-v2?view=first');
  render(<ConsultingV2Screen />);
  expect(await screen.findByRole('button', { name: '架空の契約甲' })).toBeTruthy();
  expect(screen.getByRole('button', { name: '初回契約' }).getAttribute('aria-current')).toBe('page');
  const tr = screen.getByRole('button', { name: '架空の契約甲' }).closest('tr');
  if (tr === null) throw new Error('案件の行が見つかりません');
  expect(tr.querySelector('.cv2-first-gates')?.textContent).toBe('●○○○○');
  expect(tr.querySelectorAll('.cv2-first-overdue').length).toBe(1);
  expect(within(tr).getByLabelText('求人が掲載中：期限超過（期限 2026-01-15）').className).toBe('cv2-first-overdue');
  expect(within(tr).getByLabelText('応募1件以上：判定できない（期限 2026-01-31）').className).toBe('');
  expect(tr.textContent).toContain('案 · 理想：採用を2名増やす前進ライン：未入力');
  expect(screen.getByText('読めませんでした')).toBeTruthy();
  expect(screen.getByRole('heading', { name: '開始前の初回契約（1 件）' })).toBeTruthy();
  expect(within(tr).getByRole('link', { name: /HubSpot で開く/ }).getAttribute('href')).toContain('80000000001');
  expect(screen.getByRole('table', { name: '初回契約' }).querySelector('caption')).toBeNull();
  expect(screen.getByRole('heading', { name: '初回契約（2 件）' })).toBeTruthy();
  expect(fetcher).toHaveBeenCalledTimes(1);
  expect(fetcher.mock.calls[0]?.[0]).toBe('/api/consulting/first-contract');
  fireEvent.change(screen.getByRole('combobox'), { target: { value: '架空の担当甲' } });
  await waitFor(() => { expect(screen.queryByRole('button', { name: '架空の契約乙' })).toBeNull(); });
  expect(window.location.search).toContain('view=first');
  expect(new URLSearchParams(window.location.search).get('c')).toBe('架空の担当甲');
});

it('行と案件名から既存の詳細を開く', async () => {
  mockFetch(); const go = vi.fn();
  render(<FirstContractView route={parseRoute('?view=first')} go={go} />);
  const button = await screen.findByRole('button', { name: '架空の契約甲' });
  fireEvent.click(button);
  expect(go).toHaveBeenLastCalledWith({ view: 'detail', id: '80000000001' });
  const tr = button.closest('tr');
  if (tr === null) throw new Error('案件の行が見つかりません');
  fireEvent.click(tr);
  expect(go).toHaveBeenCalledTimes(2);
});

it('読み取り失敗を表示し、再読込できる', async () => {
  mockFetch(false);
  render(<FirstContractView route={parseRoute('?view=first')} go={vi.fn()} />);
  const retry = await screen.findByRole('button', { name: 'もう一度読み込む' });
  mockFetch(); fireEvent.click(retry);
  expect(await screen.findByRole('button', { name: '架空の契約甲' })).toBeTruthy();
});

it('初回契約のURLは担当も含めて復元できる', () => {
  const route = parseRoute('?view=first&c=架空の担当甲');
  expect(route.view).toBe('first');
  expect(parseRoute(routeHref(route)).c).toBe('架空の担当甲');
});

it('今日では上位の外にある初回契約も続けて表示する', async () => {
  vi.stubGlobal('fetch', vi.fn<typeof fetch>(() => Promise.resolve(new Response(JSON.stringify({
    meta: { today: '2026-06-01', n_hit: 25, n_flags2: 24 }, rows: [], consultants: [],
    extra_low_satisfaction: [{ deal_id: '80000000004', name: '架空の期限超過', n_flags: 0, flags: [], first_contract: { overdue_count: 1 } }],
  }), { status: 200, headers: { 'Content-Type': 'application/json' } }))));
  render(<TodayView route={parseRoute('?view=today')} go={vi.fn()} />);
  expect(await screen.findByRole('button', { name: '架空の期限超過' })).toBeTruthy();
  expect(screen.getByRole('heading', { name: '今日動く先（1 件）' })).toBeTruthy();
});

it('CRM認可がない場合も一覧と関門を表示し目標の案内は1行だけ出す', async () => {
  const withoutGoals = (r: FirstRow): FirstRow => {
    const rest = { ...r };
    delete rest.goals;
    return rest;
  };
  vi.stubGlobal('fetch', vi.fn<typeof fetch>(() => Promise.resolve(new Response(JSON.stringify({
    ...response, goals_available: false, rows: response.rows.map(withoutGoals), not_started: response.not_started.map(withoutGoals),
  }), { status: 200, headers: { 'Content-Type': 'application/json' } }))));
  render(<FirstContractView route={parseRoute('?view=first')} go={vi.fn()} />);
  expect(await screen.findByRole('button', { name: '架空の契約甲' })).toBeTruthy();
  expect(screen.getAllByText('目標は社内の Google ログインで見られます')).toHaveLength(1);
  expect(screen.getAllByLabelText('求人が掲載中：期限超過（期限 2026-01-15）')).toHaveLength(3);
  expect(screen.queryByText('読めませんでした')).toBeNull();
  expect(screen.queryByText(/理想：/)).toBeNull();
});
