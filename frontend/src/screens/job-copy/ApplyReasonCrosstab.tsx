import { Fragment, useCallback, useMemo, useState } from 'react';
import { apiGet } from '../../api/client';
import { updateJobCopyQuery, useJobCopySearch } from './navigation';
import { useReportPolling } from './useReportPolling';
import { ReportFreshness } from './ReportFreshness';
import { MIN_SHARE_N } from './reasonCategories';
import { buildCrosstab, formatCount, formatShare, REASON_KINDS } from './applyReasonCrosstabModel';
import type { AreaKind, CrosstabRow, ApplyReasonResponse, ReasonKind } from './applyReasonCrosstabModel';
import { demoApplyReasons } from './demoApplyReasons';
import './apply-reason-crosstab.css';

const PERIODS = [[1, '直近1か月'], [3, '直近3か月'], [6, '直近6か月']] as const;
const MAX_TEXTS = 50;

function TextList({ row }: { row: CrosstabRow }) {
  const [open, setOpen] = useState(false);
  if (!row.details.length) return null;
  return <details className="ar-texts" onToggle={e => { setOpen(e.currentTarget.open); }}>
    <summary>内容を見る</summary>
    {open && <ul>
      {row.details.slice(0, MAX_TEXTS).map(d => <li key={d.key}><strong>{d.labels}</strong>{d.items.map((i, n) => <div key={n}>{i.source}: {i.text}</div>)}</li>)}
      {row.details.length > MAX_TEXTS && <li>ほか {formatCount(row.details.length - MAX_TEXTS)} 件</li>}
    </ul>}
  </details>;
}

function Cells({ row, columns }: { row: CrosstabRow; columns: string[] }) {
  return <>{columns.map(c => {
    const n = row.counts[c] ?? 0;
    const s = n > 0 ? formatShare(n, row.withReason) : null;
    return <td key={c} className={n === 0 ? 'ar-zero' : undefined}>{formatCount(n)}{s && <span className="ar-share">{s}</span>}</td>;
  })}<td>{formatCount(row.withReason)}</td></>;
}

function Head({ first, columns, base }: { first: string; columns: string[]; base: string }) {
  return <thead><tr><th scope="col">{first}</th>{columns.map(c => <th key={c} scope="col">{c}</th>)}<th scope="col">{base}</th></tr></thead>;
}

export function ApplyReasonCrosstab() {
  const search = useJobCopySearch(); const query = new URLSearchParams(search);
  const demo = query.get('demo') === '1';
  const months = [1, 3, 6].includes(Number(query.get('period'))) ? Number(query.get('period')) : 6;
  const area: AreaKind = query.get('area') === 'job' ? 'job' : 'applicant';
  const kind: ReasonKind = query.get('reason') === 'priority' ? 'priority' : 'motive';
  const kindLabel = REASON_KINDS.find(k => k.kind === kind)?.label ?? '';
  const occupation = query.get('occupation');
  const [data, setData] = useState<ApplyReasonResponse | null>(demo ? demoApplyReasons : null);
  const [failed, setFailed] = useState(false);
  const [reload, setReload] = useState(0);
  const [open, setOpen] = useState<Set<string>>(new Set());
  const load = useCallback(async (signal: AbortSignal) => {
    const r = await apiGet<ApplyReasonResponse>('/api/job-copy/apply-reasons', { signal });
    if (signal.aborted) return false;
    if (!r.ok || r.data.status === 'failed') { setFailed(true); setData(null); return false; }
    setFailed(false); setData(r.data);
    return r.data.status === 'preparing';
  }, []);
  useReportPolling(load, !demo, reload);
  const report = data?.status === 'ready' ? data.report : undefined;
  const crosstab = useMemo(() => report ? buildCrosstab(report, { months, kind, area, occupation }) : null, [report, months, kind, area, occupation]);
  const set = (values: Record<string, string | null>) => { updateJobCopyQuery(values, 'replace'); };

  if (failed) return <main className="ar-main"><h2>応募理由の地域・職種別</h2><p role="alert" className="ar-status">集計を作れませんでした。</p><button type="button" className="jc-button" onClick={() => { setFailed(false); setReload(n => n + 1); }}>再読み込み</button></main>;
  if (!report || !crosstab) return <main className="ar-main"><h2>応募理由の地域・職種別</h2><p role="status" className="ar-status">集計を作っています。</p></main>;

  const { period, excluded, columns } = crosstab;
  const toggle = (key: string) => { setOpen(prev => { const next = new Set(prev); if (next.has(key)) next.delete(key); else next.add(key); return next; }); };
  const share = period ? formatShare(crosstab.inTable, period.total) : null;
  const base = `${kindLabel}を聞けた応募`;
  const cols = columns.length + 2;
  const late = period && crosstab.firstRecorded && crosstab.firstRecorded > period.since ? crosstab.firstRecorded : null;
  return <main className="ar-main">
    <h2>応募理由の地域・職種別</h2>
    <ReportFreshness builtAt={data?.built_at} stale={data?.stale} refreshing={data?.refreshing} />
    <div className="ar-kinds" role="group" aria-label="表の中身">{REASON_KINDS.map(k => <button key={k.kind} type="button" className="ar-kind" aria-pressed={k.kind === kind} onClick={() => { set({ reason: k.kind === 'motive' ? null : k.kind }); }}>{k.label}</button>)}</div>
    <div className="ar-controls">
      <label>期間<select value={months} onChange={e => { set({ period: e.target.value }); }}>{PERIODS.map(([m, l]) => <option key={m} value={m}>{l}</option>)}</select></label>
      <label>地域<select value={area} onChange={e => { set({ area: e.target.value }); }}><option value="applicant">応募者の住所</option><option value="job">求人の勤務地</option></select></label>
      <label>職種<select value={occupation ?? ''} onChange={e => { set({ occupation: e.target.value || null }); }}><option value="">すべて</option>{report.occupation_groups.map(g => <option key={g} value={g}>{g}</option>)}</select></label>
    </div>
    {period && <p className="ar-lead">{base} {formatCount(crosstab.inTable)} 件／全応募 {formatCount(period.total)} 件{share ? `（${share}）` : ''}</p>}
    <ul className="ar-notes">
      {late && <li>選択式の記録は {late.replaceAll('-', '/')} の応募からです。それより前の応募には値がありません。</li>}
      <li>一次対応のヒアリングで選んだ値を、そのまま数えています。</li>
      {kind === 'motive' ? <li>応募動機は複数選べるので、1 件の応募が複数の列に入ることがあります。</li> : <li>①〜③のどれかに選んだ応募を数えています。1 件の応募が最大 3 つの列に入ります。</li>}
      <li>地域・職種と理由の重なりは、理由の原因を示すとは限りません。</li>
    </ul>
    <p className="ar-excluded">表から外した応募: 地域が読めない {formatCount(excluded.noArea)} 件（地域別の表）・職種が分からない {formatCount(excluded.noOccupation)} 件（職種が無い {formatCount(excluded.occupationMissing)}・対応表に無い {formatCount(excluded.occupationUnmatched)}・求人票が食い違う {formatCount(excluded.occupationConflict)}。職種別の表では「未分類」の行）。外した応募も上の {formatCount(crosstab.inTable)} 件に含みます。</p>

    <h3>地域別</h3>
    <div className="ar-scroll"><table aria-label={`地域別の${kindLabel}`}>
      <Head first="地域" columns={columns} base={base} />
      <tbody>
        {crosstab.area.map(p => <Fragment key={p.key}>
          <tr><td><div className="ar-label"><span><button type="button" className="ar-toggle" aria-expanded={open.has(p.key)} aria-label={`${p.label}の市区町村を${open.has(p.key) ? '閉じる' : '開く'}`} onClick={() => { toggle(p.key); }}>{open.has(p.key) ? '▼' : '▶'}</button>{p.label}</span><TextList row={p} /></div></td><Cells row={p} columns={columns} /></tr>
          {open.has(p.key) && p.children.map(c => <tr key={c.key} className="ar-child"><td><div className="ar-label"><span>{c.label}</span><TextList row={c} /></div></td><Cells row={c} columns={columns} /></tr>)}
        </Fragment>)}
        {!crosstab.area.length && <tr><td colSpan={cols}>この条件に当たる応募はありません。</td></tr>}
      </tbody>
    </table></div>

    <h3>職種別</h3>
    <div className="ar-scroll"><table aria-label={`職種別の${kindLabel}`}>
      <Head first="職種" columns={columns} base={base} />
      <tbody>{crosstab.occupation.map(o => <tr key={o.key}><td>{o.label}</td><Cells row={o} columns={columns} /></tr>)}
        {!crosstab.occupation.length && <tr><td colSpan={cols}>この期間に当たる応募はありません。</td></tr>}
      </tbody>
    </table></div>
    {crosstab.hiddenShare && <p className="ar-footnote">{String(MIN_SHARE_N)} 件未満の行は割合を出していません。</p>}
  </main>;
}
