// @vitest-environment happy-dom
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { FAILURE_RETRIES, PREPARING_LIMIT_MS, recheckSelfOwner, setSelfOwnerForTest, useSelfOwner } from './selfOwner';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: (path: string) => api(path) as Promise<unknown> }));
afterEach(() => { cleanup(); vi.useRealTimers(); vi.resetAllMocks(); setSelfOwnerForTest(null); });

const ready = (owner = '11', notice: string | null = null) => ({ ok: true, data: { status: 'ready', owner, owner_field: 'consultant', notice, owners: [] } });
const preparing = { ok: true, data: { status: 'preparing' } };
const failed = { ok: true, data: { status: 'failed' } };
function Show({ label }: { label: string }) { const self = useSelfOwner(); return <p>{label}:{self === undefined ? '未確認' : self ?? 'なし'}</p>; }
const advance = (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });

it('ページの読み込みごとに1回だけ owner=me で確かめ、どの部品からも同じ本人を返す', async () => {
  setSelfOwnerForTest(undefined);
  api.mockResolvedValue(ready());
  render(<><Show label="今日" /><Show label="案件一覧" /></>);
  await waitFor(() => { expect(screen.getByText('今日:11')).toBeTruthy(); });
  expect(screen.getByText('案件一覧:11')).toBeTruthy();
  expect(api.mock.calls).toEqual([['/api/job-copy/consultant-scope?owner=me']]);
});

it('索引の作り直し中（preparing）が7回以上続いても「まだ分からない」のまま取り直し、ready になれば本人と判定する', async () => {
  vi.useFakeTimers();
  setSelfOwnerForTest(undefined);
  for (let i = 0; i < 10; i++) api.mockResolvedValueOnce(preparing);
  api.mockResolvedValue(ready());
  render(<Show label="今日" />);
  await advance(0);
  // 最初の1分は5秒おき。10回 preparing が続く間は「まだ分からない」のまま。
  for (let i = 0; i < 10; i++) {
    expect(screen.getByText('今日:未確認')).toBeTruthy();
    expect(api).toHaveBeenCalledTimes(i + 1);
    await advance(5_000);
  }
  expect(api).toHaveBeenCalledTimes(11);
  expect(screen.getByText('今日:11')).toBeTruthy();
  // 本人が分かったら、それ以上は呼ばない。
  await advance(60_000);
  expect(api).toHaveBeenCalledTimes(11);
});

it('preparing の間の取り直しは5〜15秒おきで、15分を過ぎたら自動では止め、別の読み込みが揃ったときにすぐ確かめ直す', async () => {
  vi.useFakeTimers();
  setSelfOwnerForTest(undefined);
  api.mockResolvedValue(preparing);
  render(<Show label="今日" />);
  await advance(0);
  expect(api).toHaveBeenCalledTimes(1);
  await advance(4_999);
  expect(api).toHaveBeenCalledTimes(1);
  await advance(1);
  expect(api).toHaveBeenCalledTimes(2);
  await advance(PREPARING_LIMIT_MS + 60_000);
  const calls = api.mock.calls.length;
  // 1分あたり4〜12回（5〜15秒おき）で、15分の間に止まる。
  expect(calls).toBeGreaterThan(60);
  expect(calls).toBeLessThan(15 * 12 + 2);
  await advance(10 * 60_000);
  expect(api).toHaveBeenCalledTimes(calls);
  expect(screen.getByText('今日:未確認')).toBeTruthy();
  api.mockResolvedValue(ready());
  recheckSelfOwner();
  await advance(0);
  expect(api).toHaveBeenCalledTimes(calls + 1);
  expect(screen.getByText('今日:11')).toBeTruthy();
});

it('failed と通信の失敗は、少し間をおいて数回取り直してから「自分なし」にする', async () => {
  vi.useFakeTimers();
  setSelfOwnerForTest(undefined);
  api.mockResolvedValueOnce(failed).mockRejectedValueOnce(new Error('network')).mockResolvedValue(failed);
  render(<Show label="今日" />);
  await advance(0);
  for (let i = 1; i <= FAILURE_RETRIES; i++) {
    expect(screen.getByText('今日:未確認')).toBeTruthy();
    await advance(5_000 * i);
  }
  expect(api).toHaveBeenCalledTimes(FAILURE_RETRIES + 1);
  expect(screen.getByText('今日:なし')).toBeTruthy();
});

it('failed のあとに ready が返れば本人と判定する', async () => {
  vi.useFakeTimers();
  setSelfOwnerForTest(undefined);
  api.mockResolvedValueOnce(failed).mockResolvedValue(ready('22'));
  render(<Show label="今日" />);
  await advance(0);
  expect(screen.getByText('今日:未確認')).toBeTruthy();
  await advance(5_000);
  expect(screen.getByText('今日:22')).toBeTruthy();
});

it('notice 付きの応答（本人の担当者が見つからない）はすぐ「自分なし」にして取り直さない', async () => {
  setSelfOwnerForTest(undefined);
  api.mockResolvedValue(ready('all', 'ログインした方の担当者情報が見つからないため、全員を表示しています。'));
  render(<Show label="今日" />);
  await waitFor(() => { expect(screen.getByText('今日:なし')).toBeTruthy(); });
  expect(api).toHaveBeenCalledTimes(1);
});
