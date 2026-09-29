// E2E hook: after init, window.__echarts_getInstanceByDom(el) returns the ECharts instance of a
// chart root (the element carrying data-testid + data-chart-ready="true").
import type { EChartsCoreOption, EChartsType } from 'echarts/core';
import { useEffect, useRef, useState } from 'react';

declare global {
  interface Window {
    __echarts_getInstanceByDom?: (dom: HTMLElement) => EChartsType | undefined;
  }
}

export interface EChartProps {
  option: EChartsCoreOption;
  /** Attached (with data-chart-ready="true") only once the chart is initialised. */
  testId: string;
  height?: number | string;
  onReady?: (instance: EChartsType) => void;
}

export function EChart({ option, testId, height = 320, onReady }: EChartProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const instanceRef = useRef<EChartsType | null>(null);
  const optionRef = useRef(option);
  const onReadyRef = useRef(onReady);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    optionRef.current = option;
    onReadyRef.current = onReady;
  });

  // Init once per mount; the chart is disposed on unmount.
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    let cancelled = false;
    let instance: EChartsType | null = null;
    let observer: ResizeObserver | null = null;
    const resize = (): void => {
      instance?.resize();
    };

    void import('./echartsRegistry').then(({ echarts }) => {
      if (cancelled) return;
      const created = echarts.init(root);
      instance = created;
      instanceRef.current = created;
      created.setOption(optionRef.current);
      if (typeof ResizeObserver !== 'undefined') {
        observer = new ResizeObserver(resize);
        observer.observe(root);
      }
      window.addEventListener('beforeprint', resize);
      window.__echarts_getInstanceByDom = (dom) => echarts.getInstanceByDom(dom);
      setReady(true);
      onReadyRef.current?.(created);
    });

    return () => {
      cancelled = true;
      observer?.disconnect();
      window.removeEventListener('beforeprint', resize);
      instance?.dispose();
      instanceRef.current = null;
    };
  }, []);

  // Later option changes replace the previous option entirely.
  useEffect(() => {
    instanceRef.current?.setOption(option, { notMerge: true });
  }, [option]);

  return (
    <div
      ref={rootRef}
      style={{ width: '100%', height }}
      {...(ready ? { 'data-testid': testId, 'data-chart-ready': 'true' } : {})}
    />
  );
}
