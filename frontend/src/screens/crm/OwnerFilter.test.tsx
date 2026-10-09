// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError, ApiNetworkError } from '../../api/client';
import type { ApiError, ApiResult } from '../../api/client';
import type { CrmOwner } from '../../generated/CrmOwner';
import type { CrmOwnersResponse } from '../../generated/CrmOwnersResponse';
import { CallQueueScreen } from './CallQueueScreen';
import { deferredFetcher, makeItem, makeResponse, okUserFetch, okCatalogFetch, resetDockStorage } from './queueTestUtil';
import { DEFAULT_FILTERS } from './queueModel';
import type { OwnersFetch } from './useOwners';

beforeEach(() => { resetDockStorage(); });
afterEach(() => { cleanup(); });

const o = (id: string, name: string, email: string | null, archived = false): CrmOwner => ({ id, name, email, archived });
const LIST = [
  o('101', '架空 一郎', 'ichiro@example.invalid'),
  o('102', '架空 一郎', 'ichiro2@example.invalid'),
  o('103', 'サンプル 花子', null),
  o('104', 'ダミー 退職', 'gone@example.invalid', true),
];
const okOwners = (owners = LIST, truncated = false): ApiResult<CrmOwnersResponse> =>
  ({ ok: true, data: { owners, truncated, generated_at: '2026-10-05T03:00:00Z' } });
const failOwners = (error: ApiError): ApiResult<CrmOwnersResponse> => ({ ok: false, error });

function ownersStub(result: ApiResult<CrmOwnersResponse>) {
  return vi.fn<OwnersFetch>(() => Promise.resolve(result));
}

async function renderAdmin(ownersFetcher: OwnersFetch, search = '?view=queue') {
  const q = deferredFetcher();
  render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={q.fetcher} ownersFetcher={ownersFetcher} initialSearch={search} />);
  const first = q.calls[0];
  await act(async () => {
    first?.resolve({ ok: true, data: makeResponse(first.filters, [makeItem('1', { owner_id: '102' })]) });
    await Promise.resolve();
  });
  return q;
}

function pickMode() {
  fireEvent.change(screen.getByLabelText('所有者'), { target: { value: 'pick' } });
}

function optionTexts(el: HTMLElement) {
  return Array.from((el as HTMLSelectElement).options).map(x => x.text);
}

describe('owner picker (everyone)', () => {
  it('lets the admin search by name and choose a person; the queue is refetched with that owner ID', async () => {
    const q = await renderAdmin(ownersStub(okOwners()));
    pickMode();
    const list = await screen.findByLabelText('所有者を選ぶ');
    // 退職者は既定で隠れ、同名の 2 人は email で見分けられる
    expect(optionTexts(list)).toEqual([
      '架空 一郎(ichiro@example.invalid)', '架空 一郎(ichiro2@example.invalid)', 'サンプル 花子(ID 103)',
    ]);
    fireEvent.change(screen.getByLabelText('所有者を検索'), { target: { value: '花子' } });
    expect(Array.from(screen.getByLabelText<HTMLSelectElement>('所有者を選ぶ').options).map(x => x.value)).toEqual(['103']);
    fireEvent.change(screen.getByLabelText('所有者を選ぶ'), { target: { value: '103' } });
    await waitFor(() => { expect(q.calls.at(-1)?.filters.owner).toBe('103'); });
  });

  it('shows retired people only after the toggle, and picking the second same-named person passes that ID', async () => {
    const q = await renderAdmin(ownersStub(okOwners()));
    pickMode();
    await screen.findByLabelText('所有者を選ぶ');
    expect(screen.queryByText(/ダミー 退職/)).toBeNull();
    fireEvent.click(screen.getByLabelText('退職者も表示'));
    expect(screen.getByText(/ダミー 退職.*\[退職者\]/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText('所有者を選ぶ'), { target: { value: '102' } });
    await waitFor(() => { expect(q.calls.at(-1)?.filters.owner).toBe('102'); });
  });

  it('says so when there are zero owners', async () => {
    await renderAdmin(ownersStub(okOwners([])));
    pickMode();
    expect(await screen.findByText('該当する所有者がいません。')).toBeTruthy();
  });

  it('falls back to an owner-ID input when the list cannot be fetched (never a silent empty state)', async () => {
    const errors = [
      new ApiHttpError(429, { error_kind: 'hubspot_rate_limited' }),
      new ApiNetworkError('offline'),
      new ApiHttpError(504, { error_kind: 'hubspot_timeout' }),
    ];
    for (const error of errors) {
      cleanup();
      const q = await renderAdmin(ownersStub(failOwners(error)));
      pickMode();
      const alert = await screen.findByRole('alert');
      expect(alert.textContent).toContain('所有者の一覧を取得できませんでした');
      const input = screen.getByLabelText('所有者ID(HubSpot owner ID)');
      fireEvent.change(input, { target: { value: '55a5' } });
      await waitFor(() => { expect(q.calls.at(-1)?.filters.owner).toBe('555'); });
      expect(screen.getByText('一覧を再取得')).toBeTruthy();
    }
  });

  it('retries the list when asked, and then offers names', async () => {
    let n = 0;
    const fetcher: OwnersFetch = () => {
      n += 1;
      return Promise.resolve(n === 1 ? failOwners(new ApiHttpError(502, { error_kind: 'hubspot_upstream' })) : okOwners());
    };
    await renderAdmin(fetcher);
    pickMode();
    fireEvent.click(await screen.findByText('一覧を再取得'));
    expect(await screen.findByLabelText('所有者を選ぶ')).toBeTruthy();
    expect(n).toBe(2);
  });

  it('shows a loading note while the list is being fetched', async () => {
    const fetcher: OwnersFetch = () => new Promise<ApiResult<CrmOwnersResponse>>(() => undefined);
    await renderAdmin(fetcher);
    pickMode();
    expect(screen.getByText('所有者の一覧を読み込み中…')).toBeTruthy();
  });

  it('keeps an owner from the URL visible (retired or missing from the list) on the trigger, and in the list when opened', async () => {
    await renderAdmin(ownersStub(okOwners()), '?view=queue&owner=104');
    expect(screen.queryByLabelText('所有者を選ぶ')).toBeNull();
    const trigger = () => screen.getByLabelText<HTMLSelectElement>('所有者');
    expect(trigger().selectedOptions[0]?.text).toMatch(/ダミー 退職/);
    pickMode();
    expect((await screen.findByLabelText<HTMLSelectElement>('所有者を選ぶ')).value).toBe('104');
    cleanup();
    await renderAdmin(ownersStub(okOwners()), '?view=queue&owner=999');
    expect(trigger().selectedOptions[0]?.text).toBe('ID 999');
    pickMode();
    const l2 = await screen.findByLabelText<HTMLSelectElement>('所有者を選ぶ');
    expect(l2.value).toBe('999');
    expect(screen.getByText(/ID 999.*一覧にありません/)).toBeTruthy();
  });

  it('closes the list on selection and returns focus to the trigger; the owner stays applied', async () => {
    const q = await renderAdmin(ownersStub(okOwners()));
    pickMode();
    fireEvent.change(await screen.findByLabelText('所有者を選ぶ'), { target: { value: '103' } });
    expect(screen.queryByLabelText('所有者を選ぶ')).toBeNull();
    expect(document.activeElement).toBe(screen.getByLabelText('所有者'));
    await waitFor(() => { expect(q.calls.at(-1)?.filters.owner).toBe('103'); });
    expect(screen.getByLabelText<HTMLSelectElement>('所有者').selectedOptions[0]?.text).toMatch(/サンプル 花子/);
  });

  it('closes on Esc (focus back on the trigger), on outside click, and when focus leaves', async () => {
    await renderAdmin(ownersStub(okOwners()));
    const open = async () => { pickMode(); await screen.findByLabelText('所有者を選ぶ'); };
    await open();
    fireEvent.keyDown(screen.getByLabelText('所有者を検索'), { key: 'Escape' });
    expect(screen.queryByLabelText('所有者を選ぶ')).toBeNull();
    expect(document.activeElement).toBe(screen.getByLabelText('所有者'));
    await open();
    fireEvent.mouseDown(screen.getByLabelText('並び替え'));
    expect(screen.queryByLabelText('所有者を選ぶ')).toBeNull();
    await open();
    // 中のクリックでは閉じない
    fireEvent.mouseDown(screen.getByLabelText('所有者を検索'));
    expect(screen.queryByLabelText('所有者を選ぶ')).not.toBeNull();
    fireEvent.blur(screen.getByLabelText('所有者を検索'), { relatedTarget: screen.getByLabelText('並び替え') });
    expect(screen.queryByLabelText('所有者を選ぶ')).toBeNull();
  });

  it('shows the name (not the raw ID) in the table when the owner is in the list', async () => {
    await renderAdmin(ownersStub(okOwners()));
    await waitFor(() => { expect(screen.getAllByText('架空 一郎').length).toBeGreaterThan(0); });
    expect(screen.queryByText('102')).toBeNull();
  });

  it('fetches the list once, not on every filter change; the other choices keep working', async () => {
    const fetcher = ownersStub(okOwners());
    const q = await renderAdmin(fetcher);
    await waitFor(() => { expect(fetcher).toHaveBeenCalledTimes(1); });
    fireEvent.change(screen.getByLabelText('所有者'), { target: { value: 'unassigned' } });
    expect(q.calls.at(-1)?.filters.owner).toBe('unassigned');
    fireEvent.change(screen.getByLabelText('所有者'), { target: { value: 'me' } });
    expect(q.calls.at(-1)?.filters.owner).toBe('me');
    fireEvent.change(screen.getByLabelText('所有者'), { target: { value: 'all' } });
    expect(q.calls.at(-1)?.filters.owner).toBe('all');
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it('shows the owner picker to a non-admin too: default is "me", the list is fetched, and everything is selectable', async () => {
    const fetcher = ownersStub(okOwners());
    const q = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={q.fetcher} ownersFetcher={fetcher} initialSearch="?view=queue" />);
    const first = q.calls[0];
    const resp = makeResponse(DEFAULT_FILTERS, [makeItem('1')]);
    await act(async () => {
      first?.resolve({ ok: true, data: { ...resp, scope: { ...resp.scope, role: 'own', owner: 'me' } } });
      await Promise.resolve();
    });
    const select = screen.getByLabelText<HTMLSelectElement>('所有者');
    // 初期値は自分 (応答の scope.owner = me)。条件の owner は既定 ('') のまま
    expect(select.value).toBe('me');
    expect(q.calls[0]?.filters.owner).toBe('');
    expect(screen.getByTestId('scope-note').textContent).toContain('所有者: 自分 を表示中');
    await waitFor(() => { expect(fetcher).toHaveBeenCalledTimes(1); });
    // 全員分は明示の all で送る (空にすると「自分」に戻ってしまう)
    fireEvent.change(select, { target: { value: 'all' } });
    expect(q.calls.at(-1)?.filters.owner).toBe('all');
    fireEvent.change(select, { target: { value: 'unassigned' } });
    expect(q.calls.at(-1)?.filters.owner).toBe('unassigned');
    // 他人を名前で選ぶ
    pickMode();
    fireEvent.change(await screen.findByLabelText('所有者を選ぶ'), { target: { value: '103' } });
    await waitFor(() => { expect(q.calls.at(-1)?.filters.owner).toBe('103'); });
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it('asks to pick an owner (not a dead end) when the person has no matching HubSpot owner, then loads after choosing', async () => {
    const q = deferredFetcher();
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={q.fetcher} ownersFetcher={ownersStub(okOwners())} initialSearch="?view=queue" />);
    await act(async () => {
      q.calls[0]?.resolve({ ok: false, error: new ApiHttpError(409, { error_kind: 'owner_not_resolved' }) });
      await Promise.resolve();
    });
    // エラー表示 (再試行ボタン) ではなく、選択を促す表示。一覧は最初から開いている
    expect(screen.getByTestId('owner-pick-prompt').textContent).toContain('所有者を選んでください');
    expect(screen.queryByText('再試行')).toBeNull();
    const list = await screen.findByLabelText('所有者を選ぶ');
    fireEvent.change(list, { target: { value: '102' } });
    await waitFor(() => { expect(q.calls.at(-1)?.filters.owner).toBe('102'); });
    // 選んだら全員分に倒れず、その所有者で取り直す
    const last = q.calls.at(-1);
    await act(async () => {
      last?.resolve({ ok: true, data: makeResponse(last.filters, [makeItem('1', { owner_id: '102' })]) });
      await Promise.resolve();
    });
    expect(screen.queryByTestId('owner-pick-prompt')).toBeNull();
    expect(screen.getByTestId('scope-note').textContent).toContain('所有者: 架空 一郎 を表示中');
  });
});
