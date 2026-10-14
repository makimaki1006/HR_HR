import { useEffect, useState } from 'react';
import type { JobCopyRecord } from './data';
import { bandNumbers, formatBandYen } from './detailBandModel';
import { lastMarketMonth } from './timelineModel';
import { md, MARKET_CAUTION_PCT, BILLING_EVENT_MIN_PCT } from './listingEventsModel';
import type { BillingEntry } from './timelineModel';
import { publicationLabel } from './hubspotListings';
import { joinPresent } from './format';
import { chooseMarket, prefectureFromLocation } from './marketMatch';
import { marketRows } from './marketChartModel';
import { useMarketFetch } from './marketSource';
import { MarketOccupationSlot } from './MarketOccupationSlot';
import type { IndeedTitleChange } from './MarketOccupationSlot';

const jpDate = (value: string | null) => value ? value.replace(/^(\d{4})-(\d{2})-(\d{2})$/, (_all, y: string, m: string, d: string) => `${y}年${String(Number(m))}月${String(Number(d))}日`) : 'なし';

/** 詳細を縦にスクロールしても見え続ける上の帯。 */
export function DetailBand({ job, billing, onOpenFullCompare, latestEvent, onOpenLatest, onNewPattern, newPatternDisabled = false }: { job: JobCopyRecord; billing: readonly BillingEntry[] | undefined; onOpenFullCompare: () => void; latestEvent?: { date: string; title: string } | null; onOpenLatest?: () => void; onNewPattern?: () => void; newPatternDisabled?: boolean }) {
  const n = bandNumbers(job, billing);
  return <div className="jc-band" role="region" aria-label="この案件の要点">
    <div className="jc-band-name"><h1 title={job.title}>{job.title}</h1><span>{joinPresent([job.media, job.dataSource === 'hubspot' ? `公開状況：${publicationLabel(job.listingPublicationStatus ?? null)}` : ''])}</span></div>
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

type MarketNow = { jobs: number | null; month: string } | null;

function useAutoMarket(job: JobCopyRecord, mode: 'api' | 'demo') {
  const fetchMarket = useMarketFetch(mode);
  const [state, setState] = useState<{ title: string | null; prefecture: string | null; now: MarketNow }>({ title: null, prefecture: null, now: null });
  useEffect(() => {
    const run = { cancelled: false };
    const stopped = () => run.cancelled;
    void (async () => {
      try {
        const list = await fetchMarket('', '');
        if (stopped() || !list.ok) return;
        const choice = chooseMarket(job, list.data.prefectures);
        setState({ title: choice.title, prefecture: choice.prefecture, now: null });
        if (!choice.title || !choice.prefecture) return;
        const series = await fetchMarket(choice.title, choice.prefecture);
        if (stopped() || !series.ok || !series.data.series) return;
        const rows = marketRows(series.data.series);
        const month = lastMarketMonth(rows);
        const row = rows.find(item => item.month === month);
        if (row && month) setState(previous => ({ ...previous, now: { jobs: row.jobs, month } }));
      } catch { /* 市場の数字が取れなくても、ほかの表示は続ける */ }
    })();
    return () => { run.cancelled = true; };
  }, [job, fetchMarket]);
  return state;
}

/** 右に常に出す「数字の前提」。どのデータの、どの範囲の数字かを示す。 */
export function AssumptionsRail({ job, billing, mode, onIndeedTitleChanged }: { job: JobCopyRecord; billing: readonly BillingEntry[] | undefined; mode: 'api' | 'demo'; onIndeedTitleChanged: (change: IndeedTitleChange) => void }) {
  const n = bandNumbers(job, billing);
  const market = useAutoMarket(job, mode);
  return <aside className="jc-rail" aria-label="数字の前提">
    <h2>数字の前提</h2>
    <section aria-label="データ"><h3>データ</h3>
      <dl><dt>応募データの最後の日</dt><dd data-testid="rail-last-application">{jpDate(n.lastApplicationDate)}</dd>
        <dt>課金データの最後の日</dt><dd data-testid="rail-last-billing">{jpDate(n.lastBillingDate)}</dd></dl>
      {n.asOf ? <p className="jc-muted">4週は、応募を取得した{jpDate(n.asOf)}までの28日です。</p> : <p className="jc-muted" role="status">応募の取得日が分かりません</p>}
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
    {market.now && <section aria-label="市場の今"><h3>市場の今</h3>
      <dl><dt>市場の求人数（{market.now.month.replace('-', '年')}月）</dt><dd data-testid="rail-market-jobs">{market.now.jobs === null ? '—' : `${market.now.jobs.toLocaleString('ja-JP')}件`}</dd></dl>
    </section>}
  </aside>;
}
