// E2E hook: after init, window.__echarts_getInstanceByDom(el) returns the ECharts instance of a
// chart root (the element carrying data-testid + data-chart-ready="true").
import type { EChartsCoreOption, EChartsType } from 'echarts/core';
import { useEffect, useRef, useState } from 'react';

declare global {
  interface Window {
    __echarts_getInstanceByDom?: (dom: HTMLElement) => EChartsType | undefined;
    /** Resizes every mounted chart (for Playwright page.pdf() paths that skip beforeprint). */
    __echartsResizeAll?: () => void;
  }
}

/** All live chart instances, so print handling / __echartsResizeAll can resize them together. */
const liveInstances = new Set<EChartsType>();

function resizeAll(): void {
  liveInstances.forEach((i) => {
    i.resize();
  });
}

function withPrintMode(option: EChartsCoreOption, printMode: boolean | undefined): EChartsCoreOption {
  return printMode === true ? { ...option, animation: false } : option;
}

export interface EChartProps {
  option: EChartsCoreOption;
  /** Attached (with data-chart-ready="true") only while the chart's latest render has finished. */
  testId: string;
  height?: number | string;
  onReady?: (instance: EChartsType) => void;
  /** Renderer (default canvas). svg suits print / PDF output. Changing it re-creates the chart. */
  renderer?: 'canvas' | 'svg';
  /** Forces animation: false in the option (static output for print / PDF). */
  printMode?: boolean;
}

export function EChart({
  option,
  testId,
  height = 320,
  onReady,
  renderer = 'canvas',
  printMode,
}: EChartProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const instanceRef = useRef<EChartsType | null>(null);
  const optionRef = useRef(option);
  const onReadyRef = useRef(onReady);
  const printModeRef = useRef(printMode);
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    optionRef.current = option;
    onReadyRef.current = onReady;
    printModeRef.current = printMode;
  });

  // Init once per mount and per renderer; the chart is disposed on unmount / renderer change.
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    let cancelled = false;
    let instance: EChartsType | null = null;
    let observer: ResizeObserver | null = null;
    let printQuery: MediaQueryList | null = null;
    const resize = (): void => {
      instance?.resize();
    };
    setReady(false);
    setFailed(false);

    import('./echartsRegistry')
      .then(({ echarts }) => {
        if (cancelled) return;
        const created =
          renderer === 'svg'
            ? echarts.init(root, undefined, { renderer: 'svg' })
            : echarts.init(root);
        instance = created;
        instanceRef.current = created;
        liveInstances.add(created);
        created.setOption(withPrintMode(optionRef.current, printModeRef.current));
        if (typeof ResizeObserver !== 'undefined') {
          observer = new ResizeObserver(resize);
          observer.observe(root);
        }
        window.addEventListener('beforeprint', resize);
        if (typeof window.matchMedia === 'function') {
          printQuery = window.matchMedia('print');
          printQuery.addEventListener('change', resize);
        }
        window.__echarts_getInstanceByDom = (dom) => echarts.getInstanceByDom(dom);
        window.__echartsResizeAll = resizeAll;
        let announced = false;
        // "Ready" means the latest render finished (the first one, and again after each
        // option change), not just that the instance exists.
        const onFinished = (): void => {
          setReady(true);
          if (announced) return;
          announced = true;
          onReadyRef.current?.(created);
        };
        if (typeof created.on === 'function') created.on('finished', onFinished);
        else onFinished();
      })
      .catch(() => {
        // Chunk load failure (offline, stale deploy) or init error: show it instead of a blank box.
        if (!cancelled) setFailed(true);
      });

    return () => {
      cancelled = true;
      observer?.disconnect();
      window.removeEventListener('beforeprint', resize);
      printQuery?.removeEventListener('change', resize);
      if (instance) liveInstances.delete(instance);
      instance?.dispose();
      instanceRef.current = null;
    };
  }, [renderer]);

  // Later option changes replace the previous option entirely; the chart is "not ready" until
  // the re-render has finished.
  useEffect(() => {
    const instance = instanceRef.current;
    if (!instance) return;
    setReady(false);
    instance.setOption(withPrintMode(option, printMode), { notMerge: true });
  }, [option, printMode]);

  return (
    <div
      key={renderer}
      ref={rootRef}
      style={{ width: '100%', height }}
      {...(ready ? { 'data-testid': testId, 'data-chart-ready': 'true' } : {})}
      {...(failed ? { 'data-chart-error': 'true' } : {})}
    >
      {failed ? (
        <p role="alert" className="hw-chart-error">
          グラフを読み込めませんでした
        </p>
      ) : null}
    </div>
  );
}
