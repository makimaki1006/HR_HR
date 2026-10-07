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

async function renderScreen(opts: { metadataFetcher?: MetadataFetch; items?: ReturnType<typeof makeItem>[]; search?: string; now?: () => number } = {}) {
  const items = opts.items ?? [makeItem('1'), makeItem('2'), makeItem('3')];
  // キーワードは会社名で絞る (一覧から選んだ案件が外れる場合を作れるように)
  const queue = (f: QueueFilters) => Promise.resolve<ApiResult<CallQueueResponse>>({
    ok: true, data: makeResponse(f, items.filter(i => !f.q || (i.company?.name ?? '').includes(f.q))),
  });
  const r = render(<CallQueueScreen fetcher={queue} ownersFetcher={fixtureOwnersFetch} detailFetcher={neverDetail}
    metadataFetcher={opts.metadataFetcher ?? okMetadataFetch} initialSearch={opts.search ?? '?view=queue'} now={opts.now ?? NOW} />);
  await waitFor(() => { expect(screen.getByRole('list', { name: '架電キュー' })).toBeTruthy(); });
  return r;
}
const list = () => screen.getByRole('list', { name: '架電キュー' });
const open = (n: string) => { fireEvent.click(within(list()).getByText(`架空会社${n}`)); };
const form = () => screen.getByRole('form', { name: '架電結果の入力' });
const outcome = (label: string) => within(form()).getByRole('button', { name: label });
const recordBtn = () => within(form()).getByRole<HTMLButtonElement>('button', { name: /記録して次へ/ });
/** 記録ボタンは aria-disabled (押すと足りない欄を示す)。記録できないとき true */
const recordDisabled = () => recordBtn().getAttribute('aria-disabled') === 'true';
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
    expect(recordDisabled()).toBe(true);
    fireEvent.click(outcome('再架電の約束'));
    expect(outcome('再架電の約束').getAttribute('aria-pressed')).toBe('true');
    expect(within(form()).getByRole<HTMLInputElement>('radio', { name: '再架電' }).checked).toBe(true);
    // まだ触っていない欄の文言は出さない (選んだ直後に赤い文言・読み上げを並べない)
    expect(within(form()).queryByText('次回架電日を入れてください(再架電のとき必須)。')).toBeNull();
    expect(form().querySelectorAll('[role="alert"]')).toHaveLength(0);
    expect(recordDisabled()).toBe(true);
    // 記録を試みると、足りない欄の文言が出て、最初の欄にフォーカスが移る。記録はしない
    fireEvent.click(recordBtn());
    expect(selectedId()).toBe('1');
    expect(within(form()).getByText('次回架電日を入れてください(再架電のとき必須)。')).toBeTruthy();
    expect(within(form()).getByText('次回架電時間を選んでください(再架電のとき必須)。')).toBeTruthy();
    expect(document.activeElement).toBe(within(form()).getByLabelText(/^次回架電日/));
    expect(within(form()).getByLabelText(/^次回架電日/).getAttribute('aria-invalid')).toBe('true');
    expect(form().querySelectorAll('[role="alert"]')).toHaveLength(0);
    fireEvent.change(within(form()).getByLabelText(/^次回架電日/), { target: { value: '2026-10-07' } });
    expect(within(form()).getByText('今日以降の日付を入れてください。')).toBeTruthy();
    fireEvent.change(within(form()).getByLabelText(/^次回架電日/), { target: { value: '2026-10-09' } });
    const time = within(form()).getByLabelText<HTMLSelectElement>(/^次回架電時間/);
    // 15 分刻み 8:00〜19:00 の 45 択 + 未選択
    expect(time.options).toHaveLength(46);
    fireEvent.change(time, { target: { value: '9:15' } });
    expect(recordDisabled()).toBe(false);
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

  it('Ctrl+Enter / Cmd+Enter inside the form records only when the draft is valid', async () => {
    await renderScreen();
    open('1');
    await formReady();
    fireEvent.keyDown(outcome('担当者と会話'), { key: 'Enter', ctrlKey: true });
    expect(selectedId()).toBe('1');
    fireEvent.click(outcome('担当者と会話'));
    const memo = within(form()).getByLabelText(/^タスクメモ/);
    fireEvent.keyDown(memo, { key: 'Enter' }); // 修飾キーなしは何もしない
    expect(selectedId()).toBe('1');
    fireEvent.keyDown(memo, { key: 'Enter', metaKey: true });
    expect(selectedId()).toBe('2');
    expect(within(rowOf('1')).getByText('記録済み(未送信)')).toBeTruthy();
  });

  it('Ctrl+Enter outside the form (search box, document) or while folded does not record and is not swallowed', async () => {
    await renderScreen();
    open('1');
    await formReady();
    fireEvent.click(outcome('担当者と会話'));
    const search = screen.getByRole('searchbox', { name: 'キーワード(会社名・案件名)' });
    fireEvent.change(search, { target: { value: 'abc' } });
    const notPrevented = fireEvent.keyDown(search, { key: 'Enter', ctrlKey: true });
    expect(notPrevented).toBe(true);
    fireEvent.keyDown(document, { key: 'Enter', ctrlKey: true });
    expect(selectedId()).toBe('1');
    expect(within(rowOf('1')).queryByText('記録済み(未送信)')).toBeNull();
    // 折りたたみ中は、入力欄の見出しで押しても記録しない
    const toggle = within(form()).getByRole('button', { name: /架電結果/, expanded: true });
    fireEvent.click(toggle);
    fireEvent.keyDown(toggle, { key: 'Enter', ctrlKey: true });
    expect(selectedId()).toBe('1');
    expect(within(rowOf('1')).queryByText('記録済み(未送信)')).toBeNull();
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
    expect(within(form()).queryByText('架電禁止理由を入れてください。')).toBeNull();
    fireEvent.keyDown(outcome('架電停止の希望'), { key: 'Enter', ctrlKey: true }); // 記録を試みる
    expect(within(form()).getByText('架電禁止理由を入れてください。')).toBeTruthy();
    expect(document.activeElement).toBe(within(form()).getByLabelText(/^架電禁止理由/));

    fireEvent.click(outcome('番号違い'));
    const check = within(form()).getByLabelText<HTMLSelectElement>(/^不通時チェック/);
    // 表示ラベルと内部値が違う選択肢
    expect(Array.from(check.options).find(o => o.textContent === '現在使われておりません')?.value).toBe('使われておりません');
    fireEvent.change(check, { target: { value: 'その他' } });
    expect(within(form()).getByText('「その他」の理由を入れてください。')).toBeTruthy();
    fireEvent.change(within(form()).getByLabelText(/^その他理由/), { target: { value: '番号の桁不足' } });
    expect(recordDisabled()).toBe(false);
  });

  it('記録して次へ moves focus to the newly selected row (not <body>) so arrow keys keep working', async () => {
    await renderScreen();
    open('1');
    await formReady();
    fireEvent.click(outcome('担当者と会話'));
    recordBtn().focus();
    fireEvent.click(recordBtn());
    expect(selectedId()).toBe('2');
    const row2 = within(rowOf('2')).getByRole('button');
    expect(document.activeElement).toBe(row2);
    expect(row2.getAttribute('aria-pressed')).toBe('true');
  });

  it('editing a recorded draft removes the 記録済み(未送信) mark, so 記録して次へ no longer skips it', async () => {
    await renderScreen();
    open('1');
    await formReady();
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.click(recordBtn());
    expect(within(rowOf('1')).getByText('記録済み(未送信)')).toBeTruthy();
    open('1');
    // アポに切り替えて必須欄が空のまま: 記録済みの印は外れ、記録もできない
    fireEvent.click(outcome('アポイント獲得'));
    expect(within(rowOf('1')).queryByText('記録済み(未送信)')).toBeNull();
    expect(within(form()).getByText('下書き(HubSpot 未送信)')).toBeTruthy();
    expect(recordDisabled()).toBe(true);
    expect((JSON.parse(window.sessionStorage.getItem(DRAFT_STORAGE_KEY) ?? '{}') as { recorded: Record<string, boolean> }).recorded).toEqual({});
    // 3 で記録して次へ → 1 は未記録に戻ったので、末尾の 3 から先頭の 1 へ (印が残っていれば 2 へ飛ぶ)
    open('3');
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.click(recordBtn());
    expect(selectedId()).toBe('1');
  });

  it('a session left open past JST midnight does not accept yesterday: the check and the date-picker min use the date at record time', async () => {
    let t = Date.UTC(2026, 9, 8, 14, 50, 0); // JST 2026-10-08 23:50
    await renderScreen({ now: () => t });
    open('1');
    await formReady();
    fireEvent.click(outcome('再架電の約束'));
    const date = () => within(form()).getByLabelText<HTMLInputElement>(/^次回架電日/);
    expect(date().min).toBe('2026-10-08');
    fireEvent.change(date(), { target: { value: '2026-10-08' } });
    fireEvent.change(within(form()).getByLabelText(/^次回架電時間/), { target: { value: '9:15' } });
    expect(recordDisabled()).toBe(false);
    t = Date.UTC(2026, 9, 8, 15, 30, 0); // JST 2026-10-09 00:30
    fireEvent.click(recordBtn());
    expect(selectedId()).toBe('1');
    expect(within(rowOf('1')).queryByText('記録済み(未送信)')).toBeNull();
    expect(date().min).toBe('2026-10-09');
    expect(within(form()).getByText('今日以降の日付を入れてください。')).toBeTruthy();
    expect(recordDisabled()).toBe(true);
  });

  it('when a filter drops the selected deal from the list, the form says so and cannot record', async () => {
    await renderScreen();
    open('2');
    await formReady();
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.change(screen.getByRole('searchbox', { name: 'キーワード(会社名・案件名)' }), { target: { value: '架空会社3' } });
    await waitFor(() => { expect(within(list()).queryByText('架空会社2')).toBeNull(); });
    expect(within(list()).getByText('架空会社3')).toBeTruthy();
    expect(selectedId()).toBe('2');
    expect(within(form()).getByText(/この案件はいまの一覧にありません/)).toBeTruthy();
    expect(recordDisabled()).toBe(true);
    fireEvent.click(recordBtn());
    fireEvent.keyDown(outcome('担当者と会話'), { key: 'Enter', ctrlKey: true });
    expect(selectedId()).toBe('2');
    expect(within(rowOf('3')).queryByText('記録済み(未送信)')).toBeNull();
    expect(JSON.parse(window.sessionStorage.getItem(DRAFT_STORAGE_KEY) ?? '{}')).toMatchObject({ recorded: {} });
  });

  it.each(['loading', 'error'] as const)('while the list is %s, 記録して次へ does not record, does not move and does not claim the list is done', async phase => {
    // 最初は即答。条件を変えた後の取得は、テストが返すまで止めておく
    const held: ((r: ApiResult<CallQueueResponse>) => void)[] = [];
    let hold = false;
    const items = [makeItem('1'), makeItem('2')];
    const queue = (f: QueueFilters) => hold
      ? new Promise<ApiResult<CallQueueResponse>>(resolve => { held.push(resolve); })
      : Promise.resolve<ApiResult<CallQueueResponse>>({ ok: true, data: makeResponse(f, items) });
    render(<CallQueueScreen fetcher={queue} ownersFetcher={fixtureOwnersFetch} detailFetcher={neverDetail}
      metadataFetcher={okMetadataFetch} initialSearch="?view=queue" now={NOW} />);
    await waitFor(() => { expect(screen.getByRole('list', { name: '架電キュー' })).toBeTruthy(); });
    open('1');
    await formReady();
    fireEvent.click(outcome('担当者と会話'));
    expect(recordDisabled()).toBe(false);
    hold = true;
    fireEvent.click(screen.getByRole('checkbox', { name: '次回日が来たものだけ' }));
    expect(screen.getByText('読み込み中…')).toBeTruthy();
    if (phase === 'error') {
      await act(async () => { held[0]?.({ ok: false, error: new ApiHttpError(502, { error_kind: 'hubspot_upstream' }) }); await Promise.resolve(); });
      expect(screen.getByText('取得できませんでした')).toBeTruthy();
      expect(within(form()).getByText('一覧を表示できていないため記録できません。一覧を表示してから記録してください。')).toBeTruthy();
    } else {
      expect(within(form()).getByText('一覧を読み込み中です。一覧が表示されてから記録してください。')).toBeTruthy();
    }
    expect(recordDisabled()).toBe(true);
    fireEvent.click(recordBtn());
    fireEvent.keyDown(outcome('担当者と会話'), { key: 'Enter', ctrlKey: true });
    expect(selectedId()).toBe('1');
    expect(within(form()).queryByText('表示中の一覧に未記録の架電先はありません。')).toBeNull();
    expect(within(form()).queryByText('記録済み(未送信)')).toBeNull();
    expect(JSON.parse(window.sessionStorage.getItem(DRAFT_STORAGE_KEY) ?? '{}')).toMatchObject({ recorded: {} });
  });

  it('a form that showed missing-field messages on deal A starts clean on deal B', async () => {
    await renderScreen();
    open('1');
    await formReady();
    fireEvent.click(outcome('再架電の約束'));
    fireEvent.click(recordBtn());
    expect(within(form()).getByText('次回架電日を入れてください(再架電のとき必須)。')).toBeTruthy();
    expect(within(form()).getByLabelText(/^次回架電日/).getAttribute('aria-invalid')).toBe('true');
    open('2');
    await formReady();
    expect(within(form()).queryByText('今回の結果を選んでください。')).toBeTruthy(); // 未選択の案内 (赤くしない文言) だけ
    fireEvent.click(outcome('再架電の約束'));
    expect(within(form()).queryByText('次回架電日を入れてください(再架電のとき必須)。')).toBeNull();
    expect(within(form()).queryByText('次回架電時間を選んでください(再架電のとき必須)。')).toBeNull();
    expect(within(form()).getByLabelText(/^次回架電日/).getAttribute('aria-invalid')).not.toBe('true');
    expect(form().querySelectorAll('[aria-invalid="true"]')).toHaveLength(0);
  });

  it('次回日が来たものだけ: the list is fetched again when the JST date changes while the screen stays open', async () => {
    let t = Date.UTC(2026, 9, 8, 14, 50, 0); // JST 2026-10-08 23:50
    const seen: QueueFilters[] = [];
    const queue = (f: QueueFilters) => { seen.push(f); return Promise.resolve<ApiResult<CallQueueResponse>>({ ok: true, data: makeResponse(f, [makeItem(String(seen.length))]) }); };
    render(<CallQueueScreen fetcher={queue} ownersFetcher={fixtureOwnersFetch} detailFetcher={neverDetail}
      metadataFetcher={okMetadataFetch} initialSearch="?view=queue&due=today" now={() => t} />);
    await waitFor(() => { expect(within(list()).getByText('架空会社1')).toBeTruthy(); });
    expect(seen).toHaveLength(1);
    // 同じ日のうちに画面へ戻っても取り直さない
    act(() => { window.dispatchEvent(new Event('focus')); });
    expect(seen).toHaveLength(1);
    t = Date.UTC(2026, 9, 8, 15, 30, 0); // JST 2026-10-09 00:30
    act(() => { window.dispatchEvent(new Event('focus')); });
    await waitFor(() => { expect(within(list()).getByText('架空会社2')).toBeTruthy(); });
    expect(seen).toHaveLength(2);
    expect(seen[1]?.due).toBe('today');
  });

  it('without 次回日が来たものだけ, a date change does not fetch the list again', async () => {
    let t = Date.UTC(2026, 9, 8, 14, 50, 0);
    let n = 0;
    const queue = (f: QueueFilters) => { n += 1; return Promise.resolve<ApiResult<CallQueueResponse>>({ ok: true, data: makeResponse(f, [makeItem('1')]) }); };
    render(<CallQueueScreen fetcher={queue} ownersFetcher={fixtureOwnersFetch} detailFetcher={neverDetail}
      metadataFetcher={okMetadataFetch} initialSearch="?view=queue" now={() => t} />);
    await waitFor(() => { expect(within(list()).getByText('架空会社1')).toBeTruthy(); });
    t = Date.UTC(2026, 9, 8, 15, 30, 0);
    act(() => { window.dispatchEvent(new Event('focus')); });
    await act(async () => { await Promise.resolve(); });
    expect(n).toBe(1);
  });

  it('the record button and the not-saved note sit outside the scrolling fields; the list column is not a live region', async () => {
    await renderScreen();
    open('1');
    await formReady();
    expect(form().querySelector('.rf-body .rf-record')).toBeNull();
    const actions = form().querySelector('.rf-actions');
    expect(actions?.parentElement).toBe(form());
    expect(actions?.textContent).toContain('HubSpot には保存されません');
    expect(recordBtn().getAttribute('aria-describedby')).toBe(within(form()).getByText(/HubSpot には保存されません/).id);
    expect(screen.getByRole('region', { name: '架電先の一覧' }).getAttribute('aria-live')).toBeNull();
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

  it('still works when sessionStorage is unavailable, but says in red that the input will be lost (not the usual 記録済み)', async () => {
    vi.spyOn(window, 'sessionStorage', 'get').mockImplementation(() => { throw new Error('blocked'); });
    await renderScreen();
    open('1');
    await formReady();
    expect(within(form()).getByTestId('unsaved-alert').textContent).toContain('この画面を閉じたり再読み込みしたりすると入力が消えます');
    expect(within(form()).getByText('下書き(この画面を閉じると消えます)')).toBeTruthy();
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.click(recordBtn());
    expect(selectedId()).toBe('2');
    expect(within(rowOf('1')).getByText('記録済み(画面を閉じると消えます)')).toBeTruthy();
    expect(within(rowOf('1')).queryByText('記録済み(未送信)')).toBeNull();
    expect(within(rowOf('1')).getByText('記録済み(画面を閉じると消えます)').className).toContain('is-unsaved');
  });

  it('when writes to sessionStorage throw (quota / blocked), the alert shows and a reload really loses the marks', async () => {
    // 読めるが書けない sessionStorage (容量超過・ブロック)
    const full = { getItem: () => null, setItem: () => { throw new Error('QuotaExceededError'); }, removeItem: () => undefined, clear: () => undefined, key: () => null, length: 0 };
    vi.spyOn(window, 'sessionStorage', 'get').mockReturnValue(full);
    const first = await renderScreen();
    open('1');
    await formReady();
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.click(recordBtn());
    expect(within(rowOf('1')).getByText('記録済み(画面を閉じると消えます)')).toBeTruthy();
    expect(within(form()).getAllByRole('alert').map(a => a.textContent).join('')).toContain('入力を残せていません');
    first.unmount();
    await renderScreen();
    expect(within(rowOf('1')).queryByText(/記録済み/)).toBeNull();
  });

  it('a normal session shows no unsaved alert and explains the marks stay only in this tab', async () => {
    await renderScreen();
    open('1');
    await formReady();
    expect(within(form()).queryByTestId('unsaved-alert')).toBeNull();
    expect(within(form()).getByText('この画面(タブ)だけに残ります。タブを閉じると消え、HubSpot には保存されません')).toBeTruthy();
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.click(recordBtn());
    expect(within(rowOf('1')).getByText('記録済み(未送信)').getAttribute('title')).toBe('この画面(タブ)だけに残ります。タブを閉じると消え、HubSpot には保存されません');
    expect(document.body.textContent).not.toMatch(/このブラウザで|送信は未実装|代用/);
  });

  it('callback → appointment: the auto-filled 再架電 goes away, so only the three appointment fields are required', async () => {
    await renderScreen();
    open('1');
    await formReady();
    fireEvent.click(outcome('再架電の約束'));
    expect(within(form()).getByRole<HTMLInputElement>('radio', { name: '再架電' }).checked).toBe(true);
    fireEvent.click(outcome('アポイント獲得'));
    expect(within(form()).getByRole<HTMLInputElement>('radio', { name: '再架電' }).checked).toBe(false);
    fireEvent.click(recordBtn());
    const shown = Array.from(form().querySelectorAll('.rf-err')).map(e => e.textContent);
    expect(shown.sort()).toEqual(['商談予定日を入れてください。', '商談予定時間を選んでください。', '商談方法を選んでください。'].sort());
    expect(within(form()).getByLabelText(/^次回架電日/).closest('label')?.textContent).not.toContain('必須');
  });

  it('editing after 記録して次へ says the mark was removed and asks to record again', async () => {
    await renderScreen();
    open('1');
    await formReady();
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.click(recordBtn());
    open('1');
    expect(within(form()).queryByText(/印を外しました/)).toBeNull();
    fireEvent.change(within(form()).getByLabelText(/^タスクメモ/), { target: { value: '追記' } });
    expect(within(form()).getByText('内容を変えたので「記録済み」の印を外しました。もう一度「記録して次へ」を押してください。')).toBeTruthy();
    expect(within(rowOf('1')).queryByText('記録済み(未送信)')).toBeNull();
  });

  it('the unreachable check is shown with the HubSpot label (same as the form), not the stored value', async () => {
    await renderScreen({ items: [makeItem('1', { stop: { prohibited_reason: null, block_reason: null, unreachable_check: '即切電（コール音なし）' } }), makeItem('2')] });
    open('2');
    await formReady();
    const flag = within(rowOf('1')).getByText('不通チェック');
    expect(flag.getAttribute('title')).toBe('不通時チェック: 常時即切電（コール音なし）');
  });

  it('live: a failed metadata request shows an error (no fixture options) and retry loads the definitions', async () => {
    const meta = metadataStub();
    await renderScreen({ metadataFetcher: meta.fetcher });
    expect(meta.calls).toHaveLength(0); // 案件を選ぶまで取得しない
    open('1');
    expect(within(form()).getByText('選択肢を読み込み中…')).toBeTruthy();
    await act(async () => { meta.calls[0]?.({ ok: false, error: new ApiHttpError(502, { error_kind: 'hubspot_upstream' }) }); await Promise.resolve(); });
    const alert = within(form()).getByRole('alert');
    expect(alert.textContent).toContain('HubSpot から選択肢を読み込めませんでした。HubSpot との通信に失敗しました。再試行してください。');
    expect(alert.textContent).toContain('再試行しても表示されないときは管理者に連絡してください。');
    // HubSpot の席が無い架電担当者にはできない「HubSpot で直接入力」は案内しない
    expect(alert.textContent).not.toContain('直接入力');
    expect(alert.textContent).not.toContain('代用');
    expect(within(form()).queryByRole('group', { name: '今回の結果' })).toBeNull();
    expect(recordDisabled()).toBe(true);
    // 別の案件に移っても取り直さない (失敗のまま)。再試行で取り直す
    open('2');
    expect(meta.calls).toHaveLength(1);
    fireEvent.click(within(form()).getByRole('button', { name: '再試行' }));
    await act(async () => { meta.calls[1]?.({ ok: true, data: metadataFromMoc() }); await Promise.resolve(); });
    await formReady();
    expect(meta.calls).toHaveLength(2);
  });

  it('live: definitions missing from the response are named in Japanese only (no internal names), not substituted', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const fetcher: MetadataFetch = () => Promise.resolve({ ok: true, data: metadataFromMoc(['bpo_40', 'bpo_42', 'bpo_14', 'bpo_10', 'bpo_4', 'bpo__', 'bpo_33', 'bpo_13', 'bpo_23', 'bpo_16', 'bpo_3']) });
    await renderScreen({ metadataFetcher: fetcher });
    open('1');
    const alert = await within(form()).findByRole('alert');
    expect(alert.textContent).toContain('HubSpot から次の項目の選択肢・設定を受け取れませんでした: 次アクション種別、その他理由');
    expect(alert.textContent).not.toMatch(/bpo_/);
    // 内部名は調べるときのためにコンソールにだけ出す
    expect(warn).toHaveBeenCalledWith('[crm] /api/crm/metadata lacks required deal definitions:', 'bpo_45, bpo_57');
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
    const saved = JSON.parse(window.sessionStorage.getItem(DRAFT_STORAGE_KEY) ?? '{}') as { drafts: Record<string, { outcome: string; nextAction: string; nextActionAuto: boolean }> };
    expect(Object.keys(saved.drafts)).toEqual(['fixture:f-2']);
    expect(saved.drafts['fixture:f-2']).toMatchObject({ outcome: 'callback', nextAction: '再架電', nextActionAuto: true });
    expect(Object.keys(saved.drafts).some(k => k.startsWith('live:'))).toBe(false);
  });

  it('fixture mode: 記録して次へ marks the sample row, and the live deal with the same id is not marked', async () => {
    // 実データ・架空サンプルとも同じ ID (1, 2) を返す取得関数
    const items = [makeItem('1'), makeItem('2')];
    const queue = (f: QueueFilters) => Promise.resolve<ApiResult<CallQueueResponse>>({ ok: true, data: makeResponse(f, items) });
    render(<CallQueueScreen fetcher={queue} ownersFetcher={fixtureOwnersFetch} detailFetcher={neverDetail}
      metadataFetcher={okMetadataFetch} initialSearch="?view=queue&mode=fixture" now={NOW} />);
    await waitFor(() => { expect(screen.getByRole('list', { name: '架電キュー' })).toBeTruthy(); });
    open('1');
    await formReady();
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.click(recordBtn());
    expect(within(rowOf('1')).getByText('記録済み(未送信)')).toBeTruthy();
    expect(selectedId()).toBe('2');
    // 2 も記録すると、1 は記録済みなので次は無い
    fireEvent.click(outcome('担当者と会話'));
    fireEvent.click(recordBtn());
    expect(within(form()).getByText('表示中の一覧に未記録の架電先はありません。')).toBeTruthy();
    expect(JSON.parse(window.sessionStorage.getItem(DRAFT_STORAGE_KEY) ?? '{}')).toMatchObject({ recorded: { 'fixture:1': true, 'fixture:2': true } });
    fireEvent.click(within(screen.getByRole('group', { name: 'データの切り替え' })).getByRole('button', { name: '実データ' }));
    await waitFor(() => { expect(screen.getByRole('list', { name: '架電キュー' })).toBeTruthy(); });
    expect(within(list()).queryByText('記録済み(未送信)')).toBeNull();
    open('1');
    await formReady();
    expect(within(form()).queryByText('記録済み(未送信)')).toBeNull();
    expect(outcome('担当者と会話').getAttribute('aria-pressed')).toBe('false');
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
