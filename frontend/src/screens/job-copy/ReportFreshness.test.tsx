// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { ReportFreshness } from './ReportFreshness';

afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const built = '2026-10-18T05:56:39Z';
const at = (hours: number) => vi.spyOn(Date, 'now').mockReturnValue(new Date(built).getTime() + hours * 3600_000);

it('数時間の遅れは作成中と伝える', () => {
  at(2);
  render(<ReportFreshness builtAt={built} stale refreshing />);
  expect(screen.getByText('2026/10/18 14:56 時点の集計です。新しい集計を作っています。')).toBeTruthy();
});
it('6時間以上古いままなら作れていないと伝える', () => {
  at(6);
  render(<ReportFreshness builtAt={built} stale refreshing />);
  expect(screen.getByText(/2026\/10\/18 14:56 時点の集計です。新しい集計がまだ終わっていないため、前の結果を表示しています。/)).toBeTruthy();
  expect(screen.queryByText(/作っています/)).toBeNull();
});
it('古くなければ何も出さない', () => {
  at(100);
  const { container } = render(<ReportFreshness builtAt={built} stale={false} refreshing />);
  expect(container.textContent).toBe('');
});
