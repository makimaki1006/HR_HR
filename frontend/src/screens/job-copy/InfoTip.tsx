import type { ReactNode } from 'react';

/**
 * A short label with an explanation that opens on click or keyboard (Enter / Space), not only on
 * hover. Used instead of title attributes on spans, which keyboard and touch users cannot reach.
 */
export function InfoTip({ label, children, className = '' }: { label: ReactNode; children: ReactNode; className?: string }) {
  return <details className={`jc-infotip ${className}`.trim()}>
    <summary>{label}<span aria-hidden="true"> ⓘ</span></summary>
    <div className="jc-infotip-body">{children}</div>
  </details>;
}
