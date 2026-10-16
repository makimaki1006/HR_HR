import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import type { StripItem } from './metricsModel';
import './metrics.css';

/** 1 行に収まっている帯の高さの上限 (これより高ければ折り返している) */
const ONE_LINE_PX = 96;

type Fit = 'full' | 'short' | 'wrap';

/**
 * 1 行に収まらないとき、まず比べる部分を短くし (#197 と同じ短い文)、それでも入らなければ 2 行に折り返す。
 * 幅が変わる・項目が変わると、全文から測り直す。
 */
function useFit(ref: React.RefObject<HTMLElement | null>, signature: string): Fit {
  const [width, setWidth] = useState(0);
  const [measured, setMeasured] = useState<{ key: string; fit: Fit }>({ key: '', fit: 'full' });
  const key = `${String(width)}|${signature}`;
  // 幅か項目が変わったら全文から測り直す
  const fit: Fit = measured.key === key ? measured.fit : 'full';
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(entries => {
      const w = Math.round(entries[0]?.contentRect.width ?? 0);
      setWidth(prev => (prev === w ? prev : w));
    });
    ro.observe(el);
    return () => { ro.disconnect(); };
  }, [ref]);
  useEffect(() => {
    if (fit === 'wrap' || typeof requestAnimationFrame === 'undefined') return;
    const id = requestAnimationFrame(() => {
      const el = ref.current;
      const head = el?.querySelector<HTMLElement>('.tm-head');
      const ul = el?.querySelector<HTMLElement>('.tm-items');
      if (!head || !ul) return;
      const over = head.getBoundingClientRect().height > ONE_LINE_PX || head.scrollWidth > head.clientWidth + 1 || ul.scrollWidth > ul.clientWidth + 1;
      if (over) setMeasured({ key, fit: fit === 'full' ? 'short' : 'wrap' });
    });
    return () => { cancelAnimationFrame(id); };
  }, [ref, key, fit]);
  return fit;
}

export function MetricsStrip({ label, testId, title, detail, asOf, asOfTitle, items, warn, collapsed, onToggle, onSettings, settingsOpen, note, children }: {
  label: string; testId: string; title: string; detail: string; asOf: string; asOfTitle: string;
  items: StripItem[] | null; warn: string | null; collapsed: boolean; onToggle: () => void;
  onSettings: (() => void) | null; settingsOpen: boolean; note: string | null; children?: ReactNode;
}) {
  const ref = useRef<HTMLElement>(null);
  const fit = useFit(ref, `${items?.map(i => `${i.key}:${i.value}:${i.note ?? ''}`).join('|') ?? ''}|${String(collapsed)}`);
  return <section ref={ref} className={`tm${fit !== 'full' ? ' tm-short' : ''}${fit === 'wrap' ? ' tm-wrap' : ''}`} aria-label={label} data-testid={testId}>
    <div className="tm-head">
      <strong title={detail}>{title}</strong>
      <span className="tm-asof" title={asOfTitle} data-testid="metrics-as-of">{asOf}</span>
      {!collapsed && items !== null && <ul className="tm-items">
        {items.map(it => <StripCell key={it.key} it={it} />)}
      </ul>}
      {warn !== null && <span className="tm-warn" role="status">{warn}</span>}
    </div>
    <div className="tm-actions">
      {onSettings && <button type="button" className="tm-toggle" aria-expanded={settingsOpen} aria-haspopup="dialog" onClick={onSettings} title="帯に出す項目を選ぶ">⚙ 項目</button>}
      <button type="button" className="tm-toggle" aria-expanded={!collapsed} onClick={onToggle}>{collapsed ? '開く' : '閉じる'}</button>
    </div>
    {!collapsed && note !== null && <p className="tm-note" role="status">{note}</p>}
    {children}
  </section>;
}

function StripCell({ it }: { it: StripItem }) {
  const stale = it.stale === true ? '（少し前の値）' : '';
  if (it.note) {
    return <li className="tm-item" data-testid={`metric-${it.key}`} title={`${it.label} ${it.note}`}>
      <span className="tm-label">{it.label}</span>
      <span className="tm-item-note">{it.note}</span>
    </li>;
  }
  return <li className="tm-item" data-testid={`metric-${it.key}`} title={`${it.label} ${it.value}（${it.compare}）${it.extra !== null ? `・${it.extra}` : ''}${stale}`}>
    <span className="tm-label">{it.label}</span>
    <span className="tm-value">{it.value}</span>
    <span className="tm-cmp" data-short={`（${it.compareShort}）`}>（{it.compare}）</span>
    {it.extra !== null && <span className="tm-sub" data-testid={it.key === 'outbound' && it.extra.startsWith('今日') ? 'metric-today' : undefined}>{it.extra}</span>}
    {stale && <span className="tm-sub tm-stale">少し前の値</span>}
  </li>;
}
