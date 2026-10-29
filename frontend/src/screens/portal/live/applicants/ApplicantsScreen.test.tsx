// @vitest-environment happy-dom
// 見る画面: サーバーで見られなくなったら内容を捨てる・セルのタグは文字のまま出す。サーバーは fetch の偽物。値は架空。
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { PortalSheetRows } from '../../../../generated/PortalSheetRows';
import { ApplicantsScreen } from './ApplicantsScreen';
import type { EditableApplicant, EditableColumn } from './editing';

const NAME = '架空 花子';
const SCRIPT = '<script>alert(1)</script>';

const viewColumns: EditableColumn[] = [
  { id: '応募者名', label: '応募者名', kind: 'text', group: 'summary', in_list: true, multiline: false, writable: false, options: null },
  { id: '備考', label: '備考', kind: 'text', group: 'customer', in_list: true, multiline: false, writable: false, options: null },
];
const viewApplicant: EditableApplicant = { row: 3, cells: [NAME, SCRIPT], revision: 'r1' };
const rows: PortalSheetRows = {
  sheet: { id: 's1', label: '応募者一覧' },
  version: 'v1',
  read_at: '2026-10-26T01:00:00Z',
  stale: false,
  columns: viewColumns,
  applicants: [viewApplicant],
};

interface Reply { status: number; body: unknown }
let replies: Record<string, Reply[]> = {};
let calls: string[] = [];

function reply(path: string, status: number, body: unknown) {
  (replies[path] ??= []).push({ status, body });
}

beforeEach(() => {
  replies = {};
  calls = [];
  window.history.replaceState(null, '', '/portal');
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => false });
  vi.stubGlobal('fetch', vi.fn((input: string) => {
    calls.push(input);
    const list = replies[input] ?? [];
    // 最後の応答は何度でも返す
    const r = list.length > 1 ? list.shift() : list[0];
    if (!r) return Promise.reject(new Error(`no reply: ${input}`));
    return Promise.resolve(new Response(JSON.stringify(r.body), { status: r.status, headers: { 'Content-Type': 'application/json' } }));
  }));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const me = { email: 'a@example.com', customer_id: 'c1' };
const noop = vi.fn();

function renderScreen() {
  return render(<ApplicantsScreen me={me} onSignedOut={noop} onLogout={noop} />);
}

/** 表示中の確かめ (30 秒おきの確認と同じ処理) をすぐ走らせる */
async function checkNow() {
  await act(async () => { document.dispatchEvent(new Event('visibilitychange')); await Promise.resolve(); });
}

it('セルの中のタグは文字のまま出し、スクリプトとして動かさない', async () => {
  reply('/api/portal/sheets', 200, { sheets: [{ id: 's1', label: '応募者一覧' }] });
  reply('/api/portal/sheets/s1/rows', 200, rows);
  const { container } = renderScreen();
  expect((await screen.findAllByText(SCRIPT)).length).toBeGreaterThan(0);
  expect(container.querySelector('script')).toBeNull();
});

it('更新の確認が 404 なら、表示中の応募者を消して見られなくなったと出し、シートの一覧を取り直す', async () => {
  reply('/api/portal/sheets', 200, { sheets: [{ id: 's1', label: '応募者一覧' }] });
  reply('/api/portal/sheets/s1/rows', 200, rows);
  reply('/api/portal/sheets/s1/version', 404, { error: 'not_found', message: 'シートが見つかりません' });
  renderScreen();
  expect((await screen.findAllByText(NAME)).length).toBeGreaterThan(0);
  // 取り直したシートの一覧からは消えている
  replies['/api/portal/sheets'] = [{ status: 200, body: { sheets: [] } }];
  await checkNow();
  await waitFor(() => { expect(screen.queryAllByText(NAME)).toHaveLength(0); });
  expect(await screen.findByText(/「応募者一覧」は見られなくなりました/)).toBeTruthy();
  expect(screen.getByText('見られるシートがまだありません')).toBeTruthy();
  expect(calls.filter((c) => c === '/api/portal/sheets')).toHaveLength(2);
});

it('読み直しが 503 sheet_unavailable なら内容を消す。sheet_busy なら内容を残して帯で知らせる', async () => {
  reply('/api/portal/sheets', 200, { sheets: [{ id: 's1', label: '応募者一覧' }] });
  reply('/api/portal/sheets/s1/rows', 200, rows);
  reply('/api/portal/sheets/s1/version', 503, { error: 'sheet_busy', message: 'いまはシートを読めません' });
  reply('/api/portal/sheets/s1/version', 200, { version: 'v2' });
  renderScreen();
  expect((await screen.findAllByText(NAME)).length).toBeGreaterThan(0);
  // 一時的な混雑: 内容は残し、帯を出す
  await checkNow();
  expect(await screen.findByText(/シートを確かめられませんでした/)).toBeTruthy();
  expect(screen.queryAllByText(NAME).length).toBeGreaterThan(0);
  // 版が変わって読み直したら、共有が外れていた
  replies['/api/portal/sheets/s1/rows'] = [{ status: 503, body: { error: 'sheet_unavailable', message: 'シートを読めません' } }];
  await checkNow();
  expect(await screen.findByText('このシートは見られなくなりました')).toBeTruthy();
  expect(screen.queryAllByText(NAME)).toHaveLength(0);
  expect(calls.filter((c) => c === '/api/portal/sheets')).toHaveLength(2);
});
