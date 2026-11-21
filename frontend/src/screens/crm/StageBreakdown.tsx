import { useCallback, useEffect, useRef, useState } from 'react';
import { apiGet } from '../../api/client';
import type { StageBreakdownResponse } from '../../generated/StageBreakdownResponse';
import type { StageBreakdownStage } from '../../generated/StageBreakdownStage';
import { filtersToParams } from './queueModel';
import type { QueueFilters } from './queueModel';
import './stage-breakdown.css';

const DAY_CHOICES = [7, 30, 90] as const;
const POLL_MS = 3_000;

export function breakdownPath(f: QueueFilters, days: number, fresh: boolean): string {
  const p = filtersToParams(f);
  // 内訳はステージの絞り込みと並べ替えを使わない (サーバも無視する)
  p.delete('stage');
  p.delete('sort');
  p.delete('due');
  p.set('days', String(days));
  if (fresh) p.set('fresh', '1');
  return `/api/crm/call-queue/stage-breakdown?${p.toString()}`;
}

const pct = (n: number, total: number) => (total > 0 ? `${((n / total) * 100).toFixed(1)}%` : '—');
const num = (n: number) => n.toLocaleString('ja-JP');
const shortDay = (d: string) => `${String(Number(d.slice(5, 7)))}/${String(Number(d.slice(8, 10)))}`;

type Load = { phase: 'loading' } | { phase: 'error'; message: string } | { phase: 'done'; data: StageBreakdownResponse };

/** 架電一覧の条件に合う取引の、ステージの内訳と日ごとの推移 (押したときだけ読む) */
export function StageBreakdownDialog({ filters, onClose }: { filters: QueueFilters; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const [days, setDays] = useState<number>(30);
  const [load, setLoad] = useState<Load>({ phase: 'loading' });
  const [nonce, setNonce] = useState(0);
  const fresh = useRef(false);
  useEffect(() => { const d = ref.current; d?.showModal(); return () => { d?.close(); }; }, []);

  useEffect(() => {
    const ctl = new AbortController();
    let timer: number | undefined;
    const run = async (useFresh: boolean) => {
      const r = await apiGet<StageBreakdownResponse>(breakdownPath(filters, days, useFresh), { signal: ctl.signal, timeoutMs: 35_000 });
      if (ctl.signal.aborted) return;
      if (!r.ok) { setLoad({ phase: 'error', message: '読み込めませんでした。少し待ってから最新にしてください。' }); return; }
      if (r.data.status === 'loading') { setLoad({ phase: 'loading' }); timer = window.setTimeout(() => { void run(false); }, POLL_MS); return; }
      setLoad({ phase: 'done', data: r.data });
    };
    const useFresh = fresh.current;
    fresh.current = false;
    void run(useFresh);
    return () => { ctl.abort(); if (timer !== undefined) window.clearTimeout(timer); };
  }, [filters, days, nonce]);

  const refresh = useCallback(() => { fresh.current = true; setLoad({ phase: 'loading' }); setNonce(n => n + 1); }, []);

  return <dialog ref={ref} className="sb-dialog" aria-label="ステージの内訳" onCancel={e => { e.preventDefault(); onClose(); }}>
    <div className="sb-head">
      <h2>ステージの内訳</h2>
      <div className="sb-head-actions">
        <label>期間 <select value={days} onChange={e => { setLoad({ phase: 'loading' }); setDays(Number(e.target.value)); }}>
          {DAY_CHOICES.map(d => <option key={d} value={d}>直近 {d} 日</option>)}
        </select></label>
        <button type="button" className="cq-btn" onClick={refresh} disabled={load.phase === 'loading'}>最新にする</button>
        <button type="button" className="cq-btn" onClick={onClose}>閉じる</button>
      </div>
    </div>
    <p className="sb-scope">いまの条件に合う取引を、選んだパイプラインの全ステージで数えます（ステージの絞り込みは使いません）。</p>
    {load.phase === 'loading' && <p role="status" className="sb-status">集計しています…（取引が多いと 1 分ほどかかります）</p>}
    {load.phase === 'error' && <p role="alert" className="sb-status sb-error">{load.message}</p>}
    {load.phase === 'done' && <Body data={load.data} />}
  </dialog>;
}

function Body({ data }: { data: StageBreakdownResponse }) {
  if (data.status !== 'ready') return <p role="alert" className="sb-status sb-error">{data.message ?? '集計できませんでした。'}</p>;
  const stages = data.pipelines.flatMap(p => p.stages.map(s => ({ ...s, pipeline: p.label })));
  const unconsumed = stages.filter(s => s.unconsumed);
  const unNow = unconsumed.reduce((a, s) => a + s.now, 0);
  const n = data.days.length;
  const unByDay = data.days.map((_, i) => unconsumed.reduce((a, s) => a + (s.by_day[i] ?? 0), 0));
  const totalByDay = data.days.map((_, i) => stages.reduce((a, s) => a + (s.by_day[i] ?? 0), 0) + (data.other_pipeline[i] ?? 0) + (data.unknown[i] ?? 0));
  const first = unByDay[0] ?? 0;
  const multi = data.pipelines.length > 1;
  const label = (s: { label: string; pipeline: string }) => (multi ? `${s.pipeline} / ${s.label}` : s.label);
  // 推移の列: 未済を先に、期間中に 1 件でもいたステージだけ
  const shown = [...unconsumed, ...stages.filter(s => !s.unconsumed)].filter(s => s.now > 0 || s.by_day.some(v => v > 0) || s.unconsumed);
  const hasOther = data.other_pipeline.some(v => v > 0);
  const hasUnknown = data.unknown.some(v => v > 0);

  if (data.total === 0) return <p className="sb-status">条件に合う取引がありません。</p>;
  return <div className="sb-body">
    <section className="sb-summary" aria-label="未済の残り">
      <div className="sb-big"><span className="sb-big-label">未済に残っている取引</span>
        <strong data-testid="sb-unconsumed">{num(unNow)}<small> 件</small></strong>
        <span className="sb-big-sub">全 {num(data.total)} 件の {pct(unNow, data.total)}</span></div>
      {n > 1 && <div className="sb-delta"><span>{shortDay(data.days[0] ?? '')} の終わり</span><strong>{num(first)} 件</strong>
        <span>→ 今</span><strong>{num(unNow)} 件</strong>
        <span className="sb-delta-diff">{unNow - first <= 0 ? `${num(first - unNow)} 件 減` : `${num(unNow - first)} 件 増`}</span></div>}
      {unNow === 0 && <p className="sb-done">未済の取引はもうありません。このリストは消化済みです。</p>}
    </section>

    <section aria-label="今のステージの内訳">
      <h3>今のステージ</h3>
      {data.pipelines.map(p => <table key={p.id} className="sb-now">
        {multi && <caption>{p.label}</caption>}
        <tbody>{p.stages.filter(s => s.now > 0 || s.unconsumed).map(s => <NowRow key={s.id} s={s} total={data.total} />)}</tbody>
      </table>)}
    </section>

    <section aria-label="未済の推移">
      <h3>未済の推移</h3>
      <UnconsumedChart days={data.days} values={unByDay} totals={totalByDay} />
    </section>

    <section aria-label="日ごとのステージ">
      <h3>日ごとのステージ（各日の終わり、今日は今）</h3>
      <div className="sb-trend-scroll">
        <table className="sb-trend">
          <thead><tr><th scope="col">日付</th>
            {shown.map(s => <th key={s.id} scope="col" className={s.unconsumed ? 'sb-un' : undefined}>{label(s)}</th>)}
            {hasOther && <th scope="col">ほかのパイプライン</th>}
            {hasUnknown && <th scope="col">履歴が足りない</th>}
          </tr></thead>
          <tbody>{data.days.map((d, i) => ({ d, i })).reverse().map(({ d, i }) => <tr key={d}>
            <th scope="row">{shortDay(d)}</th>
            {shown.map(s => <td key={s.id} className={s.unconsumed ? 'sb-un' : undefined}>{num(s.by_day[i] ?? 0)}</td>)}
            {hasOther && <td>{num(data.other_pipeline[i] ?? 0)}</td>}
            {hasUnknown && <td>{num(data.unknown[i] ?? 0)}</td>}
          </tr>)}</tbody>
        </table>
      </div>
    </section>

    <details className="sb-notes"><summary>数え方</summary><ul>
      <li>取引は今の条件で決まります。途中で条件から外れた取引は、過去の日にも出ません。</li>
      <li>過去の日のステージは、HubSpot のステージの変更履歴から組み立てています。</li>
      <li>HubSpot は変更履歴を新しい順に 20 件までしか返さないため、ステージを 20 回以上変えた取引は、古い日が「履歴が足りない」になります。</li>
      <li>その日にまだ作られていなかった取引は、その日の数に入りません。</li>
      <li>{data.generated_at ? `${new Date(data.generated_at).toLocaleString('ja-JP', { timeZone: 'Asia/Tokyo' })} 時点の集計です（10 分間は同じ結果を出します）。` : ''}</li>
    </ul></details>
  </div>;
}

function NowRow({ s, total }: { s: StageBreakdownStage; total: number }) {
  const w = total > 0 ? Math.max((s.now / total) * 100, s.now > 0 ? 0.8 : 0) : 0;
  return <tr className={s.unconsumed ? 'sb-un' : undefined}>
    <th scope="row">{s.label}</th>
    <td className="sb-n">{num(s.now)}</td>
    <td className="sb-p">{pct(s.now, total)}</td>
    <td className="sb-bar-cell"><span className="sb-bar" style={{ width: `${String(w)}%` }} /></td>
  </tr>;
}

function UnconsumedChart({ days, values, totals }: { days: string[]; values: number[]; totals: number[] }) {
  const max = Math.max(1, ...values);
  const W = 640, H = 70, pad = 24, top = 14;
  const bw = (W - pad) / Math.max(1, values.length);
  const step = Math.ceil(days.length / 10);
  return <figure className="sb-chart">
    <svg viewBox={`0 0 ${String(W)} ${String(top + H + 18)}`} role="img" aria-label="未済の件数の推移">
      {values.map((v, i) => {
        const h = (v / max) * H;
        return <g key={days[i]}>
          <rect x={pad + i * bw + 1} y={top + H - h} width={Math.max(1, bw - 2)} height={h} className="sb-chart-bar">
            <title>{`${shortDay(days[i] ?? '')}: 未済 ${num(v)} 件（${pct(v, totals[i] ?? 0)}）`}</title>
          </rect>
          {(i === 0 || i === values.length - 1) && <text x={pad + i * bw + bw / 2} y={top + H - h - 3} textAnchor="middle" className="sb-chart-v">{num(v)}</text>}
          {i % step === 0 && <text x={pad + i * bw + bw / 2} y={top + H + 14} textAnchor="middle" className="sb-chart-x">{shortDay(days[i] ?? '')}</text>}
        </g>;
      })}
      <line x1={pad} x2={W} y1={top + H} y2={top + H} className="sb-chart-axis" />
    </svg>
    <figcaption>棒の高さは未済の件数（0 件が下の線）。棒が低くなるほど、未済が消化されています。</figcaption>
  </figure>;
}
