import { useEffect, useMemo, useRef, useState } from 'react';
import type { CSSProperties, ReactNode, RefObject } from 'react';
import type { EChartsCoreOption } from 'echarts/core';
import { EChart } from '../../components/EChart';
import type { JobCopyRecord } from './data';
import { marketRows } from './marketChartModel';
import type { MarketData, MarketRow } from './marketChartModel';
import { useMarketFetch } from './marketSource';
import { chooseMarket } from './marketMatch';
import { salaryLabel } from './salaryExtract';
import { plainWording } from './format';
import { InfoTip } from './InfoTip';
import {
  addDays, applicationBuckets, bodyMark, asOfDate, billingConflict, billingEntries, boundaryStatus, buildPeriods, dayNumber, formatDay, formatMonth, formatPerDay, formatYen,
  formatMonthJa, marketDataUntil, marketLane, MIN_RATE_DAYS, periodRows, positionOf, timelineRange, uncertainSpans, versionChanges, applicationsOutsidePeriods,
} from './timelineModel';
import type { BillingEntry, Granularity, MarketChangeResult, PeriodRow, TimelineRange, VersionChange } from './timelineModel';
import { IMAGE_CHANGE_MARK } from './images';
import { DEMO_BILLING_LABEL, DEMO_BILLING_NOTE, DUMMY_BILLING_ENABLED, DUMMY_BILLING_LABEL, DUMMY_BILLING_NOTE, isDummyBilling } from './dummyBilling';
import { REASON_CATEGORIES, MIN_SHARE_N, basisText, classifyApplicationReasons, reasonsByPeriod, shareText, topReasons } from './reasonCategories';
import type { ReasonTally } from './reasonCategories';
import './timeline.css';

interface MarketState {
  status: 'loading' | 'ready' | 'error';
  meta: MarketData | null;
  title: string;
  prefecture: string;
  /** How the occupation was chosen. */
  titleBy: 'auto-exact' | 'auto-partial' | 'user' | 'none';
  /** How the prefecture was chosen (from the job's work location, or by the user). */
  prefectureBy: 'auto' | 'user' | 'none';
  rows: MarketRow[] | null;
  /**
   * The rows shown before the choice was changed. The 市場 chart keeps drawing them while the new
   * months load, so the chart is updated in place instead of being destroyed and created again.
   */
  staleRows: MarketRow[] | null;
  /**
   * A retry pressed by the user is running. The retry button stays on screen (aria-disabled) until
   * it ends, so keyboard focus is not dropped to the page body.
   */
  retrying: boolean;
}

interface MarketFocus { retryButton: RefObject<HTMLButtonElement | null>; titleSelect: RefObject<HTMLSelectElement | null> }

function useTimelineMarket(job: JobCopyRecord, mode: 'api' | 'demo', focus: MarketFocus) {
  const fetchMarket = useMarketFetch(mode);
  const [state, setState] = useState<MarketState>({ status: 'loading', meta: null, title: '', prefecture: '', titleBy: 'none', prefectureBy: 'none', rows: null, staleRows: null, retrying: false });
  // attempt: re-fetch the list of occupations / prefectures. seriesAttempt: re-fetch only the
  // months for the current choice (a retry after the list was read keeps a hand-picked choice).
  const [attempt, setAttempt] = useState(0);
  const [seriesAttempt, setSeriesAttempt] = useState(0);
  useEffect(() => {
    let cancelled = false;
    void fetchMarket('', '').then(result => {
      if (cancelled) return;
      if (!result.ok) { setState(previous => ({ ...previous, status: 'error', rows: null, staleRows: null, retrying: false })); return; }
      const choice = chooseMarket(job, result.data.titles, result.data.prefectures);
      setState(previous => {
        // A choice the user made by hand stays when it is still in the list.
        const keepTitle = previous.titleBy === 'user' && result.data.titles.includes(previous.title);
        const keepPrefecture = previous.prefectureBy === 'user' && result.data.prefectures.includes(previous.prefecture);
        const title = keepTitle ? previous.title : choice.title ?? '';
        const prefecture = keepPrefecture ? previous.prefecture : choice.prefecture ?? '';
        const titleBy: MarketState['titleBy'] = keepTitle ? 'user' : choice.title ? (choice.titleHow === 'exact' ? 'auto-exact' : 'auto-partial') : 'none';
        const prefectureBy: MarketState['prefectureBy'] = keepPrefecture ? 'user' : choice.prefecture ? 'auto' : 'none';
        const status: MarketState['status'] = title && prefecture ? 'loading' : 'ready';
        return { status, meta: result.data, title, prefecture, titleBy, prefectureBy, rows: null, staleRows: null, retrying: previous.retrying && status === 'loading' };
      });
    }).catch(() => { if (!cancelled) setState(previous => ({ ...previous, status: 'error', rows: null, staleRows: null, retrying: false })); });
    return () => { cancelled = true; };
  }, [job, fetchMarket, attempt]);
  const { title, prefecture, meta } = state;
  useEffect(() => {
    if (!meta || !title || !prefecture) return;
    let cancelled = false;
    void fetchMarket(title, prefecture).then(result => {
      if (cancelled) return;
      // A failed request keeps rows null: [] means "this choice has no market data", which is a
      // different message from "could not be fetched".
      setState(previous => result.ok
        ? { ...previous, status: 'ready', rows: result.data.series ? marketRows(result.data.series) : [], staleRows: null, retrying: false }
        : { ...previous, status: 'error', rows: null, staleRows: null, retrying: false });
    }).catch(() => { if (!cancelled) setState(previous => ({ ...previous, status: 'error', rows: null, staleRows: null, retrying: false })); });
    return () => { cancelled = true; };
  }, [meta, title, prefecture, fetchMarket, seriesAttempt]);
  const choose = (next: { title?: string; prefecture?: string }) => {
    setState(previous => {
      const title = next.title ?? previous.title; const prefecture = next.prefecture ?? previous.prefecture;
      const status: MarketState['status'] = title && prefecture ? 'loading' : 'ready';
      return { ...previous, title, prefecture, titleBy: next.title === undefined ? previous.titleBy : 'user', prefectureBy: next.prefecture === undefined ? previous.prefectureBy : 'user', rows: null,
        staleRows: status === 'loading' ? previous.rows ?? previous.staleRows : null, status };
    });
  };
  // When a retry ends, focus moves to the 職種 select (or stays on the retry button when it failed again).
  const focusAfterRetry = useRef(false);
  const retry = () => {
    if (state.status === 'loading') return;
    focusAfterRetry.current = true;
    setState(previous => ({ ...previous, status: 'loading', retrying: true }));
    if (state.meta) setSeriesAttempt(value => value + 1); else setAttempt(value => value + 1);
  };
  const { retryButton, titleSelect } = focus;
  useEffect(() => {
    if (!focusAfterRetry.current || state.status === 'loading') return;
    focusAfterRetry.current = false;
    if (state.status === 'error') retryButton.current?.focus();
    else (titleSelect.current ?? retryButton.current)?.focus();
  }, [state.status, retryButton, titleSelect]);
  return { state, choose, retry };
}

const salaryMark = { up: '▲', down: '▼', other: '変更' } as const;
const salaryWord = { up: '前の版より上がった', down: '前の版より下がった', other: '前の版から変わった' } as const;
const granularityLabel: Record<Granularity, string> = { day: '日', week: '週', month: '月' };

function Lane({ title, source, children, className = '' }: { title: string; source: string; children: ReactNode; className?: string }) {
  return <div className={`jt-lane ${className}`} role="group" aria-label={title}>
    <div className="jt-lane-head"><h3>{title}</h3><span className="jt-source">{source}</span></div>
    <div className="jt-track">{children}</div>
  </div>;
}

/** 「n=6（応募）: 給与 3件・50%（選択2・推定1）…」 for the 応募理由 lane's tooltip. */
function reasonTallyText(result: ReasonTally, unit: string): string {
  const parts = result.counts.filter(row => row.total > 0).map(row => {
    const share = shareText(row.total, result.n);
    return `${row.category} ${String(row.total)}件${share ? `・${share}` : ''}（選択${String(row.selected)}・推定${String(row.estimated)}）`;
  });
  return `n=${String(result.n)}（${unit}）${parts.length ? `: ${parts.join('、')}` : ''}${result.unclassified ? `、分類できない ${String(result.unclassified)}件` : ''}`;
}

function span(range: TimelineRange, start: string, endExclusive: string) {
  const left = positionOf(start, range);
  const right = positionOf(endExclusive, range);
  return { left: `${left.toFixed(3)}%`, width: `${Math.max(0.6, right - left).toFixed(3)}%` };
}

/**
 * A marker that starts at a day but must stay readable inside the track. It is shifted left by the
 * same share of its own width as its position on the track (0% → not shifted, 100% → fully to the
 * left of the day), so it never runs past the right edge. The pin (::before) marks the exact day.
 */
function pinned(date: string, range: TimelineRange): CSSProperties {
  const position = positionOf(date, range);
  return { left: `${position.toFixed(3)}%`, '--jt-pin': position.toFixed(3) } as CSSProperties;
}

function noDataNote(month: string | null | undefined): string {
  return month ? `、${formatMonthJa(month)}以降はデータなし` : '';
}
/** The market cell of the period table. While loading or after a failed request it says so (not "pick a market"). */
function marketText(market: MarketChangeResult, status: MarketState['status'] = 'ready'): string {
  if (status === 'error') return '取得できませんでした';
  if (status === 'loading') return '取得中…';
  if (market.ok) {
    const sign = market.value.changePct > 0 ? '+' : market.value.changePct < 0 ? '−' : '±';
    return `${sign}${Math.abs(market.value.changePct).toFixed(1)}%（${formatMonthJa(market.value.fromMonth)} ${market.value.fromJobs.toLocaleString('ja-JP')}件 → ${formatMonthJa(market.value.toMonth)} ${market.value.toJobs.toLocaleString('ja-JP')}件${noDataNote(market.value.noDataFrom)}）`;
  }
  if (market.reason === 'not_selected') return '市場を選ぶと表示';
  if (market.reason === 'same_month') return `同じ月の中（${formatMonthJa(market.month ?? '')} ${market.jobs?.toLocaleString('ja-JP') ?? ''}件${noDataNote(market.noDataFrom)}）`;
  // The table compares 市場求人数 only, so it names the last month with a 求人数 (the lane says
  // when the 閲覧者指標 runs further).
  if (market.reason === 'after_data' && market.lastDataMonth) return `データなし（市場求人数は${formatMonthJa(market.lastDataMonth)}まで）`;
  return 'データなし';
}
/** Text for the screen-reader live region of the market lane. */
function marketStatusText(state: MarketState): string {
  if (state.status === 'loading') return state.retrying ? '市場データを取り直しています' : '市場データを読み込んでいます';
  if (state.status === 'error') return '市場データの取得に失敗しました。「市場データを再取得」で取り直せます';
  if (state.rows?.length) return `市場データを表示しました（${state.title}・${state.prefecture}）`;
  return '';
}
function realBillingText(row: PeriodRow): string | null {
  if (!row.billing.connected) return null;
  if (row.billing.conflict) return 'HRハッカーの実績と課金CSVが重なっています（どちらも合計していません）';
  if (row.billing.overlapping) return '期間が重なる課金あり（合計していません）';
  if (row.billing.yen === null) return row.billing.missingAmount ? '金額の記載なし' : row.billing.entries === 0 ? null : 'この期間の課金データなし';
  return `${row.billing.fictional ? `${DEMO_BILLING_LABEL} ` : ''}${row.billing.prorated ? '約' : ''}${formatYen(row.billing.yen)}${row.billing.missingAmount ? '（金額の記載がない期間あり）' : ''}`;
}
/**
 * The 課金額 cell: real billing only. The dummy billing is shown in the 課金 lane but never added
 * up here, so a period with only the dummy billing says there is no real billing data.
 */
export function billingText(row: PeriodRow): string {
  // A version row with no whole known day: its acquisition day is counted in another row.
  if (row.kind === 'period' && row.days === 0) return '—';
  const real = realBillingText(row);
  if (real) return row.dummyBilling ? `${real}（${DUMMY_BILLING_LABEL}は合計に入れていません）` : real;
  if (row.dummyBilling) return `実際の課金データなし（${DUMMY_BILLING_LABEL}は合計しません）`;
  return row.billing.connected ? 'この期間の課金データなし（0円という意味ではありません）' : '課金データなし（0円という意味ではありません）';
}

/** The 選んだ版 panel's date: the acquisition day, and between which acquisitions it changed. */
export function selectionDateText(change: VersionChange, captured: boolean): string {
  if (!captured) return `${formatDay(change.date)} から（媒体の掲載日時）`;
  if (change.index === 0 || change.previousDate === null) return `${formatDay(change.date)} に取得（最初の取得）`;
  const status = boundaryStatus(change);
  if (status === 'changed') return `${formatDay(change.date)} に取得（前回の取得 ${formatDay(change.previousDate)} 以降に変化）`;
  if (status === 'same') return `${formatDay(change.date)} に取得（前回の取得 ${formatDay(change.previousDate)} から変化は見つかっていません）`;
  return `${formatDay(change.date)} に取得（前回の取得 ${formatDay(change.previousDate)} と比べられない項目があります）`;
}

function axisMonths(range: TimelineRange) {
  const ticks: { date: string; label: string }[] = [];
  let cursor = `${range.start.slice(0, 7)}-01`;
  if (cursor < range.start) cursor = addDays(`${range.start.slice(0, 7)}-01`, 32).slice(0, 7) + '-01';
  while (cursor <= range.end && ticks.length < 60) {
    ticks.push({ date: cursor, label: formatMonth(cursor.slice(0, 7)) });
    cursor = addDays(cursor, 32).slice(0, 7) + '-01';
  }
  // Long ranges: label every few months so the labels do not run into each other.
  const step = Math.ceil(ticks.length / 12);
  return ticks.map((tick, index) => index % step === 0 ? tick : { ...tick, label: '' });
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
  /** Show the dummy billing (仮の課金データ) in the 課金 lane. Default DUMMY_BILLING_ENABLED. */
  showDummyBilling?: boolean | undefined;
}

export function JobTimeline(props: JobTimelineProps) {
  return <JobTimelineForJob key={props.job.id} {...props} />;
}

function JobTimelineForJob({ job, billing: injected, marketMode = 'api', onOpenVersion, onCompareVersions, now, showDummyBilling = DUMMY_BILLING_ENABLED }: JobTimelineProps) {
  const asOf = asOfDate(job, now);
  const billing = useMemo(() => billingEntries(job, injected, { asOf, dummy: showDummyBilling }), [job, injected, asOf, showDummyBilling]);
  const periods = useMemo(() => buildPeriods(job, asOf), [job, asOf]);
  const spans = useMemo(() => uncertainSpans(job, asOf, periods), [job, asOf, periods]);
  const changes = useMemo(() => versionChanges(job), [job]);
  const range = useMemo(() => timelineRange(job, asOf, billing), [job, asOf, billing]);
  const retryButton = useRef<HTMLButtonElement>(null);
  const titleSelect = useRef<HTMLSelectElement>(null);
  const market = useTimelineMarket(job, marketMode, { retryButton, titleSelect });
  const rows = useMemo(() => periodRows(job, { asOf, billing: injected, market: market.state.rows, dummyBilling: showDummyBilling }), [job, asOf, injected, market.state.rows, showDummyBilling]);
  const [granularity, setGranularity] = useState<Granularity>('week');
  const [selected, setSelected] = useState<string | null>(null);
  // After a mark or period is chosen, bring the 選んだ版 panel into view (it can sit below the fold).
  const selectionPanel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const panel = selectionPanel.current;
    if (selected && panel && 'scrollIntoView' in panel) panel.scrollIntoView({ block: 'nearest' });
  }, [selected]);
  const applications = job.overallApplications;
  const buckets = useMemo(() => applicationBuckets(applications?.byDate, granularity), [applications, granularity]);
  // While a new choice loads, the chart keeps the previous rows (staleRows) so it is not torn down.
  const laneRows = market.state.rows ?? (market.state.status === 'loading' ? market.state.staleRows : null);
  const lane = useMemo(() => range && laneRows ? marketLane(laneRows, range) : null, [range, laneRows]);
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
  const hasDummyBilling = billing.some(isDummyBilling);
  const hrhBilling = billing.some(entry => entry.source === 'hrhacker');
  const fictionalBilling = billing.some(entry => entry.fictional === true);
  // Name only the sources that have rows on this job: an AirWork job with dummy billing only
  // must not suggest the made-up amounts come from HRハッカー.
  const billingSource = [
    hrhBilling ? fictionalBilling ? `HRハッカーの期間別実績（${DEMO_BILLING_LABEL}）` : 'HRハッカーの期間別実績' : '',
    csvBilling ? '読み込んだ課金CSV' : '',
    hasDummyBilling ? DUMMY_BILLING_LABEL : '',
  ].filter(Boolean).join('・') || '出典なし';
  const outside = applicationsOutsidePeriods(job, rows);
  const conflict = billingConflict(billing);
  const multi = applications?.multiListing;
  const multiTotal = multi ? Object.values(multi.byDate).reduce((sum, count) => sum + count, 0) + multi.missingDate : 0;
  const unsure = rows.filter(row => row.kind === 'between' || row.kind === 'unacquired').reduce((sum, row) => sum + (row.applications ?? 0), 0);
  const marketMeta = market.state.meta;
  const reasonClasses = classifyApplicationReasons(job.applicantReasons);
  const reasonUnit = reasonClasses?.unit === 'text' ? '記述' : '応募';
  const reasonPeriods = reasonClasses ? reasonsByPeriod(reasonClasses.applications, rows.filter(row => !row.afterCounts).map(row => ({ key: row.key, start: row.start, end: row.end ?? addDays(asOf, 1) }))) : null;
  const reasonOf = (key: string): ReasonTally | null => reasonPeriods?.periods.find(period => period.key === key)?.tally ?? null;

  return <section className="jt-timeline" aria-label="タイムライン">
    <header className="jt-heading">
      <div><h2>タイムライン</h2><p className="jc-muted">{formatDay(range.start)} 〜 {formatDay(range.end)}（応募は {formatDay(asOf)} 時点）</p></div>
      <div className="jt-scope">応募は HubSpot に記録されたものだけです。<InfoTip label="並べて見るための表示です">
        <p>同じ時期に起きたことを並べて表示しています。応募が増えた・減った理由を示すものではありません。</p>
        <p>応募件数は HubSpot に記録された応募日で数えています。媒体上のすべての応募ではなく、どの版を見て応募したかは分かりません。</p>
        <p>掲載が変わった日は分かりません。期間比較表は求人データを取得した日で区切り、前後の取得で内容が違うときは「取得日A〜取得日Bの間に変化」として、その間の応募を前後どちらの期間にも入れていません。取得した日も、取得した時刻の前後で変わった可能性があるため、この間に含めます。最後に取得した日より後は「未取得」です。</p>
        <p>期間の長さが違うので「1日あたり」で並べて確認してください。{MIN_RATE_DAYS}日に満たない期間は1日あたりを出さず、比べません（求人の横断比較と同じ扱い）。</p>
      </InfoTip></div>
    </header>

    <div className="jt-lanes">
      <div className="jt-axis" aria-hidden="true"><div className="jt-lane-head" /><div className="jt-track">{ticks.map(tick => <span key={tick.date} style={{ left: `${positionOf(tick.date, range).toFixed(3)}%` }}>{tick.label}</span>)}</div></div>

      <Lane title="掲載期間" source={captured ? '求人データを取得した日（掲載日は不明）' : '媒体の掲載日時'}>
        {periods.map(period => <button type="button" key={period.versionId} className={`jt-period ${period.basis === 'captured' ? 'jt-basis-captured' : `jt-cert-${period.certainty}`}`} aria-pressed={selected === period.versionId}
          style={span(range, period.start, period.end ?? addDays(asOf, 1))} onClick={() => { setSelected(period.versionId); }}
          title={period.basis === 'captured' ? `${period.label}: ${formatDay(period.start)}に取得${period.days > 1 && period.end ? `（${formatDay(addDays(period.end, -1))}まで同じ内容）` : ''}` : `${period.label}: ${formatDay(period.start)}〜${period.end ? formatDay(addDays(period.end, -1)) : '継続中'}（媒体の掲載日時）`}>
          <span>{period.label}</span></button>)}
        {spans.map(item => <div key={`${item.kind}-${item.fromVersionId}`} className={item.kind === 'between' ? 'jt-zone' : 'jt-unacquired'} style={span(range, item.start, item.end)}
          title={item.kind === 'between' ? `取得日${formatDay(item.from)}〜${formatDay(item.to ?? item.from)}の間${item.reason === 'changed' ? 'に変化' : '（変化したか確認できない）'}。どちらの内容か分からない期間です` : `最後の取得（${formatDay(item.from)}）より後は未取得です`}>
          <span>{item.kind === 'between' ? '取得日の間' : '未取得'}</span></div>)}
        {!periods.length && <p className="jt-empty">掲載期間は未取得です</p>}
      </Lane>

      <Lane title="給与" source="本文の「給与」の行から読み取り">
        {changes.map((change, index) => {
          const period = periods[index];
          if (!period) return null;
          const info = change.salary;
          const readable = info && info.kind === salaryKind && info.min !== null;
          const height = readable && high > low ? 18 + (((info.min ?? low) - low) / (high - low)) * 40 : 30;
          const className = `${readable ? '' : ' jt-salary-unknown'}${change.salaryChanged ? ' jt-salary-changed' : ''}`;
          // The line spans the period; the label starts at the period start and stays inside the
          // track even when the period is only a few days long (the latest version).
          return <div key={change.versionId} className="jt-salary-item">
            <div className={`jt-salary${className}`} aria-hidden="true" style={{ ...span(range, period.start, period.end ?? addDays(asOf, 1)), bottom: `calc(${String(height)}% + 14px)` }} />
            <div className={`jt-salary-label${className}`} style={{ ...pinned(period.start, range), bottom: `${String(height)}%` }}
              title={`${info ? `${change.label}: ${info.raw || '記載なし'}` : `${change.label}: 給与の記載なし`}${change.salaryDirection ? `（${salaryWord[change.salaryDirection]}）` : ''}`}>
              {change.salaryDirection && <i className={`jt-salary-dir jt-salary-${change.salaryDirection}`} role="img" aria-label={salaryWord[change.salaryDirection]}>{salaryMark[change.salaryDirection]}</i>}<span>{salaryLabel(info)}</span>
            </div>
          </div>;
        })}
      </Lane>

      <Lane title="本文" source="前の版との行の比較">
        {changes.map(change => <button type="button" key={change.versionId} className={`jt-mark${change.index > 0 && change.bodyStatus === 'changed' ? ' jt-mark-changed' : ''}`} aria-pressed={selected === change.versionId}
          style={pinned(change.date, range)} onClick={() => { setSelected(change.versionId); }}
          aria-label={`${change.label}の本文：${bodyMark(change).spoken}`}>
          {bodyMark(change).text}
        </button>)}
      </Lane>

      <Lane title="画像" source="前の版との画像の比較（差し替え・並び順・中身）">
        {changes.map(change => <button type="button" key={change.versionId} className={`jt-mark jt-image-${change.imageChange}`} aria-pressed={selected === change.versionId}
          style={pinned(change.date, range)} onClick={() => { setSelected(change.versionId); }}
          aria-label={`${change.label}の画像：${IMAGE_CHANGE_MARK[change.imageChange].spoken}`}>
          {IMAGE_CHANGE_MARK[change.imageChange].text}
        </button>)}
      </Lane>

      <Lane title="課金" source={billingSource}>
        {billing.length ? billing.map(entry => {
          const dummy = isDummyBilling(entry);
          return <div key={`${entry.source}-${String(entry.sourceRow ?? '')}-${entry.start}-${entry.end}`} className={`jt-billing jt-billing-${entry.source}`} style={span(range, entry.start, addDays(entry.end, 1))}
            title={`${dummy ? `${DUMMY_BILLING_LABEL} ` : entry.fictional ? `${DEMO_BILLING_LABEL} ` : ''}${formatDay(entry.start)}〜${formatDay(entry.end)}: ${entry.amountYen === null ? '金額の記載なし' : `${entry.amountYen.toLocaleString('ja-JP')}円`}${entry.taxIncluded === true ? '（税込）' : entry.taxIncluded === false ? '（税抜）' : ''}${entry.plan && !dummy ? ` · ${entry.plan}` : ''}`}>
            <span>{dummy ? 'ダミー ' : entry.fictional ? '架空 ' : ''}{entry.amountYen === null ? '金額なし' : formatYen(entry.amountYen)}</span></div>;
        })
          : <p className="jt-empty jt-unconnected">課金データなし（0円という意味ではありません）</p>}
      </Lane>
      {fictionalBilling && <p className="jt-demo-billing" role="note">{DEMO_BILLING_NOTE}</p>}
      {hasDummyBilling && <p className="jt-dummy-billing" role="note">{DUMMY_BILLING_NOTE}上の「課金」の段では金額に「ダミー」と付けています。期間比較表の課金額の合計には入れていません。</p>}
      {conflict && <p className="jt-billing-conflict" role="note">HRハッカーの期間別実績と読み込んだ課金CSVに、同じ日を含む課金があります。どちらの金額が正しいか決められないため、重なる期間は期間比較表で合計していません。</p>}
      {csvBilling && <p className="jt-volatile" role="note">読み込んだ課金CSVはこの画面を開いている間だけ表示します。再読み込みすると消えます。</p>}

      <Lane title="応募" source="応募日ごとの件数" className="jt-lane-chart">
        {applicationOption ? <EChart option={applicationOption} testId="jt-applications" height={72} renderer="svg" /> : <p className="jt-empty">{!applications?.byDate ? '応募日別の件数は未取得です' : '応募日の分かる応募はありません'}</p>}
      </Lane>
      <div className="jt-lane-tools">
        <div className="jt-granularity" role="group" aria-label="応募の集計単位">{(['day', 'week', 'month'] as const).map(value => <button type="button" key={value} aria-pressed={granularity === value} onClick={() => { setGranularity(value); }}>{granularityLabel[value]}ごと</button>)}</div>
        {applications && applications.missingDate > 0 && <span>応募日が分からない応募 {applications.missingDate}件 はグラフに含めていません</span>}
        {outside > 0 && <span>{captured ? `最初に取得した日まで（その日を含む）の応募 ${String(outside)}件` : `最初の掲載より前の日付の応募 ${String(outside)}件`}</span>}
        {unsure > 0 && <span>取得日の間・最後の取得より後の応募 {unsure}件 は、どちらの内容への応募か分からないため期間比較表の各版には入れていません</span>}
        {multi ? multiTotal > 0 && <span>複数の求人に関連する応募 {multiTotal}件 は期間比較表に入れていません</span>
          : applications?.byDate && <span>複数の求人に関連する応募を見分ける情報を取得していないため、期間比較表の件数に含まれている場合があります</span>}
      </div>

      <Lane title="応募理由" source="応募日ごとの分類（選択済みと推定）">
        {!reasonClasses ? <p className="jt-empty">応募理由は未取得です（0件という意味ではありません）</p>
          : !reasonClasses.applications.length ? <p className="jt-empty">{job.applicantReasons?.selections === null ? '応募理由の文の記録はありません（分類の選択はこのデータでは未取得です。0件という意味ではありません）' : '応募理由の記録はありません'}</p>
            : rows.filter(row => !row.afterCounts).map(row => {
              const result = reasonOf(row.key);
              if (!result || result.n === 0) return null;
              const top = topReasons(result, 1)[0];
              return <div key={row.key} className={`jt-reason${row.kind !== 'period' ? ' jt-reason-zone' : ''}`} style={span(range, row.start, row.end ?? addDays(asOf, 1))}
                title={`${row.label}: ${reasonTallyText(result, reasonUnit)}`}>
                <span>n={result.n}{top ? ` ${top.category}${String(top.total)}件（${basisText(top)}）` : ''}</span></div>;
            })}
      </Lane>
      {reasonPeriods && (reasonPeriods.undated > 0 || reasonPeriods.outside > 0 || job.applicantReasons?.truncated === true || job.applicantReasons?.selections === null) && <div className="jt-lane-tools">
        {job.applicantReasons?.selections === null && reasonClasses?.applications.length ? <span>このデータでは分類の選択を取得していないため、数はすべて文から言葉で推定したものです（選択済みは0件ではなく未取得）</span> : null}
        {job.applicantReasons?.truncated === true && <span>記述が多く一部しか読み込んでいないため、期間ごとの応募理由の数は実際より少ないことがあります</span>}
        {reasonPeriods.undated > 0 && <span>応募日が分からない応募理由 {reasonPeriods.undated}件 は段と表に入れていません</span>}
        {reasonPeriods.outside > 0 && <span>表のどの期間にも入らない日付の応募理由 {reasonPeriods.outside}件 は段と表に入れていません</span>}
      </div>}

      <Lane title="市場" source="Indeed（都道府県・職種の月ごと）" className="jt-lane-chart">
        {lane?.noDataFrom && <div className="jt-nodata" style={span(range, `${lane.noDataFrom}-01` > range.start ? `${lane.noDataFrom}-01` : range.start, addDays(range.end, 1))} title={lane.lastDataMonth ? marketDataUntil(lane.lastDataMonth) : undefined}>データなし</div>}
        {marketOption && market.state.status === 'loading' && <p className="jt-empty jt-market-loading" role="status">市場データを取得中…</p>}
        {marketOption ? <EChart option={marketOption} testId="jt-market" height={72} renderer="svg" />
          : <p className="jt-empty">{market.state.status === 'loading' ? '市場データを取得中…' : market.state.status === 'error' ? '市場データを取得できませんでした' : !market.state.title ? '職種を選ぶと市場の動きを表示します' : !market.state.prefecture ? '都道府県を選ぶと市場の動きを表示します' : 'この職種・都道府県の市場データはありません'}</p>}
      </Lane>
      <div className="jt-lane-tools jt-market-tools">
        <div className="jt-market-key"><i className="jt-key jt-key-jobs" />市場求人数 <i className="jt-key jt-key-viewers" /><InfoTip className="jc-infotip-left" label="Indeed閲覧者指標"><p>{plainWording(marketMeta?.ctk_basis ?? 'Indeed閲覧者指標は、求職者の人数やこの求人への応募数ではありません。')}</p></InfoTip></div>
        {marketMeta && <>
          <label>職種<select ref={titleSelect} value={market.state.title} onChange={event => { market.choose({ title: event.target.value }); }}><option value="">選んでください</option>{marketMeta.titles.map(value => <option key={value}>{value}</option>)}</select></label>
          <label>都道府県<select value={market.state.prefecture} onChange={event => { market.choose({ prefecture: event.target.value }); }}><option value="">選んでください</option>{marketMeta.prefectures.map(value => <option key={value}>{value}</option>)}</select></label>
          <span className="jt-choice">{market.state.titleBy === 'auto-exact' ? '求人名と同じ職種を自動で選びました' : market.state.titleBy === 'auto-partial' ? '求人名に含まれる職種を自動で選びました。違う場合は選び直してください' : market.state.titleBy === 'user' ? '手で選んだ職種です' : '求人名から職種を決められませんでした'}</span>
          <span className="jt-choice">{market.state.prefectureBy === 'auto' ? `勤務地から${market.state.prefecture}を自動で選びました` : market.state.prefectureBy === 'user' ? '手で選んだ都道府県です' : '勤務地から都道府県を決められませんでした'}</span>
        </>}
        {(market.state.status === 'error' || market.state.retrying) && <button type="button" ref={retryButton} className="jc-button" aria-disabled={market.state.status === 'loading'} onClick={market.retry}>市場データを再取得</button>}
        <span className="jc-visually-hidden" role="status">{marketStatusText(market.state)}</span>
        {lane?.lastDataMonth && <span className="jt-market-until">{marketDataUntil(lane.lastDataMonth)}{lane.lastJobsMonth && lane.lastJobsMonth < lane.lastDataMonth ? `。市場求人数は${formatMonthJa(lane.lastJobsMonth)}まで` : ''}{lane.noDataFrom ? `。${formatMonthJa(lane.noDataFrom)}以降はデータなしとして表示しています` : ''}</span>}
      </div>
    </div>

    {selectedChange && <div className="jt-selection" ref={selectionPanel} role="region" aria-label="選んだ版">
      <strong>{selectedChange.label}</strong><span>{selectionDateText(selectedChange, periods[selectedChange.index]?.basis === 'captured')} · 給与 {salaryLabel(selectedChange.salary)}</span>
      {onOpenVersion && <button type="button" className="jc-button" onClick={() => { onOpenVersion(selectedChange.versionId); }}>本文・画像を開く</button>}
      {onCompareVersions && previous && <button type="button" className="jc-button" onClick={() => { onCompareVersions(previous.versionId, selectedChange.versionId); }}>前の版との差分を開く</button>}
      {previous && selectedChange.bodyStatus === 'unchanged' && selectedChange.imageChange === 'same' && <span>前の版から本文・画像の変更はありません</span>}
      {previous && selectedChange.bodyStatus === 'unchanged' && selectedChange.imageChange !== 'same' && <span>本文は前の版と同じです。画像：{IMAGE_CHANGE_MARK[selectedChange.imageChange].spoken}</span>}
    </div>}
    <div className="jt-legend" role="group" aria-label="凡例">{captured
      ? <><span className="jt-legend-title">掲載日は不明（取得日で表示）：</span><span><i className="jt-key jt-basis-captured" />取得した日の内容</span><span><i className="jt-key jt-key-zone" />取得日の間（どちらの内容か分からない）</span><span><i className="jt-key jt-key-unacquired" />最後の取得より後（未取得）</span></>
      : <><span className="jt-legend-title">掲載日：</span><span><i className="jt-key jt-cert-confirmed" />媒体の掲載日時</span></>}</div>

    <section className="jt-periods" aria-label="期間比較表">
      <h3>期間比較表</h3>
      {rows.length === 0 ? <p className="jc-notice">掲載期間が取得できていないため、期間ごとの比較はできません。</p>
        : <div className="jt-table-scroll" role="region" aria-label="期間比較表の数値" tabIndex={0}><table>
        <thead><tr><th scope="col">期間</th><th scope="col">日数</th><th scope="col">応募件数</th><th scope="col">1日あたり</th><th scope="col">課金額</th><th scope="col">市場求人数の同時期変化</th></tr></thead>
        <tbody>{rows.map(row => <tr key={row.key} className={row.kind !== 'period' ? `jt-gap-row jt-row-${row.kind}` : selected === row.versionId ? 'jt-row-selected' : undefined} aria-current={row.kind === 'period' && selected === row.versionId ? 'true' : undefined}>
          <th scope="row">{row.versionId ? <button type="button" className="jc-text-button" aria-pressed={selected === row.versionId} onClick={() => { setSelected(row.versionId); }}>{row.label}</button> : row.label}{row.kind === 'period' && selected === row.versionId && <span className="jt-selected-tag">選択中</span>}<small>{row.detail}</small></th>
          <td>{row.afterCounts && row.days === 0 ? '—' : `${String(row.days)}日`}</td>
          <td>{row.afterCounts ? '応募集計の取得後に始まった期間' : row.applications === null ? '未取得' : row.kind === 'period' && row.days === 0 ? '別の行に数えます' : `${String(row.applications)}件`}</td>
          <td>{row.afterCounts ? '—' : row.applications === null ? '未取得' : row.kind === 'between' || row.kind === 'unacquired' ? '比べません' : row.shortPeriod ? '期間が短いため比べません' : formatPerDay(row.perDay)}</td>
          <td>{billingText(row)}</td>
          <td>{marketText(row.market, market.state.status)}</td>
        </tr>)}</tbody>
      </table></div>}
      {rows.some(row => row.afterCounts) && <p className="jt-table-note">「応募集計の取得後に始まった期間」は、応募件数を{formatDay(asOf)}に取得した後に始まった期間です。0件という意味ではありません。</p>}
      {rows.some(row => !row.afterCounts && row.applications === null) && <p className="jt-table-note">「未取得」は応募日ごとの件数を取得していないという意味です。0件という意味ではありません。</p>}
      {rows.some(row => row.kind === 'between' || row.kind === 'unacquired') && <p className="jt-table-note">「取得日の間」と「最後の取得より後」の行の応募は、どちらの内容を見た応募か分からないため、前後の期間に入れず別に数えています。応募は日付だけで記録されていて、取得した日も取得した時刻の前後で内容が変わった可能性があるため、取得した日の応募もこれらの行に入れています。1日あたりは比べません。</p>}
      {rows.some(row => !row.afterCounts && row.applications !== null && row.shortPeriod && (row.kind === 'period' || row.kind === 'gap')) && <p className="jt-table-note">「期間が短いため比べません」は、{MIN_RATE_DAYS}日に満たない期間です。1日や2日の件数を1日あたりに直すと大きく振れるため、比べません。</p>}
      {rows.some(row => row.billing.connected && row.billing.prorated) && <p className="jt-table-note">「約」の付いた課金額は、課金の期間と版の期間がずれているため、日数で割って配分した金額です。</p>}
    </section>
    {reasonClasses && rows.length > 0 && <section className="jt-periods jt-reasons" aria-label="期間ごとの応募理由">
      <h3>期間ごとの応募理由</h3>
      <p className="jc-muted">期間比較表と同じ期間（取得日の間を含む）で、応募日ごとに応募理由の分類を数えています。nは応募理由の記録がある{reasonUnit}の件数です。分類ごとの数の後ろの「選択」はHubSpotで分類が選ばれた件数、「推定」は文から言葉で推定した件数です。1件が複数の分類に入ることがあります。nが{MIN_SHARE_N}件に満たない期間は割合を出しません。</p>
      <div className="jt-table-scroll" role="region" aria-label="期間ごとの応募理由の数値" tabIndex={0}><table>
        <thead><tr><th scope="col">期間</th><th scope="col">n</th><th scope="col">選択済み・推定・分類できない</th>{REASON_CATEGORIES.map(category => <th scope="col" key={category}>{category}</th>)}</tr></thead>
        <tbody>{rows.map(row => {
          const result = row.afterCounts ? null : reasonOf(row.key);
          return <tr key={row.key} className={row.kind !== 'period' ? `jt-gap-row jt-row-${row.kind}` : undefined}>
            <th scope="row">{row.label}<small>{row.detail}</small></th>
            {!result ? <td colSpan={2 + REASON_CATEGORIES.length}>応募集計の取得後に始まった期間</td> : <>
              <td>n={result.n}</td>
              <td>{result.n ? `選択済み${String(result.selectedN)}件・推定${String(result.estimatedN)}件・分類できない${String(result.unclassified)}件` : '記録なし'}</td>
              {result.counts.map(count => { const share = shareText(count.total, result.n); const basis = basisText(count); return <td key={count.category}>{result.n ? `${String(count.total)}件${share ? `（${share}）` : ''}${basis ? ` ${basis}` : ''}` : '—'}</td>; })}
            </>}
          </tr>;
        })}</tbody>
      </table></div>
      <p className="jt-table-note">件数は並べて見るためのものです。ある期間に件数が多い分類があっても、それがその期間の文面によるものかどうかは、この数だけでは分かりません。複数の求人に関連する応募を見分けられないため、期間比較表の応募件数と合わないことがあります。</p>
    </section>}
  </section>;
}
