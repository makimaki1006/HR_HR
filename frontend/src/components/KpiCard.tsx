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
  /** Pre-formatted text shown instead of format(value) (unit is still appended). */
  display?: string;
  /** Text when there is no value (default "データなし"; pass "-" for a dash). */
  emptyText?: string;
}

const defaultFormat = (v: number): string => v.toLocaleString('ja-JP');

export function KpiCard({ label, value, unit, n, format = defaultFormat, note, display, emptyText }: KpiCardProps) {
  const shownValue = display ?? (value === null ? null : format(value));
  return (
    <section className="hw-kpi-card" aria-label={label}>
      <h3 className="hw-kpi-label">{label}</h3>
      <p className="hw-kpi-value" data-testid="kpi-value">
        {shownValue === null ? (
          <span className="hw-kpi-empty">{emptyText ?? 'データなし'}</span>
        ) : (
          <>
            <span>{shownValue}</span>
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
