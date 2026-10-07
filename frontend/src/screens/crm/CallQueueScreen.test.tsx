// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError } from '../../api/client';
import { CallQueueScreen, partialNotes } from './CallQueueScreen';
import { makeItem, makeResponse, deferredFetcher } from './queueTestUtil';
import { DEFAULT_FILTERS, parseFilters } from './queueModel';

afterEach(() => { cleanup(); });

function at<T>(list: T[], index: number): T {
  const v = list[index];
  if (v === undefined) throw new Error(`missing element ${String(index)}`);
  return v;
}

async function ready(calls: ReturnType<typeof deferredFetcher>['calls'], items = [makeItem('1')], over = {}) {
  await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(calls[0].filters, items, over) }); await Promise.resolve(); });
}

describe('CallQueueScreen', () => {
  it('shows loading, then rows with domestic phone numbers (raw value kept in the title) and the live-mode badge', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen fetcher={fetcher} initialSearch="?view=queue" />);
    expect(screen.getByText('読み込み中…')).toBeTruthy();
    expect(screen.getByText('実データ(HubSpot)')).toBeTruthy();
    await ready(calls, [
      makeItem('1', { phone: '+81312345678' }),
      makeItem('2', { phone: '03-1111-2222', phone_source: 'deal', next_call_date: '2026-10-05', next_call_time: '10:30', stop: { prohibited_reason: null, block_reason: null, unreachable_check: '通話中' } }),
      makeItem('3', { phone: null, phone_source: null, contact: null, company: null, owner_id: null }),
    ]);
    const row1 = screen.getByText('0312345678');
    expect(row1.getAttribute('title')).toBe('+81312345678');
    expect(screen.getByText('0311112222')).toBeTruthy();
    expect(screen.getByText('2026/10/05')).toBeTruthy();
    expect(screen.getByText('10:30')).toBeTruthy();
    expect(screen.getByText('不通時チェック: 通話中')).toBeTruthy();
    expect(screen.getByText('番号を確認できません')).toBeTruthy();
    expect(screen.getByText('担当者情報を取得できませんでした')).toBeTruthy();
    expect(screen.getByText('3 件を表示')).toBeTruthy();
  });

  it('empty: with no conditions and with conditions show different messages; no cursor means no load-more button', async () => {
    const a = deferredFetcher();
    render(<CallQueueScreen fetcher={a.fetcher} initialSearch="?view=queue" />);
    await ready(a.calls, []);
    expect(screen.getByText('いま架電キューに出ている架電先はありません')).toBeTruthy();
    expect(screen.queryByText('さらに読み込む')).toBeNull();
    cleanup();
    const b = deferredFetcher();
    render(<CallQueueScreen fetcher={b.fetcher} initialSearch="?view=queue&due=today" />);
    await ready(b.calls, []);
    expect(screen.getByText('条件に一致する架電先がありません')).toBeTruthy();
  });

  it('an empty page that still has a cursor offers to load more instead of saying there is nothing', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen fetcher={fetcher} initialSearch="?view=queue" />);
    await ready(calls, [], { next_cursor: 'c1' });
    expect(screen.getByText('このページには表示できる行がありません。続きを読み込んでください。')).toBeTruthy();
    expect(screen.getByText('さらに読み込む')).toBeTruthy();
  });

  it('partial: shows missing counts, failed parts and excluded counts; truncated shows the limit notice', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen fetcher={fetcher} initialSearch="?view=queue" />);
    await ready(calls, [makeItem('1', { contact: null })], {
      truncated: true,
      partial: { missing_contacts: 1, missing_companies: 2, failed: ['contacts'], excluded: { no_phone: 3, stop_reason: 4, out_of_scope: 5 } },
    });
    expect(screen.getByText('一部の情報が欠けています')).toBeTruthy();
    expect(screen.getByText('担当者情報を取得できなかった行が 1 件あります')).toBeTruthy();
    expect(screen.getByText('会社情報を取得できなかった行が 2 件あります')).toBeTruthy();
    expect(screen.getByText(/取得に失敗した部分: contacts/)).toBeTruthy();
    expect(screen.getByText('電話番号がどこにも無いため 3 件を除きました')).toBeTruthy();
    expect(screen.getByText(/1 万件までしか取得できない/)).toBeTruthy();
    expect(partialNotes(null)).toEqual([]);
    expect(partialNotes({ missing_contacts: 0, missing_companies: 0, failed: [], excluded: { no_phone: 0, stop_reason: 0, out_of_scope: 0 } })).toEqual([]);
  });

  it('error: shows the kind-specific message and retries from the first page; it does not show fixture rows', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen fetcher={fetcher} initialSearch="?view=queue" />);
    await act(async () => { calls[0]?.resolve({ ok: false, error: new ApiHttpError(503, { error_kind: 'hubspot_rate_limited' }) }); await Promise.resolve(); });
    expect(screen.getByRole('alert').textContent).toContain('呼び出し回数の上限');
    expect(screen.queryByRole('list', { name: '架電キュー' })).toBeNull();
    fireEvent.click(screen.getByText('再試行'));
    expect(calls).toHaveLength(2);
    expect(calls[1]?.cursor).toBeNull();
  });

  it('unauthorized: 401 shows the login message; plain 403 shows the permission message (no owner-specific dead end any more)', async () => {
    const a = deferredFetcher();
    render(<CallQueueScreen fetcher={a.fetcher} initialSearch="?view=queue" />);
    await act(async () => { a.calls[0]?.resolve({ ok: false, error: new ApiHttpError(401) }); await Promise.resolve(); });
    expect(screen.getByRole('alert').textContent).toContain('ログインが必要');
    cleanup();
    const b = deferredFetcher();
    render(<CallQueueScreen fetcher={b.fetcher} initialSearch="?view=queue&owner=unassigned" />);
    await act(async () => { b.calls[0]?.resolve({ ok: false, error: new ApiHttpError(403, { error_kind: 'forbidden' }) }); await Promise.resolve(); });
    expect(screen.getByRole('alert').textContent).toContain('権限がありません');
    expect(screen.queryByText('担当者の指定を外す')).toBeNull();
  });

  it('restores every condition from the URL and sends exactly those to the fetcher', () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen fetcher={fetcher} initialSearch="?view=queue&q=架空&stage=1095387445&due=today&sort=next_call_desc&next_from=2026-10-01&next_to=2026-10-31&last_from=2026-09-01&last_to=2026-09-30" />);
    expect(calls[0]?.filters).toEqual(parseFilters('?q=架空&stage=1095387445&due=today&sort=next_call_desc&next_from=2026-10-01&next_to=2026-10-31&last_from=2026-09-01&last_to=2026-09-30'));
    expect((screen.getByLabelText<HTMLSelectElement>('並び替え')).value).toBe('next_call_desc');
    expect((screen.getAllByLabelText('から')[0] as HTMLInputElement).value).toBe('2026-10-01');
    expect((screen.getAllByLabelText('まで')[1] as HTMLInputElement).value).toBe('2026-09-30');
    expect((screen.getByLabelText<HTMLInputElement>('次回日が来たものだけ')).checked).toBe(true);
    expect((screen.getByLabelText<HTMLInputElement>('不在')).checked).toBe(true);
    expect((screen.getByLabelText<HTMLInputElement>('キーワード(会社名・案件名)')).value).toBe('架空');
  });

  it('changing the sort, due toggle, stage and dates aborts the old request and refetches with the new condition', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen fetcher={fetcher} initialSearch="?view=queue" />);
    fireEvent.change(screen.getByLabelText('並び替え'), { target: { value: 'last_call_asc' } });
    expect(calls).toHaveLength(2);
    expect(calls[0]?.signal.aborted).toBe(true);
    expect(calls[1]?.filters.sort).toBe('last_call_asc');
    fireEvent.click(screen.getByLabelText('次回日が来たものだけ'));
    expect(calls[2]?.filters.due).toBe('today');
    fireEvent.click(screen.getByLabelText('不在'));
    expect(calls[3]?.filters.stages).toEqual(['1095387445']);
    const dates = screen.getAllByLabelText('から');
    fireEvent.change(at(dates, 0), { target: { value: '2026-10-01' } });
    expect(calls[4]?.filters.nextFrom).toBe('2026-10-01');
    // 範囲の逆転は取得せずに知らせる
    const tos = screen.getAllByLabelText('まで');
    fireEvent.change(at(tos, 0), { target: { value: '2026-09-01' } });
    expect(calls).toHaveLength(5);
    expect(screen.getByRole('alert').textContent).toContain('次回架電日の開始日が終了日より後');
    // 古い (最初の) 応答が今ごろ返っても何も出ない
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(DEFAULT_FILTERS, [makeItem('stale')]) }); await Promise.resolve(); });
    expect(screen.queryByText('架空案件stale')).toBeNull();
  });

  it('the keyword is applied after typing stops (debounced), once', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen fetcher={fetcher} initialSearch="?view=queue" />);
    const input = screen.getByLabelText('キーワード(会社名・案件名)');
    fireEvent.change(input, { target: { value: '架' } });
    fireEvent.change(input, { target: { value: '架空' } });
    expect(calls).toHaveLength(1);
    await waitFor(() => { expect(calls).toHaveLength(2); });
    expect(calls[1]?.filters.q).toBe('架空');
  });

  it('owner control is for everyone; choosing unassigned refetches with owner=unassigned, and the note names the shown owner', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen fetcher={fetcher} initialSearch="?view=queue" />);
    await ready(calls, [makeItem('1')], {});
    const owner = screen.getByLabelText('所有者');
    expect((owner as HTMLSelectElement).value).toBe('all');
    expect(screen.getByTestId('scope-note').textContent).toContain('所有者: 全員 を表示中');
    fireEvent.change(owner, { target: { value: 'unassigned' } });
    expect(calls[1]?.filters.owner).toBe('unassigned');
    // 管理者でない人 (role=own) にも出る。既定は自分
    cleanup();
    const b = deferredFetcher();
    render(<CallQueueScreen fetcher={b.fetcher} initialSearch="?view=queue" />);
    const resp = makeResponse(DEFAULT_FILTERS, [makeItem('1')]);
    await act(async () => { b.calls[0]?.resolve({ ok: true, data: { ...resp, scope: { ...resp.scope, role: 'own', owner: 'me' } } }); await Promise.resolve(); });
    expect(screen.getByLabelText<HTMLSelectElement>('所有者').value).toBe('me');
    expect(screen.getByTestId('scope-note').textContent).not.toContain('自分の担当分だけ');
  });

  it('load more appends below, drops duplicates, and shows the end marker', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen fetcher={fetcher} initialSearch="?view=queue" />);
    await ready(calls, [makeItem('1'), makeItem('2')], { next_cursor: 'c1' });
    fireEvent.click(screen.getByText('さらに読み込む'));
    expect(calls[1]?.cursor).toBe('c1');
    await act(async () => { calls[1]?.resolve({ ok: true, data: makeResponse(calls[1].filters, [makeItem('2'), makeItem('3')]) }); await Promise.resolve(); });
    const table = screen.getByRole('list', { name: '架電キュー' });
    expect(within(table).getAllByText(/^架空会社/).map(e => e.textContent)).toEqual(['架空会社1', '架空会社2', '架空会社3']);
    expect(screen.getByText('これで最後です。')).toBeTruthy();
  });

  it('fixture mode is explicit: fictional rows without any request, and switching to live calls the server (and fails visibly, not back to fixture)', async () => {
    const spy = vi.fn<typeof fetch>(() => Promise.reject(new TypeError('offline')));
    vi.stubGlobal('fetch', spy);
    try {
      render(<CallQueueScreen initialSearch="?view=queue&mode=fixture" />);
      expect(screen.getByText('表示内容はすべて架空です。HubSpot には接続しません。')).toBeTruthy();
      await waitFor(() => { expect(screen.getByText('架空食品株式会社')).toBeTruthy(); });
      expect(spy).not.toHaveBeenCalled();
      fireEvent.click(screen.getByRole('button', { name: '実データ' }));
      expect(screen.getByText('実データ(HubSpot)')).toBeTruthy();
      await waitFor(() => { expect(screen.getByRole('alert').textContent).toContain('ネットワーク'); });
      // キュー (1 回) と、所有者の一覧 (全員が使う。実データのときだけ) だけ
      const urls = spy.mock.calls.map(c => (typeof c[0] === 'string' ? c[0] : ''));
      expect(urls.filter(u => u.startsWith('/api/crm/call-queue'))).toEqual(['/api/crm/call-queue?limit=25']);
      expect(urls.filter(u => !u.startsWith('/api/crm/call-queue'))).toEqual(['/api/crm/owners']);
      expect(screen.queryByText('架空食品株式会社')).toBeNull();
    } finally { vi.unstubAllGlobals(); }
  });
});
