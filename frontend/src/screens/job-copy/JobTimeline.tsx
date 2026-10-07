import { useEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import type { EChartsCoreOption } from 'echarts/core';
import { apiGet } from '../../api/client';
import { EChart } from '../../components/EChart';
import type { JobCopyRecord } from './data';
import { demoMarketData } from './data';
import { marketRows } from './marketChartModel';
import type { MarketData, MarketRow } from './marketChartModel';
import { chooseMarket } from './marketMatch';
import { salaryLabel } from './salaryExtract';
import {
  addDays, applicationBuckets, asOfDate, billingEntries, buildPeriods, dayNumber, formatDay, formatPerDay, formatYen,
  marketLane, periodRows, positionOf, timelineRange, versionChanges, applicationsOutsidePeriods,
} from './timelineModel';
import type { BillingEntry, Granularity, MarketChangeResult, PeriodRow, TimelineRange } from './timelineModel';
import './timeline.css';

type MarketFetch = (title: string, prefecture: string) => Promise<{ ok: true; data: MarketData } | { ok: false }>;
const apiMarket: MarketFetch = async (title, prefecture) => {
  const params = title && prefecture ? `?${new URLSearchParams({ title, prefecture }).toString()}` : '';
  const result = await apiGet<MarketData>(`/api/job-copy/market${params}`);
  return result.ok ? { ok: true, data: result.data } : { ok: false };
};
const demoMarket: MarketFetch = (title, prefecture) => Promise.resolve({ ok: true, data: demoMarketData(title, prefecture) });

interface MarketState {
  status: 'loading' | 'ready' | 'error';
  meta: MarketData | null;
  title: string;
  prefecture: string;
  /** How the current choice was made. */
  chosenBy: 'auto-exact' | 'auto-partial' | 'user' | 'none';
  rows: MarketRow[] | null;
}

function useTimelineMarket(job: JobCopyRecord, mode: 'api' | 'demo') {
  const fetchMarket = mode === 'demo' ? demoMarket : apiMarket;
  const [state, setState] = useState<MarketState>({ status: 'loading', meta: null, title: '', prefecture: '', chosenBy: 'none', rows: null });
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let cancelled = false;
    void fetchMarket('', '').then(result => {
      if (cancelled) return;
      if (!result.ok) { setState(previous => ({ ...previous, status: 'error' })); return; }
      const choice = chooseMarket(job, result.data.titles, result.data.prefectures);
      const chosenBy = choice.title ? (choice.titleHow === 'exact' ? 'auto-exact' : 'auto-partial') : 'none';
      setState({ status: choice.title && choice.prefecture ? 'loading' : 'ready', meta: result.data, title: choice.title ?? '', prefecture: choice.prefecture ?? '', chosenBy, rows: null });
    }).catch(() => { if (!cancelled) setState(previous => ({ ...previous, status: 'error' })); });
    return () => { cancelled = true; };
  }, [job, fetchMarket, attempt]);
  const { title, prefecture, meta } = state;
  useEffect(() => {
    if (!meta || !title || !prefecture) return;
    let cancelled = false;
    void fetchMarket(title, prefecture).then(result => {
      if (cancelled) return;
      setState(previous => ({ ...previous, status: result.ok ? 'ready' : 'error', rows: result.ok && result.data.series ? marketRows(result.data.series) : [] }));
    }).catch(() => { if (!cancelled) setState(previous => ({ ...previous, status: 'error' })); });
    return () => { cancelled = true; };
  }, [meta, title, prefecture, fetchMarket]);
  const choose = (next: { title?: string; prefecture?: string }) => {
    setState(previous => {
      const title = next.title ?? previous.title; const prefecture = next.prefecture ?? previous.prefecture;
      return { ...previous, title, prefecture, chosenBy: 'user', rows: null, status: title && prefecture ? 'loading' : 'ready' };
    });
  };
  return { state, choose, retry: () => { setState(previous => ({ ...previous, status: 'loading' })); setAttempt(value => value + 1); } };
}

const certaintyLabel = { confirmed: '確定', estimated: '推定', unknown: '不明' } as const;
const granularityLabel: Record<Granularity, string> = { day: '日', week: '週', month: '月' };

function Lane({ title, source, children, className = '' }: { title: string; source: string; children: ReactNode; className?: string }) {
  return <div className={`jt-lane ${className}`} role="group" aria-label={title}>
    <div className="jt-lane-head"><h3>{title}</h3><span className="jt-source" title={source}>{source}</span></div>
    <div className="jt-track">{children}</div>
  </div>;
}

function span(range: TimelineRange, start: string, endExclusive: string) {
  const left = positionOf(start, range);
  const right = positionOf(endExclusive, range);
  return { left: `${left.toFixed(3)}%`, width: `${Math.max(0.6, right - left).toFixed(3)}%` };
}

function marketText(market: MarketChangeResult): string {
  if (market.ok) {
    const sign = market.value.changePct > 0 ? '+' : market.value.changePct < 0 ? '−' : '±';
    return `${sign}${Math.abs(market.value.changePct).toFixed(1)}%（${market.value.fromMonth} ${market.value.fromJobs.toLocaleString('ja-JP')}件 → ${market.value.toMonth} ${market.value.toJobs.toLocaleString('ja-JP')}件）`;
  }
  if (market.reason === 'not_selected') return '市場を選ぶと表示';
  if (market.reason === 'same_month') return `同じ月の中（${market.month ?? ''} ${market.jobs?.toLocaleString('ja-JP') ?? ''}件）`;
  return 'データなし';
}
function billingText(row: PeriodRow): string {
  if (!row.billing.connected) return '未接続';
  if (row.billing.yen === null) return row.billing.missingAmount ? '金額の記載なし' : 'この期間の課金データなし';
  return `${row.billing.prorated ? '約' : ''}${formatYen(row.billing.yen)}${row.billing.missingAmount ? '（金額の記載がない期間あり）' : ''}`;
}

function axisMonths(range: TimelineRange) {
  const ticks: { date: string; label: string }[] = [];
  let cursor = `${range.start.slice(0, 7)}-01`;
  if (cursor < range.start) cursor = addDays(`${range.start.slice(0, 7)}-01`, 32).slice(0, 7) + '-01';
  while (cursor <= range.end && ticks.length < 60) {
    ticks.push({ date: cursor, label: `${String(Number(cursor.slice(5, 7)))}月` });
    cursor = addDays(cursor, 32).slice(0, 7) + '-01';
  }
  return ticks;
}

const dayMs = (date: string) => dayNumber(date) * 86_400_000;

export interface JobTimelineProps {
  job: JobCopyRecord;
  /** Billing rows from outside the snapshot (the billing CSV). Kept in the browser only. */
  billing?: readonly BillingEntry[] | undefined;
  /** 'demo' uses fictional market data and sends no request. */
  marketMode?: 'api' | 'demo';
  onOpenVersion?: (versionId: string) => void;
  onCompareVersions?: (beforeId: string, afterId: string) => void;
  now?: Date | undefined;
}

export function JobTimeline(props: JobTimelineProps) {
  return <JobTimelineForJob key={props.job.id} {...props} />;
}

function JobTimelineForJob({ job, billing: injected, marketMode = 'api', onOpenVersion, onCompareVersions, now }: JobTimelineProps) {
  const asOf = asOfDate(job, now);
  const billing = useMemo(() => billingEntries(job, injected), [job, injected]);
  const periods = useMemo(() => buildPeriods(job, asOf), [job, asOf]);
  const changes = useMemo(() => versionChanges(job), [job]);
  const range = useMemo(() => timelineRange(job, asOf, billing), [job, asOf, billing]);
  const market = useTimelineMarket(job, marketMode);
  const rows = useMemo(() => periodRows(job, { asOf, billing: injected, market: market.state.rows }), [job, asOf, injected, market.state.rows]);
  const [granularity, setGranularity] = useState<Granularity>('week');
  const [selected, setSelected] = useState<string | null>(null);
  const applications = job.overallApplications;
  const buckets = useMemo(() => applicationBuckets(applications?.byDate, granularity), [applications, granularity]);
  const lane = useMemo(() => range && market.state.rows ? marketLane(market.state.rows, range) : null, [range, market.state.rows]);
  const captured = periods.some(period => period.basis === 'captured');

  const applicationOption = useMemo<EChartsCoreOption | null>(() => {
    if (!range || !buckets.length) return null;
    return {
      animation: false,
      aria: { enabled: true, description: `HubSpotに記録された応募の${granularityLabel[granularity]}ごとの件数` },
      grid: { left: 0, right: 0, top: 18, bottom: 2 },
      tooltip: { trigger: 'item', confine: true, renderMode: 'richText', formatter: (item: { data: { value: [number, number]; start: string; end: string } }) => `${formatDay(item.data.start)}${granularity === 'day' ? '' : `〜${formatDay(addDays(item.data.end, -1))}`}: ${String(item.data.value[1])}件` },
      xAxis: { type: 'time', min: dayMs(range.start), max: dayMs(addDays(range.end, 1)), show: false },
      yAxis: { type: 'value', min: 0, minInterval: 1, show: false },
      series: [{ type: 'bar', name: '応募', barMaxWidth: 26, barMinWidth: 2, itemStyle: { color: '#a36816' }, label: { show: granularity !== 'day', position: 'top', fontSize: 10, color: '#5b3a0c' },
        data: buckets.map(bucket => ({ value: [(dayMs(bucket.start) + dayMs(bucket.end)) / 2, bucket.count], start: bucket.start, end: bucket.end })) }],
    };
  }, [range, buckets, granularity]);

  const marketOption = useMemo<EChartsCoreOption | null>(() => {
    if (!range || !lane || !lane.points.some(point => point.jobs !== null || point.viewers !== null) && !lane.noDataFrom) return null;
    const startMs = dayMs(range.start); const endMs = dayMs(addDays(range.end, 1));
    const x = (month: string) => Math.min(endMs, Math.max(startMs, dayMs(`${month}-15`)));
    return {
      animation: false,
      aria: { enabled: true, description: '市場求人数とIndeed閲覧者指標の月ごとの推移。データがない月は線を描きません。' },
      grid: { left: 0, right: 0, top: 14, bottom: 2 },
      legend: { show: false },
      tooltip: { trigger: 'axis', confine: true, renderMode: 'richText', valueFormatter: (value: unknown) => typeof value === 'number' ? value.toLocaleString('ja-JP') : 'データなし' },
      xAxis: { type: 'time', min: startMs, max: endMs, show: false },
      yAxis: [{ type: 'value', show: false, scale: true }, { type: 'value', show: false, scale: true }],
      series: [
        { type: 'line', name: '市場求人数', yAxisIndex: 0, connectNulls: false, symbolSize: 6, itemStyle: { color: '#2463a7' }, lineStyle: { width: 2 },
          data: lane.points.map(point => [x(point.month), point.jobs]) },
        { type: 'line', name: 'Indeed閲覧者指標', yAxisIndex: 1, connectNulls: false, symbolSize: 5, itemStyle: { color: '#7c4dbd' }, lineStyle: { width: 1.5, type: 'dashed' },
          data: lane.points.map(point => [x(point.month), point.viewers]) },
      ],
    };
  }, [range, lane]);

  if (!range) {
    return <section className="jt-timeline" aria-label="タイムライン"><h2>タイムライン</h2><p className="jc-notice">掲載期間・応募日・課金のどれも取得できていないため、時間軸を作れません。</p></section>;
  }
  const ticks = axisMonths(range);
  const selectedChange = changes.find(change => change.versionId === selected) ?? null;
  const previous = selectedChange ? changes[selectedChange.index - 1] : undefined;
  const salaryValues = changes.map(change => change.salary).filter(info => info && info.kind !== '不明' && info.min !== null);
  const salaryKind = salaryValues.at(-1)?.kind;
  const sameKind = salaryValues.filter(info => info?.kind === salaryKind).map(info => info?.min ?? 0);
  const low = Math.min(...sameKind); const high = Math.max(...sameKind);
  const csvBilling = billing.some(entry => entry.source === 'csv');
  const outside = applicationsOutsidePeriods(job, rows);
  const marketMeta = market.state.meta;

  return <section className="jt-timeline" aria-label="タイムライン">
    <header className="jt-heading">
      <div><h2>タイムライン</h2><p className="jc-muted">{formatDay(range.start)} 〜 {formatDay(range.end)}（応募は {formatDay(asOf)} 時点）</p></div>
      <p className="jt-scope">応募は HubSpot に記録されたものだけです。媒体上のすべての応募ではありません。<span title="同じ時期に起きたことを並べて表示しています。応募が増えた・減った理由を示すものではありません。">並べて見るための表示です ⓘ</span></p>
    </header>

    <div className="jt-lanes">
      <div className="jt-axis" aria-hidden="true"><div className="jt-lane-head" /><div className="jt-track">{ticks.map(tick => <span key={tick.date} style={{ left: `${positionOf(tick.date, range).toFixed(3)}%` }}>{tick.label}</span>)}</div></div>

      <Lane title="掲載期間" source={captured ? '求人データを取得した日から推定' : '媒体の掲載日時'}>
        {periods.map(period => <button type="button" key={period.versionId} className={`jt-period jt-cert-${period.certainty}`} aria-pressed={selected === period.versionId}
          style={span(range, period.start, period.end ?? addDays(asOf, 1))} onClick={() => { setSelected(period.versionId); }}
          title={`${period.label}: ${formatDay(period.start)}〜${period.end ? formatDay(addDays(period.end, -1)) : '継続中'}（${certaintyLabel[period.certainty]}）`}>
          <span>{period.label}</span></button>)}
        {!periods.length && <p className="jt-empty">掲載期間は未取得です</p>}
      </Lane>

      <Lane title="給与" source="本文の「給与」の行から読み取り">
        {changes.map((change, index) => {
          const period = periods[index];
          if (!period) return null;
          const info = change.salary;
          const readable = info && info.kind === salaryKind && info.min !== null;
          const height = readable && high > low ? 18 + (((info.min ?? low) - low) / (high - low)) * 40 : 30;
          return <div key={change.versionId} className={`jt-salary${readable ? '' : ' jt-salary-unknown'}${change.salaryChanged ? ' jt-salary-changed' : ''}`}
            style={{ ...span(range, period.start, period.end ?? addDays(asOf, 1)), bottom: `${String(height)}%` }}
            title={info ? `${change.label}: ${info.raw || '記載なし'}` : `${change.label}: 給与の記載なし`}>
            {change.salaryChanged && <i aria-hidden="true">▲</i>}<span>{salaryLabel(info)}</span>
          </div>;
        })}
      </Lane>

      <Lane title="本文" source="前の版との行の比較">
        {changes.map(change => <button type="button" key={change.versionId} className={`jt-mark${change.index > 0 && change.bodyStatus === 'changed' ? ' jt-mark-changed' : ''}`} aria-pressed={selected === change.versionId}
          style={{ left: `${positionOf(change.date, range).toFixed(3)}%` }} onClick={() => { setSelected(change.versionId); }}
          aria-label={`${change.label}の本文${change.index === 0 ? '（最初の版）' : `：追加${String(change.bodyAdded)}行・削除${String(change.bodyRemoved)}行`}`}>
          {change.index === 0 ? '最初' : change.bodyStatus === 'unchanged' ? '同じ' : change.bodyStatus === 'format_only' ? '改行のみ' : `+${String(change.bodyAdded)} / −${String(change.bodyRemoved)}`}
        </button>)}
      </Lane>

      <Lane title="画像" source="前の版との画像の比較">
        {changes.map(change => <button type="button" key={change.versionId} className={`jt-mark jt-image-${change.imageChange}`} aria-pressed={selected === change.versionId}
          style={{ left: `${positionOf(change.date, range).toFixed(3)}%` }} onClick={() => { setSelected(change.versionId); }}
          aria-label={`${change.label}の画像：${change.imageChange === 'initial' ? '最初の版' : change.imageChange === 'changed' ? '変更あり' : change.imageChange === 'same' ? '同じ' : '比べられない'}`}>
          {change.imageChange === 'initial' ? '最初' : change.imageChange === 'changed' ? '変更' : change.imageChange === 'same' ? '同じ' : '不明'}
        </button>)}
      </Lane>

      <Lane title="課金" source={csvBilling ? 'HRハッカー実績・読み込んだ課金CSV' : 'HRハッカーの期間別実績'}>
        {billing.length ? billing.map(entry => <div key={`${entry.source}-${entry.start}-${entry.end}`} className={`jt-billing jt-billing-${entry.source}`} style={span(range, entry.start, addDays(entry.end, 1))}
          title={`${formatDay(entry.start)}〜${formatDay(entry.end)}: ${entry.amountYen === null ? '金額の記載なし' : `${entry.amountYen.toLocaleString('ja-JP')}円`}${entry.taxIncluded === true ? '（税込）' : entry.taxIncluded === false ? '（税抜）' : ''}${entry.plan ? ` · ${entry.plan}` : ''}`}>
          <span>{entry.amountYen === null ? '金額なし' : formatYen(entry.amountYen)}</span></div>)
          : <p className="jt-empty jt-unconnected" title="課金データが届いていないため表示していません。金額がかからなかったという意味ではありません。">未接続（課金データがありません）</p>}
      </Lane>
      {csvBilling && <p className="jt-volatile" role="note">読み込んだ課金CSVはこの画面を開いている間だけ表示します。再読み込みすると消えます。</p>}

      <Lane title="応募" source="HubSpot に記録された応募のみ" className="jt-lane-chart">
        {applicationOption ? <EChart option={applicationOption} testId="jt-applications" height={72} renderer="svg" /> : <p className="jt-empty">{!applications?.byDate ? '応募日別の件数は未取得です' : '応募日の分かる応募はありません'}</p>}
      </Lane>
      <div className="jt-lane-tools">
        <div className="jt-granularity" role="group" aria-label="応募の集計単位">{(['day', 'week', 'month'] as const).map(value => <button type="button" key={value} aria-pressed={granularity === value} onClick={() => { setGranularity(value); }}>{granularityLabel[value]}ごと</button>)}</div>
        {applications && applications.missingDate > 0 && <span>応募日が分からない応募 {applications.missingDate}件 はグラフに含めていません</span>}
        {outside > 0 && <span>掲載期間の外の日付の応募 {outside}件</span>}
      </div>

      <Lane title="市場" source="Indeed の市場データ（都道府県・職種の月ごと）" className="jt-lane-chart">
        {lane?.noDataFrom && <div className="jt-nodata" style={span(range, `${lane.noDataFrom}-01` > range.start ? `${lane.noDataFrom}-01` : range.start, addDays(range.end, 1))} title="Indeed の市場データは取得済みの月までです">データなし</div>}
        {marketOption ? <EChart option={marketOption} testId="jt-market" height={72} renderer="svg" />
          : <p className="jt-empty">{market.state.status === 'loading' ? '市場データを取得中…' : market.state.status === 'error' ? '市場データを取得できませんでした' : !market.state.title ? '職種を選ぶと市場の動きを表示します' : !market.state.prefecture ? '都道府県を選ぶと市場の動きを表示します' : 'この職種・都道府県の市場データはありません'}</p>}
      </Lane>
      <div className="jt-lane-tools jt-market-tools">
        <span><i className="jt-key jt-key-jobs" />市場求人数 <i className="jt-key jt-key-viewers" />Indeed閲覧者指標 <span title={marketMeta?.ctk_basis ?? 'Indeed閲覧者指標は、求職者の人数やこの求人への応募数ではありません。'}>ⓘ</span></span>
        {marketMeta && <>
          <label>職種<select value={market.state.title} onChange={event => { market.choose({ title: event.target.value }); }}><option value="">選んでください</option>{marketMeta.titles.map(value => <option key={value}>{value}</option>)}</select></label>
          <label>都道府県<select value={market.state.prefecture} onChange={event => { market.choose({ prefecture: event.target.value }); }}><option value="">選んでください</option>{marketMeta.prefectures.map(value => <option key={value}>{value}</option>)}</select></label>
          <span className="jt-choice">{market.state.chosenBy === 'auto-exact' ? '求人名と同じ職種を自動で選びました' : market.state.chosenBy === 'auto-partial' ? '求人名に含まれる職種を自動で選びました。違う場合は選び直してください' : market.state.chosenBy === 'user' ? '手で選んだ職種です' : '求人名から職種を決められませんでした'}</span>
        </>}
        {market.state.status === 'error' && <button type="button" className="jc-button" onClick={market.retry}>市場データを再取得</button>}
        {lane?.noDataFrom && <span>{lane.noDataFrom.replace('-', '年')}月以降は市場データがありません（{lane.lastDataMonth ? `${lane.lastDataMonth.replace('-', '年')}月まで` : '取得なし'}）</span>}
      </div>
    </div>

    <div className="jt-legend" aria-label="凡例"><span><i className="jt-key jt-cert-confirmed" />確定</span><span><i className="jt-key jt-cert-estimated" />推定</span><span><i className="jt-key jt-cert-unknown" />不明</span>
      {captured && <span>掲載期間の日付は、求人データを取得した日です。掲載を変更した日とは限りません。</span>}</div>

    {selectedChange && <div className="jt-selection" role="region" aria-label="選んだ版">
      <strong>{selectedChange.label}</strong><span>{formatDay(selectedChange.date)} から · 給与 {salaryLabel(selectedChange.salary)}</span>
      {onOpenVersion && <button type="button" className="jc-button" onClick={() => { onOpenVersion(selectedChange.versionId); }}>本文・画像を開く</button>}
      {onCompareVersions && previous && <button type="button" className="jc-button" onClick={() => { onCompareVersions(previous.versionId, selectedChange.versionId); }}>前の版との差分を開く</button>}
      {previous && selectedChange.bodyStatus === 'unchanged' && selectedChange.imageChange === 'same' && <span>前の版から本文・画像の変更はありません</span>}
    </div>}

    <section className="jt-periods" aria-label="期間比較表">
      <h3>期間比較表</h3>
      <p className="jc-muted">版が切り替わった日で期間を区切っています。期間の長さが違うので「1日あたり」で並べて確認してください。</p>
      <div className="jt-table-scroll" role="region" aria-label="期間比較表の数値" tabIndex={0}><table>
        <thead><tr><th scope="col">期間</th><th scope="col">日数</th><th scope="col">応募件数</th><th scope="col">1日あたり</th><th scope="col">課金額</th><th scope="col">市場求人数の同時期変化</th></tr></thead>
        <tbody>{rows.map(row => <tr key={row.key} className={row.kind === 'gap' ? 'jt-gap-row' : selected === row.versionId ? 'jt-row-selected' : undefined}>
          <th scope="row">{row.versionId ? <button type="button" className="jc-text-button" onClick={() => { setSelected(row.versionId); }}>{row.label}</button> : row.label}<small>{formatDay(row.start)}〜{row.ongoing ? `継続中（${formatDay(row.lastDay)}まで）` : formatDay(row.lastDay)}</small></th>
          <td>{row.days}日</td><td>{row.applications}件</td><td>{formatPerDay(row.perDay)}</td>
          <td title={row.billing.connected && row.billing.prorated ? '課金の期間と版の期間がずれているため、日数で割って配分しています' : undefined}>{billingText(row)}</td>
          <td>{marketText(row.market)}</td>
        </tr>)}</tbody>
      </table></div>
      <p className="jc-muted">応募件数は HubSpot に記録された応募日で数えています。どの版を見て応募したかは分かりません。数値は並べて示すもので、増減の理由を示すものではありません。</p>
    </section>
  </section>;
}
