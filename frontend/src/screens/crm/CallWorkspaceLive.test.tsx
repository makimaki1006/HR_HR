// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { CallQueueResponse } from '../../generated/CallQueueResponse';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { CallQueueScreen } from './CallQueueScreen';
import { makeItem, makeResponse, okMetadataFetch } from './queueTestUtil';
import { ZOOM_EMBED_ORIGIN } from './smartEmbed';
import { fixtureOwnersFetch } from './useOwners';
import type { DetailFetch } from './useDealDetail';
import type { ZoomOptions } from './useZoomPhone';
import type { QueueFilters } from './queueModel';

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

function detail(id: string, over: Partial<WorkspaceResponse> = {}): WorkspaceResponse {
  return {
    deal: {
      id, name: `架空案件${id}`, stage_id: '1095387442', stage_label: '未済', pipeline_id: '753186575', owner_id: '9001',
      amount: '120000', close_date: null, next_call_date: '2026-10-05', next_call_time: '10:30', last_call_date: '2026-10-01',
      stop: { prohibited_reason: null, block_reason: null, unreachable_check: '通話中' }, bpo_phone: null,
      deep_link: `https://app.hubspot.com/contacts/1/record/0-3/${id}/`,
    },
    dial: { number: '+81312345678', source: 'contact' },
    contacts: [{ id: `c${id}`, name: `架空 太郎${id}`, job_title: '採用担当', phone: '+81312345678', mobile: '090-1111-2222', email: 'a@example.invalid', labels: ['主'], is_primary: true, deep_link: `https://app.hubspot.com/contacts/1/record/0-1/c${id}/` }],
    contacts_total: 1,
    companies: [{ id: `co${id}`, name: `架空会社${id}`, phone: '03-9999-0000', address: '100-0001 東京都 千代田区 架空1-1', industry: '介護', domain: 'example.invalid', labels: ['主'], is_primary: true, deep_link: `https://app.hubspot.com/contacts/1/record/0-2/co${id}/` }],
    companies_total: 1,
    activities: [
      { id: 'a1', kind: 'call', timestamp: '2026-10-03T01:00:00Z', title: '架電1', body: null, direction: 'OUTBOUND', status: 'COMPLETED', duration_ms: 65000, owner_id: null, source: null, via: 'deal', via_id: id },
      { id: 'a2', kind: 'note', timestamp: '2026-10-02T01:00:00Z', title: null, body: '受付で不在', direction: null, status: null, duration_ms: null, owner_id: null, source: null, via: 'deal', via_id: id },
      { id: 'a3', kind: 'call', timestamp: '2026-10-01T01:00:00Z', title: '別案件の通話', body: null, direction: 'OUTBOUND', status: 'NO_ANSWER', duration_ms: null, owner_id: null, source: null, via: 'contact', via_id: `c${id}` },
    ],
    activities_truncated: false, activity_scope: '案件に直接つながる通話・メモ・メール・ミーティングと、案件の担当者に直接つながる通話。',
    partial: [], hubspot_portal_id: '1', data_scope: 'x', generated_at: '2026-10-05T03:00:00Z',
    ...over,
  };
}

interface Pending { id: string; signal: AbortSignal; resolve: (r: ApiResult<WorkspaceResponse>) => void }
function detailFetcher() {
  const calls: Pending[] = [];
  const fetcher: DetailFetch = (id, signal) => new Promise(resolve => { calls.push({ id, signal, resolve }); });
  return { calls, fetcher };
}

async function renderQueue(df: DetailFetch, items = [makeItem('1'), makeItem('2')], zoomOptions?: ZoomOptions) {
  const queue = (filters: QueueFilters) => Promise.resolve<ApiResult<CallQueueResponse>>({ ok: true, data: makeResponse(filters, items) });
  render(<CallQueueScreen fetcher={(f) => queue(f)} ownersFetcher={fixtureOwnersFetch} detailFetcher={df} metadataFetcher={okMetadataFetch} zoomOptions={zoomOptions} initialSearch="?view=queue" />);
  await waitFor(() => { expect(screen.getByRole('list', { name: '架電キュー' })).toBeTruthy(); });
}

function first<T>(list: T[]): T {
  const v = list[0];
  if (v === undefined) throw new Error('empty');
  return v;
}
const open = (n: string) => { fireEvent.click(within(screen.getByRole('list', { name: '架電キュー' })).getByText(`架空会社${n}`)); };
const ok = (data: WorkspaceResponse): ApiResult<WorkspaceResponse> => ({ ok: true, data });
const fail = (status: number, kind: string): ApiResult<WorkspaceResponse> => ({ ok: false, error: new ApiHttpError(status, { error_kind: kind }) });

/** iframe の contentWindow を偽物に差し替える (happy-dom は外部ページを読まない) */
function fakeZoomWindow() {
  const iframe = screen.getByTitle<HTMLIFrameElement>('Zoom Phone');
  const postMessage = vi.fn();
  const win = { postMessage } as unknown as Window;
  Object.defineProperty(iframe, 'contentWindow', { configurable: true, get: () => win });
  return { win, postMessage };
}
function zoomEvent(win: Window, data: unknown, origin = ZOOM_EMBED_ORIGIN) {
  act(() => { window.dispatchEvent(new MessageEvent('message', { origin, source: win as unknown as MessageEventSource, data })); });
}

describe('架電ワークスペース (実データ)', () => {
  it('shows a placeholder until a row is chosen, then loads and shows deal / contact / company / activities with HubSpot links', async () => {
    const { calls, fetcher } = detailFetcher();
    await renderQueue(fetcher);
    expect(screen.getByText('左の一覧から架電先を選んでください')).toBeTruthy();
    expect(calls).toHaveLength(0);
    open('1');
    expect(screen.getByText('詳細を読み込み中…')).toBeTruthy();
    expect(calls.map(c => c.id)).toEqual(['1']);
    await act(async () => { calls[0]?.resolve(ok(detail('1'))); await Promise.resolve(); });
    const d = screen.getByRole('article', { name: '架電先の詳細' });
    expect(within(d).getByText('架空会社1', { selector: 'h2' })).toBeTruthy();
    expect(within(d).getByText('不通時チェック: 通話中')).toBeTruthy();
    expect(within(d).getByText('採用担当')).toBeTruthy();
    expect(within(d).getByText('100-0001 東京都 千代田区 架空1-1')).toBeTruthy();
    expect(within(d).getByText('120,000 円')).toBeTruthy();
    expect(within(d).getAllByText('HubSpotで開く').map(a => a.getAttribute('href'))).toEqual([
      'https://app.hubspot.com/contacts/1/record/0-3/1/', 'https://app.hubspot.com/contacts/1/record/0-1/c1/', 'https://app.hubspot.com/contacts/1/record/0-2/co1/',
    ]);
    // 活動: 通話の時間は ms → 分秒、担当者経由は注記つき、種類で絞れる
    expect(within(d).getByText(/通話時間 1分05秒/)).toBeTruthy();
    expect(within(d).getByText(/担当者の通話/)).toBeTruthy();
    fireEvent.click(within(d).getByRole('button', { name: 'メモ' }));
    expect(within(d).queryByText('架電1')).toBeNull();
    expect(within(d).getByText('受付で不在')).toBeTruthy();
    // 書き込みはしない: 詳細には保存ボタンが無く、下の入力欄は「HubSpot 未送信」の下書きと明示する
    expect(within(d).queryByRole('button', { name: /保存|記録/ })).toBeNull();
    const form = await screen.findByRole('form', { name: '架電結果の入力' });
    expect(within(form).getByText('下書き(HubSpot 未送信)')).toBeTruthy();
    expect(within(form).getByText(/HubSpot には保存されません/)).toBeTruthy();
  });

  it('drops a stale detail response when another deal was chosen (and aborts the old request)', async () => {
    const { calls, fetcher } = detailFetcher();
    await renderQueue(fetcher);
    open('1');
    open('2');
    expect(calls.map(c => c.id)).toEqual(['1', '2']);
    expect(calls[0]?.signal.aborted).toBe(true);
    // 古い (案件 1) の応答が後から届いても表示しない
    await act(async () => { calls[0]?.resolve(ok(detail('1'))); await Promise.resolve(); });
    expect(screen.queryByText('架空会社1', { selector: 'h2' })).toBeNull();
    expect(screen.getByText('詳細を読み込み中…')).toBeTruthy();
    await act(async () => { calls[1]?.resolve(ok(detail('2'))); await Promise.resolve(); });
    expect(screen.getByText('架空会社2', { selector: 'h2' })).toBeTruthy();
  });

  it('refuses a response whose deal id is not the chosen deal', async () => {
    const { calls, fetcher } = detailFetcher();
    await renderQueue(fetcher);
    open('1');
    await act(async () => { calls[0]?.resolve(ok(detail('999'))); await Promise.resolve(); });
    expect(screen.getByRole('alert').textContent).toContain('一致しなかった');
    expect(screen.queryByText('架空会社999', { selector: 'h2' })).toBeNull();
  });

  it('403 forbidden_record shows the gate message and no deal data; 429 shows a retry that reloads', async () => {
    const { calls, fetcher } = detailFetcher();
    await renderQueue(fetcher);
    open('1');
    await act(async () => { calls[0]?.resolve(fail(403, 'forbidden_record')); await Promise.resolve(); });
    expect(screen.getByRole('alert').textContent).toContain('表示する権限がありません');
    expect(screen.queryByRole('article')).toBeNull();
    open('2');
    await act(async () => { calls[1]?.resolve(fail(503, 'hubspot_rate_limited')); await Promise.resolve(); });
    expect(screen.getByRole('alert').textContent).toContain('上限');
    fireEvent.click(screen.getByRole('button', { name: '再試行' }));
    expect(calls.map(c => c.id)).toEqual(['1', '2', '2']);
    await act(async () => { calls[2]?.resolve(ok(detail('2'))); await Promise.resolve(); });
    expect(screen.getByRole('article', { name: '架電先の詳細' })).toBeTruthy();
  });

  it('shows partial notes and the empty states (no associations, no activities) without breaking', async () => {
    const { calls, fetcher } = detailFetcher();
    await renderQueue(fetcher);
    open('1');
    await act(async () => {
      calls[0]?.resolve(ok(detail('1', { contacts: [], contacts_total: 0, companies: [], companies_total: 0, dial: null, activities: [],
        partial: [{ part: 'emails', error_kind: 'hubspot_auth' }, { part: 'calls_via_contacts', error_kind: 'hubspot_upstream' }] })));
      await Promise.resolve();
    });
    const d = screen.getByRole('article', { name: '架電先の詳細' });
    expect(within(d).getByText(/メールを取得できませんでした/)).toBeTruthy();
    expect(within(d).getByText(/担当者経由の通話を取得できませんでした/)).toBeTruthy();
    expect(within(d).getByText('番号を確認できません。担当者・会社の情報を HubSpot で確認してください。')).toBeTruthy();
    expect(within(d).getByText(/担当者の情報を取得できませんでした/)).toBeTruthy();
    expect(within(d).getByText('表示できる活動履歴はありません。')).toBeTruthy();
  });
});

describe('Zoom Phone (Smart Embed)', () => {
  it('the dial button posts zp-make-call (+81 form) to the Zoom origin only, never "*"', async () => {
    const { calls, fetcher } = detailFetcher();
    await renderQueue(fetcher);
    open('1');
    await act(async () => { calls[0]?.resolve(ok(detail('1'))); await Promise.resolve(); });
    const { postMessage } = fakeZoomWindow();
    fireEvent.click(first(screen.getAllByRole('button', { name: /に発信$/ })));
    expect(postMessage).toHaveBeenCalledTimes(1);
    expect(postMessage).toHaveBeenCalledWith({ type: 'zp-make-call', data: { number: '+81312345678', autoDial: true } }, 'https://applications.zoom.us');
    expect(screen.getByText(/03-1234-5678 への発信を依頼しました/)).toBeTruthy();
    // tel: とコピーも出る
    expect(screen.getAllByRole('link', { name: /tel: で発信$/ })[0]?.getAttribute('href')).toBe('tel:+81312345678');
  });

  it('reflects ringing / connected / ended from Zoom, ignores events from other origins or windows, and blocks a second dial during a call', async () => {
    const { calls, fetcher } = detailFetcher();
    await renderQueue(fetcher);
    open('1');
    await act(async () => { calls[0]?.resolve(ok(detail('1'))); await Promise.resolve(); });
    const { win, postMessage } = fakeZoomWindow();
    const base = { callId: 'call-1', direction: 'outbound', callee: { phoneNumber: '+81312345678' } };
    // 他のオリジン・他の window は無視
    zoomEvent(win, { type: 'zp-call-ringing-event', data: base }, 'https://evil.example');
    zoomEvent({} as Window, { type: 'zp-call-ringing-event', data: base });
    expect(screen.getByText('通話していません')).toBeTruthy();
    zoomEvent(win, { type: 'zp-call-ringing-event', data: base });
    expect(screen.getByText('呼び出し中')).toBeTruthy();
    fireEvent.click(first(screen.getAllByRole('button', { name: /に発信$/ })));
    expect(postMessage).not.toHaveBeenCalled();
    expect(screen.getByText('通話中のため、新しい発信はできません。')).toBeTruthy();
    zoomEvent(win, { type: 'zp-call-connected-event', data: base });
    expect(screen.getByText('通話中')).toBeTruthy();
    expect(screen.getByText('通話ID call-1')).toBeTruthy();
    zoomEvent(win, { type: 'zp-call-ended-event', data: { ...base, result: 'ended' } });
    expect(screen.getByText('通話が終了しました')).toBeTruthy();
    expect(screen.getByText(/HubSpot にはまだ保存されません/)).toBeTruthy();
    // この通話は画面から発信したものではないので、入力欄には通話終了を出さない
    expect(screen.queryByTestId('ended-call')).toBeNull();
  });

  it('a call dialed from a deal that ends shows its duration in that deal\'s result form and focuses the outcome buttons', async () => {
    const { calls, fetcher } = detailFetcher();
    let t = 1_000_000;
    await renderQueue(fetcher, [makeItem('1'), makeItem('2')], { now: () => t });
    open('1');
    await act(async () => { calls[0]?.resolve(ok(detail('1'))); await Promise.resolve(); });
    const form = await screen.findByRole('form', { name: '架電結果の入力' });
    await within(form).findByRole('group', { name: '今回の結果' });
    // 折りたたんでおいても、通話が終わったら開く
    fireEvent.click(within(form).getByRole('button', { name: /架電結果/, expanded: true }));
    const { win } = fakeZoomWindow();
    fireEvent.click(first(screen.getAllByRole('button', { name: /に発信$/ })));
    const base = { callId: 'call-9', direction: 'outbound', callee: { phoneNumber: '+81312345678' } };
    zoomEvent(win, { type: 'zp-call-ringing-event', data: base });
    zoomEvent(win, { type: 'zp-call-connected-event', data: base });
    expect(screen.queryByTestId('ended-call')).toBeNull();
    t += 65_000;
    zoomEvent(win, { type: 'zp-call-ended-event', data: { ...base, result: 'ended' } });
    expect(screen.getByTestId('ended-call').textContent).toBe('通話終了 通話時間 01:05');
    const first6 = within(screen.getByRole('group', { name: '今回の結果' })).getAllByRole('button');
    expect(first6.map(b => b.textContent)).toEqual(['担当者と会話', '不在・応答なし', '再架電の約束', 'アポイント獲得', '番号違い', '架電停止の希望']);
    expect(document.activeElement).toBe(first6[0]);
    // 別の案件の入力欄には出さない
    open('2');
    expect(screen.queryByTestId('ended-call')).toBeNull();
    const row1 = within(screen.getByRole('list', { name: '架電キュー' })).getByText('架空会社1').closest('button');
    row1?.focus();
    open('1');
    expect(screen.getByTestId('ended-call').textContent).toBe('通話終了 通話時間 01:05');
    // 戻ってきただけではフォーカスを奪わない (1 つの通話につき 1 回)
    expect(document.activeElement).toBe(row1);
  });

  it('binds the ended call to the deal by callId: a previous call that ended, or an inbound call, is never shown on the deal dialed next', async () => {
    const { calls, fetcher } = detailFetcher();
    let t = 1_000_000;
    await renderQueue(fetcher, [makeItem('1'), makeItem('2')], { now: () => t, stallMs: 60_000 });
    open('1');
    await act(async () => { calls[0]?.resolve(ok(detail('1'))); await Promise.resolve(); });
    const { win, postMessage } = fakeZoomWindow();
    fireEvent.click(first(screen.getAllByRole('button', { name: /に発信$/ })));
    const c1 = { callId: 'c1', direction: 'outbound', callee: { phoneNumber: '+81312345678' } };
    zoomEvent(win, { type: 'zp-call-ringing-event', data: c1 });
    zoomEvent(win, { type: 'zp-call-connected-event', data: c1 });
    // 通話中に案件 2 を選び、そこで通話が終わる (案件 1 の入力欄はフォーカスを扱っていない)
    open('2');
    await act(async () => { calls[1]?.resolve(ok(detail('2'))); await Promise.resolve(); });
    t += 65_000;
    zoomEvent(win, { type: 'zp-call-ended-event', data: { ...c1, result: 'ended' } });
    expect(screen.queryByTestId('ended-call')).toBeNull();
    // 案件 2 から発信。呼び出しが始まる前は、c1 の「終了 01:05」を 2 に出さない・フォーカスも移さない
    const dial2 = first(screen.getAllByRole('button', { name: /に発信$/ }));
    dial2.focus();
    fireEvent.click(dial2);
    expect(postMessage).toHaveBeenCalledTimes(2);
    expect(screen.queryByTestId('ended-call')).toBeNull();
    expect(document.activeElement).toBe(dial2);
    // 着信 (inbound) が来て終わっても、案件 2 のものにしない
    const inbound = { callId: 'in-1', direction: 'inbound', caller: { phoneNumber: '+81355550000' } };
    zoomEvent(win, { type: 'zp-call-ringing-event', data: inbound });
    zoomEvent(win, { type: 'zp-call-ended-event', data: { ...inbound, result: 'missed' } });
    expect(screen.queryByTestId('ended-call')).toBeNull();
    // 案件 2 の発信の通話 (c2) が始まって終わったら、そのときだけ 2 に出す
    const c2 = { callId: 'c2', direction: 'outbound', callee: { phoneNumber: '+81312345678' } };
    zoomEvent(win, { type: 'zp-call-ringing-event', data: c2 });
    zoomEvent(win, { type: 'zp-call-connected-event', data: c2 });
    t += 7_000;
    zoomEvent(win, { type: 'zp-call-ended-event', data: { ...c2, result: 'ended' } });
    expect(screen.getByTestId('ended-call').textContent).toBe('通話終了 通話時間 00:07');
    // 案件 1 には c1 も c2 も出さない
    open('1');
    expect(screen.queryByTestId('ended-call')).toBeNull();
  });

  it('does not steal focus from the memo when the call ends while the caller is typing', async () => {
    const { calls, fetcher } = detailFetcher();
    await renderQueue(fetcher, [makeItem('1'), makeItem('2')], { now: () => 1_000_000 });
    open('1');
    await act(async () => { calls[0]?.resolve(ok(detail('1'))); await Promise.resolve(); });
    const form = await screen.findByRole('form', { name: '架電結果の入力' });
    await within(form).findByRole('group', { name: '今回の結果' });
    const { win } = fakeZoomWindow();
    fireEvent.click(first(screen.getAllByRole('button', { name: /に発信$/ })));
    const base = { callId: 'call-m', direction: 'outbound', callee: { phoneNumber: '+81312345678' } };
    zoomEvent(win, { type: 'zp-call-ringing-event', data: base });
    zoomEvent(win, { type: 'zp-call-connected-event', data: base });
    const memo = within(form).getByLabelText<HTMLTextAreaElement>(/^タスクメモ/);
    memo.focus();
    zoomEvent(win, { type: 'zp-call-ended-event', data: { ...base, result: 'ended' } });
    expect(screen.getByTestId('ended-call').textContent).toBe('通話終了 通話時間 00:00');
    expect(document.activeElement).toBe(memo);
    // 結果はどれも選ばれていない (Space / Enter で誤って選ぶ経路を作らない)
    expect(within(form).getByRole('group', { name: '今回の結果' }).querySelector('[aria-pressed="true"]')).toBeNull();
  });

  it('shows guidance and keeps copy / tel: available when the embed cannot be loaded or the dial does not start', async () => {
    const { calls, fetcher } = detailFetcher();
    await renderQueue(fetcher, [makeItem('1'), makeItem('2')], { loadTimeoutMs: 40, stallMs: 40 });
    open('1');
    await act(async () => { calls[0]?.resolve(ok(detail('1'))); await Promise.resolve(); });
    fakeZoomWindow();
    // 読み込みが終わらない (iframe の load が起きない)
    await waitFor(() => { expect(screen.getByText('Zoom Phone を読み込めません')).toBeTruthy(); });
    expect(screen.getByText(/許可ドメインへの登録/)).toBeTruthy();
    // 発信を依頼したが呼び出しが始まらない
    fireEvent.click(first(screen.getAllByRole('button', { name: /に発信$/ })));
    await waitFor(() => { expect(screen.getByText('発信が始まりません')).toBeTruthy(); });
    expect(screen.getAllByRole('button', { name: /番号をコピー$/ }).length).toBeGreaterThan(0);
    expect(screen.getAllByRole('link', { name: /tel: で発信$/ }).length).toBeGreaterThan(0);
  });

  it('copies the number, and says so when the clipboard is unavailable', async () => {
    const { calls, fetcher } = detailFetcher();
    await renderQueue(fetcher);
    open('1');
    await act(async () => { calls[0]?.resolve(ok(detail('1'))); await Promise.resolve(); });
    const writeText = vi.fn(() => Promise.resolve());
    vi.stubGlobal('navigator', { clipboard: { writeText } });
    fireEvent.click(first(screen.getAllByRole('button', { name: /番号をコピー$/ })));
    expect(writeText).toHaveBeenCalledWith('0312345678');
    await waitFor(() => { expect(screen.getByText('番号をコピーしました。')).toBeTruthy(); });
    vi.stubGlobal('navigator', {});
    fireEvent.click(first(screen.getAllByRole('button', { name: /番号をコピー$/ })));
    expect(screen.getByText('コピーできませんでした。番号を選んでコピーしてください。')).toBeTruthy();
    vi.unstubAllGlobals();
  });
});

describe('架空サンプル', () => {
  it('has no Zoom iframe and cannot dial (fictional numbers must never be dialed); detail is fictional and makes no request', async () => {
    const spy = vi.fn<typeof fetch>(() => Promise.reject(new TypeError('offline')));
    vi.stubGlobal('fetch', spy);
    try {
      render(<CallQueueScreen initialSearch="?view=queue&mode=fixture" />);
      await waitFor(() => { expect(screen.getByText('架空食品株式会社')).toBeTruthy(); });
      expect(screen.queryByTitle('Zoom Phone')).toBeNull();
      expect(screen.getByText('架空サンプルでは発信できません')).toBeTruthy();
      fireEvent.click(screen.getByText('架空食品株式会社'));
      await waitFor(() => { expect(screen.getByRole('article', { name: '架電先の詳細' })).toBeTruthy(); });
      for (const b of screen.getAllByRole('button', { name: /に発信$/ })) expect((b as HTMLButtonElement).disabled).toBe(true);
      expect(spy).not.toHaveBeenCalled();
    } finally { vi.unstubAllGlobals(); }
  });
});
