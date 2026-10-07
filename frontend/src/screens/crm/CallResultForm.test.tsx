// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { CallQueueResponse } from '../../generated/CallQueueResponse';
import { CallQueueScreen } from './CallQueueScreen';
import { DRAFT_STORAGE_KEY } from './callResultModel';
import { makeItem, makeResponse, metadataFromMoc, metadataStub, okMetadataFetch } from './queueTestUtil';
import type { QueueFilters } from './queueModel';
import type { DetailFetch } from './useDealDetail';
import type { MetadataFetch } from './useResultDefinitions';
import { fixtureOwnersFetch } from './useOwners';

// JST 2026-10-08 12:00
const NOW = () => Date.UTC(2026, 9, 8, 3, 0, 0);
const neverDetail: DetailFetch = () => new Promise(() => undefined);

beforeEach(() => { try { window.sessionStorage.clear(); } catch { /* ignore */ } });
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

async function renderScreen(opts: { metadataFetcher?: MetadataFetch; items?: ReturnType<typeof makeItem>[]; search?: string } = {}) {
  const items = opts.items ?? [makeItem('1'), makeItem('2'), makeItem('3')];
  const queue = (f: QueueFilters) => Promise.resolve<ApiResult<CallQueueResponse>>({ ok: true, data: makeResponse(f, items) });
  const r = render(<CallQueueScreen fetcher={queue} ownersFetcher={fixtureOwnersFetch} detailFetcher={neverDetail}
    metadataFetcher={opts.metadataFetcher ?? okMetadataFetch} initialSearch={opts.search ?? '?view=queue'} now={NOW} />);
  await waitFor(() => { expect(screen.getByRole('list', { name: '架電キュー' })).toBeTruthy(); });
  return r;
}
const list = () => screen.getByRole('list', { name: '架電キュー' });
const open = (n: string) => { fireEvent.click(within(list()).getByText(`架空会社${n}`)); };
const form = () => screen.getByRole('form', { name: '架電結果の入力' });
const outcome = (label: string) => within(form()).getByRole('button', { name: label });
const recordBtn = () => within(form()).getByRole<HTMLButtonElement>('button', { name: /記録して次へ/ });
const selectedId = () => screen.getByTestId('result-slot').getAttribute('data-deal-id');
const rowOf = (n: string): HTMLElement => { const li = within(list()).getByText(`架空会社${n}`).closest('li'); if (!li) throw new Error('row'); return li; };
async function formReady() { await within(form()).findByRole('group', { name: '今回の結果' }); }

describe('call-result form (draft only)', () => {
  it('callback: 再架電 is pre-selected, inline messages keep the button disabled until date + time are set; never claims HubSpot saved', async () => {
    await renderScreen();
    open('1');
    await formReady();
    expect(within(form()).getByText('下書き(HubSpot 未送信)')).toBeTruthy();
    expect(within(form()).getByText('今回の結果を選んでください。')).toBeTruthy();
    expect(recordBtn().disabled).toBe(true);
    fireEvent.click(outcome('再架電の約束'));
    expect(outcome('再架電の約束').getAttribute('aria-pressed')).toBe('true');
    expect(within(form()).getByRole<HTMLInputElement>('radio', { name: '再架電' }).checked).toBe(true);
    expect(within(form()).getByText('次回架電日を入れてください(再架電のとき必須)。')).toBeTruthy();
    expect(within(form()).getByText('次回架電時間を選んでください(再架電のとき必須)。')).toBeTruthy();
    expect(recordBtn().disabled).toBe(true);
    fireEvent.change(within(form()).getByLabelText(/^次回架電日/), { target: { value: '2026-10-07' } });
    expect(within(form()).getByText('今日以降の日付を入れてください。')).toBeTruthy();
    fireEvent.change(within(form()).getByLabelText(/^次回架電日/), { target: { value: '2026-10-09' } });
    const time = within(form()).getByLabelText<HTMLSelectElement>(/^次回架電時間/);
    // 15 分刻み 8:00〜19:00 の 45 択 + 未選択
    expect(time.options).toHaveLength(46);
    fireEvent.change(time, { target: { value: '9:15' } });
    expect(recordBtn().disabled).toBe(false);
    expect(document.body.textContent).not.toMatch(/HubSpot に保存しました|保存しました/);
  });

  it('記録して次へ marks the row 記録済み(未送信), keeps the draft and moves to the next unrecorded row (skipping recorded ones)', async () => {
    await renderScreen();
    open('2');
    await formReady();
    fireEvent.click(outcome('不在・応答なし'));
    fireEvent.click(recordBtn());
    expect(within(rowOf('2')).getByText('記録済み(未送信)')).toBeTruthy();
    expect(selectedId()).toBe('3');
    // 3 を記録すると、末尾なので先頭の 1 へ (2 は記録済みなので飛ばす)
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.click(recordBtn());
    expect(selectedId()).toBe('1');
    fireEvent.click(outcome('番号違い'));
    fireEvent.change(within(form()).getByLabelText(/^不通時チェック/), { target: { value: '使われておりません' } });
    fireEvent.click(recordBtn());
    // 全部記録済み: 選択はそのまま、案内を出す
    expect(selectedId()).toBe('1');
    expect(within(form()).getByText('表示中の一覧に未記録の架電先はありません。')).toBeTruthy();
    expect(within(form()).getByText('記録済み(未送信)')).toBeTruthy();
    // 2 に戻ると下書きが残っている
    open('2');
    expect(outcome('不在・応答なし').getAttribute('aria-pressed')).toBe('true');
    expect(within(form()).getByText('記録済み(未送信)')).toBeTruthy();
  });

  it('Ctrl+Enter / Cmd+Enter records only when the draft is valid', async () => {
    await renderScreen();
    open('1');
    await formReady();
    fireEvent.keyDown(document, { key: 'Enter', ctrlKey: true });
    expect(selectedId()).toBe('1');
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.keyDown(document, { key: 'Enter', metaKey: true });
    expect(selectedId()).toBe('2');
    expect(within(rowOf('1')).getByText('記録済み(未送信)')).toBeTruthy();
    fireEvent.keyDown(document, { key: 'Enter' }); // 修飾キーなしは何もしない
    expect(selectedId()).toBe('2');
  });

  it('shows only the fields the outcome needs: appointment, stop request, wrong number with その他', async () => {
    await renderScreen();
    open('1');
    await formReady();
    fireEvent.click(outcome('アポイント獲得'));
    expect(within(form()).getByLabelText(/^商談予定日/)).toBeTruthy();
    expect(within(form()).getByLabelText<HTMLSelectElement>(/^商談予定時間/).options).toHaveLength(26);
    expect(within(form()).getByLabelText(/^商談方法/)).toBeTruthy();
    expect(within(form()).queryByLabelText(/^架電禁止理由/)).toBeNull();
    // 担当者を選ぶと温度感が出る
    expect(within(form()).queryByRole('radiogroup', { name: '担当者会話温度感' })).toBeNull();
    fireEvent.click(within(form()).getByRole('radio', { name: '担当者' }));
    expect(within(within(form()).getByRole('radiogroup', { name: '担当者会話温度感' })).getAllByRole('radio').map(r => r.parentElement?.textContent))
      .toEqual(['高（前向き）', '中（検討余地あり）', '低（否定的）', '聞く耳なし']);

    fireEvent.click(outcome('架電停止の希望'));
    expect(within(form()).getByLabelText(/^架電禁止理由/)).toBeTruthy();
    expect(within(form()).queryByLabelText(/^次回架電日/)).toBeNull();
    expect(within(form()).queryByLabelText(/^商談予定日/)).toBeNull();
    expect(within(form()).getByText('架電禁止理由を入れてください。')).toBeTruthy();

    fireEvent.click(outcome('番号違い'));
    const check = within(form()).getByLabelText<HTMLSelectElement>(/^不通時チェック/);
    // 表示ラベルと内部値が違う選択肢
    expect(Array.from(check.options).find(o => o.textContent === '現在使われておりません')?.value).toBe('使われておりません');
    fireEvent.change(check, { target: { value: 'その他' } });
    expect(within(form()).getByText('「その他」の理由を入れてください。')).toBeTruthy();
    fireEvent.change(within(form()).getByLabelText(/^その他理由/), { target: { value: '番号の桁不足' } });
    expect(recordBtn().disabled).toBe(false);
  });

  it('drafts are per deal, survive switching deals and collapsing; collapsed shows a one-line summary', async () => {
    await renderScreen();
    open('1');
    await formReady();
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.change(within(form()).getByLabelText(/^タスクメモ/), { target: { value: '求人票を送る' } });
    open('2');
    expect(within(form()).getByRole('group', { name: '今回の結果' }).querySelector('[aria-pressed="true"]')).toBeNull();
    expect(within(form()).getByLabelText<HTMLTextAreaElement>(/^タスクメモ/).value).toBe('');
    open('1');
    expect(within(form()).getByLabelText<HTMLTextAreaElement>(/^タスクメモ/).value).toBe('求人票を送る');
    fireEvent.click(within(form()).getByRole('button', { name: /架電結果/, expanded: true }));
    expect(screen.getByTestId('draft-summary').textContent).toBe('担当者と会話 · メモあり');
    expect(within(form()).queryByRole('group', { name: '今回の結果' })).toBeNull();
    // 折りたたみは案件を切り替えても保つ
    open('2');
    expect(screen.getByTestId('draft-summary').textContent).toBe('結果は未選択');
  });

  it('persists drafts to sessionStorage and restores them; 下書きを消す clears the draft and the recorded mark', async () => {
    const first = await renderScreen();
    open('1');
    await formReady();
    fireEvent.click(outcome('不在・応答なし'));
    fireEvent.click(recordBtn());
    const saved = JSON.parse(window.sessionStorage.getItem(DRAFT_STORAGE_KEY) ?? '{}') as { drafts: Record<string, { outcome: string }>; recorded: Record<string, boolean> };
    expect(saved.drafts['live:1']?.outcome).toBe('no_answer');
    expect(saved.recorded).toEqual({ 'live:1': true });
    first.unmount();
    await renderScreen();
    expect(within(rowOf('1')).getByText('記録済み(未送信)')).toBeTruthy();
    open('1');
    await formReady();
    expect(outcome('不在・応答なし').getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(within(form()).getByRole('button', { name: '下書きを消す' }));
    expect(outcome('不在・応答なし').getAttribute('aria-pressed')).toBe('false');
    expect(within(rowOf('1')).queryByText('記録済み(未送信)')).toBeNull();
    expect(JSON.parse(window.sessionStorage.getItem(DRAFT_STORAGE_KEY) ?? '{}')).toEqual({ drafts: {}, recorded: {} });
  });

  it('still works when sessionStorage is unavailable', async () => {
    vi.spyOn(window, 'sessionStorage', 'get').mockImplementation(() => { throw new Error('blocked'); });
    await renderScreen();
    open('1');
    await formReady();
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.click(recordBtn());
    expect(selectedId()).toBe('2');
    expect(within(rowOf('1')).getByText('記録済み(未送信)')).toBeTruthy();
  });

  it('live: a failed metadata request shows an error (no fixture options) and retry loads the definitions', async () => {
    const meta = metadataStub();
    await renderScreen({ metadataFetcher: meta.fetcher });
    expect(meta.calls).toHaveLength(0); // 案件を選ぶまで取得しない
    open('1');
    expect(within(form()).getByText('HubSpot から選択肢の定義を読み込み中…')).toBeTruthy();
    await act(async () => { meta.calls[0]?.({ ok: false, error: new ApiHttpError(502, { error_kind: 'hubspot_upstream' }) }); await Promise.resolve(); });
    const alert = within(form()).getByRole('alert');
    expect(alert.textContent).toContain('HubSpot から選択肢の定義を取得できませんでした。');
    expect(alert.textContent).toContain('架空の選択肢で代用はしません');
    expect(within(form()).queryByRole('group', { name: '今回の結果' })).toBeNull();
    expect(recordBtn().disabled).toBe(true);
    // 別の案件に移っても取り直さない (失敗のまま)。再試行で取り直す
    open('2');
    expect(meta.calls).toHaveLength(1);
    fireEvent.click(within(form()).getByRole('button', { name: '再試行' }));
    await act(async () => { meta.calls[1]?.({ ok: true, data: metadataFromMoc() }); await Promise.resolve(); });
    await formReady();
    expect(meta.calls).toHaveLength(2);
  });

  it('live: definitions missing from the response are named, not substituted', async () => {
    const fetcher: MetadataFetch = () => Promise.resolve({ ok: true, data: metadataFromMoc(['bpo_40', 'bpo_42', 'bpo_14', 'bpo_10', 'bpo_4', 'bpo__', 'bpo_33']) });
    await renderScreen({ metadataFetcher: fetcher });
    open('1');
    const alert = await within(form()).findByRole('alert');
    expect(alert.textContent).toContain('次アクション種別(bpo_45)');
    expect(within(form()).queryByRole('group', { name: '今回の結果' })).toBeNull();
  });

  it('fixture mode uses the bundled definitions without any request, and keeps its drafts apart from live ones', async () => {
    const spy = vi.fn<typeof fetch>(() => Promise.reject(new TypeError('offline')));
    vi.stubGlobal('fetch', spy);
    const meta = metadataStub();
    render(<CallQueueScreen metadataFetcher={meta.fetcher} detailFetcher={neverDetail} initialSearch="?view=queue&mode=fixture" now={NOW} />);
    await waitFor(() => { expect(screen.getByText('架空食品株式会社')).toBeTruthy(); });
    fireEvent.click(screen.getByText('架空食品株式会社'));
    await formReady();
    fireEvent.click(outcome('再架電の約束'));
    expect(within(form()).getByLabelText<HTMLSelectElement>(/^次回架電時間/).options).toHaveLength(46);
    expect(meta.calls).toHaveLength(0);
    expect(spy).not.toHaveBeenCalled();
    const saved = JSON.parse(window.sessionStorage.getItem(DRAFT_STORAGE_KEY) ?? '{}') as { drafts: Record<string, unknown> };
    expect(Object.keys(saved.drafts).every(k => k.startsWith('fixture:'))).toBe(true);
  });

  it('the Zoom iframe stays the same element across 記録して次へ', async () => {
    await renderScreen();
    const iframe = screen.getByTitle('Zoom Phone');
    open('1');
    await formReady();
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.click(recordBtn());
    expect(selectedId()).toBe('2');
    expect(screen.getByTitle('Zoom Phone')).toBe(iframe);
  });
});
