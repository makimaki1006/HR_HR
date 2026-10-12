// @vitest-environment happy-dom
import { toLive } from './barMenuTestUtil';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError } from '../../api/client';
import { CallQueueScreen, KEY_SELECT_DELAY_MS, partialNotes } from './CallQueueScreen';
import type { DetailFetch } from './useDealDetail';
import type { OwnersFetch } from './useOwners';
import { makeItem, makeResponse, deferredFetcher, okMetadataFetch, okPipelinesFetch, okUserFetch, pipelinesResponse, okCatalogFetch, resetDockStorage } from './queueTestUtil';
import { DEFAULT_FILTERS, parseFilters, groupsOf } from './queueModel';

beforeEach(() => { resetDockStorage(); });
afterEach(() => { cleanup(); });

/** 詳細条件の欄を開く (ステージの絞り込みはこの中) */
function openDetails() {
  const toggle = screen.getByRole('button', { name: /^詳細条件/ });
  if (toggle.getAttribute('aria-expanded') !== 'true') fireEvent.click(toggle);
}

function at<T>(list: T[], index: number): T {
  const v = list[index];
  if (v === undefined) throw new Error(`missing element ${String(index)}`);
  return v;
}

async function ready(calls: ReturnType<typeof deferredFetcher>['calls'], items = [makeItem('1')], over = {}) {
  await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(calls[0].filters, items, over) }); await Promise.resolve(); });
}

describe('CallQueueScreen', () => {
  it('shows loading, then rows with domestic phone numbers (raw value kept in the title) and no data-type banner in live mode', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue" />);
    expect(screen.getByText('読み込み中…')).toBeTruthy();
    expect(screen.queryByTestId('fixture-banner')).toBeNull();
    await ready(calls, [
      makeItem('1', { phone: '+81312345678' }),
      makeItem('2', { phone: '03-1111-2222', phone_source: 'deal', next_call_date: '2026-10-05', next_call_time: '10:30', stop: { prohibited_reason: null, block_reason: null, unreachable_check: '通話中' } }),
      makeItem('3', { phone: null, phone_source: null, contact: null, company: null, owner_id: null }),
    ]);
    // 表示はハイフン区切り。元の値は title に残す
    const row1 = screen.getByText('03-1234-5678');
    expect(row1.getAttribute('title')).toBe('+81312345678');
    expect(screen.getByText('03-1111-2222')).toBeTruthy();
    expect(screen.getByText('10/05')).toBeTruthy();
    expect(screen.getByText('10:30')).toBeTruthy();
    expect(screen.getByTitle('不通時チェック: 通話中').textContent).toBe('不通チェック');
    expect(screen.getByText('番号を確認できません')).toBeTruthy();
    expect(screen.getByText('担当者情報を取得できませんでした')).toBeTruthy();
    expect(screen.getByTestId('queue-count').textContent).toBe('全 3 件中 3 件を表示(全件数は電話番号のない架電先なども含む)');
  });

  it('empty: with no conditions and with conditions show different messages; no cursor means no load-more button', async () => {
    const a = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={a.fetcher} initialSearch="?view=queue" />);
    await ready(a.calls, []);
    expect(screen.getByText('いま架電キューに出ている架電先はありません')).toBeTruthy();
    expect(screen.queryByText('さらに読み込む')).toBeNull();
    cleanup();
    const b = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={b.fetcher} initialSearch="?view=queue&due=today" />);
    await ready(b.calls, []);
    expect(screen.getByText('条件に一致する架電先がありません')).toBeTruthy();
  });

  it('an empty page that still has a cursor offers to load more instead of saying there is nothing', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue" />);
    await ready(calls, [], { next_cursor: 'c1' });
    expect(screen.getByText('このページには表示できる行がありません。続きを読み込んでください。')).toBeTruthy();
    expect(screen.getByText('さらに読み込む')).toBeTruthy();
  });

  it('partial: shows missing counts, failed parts and excluded counts; truncated shows the limit notice', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue" />);
    await ready(calls, [makeItem('1', { contact: null })], {
      truncated: true,
      partial: { missing_contacts: 1, missing_companies: 2, failed: ['associations', 'contacts', 'stage_labels', 'something_new'], excluded: { no_phone: 3, stop_reason: 4, out_of_scope: 5 }, unknown_stages: 0 },
    });
    expect(screen.getByText('一部の情報が欠けています')).toBeTruthy();
    expect(screen.getByText('担当者情報を取得できなかった行が 1 件あります')).toBeTruthy();
    expect(screen.getByText('会社情報を取得できなかった行が 2 件あります')).toBeTruthy();
    // 部分の名前は日本語 (サーバの内部名は出さない)
    expect(screen.getByText('取得に失敗した部分: 担当者・会社との関連、担当者、ステージ名、その他の情報(関連情報を表示できない行があります)')).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/associations|stage_labels|something_new/);
    expect(screen.getByText('電話番号がどこにも無いため 3 件を除きました')).toBeTruthy();
    expect(screen.getByText(/1 万件までしか取得できない/)).toBeTruthy();
    expect(partialNotes(null)).toEqual([]);
    expect(partialNotes({ missing_contacts: 0, missing_companies: 0, failed: [], excluded: { no_phone: 0, stop_reason: 0, out_of_scope: 0 }, unknown_stages: 0 })).toEqual([]);
    expect(partialNotes({ missing_contacts: 0, missing_companies: 0, failed: [], excluded: { no_phone: 0, stop_reason: 0, out_of_scope: 0 }, unknown_stages: 2 }))
      .toEqual(['架電キューの設定に無いステージが HubSpot に 2 個あり、架電対象外として扱っています(管理者に連絡してください)']);
  });

  it('error: shows the kind-specific message and retries from the first page; it does not show fixture rows', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue" />);
    await act(async () => { calls[0]?.resolve({ ok: false, error: new ApiHttpError(503, { error_kind: 'hubspot_rate_limited' }) }); await Promise.resolve(); });
    expect(screen.getByRole('alert').textContent).toContain('呼び出し回数の上限');
    expect(screen.queryByRole('list', { name: '架電キュー' })).toBeNull();
    fireEvent.click(screen.getByText('再試行'));
    expect(calls).toHaveLength(2);
    expect(calls[1]?.cursor).toBeNull();
  });

  it('unauthorized: 401 shows the login message; plain 403 shows the permission message (no owner-specific dead end any more)', async () => {
    const a = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={a.fetcher} initialSearch="?view=queue" />);
    await act(async () => { a.calls[0]?.resolve({ ok: false, error: new ApiHttpError(401) }); await Promise.resolve(); });
    expect(screen.getByRole('alert').textContent).toContain('ログインが必要');
    cleanup();
    const b = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={b.fetcher} initialSearch="?view=queue&owner=unassigned" />);
    await act(async () => { b.calls[0]?.resolve({ ok: false, error: new ApiHttpError(403, { error_kind: 'forbidden' }) }); await Promise.resolve(); });
    expect(screen.getByRole('alert').textContent).toContain('権限がありません');
    expect(screen.queryByText('担当者の指定を外す')).toBeNull();
  });

  it('restores every condition from the URL and sends exactly those to the fetcher', () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue&q=架空&stage=1095387445&due=today&sort=next_call_desc&next_from=2026-10-01&next_to=2026-10-31&last_from=2026-09-01&last_to=2026-09-30" />);
    expect(calls[0]?.filters).toEqual(parseFilters('?q=架空&stage=1095387445&due=today&sort=next_call_desc&next_from=2026-10-01&next_to=2026-10-31&last_from=2026-09-01&last_to=2026-09-30'));
    expect((screen.getByLabelText<HTMLSelectElement>('並び替え')).value).toBe('next_call_desc');
    expect((screen.getAllByRole('textbox', { name: 'から', hidden: true })[0] as HTMLInputElement).value).toBe('2026/10/01');
    expect((screen.getAllByRole('textbox', { name: 'まで', hidden: true })[1] as HTMLInputElement).value).toBe('2026/09/30');
    expect((screen.getByLabelText<HTMLInputElement>('次回日が来たものだけ')).checked).toBe(true);
    openDetails();
    expect(screen.getByRole('button', { name: 'ステージ（1件選択）' })).toBeTruthy();
    expect(screen.getByRole('button',{name:'パイプライン'}).textContent).toContain('bpo_リクロジ');
    expect((screen.getByLabelText<HTMLInputElement>('キーワード(会社名・案件名)')).value).toBe('架空');
  });

  it('changing the sort, due toggle, stage and dates aborts the old request and refetches with the new condition', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} pipelinesFetcher={okPipelinesFetch} initialSearch="?view=queue" />);
    await waitFor(() => { expect(screen.getByRole('button', { name: 'パイプライン' }).textContent).toContain('bpo_リクロジ'); });
    fireEvent.change(screen.getByLabelText('並び替え'), { target: { value: 'last_call_asc' } });
    expect(calls).toHaveLength(2);
    expect(calls[0]?.signal.aborted).toBe(true);
    expect(calls[1]?.filters.sort).toBe('last_call_asc');
    fireEvent.click(screen.getByLabelText('次回日が来たものだけ'));
    expect(calls[2]?.filters.due).toBe('today');
    openDetails();
    fireEvent.click(screen.getByRole('button', { name: 'ステージ（16件選択）' }));
    fireEvent.click(screen.getByRole('button', { name: 'すべて外す' }));
    fireEvent.click(screen.getByLabelText(/^不在/));
    fireEvent.click(screen.getByRole('button', { name: '適用' }));
    expect(calls[3]?.filters.stages).toEqual(['1095387445']);
    const dates = screen.getAllByRole('textbox', { name: 'から', hidden: true });
    fireEvent.change(at(dates, 0), { target: { value: '2026-10-01' } });
    expect(calls[4]?.filters.nextFrom).toBe('2026-10-01');
    // 範囲の逆転は取得せずに知らせる
    const tos = screen.getAllByRole('textbox', { name: 'まで', hidden: true });
    fireEvent.change(at(tos, 0), { target: { value: '2026-09-01' } });
    expect(calls).toHaveLength(5);
    expect(screen.getByRole('alert').textContent).toContain('次回架電日の開始日が終了日より後');
    // 古い (最初の) 応答が今ごろ返っても何も出ない
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(DEFAULT_FILTERS, [makeItem('stale')]) }); await Promise.resolve(); });
    expect(screen.queryByText('架空案件stale')).toBeNull();
  });

  it('the keyword is applied after typing stops (debounced), once', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue" />);
    const input = screen.getByLabelText('キーワード(会社名・案件名)');
    fireEvent.change(input, { target: { value: '架' } });
    fireEvent.change(input, { target: { value: '架空' } });
    expect(calls).toHaveLength(1);
    await waitFor(() => { expect(calls).toHaveLength(2); });
    expect(calls[1]?.filters.q).toBe('架空');
  });

  it('owner control is for everyone; choosing unassigned refetches with owner=unassigned, and the note names the shown owner', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue" />);
    await ready(calls, [makeItem('1')], {});
    const owner = screen.getByLabelText('所有者');
    expect((owner as HTMLSelectElement).value).toBe('all');
    expect(screen.getByTestId('scope-note').textContent).toContain('所有者: 全員 を表示中');
    fireEvent.change(owner, { target: { value: 'unassigned' } });
    expect(calls[1]?.filters.owner).toBe('unassigned');
    // 管理者でない人 (role=own) にも出る。既定は自分
    cleanup();
    const b = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={b.fetcher} initialSearch="?view=queue" />);
    const resp = makeResponse(DEFAULT_FILTERS, [makeItem('1')]);
    await act(async () => { b.calls[0]?.resolve({ ok: true, data: { ...resp, scope: { ...resp.scope, role: 'own', owner: 'me' } } }); await Promise.resolve(); });
    expect(screen.getByLabelText<HTMLSelectElement>('所有者').value).toBe('me');
    expect(screen.getByTestId('scope-note').textContent).not.toContain('自分の担当分だけ');
  });

  it('load more appends below, drops duplicates, and shows the end marker', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue" />);
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
      render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} initialSearch="?view=queue&mode=fixture" />);
      expect(screen.getByTestId('fixture-banner').textContent).toContain('架空サンプルを表示中です。表示内容はすべて架空で、HubSpot には接続しません。');
      await waitFor(() => { expect(screen.getByText('架空食品株式会社')).toBeTruthy(); });
      // 架空サンプルでも全体の件数を出す (1 ページ 5 件)
      expect(screen.getByTestId('queue-count').textContent).toBe('全 10 件中 5 件を表示(全件数は電話番号のない架電先なども含む)');
      expect(spy).not.toHaveBeenCalled();
      toLive();
      expect(screen.queryByTestId('fixture-banner')).toBeNull();
      await waitFor(() => { expect(screen.getByRole('alert').textContent).toContain('ネットワーク'); });
      // キュー (1 回) と、所有者の一覧 (全員が使う。実データのときだけ) だけ
      const urls = spy.mock.calls.map(c => (typeof c[0] === 'string' ? c[0] : ''));
      expect(urls.filter(u => u.startsWith('/api/crm/call-queue?'))).toEqual(['/api/crm/call-queue?limit=50']);
      // ほかは所有者の一覧とパイプライン名 (どちらも実データのときだけ)
      expect(urls.filter(u => !u.startsWith('/api/crm/call-queue?')).sort()).toEqual(['/api/crm/call-queue/pipelines', '/api/crm/metrics/today', '/api/crm/owners', '/api/crm/views']);
      expect(screen.queryByText('架空食品株式会社')).toBeNull();
    } finally { vi.unstubAllGlobals(); }
  });
});

describe('calling cockpit layout', () => {
  const ownersFetcher: OwnersFetch = () => Promise.resolve({ ok: true, data: {
    owners: [{ id: '9001', name: '架空 担当', email: null, archived: false, teams: [] }], truncated: false, generated_at: '2026-10-05T00:00:00Z',
  } });
  function detailStub() {
    const ids: string[] = [];
    const fetcher: DetailFetch = (id) => { ids.push(id); return new Promise(() => { /* 応答しない (読み込み中のまま) */ }); };
    return { ids, fetcher };
  }

  it('a queue row is three compact lines: company + flag + stage / contact · formatted phone / next, last, owner', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} ownersFetcher={ownersFetcher} initialSearch="" />);
    await ready(calls, [makeItem('1', {
      stage_label: '不在', phone: '+81300000005', next_call_date: '2026-10-05', next_call_time: '10:30', last_call_date: '2026-10-01',
      contact: { id: 'c1', name: '架空 太郎1', phone: null, mobile: null, job_title: '採用担当', extra_count: 2 },
      stop: { prohibited_reason: null, block_reason: null, unreachable_check: '通話中' },
    })]);
    await waitFor(() => { expect(screen.getByText('架空 担当')).toBeTruthy(); });
    const row = within(screen.getByRole('list', { name: '架電キュー' })).getByRole('button');
    const lines = Array.from(row.children).map(el => el.textContent);
    expect(lines).toEqual([
      '架空会社1不通チェック不在',
      '架空 太郎1 ほか2人·03-0000-0005',
      '次回 10/05 10:30最終 10/01担当 架空 担当',
    ]);
    // 案件名と年つきの日付は title で見られる
    expect(row.getAttribute('title')).toBe('架空案件1');
    expect(row.lastElementChild?.getAttribute('title')).toBe('次回架電 2026/10/05 / 最終架電 2026/10/01');
  });

  it('while owner names are not loaded (or failed), a row says 担当あり and keeps the HubSpot owner ID out of the text (tooltip only)', async () => {
    const { calls, fetcher } = deferredFetcher();
    const failingOwners: OwnersFetch = () => Promise.resolve({ ok: false, error: new ApiHttpError(503, { error_kind: 'hubspot_unavailable' }) });
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} ownersFetcher={failingOwners} initialSearch="" />);
    await ready(calls, [makeItem('1'), makeItem('2', { owner_id: null })]);
    const rows = within(screen.getByRole('list', { name: '架電キュー' })).getAllByRole('button');
    const last1 = rows[0]?.lastElementChild;
    expect(last1?.textContent).toBe('次回 なし最終 未架電担当 担当あり');
    expect(last1?.textContent).not.toContain('9001');
    expect(screen.getByText('担当あり').getAttribute('title')).toBe('HubSpot の所有者 ID: 9001');
    expect(rows[1]?.lastElementChild?.textContent).toBe('次回 なし最終 未架電担当 なし');
  });

  it('there is no dark top bar in live mode: no title row, no data-type badge, no layout-reset / mode buttons outside the 「…」 menu', async () => {
    const { calls, fetcher } = deferredFetcher();
    const { container } = render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="" />);
    await ready(calls);
    expect(container.querySelector('header.crm-topbar, .cq-topbar')).toBeNull();
    expect(screen.queryByRole('heading', { level: 1 })).toBeNull();
    expect(screen.queryByText('HR_HR')).toBeNull();
    expect(screen.queryByText('実データ(HubSpot)')).toBeNull();
    expect(screen.queryByText('HubSpot への書き込みはしません')).toBeNull();
    expect(screen.queryByRole('status', { name: 'データの種類' })).toBeNull();
    expect(screen.queryByRole('group', { name: 'データの切り替え' })).toBeNull();
    expect(screen.queryByRole('button', { name: '元の配置に戻す' })).toBeNull();
    expect(screen.queryByTestId('zoom-toggle')).toBeNull();
    expect(screen.queryByTestId('zoom-warning')).toBeNull();
  });

  it('fixture mode shows a banner with the exact notice, and 「実データに戻す」 switches back to live (banner gone)', async () => {
    const spy = vi.fn<typeof fetch>(() => Promise.reject(new TypeError('offline')));
    vi.stubGlobal('fetch', spy);
    try {
      render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} initialSearch="?view=queue&mode=fixture" />);
      const banner = screen.getByRole('status', { name: 'データの種類' });
      expect(banner.textContent).toBe('架空サンプルを表示中です。表示内容はすべて架空で、HubSpot には接続しません。実データに戻す');
      fireEvent.click(within(banner).getByRole('button', { name: '実データに戻す' }));
      expect(screen.queryByRole('status', { name: 'データの種類' })).toBeNull();
      await waitFor(() => { expect(screen.getByRole('alert').textContent).toContain('ネットワーク'); });
    } finally { vi.unstubAllGlobals(); }
  });

  it('the 「…」 menu at the end of the filter bar: ARIA menu button, two items, Esc / outside click close it, arrow keys move, 架空サンプルを表示 switches mode', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="" />);
    await ready(calls);
    const bar = screen.getByRole('form', { name: '絞り込みと並び替え' });
    const btn = within(bar).getByRole('button', { name: 'その他の操作' });
    // 絞り込みバーの行の最後の要素
    expect(btn.closest('.cq-bar')?.lastElementChild?.contains(btn)).toBe(true);
    expect(btn.textContent).toBe('…');
    expect(btn.getAttribute('aria-haspopup')).toBe('menu');
    expect(btn.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByRole('menu')).toBeNull();
    fireEvent.click(btn);
    expect(btn.getAttribute('aria-expanded')).toBe('true');
    const menu = screen.getByRole('menu', { name: 'その他の操作' });
    expect(within(menu).getAllByRole('menuitem').map(i => i.textContent)).toEqual(['元の配置に戻す', '架空サンプルを表示']);
    // 開くと先頭の項目にフォーカス、矢印で移る
    const [reset, fixture] = within(menu).getAllByRole('menuitem');
    expect(document.activeElement).toBe(reset);
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(fixture);
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(reset);
    // Esc で閉じ、フォーカスは「…」へ
    fireEvent.keyDown(menu, { key: 'Escape' });
    expect(screen.queryByRole('menu')).toBeNull();
    expect(document.activeElement).toBe(btn);
    // 外側のクリックで閉じる
    fireEvent.click(btn);
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole('menu')).toBeNull();
    // 元の配置に戻す: メニューが閉じる
    fireEvent.click(btn);
    fireEvent.click(screen.getByRole('menuitem', { name: '元の配置に戻す' }));
    expect(screen.queryByRole('menu')).toBeNull();
    // 架空サンプルを表示 → 帯が出て、メニューの項目は「実データに戻す」に変わる
    fireEvent.click(btn);
    fireEvent.click(screen.getByRole('menuitem', { name: '架空サンプルを表示' }));
    expect(screen.getByTestId('fixture-banner')).toBeTruthy();
    fireEvent.click(btn);
    expect(within(screen.getByRole('menu')).getAllByRole('menuitem').map(i => i.textContent)).toEqual(['元の配置に戻す', '実データに戻す']);
    fireEvent.click(screen.getByRole('menuitem', { name: '実データに戻す' }));
    expect(screen.queryByTestId('fixture-banner')).toBeNull();
  });

  it('詳細条件 opens and closes the stage / date panel; the button counts the detailed conditions in use', () => {
    const { fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?stage=1095387445&stage=1095387443&next_from=2026-10-01" />);
    const toggle = screen.getByRole('button', { name: /^詳細条件/ });
    const panel = document.getElementById('cq-advanced');
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(panel?.hidden).toBe(true);
    // ステージの絞り込み (2 つ選択) で 1、次回架電日で 1
    expect(toggle.textContent).toBe('詳細条件2');
    fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(panel?.hidden).toBe(false);
    fireEvent.click(toggle);
    expect(panel?.hidden).toBe(true);
  });

  it('active conditions are listed as chips; × removes exactly that condition and refetches', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} pipelinesFetcher={okPipelinesFetch}
      initialSearch="?q=架空&stage=1095387442&stage=1095387445&due=today&next_from=2026-10-01&last_to=2026-09-30&sort=next_call_desc" />);
    const chips = () => within(screen.getByRole('list', { name: '適用中の条件' })).getAllByRole('listitem').map(li => li.querySelector('span')?.textContent);
    // ステージ名を読むまでは件数、読めたら名前
    expect(chips()).toContain('ステージ: 2 件を選択');
    await waitFor(() => { expect(chips()).toContain('ステージ: 未済、不在'); });
    expect(chips()).toEqual([
      'キーワード: 架空', '次回日が来たものだけ', 'ステージ: 未済、不在', '次回架電日: 2026/10/01〜', '最終架電日: 〜2026/09/30',
    ]);
    // ステージのチップを外すとすべてのステージに戻る
    fireEvent.click(screen.getByRole('button', { name: '「ステージ: 未済、不在」を外す' }));
    expect(calls.at(-1)?.filters.stages).toEqual([]);
    fireEvent.click(screen.getByRole('button', { name: '「キーワード: 架空」を外す' }));
    expect(calls.at(-1)?.filters.q).toBe('');
    expect(screen.getByLabelText<HTMLInputElement>('キーワード(会社名・案件名)').value).toBe('');
    fireEvent.click(screen.getByRole('button', { name: '「最終架電日: 〜2026/09/30」を外す' }));
    expect(calls.at(-1)?.filters.lastTo).toBe('');
    expect(calls.at(-1)?.filters.sort).toBe('next_call_desc');
    expect(chips()).toEqual(['次回日が来たものだけ', '次回架電日: 2026/10/01〜']);
    // 何も無ければチップの列は出ない
    fireEvent.click(screen.getByText('条件をクリア'));
    expect(screen.queryByRole('list', { name: '適用中の条件' })).toBeNull();
  });

  it('ArrowDown / ArrowUp move the selection within the list; the detail is fetched after a short pause, never showing the previous deal meanwhile', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      const { calls, fetcher } = deferredFetcher();
      const detail = detailStub();
      render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} detailFetcher={detail.fetcher} metadataFetcher={okMetadataFetch} initialSearch="" />);
      await ready(calls, [makeItem('1'), makeItem('2'), makeItem('3')]);
      const list = screen.getByRole('list', { name: '架電キュー' });
      const buttons = () => within(list).getAllByRole('button');
      // 何も選んでいなければ先頭の行だけがタブで入れる
      expect(buttons().map(b => b.tabIndex)).toEqual([0, -1, -1]);
      fireEvent.click(at(buttons(), 0));
      expect(detail.ids).toEqual(['1']);
      fireEvent.keyDown(at(buttons(), 0), { key: 'ArrowDown' });
      expect(buttons().map(b => b.getAttribute('aria-pressed'))).toEqual(['false', 'true', 'false']);
      expect(document.activeElement).toBe(at(buttons(), 1));
      expect(detail.ids).toEqual(['1']);
      expect(screen.getByText('詳細を読み込み中…')).toBeTruthy();
      fireEvent.keyDown(at(buttons(), 1), { key: 'ArrowDown' });
      fireEvent.keyDown(at(buttons(), 2), { key: 'ArrowDown' });
      expect(buttons().map(b => b.getAttribute('aria-pressed'))).toEqual(['false', 'false', 'true']);
      act(() => { vi.advanceTimersByTime(KEY_SELECT_DELAY_MS); });
      // 押し続けた途中の案件 2 は読まない
      expect(detail.ids).toEqual(['1', '3']);
      fireEvent.keyDown(at(buttons(), 2), { key: 'ArrowUp' });
      expect(buttons().map(b => b.getAttribute('aria-pressed'))).toEqual(['false', 'true', 'false']);
      expect(buttons().map(b => b.tabIndex)).toEqual([-1, 0, -1]);
    } finally { vi.useRealTimers(); }
  });

  it('the result slot (holding the call-result form) sits in the 架電結果の入力 panel (center column by default) only while a deal is selected', async () => {
    const { calls, fetcher } = deferredFetcher();
    const detail = detailStub();
    const { container } = render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} detailFetcher={detail.fetcher} metadataFetcher={okMetadataFetch} initialSearch="" />);
    await ready(calls, [makeItem('1'), makeItem('2')]);
    expect(container.querySelector('.cq-result-slot')).toBeNull();
    fireEvent.click(screen.getByText('架空会社2'));
    const slot = container.querySelector('.cq-result-slot');
    expect(slot?.getAttribute('data-deal-id')).toBe('2');
    expect(slot?.childElementCount).toBe(1);
    expect(slot?.firstElementChild?.getAttribute('aria-label')).toBe('架電結果の入力');
    expect(slot?.firstElementChild?.getAttribute('data-deal-id')).toBe('2');
    // 「架電結果の入力」のパネルの中 (既定の配置では中央の列のタブ。タブを切り替えても外さない)
    const panel = container.querySelector('#dock-panel-result');
    expect(panel?.contains(slot ?? null)).toBe(true);
    expect(panel?.closest('[data-testid="dock-col-1"]')).not.toBeNull();
  });

  it('the Zoom Phone iframe is the same element after switching deals and opening the panel (never remounted)', async () => {
    const { calls, fetcher } = deferredFetcher();
    const detail = detailStub();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} detailFetcher={detail.fetcher} metadataFetcher={okMetadataFetch} initialSearch="" />);
    await ready(calls, [makeItem('1'), makeItem('2')]);
    const iframe = screen.getByTitle('Zoom Phone');
    fireEvent.click(screen.getByText('架空会社1'));
    fireEvent.click(screen.getByText('架空会社2'));
    fireEvent.click(screen.getByRole('button', { name: /^詳細条件/ }));
    expect(screen.getByTitle('Zoom Phone')).toBe(iframe);
    expect(detail.ids).toEqual(['1', '2']);
  });
});

/** テスト用の IntersectionObserver (見えた・見えなくなったをテストが起こす) */
class FakeObserver {
  static all: FakeObserver[] = [];
  readonly targets: Element[] = [];
  disconnected = false;
  constructor(readonly cb: IntersectionObserverCallback, readonly options?: IntersectionObserverInit) { FakeObserver.all.push(this); }
  observe(t: Element) { this.targets.push(t); }
  unobserve() { /* 使わない */ }
  disconnect() { this.disconnected = true; }
  takeRecords(): IntersectionObserverEntry[] { return []; }
  static live() { return FakeObserver.all.filter(o => !o.disconnected); }
}

/** 目印が見えたことを、いま動いている observer に伝える */
function intersect(isIntersecting = true) {
  act(() => {
    for (const o of FakeObserver.live()) {
      const entries = o.targets.map(target => ({ isIntersecting, target }) as unknown as IntersectionObserverEntry);
      o.cb(entries, o as unknown as IntersectionObserver);
    }
  });
}

/** 一覧の枠をスクロールした状態にする (happy-dom は配置を計算しないので scrollTop を直接決める) */
function scrollList(top: number) {
  const scroller = document.querySelector('.cq-list-scroll');
  if (!(scroller instanceof HTMLElement)) throw new Error('no scroller');
  Object.defineProperty(scroller, 'scrollTop', { configurable: true, writable: true, value: top });
  return scroller;
}

describe('CallQueueScreen: total and auto-load', () => {
  afterEach(() => { FakeObserver.all = []; vi.unstubAllGlobals(); });

  it('shows the HubSpot total with thousands separators, and keeps it after the next page (which has no total)', async () => {
    vi.stubGlobal('IntersectionObserver', FakeObserver);
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue" />);
    const fifty = Array.from({ length: 50 }, (_, i) => makeItem(String(i + 1)));
    await ready(calls, fifty, { total: 22864, next_cursor: 'c1' });
    const count = screen.getByTestId('queue-count');
    expect(count.textContent).toBe('全 22,864 件中 50 件を表示(全件数は電話番号のない架電先なども含む)');
    expect(count.getAttribute('title')).toContain('電話番号がない架電先');
    fireEvent.click(screen.getByText('さらに読み込む'));
    await act(async () => {
      calls[1]?.resolve({ ok: true, data: makeResponse(calls[1].filters, [makeItem('51'), makeItem('52')], { total: null }) });
      await Promise.resolve();
    });
    expect(screen.getByTestId('queue-count').textContent).toBe('全 22,864 件中 52 件を表示(全件数は電話番号のない架電先なども含む)');
  });

  it('without a known total shows only the loaded count', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue" />);
    await ready(calls, [makeItem('1'), makeItem('2')], { total: null });
    expect(screen.getByTestId('queue-count').textContent).toBe('2 件を表示');
    expect(screen.getByTestId('queue-count').getAttribute('title')).toBeNull();
  });

  it('scrolling to the sentinel loads exactly one next page; nothing more while it loads; re-arms for the next cursor', async () => {
    vi.stubGlobal('IntersectionObserver', FakeObserver);
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue" />);
    await ready(calls, [makeItem('1'), makeItem('2')], { next_cursor: 'c1' });
    expect(screen.getByTestId('queue-load-sentinel')).toBeTruthy();
    // 開いただけ (スクロールしていない) では読まない
    intersect();
    expect(calls).toHaveLength(1);
    // スクロールして目印が見えたら 1 回だけ読む
    const scroller = scrollList(400);
    fireEvent.scroll(scroller);
    expect(calls).toHaveLength(2);
    expect(calls[1]?.cursor).toBe('c1');
    expect(screen.getByRole('button', { name: '読み込み中…' })).toBeTruthy();
    // 読み込み中に何度スクロール・交差しても、次は始めない
    intersect(); fireEvent.scroll(scroller); intersect();
    expect(calls).toHaveLength(2);
    // 次のページが届いたら、まだ下端にいれば次の cursor を 1 回だけ読む
    await act(async () => { calls[1]?.resolve({ ok: true, data: makeResponse(calls[1].filters, [makeItem('3')], { next_cursor: 'c2' }) }); await Promise.resolve(); });
    intersect();
    intersect();
    expect(calls).toHaveLength(3);
    expect(calls[2]?.cursor).toBe('c2');
    // 最後のページ (cursor なし) の後は目印も無く、読まない
    await act(async () => { calls[2]?.resolve({ ok: true, data: makeResponse(calls[2].filters, [makeItem('4')]) }); await Promise.resolve(); });
    expect(screen.queryByTestId('queue-load-sentinel')).toBeNull();
    intersect(); fireEvent.scroll(scroller);
    expect(calls).toHaveLength(3);
    expect(screen.getByText('これで最後です。')).toBeTruthy();
    // 追記した行にも矢印キーで移れる
    const list = screen.getByRole('list', { name: '架電キュー' });
    expect(within(list).getAllByText(/^架空会社/).map(e => e.textContent)).toEqual(['架空会社1', '架空会社2', '架空会社3', '架空会社4']);
    fireEvent.click(within(list).getByText('架空会社3'));
    fireEvent.keyDown(list, { key: 'ArrowDown' });
    expect(within(list).getByText('架空会社4').closest('button')?.getAttribute('aria-pressed')).toBe('true');
  });

  it('changing the conditions brings the list back to the top, so the new list does not auto-load until scrolled again', async () => {
    vi.stubGlobal('IntersectionObserver', FakeObserver);
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue" />);
    await ready(calls, [makeItem('1')], { next_cursor: 'c1' });
    const scroller = scrollList(400);
    fireEvent.change(screen.getByLabelText('並び替え'), { target: { value: 'last_call_asc' } });
    expect(calls).toHaveLength(2);
    expect(calls[1]?.cursor).toBeNull();
    await act(async () => { calls[1]?.resolve({ ok: true, data: makeResponse(calls[1].filters, [makeItem('9')], { next_cursor: 'd1' }) }); await Promise.resolve(); });
    expect(scroller.scrollTop).toBe(0);
    intersect();
    expect(calls).toHaveLength(2);
  });

  it('does not auto-load without a next cursor, and stops auto-loading after a failed page (the button retries)', async () => {
    vi.stubGlobal('IntersectionObserver', FakeObserver);
    const a = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={a.fetcher} initialSearch="?view=queue" />);
    await ready(a.calls, [makeItem('1')]);
    scrollList(400);
    intersect();
    expect(a.calls).toHaveLength(1);
    expect(FakeObserver.live()).toHaveLength(0);
    cleanup();

    const b = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={b.fetcher} initialSearch="?view=queue" />);
    await ready(b.calls, [makeItem('1')], { next_cursor: 'c1' });
    scrollList(400);
    intersect();
    expect(b.calls).toHaveLength(2);
    await act(async () => { b.calls[1]?.resolve({ ok: false, error: new ApiHttpError(400, { error_kind: 'cursor_mismatch' }) }); await Promise.resolve(); });
    expect(screen.getByText('続きを読み込めませんでした')).toBeTruthy();
    intersect(); fireEvent.scroll(scrollList(500));
    expect(b.calls).toHaveLength(2);
    fireEvent.click(screen.getByText('さらに読み込む'));
    expect(b.calls).toHaveLength(3);
  });
});

describe('CallQueueScreen: pipeline and stage dropdowns', () => {
  it('lists the allowed pipelines with HubSpot names (table names until loaded); switching resets the stages and refetches with pipeline=…', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} pipelinesFetcher={okPipelinesFetch} initialSearch="?stage=1095387445&due=today" />);
    const select = screen.getByRole('button',{name:'パイプライン'});
    expect(select.textContent).toContain('bpo_リクロジ');
    fireEvent.click(select);
    const menu=screen.getByRole('region',{name:'パイプラインの選択'});
    expect(within(menu).getAllByRole('checkbox')).toHaveLength(6);
    await waitFor(()=>{expect(within(menu).getByLabelText('アポ前')).toBeTruthy();});
    expect(calls[0]?.filters.stages).toHaveLength(1);
    fireEvent.click(within(menu).getByLabelText('アポ前'));
    fireEvent.click(within(menu).getByLabelText('bpo_リクロジ'));
    fireEvent.click(within(menu).getByRole('button',{name:'パイプラインを適用'}));
    const last = calls.at(-1);
    expect(calls[0]?.signal.aborted).toBe(true);
    expect(last?.filters.pipeline).toBe('default');
    expect(last?.filters.stages).toEqual([]);
    expect(last?.filters.due).toBe('today'); // ほかの条件は残す
    expect(last?.cursor).toBeNull();
    // ステージのボタンは新しいパイプラインの対象ステージ (未済 1 + 次回日 14) を全部選んだ状態
    openDetails();
    expect(screen.getByRole('button', { name: 'ステージ（15件選択）' })).toBeTruthy();
    // 応答が来たら一覧を出す (scope の pipeline が合う)
    await act(async () => { last?.resolve({ ok: true, data: makeResponse(last.filters, [makeItem('7')]) }); await Promise.resolve(); });
    expect(screen.getByText('架空会社7')).toBeTruthy();
  });

  it('the stage dropdown: all checked by default, select none blocks apply, select all and single toggles, excluded and unknown stages are greyed', async () => {
    const { calls, fetcher } = deferredFetcher();
    const pipelinesFetcher = () => Promise.resolve({ ok: true as const, data: pipelinesResponse([{ id: '1500000001', label: '新しいステージ' }]) });
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} pipelinesFetcher={pipelinesFetcher} initialSearch="" />);
    await waitFor(() => { expect(screen.getByRole('button', { name: 'パイプライン' }).textContent).toContain('bpo_リクロジ'); });
    openDetails();
    fireEvent.click(screen.getByRole('button', { name: 'ステージ（16件選択）' }));
    const panel = screen.getByRole('group', { name: 'ステージの選択' });
    const boxes = () => within(panel).getAllByRole<HTMLInputElement>('checkbox');
    expect(boxes()).toHaveLength(16);
    expect(boxes().every(b => b.checked)).toBe(true);
    // 架電対象外 (アポ日確定・架電禁止・商談実施処理 + 設定に無いステージ) は選べない
    const excluded = within(panel).getByLabelText('架電対象外のステージ');
    expect(within(excluded).queryAllByRole('checkbox')).toHaveLength(0);
    expect(within(excluded).getAllByRole('listitem').map(li => li.textContent)).toEqual([
      'アポ日確定', '架電禁止 ※リーダーのみ変更', '商談実施処理', '新しいステージ(架電キューの設定に無いステージ)',
    ]);
    // すべて外す → 適用できない
    fireEvent.click(within(panel).getByRole('button', { name: 'すべて外す' }));
    expect(boxes().some(b => b.checked)).toBe(false);
    expect(within(panel).getByRole<HTMLButtonElement>('button', { name: '適用' }).disabled).toBe(true);
    expect(within(panel).getByText('ステージを 1 つ以上選んでください')).toBeTruthy();
    // すべて選択 → 1 つ外して適用
    fireEvent.click(within(panel).getByRole('button', { name: 'すべて選択' }));
    expect(boxes().every(b => b.checked)).toBe(true);
    fireEvent.click(within(panel).getByLabelText(/^不通/));
    expect(calls).toHaveLength(1); // 適用するまで取り直さない
    fireEvent.click(within(panel).getByRole('button', { name: '適用' }));
    expect(screen.queryByRole('group', { name: 'ステージの選択' })).toBeNull();
    const stages = calls.at(-1)?.filters.stages ?? [];
    expect(stages).toHaveLength(15);
    expect(stages).not.toContain('1095387443');
    expect(screen.getByRole('button', { name: 'ステージ（15件選択）' })).toBeTruthy();
    // 全部に戻すと既定 (stage を送らない)
    fireEvent.click(screen.getByRole('button', { name: 'ステージ（15件選択）' }));
    fireEvent.click(screen.getByRole('button', { name: 'すべて選択' }));
    fireEvent.click(screen.getByRole('button', { name: '適用' }));
    expect(calls.at(-1)?.filters.stages).toEqual([]);
    // キャンセルは反映しない
    const before = calls.length;
    fireEvent.click(screen.getByRole('button', { name: 'ステージ（16件選択）' }));
    fireEvent.click(screen.getByRole('button', { name: 'すべて外す' }));
    fireEvent.click(screen.getByRole('button', { name: 'キャンセル' }));
    expect(calls).toHaveLength(before);
    expect(screen.getByRole('button', { name: 'ステージ（16件選択）' })).toBeTruthy();
  });

  it('the stage checklist closes on Esc and on outside click without applying', async () => {
    const { calls, fetcher } = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="" />);
    await waitFor(() => { expect(screen.getByRole('button', { name: 'パイプライン' }).textContent).toContain('bpo_リクロジ'); });
    openDetails();
    const before = calls.length;
    fireEvent.click(screen.getByRole('button', { name: 'ステージ（16件選択）' }));
    fireEvent.click(screen.getByRole('button', { name: 'すべて外す' }));
    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(screen.queryByRole('group', { name: 'ステージの選択' })).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'ステージ（16件選択）' }));
    fireEvent.click(screen.getByRole('button', { name: 'ステージ（16件選択）' }));
    fireEvent.click(screen.getByRole('button', { name: 'すべて外す' }));
    fireEvent.mouseDown(screen.getByLabelText('並び替え'));
    expect(screen.queryByRole('group', { name: 'ステージの選択' })).toBeNull();
    expect(calls).toHaveLength(before);
    expect(screen.getByRole('button', { name: 'ステージ（16件選択）' })).toBeTruthy();
  });

  it('fixture mode has a second fictional pipeline: always-shown and due stages appear, excluded and future ones do not', async () => {
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} initialSearch="?mode=fixture" />);
    await waitFor(() => { expect(screen.getByText('架空食品株式会社')).toBeTruthy(); });
    fireEvent.click(screen.getByRole('button',{name:'パイプライン'}));
    const menu=screen.getByRole('region',{name:'パイプラインの選択'});
    expect(within(menu).getAllByRole('checkbox')).toHaveLength(2);
    fireEvent.click(within(menu).getByLabelText('架空パイプライン(確認用)'));
    fireEvent.click(within(menu).getByLabelText('bpo_リクロジ'));
    fireEvent.click(within(menu).getByRole('button',{name:'パイプラインを適用'}));
    await waitFor(() => { expect(screen.getByText('架空倉庫サービス')).toBeTruthy(); });
    expect(screen.getByText('架空ベーカリー')).toBeTruthy();
    expect(screen.queryByText('架空塾')).toBeNull(); // 次回日が未来
    expect(screen.queryByText('架空クリーニング')).toBeNull(); // 対象外ステージ
    expect(screen.queryByText('架空食品株式会社')).toBeNull();
    expect(screen.getByTestId('queue-count').textContent).toContain('全 3 件中 3 件を表示');
    // 実データに切り替えると、架空のパイプラインは既定に戻す
    toLive();
    expect(screen.getByRole('button',{name:'パイプライン'}).textContent).toContain('bpo_リクロジ');
  });
});

it('条件上限の400では同じ条件の再試行を表示しない', async () => {
  const {calls,fetcher}=deferredFetcher();
  render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch="?view=queue" />);
  await act(async()=>{calls[0]?.resolve({ok:false,error:new ApiHttpError(400,{error_kind:'condition_limit'})});await Promise.resolve();});
  expect(screen.getByRole('alert').textContent).toContain('条件が多すぎます。条件かまとまりを減らしてください。');
  expect(screen.queryByRole('button',{name:'再試行'})).toBeNull();expect(calls).toHaveLength(1);
});

it('上の要約にまとまりの全条件と「または」を並べ、外す条件だけを取り除く', async () => {
  const groups=[[{property:'amount',operator:'GTE',values:['8000']},{property:'bpo_20',operator:'NOT_HAS_PROPERTY',values:[]}],[{property:'amount',operator:'LTE',values:['2000']}]];
  const {calls,fetcher}=deferredFetcher();
  render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={fetcher} initialSearch={'?condition_groups='+encodeURIComponent(JSON.stringify(groups))} />);
  await ready(calls);
  const summary=await screen.findByRole('list',{name:'適用中の条件'});
  await waitFor(()=>{expect(within(summary).getAllByRole('listitem').map(li=>li.textContent)).toEqual(['金額 以上 8,000×','最終架電日 空×','または','金額 以下 2,000×']);});
  expect(within(summary).getAllByRole('button')).toHaveLength(3);
  fireEvent.click(within(summary).getByRole('button',{name:'「金額 以上 8,000」を外す'}));
  expect(groupsOf(calls[1]?.filters ?? DEFAULT_FILTERS)).toEqual([[groups[0]?.[1]],[groups[1]?.[0]]]);
  fireEvent.click(within(summary).getByRole('button',{name:'「最終架電日 空」を外す'}));
  expect(groupsOf(calls[2]?.filters ?? DEFAULT_FILTERS)).toEqual([groups[1]]);
  expect(within(summary).queryByText('または')).toBeNull();
});
