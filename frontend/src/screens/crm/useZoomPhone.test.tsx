// @vitest-environment happy-dom
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ZOOM_EMBED_ORIGIN } from './smartEmbed';
import { useZoomPhone } from './useZoomPhone';
import type { ZoomOptions } from './useZoomPhone';

afterEach(() => { cleanup(); vi.useRealTimers(); });

/** iframe の代わり (contentWindow だけを持つ)。ref に入れて使う */
function fakeFrame() {
  const postMessage = vi.fn();
  const win = { postMessage } as unknown as Window;
  return { frame: { contentWindow: win } as unknown as HTMLIFrameElement, win, postMessage };
}
function send(win: Window, type: string, callId: string) {
  act(() => {
    window.dispatchEvent(new MessageEvent('message', {
      origin: ZOOM_EMBED_ORIGIN, source: win as unknown as MessageEventSource,
      data: { type, data: { callId, direction: 'outbound', callee: { phoneNumber: '+81312345678' } } },
    }));
  });
}

function setup(opts: ZoomOptions = {}) {
  const hook = renderHook(({ enabled }: { enabled: boolean }) => useZoomPhone(enabled, { now: () => 1_000, ...opts }), { initialProps: { enabled: true } });
  const f = fakeFrame();
  hook.result.current.iframeRef.current = f.frame;
  return { ...hook, ...f };
}

describe('useZoomPhone', () => {
  it('returns the same zoom object while nothing changed, and a new one when the state changes', () => {
    const { result, rerender } = setup({ loadTimeoutMs: 60_000 });
    const before = result.current.zoom;
    rerender({ enabled: true });
    rerender({ enabled: true });
    expect(result.current.zoom).toBe(before);
    act(() => { result.current.zoom.onLoad(); });
    expect(result.current.zoom).not.toBe(before);
    expect(result.current.zoom.embed).toBe('loaded');
  });
  it('does not send a dial while the frame is loading or has timed out; sends once loaded', () => {
    vi.useFakeTimers();
    const { result, postMessage } = setup({ loadTimeoutMs: 100 });
    expect(result.current.zoom.embed).toBe('loading');
    let r = '';
    act(() => { r = result.current.zoom.dial('03-1234-5678'); });
    expect(r).toBe('embed_loading');
    expect(result.current.zoom.pending).toBeNull();
    act(() => { vi.advanceTimersByTime(100); });
    expect(result.current.zoom.embed).toBe('timeout');
    act(() => { r = result.current.zoom.dial('03-1234-5678'); });
    expect(r).toBe('embed_unavailable');
    expect(result.current.zoom.pending).toBeNull();
    expect(postMessage).not.toHaveBeenCalled();
    act(() => { result.current.zoom.onLoad(); });
    expect(result.current.zoom.embed).toBe('loaded');
    act(() => { r = result.current.zoom.dial('03-1234-5678'); });
    expect(r).toBe('sent');
    expect(postMessage).toHaveBeenCalledTimes(1);
  });

  it('turning it off during a call (the frame goes away) drops the call, the dial request and the loaded state', () => {
    vi.useFakeTimers();
    const { result, rerender, win } = setup({ loadTimeoutMs: 100 });
    act(() => { result.current.zoom.onLoad(); });
    act(() => { result.current.zoom.dial('03-1234-5678'); });
    expect(result.current.zoom.pending).not.toBeNull();
    send(win, 'zp-call-ringing-event', 'c1');
    send(win, 'zp-call-connected-event', 'c1');
    expect(result.current.zoom.call.phase).toBe('connected');
    rerender({ enabled: false });
    expect(result.current.zoom.embed).toBe('disabled');
    rerender({ enabled: true });
    // 新しい枠として読み込みから数え直す。前の通話は残さないので「通話中」で発信を止めない
    expect(result.current.zoom.call.phase).toBe('idle');
    expect(result.current.zoom.call.callId).toBeNull();
    expect(result.current.zoom.pending).toBeNull();
    expect(result.current.zoom.stalled).toBe(false);
    expect(result.current.zoom.embed).toBe('loading');
    // 新しい枠が読み込まれなければ、読み込めない案内に切り替わる
    act(() => { vi.advanceTimersByTime(100); });
    expect(result.current.zoom.embed).toBe('timeout');
    const f2 = fakeFrame();
    result.current.iframeRef.current = f2.frame;
    act(() => { result.current.zoom.onLoad(); });
    let r = '';
    act(() => { r = result.current.zoom.dial('03-1234-5678'); });
    expect(r).toBe('sent');
    expect(f2.postMessage).toHaveBeenCalledTimes(1);
  });
});
