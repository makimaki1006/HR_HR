// @vitest-environment happy-dom
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { FAILURE_RETRIES, PREPARING_LIMIT_MS, SELF_OWNER_TIMEOUT_MS, recheckSelfOwner, setSelfOwnerForTest, useSelfOwner, waitingDelay } from './selfOwner';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: (path: string, options: unknown) => api(path, options) as Promise<unknown> }));
let hidden = false;
afterEach(() => { cleanup(); vi.useRealTimers(); vi.resetAllMocks(); vi.restoreAllMocks(); hidden = false; setSelfOwnerForTest(null); });

const ready = (owner = '11', notice: string | null = null) => ({ ok: true, data: { status: 'ready', owner, owner_field: 'consultant', notice, owners: [] } });
const preparing = { ok: true, data: { status: 'preparing' } };
const failed = { ok: true, data: { status: 'failed' } };
const networkError = { ok: false, error: new Error('network') };
function Show({ label }: { label: string }) { const self = useSelfOwner(); return <p>{label}:{self === undefined ? '未確認' : self ?? 'なし'}</p>; }
const advance = (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });
const start = () => { vi.useFakeTimers(); vi.spyOn(document, 'hidden', 'get').mockImplementation(() => hidden); setSelfOwnerForTest(undefined); };

it('ページの読み込みごとに1回だけ owner=me で確かめ、どの部品からも同じ本人を返す（60秒で打ち切る）', async () => {
  setSelfOwnerForTest(undefined);
  api.mockResolvedValue(ready());
  render(<><Show label="今日" /><Show label="案件一覧" /></>);
  await waitFor(() => { expect(screen.getByText('今日:11')).toBeTruthy(); });
  expect(screen.getByText('案件一覧:11')).toBeTruthy();
  expect(api.mock.calls).toEqual([['/api/job-copy/consultant-scope?owner=me', { timeoutMs: SELF_OWNER_TIMEOUT_MS }]]);
  expect(SELF_OWNER_TIMEOUT_MS).toBe(60_000);
});

it('preparing の間の間隔は最初の1分が5秒、2分目が10秒、それ以降は15秒', async () => {
  expect([0, 59_999, 60_000, 119_999, 120_000, 600_000].map(waitingDelay)).toEqual([5_000, 5_000, 10_000, 10_000, 15_000, 15_000]);
  start();
  api.mockResolvedValue(preparing);
  render(<Show label="今日" />);
  await advance(0);
  await advance(55_000); // 0〜55秒: 5秒おき
  expect(api).toHaveBeenCalledTimes(12);
  await advance(5_000); // 60秒: まだ5秒の間隔で予約された分
  expect(api).toHaveBeenCalledTimes(13);
  await advance(9_999);
  expect(api).toHaveBeenCalledTimes(13);
  await advance(1); // 70秒: 10秒の間隔
  expect(api).toHaveBeenCalledTimes(14);
  await advance(50_000); // 120秒
  expect(api).toHaveBeenCalledTimes(19);
  await advance(14_999);
  expect(api).toHaveBeenCalledTimes(19);
  await advance(1); // 135秒: 15秒の間隔
  expect(api).toHaveBeenCalledTimes(20);
});

it('preparing が7回以上続いても「まだ分からない」のまま取り直し、ready になれば本人と判定する', async () => {
  start();
  for (let i = 0; i < 10; i++) api.mockResolvedValueOnce(preparing);
  api.mockResolvedValue(ready());
  render(<Show label="今日" />);
  await advance(0);
  for (let i = 0; i < 10; i++) {
    expect(screen.getByText('今日:未確認')).toBeTruthy();
    expect(api).toHaveBeenCalledTimes(i + 1);
    await advance(5_000);
  }
  expect(screen.getByText('今日:11')).toBeTruthy();
  await advance(60_000);
  expect(api).toHaveBeenCalledTimes(11);
});

it('failed（サーバの作り直しの失敗）が長く続いても、あきらめずに取り直し、ready で本人になる', async () => {
  start();
  for (let i = 0; i < 30; i++) api.mockResolvedValueOnce(failed);
  api.mockResolvedValue(ready('22'));
  render(<Show label="今日" />);
  await advance(0);
  await advance(5 * 60_000);
  expect(api.mock.calls.length).toBeGreaterThan(30);
  expect(screen.getByText('今日:22')).toBeTruthy();
});

it('通信の失敗は3回取り直してから「自分なし」にし、そのあと確かめ直すと本人になる（失敗の回数も戻す）', async () => {
  start();
  api.mockResolvedValueOnce(networkError).mockRejectedValueOnce(new Error('network')).mockResolvedValue(networkError);
  render(<Show label="今日" />);
  await advance(0);
  for (let i = 1; i <= FAILURE_RETRIES; i++) {
    expect(screen.getByText('今日:未確認')).toBeTruthy();
    await advance(5_000 * i);
  }
  expect(api).toHaveBeenCalledTimes(FAILURE_RETRIES + 1);
  expect(screen.getByText('今日:なし')).toBeTruthy();
  await advance(60_000);
  expect(api).toHaveBeenCalledTimes(FAILURE_RETRIES + 1);
  // あきらめた null は確かめ直せる。失敗の回数も戻るので、もう一度3回まで取り直す。
  api.mockResolvedValueOnce(networkError).mockResolvedValue(ready('33'));
  recheckSelfOwner();
  await advance(0);
  expect(screen.getByText('今日:なし')).toBeTruthy();
  await advance(5_000);
  expect(screen.getByText('今日:33')).toBeTruthy();
});

it('notice で決めた null は確かめ直さない', async () => {
  start();
  api.mockResolvedValue(ready('all', 'ログインした方の担当者情報が見つからないため、全員を表示しています。'));
  render(<Show label="今日" />);
  await advance(0);
  expect(screen.getByText('今日:なし')).toBeTruthy();
  recheckSelfOwner();
  await advance(60_000);
  expect(api).toHaveBeenCalledTimes(1);
});

it('タイマー待ちの途中で確かめ直すと1回だけ問い合わせ、元のタイマーの時刻に余分な1回は出ない', async () => {
  start();
  api.mockResolvedValue(preparing);
  render(<Show label="今日" />);
  await advance(0);
  expect(api).toHaveBeenCalledTimes(1);
  await advance(2_000);
  recheckSelfOwner();
  await advance(0);
  expect(api).toHaveBeenCalledTimes(2);
  await advance(3_000); // 元のタイマーなら 5 秒の時刻
  expect(api).toHaveBeenCalledTimes(2);
  await advance(2_000); // 確かめ直しから 5 秒
  expect(api).toHaveBeenCalledTimes(3);
});

it('問い合わせ中の確かめ直しは二重に送らず、終わってまだ preparing ならすぐもう1回だけ問い合わせる', async () => {
  start();
  let release: ((value: unknown) => void) | undefined;
  api.mockImplementationOnce(() => new Promise(resolve => { release = resolve; })).mockResolvedValue(preparing);
  render(<Show label="今日" />);
  await advance(0);
  recheckSelfOwner(); recheckSelfOwner();
  await advance(0);
  expect(api).toHaveBeenCalledTimes(1);
  await act(async () => { release?.(preparing); await Promise.resolve(); });
  await advance(0);
  expect(api).toHaveBeenCalledTimes(2);
  await advance(4_999);
  expect(api).toHaveBeenCalledTimes(2);
  await advance(1);
  expect(api).toHaveBeenCalledTimes(3);
});

it('15分で自動の取り直しを止め、そのあとの確かめ直しがまた preparing でも自動の取り直しは再開しない', async () => {
  start();
  api.mockResolvedValue(preparing);
  render(<Show label="今日" />);
  await advance(0);
  await advance(PREPARING_LIMIT_MS + 60_000);
  const calls = api.mock.calls.length;
  await advance(10 * 60_000);
  expect(api).toHaveBeenCalledTimes(calls);
  recheckSelfOwner();
  await advance(0);
  expect(api).toHaveBeenCalledTimes(calls + 1);
  await advance(10 * 60_000);
  expect(api).toHaveBeenCalledTimes(calls + 1);
  api.mockResolvedValue(ready());
  recheckSelfOwner();
  await advance(0);
  expect(screen.getByText('今日:11')).toBeTruthy();
});

it('使う画面が無い間・タブが隠れている間は問い合わせず、戻ったら確かめ直す', async () => {
  start();
  api.mockResolvedValue(preparing);
  const first = render(<Show label="今日" />);
  await advance(0);
  expect(api).toHaveBeenCalledTimes(1);
  // 画面を離れた（listener が 0）。
  first.unmount();
  await advance(60_000);
  expect(api).toHaveBeenCalledTimes(1);
  render(<Show label="案件一覧" />);
  await advance(0);
  expect(api).toHaveBeenCalledTimes(2);
  // タブが隠れた。
  hidden = true;
  act(() => { document.dispatchEvent(new Event('visibilitychange')); });
  await advance(60_000);
  expect(api).toHaveBeenCalledTimes(2);
  // 表示に戻ったら、すぐ確かめ直す。
  api.mockResolvedValue(ready());
  hidden = false;
  act(() => { document.dispatchEvent(new Event('visibilitychange')); });
  await advance(0);
  expect(api).toHaveBeenCalledTimes(3);
  expect(screen.getByText('案件一覧:11')).toBeTruthy();
});
