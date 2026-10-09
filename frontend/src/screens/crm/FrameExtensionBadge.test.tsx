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
  it('入っていないときは何も出さず、入ると「拡張機能: 有効」と説明が出る', async () => {
    render(<FrameExtensionBadge />);
    expect(screen.queryByText('拡張機能: 有効')).toBeNull();
    await act(async () => { el.dataset.hrhrFrames = '1.0.0'; await Promise.resolve(); });
    const b = screen.getByText('拡張機能: 有効');
    expect(b.getAttribute('title')).toBe('埋め込みを禁止しているサイトも枠の中で表示できます（ブラックリストのサイトを除く）');
  });
});

describe('shouldShowFrameHint', () => {
  it('入っていないときだけ案内を出す', () => {
    expect(shouldShowFrameHint(false)).toBe(true);
    expect(shouldShowFrameHint(true)).toBe(false);
  });
});
