// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { LinkView } from './CenterTabs';
import { makeLinkTab } from './centerLinks';

afterEach(() => { cleanup(); vi.restoreAllMocks(); try { window.localStorage.clear(); } catch { /* ignore */ } });

const tab = () => {
  const t = makeLinkTab('search', 'https://www.google.com/search?q=abc', '求人検索');
  if (t === null) throw new Error('tab');
  return t;
};
const backBtn = () => screen.getByRole<HTMLButtonElement>('button', { name: '戻る' });

describe('LinkView 戻る / 案内', () => {
  it('戻る is disabled until the frame navigates; click calls history.back once and decrements', () => {
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);
    render(<LinkView tab={tab()} />);
    const frame = screen.getByTestId('link-frame');
    expect(backBtn().disabled).toBe(true);
    fireEvent.load(frame); // 最初の読み込み = 0
    expect(backBtn().disabled).toBe(true);
    fireEvent.load(frame); // 枠の中で移動
    expect(backBtn().disabled).toBe(false);
    fireEvent.click(backBtn());
    expect(back).toHaveBeenCalledTimes(1);
    expect(backBtn().disabled).toBe(true);
    fireEvent.load(frame); // 戻ったことで起きる読み込みは移動として数えない
    expect(backBtn().disabled).toBe(true);
    fireEvent.click(backBtn());
    expect(back).toHaveBeenCalledTimes(1);
  });

  it('再読み込み resets the count', () => {
    render(<LinkView tab={tab()} />);
    fireEvent.load(screen.getByTestId('link-frame'));
    fireEvent.load(screen.getByTestId('link-frame'));
    fireEvent.load(screen.getByTestId('link-frame'));
    expect(backBtn().disabled).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: '再読み込み' }));
    expect(backBtn().disabled).toBe(true);
    fireEvent.load(screen.getByTestId('link-frame')); // 読み直しの最初の読み込み
    expect(backBtn().disabled).toBe(true);
  });

  it('hint shows on Google search tabs, dismiss persists', () => {
    const { unmount } = render(<LinkView tab={tab()} />);
    expect(screen.getByTestId('link-hint').textContent).toContain('⌘ / Ctrl を押しながらクリック');
    fireEvent.click(screen.getByRole('button', { name: '案内を閉じる' }));
    expect(screen.queryByTestId('link-hint')).toBeNull();
    expect(window.localStorage.getItem('crm.linkHintDismissed')).toBe('1');
    unmount();
    render(<LinkView tab={tab()} />);
    expect(screen.queryByTestId('link-hint')).toBeNull();
  });

  it('no hint on non-search tabs', () => {
    const t = makeLinkTab('x', 'https://www.example.com/', 'x');
    if (t === null) throw new Error('tab');
    render(<LinkView tab={t} />);
    expect(screen.queryByTestId('link-hint')).toBeNull();
  });
});
