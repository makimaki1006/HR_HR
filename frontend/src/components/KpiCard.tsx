import type { ReactNode } from 'react';

export interface KpiCardProps {
  label: string;
  /** null renders "データなし" (never a fake 0). */
  value: number | null;
  unit: string;
  /** Sample size behind the value. Required so callers must decide; null = unknown. */
  n: number | null;
  /** Defaults to ja-JP digit grouping. */
  format?: (v: number) => string;
  note?: ReactNode;
}

const defaultFormat = (v: number): string => v.toLocaleString('ja-JP');

export function KpiCard({ label, value, unit, n, format = defaultFormat, note }: KpiCardProps) {
  return (
    <section className="hw-kpi-card" aria-label={label}>
      <h3 className="hw-kpi-label">{label}</h3>
      <p className="hw-kpi-value" data-testid="kpi-value">
        {value === null ? (
          <span className="hw-kpi-empty">データなし</span>
        ) : (
          <>
            <span>{format(value)}</span>
            <span className="hw-kpi-unit">{unit}</span>
          </>
        )}
      </p>
      <p className="hw-kpi-n" data-testid="kpi-n">
        {n === null ? 'n=不明' : `n=${n.toLocaleString('ja-JP')}`}
      </p>
      {note === undefined ? null : <div className="hw-kpi-note">{note}</div>}
    </section>
  );
}
