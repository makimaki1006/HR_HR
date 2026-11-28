import { Fragment, useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import type { JobCopyRecord } from './data';
import { RetryApplicants } from './RetryApplicants';
import { bandNumbers, formatBandYen, listingPeriodText, marketMonthOverMonth, noApplicationDatesText, signedPct } from './detailBandModel';
import type { MarketMonthOverMonth } from './detailBandModel';
import { formatMonthJa, todayJst } from './timelineModel';
import type { BillingEntry } from './timelineModel';
import { publicationLabel } from './hubspotListings';
import { InfoTip } from './InfoTip';
import { chooseMarket, designatedTitle, indeedTitleState, prefectureFromLocation } from './marketMatch';
import { marketRows } from './marketChartModel';
import { useMarketFetch } from './marketSource';
import { MarketOccupationSlot } from './MarketOccupationSlot';
import type { IndeedTitleChange } from './MarketOccupationSlot';

/** 2026-11-08 → 11/8。今年でない日は 2025/11/8。 */
const shortDay = (value: string) => {
  const day = `${String(Number(value.slice(5, 7)))}/${String(Number(value.slice(8, 10)))}`;
  return value.slice(0, 4) === todayJst().slice(0, 4) ? day : `${value.slice(0, 4)}/${day}`;
};

/** 帯の中の 1 行：最後の応募・最後の課金と、応募を取り込んだ日。読み込み中・失敗はここ 1 か所だけに出す。 */
function DataLine({ job, n }: { job: JobCopyRecord; n: ReturnType<typeof bandNumbers> }) {
  const application = n.lastApplicationDate ? shortDay(n.lastApplicationDate) : job.overallApplications?.byDate ? 'なし' : null;
  return <p className="jc-band-data" data-testid="band-data">
    {application === null
      ? <span role={job.applicationDates === 'failed' ? 'alert' : 'status'} data-testid="band-application-state">{noApplicationDatesText(job)}{job.applicationDates === 'failed' && <RetryApplicants id={job.hubspotId} />}</span>
      : <span data-testid="rail-last-application">最後の応募：{application}</span>}
    <span aria-hidden="true"> ／ </span><span data-testid="rail-last-billing">最後の課金：{n.lastBillingDate ? shortDay(n.lastBillingDate) : 'なし'}</span>
    {n.asOf && <><span aria-hidden="true"> ／ </span><span data-testid="rail-fetched">取り込み {shortDay(n.asOf)} 時点</span></>}
    {n.asOf && <InfoTip className="jc-infotip-left" iconFirst label="4週の数え方"><p>直近4週は、応募を取得した{shortDay(n.asOf)}までの28日、前の4週はその前の28日です。上の応募・応募1件の費用・課金は、どれもこの同じ28日で数えます。</p><p>応募は HubSpot に記録された応募日で数えます。課金はこの28日に入る分を日割りで足します。</p><p>「今日」の画面や顧客の求人の表の分類は、媒体の週ごとの実績（応募・課金とも媒体の集計）の最新の週までの4週で数えるため、ここの数字と違うことがあります。</p></InfoTip>}
  </p>;
}

/** 課金のカードの小さな文字。直近4週の途中までしか課金の記録が無いときは、記録の最後の日を添える。 */
function billingNote(n: ReturnType<typeof bandNumbers>): string {
  if (n.billingRecentYen === null) return n.lastBillingDate && n.recentStart && n.lastBillingDate < n.recentStart ? '直近4週の課金の記録なし' : '課金データなし';
  return n.lastBillingDate && n.asOf && n.lastBillingDate < n.asOf ? `課金の記録は${shortDay(n.lastBillingDate)}まで` : '';
}

/** 帯の求人名の下の行の区切り。「・」の前では改行しない。 */
const SEP = '\u00a0・ ';

/** 詳細を縦にスクロールしても見え続ける上の帯。 */
export function DetailBand({ job, billing, onOpenFullCompare, latestEvent, onOpenLatest, listingStart = null, listingEnd = null, version = null, onOpenListing, reportHref = null, bodyState = null }: { job: JobCopyRecord; billing: readonly BillingEntry[] | undefined; onOpenFullCompare: () => void; /** いちばん新しい変更（応募は含めない）。 */ latestEvent?: { date: string; title: string } | null; onOpenLatest?: () => void; /** 掲載を始めた日（出来事の「掲載を開始」と同じ日）。分からないとき null。 */ listingStart?: string | null; /** 掲載を終えた日。掲載中なら null。 */ listingEnd?: string | null; /** 表示中の本文の短い名前（「11/13 の本文（最新）」）と、正確な日時の説明。 */ version?: { label: string; detail: ReactNode } | null; /** 「求人の内容 ▸」で求人の内容の小窓を開く。 */ onOpenListing?: () => void; /** 報告書の画面（HubSpot の求人だけ）。 */ reportHref?: string | null; /** 本文の状態（「前の本文と同じ」など）。押せない小さな文字で出す。 */ bodyState?: string | null }) {
  const n = bandNumbers(job, billing);
  const label = job.dataSource === 'hubspot' ? publicationLabel(job.listingPublicationStatus ?? null) : null;
  // 終了日は、今の公開状況が「掲載終了」のときだけ出す（掲載中なら、過去の終了の出来事があっても出さない）。
  const period = listingPeriodText(listingStart, listingEnd, todayJst(), label === '掲載終了');
  // 公開状況が分からないときは「不明」だけだと何のことか分からないので、「公開状況：」を付ける。
  const status = label !== null ? `${label === '不明' ? '公開状況：不明' : label}${period}` : period ? `掲載${period}` : '';
  const band = useRef<HTMLDivElement>(null);
  // 帯の実際の高さを --jc-band-h に持たせ、下の固定見出しの位置をそこから決める
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
  const parts = [job.media, status].filter(Boolean);
  // 会社・地域は帯の求人名の下の行に、媒体・公開状況の前に出す（見出しの行をなくしたため）。
  const places = [job.company, job.location].filter((item): item is string => Boolean(item));
  const statusLine = [...places, ...parts, version?.label].filter(Boolean).join(' ・ ');
  return <div className="jc-band" ref={band} role="region" aria-label="この求人の要点">
    {/* 1 段目：求人名と常に使う操作。2 段目：数字・最新の変更・データの日付。 */}
    <div className="jc-band-top">
    <div className="jc-band-name"><h1 title={job.title}>{job.title}</h1>
      {/* 幅が足りないときは「・」の区切りの後ろで折り返す（言葉の途中で切らず、「…」で隠さない）。 */}
      <span className="jc-band-status" title={statusLine}>
        {places.length > 0 && <><span data-testid="band-place">{places.map((place, index) => <Fragment key={index}>{index > 0 && SEP}<span className="jc-band-word">{place}</span></Fragment>)}</span>{(parts.length > 0 || version) && <span aria-hidden="true">{SEP}</span>}</>}
        <span data-testid="band-status">{parts.map((part, index) => <Fragment key={index}>{index > 0 && SEP}<span className="jc-band-word">{part}</span></Fragment>)}</span>
        {version && <>{parts.length > 0 && <span aria-hidden="true">{SEP}</span>}<InfoTip className="jc-infotip-left jc-band-version" label={<span data-testid="band-version">{version.label}</span>}>{version.detail}</InfoTip></>}</span></div>
    {/* 常に使う 3 つ。同じ大きさの枠付きで 1 列に並べる。 */}
    <div className="jc-band-actions">
      <div className="jc-band-buttons">
        <button type="button" className="jc-button" onClick={onOpenFullCompare}>全文を比べる<span aria-hidden="true"> ▸</span></button>
        {onOpenListing && <button type="button" className="jc-button" onClick={onOpenListing}>求人の内容<span aria-hidden="true"> ▸</span></button>}
        {reportHref && <a className="jc-button jc-band-report jc-no-print" href={reportHref} target="_blank" rel="noreferrer" title="報告書を作ります。新しいタブで開きます" aria-label="報告書（新しいタブで開きます）"><span aria-hidden="true">↗ </span>報告書</a>}
      </div>
      {bodyState && <small className="jc-band-body-state" data-testid="band-body-state">本文：{bodyState}</small>}
    </div>
    </div>
    <div className="jc-band-stats">
    <dl className="jc-band-numbers">
      <div><dt>応募（直近4週）</dt><dd data-testid="band-recent">{n.recent === null ? '—' : `${String(n.recent)}件`}<small>{n.recent === null ? '' : [n.recentStart && n.asOf ? `${shortDay(n.recentStart)}〜${shortDay(n.asOf)}` : '', n.previous === null ? '' : `前の4週は${String(n.previous)}件`].filter(Boolean).join('・')}</small></dd></div>
      <div><dt>応募1件の費用</dt><dd data-testid="band-per-application">{formatBandYen(n.perApplicationYen)}<small>{n.perApplicationYen === null ? n.billingRecentYen === null ? '課金データなし' : n.recent === 0 ? '応募なし' : '' : '直近4週の課金 ÷ 応募'}</small></dd></div>
      <div><dt>課金（直近4週）</dt><dd data-testid="band-billing">{formatBandYen(n.billingRecentYen)}<small>{billingNote(n)}</small></dd></div>
    </dl>
    {latestEvent && onOpenLatest && <button type="button" className="jc-band-latest jc-open" data-testid="band-latest-event" title={`最新の変更：${shortDay(latestEvent.date)} ${latestEvent.title}`} onClick={onOpenLatest}><span>最新の変更：{shortDay(latestEvent.date)} {latestEvent.title}</span><span aria-hidden="true" className="jc-band-latest-cue">見る ›</span></button>}
    <DataLine job={job} n={n} />
    </div>
  </div>;
}

/** 「市場の今」の状態。職種を指定した求人だけ数字を取りに行く。 */
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

/**
 * 「市場の今」。比べる職種（指定・変更のボタン付き）と、競合の求人数・前月比を 1 枚にまとめる。
 * 職種が未指定なら数字は出さず、指定の案内だけを出す。平均時給は市場のデータに無いので出さない。
 */
export function MarketNowBar({ job, mode, onIndeedTitleChanged, onOpenMarket }: { job: JobCopyRecord; mode: 'api' | 'demo'; onIndeedTitleChanged: (change: IndeedTitleChange) => void; /** 「市場の推移を見る」で、出来事から開くのと同じ横の窓を開く。 */ onOpenMarket?: () => void }) {
  const titleState = indeedTitleState(job);
  const { now } = useAutoMarket(job, mode);
  // 職種が未指定・指定しないときは数字が出ないので、1行に畳む（開くと職種を指定できる）。
  if (titleState !== 'set') return <details className="lo-card lo-market lo-collapsed" aria-label="市場の今" data-testid="market-collapsed">
    <summary><strong>市場の今</strong><span>{titleState === 'none' ? '職種を「指定しない」にしています' : '職種が未指定です（指定すると競合の求人数と前月比が出ます）'}</span></summary>
    <MarketOccupationSlot record={job} prefecture={prefectureFromLocation(job.location)} onChanged={onIndeedTitleChanged} />
    {titleState === 'none' && <p className="jc-muted" data-testid="rail-market-note">職種を「指定しない」にしているため、市場の数字は出しません。</p>}
    {titleState === 'unset' && <p className="jc-muted" data-testid="rail-market-note">職種を指定すると、競合の求人数と前月比が出ます。</p>}
  </details>;
  return <section className="lo-card lo-market" aria-label="市場の今">
    <div className="lo-head"><h2>市場の今</h2><span>Indeed の競合の求人と比べます。</span></div>
    <MarketOccupationSlot record={job} prefecture={prefectureFromLocation(job.location)} onChanged={onIndeedTitleChanged} />
    <div className="lo-market-now">
      {now.status === 'loading' && <p className="jc-muted" role="status">市場の数字を読み込み中です。</p>}
      {now.status === 'failed' && <p className="jc-muted" role="status">市場の数字を取得できませんでした。</p>}
      {now.status === 'no_data' && <p className="jc-muted" role="status">この職種・地域の市場の数字はまだありません。</p>}
      {now.status === 'ok' && <dl>
        <div><dt>競合の求人数（{formatMonthJa(now.value.month)}）</dt><dd data-testid="rail-market-jobs">{now.value.jobs.toLocaleString('ja-JP')}件</dd></div>
        <div><dt>前月比（{formatMonthJa(now.value.previousMonth)}と比べて）</dt><dd data-testid="rail-market-change">{now.value.changePct === null ? now.value.previousJobs === 0 ? '前月は0件のため割合は出せません' : '前月の数字がありません' : `${signedPct(now.value.changePct)}（${(now.value.previousJobs ?? 0).toLocaleString('ja-JP')}件 → ${now.value.jobs.toLocaleString('ja-JP')}件）`}</dd></div>
      </dl>}
      {onOpenMarket && <button type="button" className="jc-open" onClick={onOpenMarket}>市場の推移を見る ▸</button>}
    </div>
  </section>;
}
