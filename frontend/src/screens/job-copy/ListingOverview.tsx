import { CompetitorMonthDetails } from './CompetitorHistory';
import { PhoneHearingContent, usePhoneHearing } from './PhoneHearing';
import type { PhoneHearing as Hearing } from '../../generated/PhoneHearing';
import { formatDateJst } from './format';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import type { JobCopyRecord } from './data';
import { RetryApplicants } from './RetryApplicants';
import { applicationsCutoffDate, noApplicationDatesText } from './detailBandModel';
import { buildLifeline, columnWidth, fitsColumn, changeMarkerLabel, changeMarkerNumbers, weekIndex } from './lifelineModel';
import type { Lifeline, LifelineWeek } from './lifelineModel';
import { HEARING_TEXT_SOURCES, reasonSourceLabels } from './applicantReasonsModel';
import { ReasonCategorySummary } from './ReasonCategorySummary';
import { maskPersonalDetails } from './personalText';
import { selectedCategory } from './reasonCategories';
import { countedApplications, eventChecks, eventCostNotes, eventCosts, eventCounts, md, resultText } from './listingEventsModel';
import type { EventCosts, EventCounts, ListingEvent, MarketInput } from './listingEventsModel';
import { chooseMarket, designatedTitle, indeedTitleState } from './marketMatch';
import { marketRows } from './marketChartModel';
import type { MarketRow } from './marketChartModel';
import { useMarketFetch } from './marketSource';
import type { BillingEntry } from './timelineModel';
import { InfoTip } from './InfoTip';
import { ListingTasks } from './ListingTasks';
import type { ListingTaskOptions } from './ListingTasks';
import { BILLING_EVENT_MIN_PCT, MARKET_CAUTION_PCT } from './listingEventsModel';
import './listing-overview.css';

/** 出来事の「数え方」（見出しの ⓘ に畳む）。 */
function EventRules() {
  return <>
    <p>出来事の日の前の4週と後の4週の応募を、件数で比べます。応募の数はほかの要因でも動くため、比べた差で良し悪しや原因は決めません。</p>
    <ul className="lo-rules">
      <li>課金：前後4週にほかの課金の変更（開始・停止・{BILLING_EVENT_MIN_PCT}%以上の増減）があれば「要注意」、なければ「小さい」。</li>
      <li>市場：職種を指定した求人だけ。前後4週で競合の求人数が{MARKET_CAUTION_PCT}%以上動いていれば「要注意」。</li>
      <li>ほかの変更：前後4週に別の出来事があれば「要注意」。</li>
    </ul>
  </>;
}

/** 出来事の「市場」の確認に使う市場の数字。職種を指定した求人だけ取得する。 */
export function useListingMarket(job: JobCopyRecord, mode: 'api' | 'demo'): MarketInput {
  const fetchMarket = useMarketFetch(mode);
  const state = indeedTitleState(job);
  const title = designatedTitle(job);
  const key = `${job.id}|${title ?? ''}`;
  const [loaded, setLoaded] = useState<{ key: string; rows: readonly MarketRow[] } | null>(null);
  useEffect(() => {
    if (!title) return;
    const run = { cancelled: false };
    const stopped = () => run.cancelled;
    void (async () => {
      try {
        const list = await fetchMarket('', '');
        if (stopped() || !list.ok) return;
        const choice = chooseMarket(job, list.data.prefectures);
        if (!choice.prefecture || !list.data.titles.includes(title)) return;
        const series = await fetchMarket(title, choice.prefecture);
        if (stopped() || !series.ok) return;
        setLoaded({ key, rows: series.data.series ? marketRows(series.data.series) : [] });
      } catch { /* 市場の数字が取れなくても、ほかの表示は続ける */ }
    })();
    return () => { run.cancelled = true; };
  }, [job, title, key, fetchMarket]);
  return state === 'set' ? { state: 'set', rows: loaded?.key === key ? loaded.rows : null } : { state };
}

export type ListingDrawerKind = 'who' | 'market' | 'billing';

interface Props {
  job: JobCopyRecord;
  events: readonly ListingEvent[];
  billing: readonly BillingEntry[];
  market: MarketInput;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onCompare: (left: string | undefined, right: string | undefined) => void;
  onOpenDrawer: (kind: ListingDrawerKind, event: ListingEvent | null) => void;
  /** 「この求人でやること」の札（手直し・リクロジ向けの案・電話ヒアリング・くわしい分析）。出来事の上に並べる。 */
  tasks?: ListingTaskOptions;
  /** ライフラインのすぐ下に置くもの（市場の今）。 */
  marketSlot?: ReactNode;
}

const num = (value: number | null) => value === null ? '—' : value.toLocaleString('ja-JP');
const yen = (value: number | null) => value === null ? '—' : `¥${Math.round(value).toLocaleString('ja-JP')}`;
/** ライフラインの1列に収まる短い数（列が狭いときに使う）。10万以上は「万」「億」で縮める（正確な値は title に出す）。 */
export const shortNumber = (value: number | null, prefix = '') => {
  if (value === null) return '—';
  const abs = Math.abs(value), sign = value < 0 ? '-' : '';
  const unit = (n: number, u: string) => `${sign}${prefix}${(n >= 100 ? Math.round(n) : Math.round(n * 10) / 10).toLocaleString('ja-JP')}${u}`;
  if (abs >= 1e8) return unit(abs / 1e8, '億');
  if (abs >= 1e5) return unit(abs / 1e4, '万');
  return `${sign}${prefix}${Math.round(abs).toLocaleString('ja-JP')}`;
};

/** 横の時間軸。1週 = 1列。週が少ないときは横幅いっぱいに広げ、多いときは最小の幅で横にスクロールする。開いたときは最新の週が見える位置にする。 */
export function LifelineChart({ lifeline, job, selectedWeek, onSelect }: { lifeline: Lifeline; job: JobCopyRecord; selectedWeek: number | null; onSelect: (id: string) => void }) {
  const scroller = useRef<HTMLDivElement | null>(null);
  const [visible, setVisible] = useState<number | null>(null);
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return undefined;
    // clientWidth は整数に丸める（1159.5 → 1160 もある）ので、表示倍率によっては表が 0.5px はみ出して要らない横スクロールが出る。実寸を切り捨てて使う。
    const measure = () => { setVisible(Math.floor(element.getBoundingClientRect().width) || null); };
    measure();
    if (typeof ResizeObserver === 'undefined') return undefined;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => { observer.disconnect(); };
  }, []);
  const column = columnWidth(visible, lifeline.weeks.length);
  const width = lifeline.weeks.length * column;
  // 最新の週へ寄せるのは、求人・週の数が変わったときと、最初に幅を測れたとき（非表示で作られて表示されたときを含む）だけ。
  // 幅が変わっただけでは寄せない（手で動かした位置を保つ）。
  const measured = visible !== null;
  useEffect(() => { const element = scroller.current; if (element) element.scrollLeft = element.scrollWidth; }, [lifeline.weeks.length, job.id, measured]);
  useEffect(() => {
    const element = scroller.current;
    if (element && selectedWeek !== null) element.scrollLeft = Math.max(0, selectedWeek * column - element.clientWidth / 2 + column / 2);
  }, [selectedWeek, column]);
  // 列に収まる数はそのまま出し、収まらないときだけ「万」「億」で縮める。
  const fit = (full: string, short: string) => fitsColumn(full, column) ? full : short;
  const media = lifeline.mediaLabel;
  const noPerf = lifeline.hasPerformance ? '' : media === 'リクロジ' ? 'リクロジの実績はまだ取得していません' : media === 'AirWork' ? '課金データなし' : 'この求人の Indeed の実績データはありません';
  const allRows: { key: string; label: string; cell: (week: LifelineWeek) => string; short: (week: LifelineWeek) => string; empty: string }[] = [
    { key: 'billing', label: '課金（週）', cell: week => yen(week.billing), short: week => fit(yen(week.billing), shortNumber(week.billing, '¥')), empty: noPerf },
    { key: 'impressions', label: '閲覧数（表示回数）', cell: week => num(week.impressions), short: week => fit(num(week.impressions), shortNumber(week.impressions)), empty: noPerf },
    { key: 'clicks', label: 'クリック数', cell: week => num(week.clicks), short: week => fit(num(week.clicks), shortNumber(week.clicks)), empty: noPerf },
    { key: 'hubspot', label: '応募数（HubSpot に記録された応募日）', cell: week => num(week.hubspotApplications), short: week => fit(num(week.hubspotApplications), shortNumber(week.hubspotApplications)), empty: job.applicationDates === 'loading' ? '' : '応募の日付を取得できていません' },
    { key: 'media', label: `応募数（${media}の集計）`, cell: week => num(week.mediaApplications), short: week => fit(num(week.mediaApplications), shortNumber(week.mediaApplications)), empty: noPerf },
  ];
  // 媒体の実績（課金・閲覧・クリック・媒体の集計の応募）が無い求人は、空の4行を1行にまとめる。
  const perfKeys = ['billing', 'impressions', 'clicks', 'media'];
  const rows = noPerf && allRows.filter(row => perfKeys.includes(row.key)).every(row => lifeline.weeks.every(week => row.cell(week) === '—'))
    ? [{ key: 'performance', label: `課金・閲覧・クリック・応募数（${media}の集計）`, cell: () => '—', short: () => '—', empty: noPerf }, ...allRows.filter(row => !perfKeys.includes(row.key))]
    : allRows;
  // 同じ週に印が重なる（掲載を始めた週がまだ今週など）ときは、名前を縦にずらして重ねない
  const placed: number[] = [];
  const marker = (index: number | null, label: string, tone: string) => {
    if (index === null) return null;
    const stack = placed.filter(at => at === index).length;
    placed.push(index);
    return <div key={label} className={`lf-marker lf-marker-${tone}`} style={{ left: `${String(index * column)}px` }}><span style={stack ? { marginBottom: `${String(stack * 16)}px` } : undefined}>{label}</span></div>;
  };
  return <div className="lf" data-testid="lifeline">
    <div className="lf-labels" aria-hidden="true">
      <div className="lf-head-label">週（月曜）</div>
      <div className="lf-change-label">求人の変更</div>
      {rows.map(row => <div key={row.key} className="lf-row-label">{row.label}</div>)}
    </div>
    <div className="lf-scroll" ref={scroller} tabIndex={0} role="region" aria-label="週ごとの推移（横にスクロールできます）">
      <div className="lf-track" style={{ width: `${String(width)}px`, ['--lf-col' as string]: `${String(column)}px` }}>
        {selectedWeek !== null && <div className="lf-selected" data-testid="lifeline-selected" style={{ left: `${String(selectedWeek * column)}px`, width: `${String(column)}px` }} />}
        {marker(lifeline.startIndex, '掲載開始', 'start')}{marker(lifeline.endIndex, '掲載終了', 'end')}{marker(lifeline.todayIndex, '今日', 'today')}
        <div className="lf-row lf-head">{lifeline.weeks.map(week => <div key={week.start} className="lf-cell" title={`${week.start} の週`}>{md(week.start)}</div>)}</div>
        <div className="lf-row lf-changes">{lifeline.weeks.map(week => {
          const latest = week.changes.at(-1);
          const details = week.changes.map(event => `出来事${String(event.n)} ${event.date} ${event.title}：${event.summary}`).join('\n');
          const label = `${details}${week.changes.length > 1 ? '\n最新の出来事を開きます' : ''}`;
          return <div key={week.start} className="lf-cell">{latest && <button type="button" className="lf-change" title={label} aria-label={label} onClick={() => { onSelect(latest.id); }}><strong className="lf-change-num" data-testid="lifeline-change-num">{changeMarkerNumbers(week.changes)}</strong>{changeMarkerLabel(week.changes).split('・').map((part, index, parts) => <span key={part}>{part}{index < parts.length - 1 ? '・' : ''}</span>)}</button>}</div>;
        })}</div>
        {rows.map(row => <div key={row.key} className="lf-row" role="row" aria-label={row.label}>
          {row.empty && lifeline.weeks.every(week => row.cell(week) === '—') ? <div className="lf-empty">{row.empty}</div> : lifeline.weeks.map(week => <div key={week.start} className={`lf-cell${row.cell(week) === '0' || row.cell(week) === '¥0' ? ' lf-zero' : ''}`} title={row.short(week) === row.cell(week) ? undefined : row.cell(week)}>{row.short(week)}</div>)}
        </div>)}
      </div>
    </div>
    <p className="lf-legend" data-testid="lifeline-legend">「—」はデータを取れていない週、「0」は0件の週です。</p>
  </div>;
}

/** 出来事の前後4週の応募・課金・応募1件あたりの費用。差や判定の言葉は付けず、数字を並べるだけ。「—」の理由は下に1行ずつ出す。 */
export function EventCostTable({ event, counts, costs, asOf }: { event: Pick<ListingEvent, 'date' | 'kind'>; counts: EventCounts; costs: EventCosts; asOf: string | null }) {
  // 見出しは一覧の行（「前は掲載後14日分・後は11日分」）と同じ言い方にする。
  const beforeHead = counts.before !== null && counts.beforeDays < 28 ? `前の4週（掲載後${String(counts.beforeDays)}日分）` : '前の4週';
  const afterHead = counts.after !== null && counts.afterDays < 28 ? `後の4週（${String(counts.afterDays)}日分）` : '後の4週';
  const apps = (value: number | null) => value === null ? '—' : `${value.toLocaleString('ja-JP')}件`;
  // 応募の件数がどちらも無いときは、窓が決まらないので表を出さない（一覧の結果欄に理由が出ている）。
  if (counts.before === null && counts.after === null) return null;
  const notes = eventCostNotes(event, counts, costs, asOf);
  return <div className="lo-costs">
    <table className="lo-cost-table" data-testid="event-costs">
      <thead><tr><th scope="col"><span className="jc-visually-hidden">項目</span></th><th scope="col">{beforeHead}</th><th scope="col">{afterHead}</th></tr></thead>
      <tbody>
        <tr><th scope="row">応募</th><td data-testid="cost-before-applications">{apps(counts.before)}</td><td data-testid="cost-after-applications">{apps(counts.after)}</td></tr>
        <tr><th scope="row">課金</th><td data-testid="cost-before-billing">{yen(costs.before?.billing ?? null)}</td><td data-testid="cost-after-billing">{yen(costs.after?.billing ?? null)}</td></tr>
        <tr><th scope="row">応募1件あたり</th><td data-testid="cost-before-per-application">{yen(costs.before?.perApplication ?? null)}</td><td data-testid="cost-after-per-application">{yen(costs.after?.perApplication ?? null)}</td></tr>
      </tbody>
    </table>
    {notes.length > 0 && <ul className="lo-cost-notes" data-testid="event-cost-notes">{notes.map(note => <li key={note}>{note}</li>)}</ul>}
  </div>;
}

const KIND_LABEL: Record<ListingEvent['kind'], string> = { start: '掲載', end: '掲載', revive: '掲載', content: '求人内容', image: '画像', billing: '課金', application: '応募', competitor: '競合' };

function ApplicationDetails({ job, event, hearing, loading, onShowHearing }: { job: JobCopyRecord; event: ListingEvent; hearing: Hearing | null; loading: boolean; onShowHearing: (applicant: string) => void }) {
  const collection = job.applicantReasons;
  const items = (collection?.items ?? []).filter(item => item.applicant === event.applicationKey && item.text.trim());
  const picked = (collection?.selections ?? []).filter(item => item.applicant === event.applicationKey && selectedCategory(item) !== 'unset');
  const card = hearing?.items.find(card => card.applicant === event.applicationKey);
  const reasons = items.filter(item => !HEARING_TEXT_SOURCES.includes(item.sourceProperty));
  const reasonFields = new Set<string>(reasons.map(item => item.sourceProperty === 'oubodouki' ? '応募動機' : item.sourceProperty === 'genshokumaeshokukaranotenshokuriyuu' ? '転職理由' : item.sourceProperty === 'kinmujoukennoyuusenjuni' ? '勤務条件の優先順位' : ''));
  const fields = card?.fields.filter(field => !reasonFields.has(field.label)) ?? [];
  const recorded = items.length > 0 || picked.length > 0 || Boolean(card);
  return <div className="lo-reasons-of">
    <p className="lo-summary">応募日：{formatDateJst(event.date, '不明')} / 応募媒体：{card?.media ?? job.applicationRows?.find(row => row.key === event.applicationKey)?.media ?? '記録なし'}</p>
    {job.applicationDates === 'failed' ? <p role="alert">応募の記録を読み込めませんでした。<RetryApplicants id={job.hubspotId} /></p> : loading || job.applicationDates === 'loading' ? null : !hearing && job.hubspotId ? <p role="status">電話で聞いた内容を取得できませんでした。時間を置いて、もう一度開いてください。</p> : !recorded && job.applicantReasons?.truncated ? <p>一部だけ表示しているため、この応募の記録を確認できません。</p> : !recorded && <p>この応募には、応募理由や電話で聞いた内容の記録がありません</p>}
    <div className="lo-reasons-of">
    {picked.map(item => <p key={`${item.sourceProperty}-${item.value}`} className="lo-summary"><strong>{reasonSourceLabels[item.sourceProperty] ?? '応募理由の分類'}：</strong>{item.label ?? '分類の名前を確認できません'}</p>)}
    {reasons.map(item => <p key={item.id} className="lo-summary"><strong>{reasonSourceLabels[item.sourceProperty] ?? '応募理由'}：</strong>{maskPersonalDetails(item.text)}</p>)}
    </div>
    {card && <>
      <h3>電話ヒアリング</h3><dl className="lo-application-fields">{fields.map(field => <div key={field.label}><dt>{field.label}</dt><dd>{maskPersonalDetails(field.text)}</dd></div>)}</dl>
      {card.notes.length > 0 && <section aria-label="メモ"><h3>メモ</h3>{card.notes.map((note, index) => <p key={index}>{maskPersonalDetails(note.text)}</p>)}</section>}
      <button type="button" className="jc-button" onClick={() => { onShowHearing(card.applicant); }}>電話ヒアリングのこの応募へ</button>
    </>}
  </div>;
}

/** 求人の応募理由（ヒアリング内容）の内訳。記録が無い求人は、無いと言う。 */
export function ListingReasons({ job }: { job: JobCopyRecord }) {
  const collection = job.applicantReasons;
  // 読み込み中は上の帯の 1 か所だけに出す。ここは何も出さない。
  const body = job.applicationDates === 'loading' ? null
    : job.applicationDates === 'failed' ? <p className="lo-note" role="alert">応募理由を読み込めませんでした。<RetryApplicants id={job.hubspotId} /></p>
    : !collection?.available || (!collection.items.some(item => item.text.trim()) && !(collection.selections ?? []).length) ? null
    : <ReasonCategorySummary collection={collection} />;
  // 記録が無いときは見出しと1行だけにする。
  if (body === null && job.applicationDates !== 'loading') return <section className="lo-card lo-collapsed" aria-label="応募理由（ヒアリング内容）" data-testid="listing-reasons"><p className="lo-line"><strong>応募理由（ヒアリング内容）</strong><span role="status">応募理由の記録はまだありません。</span></p></section>;
  return <section className="lo-card" aria-labelledby="lo-reasons-title" data-testid="listing-reasons"><div className="lo-head"><h2 id="lo-reasons-title">応募理由（ヒアリング内容）</h2><span>応募の出来事を開くと、その人の応募理由を見られます。</span></div>{body}</section>;
}

/** 出来事を既定で出す件数。残りは「すべて表示」で出す。 */
export const RECENT_EVENT_COUNT = 5;

export function ListingOverview({ job, events, billing, market, selectedId, onSelect, onCompare, onOpenDrawer, tasks, marketSlot }: Props) {
  const hearing = usePhoneHearing(job);
  const asOf = applicationsCutoffDate(job);
  const listingStart = events.find(event => event.kind === 'start')?.date ?? null;
  const selected = events.find(event => event.id === selectedId) ?? null;
  const [showApplications, setShowApplications] = useState(true);
  const [showAllEvents, setShowAllEvents] = useState(false);
  const newestFirst = useMemo(() => [...events].reverse().filter(event => showApplications || event.kind !== 'application'), [events, showApplications]);
  const selectedIndex = selectedId ? newestFirst.findIndex(event => event.id === selectedId) : -1;
  // 選んだ出来事が畳まれた側にあるときは、見失わないように全件を出す。
  const expanded = showAllEvents || selectedIndex >= RECENT_EVENT_COUNT;
  const shownEvents = expanded ? newestFirst : newestFirst.slice(0, RECENT_EVENT_COUNT);
  const lifeline = useMemo(() => buildLifeline(job, events, billing), [job, events, billing]);
  const selectedWeek = lifeline && selected ? weekIndex(lifeline, selected.date) : null;
  // 同じ方の2回目以降の応募は出来事に出すが、件数には入れない。
  const applicationCount = countedApplications(events);
  const card = useRef<HTMLElement | null>(null);
  useEffect(() => { card.current?.scrollIntoView({ block: 'nearest' }); }, [selectedId]);
  const phonePanel = <PhoneHearingContent bare applicationRows={job.applicationRows ?? []} {...hearing} onApplication={key => {
    const event = events.find(event => event.applicationKey === key);
    if (!event) return;
    setShowApplications(true);
    onSelect(event.id);
    requestAnimationFrame(() => { document.getElementById(`application-${key}`)?.scrollIntoView({ block: 'center' }); });
  }} />;
  const hearingSummary = hearing.data ? `記録あり ${String(hearing.data.items.length)}件 ／ 応募 ${String(hearing.data.total_applicants)}件` : '聞き取った内容と通話の記録';
  const taskSection = tasks && <ListingTasks options={tasks} hearingSummary={hearingSummary} hearingPanel={phonePanel} />;
  // 応募の出来事から「電話ヒアリングのこの応募へ」を押したとき、電話ヒアリングの札を開いてその応募へ移る。
  const showHearing = (applicant: string) => {
    tasks?.onChange('hearing');
    requestAnimationFrame(() => { const target = document.getElementById(`hearing-${applicant}`); target?.scrollIntoView({ block: 'center' }); target?.focus(); });
  };
  if (!events.length) return <div className="lo-main"><section className="lo-card" aria-label="出来事"><h2>出来事</h2><p className="lo-note" role="status">掲載の開始や変更の記録がないため、出来事は出せません。</p></section>{taskSection}{marketSlot}<ListingReasons job={job} /></div>;
  return <div className="lo-main">
    <section className="lo-card" aria-labelledby="lo-chart-title">
      <div className="lo-head"><h2 id="lo-chart-title">求人のライフライン</h2><InfoTip className="jc-infotip-left" iconFirst label="見方"><p>掲載開始から{lifeline?.endIndex !== null && lifeline?.endIndex !== undefined ? '掲載終了' : '今日'}まで、1週を1列で並べています（月曜はじまり）。横にスクロールできます。上の段の番号を押すと、その出来事が下で開きます。</p></InfoTip><span>1週を1列で並べています。番号は下の出来事の番号です。</span></div>
      {lifeline ? <LifelineChart lifeline={lifeline} job={job} selectedWeek={selectedWeek} onSelect={onSelect} /> : <p className="lo-note" role="status">掲載の開始日が分からないため、週ごとの推移は出せません。</p>}
    </section>
    {taskSection}
    <section aria-labelledby="lo-events-title">
      <div className="lo-head"><h2 id="lo-events-title">出来事（新しい順）</h2><InfoTip className="jc-infotip-left" iconFirst label="数え方"><EventRules /></InfoTip>{applicationCount > 0 && <label className="lo-toggle"><input type="checkbox" checked={showApplications} onChange={event => { setShowApplications(event.target.checked); }} />応募（{applicationCount}件）も一覧に並べる</label>}</div>
      <p className="lo-note">行を押すと、その出来事の詳細が開きます。上のライフラインのその週にも印が付きます。</p>
      <div className="lo-events">
        {shownEvents.map(event => {
          const open = event.id === selected?.id;
          const isApplication = event.kind === 'application';
          // 競合調査は求人票を変える出来事ではないので、前後4週の応募は並べず、その月の記録だけを出す。
          const isCompetitor = event.kind === 'competitor';
          const competitorEntry = isCompetitor ? job.competitorMonthly?.find(entry => entry.month === event.competitorMonth) : undefined;
          const counts = eventCounts(job, event, asOf, listingStart);
          const checks = isApplication || isCompetitor ? [] : eventChecks(event, events, billing, market, asOf);
          // 前と後の日数が違うと、件数の差は短い方が少なく出るので出さない。
          const unequal = counts.before !== null && counts.after !== null && counts.beforeDays !== counts.afterDays;
          const diff = counts.before !== null && counts.after !== null && !unequal ? counts.after - counts.before : null;
          const shortSide = unequal ? counts.afterDays < counts.beforeDays ? '後' : '前' : '';
          const partial = [counts.before !== null && counts.beforeDays < 28 ? `前は掲載後${String(counts.beforeDays)}日分` : '', counts.after !== null && counts.afterDays < 28 ? `後は${String(counts.afterDays)}日分` : ''].filter(Boolean).join('・');
          return <article id={isApplication ? `application-${event.applicationKey ?? event.id}` : undefined} key={event.id} ref={open ? card : undefined} className={`lo-event lo-kind-${event.kind}${open ? ' lo-open' : ''}`}>
            <button type="button" className="lo-event-head" aria-expanded={open} onClick={() => { onSelect(open ? null : event.id); }}>
              <span className="lo-num">{isApplication ? '応' : event.n}</span>
              <span className="lo-date">{md(event.date)}</span>
              <span className="lo-title">{!isApplication && <small className="lo-kind">{KIND_LABEL[event.kind]}</small>}{event.title}{(isApplication || isCompetitor) && <small className="lo-attrs">{event.summary}</small>}</span>
              {isApplication || isCompetitor ? <span /> : <span className="lo-result"><strong>{resultText(event, counts, job.overallApplications ? undefined : noApplicationDatesText(job))}</strong>{diff !== null && <small>差 {diff > 0 ? '+' : diff < 0 ? '−' : '±'}{Math.abs(diff)} 件{partial ? `（${partial}）` : ''}</small>}{unequal && <small data-testid="event-no-diff">{shortSide === '後' ? '後の期間がまだ短い' : '前の期間が短い'}ため差は出していません（前 {String(counts.beforeDays)} 日・後 {String(counts.afterDays)} 日）</small>}{diff === null && !unequal && partial && <small>{partial}</small>}</span>}
              <span className="lo-caret jc-open" aria-hidden="true">{open ? '閉じる ▲' : '詳細 ▼'}</span>
            </button>
            {open && isApplication && event.repeat && <div className="lo-event-body"><p className="lo-summary">{event.repeatOf ? `この方の記録は${md(event.repeatOf)}の応募にまとめて出しています。` : 'この方の記録は最初の応募にまとめて出しています。'}</p></div>}
            {open && isApplication && !event.repeat && <div className="lo-event-body"><p className="lo-summary"><strong>応募者：</strong>{event.summary}</p><ApplicationDetails job={job} event={event} hearing={hearing.data} loading={hearing.loading} onShowHearing={showHearing} /></div>}
            {open && isCompetitor && <div className="lo-event-body">{competitorEntry ? <CompetitorMonthDetails entry={competitorEntry} /> : <p className="lo-summary">この月の記録を読めませんでした。</p>}</div>}
            {open && !isApplication && !isCompetitor && <div className="lo-event-body">
              <p className="lo-summary lo-changed"><strong>変えた中身：</strong>{event.summary}</p>
              <EventCostTable event={event} counts={counts} costs={eventCosts(event, counts, billing)} asOf={asOf} />
              <ul className="lo-checks" aria-label="ほかに応募を動かしそうなこと">
                {checks.map(check => <li key={check.key}><span className={`lo-mark lo-mark-${check.mark}`}>{check.markLabel}</span><strong>{check.label}</strong><span>{check.text}</span></li>)}
              </ul>
              <div className="lo-actions">
                {event.versionId && <button type="button" className="jc-button" onClick={() => { onCompare(event.previousVersionId ?? undefined, event.versionId ?? undefined); }}>この出来事の前と後の全文を比べる</button>}
                <button type="button" className="jc-button" onClick={() => { onOpenDrawer('who', event); }}>応募者・応募理由の前後を見る</button>
                <button type="button" className="jc-button" onClick={() => { onOpenDrawer('billing', event); }}>課金・クリックの実績を見る</button>
                <button type="button" className="jc-button" onClick={() => { onOpenDrawer('market', event); }}>市場の推移を見る</button>
              </div>
            </div>}
          </article>;
        })}
      </div>
      {newestFirst.length > RECENT_EVENT_COUNT && <p className="lo-more-events"><span>{expanded ? `${String(newestFirst.length)}件すべてを表示しています。` : `新しい順に${String(RECENT_EVENT_COUNT)}件を表示しています（全${String(newestFirst.length)}件）。`}</span>{selectedIndex < RECENT_EVENT_COUNT && <button type="button" className="jc-button" aria-expanded={expanded} onClick={() => { setShowAllEvents(value => !value); }}>{expanded ? `最新${String(RECENT_EVENT_COUNT)}件だけにする` : `すべて表示（${String(newestFirst.length)}件）`}</button>}</p>}
    </section>
    {marketSlot}
    <ListingReasons job={job} />
  </div>;
}
