import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { EChartsCoreOption } from 'echarts/core';
import { EChart } from '../../components/EChart';
import { mediaLabel, publicationLabel } from './hubspotListings';
import type { DealSelection, HubSpotListing } from './hubspotListings';
import { companyRollup, dealCutoff, dealEvents, dealTotals, eventSourceOf, eventWeekMarks, listingNumbers, markLabel, mediaCutoffs, occupationStatus, regionText, ROLLUP_RULES, shareOfDeal, shortYen, weeklySeries } from './dealSummaryModel';
import { dealGroup, loadCompanyDatas, loadDealData } from './dealSummaryLoad';
import { DealNextTodo } from './NextTodo';
import { reportUrl } from './navigation';
import { OutLink } from './Pressable';
import { DealContract } from './DealContract';
import { DealFirstGoals } from './DealFirstGoals';
import { clearCompanyDealsCache, loadCompanyDeals } from './companyDeals';
import type { CompanyDeal } from './companyDeals';
import { ChoiceSwitch, StatusFilter } from './StatusSwitch';
import { InfoTip } from './InfoTip';
import { formatDateJst } from './format';
import { setCompanyScope } from './companyScope';
import { countStatuses, matchesStatus, useStatusFilter } from './statusFilter';
import type { DealData } from './dealSummaryLoad';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { formatDay, formatYen, jstDate, todayJst } from './timelineModel';
import type { BillingEntry } from './timelineModel';
import './deal-summary.css';

export type SummarySource = { status: 'loading' } | { status: 'error'; message: string } | { status: 'ready'; datas: DealData[]; notes?: string[] };
type Source = SummarySource;
export type Billing = Readonly<Record<string, readonly BillingEntry[]>>;

const PALETTE = ['#1f6f8b', '#c2571a', '#5b8c2a', '#8a4fa0', '#b8860b', '#2e8b7a', '#a33b4e', '#4a5fa8'];
const ENDED_GRAY = '#b4b4b4';
/** 掲載を終えた求人の番号の印（白い数字が読める濃さ）。 */
const ENDED_MARK = '#6b6b6b';
/** グラフで、同じ週に複数の求人の出来事があるときの番号の色。 */
const MULTI_MARK = '#1a1a1a';
/** 求人ごとの出来事・応募を読む求人の数（案件ごとの先頭。dealSummaryLoad と同じ）。 */
const EVENT_LISTING_LIMIT = 30;
const EVENT_LIMIT = 10;
const yen = (value: number | null) => value === null ? '—' : formatYen(value);
const count = (value: number | null) => value === null ? '—' : `${String(value)}件`;
const indeedCell = (row: HubSpotListing) => row.indeed_title_state === 'set' && row.indeed_title?.trim() ? row.indeed_title.trim() : row.indeed_title_state === 'none' ? '市場と比べない' : <span className="ds-unset">未指定</span>;

export function DealSummary({ deal, billingByJob, onOpenListing, onOpenCompany, onWide, onContractSaved, onDetail, onCompanyKnown }: { onContractSaved?: (() => void) | undefined; /** 契約・成果の欄が読んだ案件の値（見出しに使う） */ onDetail?: ((detail: WorkspaceResponse) => void) | undefined; /** 案件の顧客が分かったとき（見出しとパンくずに使う） */ onCompanyKnown?: ((company: { id: string; name: string }) => void) | undefined; deal: DealSelection; billingByJob: Billing; onOpenListing: (row: HubSpotListing, opener?: HTMLElement) => void; onOpenCompany?: (company: { id: string; name: string }) => void; /** 顧客の全求人を出しているか（今の案件のとき true）を知らせる */ onWide?: (wide: boolean) => void }) {
  const [source, setSource] = useState<Source>({ status: 'loading' });
  const [revision, setRevision] = useState(0);
  const saved = useCallback(() => { clearCompanyDealsCache(); onContractSaved?.(); setRevision(value => value + 1); }, [onContractSaved]);
  const [wide, setWide] = useState(false);
  const [info, setInfo] = useState<CompanyDeal | null>(null);
  const onWideRef = useRef(onWide);
  const onCompanyRef = useRef(onCompanyKnown);
  useEffect(() => { onWideRef.current = onWide; onCompanyRef.current = onCompanyKnown; });
  useEffect(() => {
    const controller = new AbortController();
    void loadDealData(deal.id, deal.name, controller.signal).then(async data => {
      if (controller.signal.aborted) return;
      setWide(false);
      setInfo(null);
      setSource(data ? { status: 'ready', datas: [data] } : { status: 'error', message: 'この案件の求人を読み込めませんでした。時間を置いて再取得してください。' });
      // 顧客の「今の案件」なら、その顧客の全求人（過去の案件に結び付いたものを含む）を 1 か所に出す。
      const company = data?.company;
      if (!company) return;
      onCompanyRef.current?.(company);
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
  const retry = useCallback(() => { setSource({ status: 'loading' }); setRevision(value => value + 1); }, []);
  return <><p className="ds-report-entry"><OutLink href={reportUrl({ scope: 'deal', id: deal.id })} label="報告書を作る（新しいタブ）" /></p><div className="nt-contract-grid"><DealContract key={deal.id} dealId={deal.id} onSaved={saved} onQueued={saved} onDetail={onDetail} /><DealNextTodo key={`todo-${deal.id}`} dealId={deal.id} onSaved={saved} /></div><DealFirstGoals key={`goals-${deal.id}`} dealId={deal.id} /><SummaryView scope="deal" dealId={deal.id} wide={wide} dealInfo={info} source={source} billingByJob={billingByJob} onOpenListing={onOpenListing} onRetry={retry} {...(onOpenCompany ? { onOpenCompany } : {})} /></>;
}

export function SummaryView({ scope, dealId, wide = false, dealInfo = null, source, billingByJob, onOpenListing, onOpenCompany, onRetry }: { /** 読み込みに失敗したとき「もう一度読み込む」で呼ぶ */ onRetry?: () => void; scope: 'deal' | 'company'; /** 案件の画面のときの案件 ID（職種の一括指定へのリンクに使う） */ dealId?: string; /** 今の案件の画面で、顧客の全求人を出しているとき */ wide?: boolean; /** 案件の画面で、この案件の契約状態（契約中かどうか）。分からなければ null */ dealInfo?: CompanyDeal | null; source: Source; billingByJob: Billing; onOpenListing: (row: HubSpotListing, opener?: HTMLElement) => void; onOpenCompany?: (company: { id: string; name: string }) => void }) {
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
    // 応募と課金を読むのは案件ごとに先頭の30件。出来事もその求人の分だけにそろえる。
    const eventIds = new Set(source.datas.flatMap(data => data.rows.slice(0, EVENT_LISTING_LIMIT).map(row => row.id)));
    const changes: NonNullable<DealData['changes']> = Object.assign({}, ...source.datas.map(data => data.changes ?? {})) as NonNullable<DealData['changes']>;
    const totals = dealTotals(inputs, asOf, rollup.overlap);
    return {
      inputs, totals, asOf, series: weeklySeries(inputs, asOf, rollup.overlap), titleOf, cutoffs: mediaCutoffs(inputs),
      numbers: new Map(inputs.map(input => [input.id, listingNumbers(input, asOf)])),
      events: dealEvents(inputs, [...rows.values()].map(row => eventSourceOf(row, jstDate(bodyDates[row.id] ?? row.summary?.body_changed_at), changes[row.id], inputs.find(input => input.id === row.id)?.billing ?? [])), asOf, titleOf, formatYen, rollup.overlap, eventIds),
      eventsCapped: source.datas.some(data => data.rows.length > EVENT_LISTING_LIMIT),
      groupTotals: source.datas.map((data, index) => ({ data, totals: dealTotals(groups[index]?.inputs ?? [], asOf, groups[index]?.overlap ?? {}) })),
      company: source.datas.map(data => data.company).find(company => company !== null) ?? null,
      excluded: inputs.filter(input => input.billingUnavailable).length,
      multiCount: inputs.filter(input => input.inOtherDeals).length,
    };
  }, [source, billingByJob]);

  if (source.status === 'loading') return <section className="ds-root" aria-label={aria}><p role="status">{noun}のまとめを読み込んでいます…</p></section>;
  if (source.status === 'error') return <section className="ds-root" aria-label={aria}><div role="alert" className="jc-load-band"><span>{source.message}</span>{onRetry && <button type="button" className="jc-button" onClick={onRetry}>もう一度読み込む</button>}</div></section>;
  if (!model?.inputs.length) return <section className="ds-root" aria-label={aria}><p>この{noun}の求人はありません。</p></section>;

  const { inputs, totals, series, numbers, events, cutoffs, asOf } = model;
  const colorOf = (id: string) => { const index = inputs.findIndex(input => input.id === id); return inputs[index]?.ended ? ENDED_GRAY : PALETTE[index % PALETTE.length] ?? ENDED_GRAY; };
  /** 番号の印の色。掲載を終えた求人は、白い数字が読める濃い灰色。 */
  const eventColor = (id: string) => inputs.find(input => input.id === id)?.ended ? ENDED_MARK : colorOf(id);
  const colorsRepeat = inputs.length > PALETTE.length;
  const labels = series.weeks.map(week => formatDay(week).slice(5));
  const chartSeries = colored
    ? series.perListing.flatMap(row => row.counts ? [{ type: 'bar' as const, stack: 'applications', name: model.titleOf(row.id), data: row.counts, itemStyle: { color: colorOf(row.id) } }] : [])
    : [{ type: 'bar' as const, name: `${noun}の合計`, data: series.total, itemStyle: { color: '#1f6f8b' } }];
  // 出来事の番号を、その週の縦線で出す（一覧と同じ番号・同じ色）。
  const marks = eventWeekMarks(events, series.weeks);
  const markColor = (ids: readonly string[]) => ids.length === 1 && ids[0] ? eventColor(ids[0]) : MULTI_MARK;
  // 番号の線は応募の系列とは別の、値を持たない系列に付ける（どの求人の系列を隠しても消えない）。
  const markSeries = marks.length ? [{
    type: 'line' as const, name: '出来事の番号', data: series.weeks.map(() => null), silent: true, symbol: 'none', tooltip: { show: false },
    markLine: { silent: true, symbol: 'none', animation: false, data: marks.map(mark => ({ xAxis: mark.index, lineStyle: { color: markColor(mark.listingIds), type: 'dashed', width: 1 }, label: { show: true, position: 'end', formatter: markLabel(mark.numbers), fontSize: 11, fontWeight: 'bold', color: markColor(mark.listingIds) } })) },
  }] : [];
  const markedSeries = [...chartSeries, ...markSeries];
  const applicationOption: EChartsCoreOption = {
    animation: false,
    aria: { enabled: true, description: `${noun}の週ごとの応募件数${marks.length ? `。縦の点線の数字は下の「出来事」の番号` : ''}` },
    grid: { left: 36, right: 8, top: marks.length ? 30 : 16, bottom: 24 },
    tooltip: { trigger: 'axis', confine: true, axisPointer: { type: 'shadow' } },
    xAxis: { type: 'category', data: labels, axisLabel: { fontSize: 10 } },
    yAxis: { type: 'value', minInterval: 1, min: 0 },
    series: markedSeries,
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
  const rowById = new Map(allRows.map(row => [row.id, row] as const));
  const ownRows = scope === 'deal' && dealId ? source.datas.find(data => data.id === dealId)?.rows ?? allRows : allRows;
  const occupation = occupationStatus(ownRows);
  const region = regionText(ownRows);
  const listingRow = (row: HubSpotListing) => {
    const n = numbers.get(row.id);
    const input = inputs.find(item => item.id === row.id);
    const share = shareOfDeal(n?.recent ?? null, totals.recent);
    const status = publicationLabel(row.publication_status);
    return <tr key={row.id}>
      <td><button type="button" className="jc-button" onClick={event => { onOpenListing(row, event.currentTarget); }}>{row.title ?? '求人名未取得'}</button>{flat ? (dealNames.get(row.id)?.length ?? 0) > 0 && <small>案件：{dealNames.get(row.id)?.join('、')}</small> : input?.inOtherDeals && <small>ほかの案件にも結び付いています</small>}</td>
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
    {scope === 'deal' && dealInfo && <p className="ds-note ds-contract">案件の状態：<strong>{dealInfo.active ? '契約中' : '契約終了'}</strong>（{dealInfo.stage_label ?? 'ステージ未取得'}）{dealInfo.current ? ' ・ この顧客の今の案件' : ' ・ 過去の案件'} ・ 作成 {formatDateJst(dealInfo.created_at, '—')}</p>}
    <div className="ds-band" role="group" aria-label={`${noun}の合計`}>
      <div><span>直近4週の応募（{noun}の合計）</span><strong>{count(totals.recent)}</strong><small>前の4週 {count(totals.previous)}</small></div>
      <div><span>応募1件あたり</span><strong>{yen(totals.perApplication)}</strong><small>課金の合計 ÷ 応募の合計</small></div>
      <div><span>今の課金（求人の合計）/4週</span><strong>{yen(totals.billing)}</strong>{totals.unknownBilling > 0 && <small>課金が分からない求人が {String(totals.unknownBilling)}件あるため、合計は出していません</small>}{totals.excludedBilling > 0 && <small>課金データのない求人 {String(totals.excludedBilling)}件（AirWork）は含みません</small>}</div>
      <div><span>求人 {String(totals.listings)} 件{flat ? ` ・ 案件 ${String(source.datas.length)} 件` : ''}</span><strong>掲載中 {String(totals.live)} ・ 終了 {String(totals.ended)}</strong>{totals.unknownApplications > 0 && <small>応募日が未取得の求人 {String(totals.unknownApplications)}件は含みません</small>}</div>
      <div className="ds-dataline" data-testid="ds-dataline">応募の取り込み {formatDay(asOf)} 時点（直近4週はこの日まで）<span aria-hidden="true"> ／ </span>最後の応募：{cutoffs.map((cutoff, index) => <span key={cutoff.media}>{index > 0 && '、'}{mediaLabel(cutoff.media)} {cutoff.lastApplication ? formatDay(cutoff.lastApplication) : cutoff.fetched ? 'なし' : '未取得'}</span>)}<span aria-hidden="true"> ／ </span>最後の課金：{cutoffs.map((cutoff, index) => <span key={cutoff.media}>{index > 0 && '、'}{mediaLabel(cutoff.media)} {cutoff.lastBilling ? formatDay(cutoff.lastBilling) : cutoff.media === 'airwork' ? 'なし' : '未取り込み'}</span>)}
        <InfoTip className="jc-infotip-left" iconFirst label="数え方">
          <p>直近4週は{formatDay(asOf)}（全部の求人の応募データがそろっている最後の日）までの28日間、前の4週はその前の28日間です。{ROLLUP_RULES.period}。</p>
          {lastBilling !== null && lastBilling < asOf && <p>課金は {formatDay(lastBilling)} までの週の分です。それより後の日は課金に含まれません。</p>}
          <p>{scope === 'deal' ? 'この案件の求人を合計します。' : 'この事業所の全部の案件の求人を、同じ求人は1回だけ数えて合計します。'}掲載を終えた求人も、掲載していた間の応募と課金を含みます。</p>
          <p>複数の案件に結び付いた求人：{ROLLUP_RULES.multiDeal}。{model.multiCount > 0 ? `この${noun}には ${String(model.multiCount)}件あります。` : ''}</p>
          {scope === 'company' && <p>{ROLLUP_RULES.company}。</p>}
          <p>課金は Indeed と AirWork の週ごとの実績です。課金データのない求人（AirWork でまだ取り込めていないもの）は、課金の合計と応募1件あたりから外しています{model.excluded > 0 ? `（${String(model.excluded)}件）` : ''}。</p>
          <p>{noun}の画面では市場の数字を合計しません。求人ごとに職種が違うことがあるためです。</p>
        </InfoTip></div>
    </div>
    {capped && <p className="ds-note">求人が多いため、先頭の50件までで集計しています。</p>}
    {source.notes?.map(note => <p className="ds-note" key={note}>{note}</p>)}
    <div className="ds-main">
        <section aria-label="週ごとの応募">
          <header className="ds-head"><h2>週ごとの応募</h2>
            <ChoiceSwitch label="グラフの見かた" value={colored ? 'colored' : 'total'} onChange={value => { setColored(value === 'colored'); }} options={[{ key: 'colored', label: '求人ごとに色分け' }, { key: 'total', label: '合計だけ' }]} /></header>
          {series.perListing.every(row => row.counts === null) ? <p className="ds-note">応募日別の件数を取得できませんでした。</p> : <EChart option={applicationOption} testId="ds-applications" height={220} renderer="svg" />}
          {colored && <ul className="ds-legend">{inputs.map(input => <li key={input.id}><i style={{ background: colorOf(input.id) }} aria-hidden="true" />{input.title}{input.ended ? '（終了）' : ''}</li>)}</ul>}
          {billingOption ? <><p className="ds-note ds-billing-label">課金（求人の合計、日割り）</p><EChart option={billingOption} testId="ds-billing" height={84} renderer="svg" /></> : <p className="ds-note">課金の合計は出せません（課金が分からない求人があるか、課金のある求人がありません）。</p>}
          {shared > 0 && <p className="ds-note">同じ応募が複数の求人に結び付いているので、{noun}の合計では1回だけ数えています</p>}
        </section>

        <section aria-label={scope === 'deal' ? 'この案件の求人' : 'この事業所の求人'}>
          <h2>{wide ? 'この顧客の全部の求人' : scope === 'deal' ? 'この案件の求人' : 'この事業所の案件と求人'}</h2>
          {scope === 'deal' && <div className="ds-facts" role="group" aria-label="地域と比べる職種">
            <p><span>地域</span>{region ?? '求人の地域を取得できていません。'}</p>
            <p><span>比べる職種</span>{occupation ? <><strong>{String(occupation.total)}件中{String(occupation.set + occupation.none)}件が指定済み</strong>{occupation.none > 0 && <small>（うち「市場と比べない」{String(occupation.none)}件）</small>}{occupation.unset > 0 && dealId ? <a className="jc-go ds-occupation-link" href={`/app/job-copy?${new URLSearchParams({ view: 'occupations', deal: dealId }).toString()}`}><span aria-hidden="true">→ </span>未指定の {String(occupation.unset)} 件{occupation.unsetEnded > 0 ? `（うち掲載終了 ${String(occupation.unsetEnded)} 件）` : ''}を指定する</a> : <small>すべての求人で指定済みです。</small>}</> : '職種の指定状況を取得できていません。'}</p>
          </div>}
          <div className="ds-filter-line"><StatusFilter status={status} counts={statusCounts} onChange={setStatus} />
            <InfoTip className="jc-infotip-left" iconFirst label="絞り込みの範囲"><p>表の求人だけを掲載状況で絞り込みます。上の合計とグラフは、掲載を終えた求人の掲載していた間の応募・課金も含みます（絞り込みの影響を受けません）。</p>{wide && <p>今の案件（契約中で最新の案件）なので、この顧客の納品管理のどの案件に結び付いた求人も出します。過去の案件にだけ結び付いた求人も含みます。</p>}</InfoTip></div>
          <div className="jc-deal-table-scroll"><table className="jc-deal-table ds-table"><thead><tr><th>求人</th><th>媒体</th><th>掲載期間</th><th>直近4週の応募（前の4週）</th><th>{noun}の応募に占める割合</th><th>課金/4週</th><th>応募1件あたり</th>{showIndeed && <th>市場の職種</th>}</tr></thead>
            <tbody>{shown(allRows).length ? shown(allRows).map(listingRow) : emptyRow}</tbody>
          </table></div>
        </section>

        <section aria-label={`この${noun}の出来事`}>
          <h2>この{noun}の出来事</h2>
          {events.length ? <ol className="ds-events">{events.slice(0, showAllEvents ? events.length : EVENT_LIMIT).map((event, index) => <li key={`${event.date}-${event.kind}-${event.listingId}-${String(index)}`}>
            <time>{formatDay(event.date)}</time><span>{(() => { const row = rowById.get(event.listingId); const mark = <b className="ds-event-n" style={{ background: eventColor(event.listingId) }} aria-hidden="true">{String(event.n)}</b>; return row ? <button type="button" className="ds-event-open" aria-label={`${String(event.n)}. ${event.text}（この求人の画面を開く）`} onClick={click => { onOpenListing(row, click.currentTarget); }}>{mark}{event.text}</button> : <>{mark}{event.text}</>; })()}</span>
            <small>{noun}の応募：前の4週 {event.beforeIncomplete ? '（前の期間がそろっていません）' : count(event.before)} → 後の4週 {event.after === null ? '（まだ4週たっていません）' : count(event.after)}</small>
          </li>)}</ol> : <p className="ds-note">記録できる出来事はまだありません。</p>}
          {events.length > EVENT_LIMIT && <button type="button" className="jc-button" onClick={() => { setShowAllEvents(value => !value); }}>{showAllEvents ? '新しい10件だけにする' : `すべて表示（${String(events.length)}件）`}</button>}
          {model.eventsCapped && <p className="ds-note">出来事は先頭{String(EVENT_LISTING_LIMIT)}件の求人の分です。</p>}
          {events.length > 0 && <InfoTip className="jc-infotip-left" iconFirst label="番号と色の見方"><p>番号は古い順で、上の週ごとの応募のグラフの点線の数字と同じです。グラフには一覧に出ていない古い出来事の番号も出ます。「すべて表示」で古い出来事も見られます。同じ週に4件以上あるときは「3ほか2」のように縮めています。</p>
          <p>番号の色は求人の色です（グラフの色分けと同じ）。{colorsRepeat ? '9件以上は色が重なります。' : ''}掲載を終えた求人は灰色です。グラフで同じ週に複数の求人の出来事があるときは、番号を黒で出します。押すとその求人の画面を開きます。</p>
          <p>本文と画像の変更は、HubSpot に残っている求人ごとの新しい20回分までです。空白だけの違いは変更に数えません。給与だけを変えたときも「本文を変えました」と出します（求人の画面では「給与を変更」と分けています）。掲載の開始・終了は HubSpot に履歴が残っている分だけです。</p></InfoTip>}
        </section>
    </div>
  </section>;
}
