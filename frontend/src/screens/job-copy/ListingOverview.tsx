import { useEffect, useMemo, useRef, useState } from 'react';
import type { JobCopyRecord } from './data';
import { applicationsCutoffDate } from './detailBandModel';
import { buildChart } from './listingChartModel';
import { eventChecks, eventCounts, eventWindows, md, resultText } from './listingEventsModel';
import type { ListingEvent, MarketInput } from './listingEventsModel';
import { chooseMarket, designatedTitle, indeedTitleState } from './marketMatch';
import { marketRows } from './marketChartModel';
import type { MarketRow } from './marketChartModel';
import { useMarketFetch } from './marketSource';
import { pct } from './listingChartModel';
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
}

const BAR_AREA_PX = 160;

function Chart({ job, events, billing, selected, onSelect, asOf }: { job: JobCopyRecord; events: readonly ListingEvent[]; billing: readonly BillingEntry[]; selected: ListingEvent | null; onSelect: (id: string) => void; asOf: string }) {
  const chart = useMemo(() => buildChart(job, events, billing, asOf), [job, events, billing, asOf]);
  if (!chart) return <p className="lo-note" role="status">応募の日別データがないため、週ごとの応募は出せません。</p>;
  const windows = selected ? eventWindows(selected, asOf) : null;
  const shade = (range: { start: string; end: string } | null | undefined) => range ? { left: `${String(pct(range.start, chart.start, chart.totalDays))}%`, width: `${String(Math.max(0, pct(range.end, chart.start, chart.totalDays) - pct(range.start, chart.start, chart.totalDays)))}%` } : null;
  const before = shade(windows?.before); const after = shade(windows?.after);
  const rows = Math.max(1, ...chart.pins.map(pin => pin.row + 1));
  const pinArea = 6 + rows * 34;
  return <div className="lo-chart" style={{ ['--lo-pin-area' as string]: `${String(pinArea)}px` }}>
    <div className="lo-y" aria-hidden="true" style={{ height: `${String(BAR_AREA_PX + pinArea)}px` }}>
      {chart.yTicks.map(tick => <span key={tick} style={{ bottom: `${String(tick / chart.maxCount * BAR_AREA_PX)}px` }}>{tick} 件</span>)}
    </div>
    <div className="lo-plot" style={{ height: `${String(BAR_AREA_PX + pinArea)}px` }} role="group" aria-label="週ごとの応募と出来事">
      {chart.yTicks.map(tick => <div key={tick} className="lo-grid" aria-hidden="true" style={{ bottom: `${String(tick / chart.maxCount * BAR_AREA_PX)}px` }} />)}
      {before && <div className="lo-shade lo-shade-before" aria-hidden="true" style={{ ...before, top: `${String(pinArea - 4)}px` }} />}
      {after && <div className="lo-shade lo-shade-after" aria-hidden="true" style={{ ...after, top: `${String(pinArea - 4)}px` }} />}
      {chart.bars.map(bar => <div key={bar.start} className="lo-bar" title={`${md(bar.start)} からの週: ${String(bar.count)} 件`} style={{ left: `${String(bar.leftPct + 0.15)}%`, width: `${String(Math.max(0.2, bar.widthPct - 0.3))}%`, height: `${String(bar.count / chart.maxCount * BAR_AREA_PX)}px` }} />)}
      {chart.pins.map(pin => {
        const on = pin.eventId === selected?.id;
        const event = events.find(item => item.id === pin.eventId);
        return <div key={pin.eventId}>
          <div className={`lo-pin-line${on ? ' lo-on' : ''}`} aria-hidden="true" style={{ left: `${String(pin.leftPct)}%`, top: `${String(6 + pin.row * 34 + 30)}px` }} />
          <button type="button" className={`lo-pin${on ? ' lo-on' : ''}`} aria-pressed={on} aria-label={`出来事 ${String(pin.n)}：${event ? `${md(event.date)} ${event.title}` : ''}`} style={{ left: `${String(pin.leftPct)}%`, top: `${String(6 + pin.row * 34)}px` }} onClick={() => { onSelect(pin.eventId); }}>{pin.n}</button>
        </div>;
      })}
      <div className="lo-cutoff" aria-hidden="true" style={{ left: `${String(chart.cutoffPct)}%`, top: `${String(pinArea - 4)}px` }} />
      {chart.cutoffPct < 99.5 && <span className="lo-cutoff-label" style={{ left: `${String(chart.cutoffPct)}%`, top: `${String(pinArea - 4)}px` }}>応募は{md(asOf)}まで</span>}
    </div>
    <div className="lo-lane-label">課金</div>
    <div className="lo-lane" aria-label="課金">
      {chart.months.map(month => <span key={`${month.label}${String(month.leftPct)}`} className="lo-month" style={{ left: `${String(month.leftPct)}%` }}>{month.label}</span>)}
      {chart.bands.map(band => <div key={`${band.label}${String(band.leftPct)}`} className="lo-band" title="4週あたりの課金" style={{ left: `${String(band.leftPct)}%`, width: `calc(${String(band.widthPct)}% - 2px)` }}>{band.label}<small>／4週</small></div>)}
      {!chart.bands.length && <span className="lo-lane-empty">課金のデータがありません</span>}
    </div>
  </div>;
}

export function ListingOverview({ job, events, billing, market, selectedId, onSelect, onCompare, onOpenDrawer }: Props) {
  const asOf = applicationsCutoffDate(job);
  const listingStart = events.find(event => event.kind === 'start')?.date ?? null;
  const selected = events.find(event => event.id === selectedId) ?? null;
  const newestFirst = useMemo(() => [...events].reverse(), [events]);
  const card = useRef<HTMLElement | null>(null);
  useEffect(() => { card.current?.scrollIntoView({ block: 'nearest' }); }, [selectedId]);
  if (!events.length) return <section className="lo-card" aria-label="出来事"><h2>出来事</h2><p className="lo-note" role="status">掲載の開始や変更の記録がないため、出来事は出せません。</p></section>;
  return <div className="lo-main">
    <section className="lo-card" aria-labelledby="lo-chart-title">
      <div className="lo-head"><h2 id="lo-chart-title">週ごとの応募</h2><span>番号は下の出来事と同じ。押すとその出来事を開き、前の4週（灰色）と後の4週（水色）を塗ります。</span></div>
      {asOf ? <Chart job={job} events={events} billing={billing} selected={selected} onSelect={onSelect} asOf={asOf} /> : <p className="lo-note" role="status">応募を取得した日が分からないため、週ごとの応募は出せません。</p>}
    </section>
    <section aria-labelledby="lo-events-title">
      <h2 id="lo-events-title">出来事（新しい順）</h2>
      <p className="lo-note">出来事の日の前の4週と後の4週の応募を、件数で比べています。件数の差は出来事のほかの要因でも動くため、原因を決めるものではありません。</p>
      <div className="lo-events">
        {newestFirst.map(event => {
          const open = event.id === selected?.id;
          const counts = eventCounts(job, event, asOf, listingStart);
          const checks = eventChecks(event, events, billing, market, asOf);
          const diff = counts.before !== null && counts.after !== null ? counts.after - counts.before : null;
          const partial = [counts.before !== null && counts.beforeDays < 28 ? `前は掲載後${String(counts.beforeDays)}日分` : '', counts.after !== null && counts.afterDays < 28 ? `後は${String(counts.afterDays)}日分` : ''].filter(Boolean).join('・');
          return <article key={event.id} ref={open ? card : undefined} className={`lo-event${open ? ' lo-open' : ''}`}>
            <button type="button" className="lo-event-head" aria-expanded={open} onClick={() => { onSelect(open ? null : event.id); }}>
              <span className="lo-num">{event.n}</span>
              <span className="lo-date">{md(event.date)}</span>
              <span className="lo-title">{event.title}</span>
              <span className="lo-result"><strong>{resultText(event, counts)}</strong>{diff !== null && <small>差 {diff > 0 ? '+' : diff < 0 ? '−' : '±'}{Math.abs(diff)} 件{partial ? `（${partial}）` : ''}</small>}{diff === null && partial && <small>{partial}</small>}</span>
              <span className="lo-caret" aria-hidden="true">{open ? '▲' : '▼'}</span>
            </button>
            {open && <div className="lo-event-body">
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
    </section>
  </div>;
}
