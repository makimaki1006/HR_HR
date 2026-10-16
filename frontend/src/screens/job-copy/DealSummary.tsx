import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { EChartsCoreOption } from 'echarts/core';
import { EChart } from '../../components/EChart';
import { mediaLabel, publicationLabel } from './hubspotListings';
import type { DealSelection, HubSpotListing } from './hubspotListings';
import { companyRollup, dealCutoff, dealEvents, dealTotals, listingNumbers, mediaCutoffs, ROLLUP_RULES, shareOfDeal, shortYen, weeklySeries } from './dealSummaryModel';
import { dealGroup, loadCompanyDatas, loadDealData } from './dealSummaryLoad';
import { DealNextTodo } from './NextTodo';
import { DealContract } from './DealContract';
import { clearCompanyDealsCache, loadCompanyDeals } from './companyDeals';
import type { CompanyDeal } from './companyDeals';
import { StatusFilter } from './StatusSwitch';
import { setCompanyScope } from './companyScope';
import { countStatuses, matchesStatus, useStatusFilter } from './statusFilter';
import type { DealData } from './dealSummaryLoad';
import { formatDay, formatYen, jstDate, todayJst } from './timelineModel';
import type { BillingEntry } from './timelineModel';
import './deal-summary.css';

export type SummarySource = { status: 'loading' } | { status: 'error'; message: string } | { status: 'ready'; datas: DealData[]; notes?: string[] };
type Source = SummarySource;
export type Billing = Readonly<Record<string, readonly BillingEntry[]>>;

const PALETTE = ['#1f6f8b', '#c2571a', '#5b8c2a', '#8a4fa0', '#b8860b', '#2e8b7a', '#a33b4e', '#4a5fa8'];
const ENDED_GRAY = '#b4b4b4';
const EVENT_LIMIT = 10;const yen = (value: number | null) => value === null ? '—' : formatYen(value);
const count = (value: number | null) => value === null ? '—' : `${String(value)}件`;
const indeedCell = (row: HubSpotListing): string => row.indeed_title_state === 'set' && row.indeed_title?.trim() ? row.indeed_title.trim() : row.indeed_title_state === 'none' ? '市場と比べない' : '未指定';

export function DealSummary({ deal, billingByJob, onOpenListing, onOpenCompany, onWide, onContractSaved }: { onContractSaved?: (() => void) | undefined; deal: DealSelection; billingByJob: Billing; onOpenListing: (row: HubSpotListing, opener?: HTMLElement) => void; onOpenCompany?: (company: { id: string; name: string }) => void; /** 顧客の全求人を出しているか（今の取引のとき true）を知らせる */ onWide?: (wide: boolean) => void }) {
  const [source, setSource] = useState<Source>({ status: 'loading' });
  const [revision, setRevision] = useState(0);
  const saved = useCallback(() => { clearCompanyDealsCache(); onContractSaved?.(); setRevision(value => value + 1); }, [onContractSaved]);
  const [wide, setWide] = useState(false);
  const [info, setInfo] = useState<CompanyDeal | null>(null);
  const onWideRef = useRef(onWide);
  useEffect(() => { onWideRef.current = onWide; });
  useEffect(() => {
    const controller = new AbortController();
    void loadDealData(deal.id, deal.name, controller.signal).then(async data => {
      if (controller.signal.aborted) return;
      setWide(false);
      setInfo(null);
      setSource(data ? { status: 'ready', datas: [data] } : { status: 'error', message: 'この案件の求人を読み込めませんでした。時間を置いて再取得してください。' });
      // 顧客の「今の取引」なら、その顧客の全求人（過去の取引に結び付いたものを含む）を 1 か所に出す。
      const company = data?.company;
      if (!company) return;
      const deals = await loadCompanyDeals(company.id);
      const mine = deals.ok ? deals.data.deals.find(item => item.id === deal.id) : undefined;
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- aborted can change while awaiting
      if (controller.signal.aborted || !mine) return;
      setInfo(mine);
      if (!mine.current) return;
      const all = await loadCompanyDatas(company.id, controller.signal);
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- aborted can change while awaiting
      if (controller.signal.aborted || all.status !== 'ready') return;
      setSource(all);
      setWide(true);
      onWideRef.current?.(true);
      setCompanyScope({ dealId: deal.id, dealIds: all.datas.map(item => item.id) });
    });
    return () => { controller.abort(); setCompanyScope(null); onWideRef.current?.(false); };
  }, [deal.id, deal.name, revision]);
  return <><div className="nt-contract-grid"><DealContract key={deal.id} dealId={deal.id} onSaved={saved} onQueued={saved} /><DealNextTodo key={`todo-${deal.id}`} dealId={deal.id} onSaved={saved} /></div><SummaryView scope="deal" wide={wide} dealInfo={info} source={source} billingByJob={billingByJob} onOpenListing={onOpenListing} {...(onOpenCompany ? { onOpenCompany } : {})} /></>;
}

export function SummaryView({ scope, wide = false, dealInfo = null, source, billingByJob, onOpenListing, onOpenCompany }: { scope: 'deal' | 'company'; /** 今の取引の画面で、顧客の全求人を出しているとき */ wide?: boolean; /** 取引の画面で、この取引の契約状態（契約中かどうか）。分からなければ null */ dealInfo?: CompanyDeal | null; source: Source; billingByJob: Billing; onOpenListing: (row: HubSpotListing, opener?: HTMLElement) => void; onOpenCompany?: (company: { id: string; name: string }) => void }) {
  const [colored, setColored] = useState(true);
  const [showAllEvents, setShowAllEvents] = useState(false);
  const [status, setStatus] = useStatusFilter();
  const noun = scope === 'deal' ? '案件' : '事業所';
  const flat = scope === 'company' || wide;
  const aria = `${noun}のまとめ`;

  const model = useMemo(() => {
    if (source.status !== 'ready') return null;
    const groups = source.datas.map(data => dealGroup(data, billingByJob));
    const rollup = companyRollup(groups);
    const inputs = rollup.inputs.map(input => ({ ...input, inOtherDeals: input.inOtherDeals === true || groups.filter(group => group.inputs.some(item => item.id === input.id)).length > 1 }));
    const asOf = dealCutoff(inputs) ?? todayJst();
    const titleOf = (id: string) => inputs.find(input => input.id === id)?.title ?? '求人';
    const rows = new Map(source.datas.flatMap(data => data.rows.map(row => [row.id, row] as const)));
    const bodyDates: Record<string, string | null> = Object.assign({}, ...source.datas.map(data => data.bodyDates)) as Record<string, string | null>;
    const totals = dealTotals(inputs, asOf, rollup.overlap);
    return {
      inputs, totals, asOf, series: weeklySeries(inputs, asOf, rollup.overlap), titleOf, cutoffs: mediaCutoffs(inputs),
      numbers: new Map(inputs.map(input => [input.id, listingNumbers(input, asOf)])),
      events: dealEvents(inputs, [...rows.keys()].map(id => ({ listingId: id, bodyChangedAt: jstDate(bodyDates[id] ?? rows.get(id)?.summary?.body_changed_at) })), asOf, titleOf, formatYen, rollup.overlap),
      groupTotals: source.datas.map((data, index) => ({ data, totals: dealTotals(groups[index]?.inputs ?? [], asOf, groups[index]?.overlap ?? {}) })),
      company: source.datas.map(data => data.company).find(company => company !== null) ?? null,
      excluded: inputs.filter(input => input.billingUnavailable).length,
      multiCount: inputs.filter(input => input.inOtherDeals).length,
    };
  }, [source, billingByJob]);

  if (source.status === 'loading') return <section className="ds-root" aria-label={aria}><p role="status">{noun}のまとめを読み込んでいます…</p></section>;
  if (source.status === 'error') return <section className="ds-root" aria-label={aria}><p role="alert">{source.message}</p></section>;
  if (!model?.inputs.length) return <section className="ds-root" aria-label={aria}><p>この{noun}の求人はありません。</p></section>;

  const { inputs, totals, series, numbers, events, cutoffs, asOf } = model;
  const colorOf = (id: string) => { const index = inputs.findIndex(input => input.id === id); return inputs[index]?.ended ? ENDED_GRAY : PALETTE[index % PALETTE.length] ?? ENDED_GRAY; };
  const labels = series.weeks.map(week => formatDay(week).slice(5));
  const chartSeries = colored
    ? series.perListing.flatMap(row => row.counts ? [{ type: 'bar' as const, stack: 'applications', name: model.titleOf(row.id), data: row.counts, itemStyle: { color: colorOf(row.id) } }] : [])
    : [{ type: 'bar' as const, name: `${noun}の合計`, data: series.total, itemStyle: { color: '#1f6f8b' } }];
  const applicationOption: EChartsCoreOption = {
    animation: false,
    aria: { enabled: true, description: `${noun}の週ごとの応募件数` },
    grid: { left: 36, right: 8, top: 16, bottom: 24 },
    tooltip: { trigger: 'axis', confine: true, axisPointer: { type: 'shadow' } },
    xAxis: { type: 'category', data: labels, axisLabel: { fontSize: 10 } },
    yAxis: { type: 'value', minInterval: 1, min: 0 },
    series: chartSeries,
  };
  const billingOption: EChartsCoreOption | null = series.billing ? {
    animation: false,
    aria: { enabled: true, description: '週ごとの課金の合計' },
    grid: { left: 36, right: 8, top: 18, bottom: 4 },
    tooltip: { trigger: 'axis', confine: true, valueFormatter: (value: unknown) => yen(Number(value)) },
    xAxis: { type: 'category', data: labels, show: false },
    yAxis: { type: 'value', show: false, min: 0 },
    series: [{ type: 'bar', name: '課金', data: series.billing, itemStyle: { color: '#a36816' }, label: { show: true, position: 'top', fontSize: 10, formatter: (item: { value?: unknown }) => shortYen(Number(item.value)) } }],
  } : null;
  const rowsOf = (data: DealData) => data.rows;
  const dealNames = new Map<string, string[]>();
  for (const data of source.datas) for (const row of data.rows) dealNames.set(row.id, [...(dealNames.get(row.id) ?? []), data.name]);
  // 顧客の全求人は 1 か所に、同じ求人は 1 回だけ出す。
  const allRows = flat ? [...new Map(source.datas.flatMap(rowsOf).map(row => [row.id, row] as const)).values()] : source.datas.flatMap(rowsOf);
  const statusCounts = countStatuses(allRows);
  const shown = (rows: HubSpotListing[]) => rows.filter(row => matchesStatus(row.publication_status, status));
  const emptyRow = <tr><td colSpan={8}>この掲載状況の求人はありません。上の切り替えで「すべて」を選ぶと、全部の求人を表示します。</td></tr>;
  const showIndeed = allRows.some(row => row.indeed_title_state !== undefined);
  const capped = source.datas.some(data => data.capped);
  const shared = source.datas.reduce((sum, data) => sum + data.shared, 0);
  const lastBilling = cutoffs.find(cutoff => cutoff.media === 'hrh')?.lastBilling ?? null;
  const listingRow = (row: HubSpotListing) => {
    const n = numbers.get(row.id);
    const input = inputs.find(item => item.id === row.id);
    const share = shareOfDeal(n?.recent ?? null, totals.recent);
    const status = publicationLabel(row.publication_status);
    return <tr key={row.id}>
      <td><button type="button" className="jc-button" onClick={event => { onOpenListing(row, event.currentTarget); }}>{row.title ?? '求人名未取得'}</button>{flat ? (dealNames.get(row.id)?.length ?? 0) > 0 && <small>取引：{dealNames.get(row.id)?.join('、')}</small> : input?.inOtherDeals && <small>ほかの取引にも結び付いています</small>}</td>
      <td>{mediaLabel(row.media)}</td>
      <td>{status}{row.last_csv_detected_at ? <small>最後に確認 {formatDay(jstDate(row.last_csv_detected_at) ?? '—')}</small> : <small>開始日・終了日は未取得</small>}</td>
      <td>{count(n?.recent ?? null)}（{count(n?.previous ?? null)}）</td>
      <td>{share === null ? '—' : <><span className="ds-bar" aria-hidden="true"><i style={{ width: `${String(share)}%`, background: colorOf(row.id) }} /></span> {String(share)}%</>}</td>
      <td>{input?.billingUnavailable ? <small>課金データなし</small> : n?.billing == null ? <>—<small>直近4週の課金の記録なし</small></> : yen(n.billing)}</td>
      <td>{input?.billingUnavailable ? <>—<small>課金データなし</small></> : n?.perApplication == null ? <>—<small>{n?.billing == null ? '課金の記録なし' : '応募なし'}</small></> : yen(n.perApplication)}</td>
      {showIndeed && <td>{indeedCell(row)}</td>}
    </tr>;
  };

  return <section className="ds-root" aria-label={aria}>
    {scope === 'deal' && model.company && onOpenCompany && <p className="ds-note ds-company-link">この案件の事業所：<button type="button" className="jc-button" onClick={() => { if (model.company) onOpenCompany(model.company); }}>{model.company.name}の合計を見る</button></p>}
    {scope === 'deal' && dealInfo && <p className="ds-note ds-contract">取引の状態：<strong>{dealInfo.active ? '契約中' : '契約終了'}</strong>（{dealInfo.stage_label ?? 'ステージ未取得'}）{dealInfo.current ? ' ・ この顧客の今の取引' : ' ・ 過去の取引'} ・ 作成 {dealInfo.created_at ?? '—'}。求人の「掲載中／掲載終了」は、下の求人ごとの切り替えです。</p>}
    <div className="ds-band" role="group" aria-label={`${noun}の合計`}>
      <div><span>直近4週の応募（{noun}の合計）</span><strong>{count(totals.recent)}</strong><small>前の4週 {count(totals.previous)}</small></div>
      <div><span>応募1件あたり</span><strong>{yen(totals.perApplication)}</strong><small>課金の合計 ÷ 応募の合計</small></div>
      <div><span>今の課金（求人の合計）/4週</span><strong>{yen(totals.billing)}</strong>{totals.unknownBilling > 0 && <small>課金が分からない求人が {String(totals.unknownBilling)}件あるため、合計は出していません</small>}{totals.excludedBilling > 0 && <small>課金データのない求人 {String(totals.excludedBilling)}件（AirWork）は含みません</small>}</div>
      <div><span>求人 {String(totals.listings)} 件{flat ? ` ・ 取引 ${String(source.datas.length)} 件` : ''}</span><strong>掲載中 {String(totals.live)} ・ 終了 {String(totals.ended)}</strong>{totals.unknownApplications > 0 && <small>応募日が未取得の求人 {String(totals.unknownApplications)}件は含みません</small>}</div>
    </div>
    {capped && <p className="ds-note">求人が多いため、先頭の50件までで集計しています。</p>}
    {source.notes?.map(note => <p className="ds-note" key={note}>{note}</p>)}
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
          {billingOption ? <><p className="ds-note ds-billing-label">課金（求人の合計、日割り）</p><EChart option={billingOption} testId="ds-billing" height={84} renderer="svg" /></> : <p className="ds-note">課金の合計は出せません（課金が分からない求人があるか、課金のある求人がありません）。</p>}
          {shared > 0 && <p className="ds-note">同じ応募が複数の求人に結び付いているので、{noun}の合計では1回だけ数えています</p>}
        </section>

        <section aria-label={scope === 'deal' ? 'この案件の求人' : 'この事業所の求人'}>
          <h2>{wide ? 'この顧客の全部の求人' : scope === 'deal' ? 'この案件の求人' : 'この事業所の取引と求人'}</h2>
          {wide && <p className="sf-note">今の取引（契約中で最新の取引）です。この顧客の納品管理のどの取引に結び付いた求人も、ここに出ます。過去の取引にだけ結び付いた求人も含みます。</p>}
          <StatusFilter status={status} counts={statusCounts} onChange={setStatus} />
          <p className="sf-note">表の求人を掲載状況で絞り込みます。上の合計とグラフは、掲載を終えた求人の掲載していた間の応募・課金も含みます（絞り込みの影響を受けません）。</p>
          <div className="jc-deal-table-scroll"><table className="jc-deal-table ds-table"><thead><tr><th>求人</th><th>媒体</th><th>掲載期間</th><th>直近4週の応募（前の4週）</th><th>{noun}の応募に占める割合</th><th>課金/4週</th><th>応募1件あたり</th>{showIndeed && <th>市場の職種</th>}</tr></thead>
            <tbody>{shown(allRows).length ? shown(allRows).map(listingRow) : emptyRow}</tbody>
          </table></div>
        </section>

        <section aria-label={`この${noun}の出来事`}>
          <h2>この{noun}の出来事</h2>
          {events.length ? <ol className="ds-events">{events.slice(0, showAllEvents ? events.length : EVENT_LIMIT).map((event, index) => <li key={`${event.date}-${event.kind}-${event.listingId}-${String(index)}`}>
            <time>{formatDay(event.date)}</time><span>{event.text}</span>
            <small>{noun}の応募：前の4週 {event.beforeIncomplete ? '（前の期間がそろっていません）' : count(event.before)} → 後の4週 {event.after === null ? '（まだ4週たっていません）' : count(event.after)}</small>
          </li>)}</ol> : <p className="ds-note">記録できる出来事はまだありません。</p>}
          {events.length > EVENT_LIMIT && <button type="button" className="jc-button" onClick={() => { setShowAllEvents(value => !value); }}>{showAllEvents ? '新しい10件だけにする' : `すべて表示（${String(events.length)}件）`}</button>}
          <p className="ds-note">本文を変えた日は最後の1回だけです。画像の変更と掲載の開始・終了は、各求人を開くと確認できます。</p>
        </section>
      </div>

      <aside className="ds-rail" aria-label="数字の前提">
        <h2>数字の前提</h2>
        <h3>データ</h3>
        <p>直近4週の終わり：{formatDay(asOf)}（全部の求人の応募データがそろっている最後の日）</p>
        <h3>データの最終日</h3>
        <ul>{cutoffs.map(cutoff => <li key={cutoff.media}><strong>{mediaLabel(cutoff.media)}</strong><br />応募：{cutoff.lastApplication ? formatDay(cutoff.lastApplication) : '未取得'}<br />課金：{cutoff.lastBilling ? formatDay(cutoff.lastBilling) : cutoff.media === 'airwork' ? '課金データなし' : '未取り込み'}</li>)}</ul>
        {lastBilling !== null && lastBilling < asOf && <p>課金は {formatDay(lastBilling)} までの週の分です。それより後の日は課金に含まれません。</p>}
        <h3>{noun}の合計の数え方</h3>
        <p>{scope === 'deal' ? 'この案件の求人を合計します。' : 'この事業所の全部の取引の求人を、同じ求人は1回だけ数えて合計します。'}掲載を終えた求人も、掲載していた間の応募と課金を含みます。</p>
        <p>直近4週は{formatDay(asOf)}までの28日間、前の4週はその前の28日間です。{ROLLUP_RULES.period}。</p>
        <p>複数の取引に結び付いた求人：{ROLLUP_RULES.multiDeal}。{model.multiCount > 0 ? `この${noun}には ${String(model.multiCount)}件あります。` : ''}</p>
        {scope === 'company' && <p>{ROLLUP_RULES.company}。</p>}
        <p>課金は Indeed と AirWork の週ごとの実績です。課金データのない求人（AirWork でまだ取り込めていないもの）は、課金の合計と応募1件あたりから外しています{model.excluded > 0 ? `（${String(model.excluded)}件）` : ''}。</p>
        <p>{noun}の画面では市場の数字を合計しません。求人ごとに職種が違うことがあるためです。</p>
      </aside>
    </div>
  </section>;
}
