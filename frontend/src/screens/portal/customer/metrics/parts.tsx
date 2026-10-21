import type { ReactNode } from 'react';
import { cx } from '../../ui/kit';
import { IconChevronDown } from '../../ui/icons';
import type { FunnelStage } from '../../types';
import { isSmall, pct, SMALL_DEN, STAGE_LABEL, type BreakdownRow, type MonthBar, type Rate, type Tally } from './calc';

const BAR = 'bg-pt-primary/70';

/** 小さな見出し付きの面。 */
export function Panel({ title, aside, children }: { title: string; aside?: ReactNode; children: ReactNode }) {
  return (
    <section className="rounded-lg border border-pt-line bg-pt-surface">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-pt-line px-4 py-2.5">
        <h2 className="text-[15px] font-semibold">{title}</h2>
        {aside && <div className="ml-auto">{aside}</div>}
      </div>
      <div className="p-4">{children}</div>
    </section>
  );
}

function Delta({ cur, prev, unit, inverse, kind }: { cur: number | null; prev: number | null; unit: string; inverse?: boolean | undefined; kind: 'count' | 'rate' }) {
  if (cur === null || prev === null) return null;
  const diff = kind === 'rate' ? Math.round((cur - prev) * 1000) / 10 : cur - prev;
  if (diff === 0) return <span className="text-xs text-pt-faint">変わらず</span>;
  const up = diff > 0;
  const good = inverse ? !up : up;
  return (
    <span className={cx('pt-num text-xs', good ? 'text-pt-ok' : 'text-pt-danger')}>
      {up ? '▲' : '▼'} {String(Math.abs(diff))}{unit}
    </span>
  );
}

/** 4 枚をひとつの面にまとめた横一列。人数を大きく出す。 */
export function KpiStrip({ children }: { children: ReactNode }) {
  return <div className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-pt-line bg-pt-line sm:grid-cols-4 lg:grid-cols-4">{children}</div>;
}

export function KpiCard({ label, big, num, den, basis, unit, cur, prev, kind, inverse, small, unavailable }: {
  label: string;
  /** 率の分母の説明（「応募から」「面接設定から」）。 */
  basis?: string | undefined;
  big: string;
  num?: number;
  den?: number;
  unit?: string;
  cur: number | null;
  prev: number | null;
  kind: 'count' | 'rate';
  inverse?: boolean;
  small?: boolean;
  unavailable?: boolean;
}) {
  return (
    <div className="bg-pt-surface px-3.5 py-3">
      <p className="text-xs text-pt-muted">{label}</p>
      <p className="mt-0.5 flex flex-wrap items-baseline gap-x-2 leading-8">
        <span className={cx('pt-num text-[22px] font-semibold', (small === true || unavailable === true) && 'text-pt-faint')}>{unavailable === true ? '—' : big}</span>
        {unavailable !== true && <Delta cur={cur} prev={prev} unit={kind === 'rate' ? 'pt' : (unit ?? '')} inverse={inverse} kind={kind} />}
      </p>
      <p className="pt-num min-h-4 text-xs leading-4 text-pt-faint">
        {unavailable === true
          ? 'この段階の選択肢なし'
          : (<>
            {num !== undefined && den !== undefined ? `${String(num)} / ${String(den)} 人${basis !== undefined ? `・${basis}` : ''}` : ''}
            {small === true && <span className="ml-1.5 whitespace-nowrap">人数が少ない</span>}
          </>)}
      </p>
    </div>
  );
}

export function Funnel({ t, missing, optionNames }: { t: Tally; missing: Set<FunnelStage>; /** 段ごとに数えている選択肢の名前（採用は入社予定日の列があればその旨）。 */ optionNames: Partial<Record<FunnelStage, string>> }) {
  const steps = [
    { stage: 'applied' as const, n: t.total },
    { stage: 'screening' as const, n: t.reached.screening },
    { stage: 'interview_set' as const, n: t.reached.interviewSet },
    { stage: 'interviewed' as const, n: t.reached.interviewed },
    { stage: 'offer' as const, n: t.reached.offer },
    { stage: 'hired' as const, n: t.reached.hired },
  ];
  return (
    <div>
      <ul className="space-y-2">
        {steps.map((s, i) => {
          const prev = i === 0 ? null : (steps[i - 1]?.n ?? 0);
          const none = missing.has(s.stage);
          return (
            <li key={s.stage} title={!none && prev !== null && prev > 0 ? `前の段階から ${pct(s.n / prev)}` : undefined} className={cx('grid grid-cols-[104px_1fr_auto] items-center gap-3 sm:grid-cols-[156px_1fr_auto]', none && 'opacity-60')}>
              <span className="min-w-0 text-[13px] text-pt-muted">
                {STAGE_LABEL[s.stage]}
                {!none && i === 0 && <span className="block text-xs leading-4 text-pt-faint">すべての応募</span>}
                {!none && i > 0 && optionNames[s.stage] !== undefined && (
                  <span className="block text-xs leading-4 text-pt-faint">
                    {(optionNames[s.stage] ?? '').split('・').map((n, j) => <span key={n} className="whitespace-nowrap">{j > 0 && '・'}{n}</span>)}
                    {s.stage !== 'hired' && <span className="whitespace-nowrap"> 以降</span>}
                  </span>
                )}
              </span>
              {none ? (
                <span className="text-xs text-pt-faint">—（この段階の選択肢がありません）</span>
              ) : (
                <span className="h-5 rounded-sm bg-pt-sunken">
                  <span
                    className={cx('block h-5 rounded-sm', s.stage === 'hired' ? 'bg-pt-ok' : BAR)}
                    style={{ width: `${String(t.total === 0 ? 0 : Math.max(s.n / t.total, s.n > 0 ? 0.01 : 0) * 100)}%` }}
                  />
                </span>
              )}
              <span className="pt-num min-w-24 text-right text-[13px] whitespace-nowrap">
                {none ? '—' : (<><b className="font-semibold">{s.n}</b><span className="text-pt-muted"> 人 {pct(t.total === 0 ? null : s.n / t.total)}</span></>)}
              </span>
            </li>
          );
        })}
      </ul>
      {t.total > 0 && t.total < SMALL_DEN && <p className="mt-2 text-xs text-pt-faint">応募が {String(SMALL_DEN)} 人より少ないため、割合は目安です。</p>}
      <p className="pt-num mt-3 flex flex-wrap gap-x-5 gap-y-1 border-t border-pt-line pt-2.5 text-[13px] text-pt-muted">
        <span>不採用 <b className="font-semibold text-pt-ink">{t.rejected}</b> 人（{pct(t.total === 0 ? null : t.rejected / t.total)}）</span>
        <span>辞退 <b className="font-semibold text-pt-ink">{t.declined}</b> 人（{pct(t.total === 0 ? null : t.declined / t.total)}）</span>
      </p>
    </div>
  );
}

function RateText({ r }: { r: Rate }) {
  const small = isSmall(r);
  return (
    <span className={cx(small && 'text-pt-faint')} title={small ? `${String(r.num)} / ${String(r.den)} 人（人数が少ない）` : `${String(r.num)} / ${String(r.den)} 人`}>
      {pct(r.value)}
      {small && <span className="ml-1 text-xs">少</span>}
    </span>
  );
}

export function BreakdownTable({ rows }: { rows: BreakdownRow[] }) {
  if (rows.length === 0) return <p className="text-[13px] text-pt-muted">この期間の応募はありません。</p>;
  const cols = 'sm:grid sm:grid-cols-[minmax(0,1.1fr)_minmax(0,1.6fr)_4rem_6rem_5rem]';
  return (
    <div>
      <div className={cx('hidden gap-3 border-b border-pt-line pb-1.5 text-xs text-pt-faint', cols)}>
        <span>値</span><span>構成比</span><span className="text-right">応募数</span><span className="text-right">面接移行率</span><span className="text-right">採用率</span>
      </div>
      <ul>
        {rows.map((r) => (
          <li key={r.value} className={cx('border-b border-pt-line py-2 last:border-b-0 sm:min-h-9 sm:items-center sm:gap-3 sm:py-1.5', cols)}>
            <div className="flex items-baseline justify-between gap-2 sm:block">
              <span className="truncate text-[13px] font-medium">{r.value}</span>
              <span className="pt-num text-[13px] sm:hidden">{r.count} 人</span>
            </div>
            <div className="mt-1 flex items-center gap-2 sm:mt-0">
              <span className="h-2 flex-1 rounded-sm bg-pt-sunken"><span className={cx('block h-2 rounded-sm', BAR)} style={{ width: `${String(r.share * 100)}%` }} /></span>
              <span className="pt-num w-11 text-right text-xs text-pt-muted">{pct(r.share)}</span>
            </div>
            <span className="pt-num hidden text-right text-[13px] sm:block">{r.count}</span>
            <span className="pt-num hidden text-right text-[13px] sm:block"><RateText r={r.interview} /></span>
            <span className="pt-num hidden text-right text-[13px] sm:block"><RateText r={r.hire} /></span>
            <p className="pt-num mt-0.5 flex gap-4 text-xs text-pt-muted sm:hidden"><span>面接移行 <RateText r={r.interview} /></span><span>採用 <RateText r={r.hire} /></span></p>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function MonthlyChart({ bars, range }: { bars: MonthBar[]; range: { from: number; to: number } | null }) {
  const max = Math.max(2, ...bars.map((b) => b.applied));
  const W = 420;
  const H = 168;
  const plot = 116;
  const slot = W / bars.length;
  const bw = 22;
  const y = (n: number): number => 20 + plot - (n / max) * plot;
  const ticks = [Math.round(max / 2), max];
  return (
    <div>
      <p className="mb-2 flex gap-4 text-xs text-pt-muted">
        <span className="inline-flex items-center gap-1.5"><span className={cx('size-2.5 rounded-sm', BAR)} />応募数</span>
        <span className="inline-flex items-center gap-1.5"><span className="size-2.5 rounded-sm bg-pt-ok" />採用数</span>
        <span className="ml-auto text-pt-faint">濃い月が選んだ期間</span>
      </p>
      <svg viewBox={`0 0 ${String(W)} ${String(H)}`} className="w-full" role="img" aria-label="直近 6 か月の応募数と採用数">
        {ticks.map((tk) => (
          <g key={tk}>
            <line x1="24" x2={W} y1={y(tk)} y2={y(tk)} className="stroke-pt-line" strokeDasharray="2 3" />
            <text x="0" y={y(tk) + 4} fontSize="12" className="pt-num fill-pt-faint">{tk}</text>
          </g>
        ))}
        <line x1="24" x2={W} y1={20 + plot} y2={20 + plot} className="stroke-pt-line-strong" />
        {bars.map((b, i) => {
          const cx0 = 24 + (slot - 6) * i + (slot - 6) / 2;
          const on = range === null || (b.from <= range.to && b.to >= range.from);
          return (
            <g key={b.label} opacity={on ? 1 : 0.4}>
              {b.applied > 0 && <rect x={cx0 - bw - 1} y={y(b.applied)} width={bw} height={20 + plot - y(b.applied)} rx="2" className="fill-pt-primary" opacity="0.7" />}
              {b.hired > 0 && <rect x={cx0 + 1} y={y(b.hired)} width={bw} height={20 + plot - y(b.hired)} rx="2" className="fill-pt-ok" />}
              {b.applied > 0 && <text x={cx0 - bw / 2 - 1} y={y(b.applied) - 4} textAnchor="middle" fontSize="12" className="pt-num fill-pt-ink">{b.applied}</text>}
              {b.hired > 0 && <text x={cx0 + bw / 2 + 1} y={y(b.hired) - 4} textAnchor="middle" fontSize="12" className="pt-num fill-pt-muted">{b.hired}</text>}
              <text x={cx0} y={20 + plot + 18} textAnchor="middle" fontSize="12" className="fill-pt-muted">{b.label}</text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

export function Chevron() {
  return <IconChevronDown className="ml-auto size-4 text-pt-faint transition-transform group-open:rotate-180" />;
}
