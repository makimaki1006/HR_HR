import { useState, type ReactNode } from 'react';

export interface PopularityValue { value: number | null; n?: number; nLabel?: string; popular?: number }
export interface PopularityChartData {
  unit: '%' | 'pt' | '件' | '万円';
  series: readonly string[];
  note?: string;
  rows: readonly { label: string; values: readonly PopularityValue[] }[];
}

/** 図の数値・位置・尺度は同じ表示精度で計算する。元表の値は変更しない。 */
export const popularityDisplayValue = (value: number | null): number | null => value !== null && Number.isFinite(value) ? Number(value.toFixed(1)) : null;

/** 尺度を変えても値の順序・符号を変えない。百分率の通常表示は0始点。 */
export function popularityDomain(data: PopularityChartData, zoom: boolean): [number, number] {
  const values = data.rows.flatMap(r => r.values.map(v => popularityDisplayValue(v.value))).filter((v): v is number => v !== null && Number.isFinite(v));
  if (!values.length) return [0, data.unit === '%' ? 100 : 1];
  const lo = Math.min(...values), hi = Math.max(...values);
  if (zoom) {
    const pad = Math.max((hi - lo) * 0.15, data.unit === '%' ? 1 : 0.1);
    return [data.unit === '%' ? Math.max(0, lo - pad) : lo - pad, data.unit === '%' ? Math.min(100, hi + pad) : hi + pad];
  }
  if (data.unit === '%') return [0, 100];
  if (data.unit === 'pt') { const max = Math.max(1, Math.abs(lo), Math.abs(hi)) * 1.1; return [-max, max]; }
  return [Math.min(0, lo), Math.max(1, hi) * 1.1];
}
export const popularityPosition = (value: number, domain: [number, number]): number => (value - domain[0]) / (domain[1] - domain[0]) * 100;
const fmt = (v: number): string => Number(v.toFixed(1)).toString();

export function PopularityChart({ data: source }: { data: PopularityChartData }) {
  const data: PopularityChartData = { ...source, rows: source.rows.map(r => ({ ...r, values: r.values.map(v => ({ ...v, value: popularityDisplayValue(v.value) })) })) };
  const [zoom, setZoom] = useState(false);
  const domain = popularityDomain(data, zoom);
  const values = data.rows.flatMap(r => r.values.map(v => popularityDisplayValue(v.value))).filter((v): v is number => v !== null && Number.isFinite(v));
  const equal = values.length > 1 && values.every(v => v === values[0]);
  const origin = popularityPosition(Math.max(domain[0], Math.min(domain[1], 0)), domain);
  return <figure className="cmp-pop-chart" aria-label={`${data.series.join('・')}の比較`}>
    <figcaption><span>{data.unit === '%' ? '割合' : data.unit === 'pt' ? '割合の差' : data.unit === '万円' ? '月給下限の中央値' : '件数'}の比較（{data.unit}）</span>
      {values.length > 1 && <button type="button" className="cmp-pop-zoom" aria-pressed={zoom} onClick={() => { setZoom(!zoom); }}>{zoom ? '全体を見る' : '差を拡大'}</button>}
    </figcaption>
    <p className="cmp-note">{zoom ? `点で差を拡大しています。表示範囲 ${fmt(domain[0])}〜${fmt(domain[1])}${data.unit}（0始点とは限りません）。` : data.unit === '%' ? '0〜100%の共通目盛り。棒は各条件の割合を示します。' : `表示範囲 ${fmt(domain[0])}〜${fmt(domain[1])}${data.unit}。`}{equal ? ' 表示値は同じです。' : values.length > 1 ? ` 最大差 ${fmt(Math.max(...values) - Math.min(...values))}${data.unit === '%' ? 'ポイント' : data.unit}。` : ''}</p>
    {data.series.length > 1 && <ul className="cmp-pop-legend">{data.series.map((s, i) => <li key={s}><i style={{ background: ['#006666', '#4472c4', '#927138'][i % 3] }} />{s}</li>)}</ul>}
    <div className="cmp-pop-axis" aria-hidden="true">{[domain[0], (domain[0] + domain[1]) / 2, domain[1]].map((v, i) => <span key={i}>{fmt(v)}{data.unit}</span>)}</div>
    {data.rows.map((r, index) => <div className="cmp-pop-chart-row" key={`${r.label}-${String(index)}`}>
      <span className="cmp-pop-chart-label">{r.label}</span>
      <div className="cmp-pop-plot">
        {r.values.map((v, i) => { const valid = v.value !== null && Number.isFinite(v.value); const position = valid ? popularityPosition((v.value ?? 0), domain) : 0; const small = v.n !== undefined && v.n < 10; return <div className="cmp-pop-series-row" key={i}>
          <div className="cmp-pop-lane">
            {valid && <span className={zoom ? 'cmp-pop-dot' : 'cmp-pop-bar'} data-value={v.value} style={{ left: `${String(zoom ? position : Math.min(origin, position))}%`, width: zoom ? undefined : `${String(Math.abs(position - origin))}%`, background: ['#006666', '#4472c4', '#927138'][i % 3], opacity: small ? 0.45 : 1 }} />}
          </div>
          <span className="cmp-pop-value">{valid ? `${fmt((v.value ?? 0))}${data.unit}` : '—'}{v.n !== undefined && <small>{v.popular !== undefined ? `人気・超人気 ${String(v.popular)}／${String(v.n)}件` : `${v.nLabel ?? '求人数'} ${String(v.n)}件`}{small ? '（少数）' : ''}</small>}</span>
        </div>; })}
      </div>
    </div>)}
    {!values.length && <p className="cmp-note">比較できる値がありません。</p>}
    {data.note && <p className="cmp-note">{data.note}</p>}
  </figure>;
}

export function VisualTable({ chart, children }: { chart: PopularityChartData; children: ReactNode }) {
  return <><PopularityChart data={chart} /><details className="cmp-pop-raw"><summary>元の表・全数値を見る</summary><div className="cmp-pop-table-scroll"><table className="cmp-table">{children}</table></div></details></>;
}
