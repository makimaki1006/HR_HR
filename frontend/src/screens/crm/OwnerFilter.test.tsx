// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError, ApiNetworkError } from '../../api/client';
import type { ApiError, ApiResult } from '../../api/client';
import type { CrmOwner } from '../../generated/CrmOwner';
import type { CrmOwnersResponse } from '../../generated/CrmOwnersResponse';
import { CallQueueScreen } from './CallQueueScreen';
import { deferredFetcher, makeItem, makeResponse } from './queueTestUtil';
import { DEFAULT_FILTERS } from './queueModel';
import type { OwnersFetch } from './useOwners';

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
  render(<CallQueueScreen fetcher={q.fetcher} ownersFetcher={ownersFetcher} initialSearch={search} />);
  const first = q.calls[0];
  await act(async () => {
    first?.resolve({ ok: true, data: makeResponse(first.filters, [makeItem('1', { owner_id: '102' })]) });
    await Promise.resolve();
  });
  return q;
}

function pickMode() {
  fireEvent.change(screen.getByLabelText('担当者'), { target: { value: 'pick' } });
}

function optionTexts(el: HTMLElement) {
  return Array.from((el as HTMLSelectElement).options).map(x => x.text);
}

describe('owner picker (admin)', () => {
  it('lets the admin search by name and choose a person; the queue is refetched with that owner ID', async () => {
    const q = await renderAdmin(ownersStub(okOwners()));
    pickMode();
    const list = await screen.findByLabelText('担当者を選ぶ');
    // 退職者は既定で隠れ、同名の 2 人は email で見分けられる
    expect(optionTexts(list)).toEqual([
      '架空 一郎(ichiro@example.invalid)', '架空 一郎(ichiro2@example.invalid)', 'サンプル 花子(ID 103)',
    ]);
    fireEvent.change(screen.getByLabelText('担当者を検索'), { target: { value: '花子' } });
    expect(Array.from(screen.getByLabelText<HTMLSelectElement>('担当者を選ぶ').options).map(x => x.value)).toEqual(['103']);
    fireEvent.change(screen.getByLabelText('担当者を選ぶ'), { target: { value: '103' } });
    await waitFor(() => { expect(q.calls.at(-1)?.filters.owner).toBe('103'); });
  });

  it('shows retired people only after the toggle, and picking the second same-named person passes that ID', async () => {
    const q = await renderAdmin(ownersStub(okOwners()));
    pickMode();
    await screen.findByLabelText('担当者を選ぶ');
    expect(screen.queryByText(/ダミー 退職/)).toBeNull();
    fireEvent.click(screen.getByLabelText('退職者も表示'));
    expect(screen.getByText(/ダミー 退職.*\[退職者\]/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText('担当者を選ぶ'), { target: { value: '102' } });
    await waitFor(() => { expect(q.calls.at(-1)?.filters.owner).toBe('102'); });
  });

  it('says so when there are zero owners', async () => {
    await renderAdmin(ownersStub(okOwners([])));
    pickMode();
    expect(await screen.findByText('該当する担当者がいません。')).toBeTruthy();
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
      expect(alert.textContent).toContain('担当者の一覧を取得できませんでした');
      const input = screen.getByLabelText('担当者ID(HubSpot owner ID)');
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
    expect(await screen.findByLabelText('担当者を選ぶ')).toBeTruthy();
    expect(n).toBe(2);
  });

  it('shows a loading note while the list is being fetched', async () => {
    const fetcher: OwnersFetch = () => new Promise<ApiResult<CrmOwnersResponse>>(() => undefined);
    await renderAdmin(fetcher);
    pickMode();
    expect(screen.getByText('担当者の一覧を読み込み中…')).toBeTruthy();
  });

  it('keeps an owner from the URL visible even when retired or missing from the list', async () => {
    await renderAdmin(ownersStub(okOwners()), '?view=queue&owner=104');
    const list = await screen.findByLabelText('担当者を選ぶ');
    expect((list as HTMLSelectElement).value).toBe('104');
    cleanup();
    await renderAdmin(ownersStub(okOwners()), '?view=queue&owner=999');
    const l2 = await screen.findByLabelText('担当者を選ぶ');
    expect((l2 as HTMLSelectElement).value).toBe('999');
    expect(screen.getByText(/ID 999.*一覧にありません/)).toBeTruthy();
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
    fireEvent.change(screen.getByLabelText('担当者'), { target: { value: 'unassigned' } });
    expect(q.calls.at(-1)?.filters.owner).toBe('unassigned');
    fireEvent.change(screen.getByLabelText('担当者'), { target: { value: 'me' } });
    expect(q.calls.at(-1)?.filters.owner).toBe('me');
    fireEvent.change(screen.getByLabelText('担当者'), { target: { value: '' } });
    expect(q.calls.at(-1)?.filters.owner).toBe('');
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it('never fetches the owner list for a BPO user', async () => {
    const fetcher = ownersStub(okOwners());
    const q = deferredFetcher();
    render(<CallQueueScreen fetcher={q.fetcher} ownersFetcher={fetcher} initialSearch="?view=queue" />);
    const first = q.calls[0];
    const resp = makeResponse(DEFAULT_FILTERS, [makeItem('1')]);
    await act(async () => {
      first?.resolve({ ok: true, data: { ...resp, scope: { ...resp.scope, role: 'bpo', owner: 'me' } } });
      await Promise.resolve();
    });
    expect(screen.queryByLabelText('担当者')).toBeNull();
    expect(fetcher).not.toHaveBeenCalled();
  });
});
