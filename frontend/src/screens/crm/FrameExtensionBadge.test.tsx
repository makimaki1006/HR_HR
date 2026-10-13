// @vitest-environment happy-dom
import { act, cleanup, render, renderHook, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { FrameExtensionBadge } from './FrameExtensionBadge';
import { shouldShowFrameHint, useFrameExtension } from './useFrameExtension';

const el = document.documentElement;
afterEach(() => { cleanup(); el.removeAttribute('data-hrhr-frames'); });

describe('useFrameExtension', () => {
  it('属性が無ければ false / null、後から付くと true とバージョン文字列になる', async () => {
    const { result } = renderHook(() => useFrameExtension());
    expect(result.current).toEqual({ installed: false, version: null });
    await act(async () => { el.dataset.hrhrFrames = '1.2.3'; await Promise.resolve(); });
    expect(result.current).toEqual({ installed: true, version: '1.2.3' });
    await act(async () => { el.removeAttribute('data-hrhr-frames'); await Promise.resolve(); });
    expect(result.current.installed).toBe(false);
  });

  it('最初から属性があれば最初の描画で true', () => {
    el.dataset.hrhrFrames = '0.1.0';
    const { result } = renderHook(() => useFrameExtension());
    expect(result.current).toEqual({ installed: true, version: '0.1.0' });
  });

  it('アンマウントで監視をやめる (disconnect される)', () => {
    let disconnected = 0;
    const Orig = globalThis.MutationObserver;
    globalThis.MutationObserver = class extends Orig {
      override disconnect() { disconnected += 1; super.disconnect(); }
    };
    try {
      const { unmount } = renderHook(() => useFrameExtension());
      unmount();
      expect(disconnected).toBe(1);
    } finally { globalThis.MutationObserver = Orig; }
  });
});

describe('FrameExtensionBadge', () => {
  it('入っていないときだけ「枠で開けないサイトがあります」と理由が出て、入ると消える (正常なときは出さない)', async () => {
    render(<FrameExtensionBadge />);
    const b = screen.getByText('枠で開けないサイトがあります');
    expect(b.getAttribute('title')).toBe('拡張機能を入れると枠の中で開けるサイトが増えます');
    await act(async () => { el.dataset.hrhrFrames = '1.0.0'; await Promise.resolve(); });
    expect(screen.queryByText('枠で開けないサイトがあります')).toBeNull();
    expect(screen.queryByText('枠で開けないサイトがあります')).toBeNull();
  });
});

describe('shouldShowFrameHint', () => {
  it('入っていないときだけ案内を出す', () => {
    expect(shouldShowFrameHint(false)).toBe(true);
    expect(shouldShowFrameHint(true)).toBe(false);
  });
});
