// /app/admin?view=callers: 今日の架電成績を全員分。データは GET /api/crm/metrics/today/all (管理者だけ)。
// 2 分ごとに自動で読み直す (タブが隠れている間は止める)。&mode=fixture で架空の数字。
import { useState } from 'react';
import type { AllCallersResponse } from '../../generated/AllCallersResponse';
import { asOfLabel, clockJst, filterByTeam, formatDuration, formatRate, sortRows } from '../crm/metricsModel';
import type { SortKey } from '../crm/metricsModel';
import { fixtureAllFetch, liveAllFetch, useMetrics } from '../crm/useTodayMetrics';
import type { MetricsFetch } from '../crm/useTodayMetrics';

interface Col {
  key: SortKey;
  label: string;
  num: boolean;
  cell: (r: AllCallersResponse['callers'][number]) => string;
}

export const CALLER_COLUMNS: Col[] = [
  { key: 'name', label: '名前', num: false, cell: r => r.name },
  { key: 'outbound', label: '架電数', num: true, cell: r => String(r.metrics.outbound) },
  { key: 'inbound', label: '着信数', num: true, cell: r => String(r.metrics.inbound) },
  { key: 'over_30s', label: '30秒以上', num: true, cell: r => String(r.metrics.over_30s) },
  { key: 'over_90s', label: '90秒以上', num: true, cell: r => String(r.metrics.over_90s) },
  { key: 'over_360s', label: '360秒以上', num: true, cell: r => String(r.metrics.over_360s) },
  { key: 'rate_90s', label: '90秒以上の割合', num: true, cell: r => formatRate(r.metrics.rate_90s) },
  { key: 'total_duration', label: '通話時間合計', num: true, cell: r => formatDuration(r.metrics.total_duration_ms) },
  { key: 'avg_duration', label: '通話時間平均', num: true, cell: r => formatDuration(r.metrics.avg_duration_ms) },
  { key: 'appointments', label: 'アポ', num: true, cell: r => String(r.metrics.appointments) },
  { key: 'first', label: '最初の架電', num: true, cell: r => clockJst(r.metrics.first_call_at) ?? '-' },
  { key: 'last', label: '最後の架電', num: true, cell: r => clockJst(r.metrics.last_call_at) ?? '-' },
];

export function CallersTable({ data }: { data: AllCallersResponse }) {
  const [sort, setSort] = useState<{ key: SortKey; dir: 'asc' | 'desc' }>({ key: 'outbound', dir: 'desc' });
  const [team, setTeam] = useState('');
  const rows = sortRows(filterByTeam(data.callers, team), sort.key, sort.dir);
  const click = (key: SortKey) => {
    setSort(prev => (prev.key === key ? { key, dir: prev.dir === 'desc' ? 'asc' : 'desc' } : { key, dir: key === 'name' ? 'asc' : 'desc' }));
  };
  return (
    <>
      <p className="w8-subtle">
        {asOfLabel(data.as_of)}（{data.day}）。Zoom の通話記録から集計しています。2 分ごとに自動で更新します。
        {data.stale && <span className="w8-alert" role="status"> 最新の数字を読めていません（前回の数字を表示中）。</span>}
      </p>
      <label className="w8-subtle">
        チーム{' '}
        <select value={team} onChange={e => { setTeam(e.target.value); }} aria-label="チームで絞り込む">
          <option value="">すべて</option>
          {data.teams.map(t => <option key={t} value={t}>{t}</option>)}
        </select>
        {' '}<span data-testid="callers-count">{rows.length} 人</span>
      </label>
      <div className="w8-table-wrap">
        <table className="w8-table w8-table-dense" data-testid="callers-table">
          <thead>
            <tr>
              {CALLER_COLUMNS.map(c => (
                <th key={c.key} className={c.num ? 'w8-right' : undefined}
                  aria-sort={sort.key === c.key ? (sort.dir === 'asc' ? 'ascending' : 'descending') : 'none'}>
                  <button type="button" className="w8-sort" onClick={() => { click(c.key); }}>
                    {c.label}{sort.key === c.key ? (sort.dir === 'asc' ? ' ▲' : ' ▼') : ''}
                  </button>
                </th>
              ))}
              <th>チーム</th>
            </tr>
          </thead>
          <tbody>
            {rows.map(r => (
              <tr key={r.owner_id}>
                {CALLER_COLUMNS.map(c => <td key={c.key} className={c.num ? 'w8-right' : undefined}>{c.cell(r)}</td>)}
                <td className="w8-muted">{r.teams.join('・') || '-'}</td>
              </tr>
            ))}
            {rows.length === 0 && <tr><td colSpan={CALLER_COLUMNS.length + 1} className="w8-muted">今日の架電はまだありません。</td></tr>}
          </tbody>
        </table>
      </div>
      <p className="w8-subtle">
        架電数は発信だけ（着信は別に表示）。割合は件数 ÷ 架電数。アポは「アポ日確定」に今日入った取引の担当者に付きます。
      </p>
    </>
  );
}

export function CallersPanel({ fetcher }: { fetcher?: MetricsFetch<AllCallersResponse> }) {
  const fixture = typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('mode') === 'fixture';
  const { state, reload } = useMetrics(fetcher ?? (fixture ? fixtureAllFetch : liveAllFetch));
  return (
    <>
      <h1 className="w8-h1" style={{ marginBottom: 4 }}>今日の架電成績（全員）</h1>
      {fixture && <p className="w8-alert" role="status">架空サンプルを表示中です。</p>}
      {state.phase === 'loading' && <p className="w8-subtle" role="status">読み込み中…</p>}
      {state.phase === 'error' && (
        <div role="alert" className="w8-alert" data-testid="admin-error">
          <p>{state.message}</p>
          <button type="button" onClick={reload}>再読み込み</button>
        </div>
      )}
      {state.phase === 'ready' && (
        <>
          {state.refreshFailed && <p className="w8-alert" role="status">更新に失敗しました。前回の数字を表示しています。</p>}
          <CallersTable data={state.data} />
        </>
      )}
    </>
  );
}
