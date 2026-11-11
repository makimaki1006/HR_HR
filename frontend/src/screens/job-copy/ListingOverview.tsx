import { PhoneHearingContent, usePhoneHearing } from './PhoneHearing';
import type { PhoneHearing as Hearing } from '../../generated/PhoneHearing';
import { formatDateJst } from './format';
import { useEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import type { JobCopyRecord } from './data';
import { RetryApplicants } from './RetryApplicants';
import { applicationsCutoffDate, noApplicationDatesText } from './detailBandModel';
import { COLUMN_PX, buildLifeline, changeMarkerLabel, changeMarkerNumbers, weekIndex } from './lifelineModel';
import type { Lifeline, LifelineWeek } from './lifelineModel';
import { HEARING_TEXT_SOURCES, reasonSourceLabels } from './applicantReasonsModel';
import { ReasonCategorySummary } from './ReasonCategorySummary';
import { maskPersonalDetails } from './personalText';
import { selectedCategory } from './reasonCategories';
import { eventChecks, eventCostNotes, eventCosts, eventCounts, md, resultText } from './listingEventsModel';
import type { EventCosts, EventCounts, ListingEvent, MarketInput } from './listingEventsModel';
import { chooseMarket, designatedTitle, indeedTitleState } from './marketMatch';
import { marketRows } from './marketChartModel';
import type { MarketRow } from './marketChartModel';
import { useMarketFetch } from './marketSource';
import type { BillingEntry } from './timelineModel';
import './listing-overview.css';

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
  /** 必要なときだけ使うもの（案を作る・手直し・ほかの分析）。電話ヒアリングと一緒に「作業」へ畳む。 */
  work?: ReactNode;
  workOpen?: boolean;
  onWorkToggle?: (open: boolean) => void;
}

const num = (value: number | null) => value === null ? '—' : value.toLocaleString('ja-JP');
const yen = (value: number | null) => value === null ? '—' : `¥${Math.round(value).toLocaleString('ja-JP')}`;
/** ライフラインの1列（56px）に収まる短い数。10万以上は「万」「億」で縮める（正確な値は title に出す）。 */
export const shortNumber = (value: number | null, prefix = '') => {
  if (value === null) return '—';
  const abs = Math.abs(value), sign = value < 0 ? '-' : '';
  const unit = (n: number, u: string) => `${sign}${prefix}${(n >= 100 ? Math.round(n) : Math.round(n * 10) / 10).toLocaleString('ja-JP')}${u}`;
  if (abs >= 1e8) return unit(abs / 1e8, '億');
  if (abs >= 1e5) return unit(abs / 1e4, '万');
  return `${sign}${prefix}${Math.round(abs).toLocaleString('ja-JP')}`;
};

/** 横の時間軸。1週 = 1列（幅は固定）。開いたときは最新の週が見える位置にする。 */
export function LifelineChart({ lifeline, job, selectedWeek, onSelect }: { lifeline: Lifeline; job: JobCopyRecord; selectedWeek: number | null; onSelect: (id: string) => void }) {
  const scroller = useRef<HTMLDivElement | null>(null);
  const width = lifeline.weeks.length * COLUMN_PX;
  useEffect(() => { const element = scroller.current; if (element) element.scrollLeft = element.scrollWidth; }, [lifeline.weeks.length, job.id]);
  useEffect(() => {
    const element = scroller.current;
    if (element && selectedWeek !== null) element.scrollLeft = Math.max(0, selectedWeek * COLUMN_PX - element.clientWidth / 2 + COLUMN_PX / 2);
  }, [selectedWeek]);
  const media = lifeline.mediaLabel;
  const noPerf = lifeline.hasPerformance ? '' : media === 'リクロジ' ? 'リクロジの実績はまだ取得していません' : media === 'AirWork' ? '課金データなし' : 'Indeed の実績データなし';
  const rows: { key: string; label: string; cell: (week: LifelineWeek) => string; short: (week: LifelineWeek) => string; empty: string }[] = [
    { key: 'billing', label: '課金（週）', cell: week => yen(week.billing), short: week => shortNumber(week.billing, '¥'), empty: noPerf },
    { key: 'impressions', label: '閲覧数（表示回数）', cell: week => num(week.impressions), short: week => shortNumber(week.impressions), empty: noPerf },
    { key: 'clicks', label: 'クリック数', cell: week => num(week.clicks), short: week => shortNumber(week.clicks), empty: noPerf },
    { key: 'hubspot', label: '応募数（HubSpot に記録された応募日）', cell: week => num(week.hubspotApplications), short: week => shortNumber(week.hubspotApplications), empty: '' },
    { key: 'media', label: `応募数（${media}の集計）`, cell: week => num(week.mediaApplications), short: week => shortNumber(week.mediaApplications), empty: noPerf },
  ];
  // 同じ週に印が重なる（掲載を始めた週がまだ今週など）ときは、名前を縦にずらして重ねない
  const placed: number[] = [];
  const marker = (index: number | null, label: string, tone: string) => {
    if (index === null) return null;
    const stack = placed.filter(at => at === index).length;
    placed.push(index);
    return <div key={label} className={`lf-marker lf-marker-${tone}`} style={{ left: `${String(index * COLUMN_PX)}px` }}><span style={stack ? { marginBottom: `${String(stack * 16)}px` } : undefined}>{label}</span></div>;
  };
  return <div className="lf" data-testid="lifeline">
    <div className="lf-labels" aria-hidden="true">
      <div className="lf-head-label">週（月曜）</div>
      <div className="lf-change-label">求人の変更</div>
      {rows.map(row => <div key={row.key} className="lf-row-label">{row.label}</div>)}
    </div>
    <div className="lf-scroll" ref={scroller} tabIndex={0} role="region" aria-label="週ごとの推移（横にスクロールできます）">
      <div className="lf-track" style={{ width: `${String(width)}px` }}>
        {selectedWeek !== null && <div className="lf-selected" data-testid="lifeline-selected" style={{ left: `${String(selectedWeek * COLUMN_PX)}px`, width: `${String(COLUMN_PX)}px` }} />}
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

const KIND_LABEL: Record<ListingEvent['kind'], string> = { start: '掲載', end: '掲載', revive: '掲載', content: '求人内容', image: '画像', billing: '課金', application: '応募' };

function ApplicationDetails({ job, event, hearing, loading }: { job: JobCopyRecord; event: ListingEvent; hearing: Hearing | null; loading: boolean }) {
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
    {job.applicationDates === 'failed' ? <p role="alert">応募の記録を読み込めませんでした。<RetryApplicants id={job.hubspotId} /></p> : loading || job.applicationDates === 'loading' ? <p role="status">電話で聞いた内容を読み込み中です。</p> : !hearing && job.hubspotId ? <p role="status">電話で聞いた内容を取得できませんでした。時間を置いて、もう一度開いてください。</p> : !recorded && job.applicantReasons?.truncated ? <p>一部だけ表示しているため、この応募の記録を確認できません。</p> : !recorded && <p>この応募には、応募理由や電話で聞いた内容の記録がありません</p>}
    <div className="lo-reasons-of">
    {picked.map(item => <p key={`${item.sourceProperty}-${item.value}`} className="lo-summary"><strong>{reasonSourceLabels[item.sourceProperty] ?? '応募理由の分類'}：</strong>{item.label ?? '分類の名前を確認できません'}</p>)}
    {reasons.map(item => <p key={item.id} className="lo-summary"><strong>{reasonSourceLabels[item.sourceProperty] ?? '応募理由'}：</strong>{maskPersonalDetails(item.text)}</p>)}
    </div>
    {card && <>
      <h3>電話ヒアリング</h3><dl className="lo-application-fields">{fields.map(field => <div key={field.label}><dt>{field.label}</dt><dd>{maskPersonalDetails(field.text)}</dd></div>)}</dl>
      {card.notes.length > 0 && <section aria-label="メモ"><h3>メモ</h3>{card.notes.map((note, index) => <p key={index}>{maskPersonalDetails(note.text)}</p>)}</section>}
      <button type="button" className="jc-button" onClick={() => { const target = document.getElementById(`hearing-${card.applicant}`); for (let section = target?.closest('details') ?? null; section; section = section.parentElement?.closest('details') ?? null) section.open = true; target?.scrollIntoView({ block: 'center' }); target?.focus(); }}>電話ヒアリングのこの応募へ</button>
    </>}
  </div>;
}

/** 求人の応募理由（ヒアリング内容）の内訳。記録が無い求人は、無いと言う。 */
export function ListingReasons({ job }: { job: JobCopyRecord }) {
  const collection = job.applicantReasons;
  const body = job.applicationDates === 'loading' ? <p className="lo-note" role="status">応募理由を読み込み中です。</p>
    : job.applicationDates === 'failed' ? <p className="lo-note" role="alert">応募理由を読み込めませんでした。<RetryApplicants id={job.hubspotId} /></p>
    : !collection?.available || (!collection.items.some(item => item.text.trim()) && !(collection.selections ?? []).length) ? <p className="lo-note" role="status">応募理由の記録はまだありません。</p>
    : <ReasonCategorySummary collection={collection} />;
  return <section className="lo-card" aria-labelledby="lo-reasons-title" data-testid="listing-reasons"><div className="lo-head"><h2 id="lo-reasons-title">応募理由（ヒアリング内容）</h2><span>応募の出来事を開くと、その人の応募理由を見られます。</span></div>{body}</section>;
}

/** 出来事を既定で出す件数。残りは「すべて表示」で出す。 */
export const RECENT_EVENT_COUNT = 5;

export function ListingOverview({ job, events, billing, market, selectedId, onSelect, onCompare, onOpenDrawer, work, workOpen = false, onWorkToggle }: Props) {
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
  const applicationCount = events.filter(event => event.kind === 'application').length;
  const card = useRef<HTMLElement | null>(null);
  useEffect(() => { card.current?.scrollIntoView({ block: 'nearest' }); }, [selectedId]);
  const phonePanel = <PhoneHearingContent applicationRows={job.applicationRows ?? []} {...hearing} onApplication={key => {
    const event = events.find(event => event.applicationKey === key);
    if (!event) return;
    setShowApplications(true);
    onSelect(event.id);
    requestAnimationFrame(() => { document.getElementById(`application-${key}`)?.scrollIntoView({ block: 'center' }); });
  }} />;
  const workSection = <details className="lo-work" open={workOpen} onToggle={event => { onWorkToggle?.(event.currentTarget.open); }}><summary>作業（案を作る・手直し・電話ヒアリング・ほかの分析）</summary><div className="lo-work-body">{work}{phonePanel}</div></details>;
  if (!events.length) return <div className="lo-main"><section className="lo-card" aria-label="出来事"><h2>出来事</h2><p className="lo-note" role="status">掲載の開始や変更の記録がないため、出来事は出せません。</p></section><ListingReasons job={job} />{workSection}</div>;
  return <div className="lo-main">
    <section className="lo-card" aria-labelledby="lo-chart-title">
      <div className="lo-head"><h2 id="lo-chart-title">求人のライフライン</h2><span>掲載開始から{lifeline?.endIndex !== null && lifeline?.endIndex !== undefined ? '掲載終了' : '今日'}まで、1週を1列で並べています（月曜はじまり）。横にスクロールできます。</span></div>
      {lifeline ? <LifelineChart lifeline={lifeline} job={job} selectedWeek={selectedWeek} onSelect={onSelect} /> : <p className="lo-note" role="status">掲載の開始日が分からないため、週ごとの推移は出せません。</p>}
      {!job.overallApplications && <p className="lo-note" role={job.applicationDates === 'failed' ? 'alert' : 'status'}>{noApplicationDatesText(job)}{job.applicationDates === 'failed' && <RetryApplicants id={job.hubspotId} />}</p>}
    </section>
    <section aria-labelledby="lo-events-title">
      <div className="lo-head"><h2 id="lo-events-title">出来事（新しい順）</h2>{applicationCount > 0 && <label className="lo-toggle"><input type="checkbox" checked={showApplications} onChange={event => { setShowApplications(event.target.checked); }} />応募（{applicationCount}件）を出す</label>}</div>
      <p className="lo-note">押すと、横の時間軸のその週に印が付きます。前後4週の応募の件数の差は、出来事のほかの要因でも動くため、原因を決めるものではありません。</p>
      <div className="lo-events">
        {shownEvents.map(event => {
          const open = event.id === selected?.id;
          const isApplication = event.kind === 'application';
          const counts = eventCounts(job, event, asOf, listingStart);
          const checks = isApplication ? [] : eventChecks(event, events, billing, market, asOf);
          const diff = counts.before !== null && counts.after !== null ? counts.after - counts.before : null;
          const partial = [counts.before !== null && counts.beforeDays < 28 ? `前は掲載後${String(counts.beforeDays)}日分` : '', counts.after !== null && counts.afterDays < 28 ? `後は${String(counts.afterDays)}日分` : ''].filter(Boolean).join('・');
          return <article id={isApplication ? `application-${event.applicationKey ?? event.id}` : undefined} key={event.id} ref={open ? card : undefined} className={`lo-event lo-kind-${event.kind}${open ? ' lo-open' : ''}`}>
            <button type="button" className="lo-event-head" aria-expanded={open} onClick={() => { onSelect(open ? null : event.id); }}>
              <span className="lo-num">{isApplication ? '応' : event.n}</span>
              <span className="lo-date">{md(event.date)}</span>
              <span className="lo-title"><small className="lo-kind">{KIND_LABEL[event.kind]}</small>{event.title}{isApplication && <small className="lo-attrs">{event.summary}</small>}</span>
              {isApplication ? <span /> : <span className="lo-result"><strong>{resultText(event, counts, job.overallApplications ? undefined : noApplicationDatesText(job))}</strong>{diff !== null && <small>差 {diff > 0 ? '+' : diff < 0 ? '−' : '±'}{Math.abs(diff)} 件{partial ? `（${partial}）` : ''}</small>}{diff === null && partial && <small>{partial}</small>}</span>}
              <span className="lo-caret" aria-hidden="true">{open ? '▲' : '▼'}</span>
            </button>
            {open && isApplication && <div className="lo-event-body"><p className="lo-summary"><strong>応募者：</strong>{event.summary}</p><ApplicationDetails job={job} event={event} hearing={hearing.data} loading={hearing.loading} /></div>}
            {open && !isApplication && <div className="lo-event-body">
              <EventCostTable event={event} counts={counts} costs={eventCosts(event, counts, billing)} asOf={asOf} />
              <ul className="lo-checks" aria-label="ほかに応募を動かしそうなこと">
                {checks.map(check => <li key={check.key}><span className={`lo-mark lo-mark-${check.mark}`}>{check.markLabel}</span><strong>{check.label}</strong><span>{check.text}</span></li>)}
              </ul>
              <p className="lo-summary"><strong>変えた中身：</strong>{event.summary}</p>
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
    <ListingReasons job={job} />
    {workSection}
  </div>;
}
