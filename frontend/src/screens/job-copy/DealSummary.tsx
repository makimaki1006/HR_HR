import { useEffect, useMemo, useState } from 'react';
import type { EChartsCoreOption } from 'echarts/core';
import { apiGet } from '../../api/client';
import { EChart } from '../../components/EChart';
import { mediaLabel, publicationLabel } from './hubspotListings';
import type { DealSelection, HubSpotListing, HubSpotListingPage } from './hubspotListings';
import { dealEvents, dealTotals, listingNumbers, mediaCutoffs, shareOfDeal, weeklySeries } from './dealSummaryModel';
import type { DealListingInput } from './dealSummaryModel';
import { formatDay, formatYen, jstDate, todayJst } from './timelineModel';
import type { BillingEntry } from './timelineModel';
import './deal-summary.css';

interface DealApplications { listings: { listing_id: string; total?: number; by_date?: Record<string, number>; error?: string }[]; shared_applications: number }
type Source = { status: 'loading' } | { status: 'error'; message: string } | { status: 'ready'; rows: HubSpotListing[]; capped: boolean; applications: Map<string, Record<string, number> | null>; shared: number; bodyDates: Record<string, string | null> };

const PALETTE = ['#1f6f8b', '#c2571a', '#5b8c2a', '#8a4fa0', '#b8860b', '#2e8b7a', '#a33b4e', '#4a5fa8'];
const ENDED_GRAY = '#b4b4b4';
const yen = (value: number | null) => value === null ? '—' : formatYen(value);
const count = (value: number | null) => value === null ? '—' : `${String(value)}件`;
type IndeedRow = HubSpotListing & { indeed_title_state?: string | null };

export function DealSummary({ deal, billingByJob, onOpenListing }: { deal: DealSelection; billingByJob: Readonly<Record<string, readonly BillingEntry[]>>; onOpenListing: (row: HubSpotListing, opener?: HTMLElement) => void }) {
  const [source, setSource] = useState<Source>({ status: 'loading' });
  const [colored, setColored] = useState(true);
  const asOf = useMemo(() => todayJst(), []);

  useEffect(() => {
    const controller = new AbortController();
    const fail = (message: string) => { if (!controller.signal.aborted) setSource({ status: 'error', message }); };
    void (async () => {
      const page = await apiGet<HubSpotListingPage>(`/api/job-copy/listings?${new URLSearchParams({ deal: deal.id, offset: '0', sort: 'title', descending: 'false' }).toString()}`, { signal: controller.signal, timeoutMs: 120_000 });
      if (controller.signal.aborted) return;
      if (!page.ok) { fail('この案件の求人を読み込めませんでした。時間を置いて再取得してください。'); return; }
      const rows = page.data.listings;
      let applications = new Map<string, Record<string, number> | null>(rows.map(row => [row.id, null]));
      let shared = 0;
      let bodyDates: Record<string, string | null> = {};
      if (rows.length) {
        const ids = rows.slice(0, 30).map(row => encodeURIComponent(row.id)).join(',');
        const [apps, dates] = await Promise.all([
          apiGet<DealApplications>(`/api/job-copy/deal-applications?deal=${encodeURIComponent(deal.id)}&ids=${ids}`, { signal: controller.signal, timeoutMs: 120_000 }),
          apiGet<{ dates?: Record<string, string | null> }>(`/api/job-copy/listing-body-dates?ids=${rows.map(row => encodeURIComponent(row.id)).join(',')}`, { signal: controller.signal, timeoutMs: 120_000 }),
        ]);
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- aborted can change while awaiting
        if (controller.signal.aborted) return;
        if (apps.ok) {
          applications = new Map(rows.map(row => { const found = apps.data.listings.find(item => item.listing_id === row.id); return [row.id, found?.by_date && !found.error ? found.by_date : null]; }));
          shared = apps.data.shared_applications;
        }
        if (dates.ok && dates.data.dates) bodyDates = dates.data.dates;
      }
      setSource({ status: 'ready', rows, capped: page.data.next_offset !== null, applications, shared, bodyDates });
    })();
    return () => { controller.abort(); };
  }, [deal.id]);

  const model = useMemo(() => {
    if (source.status !== 'ready') return null;
    const inputs: DealListingInput[] = source.rows.map(row => ({
      id: row.id, title: row.title ?? '求人名未取得', media: row.media, ended: publicationLabel(row.publication_status) === '掲載終了',
      byDate: source.applications.get(row.id) ?? null, billing: billingByJob[`hubspot-history-${row.id}`] ?? [],
    }));
    const titleOf = (id: string) => inputs.find(input => input.id === id)?.title ?? '求人';
    const totals = dealTotals(inputs, asOf);
    return {
      inputs, totals, series: weeklySeries(inputs, asOf), titleOf, cutoffs: mediaCutoffs(inputs),
      numbers: new Map(inputs.map(input => [input.id, listingNumbers(input, asOf)])),
      events: dealEvents(inputs, source.rows.map(row => ({ listingId: row.id, bodyChangedAt: jstDate(source.bodyDates[row.id] ?? row.summary?.body_changed_at) })), asOf, titleOf, formatYen),
    };
  }, [source, billingByJob, asOf]);

  if (source.status === 'loading') return <section className="ds-root" aria-label="案件のまとめ"><p role="status">案件のまとめを読み込んでいます…</p></section>;
  if (source.status === 'error') return <section className="ds-root" aria-label="案件のまとめ"><p role="alert">{source.message}</p></section>;
  if (!model || !source.rows.length) return <section className="ds-root" aria-label="案件のまとめ"><p>この案件の求人はありません。</p></section>;

  const { inputs, totals, series, numbers, events, cutoffs } = model;
  const colorOf = (id: string) => { const index = inputs.findIndex(input => input.id === id); return inputs[index]?.ended ? ENDED_GRAY : PALETTE[index % PALETTE.length] ?? ENDED_GRAY; };
  const labels = series.weeks.map(week => formatDay(week).slice(5));
  const chartSeries = colored
    ? series.perListing.flatMap(row => row.counts ? [{ type: 'bar' as const, stack: 'applications', name: model.titleOf(row.id), data: row.counts, itemStyle: { color: colorOf(row.id) } }] : [])
    : [{ type: 'bar' as const, name: '案件の合計', data: series.total, itemStyle: { color: '#1f6f8b' } }];
  const applicationOption: EChartsCoreOption = {
    animation: false,
    aria: { enabled: true, description: '案件の週ごとの応募件数' },
    grid: { left: 36, right: 8, top: 16, bottom: 24 },
    tooltip: { trigger: 'axis', confine: true, axisPointer: { type: 'shadow' } },
    xAxis: { type: 'category', data: labels, axisLabel: { fontSize: 10 } },
    yAxis: { type: 'value', minInterval: 1, min: 0 },
    series: chartSeries,
  };
  const billingOption: EChartsCoreOption | null = series.billing ? {
    animation: false,
    aria: { enabled: true, description: '週ごとの課金の合計' },
    grid: { left: 36, right: 8, top: 4, bottom: 4 },
    tooltip: { trigger: 'axis', confine: true, valueFormatter: (value: unknown) => yen(Number(value)) },
    xAxis: { type: 'category', data: labels, show: false },
    yAxis: { type: 'value', show: false, min: 0 },
    series: [{ type: 'bar', name: '課金', data: series.billing, itemStyle: { color: '#a36816' } }],
  } : null;
  const showIndeed = source.rows.some(row => (row as IndeedRow).indeed_title_state !== undefined);

  return <section className="ds-root" aria-label="案件のまとめ">
    <div className="ds-band" role="group" aria-label="案件の合計">
      <div><span>直近4週の応募（案件の合計）</span><strong>{count(totals.recent)}</strong><small>前の4週 {count(totals.previous)}</small></div>
      <div><span>応募1件あたり</span><strong>{yen(totals.perApplication)}</strong><small>課金の合計 ÷ 応募の合計</small></div>
      <div><span>今の課金（求人の合計）/4週</span><strong>{yen(totals.billing)}</strong>{totals.unknownBilling > 0 && <small>課金が分からない掲載中の求人 {String(totals.unknownBilling)}件は含みません</small>}</div>
      <div><span>求人 {String(totals.listings)} 件</span><strong>掲載中 {String(totals.live)} ・ 終了 {String(totals.ended)}</strong>{totals.unknownApplications > 0 && <small>応募日が未取得の求人 {String(totals.unknownApplications)}件は含みません</small>}</div>
    </div>
    {source.capped && <p className="ds-note">求人が多いため、先頭の50件までで集計しています。</p>}
    <div className="ds-grid">
      <div className="ds-main">
        <section aria-label="週ごとの応募">
          <header className="ds-head"><h2>週ごとの応募</h2>
            <div className="ds-toggle" role="group" aria-label="グラフの見かた">
              <button type="button" className="jc-button" aria-pressed={colored} onClick={() => { setColored(true); }}>求人ごとに色分け</button>
              <button type="button" className="jc-button" aria-pressed={!colored} onClick={() => { setColored(false); }}>合計だけ</button>
            </div></header>
          {series.perListing.every(row => row.counts === null) ? <p className="ds-note">応募日別の件数を取得できませんでした。</p> : <EChart option={applicationOption} testId="ds-applications" height={220} renderer="svg" />}
          {colored && <ul className="ds-legend">{inputs.map(input => <li key={input.id}><i style={{ background: colorOf(input.id) }} aria-hidden="true" />{input.title}{input.ended ? '（終了）' : ''}</li>)}</ul>}
          {billingOption ? <><p className="ds-note ds-billing-label">課金（求人の合計、日割り）</p><EChart option={billingOption} testId="ds-billing" height={64} renderer="svg" /></> : <p className="ds-note">課金は取り込まれていません（課金CSVを取り込むと表示します）。</p>}
          {source.shared > 0 && <p className="ds-note">{String(source.shared)}件の応募は複数の求人に関連づいており、求人ごとの数にそれぞれ入っています。合計はそれらを重ねて数えます。</p>}
        </section>

        <section aria-label="この案件の求人">
          <h2>この案件の求人</h2>
          <div className="jc-deal-table-scroll"><table className="jc-deal-table ds-table"><thead><tr><th>求人</th><th>媒体</th><th>掲載期間</th><th>直近4週の応募（前の4週）</th><th>案件の応募に占める割合</th><th>課金/4週</th><th>応募1件あたり</th>{showIndeed && <th>市場の職種</th>}</tr></thead><tbody>
            {source.rows.map(row => {
              const n = numbers.get(row.id);
              const share = shareOfDeal(n?.recent ?? null, totals.recent);
              const status = publicationLabel(row.publication_status);
              return <tr key={row.id}>
                <td><button type="button" className="jc-button" onClick={event => { onOpenListing(row, event.currentTarget); }}>{row.title ?? '求人名未取得'}</button></td>
                <td>{mediaLabel(row.media)}</td>
                <td>{status}{row.last_csv_detected_at ? <small>最後に確認 {formatDay(jstDate(row.last_csv_detected_at) ?? '—')}</small> : <small>開始日・終了日は未取得</small>}</td>
                <td>{count(n?.recent ?? null)}（{count(n?.previous ?? null)}）</td>
                <td>{share === null ? '—' : <><span className="ds-bar" aria-hidden="true"><i style={{ width: `${String(share)}%`, background: colorOf(row.id) }} /></span> {String(share)}%</>}</td>
                <td>{yen(n?.billing ?? null)}</td>
                <td>{yen(n?.perApplication ?? null)}</td>
                {showIndeed && <td>{(row as IndeedRow).indeed_title_state ?? '—'}</td>}
              </tr>;
            })}
          </tbody></table></div>
        </section>

        <section aria-label="この案件の出来事">
          <h2>この案件の出来事</h2>
          {events.length ? <ol className="ds-events">{events.map((event, index) => <li key={`${event.date}-${event.kind}-${event.listingId}-${String(index)}`}>
            <time>{formatDay(event.date)}</time><span>{event.text}</span>
            <small>案件の応募：前の4週 {count(event.before)} → 後の4週 {event.after === null ? '（まだ4週たっていません）' : count(event.after)}</small>
          </li>)}</ol> : <p className="ds-note">記録できる出来事はまだありません。</p>}
          <p className="ds-note">本文を変えた日は最後の1回だけです。画像の変更と掲載の開始・終了は、各求人を開くと確認できます。</p>
        </section>
      </div>

      <aside className="ds-rail" aria-label="数字の前提">
        <h2>数字の前提</h2>
        <h3>データの最終日</h3>
        <ul>{cutoffs.map(cutoff => <li key={cutoff.media}><strong>{mediaLabel(cutoff.media)}</strong><br />応募：{cutoff.lastApplication ? formatDay(cutoff.lastApplication) : '未取得'}<br />課金：{cutoff.lastBilling ? formatDay(cutoff.lastBilling) : '未取り込み'}</li>)}</ul>
        <h3>案件の合計の数え方</h3>
        <p>この案件の求人を合計します。掲載を終えた求人も、掲載していた間の応募と課金を含みます。</p>
        <p>直近4週は今日までの28日間、前の4週はその前の28日間です。</p>
        <p>案件の画面では市場の数字を合計しません。求人ごとに職種が違うことがあるためです。</p>
      </aside>
    </div>
  </section>;
}
