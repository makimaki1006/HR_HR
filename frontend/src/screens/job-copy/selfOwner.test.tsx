// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { setSelfOwnerForTest, useSelfOwner } from './selfOwner';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: (path: string) => api(path) as Promise<unknown> }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

function Show({ label }: { label: string }) { const self = useSelfOwner(); return <p>{label}:{self === undefined ? '未確認' : self ?? 'なし'}</p>; }

it('ページの読み込みごとに1回だけ owner=me で確かめ、どの部品からも同じ本人を返す', async () => {
  setSelfOwnerForTest(undefined);
  api.mockResolvedValue({ ok: true, data: { status: 'ready', owner: '11', owner_field: 'consultant', notice: null, owners: [] } });
  render(<><Show label="今日" /><Show label="案件一覧" /></>);
  await waitFor(() => { expect(screen.getByText('今日:11')).toBeTruthy(); });
  expect(screen.getByText('案件一覧:11')).toBeTruthy();
  expect(api.mock.calls).toEqual([['/api/job-copy/consultant-scope?owner=me']]);
});

it('notice 付きの応答（本人の担当者が見つからない）と読み込めないときは「自分なし」', async () => {
  setSelfOwnerForTest(undefined);
  api.mockResolvedValue({ ok: true, data: { status: 'ready', owner: 'all', owner_field: 'consultant', notice: 'ログインした方の担当者情報が見つからないため、全員を表示しています。', owners: [] } });
  render(<Show label="今日" />);
  await waitFor(() => { expect(screen.getByText('今日:なし')).toBeTruthy(); });
  cleanup();
  setSelfOwnerForTest(undefined);
  api.mockRejectedValue(new Error('network'));
  render(<Show label="今日" />);
  await waitFor(() => { expect(screen.getByText('今日:なし')).toBeTruthy(); });
});
