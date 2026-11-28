import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';

/**
 * A short label with an explanation that opens on click or keyboard (Enter / Space), not only on
 * hover. Used instead of title attributes on spans, which keyboard and touch users cannot reach.
 * 開いた説明は、外を押すか Esc で閉じる（下の部品に重なったまま残らないように）。
 */
export function InfoTip({ label, children, className = '', iconFirst = false }: { label: ReactNode; children: ReactNode; className?: string; /** 「ⓘ 数え方を見る」のように印を前に置く */ iconFirst?: boolean }) {
  const root = useRef<HTMLDetailsElement>(null);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!open) return;
    const close = () => { if (root.current) root.current.open = false; };
    const outside = (event: PointerEvent) => { if (event.target instanceof Node && !root.current?.contains(event.target)) close(); };
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') { close(); root.current?.querySelector('summary')?.focus(); } };
    document.addEventListener('pointerdown', outside); document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', outside); document.removeEventListener('keydown', escape); };
  }, [open]);
  return <details ref={root} className={`jc-infotip ${className}`.trim()} onToggle={event => { setOpen(event.currentTarget.open); }}>
    <summary>{iconFirst ? <><span aria-hidden="true">ⓘ </span>{label}</> : <>{label}<span aria-hidden="true"> ⓘ</span></>}</summary>
    <div className="jc-infotip-body">{children}</div>
  </details>;
}
