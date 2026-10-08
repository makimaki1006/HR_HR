// @vitest-environment happy-dom
import { Suspense, lazy } from 'react';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ScreenLoadBoundary } from './ScreenLoadBoundary';

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe('ScreenLoadBoundary', () => {
  it('a screen file that fails to load shows a plain message and a reload button instead of a blank page', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const Broken = lazy(() => Promise.reject(new Error('Failed to fetch dynamically imported module: /static/app/assets/CallQueueScreen-x.js')));
    const onReload = vi.fn();
    render(<ScreenLoadBoundary onReload={onReload}><Suspense fallback={<p>読み込み中…</p>}><Broken /></Suspense></ScreenLoadBoundary>);
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('画面を読み込めませんでした。通信の状態を確かめて、再読み込みしてください。');
    // 開発者向けの文言 (ファイル名・英語のエラー) は画面に出さない
    expect(alert.textContent).not.toMatch(/Failed to fetch|\.js|import/);
    fireEvent.click(screen.getByRole('button', { name: '再読み込み' }));
    expect(onReload).toHaveBeenCalledTimes(1);
  });

  it('a screen that loads renders normally', async () => {
    const Ok = lazy(() => Promise.resolve({ default: () => <p>架電</p> }));
    render(<ScreenLoadBoundary><Suspense fallback={<p>読み込み中…</p>}><Ok /></Suspense></ScreenLoadBoundary>);
    expect(await screen.findByText('架電')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
