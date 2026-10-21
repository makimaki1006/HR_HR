// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { JobCopyScreen } from './JobCopyScreen';
import { JobCopyHome } from './JobCopyHome';
import { demoHome } from './demoHome';
import { navigateJobCopy } from './navigation';

vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: (path: string, options: unknown) => path === '/api/job-copy/recent-changes' ? Promise.resolve({ ok: true, data: { status: 'ready', refresh_failed: false, report: { count: 0, listings: [] } } }) : api(path, options) as Promise<unknown> }));
beforeEach(() => {
  window.history.replaceState(null, '', '/app/job-copy?demo=1');
  localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true');
  api.mockResolvedValue({ ok: true, data: demoHome });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); localStorage.clear(); });
// happy-dom の履歴移動の代わりにブラウザが行う URL の切替と popstate を再現する。
function pop(url: string) {
  act(() => { window.history.replaceState(null, '', url); window.dispatchEvent(new PopStateEvent('popstate')); });
}
it('求人を push し、popstate で前の選択と進む先をキャッシュから復元する', () => {
  render(<JobCopyScreen />);
  const rows = Array.from(document.querySelectorAll<HTMLButtonElement>('.jc-job'));
  const first = rows[0]; const second = rows[1];
  if (!first || !second) throw new Error('合成求人がありません');
  const start = window.location.href;
  const push = vi.spyOn(window.history, 'pushState');
  fireEvent.click(second);
  const next = window.location.href;
  expect(push).toHaveBeenCalledTimes(1);
  expect(second.getAttribute('aria-pressed')).toBe('true');
  expect(new URL(next).searchParams.get('demo')).toBe('1');
  pop(start);
  expect(first.getAttribute('aria-pressed')).toBe('true');
  pop(next);
  expect(second.getAttribute('aria-pressed')).toBe('true');
  expect(push).toHaveBeenCalledTimes(1);
  expect(api).not.toHaveBeenCalled();
});
it('求人内容のポップアップを1回だけ積み、戻るで閉じて進むで開く', () => {
  render(<JobCopyScreen />);
  const start = window.location.href;
  const push = vi.spyOn(window.history, 'pushState');
  fireEvent.click(screen.getByRole('button', { name: '求人の内容を確認する' }));
  const opened = window.location.href;
  expect(new URL(opened).searchParams.get('popup')).toBe('listing');
  expect(screen.getByRole('dialog')).toBeTruthy();
  expect(push).toHaveBeenCalledTimes(1);
  pop(start);
  expect(screen.queryByRole('dialog')).toBeNull();
  pop(opened);
  expect(screen.getByRole('dialog')).toBeTruthy();
  expect(push).toHaveBeenCalledTimes(1);
});
it('同じURLを連続で積まず、初回表示でも積まない', () => {
  const push = vi.spyOn(window.history, 'pushState');
  render(<JobCopyScreen />);
  expect(push).not.toHaveBeenCalled();
  const row = document.querySelectorAll<HTMLButtonElement>('.jc-job')[1];
  if (!row) throw new Error('合成求人がありません');
  fireEvent.click(row); fireEvent.click(row);
  expect(push).toHaveBeenCalledTimes(1);
  act(() => { navigateJobCopy(new URL(window.location.href)); });
  expect(push).toHaveBeenCalledTimes(1);
});
it('分析の開閉と機能タブを各1回積み、戻ると前のタブと閉じた状態が戻る', () => {
  render(<JobCopyScreen />);
  const start = window.location.href;
  const push = vi.spyOn(window.history, 'pushState');
  fireEvent.click(within(screen.getByRole('region', { name: 'ほかの分析' })).getByRole('button', { name: 'ほかの分析を開く' }));
  const timeline = window.location.href;
  fireEvent.click(screen.getByRole('tab', { name: '本文・画像' }));
  const body = window.location.href;
  expect(push).toHaveBeenCalledTimes(2);
  pop(timeline);
  expect(screen.getByRole('tab', { name: 'タイムライン' }).getAttribute('aria-selected')).toBe('true');
  pop(body);
  expect(screen.getByRole('tab', { name: '本文・画像' }).getAttribute('aria-selected')).toBe('true');
  pop(start);
  expect(screen.queryByRole('dialog', { name: 'ほかの分析' })).toBeNull();
});
it('ホームのカード一覧を履歴から復元し、既存クエリを保つ', async () => {
  window.history.replaceState(null, '', '/app/job-copy?data=actual');
  const start = window.location.href;
  render(<JobCopyHome onOpenCompany={vi.fn()} onOpenListing={vi.fn()} onOpenDeal={vi.fn()} />);
  fireEvent.click(await screen.findByRole('button', { name: /応募がゼロのままの求人/ }));
  const opened = window.location.href;
  expect(new URL(opened).searchParams.get('data')).toBe('actual');
  expect(screen.getByRole('region', { name: '応募がゼロのままの求人の一覧' })).toBeTruthy();
  pop(start);
  expect(screen.queryByRole('region', { name: '応募がゼロのままの求人の一覧' })).toBeNull();
  pop(opened);
  expect(screen.getByRole('region', { name: '応募がゼロのままの求人の一覧' })).toBeTruthy();
  expect(api).toHaveBeenCalledTimes(1);
});
it('変更案件カードの開く・閉じるをpushし、履歴で一覧と既存クエリを復元する', async () => {
  window.history.replaceState(null, '', '/app/job-copy?data=actual&keep=1');
  const push = vi.spyOn(window.history, 'pushState');
  render(<JobCopyHome onOpenCompany={vi.fn()} onOpenListing={vi.fn()} onOpenDeal={vi.fn()} />);
  const button = await screen.findByRole('button', { name: /直近7日に変更/ });
  expect(push).not.toHaveBeenCalled();
  fireEvent.click(button);
  const opened = window.location.href;
  expect(new URL(opened).searchParams.get('card')).toBe('recent_changes');
  expect(new URL(opened).searchParams.get('data')).toBe('actual');
  expect(new URL(opened).searchParams.get('keep')).toBe('1');
  await screen.findByRole('heading', { name: '直近7日に変更（0件）' });
  expect(push).toHaveBeenCalledTimes(1);
  fireEvent.click(button);
  const closed = window.location.href;
  expect(new URL(closed).searchParams.has('card')).toBe(false);
  expect(screen.queryByRole('region', { name: '直近7日に変更の一覧' })).toBeNull();
  expect(push).toHaveBeenCalledTimes(2);
  pop(opened);
  expect(screen.getByRole('heading', { name: '直近7日に変更（0件）' })).toBeTruthy();
  expect(button.getAttribute('aria-pressed')).toBe('true');
  pop(closed);
  expect(screen.queryByRole('region', { name: '直近7日に変更の一覧' })).toBeNull();
  expect(button.getAttribute('aria-pressed')).toBe('false');
  expect(push).toHaveBeenCalledTimes(2);
  expect(api).toHaveBeenCalledTimes(1);
});
