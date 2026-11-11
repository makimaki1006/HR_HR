import { useEffect, useRef, useState } from 'react';
import type { JobCopyRecord } from './data';
import { RetryApplicants } from './RetryApplicants';
import { bandNumbers, formatBandYen, listingPeriodText, marketMonthOverMonth, noApplicationDatesText, signedPct } from './detailBandModel';
import type { MarketMonthOverMonth } from './detailBandModel';
import { formatMonthJa, todayJst } from './timelineModel';
import { md, MARKET_CAUTION_PCT, BILLING_EVENT_MIN_PCT } from './listingEventsModel';
import type { BillingEntry } from './timelineModel';
import { publicationLabel } from './hubspotListings';
import { joinPresent } from './format';
import { chooseMarket, designatedTitle, indeedTitleState, prefectureFromLocation } from './marketMatch';
import { marketRows } from './marketChartModel';
import { useMarketFetch } from './marketSource';
import { MarketOccupationSlot } from './MarketOccupationSlot';
import type { IndeedTitleChange } from './MarketOccupationSlot';

const jpDate = (value: string | null) => value ? value.replace(/^(\d{4})-(\d{2})-(\d{2})$/, (_all, y: string, m: string, d: string) => `${y}年${String(Number(m))}月${String(Number(d))}日`) : 'なし';

/** 詳細を縦にスクロールしても見え続ける上の帯。 */
export function DetailBand({ job, billing, onOpenFullCompare, latestEvent, onOpenLatest, onNewPattern, newPatternDisabled = false, listingStart = null, listingEnd = null }: { job: JobCopyRecord; billing: readonly BillingEntry[] | undefined; onOpenFullCompare: () => void; latestEvent?: { date: string; title: string } | null; onOpenLatest?: () => void; onNewPattern?: () => void; newPatternDisabled?: boolean; /** 掲載を始めた日（出来事の「掲載を開始」と同じ日）。分からないとき null。 */ listingStart?: string | null; /** 掲載を終えた日。掲載中なら null。 */ listingEnd?: string | null }) {
  const n = bandNumbers(job, billing);
  const period = listingPeriodText(listingStart, listingEnd, todayJst());
  const status = job.dataSource === 'hubspot' ? `公開状況：${publicationLabel(job.listingPublicationStatus ?? null)}${period}` : period ? `掲載${period}` : '';
  const band = useRef<HTMLDivElement>(null);
  // 帯の実際の高さを --jc-band-h に持たせ、右の欄・下の固定見出しの位置をそこから決める
  useEffect(() => {
    const element = band.current;
    const host = element?.closest<HTMLElement>('.jc-detail');
    if (!element || !host || typeof ResizeObserver === 'undefined') return;
    const apply = () => { host.style.setProperty('--jc-band-h', `${String(Math.ceil(element.getBoundingClientRect().height))}px`); };
    apply();
    const observer = new ResizeObserver(apply);
    observer.observe(element);
    return () => { observer.disconnect(); host.style.removeProperty('--jc-band-h'); };
  }, []);
  return <div className="jc-band" ref={band} role="region" aria-label="この案件の要点">
    <div className="jc-band-name"><h1 title={job.title}>{job.title}</h1><span data-testid="band-status">{joinPresent([job.media, status])}</span></div>
    <dl className="jc-band-numbers">
      <div><dt>直近4週の応募</dt><dd data-testid="band-recent">{n.recent ?? '—'}<small>{n.previous === null ? '' : `前の4週 ${String(n.previous)}`}</small></dd></div>
      <div><dt>応募1件あたり</dt><dd data-testid="band-per-application">{formatBandYen(n.perApplicationYen)}</dd></div>
      <div><dt>今の課金（4週あたり）</dt><dd data-testid="band-billing">{formatBandYen(n.billingPer4WeeksYen)}</dd></div>
    </dl>
    {latestEvent && onOpenLatest && <button type="button" className="jc-band-latest" data-testid="band-latest-event" onClick={onOpenLatest}><small>最新の出来事</small><strong>{md(latestEvent.date)} {latestEvent.title}</strong></button>}
    <span className="jc-band-spacer" />
    <button type="button" className="jc-button" onClick={onOpenFullCompare}>求人の全文を比べる</button>
    {onNewPattern && <button type="button" className="jc-button jc-primary" disabled={newPatternDisabled} title={newPatternDisabled ? '現在の本文が未取得、または手直しを保存できない求人です。' : undefined} onClick={onNewPattern}>この求人から新しいパターンを作る</button>}
  </div>;
}

/** 右の欄の「市場の今」の状態。職種を指定した求人だけ数字を取りに行く。 */
type MarketNow =
  | { status: 'loading' }
  | { status: 'failed' }
  | { status: 'no_data' }
  | { status: 'ok'; value: MarketMonthOverMonth };

function useAutoMarket(job: JobCopyRecord, mode: 'api' | 'demo'): { key: string; now: MarketNow } {
  const fetchMarket = useMarketFetch(mode);
  const key = `${job.id}|${designatedTitle(job) ?? ''}|${job.location}`;
  const [state, setState] = useState<{ key: string; now: MarketNow }>({ key: '', now: { status: 'loading' } });
  useEffect(() => {
    if (!designatedTitle(job)) return;
    const run = { cancelled: false };
    const stopped = () => run.cancelled;
    const done = (now: MarketNow) => { if (!stopped()) setState({ key, now }); };
    void (async () => {
      try {
        const list = await fetchMarket('', '');
        if (!list.ok) { done({ status: 'failed' }); return; }
        const choice = chooseMarket(job, list.data.prefectures);
        if (!choice.title || !choice.prefecture) { done({ status: 'no_data' }); return; }
        const series = await fetchMarket(choice.title, choice.prefecture);
        if (!series.ok) { done({ status: 'failed' }); return; }
        const value = series.data.series ? marketMonthOverMonth(marketRows(series.data.series)) : null;
        done(value ? { status: 'ok', value } : { status: 'no_data' });
      } catch { done({ status: 'failed' }); /* 市場の数字が取れなくても、ほかの表示は続ける */ }
    })();
    return () => { run.cancelled = true; };
  }, [job, key, fetchMarket]);
  return state.key === key ? state : { key, now: { status: 'loading' } };
}

/** 右の欄の「市場の今」。職種が未指定なら案内だけを出す。平均時給は市場のデータに無いので出さない。 */
function MarketNowSection({ job, mode, onOpenMarket }: { job: JobCopyRecord; mode: 'api' | 'demo'; onOpenMarket: (() => void) | undefined }) {
  const titleState = indeedTitleState(job);
  const { now } = useAutoMarket(job, mode);
  const button = onOpenMarket && <button type="button" className="jc-button" onClick={onOpenMarket}>市場の推移を見る</button>;
  if (titleState === 'none') return <section aria-label="市場の今"><h3>市場の今</h3><p className="jc-muted" data-testid="rail-market-note">市場と比べないと指定されています。</p></section>;
  if (titleState === 'unset') return <section aria-label="市場の今"><h3>市場の今</h3><p className="jc-muted" data-testid="rail-market-note">市場と比べる職種が未指定です。上の「市場（Indeed）と比べる範囲」で職種を指定すると、競合の求人数と前月比が出ます。</p></section>;
  return <section aria-label="市場の今"><h3>市場の今</h3>
    {now.status === 'loading' && <p className="jc-muted" role="status">市場の数字を読み込み中です。</p>}
    {now.status === 'failed' && <p className="jc-muted" role="status">市場の数字を取得できませんでした。</p>}
    {now.status === 'no_data' && <p className="jc-muted" role="status">この職種・地域の市場の数字はまだありません。</p>}
    {now.status === 'ok' && <dl>
      <dt>競合の求人数（{formatMonthJa(now.value.month)}）</dt>
      <dd data-testid="rail-market-jobs">{now.value.jobs.toLocaleString('ja-JP')}件</dd>
      <dt>前月比（{formatMonthJa(now.value.previousMonth)}と比べて）</dt>
      <dd data-testid="rail-market-change">{now.value.changePct === null ? now.value.previousJobs === 0 ? '前月は0件のため割合は出せません' : '前月の数字がありません' :`${signedPct(now.value.changePct)}（${(now.value.previousJobs ?? 0).toLocaleString('ja-JP')}件 → ${now.value.jobs.toLocaleString('ja-JP')}件）`}</dd>
    </dl>}
    {button}
  </section>;
}

/** 右に常に出す「数字の前提」。どのデータの、どの範囲の数字かを示す。 */
export function AssumptionsRail({ job, billing, mode, onIndeedTitleChanged, onOpenMarket }: { job: JobCopyRecord; billing: readonly BillingEntry[] | undefined; mode: 'api' | 'demo'; onIndeedTitleChanged: (change: IndeedTitleChange) => void; /** 「市場の推移を見る」で、出来事から開くのと同じ横の窓を開く。 */ onOpenMarket?: () => void }) {
  const n = bandNumbers(job, billing);
  return <aside className="jc-rail" aria-label="数字の前提">
    <h2>数字の前提</h2>
    <section aria-label="データ"><h3>データ</h3>
      <dl><dt>応募データの最後の日</dt><dd data-testid="rail-last-application">{n.lastApplicationDate === null && job.overallApplications?.byDate ? 'まだ応募なし' : jpDate(n.lastApplicationDate)}</dd>
        <dt>課金データの最後の日</dt><dd data-testid="rail-last-billing">{jpDate(n.lastBillingDate)}</dd></dl>
      {n.asOf ? <p className="jc-muted">4週は、応募を取得した{jpDate(n.asOf)}までの28日です。</p> : <p className="jc-muted" role={job.applicationDates === 'failed' ? 'alert' : 'status'}>{noApplicationDatesText(job)}{job.applicationDates === 'failed' && <RetryApplicants id={job.hubspotId} />}</p>}
    </section>
    <section aria-label="市場と比べる範囲"><h3>市場（Indeed）と比べる範囲</h3>
      <MarketOccupationSlot record={job} prefecture={prefectureFromLocation(job.location)} onChanged={onIndeedTitleChanged} />
    </section>
    <section aria-label="出来事の見方"><h3>出来事の見方</h3>
      <p className="jc-muted">出来事の日の前の4週と後の4週の応募を、件数だけで比べます。増えた・減ったの判定はしません。</p>
      <ul className="lo-rules">
        <li>課金: 前後4週にほかの課金の変更（開始・停止・{BILLING_EVENT_MIN_PCT}%以上の増減）があれば「要注意」、なければ「小さい」。</li>
        <li>市場: 職種を指定した求人だけ。前後4週で競合の求人数が{MARKET_CAUTION_PCT}%以上動いていれば「要注意」。</li>
        <li>ほかの変更: 前後4週に別の出来事があれば「要注意」。</li>
      </ul>
    </section>
    <MarketNowSection job={job} mode={mode} onOpenMarket={onOpenMarket} />
  </aside>;
}
