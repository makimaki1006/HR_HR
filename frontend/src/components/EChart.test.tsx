// @vitest-environment happy-dom
import { act, cleanup, render, waitFor } from '@testing-library/react';
import type { EChartsType } from 'echarts/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const fake = vi.hoisted(() => {
  const instance = {
    setOption: vi.fn(),
    resize: vi.fn(),
    dispose: vi.fn(),
    // 'finished' fires asynchronously after the first render, like the real thing.
    on: vi.fn((event: string, cb: () => void) => {
      if (event === 'finished') setTimeout(cb, 0);
    }),
  };
  return {
    instance,
    failImport: false,
    init: vi.fn((..._args: unknown[]) => instance),
    getInstanceByDom: vi.fn(() => instance),
  };
});

vi.mock('./echartsRegistry', () => ({
  // A throwing getter makes the dynamic import().then(({ echarts }) => ...) path fail like a
  // chunk that cannot be loaded.
  get echarts() {
    if (fake.failImport) throw new Error('Failed to fetch dynamically imported module');
    return { init: fake.init, getInstanceByDom: fake.getInstanceByDom };
  },
}));

import { EChart } from './EChart';

let resizeCallbacks: (() => void)[] = [];
let observerDisconnects = 0;

beforeEach(() => {
  resizeCallbacks = [];
  observerDisconnects = 0;
  vi.stubGlobal(
    'ResizeObserver',
    class {
      constructor(cb: () => void) {
        resizeCallbacks.push(cb);
      }
      observe(): void {
        // no-op: resize is driven manually through resizeCallbacks
      }
      disconnect(): void {
        observerDisconnects += 1;
      }
    },
  );
});

afterEach(() => {
  fake.failImport = false;
  cleanup();
  vi.clearAllMocks();
  vi.unstubAllGlobals();
  delete window.__echarts_getInstanceByDom;
});

const optionA = { xAxis: { type: 'category', data: ['a'] }, yAxis: {}, series: [{ type: 'bar', data: [1] }] };

describe('EChart', () => {
  it('has no testid before init and gets data-testid + data-chart-ready after', async () => {
    const { container, findByTestId } = render(<EChart option={optionA} testId="chart-x" height={200} />);
    const root = container.firstElementChild as HTMLElement;
    expect(root.getAttribute('data-testid')).toBeNull();
    expect(root.style.height).toBe('200px');

    const ready = await findByTestId('chart-x');
    expect(ready).toBe(root);
    expect(ready.getAttribute('data-chart-ready')).toBe('true');
    expect(fake.init).toHaveBeenCalledWith(root);
    expect(fake.instance.on).toHaveBeenCalledWith('finished', expect.any(Function));
    expect(fake.instance.setOption).toHaveBeenCalledWith(optionA);
  });

  it('exposes window.__echarts_getInstanceByDom and calls onReady with the instance', async () => {
    const onReady = vi.fn<(i: EChartsType) => void>();
    const { findByTestId } = render(<EChart option={optionA} testId="c" onReady={onReady} />);
    const root = await findByTestId('c');
    expect(onReady).toHaveBeenCalledTimes(1);
    expect(onReady).toHaveBeenCalledWith(fake.instance);
    expect(window.__echarts_getInstanceByDom?.(root)).toBe(fake.instance);
    expect(fake.getInstanceByDom).toHaveBeenCalledWith(root);
  });

  it('resizes on ResizeObserver and on beforeprint', async () => {
    const { findByTestId } = render(<EChart option={optionA} testId="c" />);
    await findByTestId('c');
    expect(resizeCallbacks).toHaveLength(1);
    resizeCallbacks[0]?.();
    expect(fake.instance.resize).toHaveBeenCalledTimes(1);
    window.dispatchEvent(new Event('beforeprint'));
    expect(fake.instance.resize).toHaveBeenCalledTimes(2);
  });

  it('replaces the option (notMerge) when the option prop changes', async () => {
    const { findByTestId, rerender } = render(<EChart option={optionA} testId="c" />);
    await findByTestId('c');
    const optionB = { ...optionA, series: [{ type: 'line', data: [2] }] };
    rerender(<EChart option={optionB} testId="c" />);
    await waitFor(() => {
      expect(fake.instance.setOption).toHaveBeenCalledWith(optionB, { notMerge: true });
    });
    expect(fake.init).toHaveBeenCalledTimes(1);
  });

  it('disposes, disconnects the observer and stops listening on unmount', async () => {
    const { findByTestId, unmount } = render(<EChart option={optionA} testId="c" />);
    await findByTestId('c');
    unmount();
    expect(fake.instance.dispose).toHaveBeenCalledTimes(1);
    expect(observerDisconnects).toBe(1);
    window.dispatchEvent(new Event('beforeprint'));
    expect(fake.instance.resize).not.toHaveBeenCalled();
  });

  it('does not init when unmounted before the dynamic import resolves', () => {
    const { unmount } = render(<EChart option={optionA} testId="c" />);
    unmount();
    return new Promise<void>((resolve) => {
      setTimeout(() => {
        expect(fake.init).not.toHaveBeenCalled();
        expect(fake.instance.dispose).not.toHaveBeenCalled();
        resolve();
      }, 50);
    });
  });

  it('sets data-chart-ready only after the finished event, and announces onReady once', async () => {
    let finish: (() => void) | undefined;
    fake.instance.on.mockImplementationOnce((_e: string, cb: () => void) => {
      finish = cb;
    });
    const onReady = vi.fn();
    const { container } = render(<EChart option={optionA} testId="c" onReady={onReady} />);
    const root = container.firstElementChild as HTMLElement;
    await waitFor(() => {
      expect(finish).toBeDefined();
    });
    expect(root.getAttribute('data-chart-ready')).toBeNull();
    expect(onReady).not.toHaveBeenCalled();
    act(() => {
      finish?.();
      finish?.();
    });
    expect(root.getAttribute('data-chart-ready')).toBe('true');
    expect(onReady).toHaveBeenCalledTimes(1);
  });

  it('inits with the svg renderer only when asked, canvas (no extra args) otherwise', async () => {
    const { findByTestId } = render(<EChart option={optionA} testId="svg" renderer="svg" />);
    const root = await findByTestId('svg');
    expect(fake.init).toHaveBeenCalledWith(root, undefined, { renderer: 'svg' });
  });

  it('printMode forces animation: false without mutating the caller option', async () => {
    const opt = { ...optionA, animation: true };
    const { findByTestId, rerender } = render(<EChart option={opt} testId="c" printMode />);
    await findByTestId('c');
    expect(fake.instance.setOption).toHaveBeenCalledWith({ ...opt, animation: false });
    expect(opt.animation).toBe(true);
    rerender(<EChart option={opt} testId="c" printMode={false} />);
    await waitFor(() => {
      expect(fake.instance.setOption).toHaveBeenLastCalledWith(opt, { notMerge: true });
    });
  });

  it('resizes on matchMedia(print) change and exposes __echartsResizeAll for every instance', async () => {
    const listeners: (() => void)[] = [];
    vi.stubGlobal(
      'matchMedia',
      vi.fn(() => ({
        matches: false,
        addEventListener: (_t: string, cb: () => void) => listeners.push(cb),
        removeEventListener: vi.fn(),
      })),
    );
    window.matchMedia = globalThis.matchMedia;
    const first = render(<EChart option={optionA} testId="c1" />);
    await first.findByTestId('c1');
    const second = render(<EChart option={optionA} testId="c2" />);
    await second.findByTestId('c2');
    await waitFor(() => {
      expect(listeners).toHaveLength(2);
    });
    listeners[0]?.();
    expect(fake.instance.resize).toHaveBeenCalledTimes(1);
    fake.instance.resize.mockClear();
    window.__echartsResizeAll?.();
    expect(fake.instance.resize).toHaveBeenCalledTimes(1);
  });

  it('re-creates the chart when the renderer changes after mount', async () => {
    const { findByTestId, rerender, container } = render(
      <EChart option={optionA} testId="c" renderer="canvas" />,
    );
    await findByTestId('c');
    expect(fake.init).toHaveBeenCalledTimes(1);
    rerender(<EChart option={optionA} testId="c" renderer="svg" />);
    await waitFor(() => {
      expect(fake.init).toHaveBeenCalledTimes(2);
    });
    const root = container.firstElementChild as HTMLElement;
    expect(fake.init).toHaveBeenLastCalledWith(root, undefined, { renderer: 'svg' });
    expect(fake.instance.dispose).toHaveBeenCalledTimes(1);
    // The new instance becomes ready on its own finished event.
    await findByTestId('c');
  });

  it('shows data-chart-error and a message when the chart module cannot be loaded', async () => {
    fake.failImport = true;
    const { container, findByText } = render(<EChart option={optionA} testId="c" />);
    expect(await findByText('グラフを読み込めませんでした')).toBeTruthy();
    const root = container.firstElementChild as HTMLElement;
    expect(root.getAttribute('data-chart-error')).toBe('true');
    expect(root.getAttribute('data-chart-ready')).toBeNull();
    expect(fake.init).not.toHaveBeenCalled();
  });

  it('shows data-chart-error when init itself throws', async () => {
    fake.init.mockImplementationOnce(() => {
      throw new Error('init failed');
    });
    const { container, findByText } = render(<EChart option={optionA} testId="c" />);
    expect(await findByText('グラフを読み込めませんでした')).toBeTruthy();
    expect((container.firstElementChild as HTMLElement).getAttribute('data-chart-error')).toBe('true');
  });

  it('after an option change data-chart-ready goes false until the next finished event', async () => {
    const finishers: (() => void)[] = [];
    fake.instance.on.mockImplementationOnce((_e: string, cb: () => void) => {
      finishers.push(cb);
    });
    const { container, rerender } = render(<EChart option={optionA} testId="c" />);
    const root = container.firstElementChild as HTMLElement;
    await waitFor(() => {
      expect(finishers).toHaveLength(1);
    });
    act(() => {
      finishers[0]?.();
    });
    expect(root.getAttribute('data-chart-ready')).toBe('true');
    const optionB = { ...optionA, series: [{ type: 'line', data: [2, 3] }] };
    rerender(<EChart option={optionB} testId="c" />);
    await waitFor(() => {
      expect(fake.instance.setOption).toHaveBeenCalledWith(optionB, { notMerge: true });
    });
    expect(root.getAttribute('data-chart-ready')).toBeNull();
    expect(root.getAttribute('data-testid')).toBeNull();
    act(() => {
      finishers[0]?.();
    });
    expect(root.getAttribute('data-chart-ready')).toBe('true');
  });
});
