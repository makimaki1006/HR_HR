// @vitest-environment happy-dom
// 架電結果の入力欄 (実在の取引プロパティへの保存)。架空サンプルのモードで、画面全体を通して確かめる
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ApiResult } from '../../api/client';
import type { CallQueueResponse } from '../../generated/CallQueueResponse';
import type { CrmOwnersResponse } from '../../generated/CrmOwnersResponse';
import { CallQueueScreen } from './CallQueueScreen';
import { MEMO_EDIT_HINT, RESULT_SCHEMA_FAILED, RESULT_WRITES_OFF } from './CallResultForm';
import { DRAFT_STORAGE_KEY } from './callResultModel';
import type { PatchOutcome, WriteApi } from './crmWrite';
import { FAKE_SCHEMA, fakeWriteApi, resetFakeWrite } from './fakeWrite';
import { makeItem, makeResponse, okCatalogFetch, okUserFetch, resetDockStorage } from './queueTestUtil';
import type { QueueFilters } from './queueModel';
import type { DetailFetch } from './useDealDetail';
import { fixtureDetail } from './workspaceFixture';
import { FIXTURE_OWNERS } from './ownerModel';
import type { OwnersFetch } from './useOwners';
import type { PatchRequest } from './writeTypes';

// JST 2026-10-14 12:00
const NOW = () => Date.UTC(2026, 9, 14, 3, 0, 0);
const TODAY = '2026-10-14';

beforeEach(() => { try { window.sessionStorage.clear(); } catch { /* ignore */ } resetDockStorage(); resetFakeWrite(); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

type PatchSpy = ReturnType<typeof vi.fn<WriteApi['patchDeal']>>;
/** 架空の書き込みをそのまま使い、送った本文を記録する */
function spyApi(over: Partial<WriteApi> = {}): { api: WriteApi; patch: PatchSpy } {
  const patch = vi.fn<WriteApi['patchDeal']>((id, body) => fakeWriteApi.patchDeal(id, body));
  return { api: { ...fakeWriteApi, patchDeal: patch, ...over }, patch };
}

async function renderFixture(search: string, api: WriteApi) {
  render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} writeApi={api} now={NOW}
    initialSearch={`?view=queue&mode=fixture${search}`} />);
  await waitFor(() => { expect(screen.getByRole('list', { name: '架電キュー' })).toBeTruthy(); });
}
const list = () => screen.getByRole('list', { name: '架電キュー' });
const showForm = () => { fireEvent.click(screen.getByRole('tab', { name: '架電結果の入力' })); };
const open = (company: string) => { fireEvent.click(within(list()).getByText(company)); showForm(); };
const form = () => screen.getByRole('form', { name: '架電結果の入力' });
const selectedId = () => screen.getByTestId('result-slot').getAttribute('data-deal-id');
const recordBtn = () => within(form()).getByRole<HTMLButtonElement>('button', { name: /記録して次へ/ });
/** 入力欄が書ける状態になるまで待つ (案件の詳細と書ける項目の一覧が読めたら、追記の欄が出る) */
async function formReady() { await within(form()).findByRole('button', { name: /^不在/ }); }
const fieldLabels = () => Array.from(form().querySelectorAll('[data-field] .rf-label')).map(e => e.textContent);
const bodyOf = (patch: PatchSpy, i = 0): PatchRequest => { const c = patch.mock.calls[i]; if (!c) throw new Error('no PATCH'); return c[1]; };
/** 項目の入力欄 (項目名の見出しと入力欄の両方に名前があるので、入力欄だけを取る) */
const input = (name: string): HTMLInputElement => { const el = within(form()).getAllByLabelText<HTMLInputElement>(name).find(x => x instanceof HTMLInputElement); if (!el) throw new Error(name); return el; };

describe('架電結果の入力欄: チームごとの項目の組', () => {
  it('BPO のパイプラインの取引 (チーム不明) は BPO の組: 架電日・次回架電日・次回架電時間・タスクメモ・接触・日程切り', async () => {
    const { api } = spyApi();
    await renderFixture('', api);
    open('架空食品株式会社');
    await formReady();
    expect(form().getAttribute('data-team')).toBe('bpo');
    expect(within(form()).getByTestId('result-group').textContent).toBe('BPOの項目');
    expect(fieldLabels()).toEqual(['架電日', '次回架電日', '次回架電時間', 'タスクメモ', '接触', '日程切り']);
    // 内部名は画面に出さない
    expect(form().textContent).not.toMatch(/bpo_|risuto_|nitteigiri/);
    // よく使うステージのボタン (今のステージ「不在」は押せない)
    expect(within(form()).getByRole('button', { name: /^不在/ }).hasAttribute('disabled')).toBe(true);
    expect(within(form()).getByRole('button', { name: /^受付ブロック/ }).hasAttribute('disabled')).toBe(false);
  });

  it('BPO 以外のパイプラインの取引は新規営業の組: 架電日・再架電日・再架電時間・激アツメモ', async () => {
    const { api } = spyApi();
    await renderFixture('&pipeline=fx-sample', api);
    open('架空倉庫サービス');
    await within(form()).findByRole('button', { name: /^TEL_アポ日確定/ });
    expect(form().getAttribute('data-team')).toBe('sales');
    expect(within(form()).getByTestId('result-group').textContent).toBe('新規営業の項目');
    expect(fieldLabels()).toEqual(['架電日', '再架電日', '再架電時間', '激アツメモ（決済・利用サービス・決算・職種など）']);
  });
});

describe('架電結果の入力欄: 初期値と追記', () => {
  it('架電日が空なら今日 (JST) を最初から入れる。前回の日は無いので出さない', async () => {
    const { api, patch } = spyApi();
    await renderFixture('&q=架空商事', api);
    open('架空商事株式会社');
    await formReady();
    expect(input('架電日').value).toBe('2026/10/14');
    expect(within(form()).queryByTestId('call-date-prev')).toBeNull();
    // 何も変えずに記録すると、架電日 (今日) だけを送る
    fireEvent.click(recordBtn());
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    expect(bodyOf(patch).set).toEqual({ risuto_kadennbi: TODAY });
    expect(bodyOf(patch).base).toEqual({ risuto_kadennbi: null });
    expect(bodyOf(patch).stage).toBeUndefined();
  });

  it('前回の架電日が入っていても今日を最初から入れ、前回の日は欄の横に小さく出す。保存すると今日で上書きする', async () => {
    const { api, patch } = spyApi();
    await renderFixture('', api);
    open('架空食品株式会社');
    await formReady();
    expect(input('架電日').value).toBe('2026/10/14');
    expect(within(form()).getByTestId('call-date-prev').textContent).toBe('前回: 09/28');
    fireEvent.click(recordBtn());
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    expect(bodyOf(patch).set).toEqual({ risuto_kadennbi: TODAY });
    expect(bodyOf(patch).base).toEqual({ risuto_kadennbi: '2026-09-28' });
  });

  it('架電日は変えられる (前回と同じ日に戻せば送らない)', async () => {
    const { api, patch } = spyApi();
    await renderFixture('', api);
    open('架空食品株式会社');
    await formReady();
    fireEvent.change(input('架電日'), { target: { value: '2026/09/28' } });
    fireEvent.change(input('日程切り'), { target: { value: '1' } });
    fireEvent.click(recordBtn());
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    expect(bodyOf(patch).set).toEqual({ nitteigiri: '1' });
  });

  it('タスクメモ (複数行) は、今の内容の末尾に改行と日付を付けて足す。今の内容は読み取りで上に出す', async () => {
    const { api, patch } = spyApi();
    await renderFixture('', api);
    open('架空食品株式会社');
    await formReady();
    const now = '09/28 受付で不在。採用担当は火曜午後(架空)\n10/02 再度不在。折り返し依頼済み';
    expect(within(form()).getByTestId('memo-current').textContent).toBe(now);
    expect(within(form()).getByText(MEMO_EDIT_HINT)).toBeTruthy();
    fireEvent.change(within(form()).getByRole('textbox', { name: 'タスクメモに追記' }), { target: { value: '不在。午後に再架電  \n' } });
    fireEvent.click(recordBtn());
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    expect(bodyOf(patch).set.bpo_16).toBe(`${now}\n10/14 不在。午後に再架電`);
    expect(bodyOf(patch).base.bpo_16).toBe(now);
  });

  it('激アツメモ (1 行) は改行ではなく「 / 」と日付で足す', async () => {
    const { api, patch } = spyApi();
    await renderFixture('&pipeline=fx-sample', api);
    open('架空倉庫サービス');
    await within(form()).findByRole('button', { name: /^TEL_アポ日確定/ });
    fireEvent.change(within(form()).getByRole('textbox', { name: /激アツメモ.*に追記/ }), { target: { value: '決裁者は社長 来週電話' } });
    fireEvent.click(recordBtn());
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    expect(bodyOf(patch).set.risuto_bikou).toBe('決裁は本社。拠点は採用人数だけ決める(架空) / 来月から媒体の契約見直し / 10/14 決裁者は社長 来週電話');
  });
});

describe('架電結果の入力欄: 保存', () => {
  it('必須の無いステージ: 項目とステージを 1 回の PATCH で送り、保存できたら次の行へ進む', async () => {
    const { api, patch } = spyApi();
    await renderFixture('', api);
    open('架空食品株式会社');
    await formReady();
    const from = selectedId();
    fireEvent.click(within(form()).getByRole('button', { name: /^受付ブロック/ }));
    expect(within(form()).getByRole('button', { name: /^受付ブロック/ }).getAttribute('aria-pressed')).toBe('true');
    fireEvent.change(input('次回架電日'), { target: { value: '2026/10/16' } });
    fireEvent.change(within(form()).getByRole('combobox', { name: '次回架電時間' }), { target: { value: '9:15' } });
    fireEvent.click(recordBtn());
    await waitFor(() => { expect(selectedId()).not.toBe(from); });
    expect(patch).toHaveBeenCalledTimes(1);
    const b = bodyOf(patch);
    expect(b.stage).toEqual({ pipeline_id: '753186575', stage_id: '1095387444' });
    expect(b.set).toEqual({ risuto_kadennbi: TODAY, bpo_13: '2026-10-16', bpo_14: '9:15' });
    expect(b.base).toEqual({ risuto_kadennbi: '2026-09-28', bpo_13: '2026-10-01', bpo_14: '10:00' });
    // 受付ブロックは架電対象のステージではないので、保存した行は一覧から外れる (外れた件数を件数の行に出す)
    expect(within(list()).queryByText('架空食品株式会社')).toBeNull();
    expect(screen.getByTestId('queue-count').textContent).toContain('(保存して外れた 1 件)');
    // 次の行の入力欄に、前の行を保存したことを緑で出す
    await waitFor(() => { expect(within(form()).getByTestId('result-notice').textContent).toBe('前の架電先: 架空食品株式会社 を HubSpot に保存しました。'); });
    expect(within(form()).getByTestId('result-notice').classList.contains('is-saved')).toBe(true);
  });

  it('選んでいるステージのボタンをもう一度押しても選択は外れない。外すのは「(変えない)」を選ぶとき', async () => {
    const { api, patch } = spyApi();
    await renderFixture('', api);
    open('架空食品株式会社');
    await formReady();
    const btn = () => within(form()).getByRole('button', { name: /^受付ブロック/ });
    fireEvent.click(btn());
    fireEvent.click(btn());
    expect(btn().getAttribute('aria-pressed')).toBe('true');
    const select = within(form()).getByRole<HTMLSelectElement>('combobox', { name: 'ステージを選ぶ' });
    expect(select.selectedOptions[0]?.textContent).toBe('受付ブロック');
    fireEvent.change(select, { target: { value: '' } });
    expect(btn().getAttribute('aria-pressed')).toBe('false');
    expect(patch).not.toHaveBeenCalled();
  });

  it('Ctrl+Enter でも記録して次へ', async () => {
    const { api, patch } = spyApi();
    await renderFixture('', api);
    open('架空食品株式会社');
    await formReady();
    const from = selectedId();
    fireEvent.change(input('日程切り'), { target: { value: '2' } });
    fireEvent.keyDown(input('日程切り'), { key: 'Enter', ctrlKey: true });
    await waitFor(() => { expect(selectedId()).not.toBe(from); });
    expect(bodyOf(patch).set).toEqual({ risuto_kadennbi: TODAY, nitteigiri: '2' });
  });

  it('必須のあるステージ: ステージを移す画面で必須を確かめ、入力欄の変更と一緒に 1 回で送る', async () => {
    const { api, patch } = spyApi();
    await renderFixture('', api);
    open('架空食品株式会社');
    await formReady();
    const from = selectedId();
    fireEvent.click(within(form()).getByRole('button', { name: /^不通/ }));
    expect(within(form()).getByTestId('stage-needs-input').textContent).toContain('「不通」に移すには必要な項目があります');
    fireEvent.change(within(form()).getByRole('textbox', { name: 'タスクメモに追記' }), { target: { value: '番号が通じない' } });
    fireEvent.click(recordBtn());
    const modal = await screen.findByTestId('stage-modal');
    expect(patch).not.toHaveBeenCalled();
    expect(within(modal).getByText('不通')).toBeTruthy();
    await within(modal).findByRole('combobox', { name: '不通時チェック' });
    expect(within(modal).getByTestId('stage-remaining').textContent).toContain('必須 あと 1 項目');
    const opt = within(modal).getByRole<HTMLSelectElement>('combobox', { name: '不通時チェック' }).options[1]?.value ?? '';
    expect(opt).not.toBe('');
    fireEvent.change(within(modal).getByRole('combobox', { name: '不通時チェック' }), { target: { value: opt } });
    fireEvent.click(within(modal).getByRole('button', { name: '移す' }));
    await waitFor(() => { expect(selectedId()).not.toBe(from); });
    expect(patch).toHaveBeenCalledTimes(1);
    const b = bodyOf(patch);
    expect(b.stage).toEqual({ pipeline_id: '753186575', stage_id: '1095387443' });
    expect(Object.keys(b.set).sort()).toEqual(['bpo_10', 'bpo_16', 'risuto_kadennbi']);
    expect(b.set.bpo_10).toBe(opt);
    expect(b.set.bpo_16).toMatch(/\n10\/14 番号が通じない$/u);
  });

  it('必須の画面で「やめる」と何も送らず、入力欄に戻る', async () => {
    const { api, patch } = spyApi();
    await renderFixture('', api);
    open('架空食品株式会社');
    await formReady();
    fireEvent.click(within(form()).getByRole('button', { name: /^不通/ }));
    fireEvent.click(recordBtn());
    const modal = await screen.findByTestId('stage-modal');
    fireEvent.click(within(modal).getByRole('button', { name: 'やめる' }));
    expect(screen.queryByTestId('stage-modal')).toBeNull();
    expect(patch).not.toHaveBeenCalled();
    expect(within(form()).getByRole('button', { name: /^不通/ }).getAttribute('aria-pressed')).toBe('true');
  });

  it('新規営業の TEL_アポ日確定: アポ取得者が空なら本人が入り、架電日は入力欄の値 (今日) が入って送られる', async () => {
    const { api, patch } = spyApi();
    await renderFixture('&pipeline=fx-sample', api);
    open('架空倉庫サービス');
    await within(form()).findByRole('button', { name: /^TEL_アポ日確定/ });
    fireEvent.click(within(form()).getByRole('button', { name: /^TEL_アポ日確定/ }));
    fireEvent.click(recordBtn());
    const modal = await screen.findByTestId('stage-modal');
    await within(modal).findByLabelText('アポ取得者');
    expect(within(modal).getByLabelText<HTMLInputElement>('架電日').value).toBe('2026/10/14');
    fireEvent.change(within(modal).getByLabelText('アポ取得日'), { target: { value: '2026/10/14' } });
    fireEvent.click(within(modal).getByRole('button', { name: '移す' }));
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    const b = bodyOf(patch);
    expect(b.stage).toEqual({ pipeline_id: 'fx-sample', stage_id: 'fx-appo' });
    expect(b.set).toEqual({ aposyutokusya: '9001', risuto_kadennbi: TODAY, aposyutokubi: TODAY });
  });

  it('保存できない (赤) ときは次へ進まず、理由を出す。入力は残る', async () => {
    const fail: WriteApi['patchDeal'] = () => Promise.resolve<PatchOutcome>({ kind: 'queue_full' });
    const { api } = spyApi({ patchDeal: vi.fn(fail) });
    await renderFixture('', api);
    open('架空食品株式会社');
    await formReady();
    const from = selectedId();
    fireEvent.change(within(form()).getByRole('textbox', { name: 'タスクメモに追記' }), { target: { value: '残しておく' } });
    fireEvent.click(recordBtn());
    const err = await within(form()).findByTestId('result-error');
    expect(err.textContent).toContain('保存できていません: いま保存を受け付けられません。何も保存されていません。');
    expect(err.getAttribute('role')).toBe('alert');
    expect(selectedId()).toBe(from);
    expect(within(form()).getByRole<HTMLTextAreaElement>('textbox', { name: 'タスクメモに追記' }).value).toBe('残しておく');
  });

  it('一時保存 (黄) も保存できたとして次へ進み、反映待ちと出す', async () => {
    const queued: WriteApi['patchDeal'] = () => Promise.resolve<PatchOutcome>({ kind: 'queued', response: { status: 'queued', operation_id: 'op-1' } });
    const { api } = spyApi({ patchDeal: vi.fn(queued), operation: () => new Promise(() => undefined) });
    await renderFixture('', api);
    open('架空食品株式会社');
    await formReady();
    const from = selectedId();
    fireEvent.change(input('日程切り'), { target: { value: '1' } });
    fireEvent.click(recordBtn());
    await waitFor(() => { expect(selectedId()).not.toBe(from); });
    await waitFor(() => { expect(within(form()).getByTestId('result-notice').textContent).toContain('一時保存しました'); });
    expect(within(form()).getByTestId('result-notice').classList.contains('is-queued')).toBe(true);
  });

  it('入力途中の内容はこのタブに下書きとして残る (送る前)', async () => {
    const { api } = spyApi();
    await renderFixture('', api);
    open('架空食品株式会社');
    await formReady();
    fireEvent.change(within(form()).getByRole('textbox', { name: 'タスクメモに追記' }), { target: { value: '途中' } });
    const raw = window.sessionStorage.getItem(DRAFT_STORAGE_KEY) ?? '';
    expect(raw).toContain('"bpo_16":"途中"');
  });
});

describe('架電結果の入力欄: 書けないとき', () => {
  it('書き込みが OFF のときは読み取りの表示にし、理由を 1 行出す。記録しても送らない', async () => {
    const { api, patch } = spyApi({ editSchema: dealId => Promise.resolve({ ok: true, data: { ...FAKE_SCHEMA(dealId), writes_enabled: false } }) });
    await renderFixture('', api);
    open('架空食品株式会社');
    expect(await within(form()).findByTestId('result-readonly')).toBeTruthy();
    expect(within(form()).getByTestId('result-readonly').textContent).toBe(RESULT_WRITES_OFF);
    expect(within(form()).queryByRole('textbox', { name: 'タスクメモに追記' })).toBeNull();
    expect(within(form()).queryByRole('button', { name: /^受付ブロック/ })).toBeNull();
    expect(recordBtn().getAttribute('aria-disabled')).toBe('true');
    fireEvent.click(recordBtn());
    expect(patch).not.toHaveBeenCalled();
  });

  it('書ける項目の一覧を読めない (権限が無い等) ときも読み取りの表示にする', async () => {
    const { api } = spyApi({ editSchema: () => Promise.resolve({ ok: false, error: new Error('forbidden') }) });
    await renderFixture('', api);
    open('架空食品株式会社');
    expect((await within(form()).findByTestId('result-readonly')).textContent).toBe(RESULT_SCHEMA_FAILED);
  });
});

describe('架電結果の入力欄: 実データのモードでチームから組を決める', () => {
  it('見ている人のチームが 新規営業 なら、BPO のパイプラインの取引でも新規営業の組', async () => {
    const owners: OwnersFetch = () => Promise.resolve<ApiResult<CrmOwnersResponse>>({ ok: true, data: {
      owners: FIXTURE_OWNERS.map(o => (o.id === '9001' ? { ...o, teams: ['新規営業'] } : o)), truncated: false, generated_at: '2026-10-14T00:00:00Z', me_id: '9001',
    } });
    const queue = (f: QueueFilters) => Promise.resolve<ApiResult<CallQueueResponse>>({ ok: true, data: makeResponse(f, [makeItem('1')]) });
    const detail: DetailFetch = (id, _signal, props) => {
      const d = fixtureDetail('f-2', props);
      if (d === null) throw new Error('fixture');
      return Promise.resolve({ ok: true, data: { ...d, deal: { ...d.deal, id } } });
    };
    const { api } = spyApi();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={queue} ownersFetcher={owners} detailFetcher={detail}
      writeApi={api} now={NOW} initialSearch="?view=queue" />);
    await waitFor(() => { expect(screen.getByRole('list', { name: '架電キュー' })).toBeTruthy(); });
    open('架空会社1');
    await waitFor(() => { expect(form().getAttribute('data-team')).toBe('sales'); });
    expect(within(form()).getByTestId('result-group').textContent).toBe('新規営業の項目');
  });
});
