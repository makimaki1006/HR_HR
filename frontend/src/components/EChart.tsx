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
  /** Attached (with data-chart-ready="true") only once the chart is initialised. */
  testId: string;
  height?: number | string;
  onReady?: (instance: EChartsType) => void;
  /** Renderer, fixed at mount (default canvas). svg suits print / PDF output. */
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
  const rendererRef = useRef(renderer);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    optionRef.current = option;
    onReadyRef.current = onReady;
    printModeRef.current = printMode;
  });

  // Init once per mount; the chart is disposed on unmount.
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

    void import('./echartsRegistry').then(({ echarts }) => {
      if (cancelled) return;
      const created =
        rendererRef.current === 'svg'
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
      const markReady = (): void => {
        if (announced) return;
        announced = true;
        setReady(true);
        onReadyRef.current?.(created);
      };
      // "Ready" means the first render finished, not just that the instance exists.
      if (typeof created.on === 'function') created.on('finished', markReady);
      else markReady();
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
  }, []);

  // Later option changes replace the previous option entirely.
  useEffect(() => {
    instanceRef.current?.setOption(withPrintMode(option, printMode), { notMerge: true });
  }, [option, printMode]);

  return (
    <div
      ref={rootRef}
      style={{ width: '100%', height }}
      {...(ready ? { 'data-testid': testId, 'data-chart-ready': 'true' } : {})}
    />
  );
}
